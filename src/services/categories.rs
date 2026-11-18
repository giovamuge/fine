//! Categories, categorization (by hand and by rules) and the rules it learns.

use serde::Serialize;

use super::budgets;
use crate::domain::category::{self, CategoryKind};
use crate::domain::rules::learnable_pattern;
use crate::domain::transaction::CategorizedBy;
use crate::error::Result;
use crate::storage::{CategoryView, RuleView, Store};
use crate::tr;

pub use crate::storage::CategoryPatch;

pub fn list(store: &Store) -> Result<Vec<CategoryView>> {
    store.db().list_categories()
}

/// Create a category, or change the kind of an existing one.
pub fn upsert(store: &mut Store, name: &str, kind: &str) -> Result<()> {
    let name = category::clean_name(name)?;
    let kind: CategoryKind = kind.parse()?;
    store.write(|db| {
        if db.upsert_category(name, kind)? {
            // A new category may fit what the AI could not place before.
            db.reset_ai_checked()?;
        }
        Ok(())
    })
}

/// Create a new category with its look (the name must not exist yet).
pub fn create(
    store: &mut Store,
    name: &str,
    kind: &str,
    icon: String,
    color: String,
) -> Result<()> {
    if store.db().find_category(name)?.is_some() {
        crate::invalid!(
            "{}",
            tr!(
                "esiste già una categoria {:?}",
                "a category {:?} already exists",
                name.trim()
            )
        );
    }
    upsert(store, name, kind)?;
    let patch = CategoryPatch {
        icon: Some(icon),
        color: Some(color),
        ..Default::default()
    };
    update(store, name, &patch)
}

/// Change a category; budget rules follow a rename.
pub fn update(store: &mut Store, name: &str, patch: &CategoryPatch) -> Result<()> {
    store.write(|db| {
        let id = db.category_id(name)?;
        if let Some(new) = patch.rename.as_deref() {
            let new = category::clean_name(new)?;
            if db.find_category(new)?.is_some_and(|other| other != id) {
                crate::invalid!(
                    "{}",
                    tr!(
                        "esiste già una categoria {new:?}: per unirle elimina questa spostando i movimenti lì",
                        "a category {new:?} already exists: to merge them, delete this one moving its transactions there"
                    )
                );
            }
            db.rename_category(id, new)?;
            budgets::rename_category(db, name, new)?;
        }
        if let Some(kind) = &patch.kind {
            db.set_category_kind(id, kind.parse()?)?;
        }
        if let Some(icon) = &patch.icon {
            db.set_category_icon(id, category::clean_icon(icon)?.as_deref())?;
        }
        if let Some(color) = &patch.color {
            db.set_category_color(id, category::clean_color(color)?.as_deref())?;
        }
        Ok(())
    })
}

#[derive(Debug, Serialize)]
pub struct DeleteOutcome {
    /// Transactions moved to `move_to`, or left without a category.
    pub transactions: i64,
    pub moved_to: Option<String>,
}

/// Delete a category. With `move_to` its transactions, rules, budgets and planned items move
/// there (a merge); without, transactions become uncategorized (and the AI may look at them
/// again) while its rules are deleted (budget rules naming it are then reported as problems).
pub fn delete(store: &mut Store, name: &str, move_to: Option<&str>) -> Result<DeleteOutcome> {
    store.write(|db| {
        let id = db.category_id(name)?;
        let target = match move_to.map(str::trim).filter(|m| !m.is_empty()) {
            Some(m) => {
                let t = db.category_id(m)?;
                if t == id {
                    crate::invalid!("non puoi spostare i movimenti nella stessa categoria");
                }
                Some((t, db.category_name(t)?))
            }
            None => None,
        };
        let transactions = db.category_transaction_count(id)?;
        match &target {
            Some((t, target_name)) => {
                db.move_category_contents(id, *t)?;
                budgets::rename_category(db, name, target_name)?;
            }
            None => db.uncategorize(id)?,
        }
        db.delete_category(id)?;
        Ok(DeleteOutcome {
            transactions,
            moved_to: target.map(|(_, n)| n),
        })
    })
}

#[derive(Debug, Serialize)]
pub struct CategorizeOutcome {
    pub updated: usize,
    pub rule_pattern: Option<String>,
    /// Other uncategorized transactions categorized automatically by the new rule.
    pub auto_categorized: usize,
}

