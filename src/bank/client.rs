//! Minimal Enable Banking (PSD2 AIS) client. Read-only: no payment endpoints.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use serde_json::value::RawValue;

use super::error::{ApiError, Error};
use super::model::{Application, Aspsp, AuthResponse, Balance, Balances, Session, TransactionPage};
use crate::config::EnableBankingConfig;

type Result<T> = std::result::Result<T, Error>;

/// A JWT is valid for an hour; a new one is signed when less than this is left.
const TOKEN_TTL_SECS: i64 = 3600;
const TOKEN_MARGIN_SECS: i64 = 60;
/// Defensive bound on pagination (a bank that never stops paging).
const MAX_PAGES: usize = 200;

#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    aud: &'a str,
    iat: i64,
    exp: i64,
}

/// Signs the application's JWTs and reuses each one while it is valid.
struct Signer {
    app_id: String,
    key: EncodingKey,
    cached: Mutex<Option<(String, i64)>>,
}

impl Signer {
    fn token(&self) -> Result<String> {
        let now = chrono::Utc::now().timestamp();
        let mut cached = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((token, exp)) = cached.as_ref()
            && *exp - now > TOKEN_MARGIN_SECS
        {
            return Ok(token.clone());
        }
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(self.app_id.clone());
        let exp = now + TOKEN_TTL_SECS;
        let claims = Claims {
            iss: "enablebanking.com",
            aud: "api.enablebanking.com",
            iat: now,
            exp,
        };
        let token = jsonwebtoken::encode(&header, &claims, &self.key).map_err(Error::Jwt)?;
        *cached = Some((token.clone(), exp));
        Ok(token)
    }
}

pub struct Client {
    http: reqwest::Client,
    base: String,
    signer: Signer,
}

impl Client {
    pub fn new(cfg: &EnableBankingConfig) -> Result<Self> {
        let pem = std::fs::read(&cfg.key_path).map_err(|cause| Error::KeyRead {
            path: cfg.key_path.clone(),
            cause,
        })?;
        let key = EncodingKey::from_rsa_pem(&pem).map_err(Error::KeyInvalid)?;
        Ok(Self::with_key(&cfg.api_base, &cfg.app_id, key))
    }

    pub fn with_key(base: &str, app_id: &str, key: EncodingKey) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .user_agent(concat!("fine/", env!("FINE_VERSION")))
            .build()
            .expect("static HTTP client configuration");
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
            signer: Signer {
                app_id: app_id.to_owned(),
                key,
                cached: Mutex::new(None),
            },
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    async fn send<T: DeserializeOwned>(&self, req: reqwest::RequestBuilder) -> Result<T> {
        let resp = req
            .bearer_auth(self.signer.token()?)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            return Err(Error::Api(ApiError {
                status,
                body: body.chars().take(500).collect(),
            }));
        }
        serde_json::from_str(&body).map_err(|cause| Error::Decode {
            snippet: body.chars().take(300).collect(),
            cause,
        })
    }

    pub async fn application(&self) -> Result<Application> {
        self.send(self.http.get(self.url("/application"))).await
    }

    /// Redirect URL to use: the configured one, or the first registered in the application.
    pub async fn redirect_url(&self, configured: &str) -> Result<String> {
        if !configured.is_empty() {
            return Ok(configured.to_owned());
        }
        self.application()
            .await?
            .redirect_urls
            .into_iter()
            .next()
            .ok_or(Error::NoRedirectUrl)
    }

    pub async fn aspsps(&self, country: &str) -> Result<Vec<Aspsp>> {
        #[derive(Deserialize)]
        struct Aspsps {
            aspsps: Vec<Aspsp>,
        }
        let r: Aspsps = self
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

    /// Revoke a session (consent) at the bank. Best effort: the session may already be expired.
    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        let resp = self
            .http
            .delete(self.url(&format!("/sessions/{}", encode_segment(session_id))))
            .bearer_auth(self.signer.token()?)
            .send()
            .await?;
        if !resp.status().is_success() {
            return Err(Error::Api(ApiError {
                status: resp.status(),
                body: String::new(),
            }));
        }
        Ok(())
    }

    pub async fn balances(&self, account_uid: &str) -> Result<Vec<Balance>> {
        let path = format!("/accounts/{}/balances", encode_segment(account_uid));
        let r: Balances = self.send(self.http.get(self.url(&path))).await?;
        Ok(r.balances)
    }

    /// All transactions since `date_from` (YYYY-MM-DD) as raw JSON, following pagination.
    /// Stops if the bank repeats a continuation key (a known ASPSP quirk) to avoid loops.
    pub async fn transactions(
        &self,
        account_uid: &str,
        date_from: &str,
    ) -> Result<Vec<Box<RawValue>>> {
        let path = format!("/accounts/{}/transactions", encode_segment(account_uid));
        let mut all = Vec::new();
        let mut seen = HashSet::new();
        let mut key: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let mut query = vec![("date_from", date_from)];
            if let Some(k) = &key {
                query.push(("continuation_key", k.as_str()));
            }
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
fn encode_segment(s: &str) -> String {
    use std::fmt::Write;
    s.bytes()
        .fold(String::with_capacity(s.len()), |mut out, b| {
            if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
                out.push(char::from(b));
            } else {
                let _ = write!(out, "%{b:02X}");
            }
            out
        })
}
