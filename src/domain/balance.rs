//! Balance history. Banks only give today's balance, so the past is reconstructed from the
//! stored transactions: balance(d) = balance(as_of) - movements in (d, as_of].

use std::collections::BTreeMap;

use chrono::{Days, NaiveDate};
use serde::Serialize;

use super::money::Money;

/// An account's last balance from the bank, with its daily net movements (base currency only).
#[derive(Debug)]
pub struct AccountLedger {
    pub label: String,
    pub balance: Money,
    pub currency: String,
    /// When the bank reported the balance (RFC 3339).
    pub as_of: String,
    /// `Some` for accounts in the base currency: they are summed in the total.
    pub daily: Option<BTreeMap<NaiveDate, Money>>,
}

#[derive(Debug, Serialize)]
pub struct AccountBalance {
    pub account: String,
    pub balance: Money,
    pub currency: String,
    /// When the bank reported this balance.
    pub as_of: String,
    /// Counted in `current` / `points` (only base-currency accounts are summed).
    pub in_total: bool,
}

#[derive(Debug, Serialize)]
pub struct BalancePoint {
    pub date: String,
    pub balance: Money,
}

#[derive(Debug, Serialize)]
pub struct BalanceChange {
    pub days: u64,
    pub change: Money,
}

#[derive(Debug, Serialize)]
pub struct BalanceHistory {
    /// Sum of the latest balances of the base-currency accounts.
    pub current: Option<Money>,
    pub accounts: Vec<AccountBalance>,
    /// End-of-day total balance, one point per day.
    pub points: Vec<BalancePoint>,
    /// Change over the last 7/30/90/365 days (only those covered by the history).
    pub changes: Vec<BalanceChange>,
}

/// Total balance over the last `days` days. The history starts at the oldest stored transaction
/// (before that it would be a guess).
pub fn history(ledgers: Vec<AccountLedger>, today: NaiveDate, days: u64) -> BalanceHistory {
    // (balance, as_of date, daily net movements) of the accounts in the total
    let mut series: Vec<(Money, NaiveDate, BTreeMap<NaiveDate, Money>)> = vec![];
    let mut accounts = Vec::with_capacity(ledgers.len());
    for l in ledgers {
        let in_total = l.daily.is_some();
        if let Some(daily) = l.daily {
            let as_of = l
                .as_of
                .get(..10)
                .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
                .unwrap_or_default();
            series.push((l.balance, as_of, daily));
        }
        accounts.push(AccountBalance {
            account: l.label,
            balance: l.balance,
            currency: l.currency,
            as_of: l.as_of,
            in_total,
        });
    }
    if series.is_empty() {
        return BalanceHistory {
            current: None,
            accounts,
            points: vec![],
            changes: vec![],
        };
    }

    let oldest = series
        .iter()
        .filter_map(|(_, _, daily)| daily.keys().next().copied())
        .min()
        .unwrap_or(today);
    let start = (today - Days::new(days)).max(oldest).min(today);
    let mut totals: Vec<(NaiveDate, Money)> = start
        .iter_days()
        .take_while(|d| *d <= today)
        .map(|d| (d, Money::ZERO))
        .collect();
    for (balance, as_of, daily) in &series {
        let up_to_as_of: Money = daily.range(..=*as_of).map(|(_, v)| v).sum();
        let mut running: Money = daily.range(..start).map(|(_, v)| v).sum();
        for (d, total) in &mut totals {
            running += daily.get(d).copied().unwrap_or_default();
            *total += *balance + running - up_to_as_of;
        }
    }
    let last = totals.last().map(|(_, v)| *v).unwrap_or_default();
    let changes = [7, 30, 90, 365]
        .into_iter()
        .filter_map(|k| {
            let past = today - Days::new(k);
            totals
                .iter()
                .find(|(d, _)| *d == past)
                .map(|(_, v)| BalanceChange {
                    days: k,
                    change: last - *v,
                })
        })
        .collect();
    BalanceHistory {
        current: Some(series.iter().map(|(b, _, _)| *b).sum()),
        accounts,
        points: totals
            .into_iter()
            .map(|(d, balance)| BalancePoint {
                date: d.to_string(),
                balance,
            })
            .collect(),
        changes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::testing::d;

    #[test]
    fn is_reconstructed_backwards() {
        let daily: BTreeMap<NaiveDate, Money> = [
            ("2026-03-01", 200000), // salary
            ("2026-03-05", -50000),
            ("2026-03-09", -10000),
            ("2026-03-11", -2000), // booked after the balance was read
        ]
        .into_iter()
        .map(|(day, c)| (d(day), Money::from_cents(c)))
        .collect();
        let ledgers = vec![
            AccountLedger {
                label: "a".into(),
                balance: Money::from_cents(100000),
                currency: "EUR".into(),
                as_of: "2026-03-10T08:00:00+00:00".into(),
                daily: Some(daily),
            },
            AccountLedger {
                label: "b".into(),
                balance: Money::from_cents(5000),
                currency: "USD".into(),
                as_of: "2026-03-10T08:00:00+00:00".into(),
                daily: None,
            },
        ];
        let h = history(ledgers, d("2026-03-12"), 365);
        assert_eq!(h.current, Some(Money::from_cents(100000)));
        assert!(!h.accounts[1].in_total);
        let at = |day: &str| {
            h.points
                .iter()
                .find(|p| p.date == day)
                .unwrap()
                .balance
                .cents()
        };
        assert_eq!(h.points.first().unwrap().date, "2026-03-01");
        assert_eq!(at("2026-03-01"), 160000);
        assert_eq!(at("2026-03-05"), 110000);
        assert_eq!(at("2026-03-10"), 100000);
        assert_eq!(at("2026-03-12"), 98000);
        let week = h.changes.iter().find(|c| c.days == 7).unwrap();
        assert_eq!(week.change.cents(), 98000 - 110000);
    }
}
