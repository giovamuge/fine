//! Deterministic analysis over stored transactions. All functions take `today` for testability.
//!
//! Conventions:
//! - "spent" amounts are positive cents (money out); refunds in expense categories reduce them.
//! - Uncategorized transactions count as expense (if negative) or income (if positive).
//! - Transfers between own accounts and savings/investments are never "spending".
//! - Amortized purchases are spread over N monthly slices starting from their booking month.
//! - Payments matched to an annual planned expense are excluded from monthly spending:
//!   they are covered by the monthly sinking-fund quota instead.

use anyhow::Result;
use chrono::{Datelike, Days, Months, NaiveDate};
use rusqlite::{Connection, params};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

use crate::categorize::normalize_text;
use crate::money::{eur, split};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Expense,
    Income,
    Transfer,
    Saving,
}

#[derive(Debug, Clone)]
pub struct Tx {
    pub id: i64,
    pub date: NaiveDate,
    pub cents: i64,
    pub category_id: Option<i64>,
    pub category: Option<String>,
    pub kind: Kind,
    pub merchant: String,
    pub counterparty: Option<String>,
    pub description: Option<String>,
    pub amort_months: Option<i64>,
}

impl Tx {
    fn haystack(&self) -> String {
        format!(
            " {} {} {} ",
            self.merchant,
            normalize_text(self.counterparty.as_deref().unwrap_or("")),
            normalize_text(self.description.as_deref().unwrap_or(""))
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Cadence {
    Week,
    Month,
    Year,
}

impl Cadence {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "week" => Some(Self::Week),
            "month" => Some(Self::Month),
            "year" => Some(Self::Year),
            _ => None,
        }
    }
    pub fn step(self, d: NaiveDate, forward: bool) -> NaiveDate {
        match (self, forward) {
            (Self::Week, true) => d + Days::new(7),
            (Self::Week, false) => d - Days::new(7),
            (Self::Month, true) => d + Months::new(1),
            (Self::Month, false) => d - Months::new(1),
            (Self::Year, true) => d + Months::new(12),
            (Self::Year, false) => d - Months::new(12),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Planned {
    pub id: i64,
    pub name: String,
    pub kind: Kind,
    #[serde(skip)]
    pub amount_cents: i64,
    pub amount: f64,
    pub cadence: Cadence,
    pub next_due: NaiveDate,
    #[serde(skip)]
    pub category_id: Option<i64>,
    pub category: Option<String>,
    pub pattern: Option<String>,
}

impl Planned {
    /// Does this transaction look like a payment (or receipt) of this planned item?
    pub fn matches(&self, tx: &Tx) -> bool {
        let sign_ok = match self.kind {
            Kind::Income => tx.cents > 0,
            _ => tx.cents < 0,
        };
        if !sign_ok {
            return false;
        }
        match &self.pattern {
            Some(p) if !p.trim().is_empty() => tx.haystack().contains(&normalize_text(p)),
            _ => {
                let amt = tx.cents.abs();
                self.category_id.is_some()
                    && self.category_id == tx.category_id
                    && amt * 10 >= self.amount_cents * 8
                    && amt * 10 <= self.amount_cents * 12
            }
        }
    }

    /// First due date on or after `today` (planned dates roll forward automatically).
    pub fn next_due_from(&self, today: NaiveDate) -> NaiveDate {
        let mut d = self.next_due;
        while d < today {
            d = self.cadence.step(d, true);
        }
        d
    }

    /// Expected amount falling inside [start, end].
    pub fn expected_between(&self, start: NaiveDate, end: NaiveDate) -> i64 {
        let mut d = self.next_due;
        while d > start {
            d = self.cadence.step(d, false);
        }
        while d < start {
            d = self.cadence.step(d, true);
        }
        let mut n = 0;
        while d <= end {
            n += 1;
            d = self.cadence.step(d, true);
        }
        n * self.amount_cents
    }

    pub fn monthly_equivalent(&self) -> i64 {
        match self.cadence {
            Cadence::Week => self.amount_cents * 52 / 12,
            Cadence::Month => self.amount_cents,
            Cadence::Year => self.amount_cents / 12,
        }
    }
}

pub fn month_start(d: NaiveDate) -> NaiveDate {
    d.with_day(1).expect("day 1")
}
pub fn month_end(d: NaiveDate) -> NaiveDate {
    month_start(d) + Months::new(1) - Days::new(1)
}
pub fn week_start(d: NaiveDate) -> NaiveDate {
    d - Days::new(d.weekday().num_days_from_monday() as u64)
}
pub fn year_start(d: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(d.year(), 1, 1).expect("jan 1")
}
fn parse_date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap_or_default()
}

/// Load transactions booked in [from, to], plus earlier amortized ones whose slices may land there.
pub fn load(conn: &Connection, from: NaiveDate, to: NaiveDate) -> Result<Vec<Tx>> {
    let amort_from = from - Months::new(120);
    let mut st = conn.prepare(
        "SELECT t.id, t.booking_date, t.amount_cents, t.category_id, c.name, c.kind, t.merchant,
                t.counterparty, t.description, a.months
         FROM transactions t
         LEFT JOIN categories c ON c.id = t.category_id
         LEFT JOIN amortizations a ON a.transaction_id = t.id
         WHERE t.currency = ?4
           AND ((t.booking_date BETWEEN ?1 AND ?2)
            OR (a.months IS NOT NULL AND t.booking_date BETWEEN ?3 AND ?2))
         ORDER BY t.booking_date, t.id",
    )?;
    let rows = st.query_map(
        params![
            from.to_string(),
            to.to_string(),
            amort_from.to_string(),
            crate::db::BASE_CURRENCY
        ],
        |r| {
            let cents: i64 = r.get(2)?;
            let kind: Option<String> = r.get(5)?;
            let kind = match kind.as_deref() {
                Some("income") => Kind::Income,
                Some("transfer") => Kind::Transfer,
                Some("saving") => Kind::Saving,
                Some(_) => Kind::Expense,
                None if cents > 0 => Kind::Income,
                None => Kind::Expense,
            };
            Ok(Tx {
                id: r.get(0)?,
                date: parse_date(&r.get::<_, String>(1)?),
                cents,
                category_id: r.get(3)?,
                category: r.get(4)?,
                kind,
                merchant: r.get(6)?,
                counterparty: r.get(7)?,
                description: r.get(8)?,
                amort_months: r.get(9)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn load_planned(conn: &Connection) -> Result<Vec<Planned>> {
    let mut st = conn.prepare(
        "SELECT p.id, p.name, p.kind, p.amount_cents, p.cadence, p.next_due, p.category_id, c.name, p.pattern
         FROM planned p LEFT JOIN categories c ON c.id = p.category_id ORDER BY p.name",
    )?;
    let rows = st.query_map([], |r| {
        let amount: i64 = r.get(3)?;
        Ok(Planned {
            id: r.get(0)?,
            name: r.get(1)?,
            kind: if r.get::<_, String>(2)? == "income" {
                Kind::Income
            } else {
                Kind::Expense
            },
            amount_cents: amount,
            amount: eur(amount),
            cadence: Cadence::parse(&r.get::<_, String>(4)?).unwrap_or(Cadence::Month),
            next_due: parse_date(&r.get::<_, String>(5)?),
            category_id: r.get(6)?,
            category: r.get(7)?,
            pattern: r.get(8)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// One effective expense entry (after amortization spreading).
#[derive(Debug, Clone)]
pub struct Entry {
    pub tx_id: i64,
    pub date: NaiveDate,
    pub spent: i64,
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
    for tx in txs.iter().filter(|t| t.kind == Kind::Expense) {
        let annual = planned
            .iter()
            .any(|p| p.kind == Kind::Expense && p.cadence == Cadence::Year && p.matches(tx));
        if annual {
            continue;
        }
        let fixed = planned
            .iter()
            .any(|p| p.kind == Kind::Expense && p.cadence != Cadence::Year && p.matches(tx));
        let spent = -tx.cents;
        match tx.amort_months {
            Some(m) if m > 1 => {
                for (k, slice) in split(spent, m).into_iter().enumerate() {
                    let date = if k == 0 {
                        tx.date
                    } else {
                        month_start(tx.date) + Months::new(k as u32)
                    };
                    if date >= from && date <= to {
                        out.push(Entry {
                            tx_id: tx.id,
                            date,
                            spent: slice,
                            category: tx.category.clone(),
                            fixed,
                        });
                    }
                }
            }
            _ if tx.date >= from && tx.date <= to => out.push(Entry {
                tx_id: tx.id,
                date: tx.date,
                spent,
                category: tx.category.clone(),
                fixed,
            }),
            _ => {}
        }
    }
    out
}

fn income_between(txs: &[Tx], from: NaiveDate, to: NaiveDate) -> i64 {
    txs.iter()
        .filter(|t| t.kind == Kind::Income && t.date >= from && t.date <= to)
        .map(|t| t.cents)
        .sum()
}

#[derive(Debug, Serialize)]
pub struct CategoryAmount {
    pub category: String,
    pub amount: f64,
    pub share: f64,
}

#[derive(Debug, Serialize)]
pub struct MonthSummary {
    pub month: String,
    pub income: f64,
    pub expenses: f64,
    pub saved: f64,
    /// (income - expenses) / income, 0..1 (can be negative).
    pub savings_rate: Option<f64>,
    pub moved_to_savings: f64,
    pub by_category: Vec<CategoryAmount>,
    pub uncategorized: usize,
}

pub fn monthly_summary(conn: &Connection, any_day_in_month: NaiveDate) -> Result<MonthSummary> {
    let (from, to) = (month_start(any_day_in_month), month_end(any_day_in_month));
    let txs = load(conn, from, to)?;
    let planned = load_planned(conn)?;
    let entries = expense_entries(&txs, &planned, from, to);
    let income = income_between(&txs, from, to);
    let expenses: i64 = entries.iter().map(|e| e.spent).sum();
    let mut by_cat: BTreeMap<String, i64> = BTreeMap::new();
    for e in &entries {
        *by_cat
            .entry(
                e.category
                    .clone()
                    .unwrap_or_else(|| "Da categorizzare".into()),
            )
            .or_default() += e.spent;
    }
    let mut by_category: Vec<CategoryAmount> = by_cat
        .into_iter()
        .map(|(category, c)| CategoryAmount {
            category,
            amount: eur(c),
            share: if expenses > 0 {
                (c as f64 / expenses as f64 * 1000.0).round() / 1000.0
            } else {
                0.0
            },
        })
        .collect();
    by_category.sort_by(|a, b| b.amount.total_cmp(&a.amount));
    let in_month = |t: &&Tx| t.date >= from && t.date <= to;
    let moved: i64 = txs
        .iter()
        .filter(in_month)
        .filter(|t| t.kind == Kind::Saving)
        .map(|t| -t.cents)
        .sum();
    Ok(MonthSummary {
        month: from.format("%Y-%m").to_string(),
        income: eur(income),
        expenses: eur(expenses),
        saved: eur(income - expenses),
        savings_rate: (income > 0)
            .then(|| ((income - expenses) as f64 / income as f64 * 1000.0).round() / 1000.0),
        moved_to_savings: eur(moved),
        by_category,
        uncategorized: txs
            .iter()
            .filter(in_month)
            .filter(|t| t.category_id.is_none())
            .count(),
    })
}

#[derive(Debug, Serialize)]
pub struct TrendPoint {
    pub month: String,
    pub income: f64,
    pub expenses: f64,
    pub saved: f64,
}

/// Income/expenses for the last `months` months (including the current one).
pub fn trend(conn: &Connection, today: NaiveDate, months: u32) -> Result<Vec<TrendPoint>> {
    let first = month_start(today) - Months::new(months.saturating_sub(1));
    let txs = load(conn, first, month_end(today))?;
    let planned = load_planned(conn)?;
    let mut out = Vec::new();
    for i in 0..months {
        let from = first + Months::new(i);
        let to = month_end(from);
        let exp: i64 = expense_entries(&txs, &planned, from, to)
            .iter()
            .map(|e| e.spent)
            .sum();
        let inc = income_between(&txs, from, to);
        out.push(TrendPoint {
            month: from.format("%Y-%m").to_string(),
            income: eur(inc),
            expenses: eur(exp),
            saved: eur(inc - exp),
        });
    }
    Ok(out)
}

/// Average monthly income over the last 3 complete months (0 if no data).
pub fn average_income(conn: &Connection, today: NaiveDate) -> Result<i64> {
    let from = month_start(today) - Months::new(3);
    let to = month_start(today) - Days::new(1);
    let txs = load(conn, from, to)?;
    let months_with_data: std::collections::HashSet<(i32, u32)> = txs
        .iter()
        .map(|t| (t.date.year(), t.date.month()))
        .collect();
    if months_with_data.is_empty() {
        return Ok(0);
    }
    Ok(income_between(&txs, from, to) / months_with_data.len() as i64)
}

#[derive(Debug, Serialize)]
pub struct BudgetStatus {
    pub category: String,
    pub period: String,
    pub limit: f64,
    pub spent: f64,
    pub remaining: f64,
    /// spent / limit
    pub used: f64,
    /// fraction of the period elapsed
    pub elapsed: f64,
    /// "ok" | "attenzione" | "sforato"
    pub state: &'static str,
}

pub fn period_range(period: &str, today: NaiveDate) -> (NaiveDate, NaiveDate) {
    match period {
        "week" => (week_start(today), week_start(today) + Days::new(6)),
        "year" => (
            year_start(today),
            year_start(today) + Months::new(12) - Days::new(1),
        ),
        _ => (month_start(today), month_end(today)),
    }
}

pub fn budget_statuses(conn: &Connection, today: NaiveDate) -> Result<Vec<BudgetStatus>> {
    let budgets: Vec<(String, String, i64)> = conn
        .prepare("SELECT c.name, b.period, b.amount_cents FROM budgets b JOIN categories c ON c.id = b.category_id ORDER BY b.period, c.name")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    if budgets.is_empty() {
        return Ok(vec![]);
    }
    let txs = load(conn, year_start(today).min(week_start(today)), today)?;
    let planned = load_planned(conn)?;
    let mut out = Vec::new();
    for (category, period, limit) in budgets {
        let (start, end) = period_range(&period, today);
        let spent: i64 = expense_entries(&txs, &planned, start, today)
            .iter()
            .filter(|e| e.category.as_deref() == Some(category.as_str()))
            .map(|e| e.spent)
            .sum();
        let total_days = (end - start).num_days() + 1;
        let elapsed = ((today - start).num_days() + 1) as f64 / total_days as f64;
        let used = spent as f64 / limit as f64;
        let state = if spent > limit {
            "sforato"
        } else if used > elapsed + 0.15 && used > 0.3 {
            "attenzione"
        } else {
            "ok"
        };
        out.push(BudgetStatus {
            category,
            period,
            limit: eur(limit),
            spent: eur(spent),
            remaining: eur(limit - spent),
            used: (used * 1000.0).round() / 1000.0,
            elapsed: (elapsed * 1000.0).round() / 1000.0,
            state,
        });
    }
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct Recurring {
    pub merchant: String,
    pub category: Option<String>,
    pub cadence: Cadence,
    pub typical_amount: f64,
    pub monthly_equivalent: f64,
    pub occurrences: usize,
    pub last_date: NaiveDate,
    pub next_expected: NaiveDate,
    /// Still active (next expected date not long overdue).
    pub active: bool,
    /// Already tracked as a planned item.
    pub planned: bool,
}

fn median(v: &mut [i64]) -> i64 {
    v.sort_unstable();
    v[v.len() / 2]
}

/// Detect recurring expenses (subscriptions, bills) from the last ~13 months.
pub fn recurring(conn: &Connection, today: NaiveDate) -> Result<Vec<Recurring>> {
    let txs = load(conn, today - Days::new(400), today)?;
    let planned = load_planned(conn)?;
    let mut groups: HashMap<&str, Vec<&Tx>> = HashMap::new();
    for t in txs
        .iter()
        .filter(|t| t.kind == Kind::Expense && t.merchant.len() >= 3 && t.date <= today)
    {
        groups.entry(t.merchant.as_str()).or_default().push(t);
    }
    let mut out = Vec::new();
    for (merchant, list) in groups {
        if list.len() < 2 {
            continue;
        }
        let mut gaps: Vec<i64> = list
            .windows(2)
            .map(|w| (w[1].date - w[0].date).num_days())
            .collect();
        let gap = median(&mut gaps);
        let cadence = match gap {
            6..=8 => Cadence::Week,
            26..=35 => Cadence::Month,
            350..=380 => Cadence::Year,
            _ => continue,
        };
        if cadence != Cadence::Year && list.len() < 3 {
            continue;
        }
        let amounts: Vec<i64> = list.iter().map(|t| -t.cents).collect();
        let typical = median(&mut amounts.clone());
        let consistent = amounts
            .iter()
            .filter(|a| (**a - typical).abs() * 5 <= typical)
            .count();
        if consistent * 10 < amounts.len() * 8 {
            continue;
        }
        let last = list.last().expect("non-empty");
        let next = cadence.step(last.date, true);
        let monthly = match cadence {
            Cadence::Week => typical * 52 / 12,
            Cadence::Month => typical,
            Cadence::Year => typical / 12,
        };
        out.push(Recurring {
            merchant: merchant.to_string(),
            category: last.category.clone(),
            cadence,
            typical_amount: eur(typical),
            monthly_equivalent: eur(monthly),
            occurrences: list.len(),
            last_date: last.date,
            next_expected: next,
            active: (today - next).num_days() <= gap / 3 + 5,
            planned: planned.iter().any(|p| p.matches(last)),
        });
    }
    out.sort_by(|a, b| b.monthly_equivalent.total_cmp(&a.monthly_equivalent));
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct SinkingFund {
    pub name: String,
    pub amount: f64,
    pub due: NaiveDate,
    pub months_to_due: i64,
    pub monthly_quota: f64,
    /// How much should already be set aside today to pay it on time.
    pub should_have_saved: f64,
}

pub fn sinking_funds(planned: &[Planned], today: NaiveDate) -> Vec<SinkingFund> {
    let mut out: Vec<SinkingFund> = planned
        .iter()
        .filter(|p| p.kind == Kind::Expense && p.cadence == Cadence::Year)
        .map(|p| {
            let due = p.next_due_from(today);
            let months = ((due.year() - today.year()) * 12 + due.month() as i32
                - today.month() as i32)
                .max(0) as i64;
            let quota = p.amount_cents / 12;
            SinkingFund {
                name: p.name.clone(),
                amount: eur(p.amount_cents),
                due,
                months_to_due: months,
                monthly_quota: eur(quota),
                should_have_saved: eur((p.amount_cents - quota * months).clamp(0, p.amount_cents)),
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
    pub amount: f64,
    pub typical: f64,
}

/// Expenses in the last 14 days larger than 3x the category median of the previous 6 months.
pub fn anomalies(conn: &Connection, today: NaiveDate) -> Result<Vec<Anomaly>> {
    let recent_from = today - Days::new(14);
    let txs = load(conn, today - Months::new(6) - Days::new(14), today)?;
    let mut history: HashMap<i64, Vec<i64>> = HashMap::new();
    for t in txs
        .iter()
        .filter(|t| t.kind == Kind::Expense && t.date < recent_from)
    {
        if let Some(c) = t.category_id {
            history.entry(c).or_default().push(-t.cents);
        }
    }
    let mut out = Vec::new();
    for t in txs
        .iter()
        .filter(|t| t.kind == Kind::Expense && t.date >= recent_from && t.amort_months.is_none())
    {
        let Some(c) = t.category_id else { continue };
        let Some(h) = history.get_mut(&c) else {
            continue;
        };
        if h.len() < 5 {
            continue;
        }
        let med = median(h);
        let spent = -t.cents;
        if spent > 3 * med && spent > 5000 {
            out.push(Anomaly {
                transaction_id: t.id,
                date: t.date,
                merchant: t.merchant.clone(),
                category: t.category.clone().unwrap_or_default(),
                amount: eur(spent),
                typical: eur(med),
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::db;

    pub fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    pub fn add_tx(
        conn: &Connection,
        date: &str,
        cents: i64,
        merchant: &str,
        category: Option<&str>,
    ) -> i64 {
        conn.execute(
            "INSERT OR IGNORE INTO accounts (uid, currency) VALUES ('a', 'EUR')",
            [],
        )
        .unwrap();
        let cat = category.map(|c| db::category_id(conn, c).unwrap());
        conn.execute(
            "INSERT INTO transactions (account_uid, booking_date, amount_cents, currency, counterparty, merchant, dedup_key, category_id)
             VALUES ('a', ?1, ?2, 'EUR', ?3, ?3, lower(hex(randomblob(8))), ?4)",
            params![date, cents, merchant, cat],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn amortization_spreads_over_months() {
        let conn = db::open_in_memory().unwrap();
        let id = add_tx(
            &conn,
            "2026-01-15",
            -120000,
            "laptop shop",
            Some("Shopping"),
        );
        conn.execute(
            "INSERT INTO amortizations (transaction_id, months) VALUES (?1, 12)",
            [id],
        )
        .unwrap();
        let jan = monthly_summary(&conn, d("2026-01-20")).unwrap();
        let mar = monthly_summary(&conn, d("2026-03-20")).unwrap();
        let next_jan = monthly_summary(&conn, d("2027-01-20")).unwrap();
        assert_eq!(jan.expenses, 100.0);
        assert_eq!(mar.expenses, 100.0);
        assert_eq!(next_jan.expenses, 0.0);
    }

    #[test]
    fn foreign_currency_is_excluded_from_analyses() {
        let conn = db::open_in_memory().unwrap();
        add_tx(
            &conn,
            "2026-11-02",
            -5000,
            "esselunga",
            Some("Spesa alimentare"),
        );
        add_tx(&conn, "2026-11-03", -90000, "usd broker", None);
        conn.execute(
            "UPDATE transactions SET currency = 'USD' WHERE merchant = 'usd broker'",
            [],
        )
        .unwrap();
        assert_eq!(
            monthly_summary(&conn, d("2026-11-10")).unwrap().expenses,
            50.0
        );
    }

    #[test]
    fn detects_monthly_subscription() {
        let conn = db::open_in_memory().unwrap();
        for m in [
            "2026-06-03",
            "2026-07-03",
            "2026-08-04",
            "2026-09-03",
            "2026-10-03",
        ] {
            add_tx(&conn, m, -1299, "netflix.com", Some("Abbonamenti"));
        }
        add_tx(&conn, "2026-09-10", -4000, "random shop", None);
        let r = recurring(&conn, d("2026-10-20")).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].cadence, Cadence::Month);
        assert_eq!(r[0].typical_amount, 12.99);
        assert!(r[0].active);
        assert!(!r[0].planned);
    }

    #[test]
    fn budget_pace_states() {
        let conn = db::open_in_memory().unwrap();
        let cat = db::category_id(&conn, "Ristoranti e bar").unwrap();
        conn.execute("INSERT INTO budgets VALUES (?1, 'month', 20000)", [cat])
            .unwrap();
        add_tx(
            &conn,
            "2026-11-02",
            -14000,
            "trattoria",
            Some("Ristoranti e bar"),
        );
        // 70% used at day 10/30 (33%) -> attenzione
        let s = budget_statuses(&conn, d("2026-11-10")).unwrap();
        assert_eq!(s[0].state, "attenzione");
        add_tx(
            &conn,
            "2026-11-09",
            -7000,
            "pizzeria",
            Some("Ristoranti e bar"),
        );
        assert_eq!(
            budget_statuses(&conn, d("2026-11-10")).unwrap()[0].state,
            "sforato"
        );
    }

    #[test]
    fn planned_expected_between_counts_occurrences() {
        let p = Planned {
            id: 1,
            name: "x".into(),
            kind: Kind::Expense,
            amount_cents: 1000,
            amount: 10.0,
            cadence: Cadence::Week,
            next_due: d("2026-11-02"),
            category_id: None,
            category: None,
            pattern: Some("x".into()),
        };
        // Mondays in Nov 2026: 2, 9, 16, 23, 30
        assert_eq!(p.expected_between(d("2026-11-01"), d("2026-11-30")), 5000);
        assert_eq!(p.next_due_from(d("2026-11-10")), d("2026-11-16"));
    }
}
