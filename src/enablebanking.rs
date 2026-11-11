//! Minimal Enable Banking (PSD2 AIS) client. Read-only: no payment endpoints.

use anyhow::{Context, Result, bail};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;

use crate::config::EnableBankingConfig;

pub struct Client {
    http: reqwest::Client,
    base: String,
    app_id: String,
    key: EncodingKey,
}

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    aud: &'a str,
    iat: i64,
    exp: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Aspsp {
    pub name: String,
    pub country: String,
    #[serde(default)]
    pub psu_types: Vec<String>,
    pub maximum_consent_validity: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct AuthResponse {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct Session {
    pub session_id: String,
    pub accounts: Vec<AccountResource>,
    pub access: Access,
}

#[derive(Debug, Deserialize)]
pub struct Access {
    pub valid_until: String,
}

#[derive(Debug, Deserialize)]
pub struct AccountResource {
    pub uid: String,
    pub account_id: Option<AccountId>,
    pub name: Option<String>,
    pub currency: String,
}

#[derive(Debug, Deserialize)]
pub struct AccountId {
    pub iban: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Amount {
    pub amount: String,
    pub currency: String,
}

#[derive(Debug, Deserialize)]
pub struct Balance {
    pub balance_amount: Amount,
    pub balance_type: String,
}

#[derive(Debug, Deserialize)]
struct Balances {
    balances: Vec<Balance>,
}

#[derive(Debug, Deserialize)]
struct TransactionPage {
    #[serde(default)]
    transactions: Vec<Value>,
    continuation_key: Option<String>,
}

impl Client {
    pub fn new(cfg: &EnableBankingConfig) -> Result<Self> {
        let pem = std::fs::read(&cfg.key_path)
            .with_context(|| format!("lettura chiave privata {}", cfg.key_path.display()))?;
        let key =
            EncodingKey::from_rsa_pem(&pem).context("chiave privata RSA non valida (serve PEM)")?;
        Ok(Self::with_key(&cfg.api_base, &cfg.app_id, key))
    }

    pub fn with_key(base: &str, app_id: &str, key: EncodingKey) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .user_agent(concat!("fine/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("http client");
        Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            app_id: app_id.to_string(),
            key,
        }
    }

    fn jwt(&self) -> Result<String> {
        let now = chrono::Utc::now().timestamp();
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.app_id.clone());
        let claims = Claims {
            iss: "enablebanking.com",
            aud: "api.enablebanking.com",
            iat: now,
            exp: now + 3600,
        };
        Ok(jsonwebtoken::encode(&header, &claims, &self.key)?)
    }

    async fn send<T: DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> Result<T> {
        let resp = req
            .bearer_auth(self.jwt()?)
            .header("Accept", "application/json")
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            let snippet: String = body.chars().take(500).collect();
            let hint = match status.as_u16() {
                401 | 403 | 410 if snippet.to_lowercase().contains("not active") => {
                    " — l'applicazione Enable Banking non è attiva: collega i tuoi conti nel portale"
                }
                401 | 403 | 410 => " — consenso scaduto o revocato? Riesegui `fine connect`",
                429 => {
                    " — limite di richieste raggiunto (PSD2: max ~4 letture/giorno senza la tua presenza)"
                }
                _ => "",
            };
            bail!("Enable Banking {status}: {snippet}{hint}");
        }
        serde_json::from_str(&body).with_context(|| {
            format!(
                "risposta Enable Banking inattesa: {}",
                body.chars().take(300).collect::<String>()
            )
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    pub async fn application(&self) -> Result<Value> {
        self.send(self.http.get(self.url("/application"))).await
    }

    /// Redirect URL to use: the configured one, or the first registered in the application.
    pub async fn redirect_url(&self, configured: &str) -> Result<String> {
        if !configured.is_empty() {
            return Ok(configured.to_string());
        }
        let app = self.application().await?;
        app["redirect_urls"]
            .as_array()
            .and_then(|a| a.iter().find_map(|u| u.as_str()))
            .map(str::to_string)
            .context("nessun redirect URL registrato nell'applicazione Enable Banking")
    }

    pub async fn aspsps(&self, country: &str) -> Result<Vec<Aspsp>> {
        #[derive(Deserialize)]
        struct R {
            aspsps: Vec<Aspsp>,
        }
        let r: R = self
            .send(
                self.http
                    .get(self.url("/aspsps"))
                    .query(&[("country", country)]),
            )
            .await?;
        Ok(r.aspsps)
    }

    pub async fn start_auth(
        &self,
        aspsp: &str,
        country: &str,
        valid_until: &str,
        redirect_url: &str,
        state: &str,
    ) -> Result<AuthResponse> {
        let body = json!({
            "access": { "valid_until": valid_until },
            "aspsp": { "name": aspsp, "country": country },
            "state": state,
            "redirect_url": redirect_url,
            "psu_type": "personal",
        });
        self.send(self.http.post(self.url("/auth")).json(&body))
            .await
    }

    pub async fn create_session(&self, code: &str) -> Result<Session> {
        self.send(
            self.http
                .post(self.url("/sessions"))
                .json(&json!({ "code": code })),
        )
        .await
    }

    pub async fn balances(&self, account_uid: &str) -> Result<Vec<Balance>> {
        let r: Balances = self
            .send(
                self.http
                    .get(self.url(&format!("/accounts/{}/balances", enc(account_uid)))),
            )
            .await?;
        Ok(r.balances)
    }

    /// Fetch all transactions since `date_from` (YYYY-MM-DD), following pagination.
    /// Stops if the bank repeats a continuation key (a known ASPSP quirk) to avoid loops.
    pub async fn transactions(&self, account_uid: &str, date_from: &str) -> Result<Vec<Value>> {
        let mut all = Vec::new();
        let mut seen = HashSet::new();
        let mut key: Option<String> = None;
        for _ in 0..200 {
            let mut query = vec![("date_from", date_from.to_string())];
            if let Some(k) = &key {
                query.push(("continuation_key", k.clone()));
            }
            let path = format!("/accounts/{}/transactions", enc(account_uid));
            let page: TransactionPage = self
                .send(self.http.get(self.url(&path)).query(&query))
                .await?;
            all.extend(page.transactions);
            match page.continuation_key.filter(|k| !k.is_empty()) {
                Some(k) if seen.insert(k.clone()) => key = Some(k),
                _ => break,
            }
        }
        Ok(all)
    }
}

/// Percent-encode a path segment (uids are UUIDs in practice, this is defensive).
fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Pick the most useful balance: closing booked > interim available > expected > first.
pub fn preferred_balance(balances: &[Balance]) -> Option<&Balance> {
    ["CLBD", "ITAV", "XPCD", "ITBD", "CLAV"]
        .iter()
        .find_map(|t| balances.iter().find(|b| b.balance_type == *t))
        .or_else(|| balances.first())
}
