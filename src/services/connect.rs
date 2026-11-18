//! Bank connections, shared by the CLI and the dashboard: connect in two steps (open the
//! bank's page, then paste back the final address), renew a consent without duplicating
//! the history, rename and remove accounts.

use chrono::{Days, SecondsFormat, Utc};
use serde::Serialize;

use crate::bank::Client;
use crate::config::EnableBankingConfig;
use crate::error::{Error, Result};
use crate::storage::{NewAccount, Store};
use crate::tr;

#[derive(Debug, Serialize)]
pub struct PendingConnect {
    /// The bank's authorization page.
    pub url: String,
    /// Where the bank sends the user at the end: its full address must be pasted back.
    pub redirect: String,
}

/// Step 1: create the authorization request. Max consent duration depends on the bank.
pub async fn start(
    store: &mut Store,
    eb: &EnableBankingConfig,
    bank: &str,
    country: &str,
    days: u64,
) -> Result<PendingConnect> {
    let client = Client::new(eb)?;
    let redirect = client.redirect_url(&eb.redirect_url).await?;
    let state = uuid::Uuid::new_v4().to_string();
    let valid_until = (Utc::now() + Days::new(days)).to_rfc3339_opts(SecondsFormat::Secs, true);
    let auth = client
        .start_auth(bank, country, &valid_until, &redirect, &state)
        .await?;
    store.db().save_pending_auth(&state, bank, country)?;
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
    store: &mut Store,
    eb: &EnableBankingConfig,
    redirected_to: &str,
) -> Result<Vec<ConnectedAccount>> {
    let url = reqwest::Url::parse(redirected_to.trim()).map_err(|e| {
        Error::invalid(format!(
            "{}: {e}",
            tr!("indirizzo non valido", "invalid address")
        ))
    })?;
    let param = |k: &str| {
        url.query_pairs()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.into_owned())
    };
    if let Some(err) = param("error") {
        let detail = param("error_description").unwrap_or_default();
        crate::invalid!(
            "{}",
            tr!(
                "la banca ha rifiutato l'autorizzazione: {err} {detail}",
                "the bank refused the authorization: {err} {detail}"
            )
        );
    }
    let state = param("state").ok_or_else(|| {
        Error::invalid(tr!(
            "parametro state mancante nell'indirizzo",
            "the address has no state parameter"
        ))
    })?;
    let (bank, country) = store.db().pending_auth(&state)?.ok_or_else(|| {
        Error::invalid(tr!(
            "l'indirizzo non corrisponde a nessun collegamento in corso (o è già stato usato)",
            "the address doesn't match any connection in progress (or it was already used)"
        ))
    })?;
    let code = param("code").ok_or_else(|| {
        Error::invalid(tr!(
            "parametro code mancante nell'indirizzo",
            "the address has no code parameter"
        ))
    })?;
    let session = Client::new(eb)?.create_session(&code).await?;
    store.write(|db| {
        db.delete_pending_auth(&state)?;
        session
            .accounts
            .iter()
            .map(|a| {
                let renewed = db.store_account(&NewAccount {
                    uid: &a.uid,
                    identification_hash: a.identification_hash.as_deref(),
                    iban: a.iban(),
                    name: a.name.as_deref(),
                    currency: &a.currency,
                    bank: &bank,
                    country: &country,
                    session_id: &session.session_id,
                    valid_until: &session.access.valid_until,
                })?;
                Ok(ConnectedAccount {
                    name: a.name.clone(),
                    iban: a.iban().map(str::to_owned),
                    currency: a.currency.clone(),
                    renewed,
                })
            })
            .collect()
    })
}

