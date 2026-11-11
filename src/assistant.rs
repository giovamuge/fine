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
//!   available       = variable budget - variable spent

use anyhow::Result;
use chrono::{Days, NaiveDate, Utc};
use rusqlite::Connection;
use serde::Serialize;

use crate::analysis::{self, Cadence, Kind};
use crate::db;
use crate::money::{eur, fmt, fmt_eur};

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
    /// What you can still spend this month on variable expenses.
    pub available_month: f64,
    /// Suggested daily allowance for the rest of the month.
    pub available_per_day: f64,
    /// Allowance for the rest of this week (bounded by month end).
    pub available_week: f64,
    pub spent_this_week: f64,
    pub month_elapsed: f64,
    pub budget_used: f64,
    pub budgets: Vec<analysis::BudgetStatus>,
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
    let available = variable_budget - variable_spent;

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
            message: "Nessuna entrata nota: aggiungi lo stipendio come entrata prevista (upsert_planned) per calcolare il disponibile.".into(),
        });
    } else if available < 0 {
        alerts.push(Alert {
            level: Level::Critical,
            message: format!(
                "Hai superato il budget variabile del mese di {}.",
                fmt(-available)
            ),
        });
    } else if used > elapsed + 0.15 && used > 0.3 {
        alerts.push(Alert {
            level: Level::Warning,
            message: format!(
                "Stai spendendo più velocemente del previsto: {:.0}% del budget variabile usato al {:.0}% del mese.",
                used * 100.0,
                elapsed * 100.0
            ),
        });
    }
    let budgets = analysis::budget_statuses(conn, today)?;
    for b in &budgets {
        let period = match b.period.as_str() {
            "week" => "settimana",
            "year" => "anno",
            _ => "mese",
        };
        match b.state {
            "sforato" => alerts.push(Alert {
                level: Level::Critical,
                message: format!(
                    "{}: budget della {period} sforato ({} su {}).",
                    b.category,
                    fmt_eur(b.spent),
                    fmt_eur(b.limit)
                ),
            }),
            "attenzione" => alerts.push(Alert {
                level: Level::Warning,
                message: format!(
                    "{}: {:.0}% del budget della {period} usato al {:.0}% del periodo.",
                    b.category,
                    b.used * 100.0,
                    b.elapsed * 100.0
                ),
            }),
            _ => {}
        }
    }
    for s in analysis::sinking_funds(&planned, today) {
        if (s.due - today).num_days() <= 30 {
            alerts.push(Alert {
                level: Level::Info,
                message: format!(
                    "In arrivo: {} da {} il {}.",
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
        alerts.push(Alert {
            level: Level::Info,
            message: format!(
                "Spesa ricorrente non pianificata: {} (~{} {}). Aggiungila alle spese previste o valuta se disdirla.",
                r.merchant,
                fmt_eur(r.typical_amount),
                match r.cadence {
                    Cadence::Week => "a settimana",
                    Cadence::Month => "al mese",
                    Cadence::Year => "all'anno",
                }
            ),
        });
    }
    for a in analysis::anomalies(conn, today)? {
        alerts.push(Alert {
            level: Level::Warning,
            message: format!(
                "Spesa insolita il {}: {} {} in {} (di solito ~{}).",
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
            message: format!("{uncategorized} {} da categorizzare (chiedi all'AI: \"categorizza le transazioni in sospeso\").", if uncategorized == 1 { "transazione" } else { "transazioni" }),
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

/// Turn a raw sync error into a short explanation for the user.
pub fn explain_sync_error(msg: &str) -> String {
    let m = msg.to_lowercase();
    if m.contains("wrong_transactions_period") {
        "la banca ha rifiutato il periodo di storico richiesto".into()
    } else if m.contains("not active") {
        "l'applicazione Enable Banking non è attiva".into()
    } else if m.contains(" 401 ")
        || m.contains(" 403 ")
        || m.contains(" 410 ")
        || m.contains("expired")
    {
        "consenso scaduto o revocato: esegui `fine connect`".into()
    } else if m.contains(" 429 ") || m.contains("rate") {
        "limite di letture giornaliere raggiunto, riprovo al prossimo turno".into()
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
                    message: format!(
                        "Consenso bancario scaduto per {name}: esegui `fine connect`."
                    ),
                });
            } else if days <= 14 {
                out.push(Alert {
                    level: Level::Warning,
                    message: format!("Il consenso bancario per {name} scade tra {days} giorni: rinnovalo con `fine connect`."),
                });
            }
        }
        match (last_ok, last_success) {
            (Some(0), _) => {
                let reason =
                    explain_sync_error(last_msg.as_deref().unwrap_or("errore sconosciuto"));
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
                        message: format!("{name}: ultimo aggiornamento riuscito {} ore fa, i dati potrebbero essere incompleti.", hours),
                    });
                }
            }
        }
    }
    for (reason, names) in failed {
        out.push(Alert {
            level: Level::Critical,
            message: format!(
                "Sincronizzazione fallita per {}: {reason}. I dati di {} non sono aggiornati e il disponibile può essere sbagliato.",
                names.join(", "),
                if names.len() == 1 { "questo conto" } else { "questi conti" }
            ),
        });
    }
    if !never.is_empty() {
        out.push(Alert {
            level: Level::Critical,
            message: format!(
                "Mai sincronizzati: {}. Esegui `fine sync`.",
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
}
