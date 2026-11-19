//! Push notifications via ntfy (https://ntfy.sh, free and self-hostable).

use crate::config::NtfyConfig;
use crate::domain::assistant::{Level, Status};
use crate::error::{Error, Result};

/// Send `body` as a push notification, with a priority matching the worst alert in `status`.
pub async fn send(cfg: &NtfyConfig, title: &str, body: &str, status: &Status) -> Result<()> {
    let (priority, tags) = match status.alerts.iter().map(|a| a.level).max() {
        Some(Level::Critical) => ("high", "rotating_light"),
        Some(Level::Warning) => ("default", "warning"),
        _ => ("low", "moneybag"),
    };
    let mut req = reqwest::Client::new()
        .post(&cfg.url)
        .header("Title", title)
        .header("Priority", priority)
        .header("Tags", tags)
        .body(body.to_owned());
    if let Some(t) = &cfg.token {
        req = req.bearer_auth(t);
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        return Err(Error::Invalid(format!(
            "ntfy ha risposto {}: {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        )));
    }
    Ok(())
}
