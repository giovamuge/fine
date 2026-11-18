//! What every interface (CLI, MCP server, dashboard) shares: the database and the configuration.

use std::sync::Arc;

use chrono::NaiveDate;

use crate::bank::Client;
use crate::config::Config;
use crate::error::{Error, Result};
use crate::i18n::Lang;
use crate::storage::{Database, Store};

#[derive(Clone)]
pub struct App {
    pub db: Database,
    pub config: Arc<Config>,
}

impl App {
    pub fn new(db: Database, config: Config) -> Self {
        Self {
            db,
            config: Arc::new(config),
        }
    }

    /// Run store work on the blocking thread pool (SQLite is synchronous), in the language of
    /// the caller.
    pub async fn blocking<T, F>(&self, f: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Store) -> Result<T> + Send + 'static,
    {
        let db = self.db.clone();
        let lang = Lang::current();
        tokio::task::spawn_blocking(move || {
            lang.enter();
            f(&mut db.connect()?)
        })
        .await
        .map_err(|e| Error::Io(std::io::Error::other(e)))?
    }

    /// A connection for async work that talks to the bank (a sync, a connection).
    pub fn store(&self) -> Result<Store> {
        self.db.connect()
    }

    pub fn bank(&self) -> Result<Client> {
        Ok(Client::new(self.config.enable_banking()?)?)
    }

    /// Today in the user's time zone.
    pub fn today() -> NaiveDate {
        chrono::Local::now().date_naive()
    }
}
