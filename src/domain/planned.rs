//! Planned items: expected income (salary) and fixed expenses (rent, bills, annual insurance).

use std::str::FromStr;

use chrono::NaiveDate;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::Serialize;

use super::calendar::Cadence;
use super::money::Money;
use super::text;
use super::transaction::Tx;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlannedKind {
    Expense,
    Income,
}

impl PlannedKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Expense => "expense",
            Self::Income => "income",
        }
    }
}

impl FromStr for PlannedKind {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "expense" => Ok(Self::Expense),
            "income" => Ok(Self::Income),
            _ => Err(Error::invalid("kind non valido: usa expense o income")),
        }
    }
}

impl ToSql for PlannedKind {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for PlannedKind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value
            .as_str()?
            .parse()
            .map_err(|e: Error| FromSqlError::Other(e.into()))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Planned {
    pub id: i64,
    pub name: String,
    pub kind: PlannedKind,
    pub amount: Money,
    pub cadence: Cadence,
    pub next_due: NaiveDate,
    #[serde(skip)]
    pub category_id: Option<i64>,
    pub category: Option<String>,
    pub pattern: Option<String>,
}

impl Planned {
    pub fn is_annual_expense(&self) -> bool {
        self.kind == PlannedKind::Expense && self.cadence == Cadence::Year
    }

    /// A weekly or monthly expense: a fixed cost of every month.
    pub fn is_fixed_expense(&self) -> bool {
        self.kind == PlannedKind::Expense && self.cadence != Cadence::Year
    }

    /// Does this transaction look like a payment (or receipt) of this planned item?
    /// With a pattern, its text must contain it; without, it must be in the same category for
    /// an amount within ±20%.
    pub fn matches(&self, tx: &Tx) -> bool {
        let sign_ok = match self.kind {
            PlannedKind::Income => tx.amount.is_positive(),
            PlannedKind::Expense => tx.amount.is_negative(),
        };
        if !sign_ok {
            return false;
        }
        match self.pattern.as_deref().filter(|p| !p.trim().is_empty()) {
            Some(p) => tx.haystack().contains(&text::normalize(p)),
            None => {
                let (amount, expected) = (tx.amount.abs().cents(), self.amount.cents());
                self.category_id.is_some()
                    && self.category_id == tx.category_id
                    && amount * 10 >= expected * 8
                    && amount * 10 <= expected * 12
            }
        }
    }

    /// First due date on or after `today` (planned dates roll forward automatically).
    pub fn next_due_from(&self, today: NaiveDate) -> NaiveDate {
        let mut d = self.next_due;
        while d < today {
            d = self.cadence.next(d);
        }
        d
    }

    /// Expected amount falling inside [start, end].
    pub fn expected_between(&self, start: NaiveDate, end: NaiveDate) -> Money {
        let mut d = self.next_due;
        while d > start {
            d = self.cadence.prev(d);
        }
        while d < start {
            d = self.cadence.next(d);
        }
        let mut n = 0;
        while d <= end {
            n += 1;
            d = self.cadence.next(d);
        }
        self.amount * n
    }
}

/// Payment of an annual planned expense (covered by its sinking fund, not by monthly spending).
pub fn is_annual_payment(tx: &Tx, planned: &[Planned]) -> bool {
    planned
        .iter()
        .any(|p| p.is_annual_expense() && p.matches(tx))
}

/// Payment of a weekly/monthly planned expense (a fixed cost).
pub fn is_fixed_payment(tx: &Tx, planned: &[Planned]) -> bool {
    planned
        .iter()
        .any(|p| p.is_fixed_expense() && p.matches(tx))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::testing::d;

    #[test]
    fn expected_between_counts_occurrences() {
        let p = Planned {
            id: 1,
            name: "x".into(),
            kind: PlannedKind::Expense,
            amount: Money::from_cents(1000),
            cadence: Cadence::Week,
            next_due: d("2026-11-02"),
            category_id: None,
            category: None,
            pattern: Some("x".into()),
        };
        // Mondays in Nov 2026: 2, 9, 16, 23, 30
        assert_eq!(
            p.expected_between(d("2026-11-01"), d("2026-11-30")),
            Money::from_cents(5000)
        );
        assert_eq!(p.next_due_from(d("2026-11-10")), d("2026-11-16"));
    }
}
