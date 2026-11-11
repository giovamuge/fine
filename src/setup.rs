//! `fine init`: one guided command that does everything for the user.
//! - connects a bank (only the Enable Banking .pem is needed);
//! - downloads transactions;
//! - registers fine in Claude Code, Codex and Claude Desktop;
//! - schedules sync every 6 hours (4/day = PSD2 limit) and a daily push notification.

use anyhow::{Context, Result, bail};
use chrono::{Days, Utc};
use rusqlite::{Connection, params};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{Config, EnableBankingConfig, NtfyConfig, Paths};
use crate::enablebanking::Client;

/// Hours at which `fine sync` runs: every 6h covers the whole day within the PSD2 limit.
pub const SYNC_HOURS: [u32; 4] = [1, 7, 13, 19];
pub const SYNC_MINUTE: u32 = 7;
/// Daily notification, right after the morning sync.
pub const NOTIFY_TIME: (u32, u32) = (8, 12);

pub fn ask(question: &str, yes: bool) -> bool {
    if yes {
        println!("{question} [S/n] s");
        return true;
    }
    let answer = prompt(&format!("{question} [S/n] "));
    !matches!(answer.trim().to_lowercase().as_str(), "n" | "no")
}

/// Question whose default answer is "no".
pub fn ask_no(question: &str) -> bool {
    matches!(
        prompt(&format!("{question} [s/N] "))
            .trim()
            .to_lowercase()
            .as_str(),
        "s" | "si" | "sì" | "y" | "yes"
    )
}

pub fn prompt(text: &str) -> String {
    print!("{text}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    let _ = std::io::stdin().lock().read_line(&mut line);
    line.trim().to_string()
}

// ---------- Bank connection ----------

/// App id from a key file named "<app_id>.pem" (how Enable Banking names the download).
pub fn app_id_from_key(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    uuid::Uuid::parse_str(stem).ok().map(|u| u.to_string())
}

fn open_browser(url: &str) {
    let cmd = if cfg!(target_os = "macos") {
        Command::new("open").arg(url).status()
    } else if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", "start", "", url]).status()
    } else {
        Command::new("xdg-open").arg(url).status()
    };
    if cmd.is_err() {
        eprintln!("(apri il link manualmente)");
    }
}

pub async fn connect_bank(
    conn: &Connection,
    eb: &EnableBankingConfig,
    bank: &str,
    country: &str,
    days: u64,
) -> Result<()> {
    let client = Client::new(eb)?;
    let redirect = client.redirect_url(&eb.redirect_url).await?;
    let state = uuid::Uuid::new_v4().to_string();
    let valid_until =
        (Utc::now() + Days::new(days)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let auth = client
        .start_auth(bank, country, &valid_until, &redirect, &state)
        .await?;
    conn.execute(
        "INSERT INTO pending_auth (state, aspsp, country) VALUES (?1, ?2, ?3)",
        params![state, bank, country],
    )?;
    println!(
        "\nSi apre il browser: autorizza l'accesso in sola lettura.\n{}\n",
        auth.url
    );
    open_browser(&auth.url);
    let pasted = prompt(&format!(
        "Alla fine verrai portato su {redirect}... (la pagina può dare errore, è normale).\nIncolla qui l'indirizzo completo dalla barra del browser: "
    ));
    let url = reqwest::Url::parse(&pasted).context("indirizzo non valido")?;
    let get = |k: &str| {
        url.query_pairs()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.into_owned())
    };
    if let Some(err) = get("error") {
        bail!(
            "la banca ha rifiutato l'autorizzazione: {err} {}",
            get("error_description").unwrap_or_default()
        );
    }
    if get("state").as_deref() != Some(state.as_str()) {
        bail!("l'indirizzo non appartiene a questa richiesta (state diverso)");
    }
    let code = get("code").context("parametro code mancante nell'indirizzo")?;
    let session = client.create_session(&code).await?;
    conn.execute("DELETE FROM pending_auth WHERE state = ?1", [&state])?;
    for a in &session.accounts {
        let iban = a.account_id.as_ref().and_then(|i| i.iban.clone());
        conn.execute(
            "INSERT INTO accounts (uid, iban, name, currency, aspsp, session_id, valid_until)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(uid) DO UPDATE SET iban = excluded.iban, name = excluded.name, currency = excluded.currency,
               aspsp = excluded.aspsp, session_id = excluded.session_id, valid_until = excluded.valid_until",
            params![a.uid, iban, a.name, a.currency, bank, session.session_id, session.access.valid_until],
        )?;
        println!(
            "Collegato: {} {}",
            a.name.as_deref().unwrap_or(""),
            iban.as_deref().unwrap_or("")
        );
    }
    Ok(())
}

