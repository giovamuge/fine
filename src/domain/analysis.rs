//! Deterministic analyses over loaded transactions. Everything takes `today` (or the period) as
//! an argument, so results are reproducible and testable.
//!
//! Conventions:
//! - "spent" amounts are positive (money out); refunds in expense categories reduce them.
//! - Uncategorized transactions count as expense (if negative) or income (if positive).
//! - Transfers between own accounts and savings/investments are never "spending".
//! - Amortized purchases are spread over N monthly slices starting from their booking month.
//! - Payments matched to an annual planned expense are excluded from monthly spending:
//!   they are covered by the monthly sinking-fund quota instead.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{Datelike, Days, Months, NaiveDate};
use serde::Serialize;

use super::calendar::{self, Cadence, month_end, month_start};
use super::category::CategoryKind;
use super::money::Money;
use super::planned::{self, Planned};
use super::transaction::Tx;

/// Label of uncategorized spending in summaries.
const UNCATEGORIZED: &str = "Da categorizzare";

/// Rounded to three decimals (ratios in JSON output).
pub fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

/// One effective expense (after spreading amortized purchases).
#[derive(Debug, Clone)]
pub struct Entry {
    pub tx_id: i64,
    pub date: NaiveDate,
    pub spent: Money,
    pub category: Option<String>,
    /// Matched to a weekly/monthly planned expense (fixed cost).
    pub fixed: bool,
}

/// Effective expense entries in [from, to].
pub fn expense_entries(
    txs: &[Tx],
    planned: &[Planned],
    from: NaiveDate,
    to: NaiveDate,
) -> Vec<Entry> {
    let mut out = Vec::new();
    for tx in txs.iter().filter(|t| t.kind == CategoryKind::Expense) {
        if planned::is_annual_payment(tx, planned) {
            continue;
        }
        let fixed = planned::is_fixed_payment(tx, planned);
        let entry = |date, spent| Entry {
            tx_id: tx.id,
            date,
            spent,
            category: tx.category.clone(),
            fixed,
        };
        let spent = -tx.amount;
        match tx.amortized_months {
            Some(months) if months > 1 => {
                for (k, slice) in spent.split(months).into_iter().enumerate() {
                    let date = if k == 0 {
                        tx.date
                    } else {
                        month_start(tx.date) + Months::new(k as u32)
                    };
                    if date >= from && date <= to {
                        out.push(entry(date, slice));
                    }
                }
            }
            _ if tx.in_range(from, to) => out.push(entry(tx.date, spent)),
            _ => {}
        }
    }
    out
}

pub fn income_between(txs: &[Tx], from: NaiveDate, to: NaiveDate) -> Money {
    txs.iter()
        .filter(|t| t.kind == CategoryKind::Income && t.in_range(from, to))
        .map(|t| t.amount)
        .sum()
}

#[derive(Debug, Serialize)]
pub struct CategoryAmount {
    pub category: String,
    pub amount: Money,
    pub share: f64,
}

#[derive(Debug, Serialize)]
pub struct MonthSummary {
    pub month: String,
    pub income: Money,
    pub expenses: Money,
    pub saved: Money,
    /// (income - expenses) / income, 0..1 (can be negative).
    pub savings_rate: Option<f64>,
    pub moved_to_savings: Money,
    pub by_category: Vec<CategoryAmount>,
    pub uncategorized: usize,
}

/// Summary of the month containing `day`; `txs` are those loaded for that month.
pub fn monthly_summary(txs: &[Tx], planned: &[Planned], day: NaiveDate) -> MonthSummary {
    let (from, to) = (month_start(day), month_end(day));
    let entries = expense_entries(txs, planned, from, to);
    let income = income_between(txs, from, to);
    let expenses: Money = entries.iter().map(|e| e.spent).sum();
    let mut by_cat: BTreeMap<&str, Money> = BTreeMap::new();
    for e in &entries {
        *by_cat
            .entry(e.category.as_deref().unwrap_or(UNCATEGORIZED))
            .or_default() += e.spent;
    }
    let mut by_category: Vec<CategoryAmount> = by_cat
        .into_iter()
        .map(|(category, amount)| CategoryAmount {
            category: category.to_owned(),
            amount,
            share: if expenses.is_positive() {
                round3(amount.ratio(expenses))
            } else {
                0.0
            },
        })
        .collect();
    by_category.sort_by_key(|c| std::cmp::Reverse(c.amount));
    let in_month = || txs.iter().filter(move |t| t.in_range(from, to));
    MonthSummary {
        month: from.format("%Y-%m").to_string(),
        income,
        expenses,
        saved: income - expenses,
        savings_rate: income
            .is_positive()
            .then(|| round3((income - expenses).ratio(income))),
        moved_to_savings: -in_month()
            .filter(|t| t.kind == CategoryKind::Saving)
            .map(|t| t.amount)
            .sum::<Money>(),
        by_category,
        uncategorized: in_month().filter(|t| t.category_id.is_none()).count(),
    }
}

