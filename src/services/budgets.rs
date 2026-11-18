//! Budgets: saving definitions and evaluating them against the stored transactions.

use chrono::NaiveDate;

use crate::domain::budget::{Budget, BudgetStatus, Evaluator, ValidationContext, earliest_needed};
use crate::error::{Error, Result};
use crate::storage::{Db, Store};

fn validation_context(db: Db<'_>) -> Result<ValidationContext> {
    Ok(ValidationContext {
        categories: db.category_names()?,
        accounts: db.account_match_texts()?,
    })
}

fn evaluator<'a>(
    db: Db<'_>,
    budgets: impl IntoIterator<Item = &'a Budget>,
    today: NaiveDate,
) -> Result<Evaluator> {
    let from = earliest_needed(budgets, today)?;
    Ok(Evaluator::new(
        db.load_txs(from, today)?,
        db.load_planned()?,
        db.category_names()?,
        today,
    ))
}

/// Status of every budget.
pub fn statuses(store: &Store, today: NaiveDate) -> Result<Vec<BudgetStatus>> {
    let db = store.db();
    let all = db.load_budgets()?;
    if all.is_empty() {
        return Ok(vec![]);
    }
    let eval = evaluator(db, all.iter().map(|s| &s.budget), today)?;
    all.iter()
        .map(|s| eval.evaluate(s.id, &s.budget, s.created, false))
        .collect()
}

/// One budget (by name, case-insensitive) with the transactions that count in its period.
pub fn status_of(store: &Store, name: &str, today: NaiveDate) -> Result<BudgetStatus> {
    let db = store.db();
    let wanted = name.trim().to_lowercase();
    let s = db
        .load_budgets()?
        .into_iter()
        .find(|s| s.budget.name.to_lowercase() == wanted)
        .ok_or_else(|| Error::not_found(format!("budget {name:?} non trovato")))?;
    evaluator(db, [&s.budget], today)?.evaluate(s.id, &s.budget, s.created, true)
}

/// Evaluate a definition without saving it (to check a rule before creating the budget).
pub fn preview(store: &Store, mut budget: Budget, today: NaiveDate) -> Result<BudgetStatus> {
    let db = store.db();
    budget.validate(&validation_context(db)?)?;
    evaluator(db, [&budget], today)?.evaluate(0, &budget, today, true)
}

/// Create or replace a budget by name. `replaces` renames an existing budget.
pub fn upsert(store: &mut Store, mut budget: Budget, replaces: Option<&str>) -> Result<Budget> {
    store.write(|db| {
        budget.validate(&validation_context(db)?)?;
        db.save_budget(&budget, replaces)?;
        Ok(budget)
    })
}

pub fn delete(store: &mut Store, name: &str) -> Result<()> {
    store.db().delete_budget(name)
}

