//! A booked transaction as the analyses see it, and how its category was decided.

use chrono::NaiveDate;
use rusqlite::types::{ToSql, ToSqlOutput};

use super::category::CategoryKind;
use super::money::Money;
use super::text;

/// Who (or what) assigned a transaction's category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CategorizedBy {
    /// A categorization rule (seeded, learned or created by the AI).
    Rule,
    /// The user, by hand.
    Manual,
    /// The AI, directly.
    Ai,
    /// A transfer between the user's own accounts (counterparty IBAN is one of theirs).
    Transfer,
    /// The bank debit that funded a PayPal purchase.
    Paypal,
}

impl CategorizedBy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Manual => "manual",
            Self::Ai => "ai",
            Self::Transfer => "transfer",
            Self::Paypal => "paypal",
        }
    }
}

impl ToSql for CategorizedBy {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

/// A base-currency transaction loaded for analysis.
#[derive(Debug, Clone)]
pub struct Tx {
    pub id: i64,
    pub date: NaiveDate,
    pub amount: Money,
    pub category_id: Option<i64>,
    pub category: Option<String>,
    /// The category's kind; uncategorized transactions count as expense (money out) or
    /// income (money in).
    pub kind: CategoryKind,
    pub merchant: String,
    pub counterparty: Option<String>,
    pub description: Option<String>,
    pub amortized_months: Option<i64>,
    /// Lowercase account label, holder name, bank and IBAN (without spaces), for matching.
    pub account: String,
    pub tags: Vec<String>,
}

impl Tx {
    /// The kind a transaction has without a category.
    pub fn uncategorized_kind(amount: Money) -> CategoryKind {
        if amount.is_positive() {
            CategoryKind::Income
        } else {
            CategoryKind::Expense
        }
    }

    pub fn haystack(&self) -> String {
        text::haystack(
            &self.merchant,
            self.counterparty.as_deref(),
            self.description.as_deref(),
        )
    }

    pub fn in_range(&self, from: NaiveDate, to: NaiveDate) -> bool {
        self.date >= from && self.date <= to
    }

    /// Whether the analyses of [from, to] need this transaction: booked in the period, or
    /// amortized from up to `MAX_AMORTIZATION_MONTHS` before (its slices may land there).
    pub fn relevant_for(&self, from: NaiveDate, to: NaiveDate) -> bool {
        self.date <= to
            && (self.date >= from
                || (self.amortized_months.is_some() && self.date >= amortization_horizon(from)))
    }
}

/// Longest amortization, in months.
pub const MAX_AMORTIZATION_MONTHS: u32 = 120;

/// The earliest booking date of an amortized purchase that can still spread into `from`.
pub fn amortization_horizon(from: NaiveDate) -> NaiveDate {
    from - chrono::Months::new(MAX_AMORTIZATION_MONTHS)
}

/// The transactions the analyses of [from, to] need, out of a larger loaded set.
pub fn window(txs: &[Tx], from: NaiveDate, to: NaiveDate) -> Vec<Tx> {
    txs.iter()
        .filter(|t| t.relevant_for(from, to))
        .cloned()
        .collect()
}
