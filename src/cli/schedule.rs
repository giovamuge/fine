//! Background schedule: sync every 6 hours, a daily notification and the dashboard,
//! with launchd on macOS and cron elsewhere.

use std::io::Write;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use fine::config::Paths;

/// Hours at which `fine sync` runs: every 6h covers the whole day within the PSD2 limit.
pub const SYNC_HOURS: [u32; 4] = [1, 7, 13, 19];
pub const SYNC_MINUTE: u32 = 7;
/// Daily notification, right after the morning sync.
pub const NOTIFY_TIME: (u32, u32) = (8, 12);

pub const LAUNCHD_SYNC: &str = "com.fine.sync";
pub const LAUNCHD_NOTIFY: &str = "com.fine.notify";
pub const LAUNCHD_DASHBOARD: &str = "com.fine.dashboard";

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// launchd agent running `fine <command>` at the given (hour, minute) times.
/// launchd runs a missed calendar job once when the Mac wakes up, so no part of the day is lost.
/// With no times it is a service instead: started at login and restarted if it exits.
pub fn launchd_plist(
    label: &str,
    exe: &str,
    command: &str,
    times: &[(u32, u32)],
    env: &[(&str, String)],
    log: &str,
) -> String {
    let when = if times.is_empty() {
        "  <key>RunAtLoad</key><true/>\n  <key>KeepAlive</key><true/>\n  <key>ThrottleInterval</key><integer>60</integer>\n".to_string()
    } else {
        let intervals: String = times
            .iter()
            .map(|(h, m)| format!("    <dict><key>Hour</key><integer>{h}</integer><key>Minute</key><integer>{m}</integer></dict>\n"))
            .collect();
        format!("  <key>StartCalendarInterval</key>\n  <array>\n{intervals}  </array>\n")
    };
    let env = if env.is_empty() {
        String::new()
    } else {
        let vars: String = env
            .iter()
            .map(|(k, v)| format!("<key>{k}</key><string>{}</string>", xml_escape(v)))
            .collect();
        format!("  <key>EnvironmentVariables</key>\n  <dict>{vars}</dict>\n")
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{label}</string>
  <key>ProgramArguments</key>
  <array><string>{exe}</string><string>{command}</string></array>
{when}{env}  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#,
        exe = xml_escape(exe),
        log = xml_escape(log),
    )
}

pub fn sync_times() -> Vec<(u32, u32)> {
    SYNC_HOURS.iter().map(|h| (*h, SYNC_MINUTE)).collect()
}

/// The labels of the launchd jobs and the crontab lines fine owns end like this.
const CRON_MARK: &str = "# fine";

fn launch_agents_dir() -> Result<std::path::PathBuf> {
    Ok(directories::BaseDirs::new()
        .context("home")?
        .home_dir()
        .join("Library/LaunchAgents"))
}

/// The launchd domain of the current user's GUI session (`gui/<uid>`).
fn gui_domain() -> Result<String> {
    let uid = String::from_utf8(Command::new("id").arg("-u").output()?.stdout)?;
    Ok(format!("gui/{}", uid.trim()))
}

/// The user's crontab without fine's lines.
fn crontab_without_fine() -> Vec<String> {
    let current = Command::new("crontab")
        .arg("-l")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    current
        .lines()
        .filter(|l| !l.ends_with(CRON_MARK))
        .map(str::to_owned)
        .collect()
}

fn write_crontab(lines: &[String]) -> Result<bool> {
    let mut child = Command::new("crontab")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("crontab stdin")?
        .write_all((lines.join("\n") + "\n").as_bytes())?;
    Ok(child.wait()?.success())
}