#[derive(Debug, Serialize)]
pub struct TrendPoint {
    pub month: String,
    pub income: Money,
    pub expenses: Money,
    pub saved: Money,
}

/// The first day of the trend window: `months` months including the current one.
pub fn trend_start(today: NaiveDate, months: u32) -> NaiveDate {
    month_start(today) - Months::new(months.saturating_sub(1))
}

/// Income and expenses month by month, from `trend_start`.
pub fn trend(txs: &[Tx], planned: &[Planned], today: NaiveDate, months: u32) -> Vec<TrendPoint> {
    let first = trend_start(today, months);
    (0..months)
        .map(|i| {
            let from = first + Months::new(i);
            let to = month_end(from);
            let expenses: Money = expense_entries(txs, planned, from, to)
                .iter()
                .map(|e| e.spent)
                .sum();
            let income = income_between(txs, from, to);
            TrendPoint {
                month: from.format("%Y-%m").to_string(),
                income,
                expenses,
                saved: income - expenses,
            }
        })
        .collect()
}

/// The window `average_income` looks at: the last 3 complete months.
pub fn average_income_window(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    (
        month_start(today) - Months::new(3),
        month_start(today) - Days::new(1),
    )
}

/// Average monthly income over the months with data in the window (zero without data).
pub fn average_income(txs: &[Tx], today: NaiveDate) -> Money {
    let (from, to) = average_income_window(today);
    let months: HashSet<(i32, u32)> = txs
        .iter()
        .map(|t| (t.date.year(), t.date.month()))
        .collect();
    if months.is_empty() {
        return Money::ZERO;
    }
    income_between(txs, from, to) / months.len() as i64
}

#[derive(Debug, Serialize)]
pub struct Recurring {
    pub merchant: String,
    pub category: Option<String>,
    pub cadence: Cadence,
    pub typical_amount: Money,
    pub monthly_equivalent: Money,
    pub occurrences: usize,
    pub last_date: NaiveDate,
    pub next_expected: NaiveDate,
    /// Still active (next expected date not long overdue).
    pub active: bool,
    /// Already tracked as a planned item.
    pub planned: bool,
}

/// How far back `recurring` looks (about 13 months).
pub const RECURRING_LOOKBACK_DAYS: u64 = 400;

fn median<T: Ord + Copy>(v: &mut [T]) -> T {
    v.sort_unstable();
    v[v.len() / 2]
}

/// Detect recurring expenses (subscriptions, bills): same merchant at a regular interval for a
/// consistent amount. Most expensive first, then by merchant.
pub fn recurring(txs: &[Tx], planned: &[Planned], today: NaiveDate) -> Vec<Recurring> {
    let mut groups: BTreeMap<&str, Vec<&Tx>> = BTreeMap::new();
    for t in txs
        .iter()
        .filter(|t| t.kind == CategoryKind::Expense && t.merchant.len() >= 3 && t.date <= today)
    {
        groups.entry(t.merchant.as_str()).or_default().push(t);
    }
    let mut out: Vec<Recurring> = groups
        .into_iter()
        .filter(|(_, list)| list.len() >= 2)
        .filter_map(|(merchant, list)| {
            let mut gaps: Vec<i64> = list
                .windows(2)
                .map(|w| (w[1].date - w[0].date).num_days())
                .collect();
            let gap = median(&mut gaps);
            let cadence = match gap {
                6..=8 => Cadence::Week,
                26..=35 => Cadence::Month,
                350..=380 => Cadence::Year,
                _ => return None,
            };
            if cadence != Cadence::Year && list.len() < 3 {
                return None;
            }
            let amounts: Vec<Money> = list.iter().map(|t| -t.amount).collect();
            let typical = median(&mut amounts.clone());
            let consistent = amounts
                .iter()
                .filter(|a| (**a - typical).abs().cents() * 5 <= typical.cents())
                .count();
            if consistent * 10 < amounts.len() * 8 {
                return None;
            }
            let last = list.last().expect("groups have at least two transactions");
            let next = cadence.next(last.date);
            Some(Recurring {
                merchant: merchant.to_owned(),
                category: last.category.clone(),
                cadence,
                typical_amount: typical,
                monthly_equivalent: cadence.monthly(typical),
                occurrences: list.len(),
                last_date: last.date,
                next_expected: next,
                active: (today - next).num_days() <= gap / 3 + 5,
                planned: planned.iter().any(|p| p.matches(last)),
            })
        })
        .collect();
    out.sort_by_key(|r| std::cmp::Reverse(r.monthly_equivalent));
    out
}

