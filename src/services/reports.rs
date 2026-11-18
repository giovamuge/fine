//! Read-only reports: the assistant's status, summaries, trends, recurring expenses, balances.

use chrono::{Days, NaiveDate, Utc};

use crate::domain::analysis::{
    self, MonthSummary, RECURRING_LOOKBACK_DAYS, Recurring, SinkingFund, TrendPoint,
};
use crate::domain::assistant::{self, Inputs, Status};
use crate::domain::balance::{self, BalanceHistory};
use crate::domain::budget::{Evaluator, earliest_needed};
use crate::domain::calendar::{month_end, month_start, week_start};
use crate::domain::transaction::window;
use crate::error::Result;
use crate::storage::{AccountView, Store};

pub fn accounts(store: &Store) -> Result<Vec<AccountView>> {
    store.db().list_accounts()
}

/// Summary of the month containing `day`.
pub fn monthly_summary(store: &Store, day: NaiveDate) -> Result<MonthSummary> {
    let db = store.db();
    let txs = db.load_txs(month_start(day), month_end(day))?;
    Ok(analysis::monthly_summary(&txs, &db.load_planned()?, day))
}

/// Income and expenses of the last `months` months (including the current one).
pub fn trend(store: &Store, today: NaiveDate, months: u32) -> Result<Vec<TrendPoint>> {
    let db = store.db();
    let txs = db.load_txs(analysis::trend_start(today, months), month_end(today))?;
    Ok(analysis::trend(&txs, &db.load_planned()?, today, months))
}

fn recurring_since(today: NaiveDate) -> NaiveDate {
    today - Days::new(RECURRING_LOOKBACK_DAYS)
}

/// Recurring expenses (subscriptions, bills) found in the last ~13 months.
pub fn recurring(store: &Store, today: NaiveDate) -> Result<Vec<Recurring>> {
    let db = store.db();
    let txs = db.load_txs(recurring_since(today), today)?;
    Ok(analysis::recurring(&txs, &db.load_planned()?, today))
}

pub fn sinking_funds(store: &Store, today: NaiveDate) -> Result<Vec<SinkingFund>> {
    Ok(analysis::sinking_funds(&store.db().load_planned()?, today))
}

/// Total balance over the last `days` days (7 to 3650).
pub fn balance_history(store: &Store, today: NaiveDate, days: u64) -> Result<BalanceHistory> {
    Ok(balance::history(
        store.db().account_ledgers()?,
        today,
        days.clamp(7, 3650),
    ))
}