/// Assign `category` to transactions by hand; optionally learn a rule (from `pattern`, or the
/// first transaction's merchant) so future ones are categorized automatically.
pub fn categorize(
    store: &mut Store,
    ids: &[i64],
    category: &str,
    learn_rule: bool,
    pattern: Option<&str>,
) -> Result<CategorizeOutcome> {
    if ids.is_empty() {
        crate::invalid!("nessuna transazione indicata");
    }
    store.write(|db| {
        let category_id = db.category_id(category)?;
        let mut updated = 0;
        let mut first_merchant = None;
        for &id in ids {
            let merchant = db.merchant_of(id)?;
            first_merchant.get_or_insert(merchant);
            updated += db.set_category(id, category_id, CategorizedBy::Manual)?;
        }
        let (rule_pattern, auto_categorized) = if learn_rule {
            let p = learnable_pattern(pattern, first_merchant)?;
            db.learn_rule(&p, category_id)?;
            (Some(p), db.apply_category_rules()?)
        } else {
            (None, 0)
        };
        Ok(CategorizeOutcome {
            updated,
            rule_pattern,
            auto_categorized,
        })
    })
}

/// Categorize what the rules recognize. Returns how many were categorized.
pub fn apply_rules(store: &mut Store) -> Result<usize> {
    store.write(|db| db.apply_category_rules())
}

pub fn list_rules(store: &Store) -> Result<Vec<RuleView>> {
    store.db().list_rules()
}

pub fn delete_rule(store: &mut Store, id: i64) -> Result<()> {
    store.db().delete_rule(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::testing::{add_tx, category_of, insert_described};

    #[test]
    fn longest_rule_wins_and_learning_spreads() {
        let mut store = Store::open_in_memory().unwrap();
        insert_described(&store, 1, "UBER EATS ORDINE");
        insert_described(&store, 2, "PALESTRA FITNESS ROMA");
        insert_described(&store, 3, "PALESTRA FITNESS ROMA");
        assert_eq!(apply_rules(&mut store).unwrap(), 1);
        assert_eq!(category_of(&store, 1).as_deref(), Some("Ristoranti e bar"));

        let out = categorize(&mut store, &[2], "Svago", true, Some("palestra fitness")).unwrap();
        assert_eq!(out.updated, 1);
        assert_eq!(out.auto_categorized, 1); // transaction 3 learned automatically
    }

    #[test]
    fn categories_are_customizable_and_mergeable() {
        let mut store = Store::open_in_memory().unwrap();
        let cats = list(&store).unwrap();
        assert!(
            cats.iter()
                .any(|c| c.name == "Affitto e mutuo" && c.icon.is_some())
        );

        upsert(&mut store, "Bici", "expense").unwrap();
        let patch = CategoryPatch {
            rename: Some("Bicicletta".into()),
            icon: Some("🚲".into()),
            color: Some("teal".into()),
            ..Default::default()
        };
        update(&mut store, "bici", &patch).unwrap();
        let bike = list(&store)
            .unwrap()
            .into_iter()
            .find(|c| c.name == "Bicicletta")
            .unwrap();
        assert_eq!(
            (bike.icon.as_deref(), bike.color.as_deref()),
            (Some("🚲"), Some("teal"))
        );
        let bad = CategoryPatch {
            color: Some("#ff0000".into()),
            ..Default::default()
        };
        assert!(update(&mut store, "Bicicletta", &bad).is_err());
        let clash = CategoryPatch {
            rename: Some("svago".into()),
            ..Default::default()
        };
        assert!(update(&mut store, "Bicicletta", &clash).is_err());

        add_tx(&store, "2026-03-01", -1000, "x", Some("Bicicletta"));
        add_tx(&store, "2026-03-01", -1000, "x", Some("Bicicletta"));
        let bike: crate::domain::budget::Budget = serde_json::from_value(serde_json::json!({
            "name": "Bici", "period": {"type": "month"}, "amount": 50, "rule": {"category": ["Bicicletta"]}
        }))
        .unwrap();
        budgets::upsert(&mut store, bike, None).unwrap();
        let out = delete(&mut store, "Bicicletta", Some("Sport e benessere")).unwrap();
        assert_eq!(out.transactions, 2);
        let count = |store: &Store, c: &str| {
            list(store)
                .unwrap()
                .into_iter()
                .find(|x| x.name == c)
                .unwrap()
                .transactions
        };
        assert_eq!(count(&store, "Sport e benessere"), 2);
        let def: String = store
            .conn()
            .query_row("SELECT definition FROM budgets", [], |r| r.get(0))
            .unwrap();
        assert!(def.contains("Sport e benessere"));

        let out = delete(&mut store, "Sport e benessere", None).unwrap();
        assert_eq!(out.transactions, 2);
        let uncategorized: i64 = store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE category_id IS NULL AND ai_checked_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(uncategorized, 2);
    }
}