/// Install the schedule. `notify`: also the daily push notification;
/// `dashboard`: keep `fine serve` running in the background (started at login).
pub fn install_schedule(
    paths: &Paths,
    exe: &Path,
    notify: bool,
    dashboard: bool,
) -> Result<String> {
    let exe = exe.to_string_lossy();
    // The scheduler starts with a minimal environment: keep the user's PATH so the
    // background sync can find `claude` / `codex` for AI categorization.
    let env: Vec<(&str, String)> = ["PATH", "FINE_DATA_DIR"]
        .into_iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (k, v)))
        .collect();
    let log = paths
        .dir
        .join("schedule.log")
        .to_string_lossy()
        .into_owned();
    if cfg!(target_os = "macos") {
        let agents = launch_agents_dir()?;
        std::fs::create_dir_all(&agents)?;
        let domain = gui_domain()?;
        let mut jobs = vec![(LAUNCHD_SYNC, "sync", sync_times())];
        if notify {
            jobs.push((LAUNCHD_NOTIFY, "notify", vec![NOTIFY_TIME]));
        }
        if dashboard {
            jobs.push((LAUNCHD_DASHBOARD, "serve", vec![]));
        }
        for (label, cmd, times) in jobs {
            let plist = agents.join(format!("{label}.plist"));
            std::fs::write(&plist, launchd_plist(label, &exe, cmd, &times, &env, &log))?;
            let _ = Command::new("launchctl")
                .args(["bootout", &format!("{domain}/{label}")])
                .output();
            let out = Command::new("launchctl")
                .args(["bootstrap", &domain, &plist.to_string_lossy()])
                .output()?;
            if !out.status.success() {
                bail!(
                    "launchctl bootstrap {label}: {}",
                    String::from_utf8_lossy(&out.stderr).trim()
                );
            }
        }
        let hours = SYNC_HOURS
            .iter()
            .map(|h| format!("{h:02}:{SYNC_MINUTE:02}"))
            .collect::<Vec<_>>()
            .join(", ");
        Ok(format!(
            "launchd: sync alle {hours} ogni giorno{}{}",
            if notify {
                format!(", notifica alle {:02}:{:02}", NOTIFY_TIME.0, NOTIFY_TIME.1)
            } else {
                String::new()
            },
            if dashboard {
                ", dashboard sempre attiva su http://127.0.0.1:7878"
            } else {
                ""
            }
        ))
    } else if cfg!(unix) {
        let env: String = env.iter().map(|(k, v)| format!("{k}='{v}' ")).collect();
        let hours = SYNC_HOURS
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let mut lines = crontab_without_fine();
        lines.push(format!(
            "{SYNC_MINUTE} {hours} * * * {env}{exe} sync >> {log} 2>&1 {CRON_MARK}"
        ));
        if notify {
            lines.push(format!(
                "{} {} * * * {env}{exe} notify >> {log} 2>&1 {CRON_MARK}",
                NOTIFY_TIME.1, NOTIFY_TIME.0
            ));
        }
        if dashboard {
            lines.push(format!(
                "@reboot {env}{exe} serve >> {log} 2>&1 {CRON_MARK}"
            ));
        }
        if !write_crontab(&lines)? {
            bail!("installazione crontab non riuscita");
        }
        Ok(format!("cron: sync alle ore {hours}:{SYNC_MINUTE:02}"))
    } else {
        bail!(
            "pianificazione automatica non supportata su questo sistema: usa l'Utilità di pianificazione con `fine sync` ogni 6 ore"
        )
    }
}

pub fn remove_schedule() -> Result<()> {
    if cfg!(target_os = "macos") {
        let agents = launch_agents_dir()?;
        let domain = gui_domain()?;
        for label in [LAUNCHD_SYNC, LAUNCHD_NOTIFY, LAUNCHD_DASHBOARD] {
            let _ = Command::new("launchctl")
                .args(["bootout", &format!("{domain}/{label}")])
                .output();
            let _ = std::fs::remove_file(agents.join(format!("{label}.plist")));
        }
    } else if cfg!(unix) {
        write_crontab(&crontab_without_fine())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_covers_the_whole_day_within_psd2_limit() {
        let hours = SYNC_HOURS;
        assert_eq!(hours.len() as i64, fine::services::sync::MAX_SYNCS_PER_DAY);
        let mut gaps: Vec<u32> = hours.windows(2).map(|w| w[1] - w[0]).collect();
        gaps.push(24 - hours[hours.len() - 1] + hours[0]);
        assert!(
            gaps.iter().all(|g| *g == 6),
            "uniform 6h gaps around the clock: {gaps:?}"
        );
        let plist = launchd_plist(
            "com.fine.sync",
            "/bin/fine",
            "sync",
            &sync_times(),
            &[("PATH", "/usr/bin:/a&b".into())],
            "/tmp/l",
        );
        assert!(plist.contains("<key>PATH</key><string>/usr/bin:/a&amp;b</string>"));
        for h in hours {
            assert!(plist.contains(&format!("<key>Hour</key><integer>{h}</integer>")));
        }
        assert!(!plist.contains("KeepAlive"));
        let service = launchd_plist(
            "com.fine.dashboard",
            "/bin/fine",
            "serve",
            &[],
            &[],
            "/tmp/l",
        );
        assert!(service.contains("<key>KeepAlive</key><true/>"));
        assert!(!service.contains("StartCalendarInterval"));
    }
}
