//! Push notifications via ntfy (https://ntfy.sh, free and self-hostable).

use anyhow::{Result, bail};

use crate::assistant::{Level, Status};
use crate::config::NtfyConfig;

pub async fn send(cfg: &NtfyConfig, title: &str, body: &str, status: &Status) -> Result<()> {
    let top = status.alerts.iter().map(|a| a.level).max();
    let (priority, tags) = match top {
        Some(Level::Critical) => ("high", "rotating_light"),
        Some(Level::Warning) => ("default", "warning"),
        _ => ("low", "moneybag"),
    };
    let mut req = reqwest::Client::new()
        .post(&cfg.url)
        .header("Title", title)
        .header("Priority", priority)
        .header("Tags", tags)
        .body(body.to_string());
    if let Some(t) = &cfg.token {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        bail!(
            "ntfy ha risposto {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }
    Ok(())
}
