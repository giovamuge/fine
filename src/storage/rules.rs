//! Categorization rules (pattern -> category), and applying them.

use rusqlite::params;
use serde::Serialize;

use super::Db;
use crate::domain::rules::{PatternRule, best_match};
use crate::domain::text;
use crate::domain::transaction::CategorizedBy;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct RuleView {
    pub id: i64,
    pub pattern: String,
    pub category: String,
    pub source: String,
    pub hits: i64,
}

impl Db<'_> {
    pub fn list_rules(&self) -> Result<Vec<RuleView>> {
        self.rows(
            "SELECT r.id, r.pattern, c.name, r.source, r.hits
             FROM rules r JOIN categories c ON c.id = r.category_id ORDER BY c.name, r.pattern",
            [],
            |r| {
                Ok(RuleView {
                    id: r.get(0)?,
                    pattern: r.get(1)?,
                    category: r.get(2)?,
                    source: r.get(3)?,
                    hits: r.get(4)?,
                })
            },
        )
    }

    fn category_rules(&self) -> Result<Vec<PatternRule>> {
        self.rows("SELECT id, pattern, category_id FROM rules", [], |r| {
            Ok(PatternRule {
                id: r.get(0)?,
                pattern: r.get(1)?,
                target: r.get(2)?,
            })
        })
    }

    /// Categorize every uncategorized transaction a rule matches (the longest pattern wins).
    /// Returns how many were categorized. Run it inside a transaction: it writes one row each.
    pub fn apply_category_rules(&self) -> Result<usize> {
        let rules = self.category_rules()?;
        if rules.is_empty() {
            return Ok(0);
        }
        let mut count = 0;
        for (id, merchant, counterparty, description) in self.uncategorized_texts()? {
            let hay = text::haystack(&merchant, counterparty.as_deref(), description.as_deref());
            if let Some(rule) = best_match(&rules, &hay) {
                self.set_category(id, rule.target, CategorizedBy::Rule)?;
                self.exec("UPDATE rules SET hits = hits + 1 WHERE id = ?1", [rule.id])?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// A rule taught by the user: replaces any rule with the same pattern.
    pub fn learn_rule(&self, pattern: &str, category_id: i64) -> Result<()> {
        self.exec(
            "INSERT INTO rules (pattern, category_id, source) VALUES (?1, ?2, 'learned')
             ON CONFLICT(pattern) DO UPDATE SET category_id = excluded.category_id, source = 'learned'",
            params![pattern, category_id],
        )?;
        Ok(())
    }

    /// A rule proposed by the AI: never overrides an existing one. Returns whether it was added.
    pub fn add_ai_rule(&self, pattern: &str, category_id: i64) -> Result<bool> {
        let added = self.exec(
            "INSERT OR IGNORE INTO rules (pattern, category_id, source) VALUES (?1, ?2, 'ai')",
            params![pattern, category_id],
        )?;
        Ok(added == 1)
    }

    pub fn delete_rule(&self, id: i64) -> Result<()> {
        if self.exec("DELETE FROM rules WHERE id = ?1", [id])? == 0 {
            return Err(Error::not_found(format!("regola {id} inesistente")));
        }
        Ok(())
    }
}
