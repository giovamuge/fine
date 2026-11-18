//! SQLite storage. All SQL lives in this module.
//!
//! - [`Database`] names the database file; opening it migrates the schema once.
//! - [`Store`] owns a connection and decides transaction boundaries.
//! - [`Db`] is a view on a connection (or on an open transaction) with the queries, grouped by
//!   table in the submodules.

mod accounts;
mod budgets;
mod categories;
mod fx;
mod planned;
mod reconcile;
mod rules;
mod schema;
mod seed;
mod settings;
mod sync_log;
mod tags;
mod transactions;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use rusqlite::{Connection, Params, Row};

use crate::error::{Error, Result};

pub use accounts::{AccountView, NewAccount};
pub use budgets::StoredBudget;
pub use categories::{CategoryPatch, CategoryView};
pub use fx::{RATE_MAX_AGE_DAYS, is_rateable};
pub use planned::NewPlanned;
pub use rules::RuleView;
pub use seed::{DEFAULT_CATEGORIES, DEFAULT_RULES};
pub use tags::TagView;
pub use transactions::{NewTransaction, SearchParams, TxView};

/// The database file. Cheap to clone; every [`connect`](Self::connect) opens a new connection.
#[derive(Debug, Clone)]
pub struct Database {
    path: Arc<PathBuf>,
}

impl Database {
    /// Open (creating it if needed) and migrate the database.
    pub fn open(path: &Path) -> Result<Self> {
        let fresh = !path.exists();
        let mut store = Store::connect(path)?;
        if fresh {
            crate::config::restrict_permissions(path, 0o600);
        }
        schema::migrate(&mut store.conn)?;
        Ok(Self {
            path: Arc::new(path.to_owned()),
        })
    }

    /// A new connection to the (already migrated) database.
    pub fn connect(&self) -> Result<Store> {
        Store::connect(&self.path)
    }
}

/// A connection to the database.
pub struct Store {
    conn: Connection,
}

impl Store {
    fn connect(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .map_err(|e| Error::invalid(format!("apertura database {}: {e}", path.display())))?;
        Self::configure(conn)
    }

    /// A private, migrated in-memory database (tests).
    pub fn open_in_memory() -> Result<Self> {
        let mut store = Self::configure(Connection::open_in_memory()?)?;
        schema::migrate(&mut store.conn)?;
        Ok(store)
    }

    fn configure(conn: Connection) -> Result<Self> {
        conn.busy_timeout(Duration::from_secs(5))?;
        // WAL lets the dashboard read while a sync writes; NORMAL sync is durable in WAL mode
        // across crashes of the program (only a power loss can drop the last transaction).
        conn.execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;",
        )?;
        Ok(Self { conn })
    }

    /// Queries outside a transaction (each statement commits on its own).
    pub fn db(&self) -> Db<'_> {
        Db { conn: &self.conn }
    }

    /// Run `f` in a transaction: committed if it returns `Ok`, rolled back otherwise.
    pub fn write<T>(&mut self, f: impl FnOnce(Db<'_>) -> Result<T>) -> Result<T> {
        let tx = self.conn.transaction()?;
        let out = f(Db { conn: &tx })?;
        tx.commit()?;
        Ok(out)
    }

    /// The raw connection, for tests that set up data directly.
    #[doc(hidden)]
    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}

/// Queries on a connection or an open transaction.
#[derive(Clone, Copy)]
pub struct Db<'c> {
    conn: &'c Connection,
}

impl Db<'_> {
    /// Collect the rows of a (cached) query.
    fn rows<T>(
        &self,
        sql: &str,
        params: impl Params,
        f: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>> {
        let mut st = self.conn.prepare_cached(sql)?;
        let rows = st.query_map(params, f)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The single row of a (cached) query, or `None`.
    fn row<T>(
        &self,
        sql: &str,
        params: impl Params,
        f: impl FnOnce(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Option<T>> {
        use rusqlite::OptionalExtension;
        let mut st = self.conn.prepare_cached(sql)?;
        Ok(st.query_row(params, f).optional()?)
    }

    /// A value every query returns exactly once (aggregates).
    fn value<T: rusqlite::types::FromSql>(&self, sql: &str, params: impl Params) -> Result<T> {
        let mut st = self.conn.prepare_cached(sql)?;
        Ok(st.query_row(params, |r| r.get(0))?)
    }

    /// Run a (cached) statement; returns the number of changed rows.
    fn exec(&self, sql: &str, params: impl Params) -> Result<usize> {
        let mut st = self.conn.prepare_cached(sql)?;
        Ok(st.execute(params)?)
    }
}

/// SQL expression (on table alias `a`) giving a human, unique-enough account label,
/// e.g. "FinecoBank · MARIO ROSSI (USD)".
const ACCOUNT_LABEL: &str = "COALESCE(a.label, COALESCE(a.aspsp || ' · ', '') || COALESCE(a.name, a.iban, a.uid) || ' (' || a.currency || ')')";

/// SQL expression (on table alias `a`): lowercase label, holder, bank and IBAN, for matching.
const ACCOUNT_MATCH_TEXT: &str = "lower(COALESCE(a.label, '') || ' ' || COALESCE(a.name, '') || ' ' || COALESCE(a.aspsp, '') || ' ' || replace(COALESCE(a.iban, ''), ' ', ''))";
