//! Bank connections, shared by the CLI and the dashboard: connect in two steps (open the
//! bank's page, then paste back the final address), renew a consent without duplicating
//! the history, rename and remove accounts.

use anyhow::{Context, Result, bail};
use chrono::{Days, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::config::EnableBankingConfig;
use crate::enablebanking::{AccountResource, Client};

#[derive(Debug, Serialize)]
pub struct PendingConnect {
    /// The bank's authorization page.
    pub url: String,
    /// Where the bank sends the user at the end: its full address must be pasted back.
    pub redirect: String,
}

/// Step 1: create the authorization request. Max consent duration depends on the bank.
/// (`&mut` keeps the future `Send`: a shared `&Connection` is not.)
pub async fn start(
    conn: &mut Connection,
    eb: &EnableBankingConfig,
    bank: &str,
    country: &str,
    days: u64,
) -> Result<PendingConnect> {
    let client = Client::new(eb)?;
    let redirect = client.redirect_url(&eb.redirect_url).await?;
    let state = uuid::Uuid::new_v4().to_string();
    let valid_until =
        (Utc::now() + Days::new(days)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let auth = client
        .start_auth(bank, country, &valid_until, &redirect, &state)
        .await?;
    conn.execute(
        "DELETE FROM pending_auth WHERE created_at < datetime('now', '-1 day')",
        [],
    )?;
    conn.execute(
        "INSERT INTO pending_auth (state, aspsp, country) VALUES (?1, ?2, ?3)",
        params![state, bank, country],
    )?;
    Ok(PendingConnect {
        url: auth.url,
        redirect,
    })
}

#[derive(Debug, Serialize)]
pub struct ConnectedAccount {
    pub name: Option<String>,
    pub iban: Option<String>,
    pub currency: String,
    /// The account was already known: its consent was renewed and its history kept.
    pub renewed: bool,
}

/// Step 2: the address the bank redirected to (it carries `code` and `state`).
pub async fn finish(
    conn: &mut Connection,
    eb: &EnableBankingConfig,
    redirected_to: &str,
) -> Result<Vec<ConnectedAccount>> {
    let url = reqwest::Url::parse(redirected_to.trim()).context("indirizzo non valido")?;
    let get = |k: &str| {
        url.query_pairs()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.into_owned())
    };
    if let Some(err) = get("error") {
        bail!(
            "la banca ha rifiutato l'autorizzazione: {err} {}",
            get("error_description").unwrap_or_default()
        );
    }
    let state = get("state").context("parametro state mancante nell'indirizzo")?;
    let (bank, country): (String, String) = conn
        .query_row(
            "SELECT aspsp, country FROM pending_auth WHERE state = ?1",
            [&state],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .context(
            "l'indirizzo non corrisponde a nessun collegamento in corso (o è già stato usato)",
        )?;
    let code = get("code").context("parametro code mancante nell'indirizzo")?;
    let session = Client::new(eb)?.create_session(&code).await?;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM pending_auth WHERE state = ?1", [&state])?;
    let mut out = vec![];
    for a in &session.accounts {
        let renewed = store_account(
            &tx,
            a,
            &bank,
            &country,
            &session.session_id,
            &session.access.valid_until,
        )?;
        out.push(ConnectedAccount {
            name: a.name.clone(),
            iban: a.account_id.as_ref().and_then(|i| i.iban.clone()),
            currency: a.currency.clone(),
            renewed,
        });
    }
    tx.commit()?;
    Ok(out)
}

/// Insert or update an account. A renewed consent gives new account uids: the account is
/// recognized (identification hash, else bank + IBAN/name + currency) and its history is
/// moved to the new uid, so nothing is duplicated. Returns true if it was already known.
pub fn store_account(
    conn: &Connection,
    a: &AccountResource,
    bank: &str,
    country: &str,
    session_id: &str,
    valid_until: &str,
) -> Result<bool> {
    let iban = a.account_id.as_ref().and_then(|i| i.iban.clone());
    let existing: Option<String> = conn
        .query_row(
            "SELECT uid FROM accounts
             WHERE uid = ?1
                OR (?2 IS NOT NULL AND identification_hash = ?2)
                OR (aspsp = ?3 AND currency = ?4 AND (
                      (?5 IS NOT NULL AND iban = ?5)
                   OR (?5 IS NULL AND iban IS NULL AND COALESCE(name, '') = COALESCE(?6, ''))))
             ORDER BY uid = ?1 DESC, identification_hash IS ?2 DESC LIMIT 1",
            params![a.uid, a.identification_hash, bank, a.currency, iban, a.name],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(old) = existing.as_deref().filter(|old| *old != a.uid) {
        rekey(conn, old, &a.uid)?;
    }
    conn.execute(
        "INSERT INTO accounts (uid, iban, name, currency, aspsp, session_id, valid_until, identification_hash, country)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(uid) DO UPDATE SET iban = excluded.iban, name = excluded.name, currency = excluded.currency,
           aspsp = excluded.aspsp, session_id = excluded.session_id, valid_until = excluded.valid_until,
           identification_hash = COALESCE(excluded.identification_hash, identification_hash),
           country = excluded.country",
        params![
            a.uid,
            iban,
            a.name,
            a.currency,
            bank,
            session_id,
            valid_until,
            a.identification_hash,
            country
        ],
    )?;
    Ok(existing.is_some())
}

/// Move an account (with transactions and sync history) to a new uid.
fn rekey(conn: &Connection, old: &str, new: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO accounts (uid, iban, name, currency, label, aspsp, session_id, valid_until,
                               balance_cents, balance_at, created_at, balance_currency, identification_hash, country)
         SELECT ?2, iban, name, currency, label, aspsp, session_id, valid_until,
                balance_cents, balance_at, created_at, balance_currency, identification_hash, country
         FROM accounts WHERE uid = ?1",
        params![old, new],
    )?;
    conn.execute(
        "UPDATE transactions SET account_uid = ?2, dedup_key = ?2 || substr(dedup_key, length(?1) + 1)
         WHERE account_uid = ?1",
        params![old, new],
    )?;
    conn.execute(
        "UPDATE sync_log SET account_uid = ?2 WHERE account_uid = ?1",
        params![old, new],
    )?;
    conn.execute("DELETE FROM accounts WHERE uid = ?1", [old])?;
    Ok(())
}

/// Custom display name for an account (None or empty = automatic).
pub fn rename(conn: &Connection, uid: &str, label: Option<&str>) -> Result<()> {
    let label = label.map(str::trim).filter(|l| !l.is_empty());
    if conn.execute(
        "UPDATE accounts SET label = ?2 WHERE uid = ?1",
        params![uid, label],
    )? == 0
    {
        bail!("conto inesistente");
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct RemoveOutcome {
    pub transactions_deleted: usize,
    /// The consent was revoked at the bank too (when no other account used it).
    pub consent_revoked: bool,
}

/// Remove an account with all its transactions (tags, amortizations and sync history too).
/// When no other account uses the same consent, it is revoked at the bank (best effort).
pub async fn remove(
    conn: &mut Connection,
    eb: Option<&EnableBankingConfig>,
    uid: &str,
) -> Result<RemoveOutcome> {
    let (transactions_deleted, orphan_session) = delete_local(conn, uid)?;
    let mut consent_revoked = false;
    if let (Some(session), Some(eb)) = (orphan_session, eb) {
        consent_revoked = match Client::new(eb) {
            Ok(c) => c.delete_session(&session).await.is_ok(),
            Err(_) => false,
        };
    }
    Ok(RemoveOutcome {
        transactions_deleted,
        consent_revoked,
    })
}

/// Delete the account locally. Returns the number of transactions deleted and its consent
/// (session) if no other account still uses it.
fn delete_local(conn: &mut Connection, uid: &str) -> Result<(usize, Option<String>)> {
    let session: Option<String> = conn
        .query_row(
            "SELECT session_id FROM accounts WHERE uid = ?1",
            [uid],
            |r| r.get(0),
        )
        .optional()?
        .context("conto inesistente")?;
    let tx = conn.transaction()?;
    let deleted = tx.execute("DELETE FROM transactions WHERE account_uid = ?1", [uid])?;
    tx.execute("DELETE FROM sync_log WHERE account_uid = ?1", [uid])?;
    tx.execute("DELETE FROM accounts WHERE uid = ?1", [uid])?;
    let orphan = match session {
        Some(s) => {
            let used: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM accounts WHERE session_id = ?1)",
                [&s],
                |r| r.get(0),
            )?;
            (!used).then_some(s)
        }
        None => None,
    };
    tx.commit()?;
    Ok((deleted, orphan))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn account(uid: &str, hash: Option<&str>, iban: Option<&str>, cur: &str) -> AccountResource {
        serde_json::from_value(serde_json::json!({
            "uid": uid,
            "identification_hash": hash,
            "account_id": { "iban": iban },
            "name": "MARIO ROSSI",
            "currency": cur,
        }))
        .unwrap()
    }

    fn add_tx(conn: &Connection, uid: &str, key: &str) {
        conn.execute(
            "INSERT INTO transactions (account_uid, booking_date, amount_cents, currency, merchant, dedup_key)
             VALUES (?1, '2026-01-01', -100, 'EUR', 'x', ?1 || ?2)",
            params![uid, key],
        )
        .unwrap();
    }

    #[test]
    fn renewal_keeps_history_without_duplicates() {
        let conn = db::open_in_memory().unwrap();
        // Old accounts had no identification hash: matched by bank + IBAN + currency.
        let eur = account("old-eur", None, Some("IT00X"), "EUR");
        let usd = account("old-usd", None, Some("IT00X"), "USD");
        assert!(!store_account(&conn, &eur, "Banca", "IT", "s1", "2026-06-01").unwrap());
        assert!(!store_account(&conn, &usd, "Banca", "IT", "s1", "2026-06-01").unwrap());
        rename(&conn, "old-eur", Some("Conto principale")).unwrap();
        add_tx(&conn, "old-eur", "|ref|1");
        add_tx(&conn, "old-usd", "|ref|9");

        let renewed = account("new-eur", Some("h1"), Some("IT00X"), "EUR");
        assert!(store_account(&conn, &renewed, "Banca", "IT", "s2", "2026-12-01").unwrap());

        let (label, session, hash): (String, String, String) = conn
            .query_row(
                "SELECT label, session_id, identification_hash FROM accounts WHERE uid = 'new-eur'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (label.as_str(), session.as_str(), hash.as_str()),
            ("Conto principale", "s2", "h1")
        );
        let key: String = conn
            .query_row(
                "SELECT dedup_key FROM transactions WHERE account_uid = 'new-eur'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(key, "new-eur|ref|1"); // a re-download of the same transaction is deduplicated
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 2); // the USD account is a different account, untouched

        // Later renewals match by hash.
        let again = account("newer-eur", Some("h1"), Some("IT00X"), "EUR");
        assert!(store_account(&conn, &again, "Banca", "IT", "s3", "2027-06-01").unwrap());
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transactions WHERE account_uid = 'newer-eur'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[tokio::test]
    async fn remove_deletes_account_and_its_data() {
        let mut conn = db::open_in_memory().unwrap();
        let a = account("a1", None, Some("IT00Y"), "EUR");
        store_account(&conn, &a, "Banca", "IT", "s1", "2026-06-01").unwrap();
        let b = account("b1", None, Some("IT00Z"), "EUR");
        store_account(&conn, &b, "Banca", "IT", "s1", "2026-06-01").unwrap();
        add_tx(&conn, "a1", "|ref|1");
        add_tx(&conn, "a1", "|ref|2");
        add_tx(&conn, "b1", "|ref|1");
        let out = remove(&mut conn, None, "a1").await.unwrap();
        assert_eq!(out.transactions_deleted, 2);
        assert!(!out.consent_revoked);
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 1);
        assert!(remove(&mut conn, None, "a1").await.is_err());
    }
}
