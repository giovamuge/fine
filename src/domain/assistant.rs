//! The assistant: "how much can I still spend?" and "am I overspending?".
//! Fully deterministic, so it runs for free from cron and its numbers are reproducible.
//!
//! Month model:
//!   income        = received + still expected (planned income, or 3-month average)
//!   committed     = fixed weekly/monthly planned expenses (paid or still due)
//!                 + 1/12 of every annual planned expense (sinking fund)
//!                 + savings goal
//!   variable budget = income - committed
//!   variable spent  = effective spending this month that is not a fixed planned payment
//!   reserved        = unspent part (this month) of budgets marked `reserve` (e.g. a trip)
//!   available       = variable budget - variable spent - reserved

use std::cmp::Reverse;
use std::fmt::Write as _;

use chrono::{DateTime, Days, NaiveDate, Utc};
use serde::Serialize;

use super::analysis::{self, Anomaly, Recurring, SinkingFund, round3};
use super::budget::{BudgetStatus, Goal, Phase, State};
use super::calendar::{Cadence, days_between, month_end, month_start, week_start};
use super::category::CategoryKind;
use super::money::Money;
use super::planned::{Planned, PlannedKind};
use super::transaction::Tx;
use crate::tr;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warning,
    Critical,
}

/// What an alert is about, for the dashboard to link the right page.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AlertKind {
    Income,
    Overspend,
    Pace,
    Budget,
    Upcoming,
    Recurring,
    Anomaly,
    Uncategorized,
    Consent,
    Stale,
    Sync,
    Currency,
}

#[derive(Debug, Serialize)]
pub struct Alert {
    pub level: Level,
    /// What the alert is about, for the dashboard to link the right page:
    /// income, overspend, pace, budget, upcoming, recurring, anomaly, uncategorized, consent, stale,
    /// sync, currency.
    pub kind: AlertKind,
    pub message: String,
}