#[derive(Debug, Serialize)]
pub struct SinkingFund {
    pub name: String,
    pub amount: Money,
    pub due: NaiveDate,
    pub months_to_due: i64,
    pub monthly_quota: Money,
    /// How much should already be set aside today to pay it on time.
    pub should_have_saved: Money,
}

/// For every annual expense, the monthly quota to put aside and how much should be there now.
pub fn sinking_funds(planned: &[Planned], today: NaiveDate) -> Vec<SinkingFund> {
    let mut out: Vec<SinkingFund> = planned
        .iter()
        .filter(|p| p.is_annual_expense())
        .map(|p| {
            let due = p.next_due_from(today);
            let months = calendar::months_until(today, due);
            let quota = p.amount / 12;
            SinkingFund {
                name: p.name.clone(),
                amount: p.amount,
                due,
                months_to_due: months,
                monthly_quota: quota,
                should_have_saved: (p.amount - quota * months).max(Money::ZERO).min(p.amount),
            }
        })
        .collect();
    out.sort_by_key(|s| s.due);
    out
}

#[derive(Debug, Serialize)]
pub struct Anomaly {
    pub transaction_id: i64,
    pub date: NaiveDate,
    pub merchant: String,
    pub category: String,
    pub amount: Money,
    pub typical: Money,
}

/// The window `anomalies` needs: six months of history before the last 14 days.
pub fn anomalies_window(today: NaiveDate) -> NaiveDate {
    today - Months::new(6) - Days::new(14)
}

/// Expenses in the last 14 days larger than 3x the category median of the previous 6 months
/// (and above 50 €).
pub fn anomalies(txs: &[Tx], today: NaiveDate) -> Vec<Anomaly> {
    let recent_from = today - Days::new(14);
    let mut history: HashMap<i64, Vec<Money>> = HashMap::new();
    for t in txs
        .iter()
        .filter(|t| t.kind == CategoryKind::Expense && t.date < recent_from)
    {
        if let Some(c) = t.category_id {
            history.entry(c).or_default().push(-t.amount);
        }
    }
    let threshold = Money::from_cents(5000);
    txs.iter()
        .filter(|t| {
            t.kind == CategoryKind::Expense && t.date >= recent_from && t.amortized_months.is_none()
        })
        .filter_map(|t| {
            let past = history.get_mut(&t.category_id?)?;
            if past.len() < 5 {
                return None;
            }
            let typical = median(past);
            let spent = -t.amount;
            (spent > typical * 3 && spent > threshold).then(|| Anomaly {
                transaction_id: t.id,
                date: t.date,
                merchant: t.merchant.clone(),
                category: t.category.clone().unwrap_or_default(),
                amount: spent,
                typical,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::testing::{d, tx};

    #[test]
    fn amortization_spreads_over_months() {
        let mut laptop = tx(1, "2026-01-15", -120000, "laptop shop", Some("Shopping"));
        laptop.amortized_months = Some(12);
        let txs = [laptop];
        let jan = monthly_summary(&txs, &[], d("2026-01-20"));
        let mar = monthly_summary(&txs, &[], d("2026-03-20"));
        let next_jan = monthly_summary(&txs, &[], d("2027-01-20"));
        assert_eq!(jan.expenses, Money::from_cents(10000));
        assert_eq!(mar.expenses, Money::from_cents(10000));
        assert_eq!(next_jan.expenses, Money::ZERO);
    }

    #[test]
    fn detects_monthly_subscription() {
        let mut txs: Vec<Tx> = [
            "2026-06-03",
            "2026-07-03",
            "2026-08-04",
            "2026-09-03",
            "2026-10-03",
        ]
        .iter()
        .enumerate()
        .map(|(i, m)| tx(i as i64, m, -1299, "netflix.com", Some("Abbonamenti")))
        .collect();
        txs.push(tx(9, "2026-09-10", -4000, "random shop", None));
        txs.sort_by_key(|t| t.date);
        let r = recurring(&txs, &[], d("2026-10-20"));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].cadence, Cadence::Month);
        assert_eq!(r[0].typical_amount, Money::from_cents(1299));
        assert!(r[0].active);
        assert!(!r[0].planned);
    }

    #[test]
    fn ties_are_ordered_by_merchant() {
        let mut txs = vec![];
        for (i, m) in ["2026-07-01", "2026-08-01", "2026-09-01"]
            .iter()
            .enumerate()
        {
            txs.push(tx(i as i64, m, -50000, "zeta", None));
            txs.push(tx(10 + i as i64, m, -50000, "alfa", None));
        }
        let r = recurring(&txs, &[], d("2026-09-10"));
        let names: Vec<&str> = r.iter().map(|r| r.merchant.as_str()).collect();
        assert_eq!(names, ["alfa", "zeta"]);
    }
}
