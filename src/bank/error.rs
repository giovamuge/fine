use std::fmt;
use std::path::PathBuf;

use reqwest::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("lettura chiave privata {}: {cause}", path.display())]
    KeyRead {
        path: PathBuf,
        cause: std::io::Error,
    },
    #[error("chiave privata RSA non valida (serve PEM): {0}")]
    KeyInvalid(jsonwebtoken::errors::Error),
    #[error("firma della richiesta: {0}")]
    Jwt(jsonwebtoken::errors::Error),
    /// The bank (through Enable Banking) answered with an error: this used a PSD2 access.
    #[error("{0}")]
    Api(ApiError),
    /// No answer at all (network, DNS, TLS): no PSD2 access was used.
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error("risposta Enable Banking inattesa: {snippet}: {cause}")]
    Decode {
        snippet: String,
        cause: serde_json::Error,
    },
    #[error("nessun redirect URL registrato nell'applicazione Enable Banking")]
    NoRedirectUrl,
}

impl Error {
    /// The request reached the bank (it answered), so it counts against the PSD2 daily limit.
    pub fn reached_bank(&self) -> bool {
        matches!(self, Self::Api(_))
    }

    /// The bank refused the requested history length (`WRONG_TRANSACTIONS_PERIOD`).
    pub fn is_wrong_period(&self) -> bool {
        matches!(self, Self::Api(e) if e.body.contains("WRONG_TRANSACTIONS_PERIOD"))
    }
}

/// An error answer from Enable Banking.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    /// The start of the response body.
    pub body: String,
}

/// The prefix "Enable Banking <status>" also marks, in the sync log, the attempts that reached
/// the bank (see `Store::syncs_reaching_bank_since`).
pub const API_ERROR_PREFIX: &str = "Enable Banking ";

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let hint = match self.status.as_u16() {
            401 | 403 | 410 if self.body.to_lowercase().contains("not active") => {
                " — l'applicazione Enable Banking non è attiva: collega i tuoi conti nel portale"
            }
            401 | 403 | 410 => " — consenso scaduto o revocato? Riesegui `fine connect`",
            429 => {
                " — limite di richieste raggiunto (PSD2: max ~4 letture/giorno senza la tua presenza)"
            }
            _ => "",
        };
        write!(f, "{API_ERROR_PREFIX}{}: {}{hint}", self.status, self.body)
    }
}