impl Alert {
    fn new(level: Level, kind: AlertKind, message: String) -> Self {
        Self {
            level,
            kind,
            message,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum IncomeSource {
    #[serde(rename = "planned")]
    Planned,
    #[serde(rename = "average_3_months")]
    Average3Months,
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub today: NaiveDate,
    pub month: String,
    pub days_left_in_month: i64,
    pub balance_total: Option<Money>,
    pub income_received: Money,
    pub income_expected_remaining: Money,
    /// "planned" or "average_3_months"
    pub income_source: IncomeSource,
    pub fixed_paid: Money,
    pub fixed_still_due: Money,
    pub sinking_fund_quota: Money,
    pub savings_goal: Money,
    pub variable_budget: Money,
    pub variable_spent: Money,
    /// Set aside for budgets marked `reserve` (their unspent part falling in this month).
    pub reserved: Money,
    /// What you can still spend this month on variable expenses.
    pub available_month: Money,
    /// Suggested daily allowance for the rest of the month.
    pub available_per_day: Money,
    /// Allowance for the rest of this week (bounded by month end).
    pub available_week: Money,
    pub spent_this_week: Money,
    pub month_elapsed: f64,
    pub budget_used: f64,
    pub budgets: Vec<BudgetStatus>,
    pub uncategorized: i64,
    pub alerts: Vec<Alert>,
}

/// An account's consent and latest sync, for the health alerts.
#[derive(Debug)]
pub struct AccountHealth {
    pub label: String,
    pub consent_valid_until: Option<String>,
    /// Outcome of the latest sync (`None` if never synced).
    pub last_ok: Option<bool>,
    pub last_message: Option<String>,
    /// When the latest successful sync happened (RFC 3339).
    pub last_success_at: Option<String>,
}

/// Everything the assistant looks at, loaded by the caller.
#[derive(Debug)]
pub struct Inputs {
    /// Transactions from the start of the month (plus amortized ones spreading into it).
    pub month: Vec<Tx>,
    /// Transactions from the start of the week.
    pub week: Vec<Tx>,
    pub planned: Vec<Planned>,
    /// Average monthly income, used when no income is planned.
    pub average_income: Money,
    pub savings_goal: Money,
    pub budgets: Vec<BudgetStatus>,
    pub recurring: Vec<Recurring>,
    pub anomalies: Vec<Anomaly>,
    pub uncategorized: i64,
    /// Foreign-currency transactions in the analysed period without an exchange rate.
    pub unconverted: i64,
    pub accounts: Vec<AccountHealth>,
    pub balance_total: Option<Money>,
}

impl Inputs {
    /// Whether `average_income` is needed (no planned income).
    pub fn needs_average_income(planned: &[Planned]) -> bool {
        !planned.iter().any(|p| p.kind == PlannedKind::Income)
    }
}

pub fn status(inputs: Inputs, today: NaiveDate, now: DateTime<Utc>) -> Status {
    let Inputs {
        month: txs,
        week,
        planned,
        average_income,
        savings_goal,
        budgets,
        recurring,
        anomalies,
        uncategorized,
        unconverted,
        accounts,
        balance_total,
    } = inputs;
    let (m_start, m_end) = (month_start(today), month_end(today));
    let this_month = || txs.iter().filter(move |t| t.date >= m_start);
    let paid_for = |p: &Planned| -> Money {
        this_month()
            .filter(|t| p.matches(t))
            .map(|t| t.amount)
            .sum()
    };

    // Income
    let received: Money = this_month()
        .filter(|t| t.kind == CategoryKind::Income)
        .map(|t| t.amount)
        .sum();
    let planned_income: Vec<&Planned> = planned
        .iter()
        .filter(|p| p.kind == PlannedKind::Income)
        .collect();
    let (expected_remaining, income_source) = if planned_income.is_empty() {
        (
            (average_income - received).max(Money::ZERO),
            IncomeSource::Average3Months,
        )
    } else {
        let remaining = planned_income
            .iter()
            .map(|p| (p.expected_between(m_start, m_end) - paid_for(p)).max(Money::ZERO))
            .sum();
        (remaining, IncomeSource::Planned)
    };
    let income = received + expected_remaining;

    // Fixed planned expenses (weekly/monthly)
    let (mut fixed_paid, mut fixed_due) = (Money::ZERO, Money::ZERO);
    for p in planned.iter().filter(|p| p.is_fixed_expense()) {
        let paid = -paid_for(p);
        fixed_paid += paid;
        fixed_due += (p.expected_between(m_start, m_end) - paid).max(Money::ZERO);
    }
    let sinking: Money = planned
        .iter()
        .filter(|p| p.is_annual_expense())
        .map(|p| p.amount / 12)
        .sum();

    let variable = |txs: &[Tx], from| -> Money {
        analysis::expense_entries(txs, &planned, from, today)
            .iter()
            .filter(|e| !e.fixed)
            .map(|e| e.spent)
            .sum()
    };
    let variable_spent = variable(&txs, m_start);
    let variable_budget = income - fixed_paid - fixed_due - sinking - savings_goal;
    let reserved: Money = budgets.iter().map(|b| b.reserved).sum();
    let available = variable_budget - variable_spent - reserved;

    let days_left = days_between(today, m_end);
    let per_day = if available.is_positive() {
        available / days_left
    } else {
        Money::ZERO
    };
    let w_start = week_start(today);
    let w_end = (w_start + Days::new(6)).min(m_end);
    let spent_week = variable(&week, w_start);
    let elapsed = days_between(m_start, today) as f64 / days_between(m_start, m_end) as f64;
    let used = if variable_budget.is_positive() {
        variable_spent.ratio(variable_budget)
    } else {
        0.0
    };

    // Alerts
    let mut alerts = Vec::new();
    if income == Money::ZERO {
        alerts.push(Alert::new(
            Level::Info,
            AlertKind::Income,
            tr!(
                "Nessuna entrata nota: aggiungi lo stipendio come entrata prevista (upsert_planned) per calcolare il disponibile.",
                "No known income: add your salary as expected income (upsert_planned) to work out what you can spend."
            ),
        ));
    } else if available.is_negative() {
        alerts.push(Alert::new(
            Level::Critical,
            AlertKind::Overspend,
            tr!(
                "Hai superato il budget variabile del mese di {}.",
                "You're {} over this month's variable budget.",
                -available
            ),
        ));
    } else if used > elapsed + 0.15 && used > 0.3 {
        alerts.push(Alert::new(
            Level::Warning,
            AlertKind::Pace,
            tr!(
                "Stai spendendo più velocemente del previsto: {:.0}% del budget variabile usato al {:.0}% del mese.",
                "You're spending faster than planned: {:.0}% of the variable budget used {:.0}% into the month.",
                used * 100.0,
                elapsed * 100.0
            ),
        ));
    }
    alerts.extend(
        budgets
            .iter()
            .filter(|b| b.budget.enabled)
            .filter_map(|b| budget_alert(b, today)),
    );
    alerts.extend(upcoming_alerts(
        &analysis::sinking_funds(&planned, today),
        today,
    ));
    alerts.extend(
        recurring
            .iter()
            .filter(|r| r.active && !r.planned)
            .map(recurring_alert),
    );
    alerts.extend(anomalies.iter().map(anomaly_alert));
    if uncategorized > 0 {
        alerts.push(Alert::new(
            Level::Info,
            AlertKind::Uncategorized,
            if uncategorized == 1 {
                tr!(
                    "1 transazione da categorizzare (chiedi all'AI: \"categorizza le transazioni in sospeso\").",
                    "1 uncategorized transaction (ask the AI: \"categorize the pending transactions\")."
                )
            } else {
                tr!(
                    "{uncategorized} transazioni da categorizzare (chiedi all'AI: \"categorizza le transazioni in sospeso\").",
                    "{uncategorized} uncategorized transactions (ask the AI: \"categorize the pending transactions\")."
                )
            },
        ));
    }
    if unconverted > 0 {
        alerts.push(Alert::new(
            Level::Warning,
            AlertKind::Currency,
            tr!(
                "{unconverted} movimenti in valuta estera senza cambio BCE: restano fuori dai conti finché i cambi non si aggiornano (serve la connessione).",
                "{unconverted} foreign-currency transactions without an ECB rate: they are left out until the rates are updated (an internet connection is needed)."
            ),
        ));
    }
    alerts.extend(account_alerts(accounts, today, now));
    alerts.sort_by_key(|a| Reverse(a.level));

    Status {
        today,
        month: m_start.format("%Y-%m").to_string(),
        days_left_in_month: days_left,
        balance_total,
        income_received: received,
        income_expected_remaining: expected_remaining,
        income_source,
        fixed_paid,
        fixed_still_due: fixed_due,
        sinking_fund_quota: sinking,
        savings_goal,
        variable_budget,
        variable_spent,
        reserved,
        available_month: available,
        available_per_day: per_day,
        available_week: per_day * days_between(today, w_end),
        spent_this_week: spent_week,
        month_elapsed: round3(elapsed),
        budget_used: round3(used),
        budgets,
        uncategorized,
        alerts,
    }
}

fn upcoming_alerts(funds: &[SinkingFund], today: NaiveDate) -> impl Iterator<Item = Alert> + '_ {
    funds
        .iter()
        .filter(move |s| (s.due - today).num_days() <= 30)
        .map(|s| {
            Alert::new(
                Level::Info,
                AlertKind::Upcoming,
                tr!(
                    "In arrivo: {} da {} il {}.",
                    "Coming up: {}, {} on {}.",
                    s.name,
                    s.amount,
                    s.due.format("%d/%m/%Y")
                ),
            )
        })
}

fn recurring_alert(r: &Recurring) -> Alert {
    let every = match r.cadence {
        Cadence::Week => tr!("a settimana", "a week"),
        Cadence::Month => tr!("al mese", "a month"),
        Cadence::Year => tr!("all'anno", "a year"),
    };
    Alert::new(
        Level::Info,
        AlertKind::Recurring,
        tr!(
            "Spesa ricorrente non pianificata: {} (~{} {}). Aggiungila alle spese previste o valuta se disdirla.",
            "Unplanned recurring expense: {} (~{} {}). Add it to your planned expenses or consider cancelling it.",
            r.merchant,
            r.typical_amount,
            every
        ),
    )
}

fn anomaly_alert(a: &Anomaly) -> Alert {
    Alert::new(
        Level::Warning,
        AlertKind::Anomaly,
        tr!(
            "Spesa insolita il {}: {} {} in {} (di solito ~{}).",
            "Unusual expense on {}: {} {} in {} (usually ~{}).",
            a.date.format("%d/%m"),
            a.merchant,
            a.amount,
            a.category,
            a.typical
        ),
    )
}

/// The most relevant alert for one budget, if any.
fn budget_alert(b: &BudgetStatus, today: NaiveDate) -> Option<Alert> {
    let name = &b.name;
    if let Some(p) = b.problems.first() {
        return Some(Alert::new(
            Level::Warning,
            AlertKind::Budget,
            tr!("Budget «{name}»: {p}.", "Budget “{name}”: {p}."),
        ));
    }
    match b.phase {
        Phase::Upcoming => return None,
        // A finished budget (fixed dates) is summed up for a week, then it goes quiet.
        Phase::Ended if (today - b.end).num_days() <= 7 => {
            let (value, limit) = (b.value, b.limit);
            return Some(Alert::new(
                Level::Info,
                AlertKind::Budget,
                if b.goal == Goal::MaxSpend {
                    tr!(
                        "Budget «{name}» concluso: speso {value} su {limit}.",
                        "Budget “{name}” ended: spent {value} of {limit}."
                    )
                } else {
                    tr!(
                        "Budget «{name}» concluso: raggiunto {value} su {limit}.",
                        "Budget “{name}” ended: reached {value} of {limit}."
                    )
                },
            ));
        }
        Phase::Ended => return None,
        Phase::Active => {}
    }
    let (level, message) = match (b.goal, b.state) {
        (_, State::Over) => (
            Level::Critical,
            tr!(
                "Budget «{name}» sforato: {} su {} ({}).",
                "Budget “{name}” over: {} of {} ({}).",
                b.value,
                b.limit,
                b.period_label
            ),
        ),
        (Goal::MaxSpend, State::Warning) => {
            let text = match b.thresholds_reached.iter().rev().find(|t| **t < 100) {
                Some(t) => tr!("superato l'{t}%", "past {t}%"),
                None => tr!(
                    "{:.0}% usato al {:.0}% del periodo",
                    "{:.0}% used {:.0}% into the period",
                    b.used * 100.0,
                    b.elapsed * 100.0
                ),
            };
            (
                Level::Warning,
                tr!(
                    "Budget «{name}»: {text} ({} su {}, restano {}).",
                    "Budget “{name}”: {text} ({} of {}, {} left).",
                    b.value,
                    b.limit,
                    b.remaining
                ),
            )
        }
        (_, State::Behind) => (
            Level::Warning,
            tr!(
                "Budget «{name}» indietro: {} su {} al {:.0}% del periodo.",
                "Budget “{name}” behind: {} of {} {:.0}% into the period.",
                b.value,
                b.limit,
                b.elapsed * 100.0
            ),
        ),
        _ => return None,
    };
    Some(Alert::new(level, AlertKind::Budget, message))
}

/// Turn a raw sync error into a short explanation for the user.
pub fn explain_sync_error(msg: &str) -> String {
    let m = msg.to_lowercase();
    if m.contains("wrong_transactions_period") {
        tr!(
            "la banca ha rifiutato il periodo di storico richiesto",
            "the bank refused the requested history period"
        )
    } else if m.contains("not active") {
        tr!(
            "l'applicazione Enable Banking non è attiva",
            "the Enable Banking application is not active"
        )
    } else if [" 401 ", " 403 ", " 410 ", "expired"]
        .iter()
        .any(|s| m.contains(s))
    {
        tr!(
            "consenso scaduto o revocato: esegui `fine connect`",
            "consent expired or revoked: run `fine connect`"
        )
    } else if m.contains(" 429 ") || m.contains("rate") {
        tr!(
            "limite di letture giornaliere raggiunto, riprovo al prossimo turno",
            "daily read limit reached, retrying next round"
        )
    } else {
        msg.chars().take(160).collect()
    }
}

fn account_alerts(
    accounts: Vec<AccountHealth>,
    today: NaiveDate,
    now: DateTime<Utc>,
) -> Vec<Alert> {
    let mut out = Vec::new();
    // Failures grouped by reason, so 4 sub-accounts with the same error make one alert.
    let mut failed: Vec<(String, Vec<String>)> = Vec::new();
    let mut never: Vec<String> = Vec::new();
    for a in accounts {
        let name = a.label;
        let consent = a
            .consent_valid_until
            .as_deref()
            .and_then(|v| v.get(..10))
            .and_then(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").ok());
        if let Some(v) = consent {
            let days = (v - today).num_days();
            if days < 0 {
                out.push(Alert::new(
                    Level::Critical,
                    AlertKind::Consent,
                    tr!(
                        "Consenso bancario scaduto per {name}: esegui `fine connect`.",
                        "Bank consent expired for {name}: run `fine connect`."
                    ),
                ));
            } else if days <= 14 {
                out.push(Alert::new(
                    Level::Warning,
                    AlertKind::Consent,
                    tr!(
                        "Il consenso bancario per {name} scade tra {days} giorni: rinnovalo con `fine connect`.",
                        "Bank consent for {name} expires in {days} days: renew it with `fine connect`."
                    ),
                ));
            }
        }
        match (a.last_ok, a.last_success_at) {
            (Some(false), _) => {
                let reason = explain_sync_error(a.last_message.as_deref().unwrap_or("?"));
                match failed.iter_mut().find(|(r, _)| *r == reason) {
                    Some((_, names)) => names.push(name),
                    None => failed.push((reason, vec![name])),
                }
            }
            (None, _) | (_, None) => never.push(name),
            (Some(true), Some(at)) => {
                let hours = DateTime::parse_from_rfc3339(&at)
                    .map(|t| (now - t.with_timezone(&Utc)).num_hours())
                    .unwrap_or(i64::MAX);
                if hours > 24 {
                    out.push(Alert::new(
                        Level::Warning,
                        AlertKind::Stale,
                        tr!(
                            "{name}: ultimo aggiornamento riuscito {hours} ore fa, i dati potrebbero essere incompleti.",
                            "{name}: last successful update {hours} hours ago, the data may be incomplete."
                        ),
                    ));
                }
            }
        }
    }
    for (reason, names) in failed {
        let list = names.join(", ");
        out.push(Alert::new(
            Level::Critical,
            AlertKind::Sync,
            if names.len() == 1 {
                tr!(
                    "Sincronizzazione fallita per {list}: {reason}. I dati di questo conto non sono aggiornati e il disponibile può essere sbagliato.",
                    "Sync failed for {list}: {reason}. This account's data is out of date and what you can spend may be wrong."
                )
            } else {
                tr!(
                    "Sincronizzazione fallita per {list}: {reason}. I dati di questi conti non sono aggiornati e il disponibile può essere sbagliato.",
                    "Sync failed for {list}: {reason}. These accounts' data is out of date and what you can spend may be wrong."
                )
            },
        ));
    }
    if !never.is_empty() {
        out.push(Alert::new(
            Level::Critical,
            AlertKind::Sync,
            tr!(
                "Mai sincronizzati: {}. Esegui `fine sync`.",
                "Never synced: {}. Run `fine sync`.",
                never.join(", ")
            ),
        ));
    }
    out
}

/// Short human summary (terminal / push notification).
pub fn render(s: &Status) -> String {
    let mut out = String::new();
    if s.variable_budget.is_positive() {
        let _ = writeln!(
            out,
            "Disponibile: {} questo mese · {} questa settimana · {} al giorno",
            s.available_month, s.available_week, s.available_per_day
        );
        let _ = writeln!(
            out,
            "Spese variabili: {} su {} ({:.0}% al {:.0}% del mese)",
            s.variable_spent,
            s.variable_budget,
            s.budget_used * 100.0,
            s.month_elapsed * 100.0
        );
    }
    if s.reserved.is_positive() {
        let _ = writeln!(out, "Già riservato ai budget: {}", s.reserved);
    }
    if let Some(b) = s.balance_total {
        let _ = writeln!(out, "Saldo conti: {b}");
    }
    for a in &s.alerts {
        let icon = match a.level {
            Level::Critical => "🔴",
            Level::Warning => "🟠",
            Level::Info => "🔵",
        };
        let _ = writeln!(out, "{icon} {}", a.message);
    }
    if out.is_empty() {
        out.push_str("Tutto in ordine.\n");
    }
    out
}
