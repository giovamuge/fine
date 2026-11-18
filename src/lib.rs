//! fine — personal finance on your own bank data, powered by your AI subscription via MCP.

/// The version of this build: the release's for published binaries (see build.rs).
pub const VERSION: &str = env!("FINE_VERSION");

pub mod ai;
pub mod analysis;
pub mod assistant;
pub mod budgets;
pub mod categorize;
pub mod config;
pub mod connect;
pub mod db;
pub mod enablebanking;
pub mod i18n;
pub mod mcp;
pub mod money;
pub mod notify;
pub mod ops;
pub mod reconcile;
pub mod setup;
pub mod sync;
pub mod tags;
pub mod web;
