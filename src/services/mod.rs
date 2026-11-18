//! Use cases shared by the CLI, the MCP server and the dashboard. Each function takes the
//! [`Store`](crate::storage::Store) it works on and decides its transaction boundaries.

pub mod ai;
pub mod budgets;
pub mod categories;
pub mod connect;
pub mod notify;
pub mod planning;
pub mod rates;
pub mod reports;
pub mod sync;
pub mod tags;

#[cfg(test)]
pub(crate) mod testing;
