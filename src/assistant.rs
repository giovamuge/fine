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

use anyhow::Result;
use chrono::{Days, NaiveDate, Utc};
use rusqlite::Connection;
use serde::Serialize;

use crate::analysis::{self, Cadence, Kind};
use crate::budgets::{self, Goal, Phase, State};
use crate::db;
use crate::money::{eur, fmt, fmt_eur};
use crate::tr;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Serialize)]
pub struct Alert {
    pub level: Level,
    /// What the alert is about, for the dashboard to link the right page:
    /// income, overspend, pace, budget, upcoming, recurring, anomaly, uncategorized, consent, stale, sync.
    pub kind: &'static str,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub today: NaiveDate,
    pub month: String,
    pub days_left_in_month: i64,
    pub balance_total: Option<f64>,
    pub income_received: f64,
    pub income_expected_remaining: f64,
    /// "planned" or "average_3_months"
    pub income_source: &'static str,
    pub fixed_paid: f64,
    pub fixed_still_due: f64,
    pub sinking_fund_quota: f64,
    pub savings_goal: f64,
    pub variable_budget: f64,
    pub variable_spent: f64,
    /// Set aside for budgets marked `reserve` (their unspent part falling in this month).
    pub reserved: f64,
    /// What you can still spend this month on variable expenses.
    pub available_month: f64,
    /// Suggested daily allowance for the rest of the month.
    pub available_per_day: f64,
    /// Allowance for the rest of this week (bounded by month end).
    pub available_week: f64,
    pub spent_this_week: f64,
    pub month_elapsed: f64,
    pub budget_used: f64,
    pub budgets: Vec<budgets::BudgetStatus>,
    pub uncategorized: i64,
    pub alerts: Vec<Alert>,
}

