//! The plan: expected income and fixed expenses, amortized purchases, the savings goal.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::domain::calendar::{Cadence, parse_date};
use crate::domain::planned::{Planned, PlannedKind};
use crate::domain::{Money, text};
use crate::error::Result;
use crate::storage::{NewPlanned, Store};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PlannedInput {
    /// Unique name, e.g. "Affitto", "Stipendio", "Assicurazione auto"
    pub name: String,
    /// "expense" or "income"
    pub kind: String,
    /// Amount in euro (positive)
    pub amount: f64,
    /// "week", "month" or "year"
    pub cadence: String,
    /// Next (or last) due date YYYY-MM-DD; later dates are computed from the cadence
    pub next_due: String,
    /// Optional category name
    pub category: Option<String>,
    /// Text that identifies the payment in bank transactions (e.g. "immobiliare rossi").
    /// Without it, matching uses category + amount within ±20%.
    pub pattern: Option<String>,
}

pub fn list(store: &Store) -> Result<Vec<Planned>> {
    store.db().load_planned()
}

pub fn upsert(store: &mut Store, p: &PlannedInput) -> Result<()> {
    let kind: PlannedKind = p.kind.parse()?;
    let cadence: Cadence = p.cadence.parse()?;
    let amount = Money::from_eur(p.amount)?;
    if !amount.is_positive() {
        crate::invalid!("l'importo deve essere positivo");
    }
    let next_due = parse_date(&p.next_due)?;
    let db = store.db();
    let category_id = p
        .category
        .as_deref()
        .map(|c| db.category_id(c))
        .transpose()?;
    let pattern = p
        .pattern
        .as_deref()
        .map(text::normalize)
        .filter(|s| !s.is_empty());
    db.upsert_planned(&NewPlanned {
        name: p.name.trim(),
        kind,
        amount,
        cadence,
        next_due,
        category_id,
        pattern: pattern.as_deref(),
    })
}

pub fn delete(store: &mut Store, name: &str) -> Result<()> {
    store.db().delete_planned(name)
}

/// Spread an expense over `months` months in all analyses; 0 or 1 removes the amortization.
pub fn amortize(store: &mut Store, transaction_id: i64, months: i64) -> Result<()> {
    let db = store.db();
    let amount = db.amount_of(transaction_id)?;
    if months <= 1 {
        return db.clear_amortization(transaction_id);
    }
    if !(2..=120).contains(&months) {
        crate::invalid!("mesi non validi: da 2 a 120");
    }
    if !amount.is_negative() {
        crate::invalid!("si possono ammortizzare solo le uscite");
    }
    db.set_amortization(transaction_id, months)
}

/// The monthly savings goal, in euro: it is reserved before computing what can be spent.
pub fn set_savings_goal(store: &mut Store, eur: f64) -> Result<()> {
    let goal = Money::from_eur(eur)?;
    if goal.is_negative() {
        crate::invalid!("obiettivo negativo");
    }
    store.db().set_savings_goal(goal)
}
