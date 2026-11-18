//! The tools' arguments. Their names and doc comments are the JSON schemas MCP clients see.

use schemars::JsonSchema;
use serde::Deserialize;

use crate::domain::budget::Budget;
use crate::storage::CategoryPatch;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct MonthArg {
    /// Month YYYY-MM (default: current month)
    pub month: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LimitArg {
    /// Max results (default 50)
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CategorizeArgs {
    /// Transaction ids to categorize
    pub transaction_ids: Vec<i64>,
    /// Existing category name (see list_categories)
    pub category: String,
    /// Create a rule so future matching transactions are categorized automatically (default true)
    pub learn_rule: Option<bool>,
    /// Short distinctive lowercase text for the rule, e.g. "esselunga". Defaults to the merchant.
    pub pattern: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TagArgs {
    /// Transaction ids to tag
    pub transaction_ids: Vec<i64>,
    /// Tags to add, e.g. ["vacanza-londra", "rimborsabile"]. Free text; created if new.
    pub tags: Vec<String>,
    /// Also create a rule so transactions matching `pattern` (past and future) get these tags (default false)
    pub learn_rule: Option<bool>,
    /// Short distinctive lowercase text for the rule, e.g. "ryanair". Defaults to the merchant.
    pub pattern: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UntagArgs {
    pub transaction_ids: Vec<i64>,
    pub tags: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeriodArgs {
    /// Start date YYYY-MM-DD (inclusive, default: all time)
    pub from: Option<String>,
    /// End date YYYY-MM-DD (inclusive)
    pub to: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DaysArg {
    /// How many days of history (default 365)
    pub days: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CategoryArgs {
    pub name: String,
    /// "expense", "income", "transfer" (between own accounts) or "saving" (savings/investments)
    pub kind: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpdateCategoryArgs {
    /// Current category name
    pub name: String,
    #[serde(flatten)]
    pub patch: CategoryPatch,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeleteCategoryArgs {
    pub name: String,
    /// Move its transactions, rules, budgets and planned items to this category (a merge).
    /// Without it, its transactions become uncategorized.
    pub move_to: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IdArg {
    pub id: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UpsertBudgetArgs {
    #[serde(flatten)]
    pub budget: Budget,
    /// Current name of the budget when renaming it
    pub replaces: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BudgetStatusArgs {
    /// One budget by name: also lists the transactions that count in its current period
    pub name: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct NameArg {
    pub name: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AmortizeArgs {
    pub transaction_id: i64,
    /// Number of months to spread the expense over (2-120); 0 or 1 removes it
    pub months: i64,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct AmountArg {
    /// Amount in euro per month
    pub amount: f64,
}