pub fn status(conn: &Connection, today: NaiveDate) -> Result<Status> {
    let (m_start, m_end) = (analysis::month_start(today), analysis::month_end(today));
    let txs = analysis::load(conn, m_start, today)?;
    let planned = analysis::load_planned(conn)?;
    let mut alerts = Vec::new();

    // Income
    let received: i64 = txs
        .iter()
        .filter(|t| t.kind == Kind::Income && t.date >= m_start)
        .map(|t| t.cents)
        .sum();
    let planned_income: Vec<_> = planned.iter().filter(|p| p.kind == Kind::Income).collect();
    let (expected_remaining, income_source) = if planned_income.is_empty() {
        let avg = analysis::average_income(conn, today)?;
        ((avg - received).max(0), "average_3_months")
    } else {
        let rem = planned_income
            .iter()
            .map(|p| {
                let expected = p.expected_between(m_start, m_end);
                let got: i64 = txs
                    .iter()
                    .filter(|t| t.date >= m_start && p.matches(t))
                    .map(|t| t.cents)
                    .sum();
                (expected - got).max(0)
            })
            .sum();
        (rem, "planned")
    };
    let income = received + expected_remaining;

    // Fixed planned expenses (weekly/monthly)
    let mut fixed_paid = 0;
    let mut fixed_due = 0;
    for p in planned
        .iter()
        .filter(|p| p.kind == Kind::Expense && p.cadence != Cadence::Year)
    {
        let expected = p.expected_between(m_start, m_end);
        let paid: i64 = txs
            .iter()
            .filter(|t| t.date >= m_start && p.matches(t))
            .map(|t| -t.cents)
            .sum();
        fixed_paid += paid;
        fixed_due += (expected - paid).max(0);
    }
    let sinking: i64 = planned
        .iter()
        .filter(|p| p.kind == Kind::Expense && p.cadence == Cadence::Year)
        .map(|p| p.amount_cents / 12)
        .sum();
    let goal = db::savings_goal_cents(conn)?;

    let entries = analysis::expense_entries(&txs, &planned, m_start, today);
    let variable_spent: i64 = entries.iter().filter(|e| !e.fixed).map(|e| e.spent).sum();
    let variable_budget = income - fixed_paid - fixed_due - sinking - goal;
    let budgets = budgets::statuses(conn, today)?;
    let reserved: i64 = budgets
        .iter()
        .map(|b| (b.reserved * 100.0).round() as i64)
        .sum();
    let available = variable_budget - variable_spent - reserved;

    let days_left = (m_end - today).num_days() + 1;
    let per_day = if available > 0 {
        available / days_left
    } else {
        0
    };
    let w_start = analysis::week_start(today);
    let w_end = (w_start + Days::new(6)).min(m_end);
    let week_days_left = (w_end - today).num_days() + 1;
    let spent_week: i64 = analysis::expense_entries(
        &analysis::load(conn, w_start, today)?,
        &planned,
        w_start,
        today,
    )
    .iter()
    .filter(|e| !e.fixed)
    .map(|e| e.spent)
    .sum();
    let elapsed =
        ((today - m_start).num_days() + 1) as f64 / ((m_end - m_start).num_days() + 1) as f64;
    let used = if variable_budget > 0 {
        variable_spent as f64 / variable_budget as f64
    } else {
        0.0
    };

    // Alerts
    if income == 0 {
        alerts.push(Alert {
            level: Level::Info,
            kind: "income",
            message: tr!(
                "Nessuna entrata nota: aggiungi lo stipendio come entrata prevista (upsert_planned) per calcolare il disponibile.",
                "No known income: add your salary as expected income (upsert_planned) to work out what you can spend."
            ),
        });
    } else if available < 0 {
        alerts.push(Alert {
            level: Level::Critical,
            kind: "overspend",
            message: tr!(
                "Hai superato il budget variabile del mese di {}.",
                "You're {} over this month's variable budget.",
                fmt(-available)
            ),
        });
    } else if used > elapsed + 0.15 && used > 0.3 {
        alerts.push(Alert {
            level: Level::Warning,
            kind: "pace",
            message: tr!(
                "Stai spendendo più velocemente del previsto: {:.0}% del budget variabile usato al {:.0}% del mese.",
                "You're spending faster than planned: {:.0}% of the variable budget used {:.0}% into the month.",
                used * 100.0,
                elapsed * 100.0
            ),
        });
    }
    for b in budgets.iter().filter(|b| b.budget.enabled) {
        alerts.extend(budget_alert(b, today));
    }
    for s in analysis::sinking_funds(&planned, today) {
        if (s.due - today).num_days() <= 30 {
            alerts.push(Alert {
                level: Level::Info,
                kind: "upcoming",
                message: tr!(
                    "In arrivo: {} da {} il {}.",
                    "Coming up: {}, {} on {}.",
                    s.name,
                    fmt_eur(s.amount),
                    s.due.format("%d/%m/%Y")
                ),
            });
        }
    }
    for r in analysis::recurring(conn, today)?
        .into_iter()
        .filter(|r| r.active && !r.planned)
    {
        let every = match r.cadence {
            Cadence::Week => tr!("a settimana", "a week"),
            Cadence::Month => tr!("al mese", "a month"),
            Cadence::Year => tr!("all'anno", "a year"),
        };
        alerts.push(Alert {
            level: Level::Info,
            kind: "recurring",
            message: tr!(
                "Spesa ricorrente non pianificata: {} (~{} {}). Aggiungila alle spese previste o valuta se disdirla.",
                "Unplanned recurring expense: {} (~{} {}). Add it to your planned expenses or consider cancelling it.",
                r.merchant,
                fmt_eur(r.typical_amount),
                every
            ),
        });
    }
    for a in analysis::anomalies(conn, today)? {
        alerts.push(Alert {
            level: Level::Warning,
            kind: "anomaly",
            message: tr!(
                "Spesa insolita il {}: {} {} in {} (di solito ~{}).",
                "Unusual expense on {}: {} {} in {} (usually ~{}).",
                a.date.format("%d/%m"),
                a.merchant,
                fmt_eur(a.amount),
                a.category,
                fmt_eur(a.typical)
            ),
        });
    }
    let uncategorized: i64 = conn.query_row(
        "SELECT COUNT(*) FROM transactions WHERE category_id IS NULL",
        [],
        |r| r.get(0),
    )?;
    if uncategorized > 0 {
        alerts.push(Alert {
            level: Level::Info,
            kind: "uncategorized",
            message: if uncategorized == 1 {
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
        });
    }
    alerts.extend(account_alerts(conn, today)?);
    alerts.sort_by_key(|a| std::cmp::Reverse(a.level));

    let balance: Option<i64> = conn.query_row(
        "SELECT SUM(balance_cents) FROM accounts WHERE COALESCE(balance_currency, currency) = ?1",
        [db::BASE_CURRENCY],
        |r| r.get(0),
    )?;
    Ok(Status {
        today,
        month: m_start.format("%Y-%m").to_string(),
        days_left_in_month: days_left,
        balance_total: balance.map(eur),
        income_received: eur(received),
        income_expected_remaining: eur(expected_remaining),
        income_source,
        fixed_paid: eur(fixed_paid),
        fixed_still_due: eur(fixed_due),
        sinking_fund_quota: eur(sinking),
        savings_goal: eur(goal),
        variable_budget: eur(variable_budget),
        variable_spent: eur(variable_spent),
        reserved: eur(reserved),
        available_month: eur(available),
        available_per_day: eur(per_day),
        available_week: eur(per_day * week_days_left),
        spent_this_week: eur(spent_week),
        month_elapsed: (elapsed * 1000.0).round() / 1000.0,
        budget_used: (used * 1000.0).round() / 1000.0,
        budgets,
        uncategorized,
        alerts,
    })
}

/// The most relevant alert for one budget, if any.
fn budget_alert(b: &budgets::BudgetStatus, today: NaiveDate) -> Option<Alert> {
    let name = &b.name;
    if let Some(p) = b.problems.first() {
        return Some(Alert {
            level: Level::Warning,
            kind: "budget",
            message: tr!("Budget «{name}»: {p}.", "Budget “{name}”: {p}."),
        });
    }
    match b.phase {
        Phase::Upcoming => return None,
        // A finished budget (fixed dates) is summed up for a week, then it goes quiet.
        Phase::Ended if (today - b.end).num_days() <= 7 => {
            let (value, limit) = (fmt_eur(b.value), fmt_eur(b.limit));
            return Some(Alert {
                level: Level::Info,
                kind: "budget",
                message: if b.goal == Goal::MaxSpend {
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
            });
        }
        Phase::Ended => return None,
        Phase::Active => {}
    }
    let (level, message) = match (b.goal, b.state) {
        (_, State::Sforato) => (
            Level::Critical,
            tr!(
                "Budget «{name}» sforato: {} su {} ({}).",
                "Budget “{name}” over: {} of {} ({}).",
                fmt_eur(b.value),
                fmt_eur(b.limit),
                b.period_label
            ),
        ),
        (Goal::MaxSpend, State::Attenzione) => {
            let threshold = b.thresholds_reached.iter().rev().find(|t| **t < 100);
            let text = match threshold {
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
                    fmt_eur(b.value),
                    fmt_eur(b.limit),
                    fmt_eur(b.remaining)
                ),
            )
        }
        (_, State::Indietro) => (
            Level::Warning,
            tr!(
                "Budget «{name}» indietro: {} su {} al {:.0}% del periodo.",
                "Budget “{name}” behind: {} of {} {:.0}% into the period.",
                fmt_eur(b.value),
                fmt_eur(b.limit),
                b.elapsed * 100.0
            ),
        ),
        _ => return None,
    };
    Some(Alert {
        level,
        kind: "budget",
        message,
    })
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
    } else if m.contains(" 401 ")
        || m.contains(" 403 ")
        || m.contains(" 410 ")
        || m.contains("expired")
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

fn account_alerts(conn: &Connection, today: NaiveDate) -> Result<Vec<Alert>> {
    let mut out = Vec::new();
    let sql = format!(
        "SELECT {label}, a.valid_until,
            (SELECT ok FROM sync_log s WHERE s.account_uid = a.uid ORDER BY s.id DESC LIMIT 1),
            (SELECT message FROM sync_log s WHERE s.account_uid = a.uid ORDER BY s.id DESC LIMIT 1),
            (SELECT MAX(at) FROM sync_log s WHERE s.account_uid = a.uid AND s.ok = 1)
         FROM accounts a ORDER BY a.created_at, a.uid",
        label = db::ACCOUNT_LABEL
    );
    type Row = (
        String,
        Option<String>,
        Option<i64>,
        Option<String>,
        Option<String>,
    );
    let rows: Vec<Row> = conn
        .prepare(&sql)?
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<Result<_, _>>()?;
    // Failures grouped by reason, so 4 sub-accounts with the same error make one alert.
    let mut failed: Vec<(String, Vec<String>)> = Vec::new();
    let mut never: Vec<String> = Vec::new();
    for (name, valid_until, last_ok, last_msg, last_success) in rows {
        if let Some(v) = valid_until
            .as_deref()
            .and_then(|v| v.get(..10))
            .and_then(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
        {
            let days = (v - today).num_days();
            if days < 0 {
                out.push(Alert {
                    level: Level::Critical,
                    kind: "consent",
                    message: tr!(
                        "Consenso bancario scaduto per {name}: esegui `fine connect`.",
                        "Bank consent expired for {name}: run `fine connect`."
                    ),
                });
            } else if days <= 14 {
                out.push(Alert {
                    level: Level::Warning,
                    kind: "consent",
                    message: tr!(
                        "Il consenso bancario per {name} scade tra {days} giorni: rinnovalo con `fine connect`.",
                        "Bank consent for {name} expires in {days} days: renew it with `fine connect`."
                    ),
                });
            }
        }
        match (last_ok, last_success) {
            (Some(0), _) => {
                let reason = explain_sync_error(last_msg.as_deref().unwrap_or("?"));
                match failed.iter_mut().find(|(r, _)| *r == reason) {
                    Some((_, names)) => names.push(name),
                    None => failed.push((reason, vec![name])),
                }
            }
            (None, _) | (_, None) => never.push(name),
            (Some(_), Some(at)) => {
                let hours = chrono::DateTime::parse_from_rfc3339(&at)
                    .map(|t| (Utc::now() - t.with_timezone(&Utc)).num_hours())
                    .unwrap_or(i64::MAX);
                if hours > 24 {
                    out.push(Alert {
                        level: Level::Warning,
                        kind: "stale",
                        message: tr!(
                            "{name}: ultimo aggiornamento riuscito {hours} ore fa, i dati potrebbero essere incompleti.",
                            "{name}: last successful update {hours} hours ago, the data may be incomplete."
                        ),
                    });
                }
            }
        }
    }
    for (reason, names) in failed {
        let list = names.join(", ");
        out.push(Alert {
            level: Level::Critical,
            kind: "sync",
            message: if names.len() == 1 {
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
        });
    }
    if !never.is_empty() {
        out.push(Alert {
            level: Level::Critical,
            kind: "sync",
            message: tr!(
                "Mai sincronizzati: {}. Esegui `fine sync`.",
                "Never synced: {}. Run `fine sync`.",
                never.join(", ")
            ),
        });
    }
    Ok(out)
}

/// Short human summary (terminal / push notification).
pub fn render(s: &Status) -> String {
    let mut out = String::new();
    if s.variable_budget > 0.0 {
        out.push_str(&format!(
            "Disponibile: {} questo mese · {} questa settimana · {} al giorno\n",
            fmt((s.available_month * 100.0).round() as i64),
            fmt((s.available_week * 100.0).round() as i64),
            fmt((s.available_per_day * 100.0).round() as i64),
        ));
        out.push_str(&format!(
            "Spese variabili: {} su {} ({:.0}% al {:.0}% del mese)\n",
            fmt((s.variable_spent * 100.0).round() as i64),
            fmt((s.variable_budget * 100.0).round() as i64),
            s.budget_used * 100.0,
            s.month_elapsed * 100.0
        ));
    }
    if s.reserved > 0.0 {
        out.push_str(&format!(
            "Già riservato ai budget: {}\n",
            fmt((s.reserved * 100.0).round() as i64)
        ));
    }
    if let Some(b) = s.balance_total {
        out.push_str(&format!(
            "Saldo conti: {}\n",
            fmt((b * 100.0).round() as i64)
        ));
    }
    for a in &s.alerts {
        let icon = match a.level {
            Level::Critical => "🔴",
            Level::Warning => "🟠",
            Level::Info => "🔵",
        };
        out.push_str(&format!("{icon} {}\n", a.message));
    }
    if out.is_empty() {
        out.push_str("Tutto in ordine.\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tests::{add_tx, d};
    use rusqlite::params;

    #[test]
    fn computes_available_with_fixed_annual_and_goal() {
        let conn = db::open_in_memory().unwrap();
        let stip = db::category_id(&conn, "Stipendio").unwrap();
        let casa = db::category_id(&conn, "Casa").unwrap();
        let ass = db::category_id(&conn, "Assicurazioni").unwrap();
        conn.execute("INSERT INTO planned (name, kind, amount_cents, cadence, next_due, category_id, pattern) VALUES ('Stipendio','income',200000,'month','2026-11-27',?1,'acme')", params![stip]).unwrap();
        conn.execute("INSERT INTO planned (name, kind, amount_cents, cadence, next_due, category_id, pattern) VALUES ('Affitto','expense',70000,'month','2026-11-05',?1,'immobiliare')", params![casa]).unwrap();
        conn.execute("INSERT INTO planned (name, kind, amount_cents, cadence, next_due, category_id, pattern) VALUES ('RC auto','expense',60000,'year','2027-03-01',?1,'assicura')", params![ass]).unwrap();
        db::set_setting(&conn, "savings_goal_cents", "30000").unwrap();

        add_tx(
            &conn,
            "2026-11-05",
            -70000,
            "immobiliare rossi",
            Some("Casa"),
        );
        add_tx(
            &conn,
            "2026-11-08",
            -10000,
            "esselunga",
            Some("Spesa alimentare"),
        );

        let s = status(&conn, d("2026-11-10")).unwrap();
        // income 2000 expected; committed: rent 700 + RC 50 + goal 300 = 1050 -> variable budget 950
        assert_eq!(s.income_source, "planned");
        assert_eq!(s.variable_budget, 950.0);
        assert_eq!(s.fixed_paid, 700.0);
        assert_eq!(s.variable_spent, 100.0);
        assert_eq!(s.available_month, 850.0);
        // 21 days left (10..30) -> 40.47/day
        assert_eq!(s.days_left_in_month, 21);
        assert_eq!(s.available_per_day, 40.47);
    }

    #[test]
    fn failed_sync_is_a_critical_alert_never_silent() {
        let conn = db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO accounts (uid, currency, name, aspsp) VALUES ('f1','EUR','MARIO','FinecoBank'), ('f2','USD','MARIO','FinecoBank'), ('p','EUR','Mario','PayPal')",
            [],
        )
        .unwrap();
        let now = Utc::now().to_rfc3339();
        let err = "Enable Banking 422: {\"error\":\"WRONG_TRANSACTIONS_PERIOD\"}";
        for uid in ["f1", "f2"] {
            conn.execute(
                "INSERT INTO sync_log (account_uid, at, ok, message) VALUES (?1, ?2, 0, ?3)",
                params![uid, now, err],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO sync_log (account_uid, at, ok) VALUES ('p', ?1, 1)",
            [&now],
        )
        .unwrap();

        let s = status(&conn, d("2026-11-10")).unwrap();
        let critical: Vec<&str> = s
            .alerts
            .iter()
            .filter(|a| a.level == Level::Critical)
            .map(|a| a.message.as_str())
            .collect();
        assert_eq!(critical.len(), 1, "same reason grouped: {critical:?}");
        assert!(
            critical[0].contains("FinecoBank · MARIO (EUR)")
                && critical[0].contains("FinecoBank · MARIO (USD)")
        );
        assert!(critical[0].contains("periodo di storico"));
        assert!(!critical[0].contains("PayPal"));
        assert!(
            render(&s).contains("🔴"),
            "the summary must show the failure"
        );
    }

    #[test]
    fn annual_payment_does_not_hit_monthly_budget() {
        let conn = db::open_in_memory().unwrap();
        conn.execute("INSERT INTO planned (name, kind, amount_cents, cadence, next_due, pattern) VALUES ('RC auto','expense',60000,'year','2026-11-03','assicura')", []).unwrap();
        add_tx(
            &conn,
            "2026-11-03",
            -60000,
            "assicura spa",
            Some("Assicurazioni"),
        );
        let s = status(&conn, d("2026-11-10")).unwrap();
        assert_eq!(s.variable_spent, 0.0);
        assert_eq!(s.sinking_fund_quota, 50.0);
    }

    #[test]
    fn reserved_budgets_reduce_available_and_alert_once_over() {
        let conn = db::open_in_memory().unwrap();
        db::set_setting(&conn, "savings_goal_cents", "0").unwrap();
        let stip = db::category_id(&conn, "Stipendio").unwrap();
        conn.execute("INSERT INTO planned (name, kind, amount_cents, cadence, next_due, category_id) VALUES ('Stipendio','income',200000,'month','2026-11-27',?1)", params![stip]).unwrap();
        let before = status(&conn, d("2026-11-10")).unwrap().available_month;
        let trip: budgets::Budget = serde_json::from_value(serde_json::json!({
            "name": "Viaggio", "amount": 300, "reserve": true,
            "period": {"type": "custom", "from": "2026-11-20", "to": "2026-11-29"}
        }))
        .unwrap();
        budgets::upsert(&conn, trip, None).unwrap();
        let s = status(&conn, d("2026-11-10")).unwrap();
        assert_eq!(s.reserved, 300.0);
        assert_eq!(s.available_month, before - 300.0);

        let svago: budgets::Budget = serde_json::from_value(serde_json::json!({
            "name": "Svago", "amount": 50, "period": {"type": "month"},
            "rule": {"category": ["Svago"]}
        }))
        .unwrap();
        budgets::upsert(&conn, svago, None).unwrap();
        add_tx(&conn, "2026-11-03", -6000, "cinema", Some("Svago"));
        let s = status(&conn, d("2026-11-10")).unwrap();
        let over: Vec<_> = s
            .alerts
            .iter()
            .filter(|a| a.message.contains("«Svago» sforato"))
            .collect();
        assert_eq!(over.len(), 1);
        assert_eq!(over[0].level, Level::Critical);
    }
}