/// Interactive bank choice from a numbered list.
pub async fn choose_and_connect(conn: &Connection, eb: &EnableBankingConfig) -> Result<()> {
    let client = Client::new(eb)?;
    let country = prompt("Paese della banca [IT]: ").to_uppercase();
    let country = if country.is_empty() {
        "IT".to_string()
    } else {
        country
    };
    let banks = client.aspsps(&country).await?;
    loop {
        let q = prompt("Cerca la tua banca (es. intesa, fineco, revolut): ").to_lowercase();
        let found: Vec<_> = banks
            .iter()
            .filter(|b| b.name.to_lowercase().contains(&q))
            .take(20)
            .collect();
        if found.is_empty() {
            println!("Nessun risultato, riprova.");
            continue;
        }
        for (i, b) in found.iter().enumerate() {
            println!("  {}. {}", i + 1, b.name);
        }
        let pick = prompt("Numero (invio per cercare di nuovo): ");
        if let Some(b) = pick
            .parse::<usize>()
            .ok()
            .and_then(|n| found.get(n.wrapping_sub(1)))
        {
            return connect_bank(conn, eb, &b.name, &country, 180).await;
        }
    }
}

// ---------- AI clients ----------

fn has_cmd(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Register `fine mcp` in the AI clients found on this machine. Returns what was done.
pub fn register_clients(exe: &Path) -> Vec<String> {
    let exe_s = exe.to_string_lossy().to_string();
    let mut done = Vec::new();
    if has_cmd("claude") {
        let exists = Command::new("claude")
            .args(["mcp", "get", "fine"])
            .output()
            .is_ok_and(|o| o.status.success());
        if exists {
            done.push("Claude Code: già registrato".into());
        } else {
            let ok = Command::new("claude")
                .args(["mcp", "add", "--scope", "user", "fine", "--", &exe_s, "mcp"])
                .output()
                .is_ok_and(|o| o.status.success());
            done.push(format!(
                "Claude Code: {}",
                if ok {
                    "registrato"
                } else {
                    "errore nella registrazione"
                }
            ));
        }
    }
    if has_cmd("codex") {
        let exists = Command::new("codex")
            .args(["mcp", "get", "fine"])
            .output()
            .is_ok_and(|o| o.status.success());
        if exists {
            done.push("Codex: già registrato".into());
        } else {
            let ok = Command::new("codex")
                .args(["mcp", "add", "fine", "--", &exe_s, "mcp"])
                .output()
                .is_ok_and(|o| o.status.success());
            done.push(format!(
                "Codex: {}",
                if ok {
                    "registrato"
                } else {
                    "errore nella registrazione"
                }
            ));
        }
    }
    if let Some(path) = claude_desktop_config() {
        match add_to_desktop_config(&path, &exe_s) {
            Ok(true) => done.push("Claude Desktop: registrato (riavvia l'app)".into()),
            Ok(false) => done.push("Claude Desktop: già registrato".into()),
            Err(e) => done.push(format!("Claude Desktop: errore ({e:#})")),
        }
    }
    done
}

fn claude_desktop_config() -> Option<PathBuf> {
    let p = directories::BaseDirs::new()?
        .config_dir()
        .join("Claude")
        .join("claude_desktop_config.json");
    p.parent().is_some_and(|d| d.exists()).then_some(p)
}

/// Adds `mcpServers.fine` keeping everything else; writes a .bak copy first. Ok(false) if present.
pub fn add_to_desktop_config(path: &Path, exe: &str) -> Result<bool> {
    let mut root: serde_json::Value = if path.exists() {
        serde_json::from_slice(&std::fs::read(path)?)
            .context("config di Claude Desktop non valida")?
    } else {
        serde_json::json!({})
    };
    let servers = root
        .as_object_mut()
        .context("config di Claude Desktop non è un oggetto JSON")?
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}));
    let servers = servers
        .as_object_mut()
        .context("mcpServers non è un oggetto")?;
    if servers.contains_key("fine") {
        return Ok(false);
    }
    servers.insert(
        "fine".into(),
        serde_json::json!({ "command": exe, "args": ["mcp"] }),
    );
    if path.exists() {
        std::fs::copy(path, path.with_extension("json.bak"))?;
    }
    std::fs::write(path, serde_json::to_string_pretty(&root)?)?;
    Ok(true)
}