/// Keep budget rules in sync when a category is renamed or merged into another (`new`).
pub(crate) fn rename_category(db: Db<'_>, old: &str, new: &str) -> Result<()> {
    for mut s in db.load_budgets()? {
        if s.budget.rename_category(old, new) {
            db.update_budget_definition(s.id, &s.budget)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Money;
    use crate::domain::budget::{Basis, Goal, Period, Phase, Rollover, Rule, State};
    use crate::domain::testing::d;
    use crate::services::testing::{add_tx, tag};
    use crate::services::{categories, categories::CategoryPatch};

    fn budget(name: &str, rule: Option<Rule>, period: Period, amount: f64) -> Budget {
        serde_json::from_value(serde_json::json!({
            "name": name, "period": period, "amount": amount, "rule": rule
        }))
        .unwrap()
    }

    fn month() -> Period {
        Period::Month { start_day: None }
    }

    fn rule(v: serde_json::Value) -> Rule {
        serde_json::from_value(v).unwrap()
    }

    fn eur(m: Money) -> f64 {
        m.to_eur()
    }

    #[test]
    fn nested_rules_with_tags_text_and_amounts() {
        let mut store = Store::open_in_memory().unwrap();
        add_tx(
            &store,
            "2026-11-02",
            -3000,
            "trattoria",
            Some("Ristoranti e bar"),
        );
        let b = add_tx(
            &store,
            "2026-11-03",
            -5000,
            "pizzeria",
            Some("Ristoranti e bar"),
        );
        add_tx(&store, "2026-11-04", -2000, "cinema odeon", Some("Svago"));
        add_tx(
            &store,
            "2026-11-05",
            -9000,
            "esselunga",
            Some("Spesa alimentare"),
        );
        tag(&mut store, b, "lavoro");
        // Restaurants + leisure, without work lunches.
        let svago = rule(serde_json::json!({"all": [
            {"category": ["ristoranti e bar", "Svago"]},
            {"not": {"tag": ["#Lavoro"]}}
        ]}));
        let today = d("2026-11-10");
        let s = preview(&store, budget("Svago", Some(svago), month(), 100.0), today).unwrap();
        assert_eq!(eur(s.value), 50.0);
        assert_eq!(s.transactions.as_ref().unwrap().len(), 2);
        // Names were canonicalized and tags normalized.
        assert!(s.rule_summary.contains("Ristoranti e bar"));
        assert!(s.rule_summary.contains("#lavoro"));

        let big =
            rule(serde_json::json!({"any": [{"text": ["Esselunga"]}, {"amount": {"min": 40}}]}));
        let s = preview(&store, budget("Grandi", Some(big), month(), 500.0), today).unwrap();
        assert_eq!(eur(s.value), 140.0);

        // Everything except groceries.
        let not_food = rule(serde_json::json!({"not": {"category": ["Spesa alimentare"]}}));
        let s = preview(
            &store,
            budget("Variabili", Some(not_food), month(), 500.0),
            today,
        )
        .unwrap();
        assert_eq!(eur(s.value), 100.0);

        // Weekday and day of month: 2026-11-02 is a Monday.
        let monday = rule(serde_json::json!({"weekday": [1]}));
        let s = preview(
            &store,
            budget("Lunedì", Some(monday), month(), 500.0),
            today,
        )
        .unwrap();
        assert_eq!(eur(s.value), 30.0);
        let early = rule(serde_json::json!({"day_of_month": {"from": 4, "to": 5}}));
        let s = preview(&store, budget("Inizio", Some(early), month(), 500.0), today).unwrap();
        assert_eq!(eur(s.value), 110.0);
    }

    #[test]
    fn validation_errors_are_clear() {
        let mut store = Store::open_in_memory().unwrap();
        let bad = |r: serde_json::Value, period: Period| {
            preview(
                &store,
                budget("x", Some(rule(r)), period, 10.0),
                d("2026-11-10"),
            )
            .unwrap_err()
            .to_string()
        };
        assert!(
            bad(serde_json::json!({"category": ["Inesistente"]}), month()).contains("Inesistente")
        );
        assert!(bad(serde_json::json!({"account": ["fineco"]}), month()).contains("conto"));
        assert!(
            bad(
                serde_json::json!({"amount": {"min": 50, "max": 10}}),
                month()
            )
            .contains("min")
        );
        assert!(bad(serde_json::json!({"all": []}), month()).contains("vuota"));
        assert!(bad(serde_json::json!({"text": ["x"]}), month()).contains("corto"));
        let custom = Period::Custom {
            from: "2026-08-07".into(),
            to: "2026-08-01".into(),
        };
        assert!(bad(serde_json::json!({"direction": "out"}), custom).contains("finisce prima"));
        let mut b = budget("Risparmio", None, Period::Rolling { days: 30 }, 10.0);
        b.goal = Goal::MinSave;
        b.rollover = Rollover::Surplus;
        assert!(
            upsert(&mut store, b, None)
                .unwrap_err()
                .to_string()
                .contains("riporto")
        );
    }

    #[test]
    fn pace_thresholds_and_overspending() {
        let mut store = Store::open_in_memory().unwrap();
        let r = rule(serde_json::json!({"category": ["Ristoranti e bar"]}));
        let mut b = budget("Ristoranti", Some(r), month(), 200.0);
        b.alerts = vec![100];
        upsert(&mut store, b, None).unwrap();
        add_tx(
            &store,
            "2026-11-02",
            -14000,
            "trattoria",
            Some("Ristoranti e bar"),
        );
        // 70% used at day 10/30 -> pace warning
        let s = &statuses(&store, d("2026-11-10")).unwrap()[0];
        assert_eq!(s.state, State::Warning);
        assert_eq!(s.per_day, Some(Money::from_cents(285))); // 60 € left over 21 days
        assert_eq!(s.projected, Some(Money::from_cents(42000)));
        add_tx(
            &store,
            "2026-11-09",
            -7000,
            "pizzeria",
            Some("Ristoranti e bar"),
        );
        let s = &statuses(&store, d("2026-11-10")).unwrap()[0];
        assert_eq!(s.state, State::Over);
        assert_eq!(s.thresholds_reached, vec![100]);
    }

    #[test]
    fn rollover_with_cap_starts_when_the_budget_was_created() {
        let mut store = Store::open_in_memory().unwrap();
        let r = rule(serde_json::json!({"category": ["Ristoranti e bar"]}));
        let mut b = budget("Ristoranti", Some(r), month(), 100.0);
        b.rollover = Rollover::Both;
        b.rollover_cap = Some(50.0);
        upsert(&mut store, b, None).unwrap();
        store
            .conn()
            .execute("UPDATE budgets SET created_at = '2026-08-15 10:00:00'", [])
            .unwrap();
        add_tx(&store, "2026-07-10", -1000, "bar", Some("Ristoranti e bar")); // before: ignored
        add_tx(&store, "2026-08-10", -4000, "bar", Some("Ristoranti e bar")); // +60, capped at 50
        add_tx(
            &store,
            "2026-09-10",
            -17000,
            "bar",
            Some("Ristoranti e bar"),
        ); // limit 150 -> -20
        add_tx(&store, "2026-10-10", -5000, "bar", Some("Ristoranti e bar")); // limit 80 -> +30
        let s = &statuses(&store, d("2026-11-05")).unwrap()[0];
        assert_eq!(eur(s.carried), 30.0);
        assert_eq!(eur(s.limit), 130.0);
        assert_eq!(s.history.len(), 6);
        assert_eq!(eur(s.history.last().unwrap().limit), 80.0);
    }

    #[test]
    fn cash_basis_counts_amortized_purchases_in_full() {
        let store = Store::open_in_memory().unwrap();
        let id = add_tx(
            &store,
            "2026-11-03",
            -120000,
            "apple store",
            Some("Elettronica"),
        );
        store.db().set_amortization(id, 12).unwrap();
        let r = rule(serde_json::json!({"category": ["Elettronica"]}));
        let mut b = budget("Tech", Some(r), month(), 2000.0);
        let today = d("2026-11-10");
        assert_eq!(eur(preview(&store, b.clone(), today).unwrap().value), 100.0);
        b.basis = Basis::Cash;
        assert_eq!(eur(preview(&store, b, today).unwrap().value), 1200.0);
    }

    #[test]
    fn savings_and_trip_budgets() {
        let mut store = Store::open_in_memory().unwrap();
        add_tx(
            &store,
            "2026-11-01",
            -30000,
            "giroconto deposito",
            Some("Risparmi e investimenti"),
        );
        add_tx(
            &store,
            "2026-11-08",
            5000,
            "prelievo deposito",
            Some("Risparmi e investimenti"),
        );
        let mut save = budget("Metti da parte", None, month(), 200.0);
        save.goal = Goal::MinSave;
        let s = preview(&store, save, d("2026-11-10")).unwrap();
        assert_eq!(eur(s.value), 250.0);
        assert_eq!(s.state, State::Reached);

        let trip = rule(serde_json::json!({"tag": ["vacanza-londra"]}));
        let p = Period::Custom {
            from: "2026-08-01".into(),
            to: "2026-08-07".into(),
        };
        let t = add_tx(&store, "2026-08-03", -30000, "hotel", Some("Viaggi"));
        tag(&mut store, t, "vacanza-londra");
        add_tx(&store, "2026-08-04", -2000, "pub", Some("Ristoranti e bar"));
        let s = preview(
            &store,
            budget("Londra", Some(trip.clone()), p.clone(), 800.0),
            d("2026-11-10"),
        )
        .unwrap();
        assert_eq!(s.phase, Phase::Ended);
        assert_eq!(eur(s.value), 300.0);
        assert_eq!(s.per_day, None);
        let s = preview(
            &store,
            budget("Londra", Some(trip), p, 800.0),
            d("2026-07-01"),
        )
        .unwrap();
        assert_eq!(s.phase, Phase::Upcoming);
        assert_eq!(s.value, Money::ZERO);
    }

    #[test]
    fn reserve_takes_the_unspent_part_of_this_month() {
        let store = Store::open_in_memory().unwrap();
        let p = Period::Custom {
            from: "2026-11-21".into(),
            to: "2026-12-10".into(),
        };
        let mut b = budget("Viaggio", None, p, 200.0);
        b.reserve = true;
        // From 21/11 to 10/12: 20 days, 10 of them in November.
        let s = preview(&store, b, d("2026-11-12")).unwrap();
        assert_eq!(eur(s.reserved), 100.0);
    }

    #[test]
    fn category_rename_and_merge_update_rules() {
        let mut store = Store::open_in_memory().unwrap();
        let r = rule(serde_json::json!({"category": ["Ristoranti e bar", "Svago"]}));
        upsert(&mut store, budget("Svago", Some(r), month(), 100.0), None).unwrap();
        let patch = CategoryPatch {
            rename: Some("Locali".into()),
            ..Default::default()
        };
        categories::update(&mut store, "ristoranti e bar", &patch).unwrap();
        categories::delete(&mut store, "Svago", Some("Locali")).unwrap();
        let s = &statuses(&store, d("2026-11-10")).unwrap()[0];
        assert_eq!(s.budget.rule, Some(Rule::Category(vec!["Locali".into()])));
        assert!(s.problems.is_empty());
        categories::delete(&mut store, "Locali", None).unwrap();
        assert_eq!(
            statuses(&store, d("2026-11-10")).unwrap()[0].problems.len(),
            1
        );
    }

    #[test]
    fn upsert_renames_and_deletes() {
        let mut store = Store::open_in_memory().unwrap();
        upsert(&mut store, budget("A", None, month(), 100.0), None).unwrap();
        upsert(&mut store, budget("B", None, month(), 50.0), Some("A")).unwrap();
        upsert(&mut store, budget("B", None, month(), 70.0), None).unwrap();
        let all = statuses(&store, d("2026-11-10")).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].amount, 70.0);
        delete(&mut store, "b").unwrap();
        assert!(statuses(&store, d("2026-11-10")).unwrap().is_empty());
    }
}
