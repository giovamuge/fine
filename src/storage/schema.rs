//! Schema migrations, tracked with `PRAGMA user_version`. Each step runs in its own transaction,
//! with the data changes it needs right after its SQL.

use rusqlite::{Connection, params};

use super::Db;
use super::seed::{DEFAULT_CATEGORIES, DEFAULT_RULES};
use crate::error::Result;

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
    // v6: customizable categories (icon, color); the missing defaults are added by `seed_v6`
    "ALTER TABLE categories ADD COLUMN icon TEXT;
     ALTER TABLE categories ADD COLUMN color TEXT;",
    // v7: dynamic budgets (see budgets.rs): named, with a JSON definition (rule, period, options).
    // The old per-category budgets are converted by `migrate_budgets_v7`.
    r#"
CREATE TABLE budgets_v7 (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
    definition TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
"#,
    // v8: a PayPal purchase records the bank debit that paid for it (see reconcile.rs);
    // the existing history is reconciled right after the migration
    "ALTER TABLE transactions ADD COLUMN funded_by INTEGER REFERENCES transactions(id) ON DELETE SET NULL;",
    // v9: ECB reference rates (units of `currency` per 1 EUR), one row per published business day
    r#"
CREATE TABLE fx_rates (
    currency TEXT NOT NULL,
    date     TEXT NOT NULL,
    rate     REAL NOT NULL CHECK (rate > 0),
    PRIMARY KEY (currency, date)
) WITHOUT ROWID;
"#,
];

/// Data changes that go with a schema version (index = position in `MIGRATIONS`).
fn after(version: usize, db: Db<'_>) -> Result<()> {
    match version {
        0 => seed(db),
        5 => add_missing_defaults(db),
        6 => convert_budgets_v7(db),
        // v8 added `funded_by`; reconciliation also converts currencies, so it runs once the
        // exchange-rate table (v9) exists.
        8 => db.reconcile_paypal().map(drop),
        _ => Ok(()),
    }
}

pub(super) fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let version = usize::try_from(version).unwrap_or(0);
    if version > MIGRATIONS.len() {
        crate::invalid!("il database è di una versione più recente di fine: aggiorna il programma");
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        after(i, Db { conn: &tx })?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)?;
        tx.commit()?;
    }
    Ok(())
}

fn seed(db: Db<'_>) -> Result<()> {
    for (name, kind, _) in DEFAULT_CATEGORIES {
        db.exec(
            "INSERT INTO categories (name, kind) VALUES (?1, ?2)",
            params![name, kind],
        )?;
    }
    for (pattern, category) in DEFAULT_RULES {
        db.exec(
            "INSERT INTO rules (pattern, category_id, source)
             SELECT ?1, id, 'seed' FROM categories WHERE name = ?2",
            params![pattern, category],
        )?;
    }
    Ok(())
}

/// Adds the default categories an older database lacks, gives every default its icon and lets
/// the AI look again at what it could not place before (the new categories may fit).
fn add_missing_defaults(db: Db<'_>) -> Result<()> {
    for (name, kind, icon) in DEFAULT_CATEGORIES {
        db.exec(
            "INSERT INTO categories (name, kind) VALUES (?1, ?2) ON CONFLICT(name) DO NOTHING",
            params![name, kind],
        )?;
        db.exec(
            "UPDATE categories SET icon = ?2 WHERE name = ?1 AND icon IS NULL",
            params![name, icon],
        )?;
    }
    for (pattern, category) in DEFAULT_RULES {
        db.exec(
            "INSERT INTO rules (pattern, category_id, source)
             SELECT ?1, id, 'seed' FROM categories WHERE name = ?2
             ON CONFLICT(pattern) DO NOTHING",
            params![pattern, category],
        )?;
    }
    db.apply_category_rules()?;
    db.reset_ai_checked()?;
    Ok(())
}

/// Old budgets (category, week/month/year, amount) become named dynamic budgets.
fn convert_budgets_v7(db: Db<'_>) -> Result<()> {
    let old: Vec<(String, String, i64, i64)> = db.rows(
        "SELECT c.name, b.period, b.amount_cents,
                (SELECT COUNT(*) FROM budgets o WHERE o.category_id = b.category_id)
         FROM budgets b JOIN categories c ON c.id = b.category_id",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    for (category, period, cents, same_category) in old {
        let name = if same_category > 1 {
            let label = match period.as_str() {
                "week" => "settimana",
                "year" => "anno",
                _ => "mese",
            };
            format!("{category} ({label})")
        } else {
            category.clone()
        };
        let definition = serde_json::json!({
            "name": name,
            "goal": "max_spend",
            "rule": { "category": [category] },
            "period": { "type": period },
            "amount": cents as f64 / 100.0,
            "alerts": [100],
        });
        db.exec(
            "INSERT INTO budgets_v7 (name, definition) VALUES (?1, ?2)",
            params![name, definition.to_string()],
        )?;
    }
    db.conn
        .execute_batch("DROP TABLE budgets; ALTER TABLE budgets_v7 RENAME TO budgets;")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::testing::d;
    use crate::storage::Store;

    #[test]
    fn migrates_and_seeds() {
        let store = Store::open_in_memory().unwrap();
        let n: i64 = store.db().value("SELECT COUNT(*) FROM rules", []).unwrap();
        assert_eq!(n as usize, DEFAULT_RULES.len());
        assert!(store.db().category_id("spesa alimentare").is_ok());
    }

    #[test]
    fn v7_converts_old_budgets() {
        let mut conn = Connection::open_in_memory().unwrap();
        for (i, sql) in MIGRATIONS.iter().enumerate().take(6) {
            conn.execute_batch(sql).unwrap();
            after(i, Db { conn: &conn }).unwrap();
        }
        conn.pragma_update(None, "user_version", 6).unwrap();
        conn.execute_batch(
            "INSERT INTO budgets SELECT id, 'month', 30000 FROM categories WHERE name = 'Ristoranti e bar';
             INSERT INTO budgets SELECT id, 'week', 8000 FROM categories WHERE name = 'Ristoranti e bar';
             INSERT INTO budgets SELECT id, 'year', 50000 FROM categories WHERE name = 'Viaggi';",
        )
        .unwrap();
        migrate(&mut conn).unwrap();
        let store = Store { conn };
        let all = crate::services::budgets::statuses(&store, d("2026-11-10")).unwrap();
        let names: Vec<&str> = all.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Ristoranti e bar (mese)",
                "Ristoranti e bar (settimana)",
                "Viaggi"
            ]
        );
        assert_eq!(all[0].amount, 300.0);
        assert_eq!(all[2].period_label, "ogni anno");
        assert_eq!(all[2].rule_summary, "categoria Viaggi");
    }
}