// ---------- Scheduling ----------

pub const LAUNCHD_SYNC: &str = "com.fine.sync";
pub const LAUNCHD_NOTIFY: &str = "com.fine.notify";

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// launchd agent running `fine <command>` at the given (hour, minute) times.
/// launchd runs a missed calendar job once when the Mac wakes up, so no part of the day is lost.
pub fn launchd_plist(
    label: &str,
    exe: &str,
    command: &str,
    times: &[(u32, u32)],
    data_dir: Option<&str>,
    log: &str,
) -> String {
    let intervals: String = times
        .iter()
        .map(|(h, m)| format!("    <dict><key>Hour</key><integer>{h}</integer><key>Minute</key><integer>{m}</integer></dict>\n"))
        .collect();
    let env = data_dir
        .map(|d| format!("  <key>EnvironmentVariables</key>\n  <dict><key>FINE_DATA_DIR</key><string>{}</string></dict>\n", xml_escape(d)))
        .unwrap_or_default();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{label}</string>
  <key>ProgramArguments</key>
  <array><string>{exe}</string><string>{command}</string></array>
  <key>StartCalendarInterval</key>
  <array>
{intervals}  </array>
{env}  <key>StandardOutPath</key><string>{log}</string>
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

/// Install the schedule. `notify`: also the daily push notification.
pub fn install_schedule(paths: &Paths, exe: &Path, notify: bool) -> Result<String> {
    let exe_s = exe.to_string_lossy().to_string();
    let data_dir = std::env::var("FINE_DATA_DIR").ok();
    let log = paths.dir.join("schedule.log").to_string_lossy().to_string();
    if cfg!(target_os = "macos") {
        let agents = directories::BaseDirs::new()
            .context("home")?
            .home_dir()
            .join("Library/LaunchAgents");
        std::fs::create_dir_all(&agents)?;
        let uid = String::from_utf8(Command::new("id").arg("-u").output()?.stdout)?
            .trim()
            .to_string();
        let mut jobs = vec![(LAUNCHD_SYNC, "sync", sync_times())];
        if notify {
            jobs.push((LAUNCHD_NOTIFY, "notify", vec![NOTIFY_TIME]));
        }
        for (label, cmd, times) in jobs {
            let plist = agents.join(format!("{label}.plist"));
            std::fs::write(
                &plist,
                launchd_plist(label, &exe_s, cmd, &times, data_dir.as_deref(), &log),
            )?;
            let domain = format!("gui/{uid}");
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
        Ok(format!(
            "launchd: sync alle {} ogni giorno{}",
            SYNC_HOURS
                .iter()
                .map(|h| format!("{h:02}:{SYNC_MINUTE:02}"))
                .collect::<Vec<_>>()
                .join(", "),
            if notify {
                format!(", notifica alle {:02}:{:02}", NOTIFY_TIME.0, NOTIFY_TIME.1)
            } else {
                String::new()
            }
        ))
    } else if cfg!(unix) {
        let env = data_dir
            .map(|d| format!("FINE_DATA_DIR={d} "))
            .unwrap_or_default();
        let hours = SYNC_HOURS
            .iter()
            .map(|h| h.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let mut lines = vec![format!(
            "{SYNC_MINUTE} {hours} * * * {env}{exe_s} sync >> {log} 2>&1 # fine"
        )];
        if notify {
            lines.push(format!(
                "{} {} * * * {env}{exe_s} notify >> {log} 2>&1 # fine",
                NOTIFY_TIME.1, NOTIFY_TIME.0
            ));
        }
        let current = Command::new("crontab")
            .arg("-l")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        let mut new: Vec<String> = current
            .lines()
            .filter(|l| !l.ends_with("# fine"))
            .map(str::to_string)
            .collect();
        new.extend(lines);
        let mut child = Command::new("crontab")
            .arg("-")
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .context("crontab stdin")?
            .write_all((new.join("\n") + "\n").as_bytes())?;
        if !child.wait()?.success() {
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
        let agents = directories::BaseDirs::new()
            .context("home")?
            .home_dir()
            .join("Library/LaunchAgents");
        let uid = String::from_utf8(Command::new("id").arg("-u").output()?.stdout)?
            .trim()
            .to_string();
        for label in [LAUNCHD_SYNC, LAUNCHD_NOTIFY] {
            let _ = Command::new("launchctl")
                .args(["bootout", &format!("gui/{uid}/{label}")])
                .output();
            let _ = std::fs::remove_file(agents.join(format!("{label}.plist")));
        }
    } else if cfg!(unix) {
        let current = Command::new("crontab")
            .arg("-l")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        let kept: Vec<&str> = current.lines().filter(|l| !l.ends_with("# fine")).collect();
        let mut child = Command::new("crontab")
            .arg("-")
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        child
            .stdin
            .take()
            .context("crontab stdin")?
            .write_all((kept.join("\n") + "\n").as_bytes())?;
        child.wait()?;
    }
    Ok(())
}

pub fn new_ntfy_topic() -> NtfyConfig {
    let id = uuid::Uuid::new_v4().simple().to_string();
    NtfyConfig {
        url: format!("https://ntfy.sh/fine-{}", &id[..20]),
        token: None,
    }
}

// ---------- The wizard ----------

pub async fn init(paths: &Paths, config: &mut Config, yes: bool) -> Result<()> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let mut conn = crate::db::open(&paths.db)?;
    println!("fine · configurazione guidata\n");

    // 1. Bank
    let has_accounts: i64 = conn.query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))?;
    if has_accounts > 0 && config.enable_banking.is_some() {
        println!("1. Banca: {has_accounts} conti già collegati.");
    }
    if config.enable_banking.is_none() {
        println!("1. Collega la banca con Enable Banking (gratis per uso personale):");
        println!(
            "   - crea un'applicazione Production su https://enablebanking.com/cp/applications"
        );
        println!("   - collega i tuoi conti nel pannello e scarica la chiave .pem");
        let key = PathBuf::from(
            prompt("   Trascina qui il file .pem e premi invio: ").trim_matches(['\'', '"', ' ']),
        );
        let key = key
            .canonicalize()
            .with_context(|| format!("file non trovato: {}", key.display()))?;
        let app_id = match app_id_from_key(&key) {
            Some(id) => id,
            None => prompt("   Application id: "),
        };
        let eb = EnableBankingConfig {
            app_id,
            key_path: key,
            redirect_url: String::new(),
            api_base: crate::config::DEFAULT_API_BASE.to_string(),
        };
        let app = Client::new(&eb)?
            .application()
            .await
            .context("verifica dell'applicazione")?;
        println!(
            "   Applicazione \"{}\" verificata.",
            app["name"].as_str().unwrap_or("?")
        );
        config.enable_banking = Some(eb);
        config.save(paths)?;
    }
    let eb = config.enable_banking()?.clone();
    let accounts: i64 = conn.query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))?;
    if accounts == 0 || (!yes && ask_no("   Vuoi collegare un'altra banca?")) {
        choose_and_connect(&conn, &eb).await?;
    }

    // 2. First sync
    println!("\n2. Scarico i movimenti...");
    let report = crate::sync::sync_all(&mut conn, &Client::new(&eb)?).await?;
    let mut sync_failures = 0;
    for a in &report.accounts {
        match (&a.skipped, &a.error) {
            (Some(s), _) => println!("   {}: saltato ({s})", a.account),
            (_, Some(e)) => {
                sync_failures += 1;
                println!(
                    "   ✗ {}: {}",
                    a.account,
                    crate::assistant::explain_sync_error(e)
                );
            }
            _ => println!("   ✓ {}: {} transazioni", a.account, a.inserted),
        }
    }
    println!(
        "   Categorizzate automaticamente: {}",
        report.auto_categorized
    );

    // 3. AI clients
    println!();
    if ask(
        "3. Registrare fine nei client AI trovati (Claude Code, Codex, Claude Desktop)?",
        yes,
    ) {
        for line in register_clients(&exe) {
            println!("   {line}");
        }
    }

    // 4. Schedule + notifications
    println!();
    let notify = ask(
        &format!(
            "4. Notifica giornaliera sul telefono alle {:02}:{:02} (app gratuita ntfy)?",
            NOTIFY_TIME.0, NOTIFY_TIME.1
        ),
        yes,
    );
    if notify && config.ntfy.is_none() {
        config.ntfy = Some(new_ntfy_topic());
        config.save(paths)?;
    }
    match install_schedule(paths, &exe, notify) {
        Ok(msg) => println!("   Pianificato: {msg}."),
        Err(e) => println!("   Pianificazione non installata: {e:#}"),
    }
    if let (true, Some(n)) = (notify, &config.ntfy) {
        println!("   Installa l'app ntfy e iscriviti a: {}", n.url);
        println!("   (il nome del topic è la tua chiave: non condividerlo)");
    }

    if sync_failures > 0 {
        println!(
            "\nConfigurazione completata, ma {sync_failures} {} NON sincronizzat{}: i dati sono incompleti.",
            if sync_failures == 1 { "conto" } else { "conti" },
            if sync_failures == 1 { "o" } else { "i" }
        );
        println!(
            "Dettagli con `fine status`; riprova con `fine sync` (o attendi la prossima sincronizzazione automatica)."
        );
        bail!("sincronizzazione non riuscita per {sync_failures} conti");
    }
    println!("\nFatto! Prova:");
    println!("   fine status        quanto puoi spendere");
    println!("   fine serve         dashboard su http://127.0.0.1:7878");
    println!("   e chiedi a Claude: \"aiutami a impostare il piano mensile\"");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("fine-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn desktop_config_is_merged_not_replaced() {
        let p = tmp().join("claude_desktop_config.json");
        std::fs::write(
            &p,
            r#"{"mcpServers":{"bankmcp":{"command":"npx"}},"other":true}"#,
        )
        .unwrap();
        assert!(add_to_desktop_config(&p, "/usr/local/bin/fine").unwrap());
        assert!(!add_to_desktop_config(&p, "/usr/local/bin/fine").unwrap());
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        assert_eq!(v["mcpServers"]["bankmcp"]["command"], "npx");
        assert_eq!(v["mcpServers"]["fine"]["args"][0], "mcp");
        assert_eq!(v["other"], true);
        assert!(p.with_extension("json.bak").exists());
    }

    #[test]
    fn schedule_covers_the_whole_day_within_psd2_limit() {
        let hours = SYNC_HOURS;
        assert_eq!(hours.len() as i64, crate::sync::MAX_SYNCS_PER_DAY);
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
            None,
            "/tmp/l",
        );
        for h in hours {
            assert!(plist.contains(&format!("<key>Hour</key><integer>{h}</integer>")));
        }
    }

    #[test]
    fn app_id_comes_from_key_filename() {
        let id = "3fa85f64-5717-4562-b3fc-2c963f66afa6";
        assert_eq!(
            app_id_from_key(Path::new(&format!("/x/{id}.pem"))).as_deref(),
            Some(id)
        );
        assert_eq!(app_id_from_key(Path::new("/x/private.pem")), None);
    }
}
