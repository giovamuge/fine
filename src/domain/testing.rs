//! Builders for unit tests of the pure domain.

use chrono::NaiveDate;

use super::category::CategoryKind;
use super::money::Money;
use super::transaction::Tx;

pub fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

/// An expense/income in category `category` (an expense category, if any).
pub fn tx(id: i64, date: &str, cents: i64, merchant: &str, category: Option<&str>) -> Tx {
    let amount = Money::from_cents(cents);
    Tx {
        id,
        date: d(date),
        amount,
        category_id: category.map(|c| c.len() as i64),
        category: category.map(str::to_owned),
        kind: if category.is_some() {
            CategoryKind::Expense
        } else {
            Tx::uncategorized_kind(amount)
        },
        merchant: merchant.to_owned(),
        counterparty: Some(merchant.to_owned()),
        description: None,
        amortized_months: None,
        account: String::new(),
        tags: vec![],
    }
}
