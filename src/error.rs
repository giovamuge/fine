//! The error type of the library. User-facing failures (bad input, missing records) carry a
//! message ready to show; infrastructure failures keep their source for diagnostics.

use crate::bank;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The request is not valid (bad input or a business rule): the message says why.
    #[error("{0}")]
    Invalid(String),
    /// A named record does not exist.
    #[error("{0}")]
    NotFound(String),
    /// Something is not set up yet (e.g. Enable Banking): the message says what to run.
    #[error("{0}")]
    NotConfigured(String),
    #[error(transparent)]
    Bank(#[from] bank::Error),
    /// The AI client (Claude Code / Codex) failed or answered badly.
    #[error("{0}")]
    Ai(String),
    /// The exchange-rate service failed or answered badly.
    #[error("{0}")]
    Rates(String),
    /// An HTTP call outside the bank API (e.g. push notifications).
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Database(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A file could not be read or written (`context` says which and why).
    #[error("{context}: {cause}")]
    File {
        context: String,
        cause: std::io::Error,
    },
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Error {
    pub fn invalid(msg: impl Into<String>) -> Self {
        Self::Invalid(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// The message followed by its causes: "a: b: c".
    pub fn chain(&self) -> String {
        chain(self)
    }
}

/// An error and its causes on one line: "a: b: c".
pub fn chain(e: &dyn std::error::Error) -> String {
    let mut out = e.to_string();
    let mut cause = e.source();
    while let Some(c) = cause {
        out.push_str(": ");
        out.push_str(&c.to_string());
        cause = c.source();
    }
    out
}

/// `return Err(Error::Invalid(format!(...)))` in one line, like `anyhow::bail!`.
#[macro_export]
macro_rules! invalid {
    ($($arg:tt)*) => {
        return Err($crate::error::Error::Invalid(format!($($arg)*)))
    };
}