/// "How much can I still spend?": the month's numbers, every budget and the alerts.
pub fn status(store: &Store, today: NaiveDate) -> Result<Status> {
    let db = store.db();
    let planned = db.load_planned()?;
    let budgets = db.load_budgets()?;

    // One load covers every window the status looks at.
    let budgets_from = earliest_needed(budgets.iter().map(|s| &s.budget), today)?;
    let anomalies_from = analysis::anomalies_window(today);
    let (avg_from, avg_to) = analysis::average_income_window(today);
    let from = [
        recurring_since(today),
        budgets_from,
        anomalies_from,
        avg_from,
        week_start(today),
    ]
    .into_iter()
    .min()
    .expect("non-empty");
    let all = db.load_txs(from, today)?;

    let evaluator = Evaluator::new(
        window(&all, budgets_from, today),
        planned.clone(),
        db.category_names()?,
        today,
    );
    let budget_statuses = budgets
        .iter()
        .map(|s| evaluator.evaluate(s.id, &s.budget, s.created, false))
        .collect::<Result<Vec<_>>>()?;
    let average_income = if Inputs::needs_average_income(&planned) {
        analysis::average_income(&window(&all, avg_from, avg_to), today)
    } else {
        Default::default()
    };
    let inputs = Inputs {
        month: window(&all, month_start(today), today),
        week: window(&all, week_start(today), today),
        recurring: analysis::recurring(
            &window(&all, recurring_since(today), today),
            &planned,
            today,
        ),
        anomalies: analysis::anomalies(&window(&all, anomalies_from, today), today),
        planned,
        average_income,
        savings_goal: db.savings_goal()?,
        budgets: budget_statuses,
        uncategorized: db.uncategorized_count()?,
        unconverted: db.unconverted_count(from, today)?,
        accounts: db.account_health()?,
        balance_total: db.base_balance_total()?,
    };
    Ok(assistant::status(inputs, today, Utc::now()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Money;
    use crate::domain::assistant::{AlertKind, IncomeSource, Level};
    use crate::domain::testing::d;
    use crate::services::budgets;
    use crate::services::testing::add_tx;
    use rusqlite::params;

    fn eur(m: Money) -> f64 {
        m.to_eur()
    }

    fn plan(store: &Store, sql: &str, category: Option<&str>) {
        let cat = category.map(|c| store.db().category_id(c).unwrap());
        store.conn().execute(sql, params![cat]).unwrap();
    }

    #[test]
    fn computes_available_with_fixed_annual_and_goal() {
        let store = Store::open_in_memory().unwrap();
        plan(
            &store,
            "INSERT INTO planned (name, kind, amount_cents, cadence, next_due, category_id, pattern) VALUES ('Stipendio','income',200000,'month','2026-11-27',?1,'acme')",
            Some("Stipendio"),
        );
        plan(
            &store,
            "INSERT INTO planned (name, kind, amount_cents, cadence, next_due, category_id, pattern) VALUES ('Affitto','expense',70000,'month','2026-11-05',?1,'immobiliare')",
            Some("Casa"),
        );
        plan(
            &store,
            "INSERT INTO planned (name, kind, amount_cents, cadence, next_due, category_id, pattern) VALUES ('RC auto','expense',60000,'year','2027-03-01',?1,'assicura')",
            Some("Assicurazioni"),
        );
        store
            .db()
            .set_savings_goal(Money::from_cents(30000))
            .unwrap();
        add_tx(
            &store,
            "2026-11-05",
            -70000,
            "immobiliare rossi",
            Some("Casa"),
        );
        add_tx(
            &store,
            "2026-11-08",
            -10000,
            "esselunga",
            Some("Spesa alimentare"),
        );

        let s = status(&store, d("2026-11-10")).unwrap();
        // income 2000 expected; committed: rent 700 + RC 50 + goal 300 = 1050 -> variable budget 950
        assert_eq!(s.income_source, IncomeSource::Planned);
        assert_eq!(eur(s.variable_budget), 950.0);
        assert_eq!(eur(s.fixed_paid), 700.0);
        assert_eq!(eur(s.variable_spent), 100.0);
        assert_eq!(eur(s.available_month), 850.0);
        // 21 days left (10..30) -> 40.47/day
        assert_eq!(s.days_left_in_month, 21);
        assert_eq!(eur(s.available_per_day), 40.47);
    }

    #[test]
    fn failed_sync_is_a_critical_alert_never_silent() {
        let store = Store::open_in_memory().unwrap();
        store.conn().execute(
            "INSERT INTO accounts (uid, currency, name, aspsp) VALUES ('f1','EUR','MARIO','FinecoBank'), ('f2','USD','MARIO','FinecoBank'), ('p','EUR','Mario','PayPal')",
            [],
        )
        .unwrap();
        let err = "Enable Banking 422: {\"error\":\"WRONG_TRANSACTIONS_PERIOD\"}";
        for uid in ["f1", "f2"] {
            store.db().log_sync(uid, Some(err)).unwrap();
        }
        store.db().log_sync("p", None).unwrap();

        let s = status(&store, d("2026-11-10")).unwrap();
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
            assistant::render(&s).contains("🔴"),
            "the summary must show the failure"
        );
    }

    #[test]
    fn annual_payment_does_not_hit_monthly_budget() {
        let store = Store::open_in_memory().unwrap();
        plan(
            &store,
            "INSERT INTO planned (name, kind, amount_cents, cadence, next_due, pattern, category_id) VALUES ('RC auto','expense',60000,'year','2026-11-03','assicura', ?1)",
            None,
        );
        add_tx(
            &store,
            "2026-11-03",
            -60000,
            "assicura spa",
            Some("Assicurazioni"),
        );
        let s = status(&store, d("2026-11-10")).unwrap();
        assert_eq!(s.variable_spent, Money::ZERO);
        assert_eq!(eur(s.sinking_fund_quota), 50.0);
    }

    #[test]
    fn reserved_budgets_reduce_available_and_alert_once_over() {
        let mut store = Store::open_in_memory().unwrap();
        plan(
            &store,
            "INSERT INTO planned (name, kind, amount_cents, cadence, next_due, category_id) VALUES ('Stipendio','income',200000,'month','2026-11-27',?1)",
            Some("Stipendio"),
        );
        let today = d("2026-11-10");
        let before = status(&store, today).unwrap().available_month;
        let trip = serde_json::from_value(serde_json::json!({
            "name": "Viaggio", "amount": 300, "reserve": true,
            "period": {"type": "custom", "from": "2026-11-20", "to": "2026-11-29"}
        }))
        .unwrap();
        budgets::upsert(&mut store, trip, None).unwrap();
        let s = status(&store, today).unwrap();
        assert_eq!(eur(s.reserved), 300.0);
        assert_eq!(s.available_month, before - Money::from_cents(30000));

        let svago = serde_json::from_value(serde_json::json!({
            "name": "Svago", "amount": 50, "period": {"type": "month"},
            "rule": {"category": ["Svago"]}
        }))
        .unwrap();
        budgets::upsert(&mut store, svago, None).unwrap();
        add_tx(&store, "2026-11-03", -6000, "cinema", Some("Svago"));
        let s = status(&store, today).unwrap();
        let over: Vec<_> = s
            .alerts
            .iter()
            .filter(|a| a.message.contains("«Svago» sforato"))
            .collect();
        assert_eq!(over.len(), 1);
        assert_eq!(over[0].level, Level::Critical);
    }

    #[test]
    fn foreign_currency_counts_at_the_ecb_rate_once_known() {
        let store = Store::open_in_memory().unwrap();
        add_tx(
            &store,
            "2026-11-02",
            -5000,
            "esselunga",
            Some("Spesa alimentare"),
        );
        let hotel = add_tx(&store, "2026-11-04", -8500, "hotel london", Some("Viaggi"));
        store
            .conn()
            .execute(
                "UPDATE transactions SET currency = 'GBP' WHERE id = ?1",
                [hotel],
            )
            .unwrap();
        let today = d("2026-11-10");
        // Without a rate it is left out, and the status says so.
        assert_eq!(eur(monthly_summary(&store, today).unwrap().expenses), 50.0);
        let s = status(&store, today).unwrap();
        assert!(s.alerts.iter().any(|a| a.kind == AlertKind::Currency));
        // With Monday's rate (2026-11-02) it counts, converted.
        store
            .conn()
            .execute(
                "INSERT INTO fx_rates VALUES ('GBP', '2026-11-02', 0.85)",
                [],
            )
            .unwrap();
        let m = monthly_summary(&store, today).unwrap();
        assert_eq!(eur(m.expenses), 150.0);
        let s = status(&store, today).unwrap();
        assert!(!s.alerts.iter().any(|a| a.kind == AlertKind::Currency));
        let found = store.db().search(&Default::default()).unwrap();
        let tx = found.iter().find(|t| t.id == hotel).unwrap();
        assert_eq!((tx.amount.cents(), tx.currency.as_str()), (-8500, "GBP"));
        assert_eq!(tx.amount_eur, Some(Money::from_cents(-10000)));
    }

    #[test]
    fn foreign_accounts_join_the_total_balance() {
        let store = Store::open_in_memory().unwrap();
        store
            .conn()
            .execute_batch(
                "INSERT INTO accounts (uid, currency, balance_cents, balance_at)
                 VALUES ('e', 'EUR', 100000, '2026-11-09T08:00:00+00:00'),
                        ('u', 'USD', 11000, '2026-11-09T08:00:00+00:00');
                 INSERT INTO fx_rates VALUES ('USD', '2026-11-06', 1.10);",
            )
            .unwrap();
        let h = balance_history(&store, d("2026-11-10"), 30).unwrap();
        assert_eq!(h.current, Some(Money::from_cents(110000)));
        let usd = h.accounts.iter().find(|a| a.currency == "USD").unwrap();
        assert!(usd.in_total);
        assert_eq!(usd.balance_eur, Some(Money::from_cents(10000)));
        assert_eq!(
            status(&store, d("2026-11-10")).unwrap().balance_total,
            Some(Money::from_cents(110000))
        );
    }
}
