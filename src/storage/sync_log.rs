//! The log of sync attempts, which enforces the PSD2 daily limit and drives freshness.

use chrono::{DateTime, Utc};
use rusqlite::params;

use super::Db;
use crate::bank::API_ERROR_PREFIX;
use crate::error::Result;

impl Db<'_> {
    pub fn log_sync(&self, account_uid: &str, error: Option<&str>) -> Result<()> {
        self.exec(
            "INSERT INTO sync_log (account_uid, at, ok, message) VALUES (?1, ?2, ?3, ?4)",
            params![account_uid, Utc::now().to_rfc3339(), error.is_none(), error],
        )?;
        Ok(())
    }

    /// Attempts on an account that reached the bank since `since`. A failure that never got an
    /// answer from Enable Banking (no network, DNS down right after the Mac wakes up) did not use
    /// any of the PSD2 daily quota, so it must not block the next attempts. Bank answers are
    /// logged with the `bank::API_ERROR_PREFIX` prefix.
    pub fn syncs_reaching_bank_since(
        &self,
        account_uid: &str,
        since: DateTime<Utc>,
    ) -> Result<i64> {
        self.value(
            "SELECT COUNT(*) FROM sync_log WHERE account_uid = ?1 AND at >= ?2
               AND (ok = 1 OR message LIKE ?3 || '%')",
            params![account_uid, since.to_rfc3339(), API_ERROR_PREFIX],
        )
    }

    /// Successful syncs (of any account) since `since`.
    pub fn successful_syncs_since(&self, since: DateTime<Utc>) -> Result<i64> {
        self.value(
            "SELECT COUNT(*) FROM sync_log WHERE ok = 1 AND at >= ?1",
            [since.to_rfc3339()],
        )
    }
}
