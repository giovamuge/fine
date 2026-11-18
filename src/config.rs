//! Configuration: infrastructure and secrets only (`~/.fine/config.toml`).
//! Financial settings (budgets, goals) live in the database so the AI can change them.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const DEFAULT_API_BASE: &str = "https://api.enablebanking.com";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    pub enable_banking: Option<EnableBankingConfig>,
    pub ntfy: Option<NtfyConfig>,
    pub dashboard: Option<DashboardConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnableBankingConfig {
    /// Application id (the JWT `kid`).
    pub app_id: String,
    /// Path to the PKCS#8 private key (.pem) downloaded from Enable Banking.
    pub key_path: PathBuf,
    /// Redirect URL registered in the Enable Banking application.
    /// Empty = use the first one registered in the application (read from the API).
    #[serde(default)]
    pub redirect_url: String,
    #[serde(default = "default_api_base")]
    pub api_base: String,
}

fn default_api_base() -> String {
    DEFAULT_API_BASE.to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NtfyConfig {
    /// Full topic URL, e.g. https://ntfy.sh/my-secret-topic
    pub url: String,
    /// Optional access token for protected topics / self-hosted servers.
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardConfig {
    /// Password required when the dashboard listens on a non-loopback address.
    pub password: Option<String>,
}

/// Where fine keeps its files.
#[derive(Debug, Clone)]
pub struct Paths {
    pub dir: PathBuf,
    pub config: PathBuf,
    pub db: PathBuf,
}

impl Paths {
    /// `FINE_DATA_DIR` or `~/.fine`.
    pub fn resolve() -> Result<Self> {
        let dir = match std::env::var_os("FINE_DATA_DIR") {
            Some(d) => PathBuf::from(d),
            None => directories::BaseDirs::new()
                .ok_or_else(|| {
                    Error::NotConfigured("impossibile determinare la home directory".into())
                })?
                .home_dir()
                .join(".fine"),
        };
        Ok(Self::in_dir(dir))
    }

    pub fn in_dir(dir: PathBuf) -> Self {
        Self {
            config: dir.join("config.toml"),
            db: dir.join("fine.db"),
            dir,
        }
    }

    pub fn ensure_dir(&self) -> Result<()> {
        std::fs::create_dir_all(&self.dir).map_err(|cause| Error::File {
            context: format!("creazione di {}", self.dir.display()),
            cause,
        })?;
        restrict_permissions(&self.dir, 0o700);
        Ok(())
    }
}

impl Config {
    pub fn load(paths: &Paths) -> Result<Self> {
        if !paths.config.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(&paths.config).map_err(|cause| Error::File {
            context: format!("lettura di {}", paths.config.display()),
            cause,
        })?;
        toml::from_str(&text).map_err(|e| {
            Error::invalid(format!(
                "config non valida: {}: {e}",
                paths.config.display()
            ))
        })
    }

    pub fn save(&self, paths: &Paths) -> Result<()> {
        paths.ensure_dir()?;
        let text = toml::to_string_pretty(self).map_err(|e| Error::invalid(e.to_string()))?;
        std::fs::write(&paths.config, text)?;
        restrict_permissions(&paths.config, 0o600);
        Ok(())
    }

    pub fn enable_banking(&self) -> Result<&EnableBankingConfig> {
        self.enable_banking.as_ref().ok_or_else(|| {
            Error::NotConfigured("Enable Banking non configurato: esegui `fine setup`".into())
        })
    }

    /// The dashboard password, if one is set.
    pub fn dashboard_password(&self) -> Option<&str> {
        self.dashboard.as_ref()?.password.as_deref()
    }
}

/// Owner-only permissions for files holding secrets or financial data.
#[cfg(unix)]
pub fn restrict_permissions(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}

#[cfg(not(unix))]
pub fn restrict_permissions(_path: &Path, _mode: u32) {}
