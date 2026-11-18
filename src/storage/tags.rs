//! Tags, their links to transactions and tag rules (pattern -> tag).

use rusqlite::params;
use serde::Serialize;

use super::Db;
use crate::domain::rules::PatternRule;
use crate::domain::{BASE_CURRENCY, Money, text};
use crate::error::{Error, Result};

/// Setting: transactions with an id above this have not been through the tag rules yet.
const TAG_RULES_APPLIED_UP_TO: &str = "tag_rules_applied_up_to";

#[derive(Debug, Serialize)]
pub struct TagView {
    pub name: String,
    /// Transactions with this tag in the period.
    pub transactions: i64,
    /// Money out in the period (EUR, positive number).
    pub spent: Money,
    /// Money in in the period (EUR).
    pub received: Money,
    /// Patterns that add this tag automatically.
    pub rules: Vec<String>,
}

impl Db<'_> {
    /// The id of a tag (already normalized), creating it if needed.
    pub fn ensure_tag(&self, name: &str) -> Result<i64> {
        self.exec("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [name])?;
        self.value("SELECT id FROM tags WHERE name = ?1", [name])
    }

    /// Link a tag by hand. Returns whether the link is new.
    pub fn tag_transaction(&self, transaction_id: i64, tag_id: i64) -> Result<bool> {
        let n = self.exec(
            "INSERT OR IGNORE INTO transaction_tags (transaction_id, tag_id, source) VALUES (?1, ?2, 'manual')",
            params![transaction_id, tag_id],
        )?;
        Ok(n == 1)
    }

    /// Remove a tag (by normalized name) from a transaction. Returns how many links went.
    pub fn untag_transaction(&self, transaction_id: i64, tag: &str) -> Result<usize> {
        self.exec(
            "DELETE FROM transaction_tags WHERE transaction_id = ?1
             AND tag_id = (SELECT id FROM tags WHERE name = ?2)",
            params![transaction_id, tag],
        )
    }

    pub fn add_tag_rule(&self, pattern: &str, tag_id: i64) -> Result<()> {
        self.exec(
            "INSERT OR IGNORE INTO tag_rules (pattern, tag_id) VALUES (?1, ?2)",
            params![pattern, tag_id],
        )?;
        Ok(())
    }

    fn tag_rules(&self, only_pattern: Option<&str>) -> Result<Vec<PatternRule>> {
        self.rows(
            "SELECT id, pattern, tag_id FROM tag_rules WHERE ?1 IS NULL OR pattern = ?1",
            [only_pattern],
            |r| {
                Ok(PatternRule {
                    id: r.get(0)?,
                    pattern: r.get(1)?,
                    target: r.get(2)?,
                })
            },
        )
    }

    /// Apply tag rules (all of them, or only `only_pattern`) to transactions with id > `after`.
    /// Returns the number of new links.
    pub fn apply_tag_rules_after(&self, after: i64, only_pattern: Option<&str>) -> Result<usize> {
        let rules = self.tag_rules(only_pattern)?;
        if rules.is_empty() {
            return Ok(0);
        }
        let mut count = 0;
        for (id, merchant, counterparty, description) in self.texts_after(after)? {
            let hay = text::haystack(&merchant, counterparty.as_deref(), description.as_deref());
            for rule in rules.iter().filter(|r| r.matches(&hay)) {
                let linked = self.exec(
                    "INSERT OR IGNORE INTO transaction_tags (transaction_id, tag_id, source) VALUES (?1, ?2, 'rule')",
                    params![id, rule.target],
                )?;
                if linked == 1 {
                    self.exec(
                        "UPDATE tag_rules SET hits = hits + 1 WHERE id = ?1",
                        [rule.id],
                    )?;
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// Apply tag rules to transactions stored since the last call (run after every sync).
    /// Only new transactions: a tag the user removed by hand is not put back.
    pub fn apply_tag_rules_to_new(&self) -> Result<usize> {
        let after: i64 = self
            .setting(TAG_RULES_APPLIED_UP_TO)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let count = self.apply_tag_rules_after(after, None)?;
        let max = self.max_transaction_id()?.unwrap_or(after);
        self.set_setting(TAG_RULES_APPLIED_UP_TO, &max.to_string())?;
        Ok(count)
    }

    /// Every tag with its totals between `from` and `to` (inclusive; None = unbounded).
    pub fn list_tags(&self, from: Option<&str>, to: Option<&str>) -> Result<Vec<TagView>> {
        self.rows(
            "SELECT g.name,
                    COUNT(t.id),
                    COALESCE(SUM(CASE WHEN t.amount_cents < 0 AND t.currency = ?3 THEN -t.amount_cents END), 0),
                    COALESCE(SUM(CASE WHEN t.amount_cents > 0 AND t.currency = ?3 THEN t.amount_cents END), 0),
                    (SELECT group_concat(r.pattern, char(31)) FROM tag_rules r WHERE r.tag_id = g.id)
             FROM tags g
             LEFT JOIN transaction_tags tt ON tt.tag_id = g.id
             LEFT JOIN transactions t ON t.id = tt.transaction_id
                  AND (?1 IS NULL OR t.booking_date >= ?1) AND (?2 IS NULL OR t.booking_date <= ?2)
             GROUP BY g.id
             ORDER BY 3 DESC, g.name",
            params![from, to, BASE_CURRENCY],
            |r| {
                Ok(TagView {
                    name: r.get(0)?,
                    transactions: r.get(1)?,
                    spent: r.get(2)?,
                    received: r.get(3)?,
                    rules: r
                        .get::<_, Option<String>>(4)?
                        .map(|s| s.split('\u{1f}').map(str::to_owned).collect())
                        .unwrap_or_default(),
                })
            },
        )
    }

    /// Delete a tag (by normalized name) everywhere, with its rules.
    pub fn delete_tag(&self, name: &str) -> Result<()> {
        if self.exec("DELETE FROM tags WHERE name = ?1", [name])? == 0 {
            return Err(Error::not_found(format!("tag {name:?} inesistente")));
        }
        Ok(())
    }
}
