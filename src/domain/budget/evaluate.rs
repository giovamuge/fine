//! Budget evaluation: what counts in a period, rollover, pace, thresholds and reserves.

use std::collections::HashMap;

use chrono::{Days, NaiveDate};
use serde::Serialize;

use super::rule::Subject;
use super::{Basis, Budget, Goal, Period, Rollover, Rule};
use crate::domain::analysis::{self, round3};
use crate::domain::calendar::{self, days_between};
use crate::domain::category::CategoryKind;
use crate::domain::money::Money;
use crate::domain::planned::{self, Planned};
use crate::domain::transaction::Tx;
use crate::error::Result;
use crate::tr;

/// Previous periods looked at for rollover and history.
const LOOKBACK: usize = 12;
const HISTORY: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// The period has not started yet (fixed dates in the future)
    Upcoming,
    Active,
    /// Fixed dates in the past
    Ended,
}

/// The state of a budget. Serialized with the Italian names the dashboard knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum State {
    // max_spend
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "attenzione")]
    Warning,
    #[serde(rename = "sforato")]
    Over,
    // min_save / min_income
    #[serde(rename = "in_linea")]
    OnTrack,
    #[serde(rename = "indietro")]
    Behind,
    #[serde(rename = "raggiunto")]
    Reached,
    #[serde(rename = "mancato")]
    Missed,
    /// Paused
    #[serde(rename = "sospeso")]
    Paused,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeriodValue {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub limit: Money,
    pub value: Money,
}

#[derive(Debug, Clone, Serialize)]
pub struct CountedTx {
    pub id: i64,
    pub date: NaiveDate,
    /// What it adds to the budget (spent, saved or received; negative for refunds/withdrawals)
    pub amount: Money,
    pub merchant: String,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BudgetStatus {
    pub id: i64,
    pub name: String,
    pub goal: Goal,
    pub period_label: String,
    /// Readable description of the rule
    pub rule_summary: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub phase: Phase,
    /// Base amount per period
    pub amount: f64,
    /// Carried over from previous periods (rollover)
    pub carried: Money,
    /// Effective limit or target for this period (amount + carried)
    pub limit: Money,
    /// Spent (max_spend), saved (min_save) or received (min_income) so far
    pub value: Money,
    /// What is left to spend, or still missing to the target
    pub remaining: Money,
    /// value / limit
    pub used: f64,
    /// Fraction of the period elapsed
    pub elapsed: f64,
    /// Linear projection at the end of the period
    pub projected: Option<Money>,
    /// Daily allowance until the end of the period (max_spend)
    pub per_day: Option<Money>,
    pub state: State,
    /// Alert thresholds (percent) already reached
    pub thresholds_reached: Vec<u32>,
    /// Amount taken out of this month's "available" (reserve)
    pub reserved: Money,
    /// Previous periods, oldest first (recurring budgets)
    pub history: Vec<PeriodValue>,
    /// Things to fix, e.g. a category in the rule that no longer exists
    pub problems: Vec<String>,
    /// The full definition, for editing
    pub budget: Budget,
    /// Transactions that count in the current period (detail and preview only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transactions: Option<Vec<CountedTx>>,
}

/// Earliest date needed to evaluate these budgets (current period plus lookback).
pub fn earliest_needed<'a>(
    budgets: impl IntoIterator<Item = &'a Budget>,
    today: NaiveDate,
) -> Result<NaiveDate> {
    let mut from = today;
    for b in budgets {
        let (mut start, _) = b.period.range(today)?;
        if b.period.is_recurring() {
            for _ in 0..LOOKBACK {
                start = b.period.range(start - Days::new(1))?.0;
            }
        }
        from = from.min(start);
    }
    Ok(from)
}

/// Evaluates budgets against the transactions loaded from `earliest_needed` to today.
pub struct Evaluator {
    txs: Vec<Tx>,
    planned: Vec<Planned>,
    categories: Vec<String>,
    today: NaiveDate,
}

