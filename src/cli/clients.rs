//! Registering `fine mcp` in the AI clients found on this machine.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};

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
}
