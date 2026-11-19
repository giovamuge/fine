//! Budget definitions, stored as JSON (see `domain::budget::Budget`).

use chrono::NaiveDate;
use rusqlite::params;

use super::Db;
use crate::domain::budget::Budget;
use crate::error::{Error, Result};
use crate::tr;

pub struct StoredBudget {
    pub id: i64,
    pub budget: Budget,
    /// When it was created: rollover starts from that period.
    pub created: NaiveDate,
}

impl Db<'_> {
    pub fn load_budgets(&self) -> Result<Vec<StoredBudget>> {
        let rows: Vec<(i64, String, String, String)> = self.rows(
            "SELECT id, name, definition, substr(created_at, 1, 10) FROM budgets ORDER BY name",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        rows.into_iter()
            .map(|(id, name, definition, created)| {
                let mut budget: Budget = serde_json::from_str(&definition).map_err(|e| {
                    Error::invalid(format!("budget {name:?} illeggibile nel database: {e}"))
                })?;
                budget.name = name;
                Ok(StoredBudget {
                    id,
                    budget,
                    created: NaiveDate::parse_from_str(&created, "%Y-%m-%d").unwrap_or_default(),
                })
            })
            .collect()
    }

    /// Save a (validated) budget by name; with `replaces`, rename that budget instead.
    pub fn save_budget(&self, b: &Budget, replaces: Option<&str>) -> Result<()> {
        let definition = serde_json::to_string(b)?;
        let Some(old) = replaces.map(str::trim).filter(|r| !r.is_empty()) else {
            self.exec(
                "INSERT INTO budgets (name, definition) VALUES (?1, ?2)
                 ON CONFLICT(name) DO UPDATE SET name = excluded.name, definition = excluded.definition",
                params![b.name, definition],
            )?;
            return Ok(());
        };
        let id: i64 = self
            .row("SELECT id FROM budgets WHERE name = ?1", [old], |r| {
                r.get(0)
            })?
            .ok_or_else(|| Error::not_found(format!("budget {old:?} non trovato")))?;
        let clash: Option<i64> = self.row(
            "SELECT id FROM budgets WHERE name = ?1 AND id <> ?2",
            params![b.name, id],
            |r| r.get(0),
        )?;
        if clash.is_some() {
            crate::invalid!(
                "{}",
                tr!(
                    "esiste già un budget {:?}",
                    "a budget {:?} already exists",
                    b.name
                )
            );
        }
        self.exec(
            "UPDATE budgets SET name = ?1, definition = ?2 WHERE id = ?3",
            params![b.name, definition, id],
        )?;
        Ok(())
    }

    /// Replace the definition of a stored budget (same name).
    pub fn update_budget_definition(&self, id: i64, b: &Budget) -> Result<()> {
        self.exec(
            "UPDATE budgets SET definition = ?1 WHERE id = ?2",
            params![serde_json::to_string(b)?, id],
        )?;
        Ok(())
    }

    pub fn delete_budget(&self, name: &str) -> Result<()> {
        if self.exec("DELETE FROM budgets WHERE name = ?1", [name.trim()])? == 0 {
            return Err(Error::not_found(format!("budget {name:?} non trovato")));
        }
        Ok(())
    }
}
