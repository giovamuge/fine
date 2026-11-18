//! fine — personal finance on your own bank data, powered by your AI subscription via MCP.
//!
//! Layers, from the inside out:
//! - [`domain`]: types and rules of personal finance, pure and deterministic;
//! - [`storage`]: SQLite, the only place with SQL;
//! - [`bank`]: the Enable Banking client;
//! - [`services`]: the use cases, shared by every interface;
//! - [`mcp`], [`web`] (and the CLI in the binary): thin adapters for AI clients, the dashboard
//!   and the terminal.

pub mod app;
pub mod bank;
pub mod config;
pub mod domain;
pub mod error;
pub mod i18n;
pub mod mcp;
pub mod services;
pub mod storage;
pub mod web;

pub use app::App;
pub use error::{Error, Result};

/// The version of this build: the release's for published binaries (see build.rs).
pub const VERSION: &str = env!("FINE_VERSION");
