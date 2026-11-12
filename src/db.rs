//! SQLite storage. Schema migrations are tracked with `PRAGMA user_version`.

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

/// SQL expression (on table alias `a`) giving a human, unique-enough account label,
/// e.g. "FinecoBank · MARIO ROSSI (USD)".
pub const ACCOUNT_LABEL: &str = "COALESCE(a.label, COALESCE(a.aspsp || ' · ', '') || COALESCE(a.name, a.iban, a.uid) || ' (' || a.currency || ')')";

/// Analyses (budgets, available, trends) are computed in this currency only.
pub const BASE_CURRENCY: &str = "EUR";

const MIGRATIONS: &[&str] = &[
    r#"
CREATE TABLE accounts (
    uid          TEXT PRIMARY KEY,
    iban         TEXT,
    name         TEXT,
    currency     TEXT NOT NULL,
    label        TEXT,
    aspsp        TEXT,
    session_id   TEXT,
    valid_until  TEXT,
    balance_cents INTEGER,
    balance_at   TEXT,
    created_at   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE categories (
    id    INTEGER PRIMARY KEY,
    name  TEXT NOT NULL UNIQUE COLLATE NOCASE,
    kind  TEXT NOT NULL CHECK (kind IN ('expense','income','transfer','saving'))
);
CREATE TABLE transactions (
    id              INTEGER PRIMARY KEY,
    account_uid     TEXT NOT NULL REFERENCES accounts(uid) ON DELETE CASCADE,
    booking_date    TEXT NOT NULL,
    amount_cents    INTEGER NOT NULL,
    currency        TEXT NOT NULL,
    counterparty    TEXT,
    description     TEXT,
    merchant        TEXT NOT NULL,
    entry_reference TEXT,
    dedup_key       TEXT NOT NULL UNIQUE,
    category_id     INTEGER REFERENCES categories(id) ON DELETE SET NULL,
    categorized_by  TEXT,
    raw_json        TEXT,
    created_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX transactions_date ON transactions(booking_date);
CREATE INDEX transactions_category ON transactions(category_id);
CREATE TABLE rules (
    id          INTEGER PRIMARY KEY,
    pattern     TEXT NOT NULL UNIQUE,
    category_id INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    source      TEXT NOT NULL,
    hits        INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TABLE budgets (
    category_id  INTEGER NOT NULL REFERENCES categories(id) ON DELETE CASCADE,
    period       TEXT NOT NULL CHECK (period IN ('week','month','year')),
    amount_cents INTEGER NOT NULL CHECK (amount_cents > 0),
    PRIMARY KEY (category_id, period)
);
CREATE TABLE planned (
    id           INTEGER PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE COLLATE NOCASE,
    kind         TEXT NOT NULL CHECK (kind IN ('expense','income')),
    amount_cents INTEGER NOT NULL CHECK (amount_cents > 0),
    cadence      TEXT NOT NULL CHECK (cadence IN ('week','month','year')),
    next_due     TEXT NOT NULL,
    category_id  INTEGER REFERENCES categories(id) ON DELETE SET NULL,
    pattern      TEXT
);
CREATE TABLE amortizations (
    transaction_id INTEGER PRIMARY KEY REFERENCES transactions(id) ON DELETE CASCADE,
    months         INTEGER NOT NULL CHECK (months BETWEEN 2 AND 120)
);
CREATE TABLE sync_log (
    id          INTEGER PRIMARY KEY,
    account_uid TEXT NOT NULL,
    at          TEXT NOT NULL,
    ok          INTEGER NOT NULL,
    message     TEXT
);
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE pending_auth (
    state      TEXT PRIMARY KEY,
    aspsp      TEXT NOT NULL,
    country    TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
"#,
    // v2: currency of the stored balance (an account may report "XXX" while balances are in EUR)
    "ALTER TABLE accounts ADD COLUMN balance_currency TEXT;",
    // v3: when the AI last looked at an uncategorized transaction (so it is not asked twice)
    "ALTER TABLE transactions ADD COLUMN ai_checked_at TEXT;",
    // v4: free-form tags, many per transaction, with learnable rules (pattern -> tag)
    r#"
CREATE TABLE tags (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);
CREATE TABLE transaction_tags (
    transaction_id INTEGER NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
    tag_id         INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    source         TEXT NOT NULL DEFAULT 'manual',
    PRIMARY KEY (transaction_id, tag_id)
);
CREATE INDEX transaction_tags_tag ON transaction_tags(tag_id);
CREATE TABLE tag_rules (
    id      INTEGER PRIMARY KEY,
    pattern TEXT NOT NULL,
    tag_id  INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    hits    INTEGER NOT NULL DEFAULT 0,
    UNIQUE (pattern, tag_id)
);
"#,
    // v5: stable account identity across consents (renewing gives new uids) and the bank's country
    "ALTER TABLE accounts ADD COLUMN identification_hash TEXT;
     ALTER TABLE accounts ADD COLUMN country TEXT;",
];

/// Default categories (Italian). (name, kind)
pub const DEFAULT_CATEGORIES: &[(&str, &str)] = &[
    ("Spesa alimentare", "expense"),
    ("Ristoranti e bar", "expense"),
    ("Trasporti", "expense"),
    ("Carburante", "expense"),
    ("Casa", "expense"),
    ("Utenze", "expense"),
    ("Telefono e internet", "expense"),
    ("Abbonamenti", "expense"),
    ("Salute", "expense"),
    ("Shopping", "expense"),
    ("Svago", "expense"),
    ("Viaggi", "expense"),
    ("Istruzione", "expense"),
    ("Regali", "expense"),
    ("Assicurazioni", "expense"),
    ("Tasse", "expense"),
    ("Animali", "expense"),
    ("Cura personale", "expense"),
    ("Commissioni bancarie", "expense"),
    ("Altro", "expense"),
    ("Stipendio", "income"),
    ("Altre entrate", "income"),
    ("Giroconti", "transfer"),
    ("Risparmi e investimenti", "saving"),
];

/// Seed rules for common Italian merchants. (pattern, category)
pub const DEFAULT_RULES: &[(&str, &str)] = &[
    ("esselunga", "Spesa alimentare"),
    ("conad", "Spesa alimentare"),
    ("coop", "Spesa alimentare"),
    ("lidl", "Spesa alimentare"),
    ("carrefour", "Spesa alimentare"),
    ("eurospin", "Spesa alimentare"),
    (" pam ", "Spesa alimentare"),
    ("aldi", "Spesa alimentare"),
    ("despar", "Spesa alimentare"),
    (" iper", "Spesa alimentare"),
    ("mcdonald", "Ristoranti e bar"),
    ("burger king", "Ristoranti e bar"),
    ("glovo", "Ristoranti e bar"),
    ("deliveroo", "Ristoranti e bar"),
    ("just eat", "Ristoranti e bar"),
    ("trenitalia", "Trasporti"),
    ("italo", "Trasporti"),
    ("uber", "Trasporti"),
    ("uber eats", "Ristoranti e bar"),
    ("telepass", "Trasporti"),
    ("autostrade", "Trasporti"),
    (" eni ", "Carburante"),
    ("enilive", "Carburante"),
    ("q8", "Carburante"),
    ("tamoil", "Carburante"),
    (" esso ", "Carburante"),
    ("enel", "Utenze"),
    ("hera", "Utenze"),
    ("a2a", "Utenze"),
    ("iren", "Utenze"),
    (" tim ", "Telefono e internet"),
    ("vodafone", "Telefono e internet"),
    ("windtre", "Telefono e internet"),
    ("iliad", "Telefono e internet"),
    ("fastweb", "Telefono e internet"),
    ("netflix", "Abbonamenti"),
    ("spotify", "Abbonamenti"),
    ("disney", "Abbonamenti"),
    ("dazn", "Abbonamenti"),
    ("prime video", "Abbonamenti"),
    ("apple.com", "Abbonamenti"),
    ("openai", "Abbonamenti"),
    ("anthropic", "Abbonamenti"),
    ("farmacia", "Salute"),
    ("amazon", "Shopping"),
    ("zalando", "Shopping"),
    ("ikea", "Casa"),
    ("booking.com", "Viaggi"),
    ("ryanair", "Viaggi"),
    ("airbnb", "Viaggi"),
    ("f24", "Tasse"),
    ("commissione", "Commissioni bancarie"),
    ("canone", "Commissioni bancarie"),
    ("stipendio", "Stipendio"),
    ("emolumenti", "Stipendio"),
];

pub fn open(path: &Path) -> Result<Connection> {
    let fresh = !path.exists();
    let mut conn =
        Connection::open(path).with_context(|| format!("apertura database {}", path.display()))?;
    if fresh {
        crate::config::restrict_permissions(path, 0o600);
    }
    init(&mut conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    init(&mut conn)?;
    Ok(conn)
}

fn init(conn: &mut Connection) -> Result<()> {
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let version = usize::try_from(version).unwrap_or(0);
    if version > MIGRATIONS.len() {
        bail!("il database è di una versione più recente di fine: aggiorna il programma");
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        if i == 0 {
            seed(&tx)?;
        }
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

fn seed(conn: &Connection) -> Result<()> {
    for (name, kind) in DEFAULT_CATEGORIES {
        conn.execute(
            "INSERT INTO categories (name, kind) VALUES (?1, ?2)",
            params![name, kind],
        )?;
    }
    for (pattern, cat) in DEFAULT_RULES {
        conn.execute(
            "INSERT INTO rules (pattern, category_id, source)
             SELECT ?1, id, 'seed' FROM categories WHERE name = ?2",
            params![pattern, cat],
        )?;
    }
    Ok(())
}

pub fn category_id(conn: &Connection, name: &str) -> Result<i64> {
    conn.query_row(
        "SELECT id FROM categories WHERE name = ?1",
        [name.trim()],
        |r| r.get(0),
    )
    .optional()?
    .with_context(|| {
        format!("categoria inesistente: {name:?} (usa list_categories o crea la categoria)")
    })
}

pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}

pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

pub fn savings_goal_cents(conn: &Connection) -> Result<i64> {
    Ok(get_setting(conn, "savings_goal_cents")?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0))
}

#[cfg(test)]
mod tests {
    #[test]
    fn migrates_and_seeds() {
        let conn = super::open_in_memory().unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM rules", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n as usize, super::DEFAULT_RULES.len());
        assert!(super::category_id(&conn, "spesa alimentare").is_ok());
    }
}