impl Evaluator {
    pub fn new(
        txs: Vec<Tx>,
        planned: Vec<Planned>,
        categories: Vec<String>,
        today: NaiveDate,
    ) -> Self {
        Self {
            txs,
            planned,
            categories,
            today,
        }
    }

    /// Transactions counting towards `b` in [from, to], with the date and value they count for.
    fn counted(&self, b: &Budget, from: NaiveDate, to: NaiveDate) -> Vec<(&Tx, NaiveDate, Money)> {
        let ok = |tx: &Tx, fixed: bool| {
            b.rule
                .as_ref()
                .is_none_or(|r: &Rule| r.matches(&Subject { tx, fixed }))
        };
        let in_period = |t: &&Tx, kind| t.kind == kind && t.in_range(from, to);
        match (b.goal, b.basis) {
            (Goal::MaxSpend, Basis::Amortized) => {
                let by_id: HashMap<i64, &Tx> = self.txs.iter().map(|t| (t.id, t)).collect();
                analysis::expense_entries(&self.txs, &self.planned, from, to)
                    .into_iter()
                    .filter_map(|e| {
                        let tx = by_id[&e.tx_id];
                        ok(tx, e.fixed).then_some((tx, e.date, e.spent))
                    })
                    .collect()
            }
            (Goal::MaxSpend, Basis::Cash) => self
                .txs
                .iter()
                .filter(|t| in_period(t, CategoryKind::Expense))
                .filter(|t| ok(t, planned::is_fixed_payment(t, &self.planned)))
                .map(|t| (t, t.date, -t.amount))
                .collect(),
            (Goal::MinSave, _) => self
                .txs
                .iter()
                .filter(|t| in_period(t, CategoryKind::Saving) && ok(t, false))
                .map(|t| (t, t.date, -t.amount))
                .collect(),
            (Goal::MinIncome, _) => self
                .txs
                .iter()
                .filter(|t| in_period(t, CategoryKind::Income) && ok(t, false))
                .map(|t| (t, t.date, t.amount))
                .collect(),
        }
    }

    fn total(&self, b: &Budget, from: NaiveDate, to: NaiveDate) -> Money {
        self.counted(b, from, to).iter().map(|c| c.2).sum()
    }

