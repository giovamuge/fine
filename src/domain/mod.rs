//! The domain: types and rules of personal finance, with no I/O. Everything here works on data
//! already loaded, so it is deterministic and unit-tested without a database.

pub mod analysis;
pub mod assistant;
pub mod balance;
pub mod budget;
pub mod calendar;
pub mod category;
pub mod money;
pub mod planned;
pub mod rules;
pub mod tag;
pub mod text;
pub mod transaction;

#[cfg(test)]
pub(crate) mod testing;

pub use money::Money;

/// Analyses (budgets, available, trends) are computed in this currency only.
pub const BASE_CURRENCY: &str = "EUR";
