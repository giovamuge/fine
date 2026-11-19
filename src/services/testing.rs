//! Fixtures for service tests: transactions inserted straight into an in-memory store.

use rusqlite::params;

use crate::storage::Store;

fn ensure_account(store: &Store) {
    store
        .conn()
        .execute(
            "INSERT OR IGNORE INTO accounts (uid, currency) VALUES ('a', 'EUR')",
            [],
        )
        .unwrap();
}

/// An EUR transaction on account 'a' in `category` (by name). Returns its id.
pub fn add_tx(
    store: &Store,
    date: &str,
    cents: i64,
    merchant: &str,
    category: Option<&str>,
) -> i64 {
    ensure_account(store);
    let cat = category.map(|c| store.db().category_id(c).unwrap());
    store
        .conn()
        .execute(
            "INSERT INTO transactions (account_uid, booking_date, amount_cents, currency, counterparty, merchant, dedup_key, category_id)
             VALUES ('a', ?1, ?2, 'EUR', ?3, ?3, lower(hex(randomblob(8))), ?4)",
            params![date, cents, merchant, cat],
        )
        .unwrap();
    store.conn().last_insert_rowid()
}

/// An uncategorized transaction with a given id and counterparty.
pub fn add_tx_with_id(store: &Store, id: i64, merchant: &str, cents: i64, date: &str) {
    ensure_account(store);
    store
        .conn()
        .execute(
            "INSERT INTO transactions (id, account_uid, booking_date, amount_cents, currency, counterparty, merchant, dedup_key)
             VALUES (?1, 'a', ?4, ?3, 'EUR', ?2, ?2, ?1)",
            params![id, merchant, cents, date],
        )
        .unwrap();
}

/// An uncategorized expense described like a bank would (merchant derived from it).
pub fn insert_described(store: &Store, id: i64, description: &str) {
    ensure_account(store);
    let merchant = crate::domain::text::merchant_key(None, Some(description));
    store
        .conn()
        .execute(
            "INSERT INTO transactions (id, account_uid, booking_date, amount_cents, currency, description, merchant, dedup_key)
             VALUES (?1, 'a', '2026-01-01', -1000, 'EUR', ?2, ?3, ?1)",
            params![id, description, merchant],
        )
        .unwrap();
}

pub fn category_of(store: &Store, id: i64) -> Option<String> {
    store
        .conn()
        .query_row(
            "SELECT c.name FROM transactions t LEFT JOIN categories c ON c.id = t.category_id WHERE t.id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap()
}

pub fn tags_of(store: &Store, id: i64) -> Vec<String> {
    let mut st = store
        .conn()
        .prepare(
            "SELECT g.name FROM transaction_tags tt JOIN tags g ON g.id = tt.tag_id
             WHERE tt.transaction_id = ?1 ORDER BY g.name",
        )
        .unwrap();
    st.query_map([id], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

pub fn tag(store: &mut Store, tx: i64, t: &str) {
    crate::services::tags::tag(store, &[tx], &[t.to_owned()], false, None).unwrap();
}