    /// The state of budget `b` (stored with `id`, created on `created`) today.
    pub fn evaluate(
        &self,
        id: i64,
        b: &Budget,
        created: NaiveDate,
        with_txs: bool,
    ) -> Result<BudgetStatus> {
        let today = self.today;
        let (start, end) = b.period.range(today)?;
        let phase = if today < start {
            Phase::Upcoming
        } else if today > end {
            Phase::Ended
        } else {
            Phase::Active
        };
        let amount = b.amount()?;

        // Previous periods (oldest first) for history and rollover.
        let mut previous = Vec::new();
        if b.period.is_recurring() {
            let mut d = start;
            for _ in 0..LOOKBACK {
                let (ps, pe) = b.period.range(d - Days::new(1))?;
                previous.push((ps, pe, self.total(b, ps, pe)));
                d = ps;
            }
            previous.reverse();
        }
        let first_counted = b.period.range(created)?.0;
        let cap = b.rollover_cap.map(Money::from_eur).transpose()?;
        let mut carry = Money::ZERO;
        let mut history = Vec::new();
        for &(ps, pe, value) in &previous {
            let limit = amount + carry;
            if b.rollover != Rollover::None && ps >= first_counted {
                let diff = limit - value;
                carry = match b.rollover {
                    Rollover::Surplus => diff.max(Money::ZERO),
                    Rollover::Deficit => diff.min(Money::ZERO),
                    Rollover::Both => diff,
                    Rollover::None => Money::ZERO,
                };
                if let Some(c) = cap {
                    carry = carry.max(-c).min(c);
                }
            }
            history.push(PeriodValue {
                start: ps,
                end: pe,
                limit,
                value,
            });
        }
        let history = history.split_off(history.len().saturating_sub(HISTORY));
        let limit = amount + carry;

        let counted = if phase == Phase::Upcoming {
            vec![]
        } else {
            self.counted(b, start, today.min(end))
        };
        let value: Money = counted.iter().map(|c| c.2).sum();
        let total_days = days_between(start, end);
        let elapsed_days = match phase {
            Phase::Upcoming => 0,
            Phase::Ended => total_days,
            Phase::Active => days_between(start, today),
        };
        let rolling = matches!(b.period, Period::Rolling { .. });
        let elapsed = if rolling {
            1.0
        } else {
            elapsed_days as f64 / total_days as f64
        };
        let used = value.cents() as f64 / limit.cents().max(1) as f64;
        let days_left = days_between(today, end);
        let projected = (phase == Phase::Active && !rolling && elapsed_days > 0)
            .then(|| value * total_days / elapsed_days);
        let thresholds_reached: Vec<u32> = b
            .alerts
            .iter()
            .copied()
            .filter(|t| used * 100.0 >= f64::from(*t))
            .collect();
        let pace_ahead = b.pace_alert
            && phase == Phase::Active
            && !rolling
            && used > elapsed + 0.15
            && used > 0.3;

        let (remaining, per_day, state) = match b.goal {
            Goal::MaxSpend => {
                let remaining = limit - value;
                let per_day = (phase != Phase::Ended && !rolling)
                    .then(|| remaining.max(Money::ZERO) / days_left.min(total_days).max(1));
                let state = if value > limit {
                    State::Over
                } else if pace_ahead || thresholds_reached.iter().any(|t| *t < 100) {
                    State::Warning
                } else {
                    State::Ok
                };
                (remaining, per_day, state)
            }
            Goal::MinSave | Goal::MinIncome => {
                let state = if value >= limit {
                    State::Reached
                } else if phase == Phase::Ended {
                    State::Missed
                } else if b.pace_alert && !rolling && elapsed > 0.3 && used < elapsed - 0.15 {
                    State::Behind
                } else {
                    State::OnTrack
                };
                ((limit - value).max(Money::ZERO), None, state)
            }
        };
        let state = if b.enabled { state } else { State::Paused };

        // Reserve: the unspent part falling in the rest of this month is already committed.
        let reserved = if b.enabled
            && b.reserve
            && b.goal == Goal::MaxSpend
            && phase != Phase::Ended
            && remaining.is_positive()
        {
            let from = today.max(start);
            let month_end = calendar::month_end(today);
            if from > month_end {
                Money::ZERO
            } else {
                let in_month = days_between(from, end.min(month_end));
                let left = days_between(from, end);
                remaining * in_month / left.max(1)
            }
        } else {
            Money::ZERO
        };

        let problems = b
            .rule
            .as_ref()
            .map(|r| {
                r.categories()
                    .into_iter()
                    .filter(|n| !self.categories.iter().any(|c| c.eq_ignore_ascii_case(n)))
                    .map(|n| {
                        tr!(
                            "la categoria «{n}» non esiste più: modifica la regola",
                            "the category “{n}” no longer exists: edit the rule"
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        let transactions = with_txs.then(|| {
            let mut v: Vec<CountedTx> = counted
                .iter()
                .map(|(tx, date, amount)| CountedTx {
                    id: tx.id,
                    date: *date,
                    amount: *amount,
                    merchant: tx.merchant.clone(),
                    category: tx.category.clone(),
                })
                .collect();
            v.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
            v
        });

        Ok(BudgetStatus {
            id,
            name: b.name.clone(),
            goal: b.goal,
            period_label: b.period.label(),
            rule_summary: b.rule_summary(),
            start,
            end,
            phase,
            amount: b.amount,
            carried: carry,
            limit,
            value,
            remaining,
            used: round3(used),
            elapsed: round3(elapsed),
            projected,
            per_day,
            state,
            thresholds_reached,
            reserved,
            history,
            problems,
            budget: b.clone(),
            transactions,
        })
    }
}
