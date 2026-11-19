//! Key-value settings that the AI and the dashboard can change.

use rusqlite::params;

use super::Db;
use crate::domain::Money;
use crate::error::Result;

const SAVINGS_GOAL: &str = "savings_goal_cents";

impl Db<'_> {
    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        self.row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.exec(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Monthly savings goal (zero if not set).
    pub fn savings_goal(&self) -> Result<Money> {
        Ok(self
            .setting(SAVINGS_GOAL)?
            .and_then(|v| v.parse().ok())
            .map(Money::from_cents)
            .unwrap_or_default())
    }

    pub fn set_savings_goal(&self, goal: Money) -> Result<()> {
        self.set_setting(SAVINGS_GOAL, &goal.cents().to_string())
    }
}
