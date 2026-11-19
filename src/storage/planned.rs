//! Planned income and expenses.

use chrono::NaiveDate;
use rusqlite::params;

use super::Db;
use crate::domain::Money;
use crate::domain::calendar::Cadence;
use crate::domain::planned::{Planned, PlannedKind};
use crate::error::{Error, Result};

/// A validated planned item to store.
#[derive(Debug)]
pub struct NewPlanned<'a> {
    pub name: &'a str,
    pub kind: PlannedKind,
    pub amount: Money,
    pub cadence: Cadence,
    pub next_due: NaiveDate,
    pub category_id: Option<i64>,
    pub pattern: Option<&'a str>,
}

impl Db<'_> {
    pub fn load_planned(&self) -> Result<Vec<Planned>> {
        self.rows(
            "SELECT p.id, p.name, p.kind, p.amount_cents, p.cadence, p.next_due, p.category_id, c.name, p.pattern
             FROM planned p LEFT JOIN categories c ON c.id = p.category_id ORDER BY p.name",
            [],
            |r| {
                Ok(Planned {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    kind: r.get(2)?,
                    amount: r.get(3)?,
                    cadence: r.get(4)?,
                    next_due: r.get(5)?,
                    category_id: r.get(6)?,
                    category: r.get(7)?,
                    pattern: r.get(8)?,
                })
            },
        )
    }

    /// Create or replace a planned item by name.
    pub fn upsert_planned(&self, p: &NewPlanned<'_>) -> Result<()> {
        self.exec(
            "INSERT INTO planned (name, kind, amount_cents, cadence, next_due, category_id, pattern)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(name) DO UPDATE SET kind = excluded.kind, amount_cents = excluded.amount_cents,
               cadence = excluded.cadence, next_due = excluded.next_due, category_id = excluded.category_id,
               pattern = excluded.pattern",
            params![p.name, p.kind, p.amount, p.cadence, p.next_due, p.category_id, p.pattern],
        )?;
        Ok(())
    }

    pub fn delete_planned(&self, name: &str) -> Result<()> {
        if self.exec("DELETE FROM planned WHERE name = ?1", [name.trim()])? == 0 {
            return Err(Error::not_found(format!(
                "spesa prevista inesistente: {name:?}"
            )));
        }
        Ok(())
    }
}