/// Custom display name for an account (None or empty = automatic).
pub fn rename(store: &mut Store, uid: &str, label: Option<&str>) -> Result<()> {
    store.db().rename_account(uid, label)
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
    store: &mut Store,
    eb: Option<&EnableBankingConfig>,
    uid: &str,
) -> Result<RemoveOutcome> {
    let (transactions_deleted, orphan_session) = store.write(|db| db.delete_account(uid))?;
    let consent_revoked = match (orphan_session, eb) {
        (Some(session), Some(eb)) => match Client::new(eb) {
            Ok(c) => c.delete_session(&session).await.is_ok(),
            Err(_) => false,
        },
        _ => false,
    };
    Ok(RemoveOutcome {
        transactions_deleted,
        consent_revoked,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank::AccountResource;
    use rusqlite::params;

    fn resource(uid: &str, hash: Option<&str>, iban: Option<&str>, cur: &str) -> AccountResource {
        serde_json::from_value(serde_json::json!({
            "uid": uid,
            "identification_hash": hash,
            "account_id": { "iban": iban },
            "name": "MARIO ROSSI",
            "currency": cur,
        }))
        .unwrap()
    }

    fn store_account(store: &Store, a: &AccountResource, session: &str, valid_until: &str) -> bool {
        store
            .db()
            .store_account(&NewAccount {
                uid: &a.uid,
                identification_hash: a.identification_hash.as_deref(),
                iban: a.iban(),
                name: a.name.as_deref(),
                currency: &a.currency,
                bank: "Banca",
                country: "IT",
                session_id: session,
                valid_until,
            })
            .unwrap()
    }

    fn add_tx(store: &Store, uid: &str, key: &str) {
        store
            .conn()
            .execute(
                "INSERT INTO transactions (account_uid, booking_date, amount_cents, currency, merchant, dedup_key)
                 VALUES (?1, '2026-01-01', -100, 'EUR', 'x', ?1 || ?2)",
                params![uid, key],
            )
            .unwrap();
    }

    fn count(store: &Store, sql: &str) -> i64 {
        store.conn().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn renewal_keeps_history_without_duplicates() {
        let mut store = Store::open_in_memory().unwrap();
        // Old accounts had no identification hash: matched by bank + IBAN + currency.
        let eur = resource("old-eur", None, Some("IT00X"), "EUR");
        let usd = resource("old-usd", None, Some("IT00X"), "USD");
        assert!(!store_account(&store, &eur, "s1", "2026-06-01"));
        assert!(!store_account(&store, &usd, "s1", "2026-06-01"));
        rename(&mut store, "old-eur", Some("Conto principale")).unwrap();
        add_tx(&store, "old-eur", "|ref|1");
        add_tx(&store, "old-usd", "|ref|9");

        let renewed = resource("new-eur", Some("h1"), Some("IT00X"), "EUR");
        assert!(store_account(&store, &renewed, "s2", "2026-12-01"));

        let (label, session, hash): (String, String, String) = store
            .conn()
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
        let key: String = store
            .conn()
            .query_row(
                "SELECT dedup_key FROM transactions WHERE account_uid = 'new-eur'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(key, "new-eur|ref|1"); // a re-download of the same transaction is deduplicated
        assert_eq!(count(&store, "SELECT COUNT(*) FROM accounts"), 2); // the USD account is untouched

        // Later renewals match by hash.
        let again = resource("newer-eur", Some("h1"), Some("IT00X"), "EUR");
        assert!(store_account(&store, &again, "s3", "2027-06-01"));
        assert_eq!(
            count(
                &store,
                "SELECT COUNT(*) FROM transactions WHERE account_uid = 'newer-eur'"
            ),
            1
        );
    }

    #[tokio::test]
    async fn remove_deletes_account_and_its_data() {
        let mut store = Store::open_in_memory().unwrap();
        store_account(
            &store,
            &resource("a1", None, Some("IT00Y"), "EUR"),
            "s1",
            "2026-06-01",
        );
        store_account(
            &store,
            &resource("b1", None, Some("IT00Z"), "EUR"),
            "s1",
            "2026-06-01",
        );
        add_tx(&store, "a1", "|ref|1");
        add_tx(&store, "a1", "|ref|2");
        add_tx(&store, "b1", "|ref|1");
        let out = remove(&mut store, None, "a1").await.unwrap();
        assert_eq!(out.transactions_deleted, 2);
        assert!(!out.consent_revoked);
        assert_eq!(count(&store, "SELECT COUNT(*) FROM transactions"), 1);
        assert!(remove(&mut store, None, "a1").await.is_err());
    }
}
