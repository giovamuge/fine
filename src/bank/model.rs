//! Enable Banking resources. Bank data is messy: optional fields are parsed leniently, so a
//! field with an unexpected shape is ignored instead of losing the whole transaction.

use chrono::NaiveDate;
use serde::de::{DeserializeOwned, Deserializer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

use crate::domain::Money;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Aspsp {
    pub name: String,
    pub country: String,
    #[serde(default)]
    pub psu_types: Vec<String>,
    pub maximum_consent_validity: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct Application {
    pub name: Option<String>,
    pub environment: Option<String>,
    pub active: Option<bool>,
    #[serde(default)]
    pub redirect_urls: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct AuthResponse {
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct Session {
    pub session_id: String,
    pub accounts: Vec<AccountResource>,
    pub access: Access,
}

#[derive(Debug, Deserialize)]
pub struct Access {
    pub valid_until: String,
}

#[derive(Debug, Deserialize)]
pub struct AccountResource {
    pub uid: String,
    /// Stable across sessions: identifies the same account after a consent renewal.
    pub identification_hash: Option<String>,
    pub account_id: Option<AccountId>,
    pub name: Option<String>,
    pub currency: String,
}

impl AccountResource {
    pub fn iban(&self) -> Option<&str> {
        self.account_id.as_ref()?.iban.as_deref()
    }
}

#[derive(Debug, Deserialize)]
pub struct AccountId {
    pub iban: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Amount {
    pub amount: String,
    pub currency: String,
}

#[derive(Debug, Deserialize)]
pub struct Balance {
    pub balance_amount: Amount,
    pub balance_type: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct Balances {
    pub balances: Vec<Balance>,
}

/// One page of transactions. Each is kept raw too: it is stored as it came from the bank.
#[derive(Debug, Deserialize)]
pub(super) struct TransactionPage {
    #[serde(default)]
    pub transactions: Vec<Box<RawValue>>,
    pub continuation_key: Option<String>,
}

/// Pick the most useful balance: closing booked > interim available > expected > first.
pub fn preferred_balance(balances: &[Balance]) -> Option<&Balance> {
    ["CLBD", "ITAV", "XPCD", "ITBD", "CLAV"]
        .iter()
        .find_map(|t| balances.iter().find(|b| b.balance_type == *t))
        .or_else(|| balances.first())
}

/// A non-blank string, or nothing (whatever the JSON value is).
fn text<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) if !s.trim().is_empty() => Some(s),
        _ => None,
    })
}

/// `T` if the value has its shape, else nothing.
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Option<T>, D::Error> {
    Ok(T::deserialize(Value::deserialize(d)?).ok())
}

#[derive(Debug, Default, Deserialize)]
struct RawAmount {
    #[serde(default, deserialize_with = "text")]
    amount: Option<String>,
    #[serde(default, deserialize_with = "text")]
    currency: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawParty {
    #[serde(default, deserialize_with = "text")]
    name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawAccount {
    #[serde(default, deserialize_with = "text")]
    iban: Option<String>,
}

/// A transaction as Enable Banking sends it (only the fields fine uses).
#[derive(Debug, Default, Deserialize)]
struct RawTransaction {
    #[serde(default, deserialize_with = "text")]
    status: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    transaction_amount: Option<RawAmount>,
    #[serde(default, deserialize_with = "text")]
    credit_debit_indicator: Option<String>,
    #[serde(default, deserialize_with = "text")]
    booking_date: Option<String>,
    #[serde(default, deserialize_with = "text")]
    value_date: Option<String>,
    #[serde(default, deserialize_with = "text")]
    transaction_date: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    creditor: Option<RawParty>,
    #[serde(default, deserialize_with = "lenient")]
    debtor: Option<RawParty>,
    #[serde(default, deserialize_with = "lenient")]
    creditor_account: Option<RawAccount>,
    #[serde(default, deserialize_with = "lenient")]
    debtor_account: Option<RawAccount>,
    #[serde(default, deserialize_with = "lenient")]
    remittance_information: Option<Vec<Value>>,
    #[serde(default, deserialize_with = "text")]
    entry_reference: Option<String>,
}

/// A booked transaction, ready to store.
#[derive(Debug, PartialEq)]
pub struct BookedTransaction {
    pub booking_date: NaiveDate,
    /// Negative for money out.
    pub amount: Money,
    pub currency: String,
    pub counterparty: Option<String>,
    /// Without spaces, uppercase.
    pub counterparty_iban: Option<String>,
    pub description: Option<String>,
    pub entry_reference: Option<String>,
}

impl BookedTransaction {
    /// Parse one transaction from the bank's JSON. `None` for pending (not booked) or unusable
    /// entries: pending ones change id and amount when they are booked.
    pub fn parse(json: &str) -> Option<Self> {
        let raw: RawTransaction = serde_json::from_str(json).ok()?;
        if raw.status.as_deref().is_some_and(|s| s != "BOOK") {
            return None;
        }
        let amount = raw.transaction_amount.unwrap_or_default();
        let parsed = Money::parse(amount.amount.as_deref()?).ok()?;
        let debit = match raw.credit_debit_indicator.as_deref() {
            Some("DBIT") => true,
            Some("CRDT") => false,
            _ => parsed.is_negative(),
        };
        let date = raw
            .booking_date
            .or(raw.value_date)
            .or(raw.transaction_date)?;
        let booking_date = NaiveDate::parse_from_str(date.get(..10)?, "%Y-%m-%d").ok()?;
        let (party, account) = if debit {
            (raw.creditor, raw.creditor_account)
        } else {
            (raw.debtor, raw.debtor_account)
        };
        let description = raw
            .remittance_information
            .map(|lines| {
                lines
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|s| !s.trim().is_empty());
        Some(Self {
            booking_date,
            amount: if debit { -parsed.abs() } else { parsed.abs() },
            currency: amount.currency.unwrap_or_else(|| "EUR".to_owned()),
            counterparty: party.and_then(|p| p.name).map(|s| s.trim().to_owned()),
            counterparty_iban: account
                .and_then(|a| a.iban)
                .map(|s| s.replace(' ', "").to_uppercase()),
            description,
            entry_reference: raw.entry_reference,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_debit_and_skips_pending() {
        let v = json!({
            "transaction_amount": {"amount": "12.50", "currency": "EUR"},
            "credit_debit_indicator": "DBIT",
            "status": "BOOK",
            "booking_date": "2026-03-01",
            "creditor": {"name": "ESSELUNGA"},
            "remittance_information": ["POS", "MILANO"]
        });
        let p = BookedTransaction::parse(&v.to_string()).unwrap();
        assert_eq!(p.amount, Money::from_cents(-1250));
        assert_eq!(p.counterparty.as_deref(), Some("ESSELUNGA"));
        assert_eq!(p.description.as_deref(), Some("POS MILANO"));
        let pending = json!({"transaction_amount": {"amount": "1", "currency": "EUR"}, "status": "PDNG", "booking_date": "2026-03-01"});
        assert!(BookedTransaction::parse(&pending.to_string()).is_none());
    }

    #[test]
    fn ignores_fields_with_unexpected_shapes() {
        let v = json!({
            "transaction_amount": {"amount": "3", "currency": 978},
            "booking_date": "2026-03-01T10:00:00",
            "debtor": "not an object",
            "remittance_information": "not a list"
        });
        let p = BookedTransaction::parse(&v.to_string()).unwrap();
        assert_eq!(p.amount, Money::from_cents(300));
        assert_eq!(p.currency, "EUR");
        assert_eq!(p.counterparty, None);
    }
}
