//! Dynamic budgets: named budgets whose rules live in the database.
//!
//! A budget has a goal (spend at most / save at least / earn at least), a rule that picks the
//! transactions that count (a tree of `all` / `any` / `not` over typed conditions), a period
//! (recurring week/month/quarter/year with a custom start, the last N days, or fixed dates),
//! an amount, and options: rollover, amortized or cash basis, alert thresholds, and whether its
//! unspent part is reserved out of the month's "available".
//!
//! Rules are structured data, not formulas: they are validated when saved and evaluated in Rust,
//! so a budget written by the AI can never break the analysis. Category names are kept in sync
//! when categories are renamed or merged; a rule naming a deleted category is reported as a problem.
//!
//! The doc comments on these types are part of the JSON schema MCP clients see.

mod evaluate;
mod period;
mod rule;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use evaluate::{
    BudgetStatus, CountedTx, Evaluator, PeriodValue, Phase, State, earliest_needed,
};
pub use rule::ValidationContext;

use super::category::CategoryKind;
use super::money::Money;
use crate::error::Result;
use crate::tr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Goal {
    /// Spend at most `amount` (expenses; refunds reduce the total)
    #[default]
    MaxSpend,
    /// Put aside at least `amount` (net transfers to "saving" categories)
    MinSave,
    /// Receive at least `amount` (income)
    MinIncome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Period {
    /// Every week. `start_weekday`: 1 = Monday (default) ... 7 = Sunday
    Week {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_weekday: Option<u32>,
    },
    /// Every month. `start_day`: 1-28 (default 1), e.g. 27 for a month that starts on payday
    Month {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_day: Option<u32>,
    },
    /// Calendar quarters
    Quarter,
    /// Every year. `start_month`: 1-12 (default 1 = January)
    Year {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_month: Option<u32>,
    },
    /// The last `days` days, ending today (a moving window)
    Rolling { days: u32 },
    /// Fixed dates, e.g. a trip or a project: `from` and `to` as YYYY-MM-DD, inclusive
    Custom { from: String, to: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rollover {
    #[default]
    None,
    /// What is left unspent is added to the next period
    Surplus,
    /// Overspending is taken from the next period
    Deficit,
    /// Both
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// Like the rest of fine: amortized purchases count 1/N per month and payments of annual
    /// planned expenses are left out (they have their own sinking fund)
    #[default]
    Amortized,
    /// Every matching transaction counts in full on the day it was booked
    Cash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Money out
    Out,
    /// Money in
    In,
}

/// Which transactions count. Combine conditions with `all`, `any`, `not`.
/// Examples: `{"category": ["Ristoranti e bar"]}`;
/// `{"all": [{"category": ["Ristoranti e bar", "Svago"]}, {"not": {"tag": ["lavoro"]}}]}`;
/// `{"text": ["amazon"]}`; `{"not": {"category": ["Spesa alimentare"]}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// Every condition must match
    All(Vec<Rule>),
    /// At least one condition must match
    Any(Vec<Rule>),
    /// The condition must not match
    Not(Box<Rule>),
    /// The transaction's category is one of these (existing category names)
    Category(Vec<String>),
    /// The category kind is one of these: expense, income, transfer, saving
    CategoryKind(Vec<CategoryKind>),
    /// The transaction has at least one of these tags
    Tag(Vec<String>),
    /// Merchant, counterparty or description contains one of these texts
    Text(Vec<String>),
    /// Booked on an account whose name, bank or IBAN contains one of these texts
    Account(Vec<String>),
    /// Absolute amount in euro between `min` and `max` (inclusive; either may be omitted)
    Amount {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    /// "out" (money out) or "in" (money in, e.g. refunds)
    Direction(Direction),
    /// Day of the week, 1 = Monday ... 7 = Sunday
    Weekday(Vec<u32>),
    /// Day of the month between `from` and `to` (1-31, inclusive)
    DayOfMonth { from: u32, to: u32 },
    /// true: only uncategorized transactions; false: only categorized ones
    Uncategorized(bool),
    /// true: only payments of weekly/monthly planned expenses (rent, bills); false: only the others
    Fixed(bool),
}

fn default_alerts() -> Vec<u32> {
    vec![80, 100]
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

/// A budget definition, as written by the AI or the dashboard and stored in the database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Budget {
    /// Unique name, e.g. "Svago", "Viaggio Londra", "Amazon"
    pub name: String,
    /// "max_spend" (default), "min_save" or "min_income"
    #[serde(default)]
    pub goal: Goal,
    /// Which transactions count. Omit to count all expenses (max_spend), all savings
    /// (min_save) or all income (min_income).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<Rule>,
    pub period: Period,
    /// Limit (max_spend) or target (min_save, min_income) in euro per period
    pub amount: f64,
    /// Carry what is left (or the overspending) into the next period; max_spend with a
    /// recurring period only
    #[serde(default)]
    pub rollover: Rollover,
    /// Maximum carried amount in euro (either way)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollover_cap: Option<f64>,
    /// "amortized" (default) or "cash"
    #[serde(default)]
    pub basis: Basis,
    /// Alert when these percentages of the amount are used (default [80, 100])
    #[serde(default = "default_alerts")]
    pub alerts: Vec<u32>,
    /// Also alert when spending runs ahead of the time elapsed in the period (default true)
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub pace_alert: bool,
    /// Treat the unspent part as already committed: it is taken out of the month's
    /// "available" (default false)
    #[serde(default)]
    pub reserve: bool,
    /// Paused budgets are kept but not evaluated or alerted on (default true)
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Budget {
    /// Validate and normalize in place: canonical category names, normalized tags, bounds.
    pub fn validate(&mut self, ctx: &ValidationContext) -> Result<()> {
        self.name = self.name.trim().to_owned();
        if self.name.is_empty() || self.name.chars().count() > 60 {
            crate::invalid!(
                "{}",
                tr!(
                    "il nome del budget deve avere da 1 a 60 caratteri",
                    "the budget name must be 1 to 60 characters long"
                )
            );
        }
        if !self.amount.is_finite() || self.amount <= 0.0 {
            crate::invalid!(
                "{}",
                tr!(
                    "l'importo del budget deve essere maggiore di zero",
                    "the budget amount must be greater than zero"
                )
            );
        }
        Money::from_eur(self.amount)?;
        self.period.validate()?;
        if self.rollover != Rollover::None {
            if self.goal != Goal::MaxSpend {
                crate::invalid!("il riporto vale solo per i budget di spesa (goal max_spend)");
            }
            if !self.period.is_recurring() {
                crate::invalid!(
                    "il riporto vale solo per periodi che si ripetono (week, month, quarter, year)"
                );
            }
        }
        if let Some(cap) = self.rollover_cap
            && (!cap.is_finite() || cap <= 0.0)
        {
            crate::invalid!("rollover_cap deve essere maggiore di zero");
        }
        if self.reserve && self.goal != Goal::MaxSpend {
            crate::invalid!("reserve vale solo per i budget di spesa (goal max_spend)");
        }
        if self.alerts.iter().any(|a| !(1..=500).contains(a)) {
            crate::invalid!("le soglie di avviso sono percentuali tra 1 e 500");
        }
        self.alerts.sort_unstable();
        self.alerts.dedup();
        if let Some(rule) = &mut self.rule {
            rule.validate(ctx)?;
        }
        if let Some(n) = &mut self.note {
            *n = n.trim().to_owned();
            if n.chars().count() > 500 {
                crate::invalid!("nota troppo lunga (massimo 500 caratteri)");
            }
            if n.is_empty() {
                self.note = None;
            }
        }
        Ok(())
    }

    /// Follow a category rename or merge (`old` -> `new`). Returns whether anything changed.
    pub fn rename_category(&mut self, old: &str, new: &str) -> bool {
        let Some(rule) = &mut self.rule else {
            return false;
        };
        let before = rule.clone();
        rule.rename_category(old, new);
        *rule != before
    }

    /// Readable description of what counts, in the current language.
    pub fn rule_summary(&self) -> String {
        match &self.rule {
            Some(r) => r.describe(),
            None => match self.goal {
                Goal::MaxSpend => tr!("tutte le spese", "all expenses"),
                Goal::MinSave => tr!(
                    "tutti i movimenti verso il risparmio",
                    "all transfers to savings"
                ),
                Goal::MinIncome => tr!("tutte le entrate", "all income"),
            },
        }
    }

    pub(crate) fn amount(&self) -> Result<Money> {
        Money::from_eur(self.amount)
    }
}
