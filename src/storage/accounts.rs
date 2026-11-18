//! Accounts, their balances, consents and pending authorizations.

use std::collections::{BTreeMap, HashSet};

use chrono::{NaiveDate, Utc};
use rusqlite::params;
use serde::Serialize;

use super::{ACCOUNT_LABEL, ACCOUNT_MATCH_TEXT, Db};
use crate::domain::assistant::AccountHealth;
use crate::domain::balance::AccountLedger;
use crate::domain::{BASE_CURRENCY, Money};
use crate::error::{Error, Result};

#[derive(Debug, Serialize)]
pub struct AccountView {
    pub uid: String,
    pub label: String,
    /// Name chosen by the user (None = automatic label).
    pub custom_label: Option<String>,
    pub bank: Option<String>,
    pub country: Option<String>,
    pub iban: Option<String>,
    pub currency: String,
    pub balance: Option<Money>,
    pub balance_currency: Option<String>,
    pub balance_at: Option<String>,
    pub consent_valid_until: Option<String>,
    pub transactions: i64,
    pub last_sync_at: Option<String>,
    pub last_sync_ok: Option<bool>,
    pub last_sync_error: Option<String>,
}

/// An account as a new consent describes it.
#[derive(Debug)]
pub struct NewAccount<'a> {
    pub uid: &'a str,
    pub identification_hash: Option<&'a str>,
    pub iban: Option<&'a str>,
    pub name: Option<&'a str>,
    pub currency: &'a str,
    pub bank: &'a str,
    pub country: &'a str,
    pub session_id: &'a str,
    pub valid_until: &'a str,
}

impl Db<'_> {
    pub fn list_accounts(&self) -> Result<Vec<AccountView>> {
        let sql = format!(
            "SELECT a.uid, {ACCOUNT_LABEL}, a.label, a.aspsp, a.country, a.iban, a.currency, a.balance_cents,
                    COALESCE(a.balance_currency, a.currency), a.balance_at, a.valid_until,
                    (SELECT COUNT(*) FROM transactions t WHERE t.account_uid = a.uid),
                    l.at, l.ok, l.message
             FROM accounts a
             LEFT JOIN sync_log l ON l.id = (SELECT MAX(id) FROM sync_log WHERE account_uid = a.uid)
             ORDER BY a.aspsp, a.created_at, a.uid"
        );
        self.rows(&sql, [], |r| {
            let balance: Option<Money> = r.get(7)?;
            Ok(AccountView {
                uid: r.get(0)?,
                label: r.get(1)?,
                custom_label: r.get(2)?,
                bank: r.get(3)?,
                country: r.get(4)?,
                iban: r.get(5)?,
                currency: r.get(6)?,
                balance,
                balance_currency: balance.and(r.get(8)?),
                balance_at: r.get(9)?,
                consent_valid_until: r.get(10)?,
                transactions: r.get(11)?,
                last_sync_at: r.get(12)?,
                last_sync_ok: r.get::<_, Option<i64>>(13)?.map(|v| v == 1),
                last_sync_error: r.get(14)?,
            })
        })
    }

    /// (uid, label) of every account, in the order they are synced.
    pub fn account_labels(&self) -> Result<Vec<(String, String)>> {
        let sql =
            format!("SELECT a.uid, {ACCOUNT_LABEL} FROM accounts a ORDER BY a.created_at, a.uid");
        self.rows(&sql, [], |r| Ok((r.get(0)?, r.get(1)?)))
    }

    /// Lowercase "label name bank iban" of every account (budget rules on accounts).
    pub fn account_match_texts(&self) -> Result<Vec<String>> {
        let sql = format!("SELECT {ACCOUNT_MATCH_TEXT} FROM accounts a");
        self.rows(&sql, [], |r| r.get(0))
    }

    pub fn account_count(&self) -> Result<i64> {
        self.value("SELECT COUNT(*) FROM accounts", [])
    }

    /// IBANs of the user's accounts (without spaces, uppercase): transfers between them.
    pub fn own_ibans(&self) -> Result<HashSet<String>> {
        let ibans = self.rows(
            "SELECT iban FROM accounts WHERE iban IS NOT NULL",
            [],
            |r| r.get::<_, String>(0),
        )?;
        Ok(ibans
            .into_iter()
            .map(|s| s.replace(' ', "").to_uppercase())
            .collect())
    }

    /// Insert or update an account. A renewed consent gives new account uids: the account is
    /// recognized (identification hash, else bank + IBAN/name + currency) and its history is
    /// moved to the new uid, so nothing is duplicated. Returns true if it was already known.
    pub fn store_account(&self, a: &NewAccount<'_>) -> Result<bool> {
        let existing: Option<String> = self.row(
            "SELECT uid FROM accounts
             WHERE uid = ?1
                OR (?2 IS NOT NULL AND identification_hash = ?2)
                OR (aspsp = ?3 AND currency = ?4 AND (
                      (?5 IS NOT NULL AND iban = ?5)
                   OR (?5 IS NULL AND iban IS NULL AND COALESCE(name, '') = COALESCE(?6, ''))))
             ORDER BY uid = ?1 DESC, identification_hash IS ?2 DESC LIMIT 1",
            params![
                a.uid,
                a.identification_hash,
                a.bank,
                a.currency,
                a.iban,
                a.name
            ],
            |r| r.get(0),
        )?;
        if let Some(old) = existing.as_deref().filter(|old| *old != a.uid) {
            self.rekey_account(old, a.uid)?;
        }
        self.exec(
            "INSERT INTO accounts (uid, iban, name, currency, aspsp, session_id, valid_until, identification_hash, country)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(uid) DO UPDATE SET iban = excluded.iban, name = excluded.name, currency = excluded.currency,
               aspsp = excluded.aspsp, session_id = excluded.session_id, valid_until = excluded.valid_until,
               identification_hash = COALESCE(excluded.identification_hash, identification_hash),
               country = excluded.country",
            params![
                a.uid,
                a.iban,
                a.name,
                a.currency,
                a.bank,
                a.session_id,
                a.valid_until,
                a.identification_hash,
                a.country
            ],
        )?;
        Ok(existing.is_some())
    }

    /// Move an account (with transactions and sync history) to a new uid.
    fn rekey_account(&self, old: &str, new: &str) -> Result<()> {
        self.exec(
            "INSERT INTO accounts (uid, iban, name, currency, label, aspsp, session_id, valid_until,
                                   balance_cents, balance_at, created_at, balance_currency, identification_hash, country)
             SELECT ?2, iban, name, currency, label, aspsp, session_id, valid_until,
                    balance_cents, balance_at, created_at, balance_currency, identification_hash, country
             FROM accounts WHERE uid = ?1",
            params![old, new],
        )?;
        self.exec(
            "UPDATE transactions SET account_uid = ?2, dedup_key = ?2 || substr(dedup_key, length(?1) + 1)
             WHERE account_uid = ?1",
            params![old, new],
        )?;
        self.exec(
            "UPDATE sync_log SET account_uid = ?2 WHERE account_uid = ?1",
            params![old, new],
        )?;
        self.exec("DELETE FROM accounts WHERE uid = ?1", [old])?;
        Ok(())
    }

    /// Custom display name for an account (None or empty = automatic).
    pub fn rename_account(&self, uid: &str, label: Option<&str>) -> Result<()> {
        let label = label.map(str::trim).filter(|l| !l.is_empty());
        if self.exec(
            "UPDATE accounts SET label = ?2 WHERE uid = ?1",
            params![uid, label],
        )? == 0
        {
            return Err(Error::not_found("conto inesistente"));
        }
        Ok(())
    }

    /// Delete an account with its transactions and sync history (run in a transaction).
    /// Returns the number of transactions deleted and its consent (session) if no other
    /// account still uses it.
    pub fn delete_account(&self, uid: &str) -> Result<(usize, Option<String>)> {
        let session: Option<String> = self
            .row(
                "SELECT session_id FROM accounts WHERE uid = ?1",
                [uid],
                |r| r.get(0),
            )?
            .ok_or_else(|| Error::not_found("conto inesistente"))?;
        let deleted = self.exec("DELETE FROM transactions WHERE account_uid = ?1", [uid])?;
        self.exec("DELETE FROM sync_log WHERE account_uid = ?1", [uid])?;
        self.exec("DELETE FROM accounts WHERE uid = ?1", [uid])?;
        let orphan = match session {
            Some(s) => {
                let used: bool = self.value(
                    "SELECT EXISTS (SELECT 1 FROM accounts WHERE session_id = ?1)",
                    [&s],
                )?;
                (!used).then_some(s)
            }
            None => None,
        };
        Ok((deleted, orphan))
    }

    pub fn set_balance(&self, uid: &str, balance: Money, currency: &str) -> Result<()> {
        self.exec(
            "UPDATE accounts SET balance_cents = ?1, balance_at = ?2, balance_currency = ?4 WHERE uid = ?3",
            params![balance, Utc::now().to_rfc3339(), uid, currency],
        )?;
        Ok(())
    }

    /// Sum of the latest balances in the base currency.
    pub fn base_balance_total(&self) -> Result<Option<Money>> {
        self.value(
            "SELECT SUM(balance_cents) FROM accounts WHERE COALESCE(balance_currency, currency) = ?1",
            [BASE_CURRENCY],
        )
    }

    /// Every account with a known balance, and the daily net movements of those in the base
    /// currency (PayPal purchases funded by a bank debit are counted once, on the bank).
    pub fn account_ledgers(&self) -> Result<Vec<AccountLedger>> {
        let sql = format!(
            "SELECT a.uid, {ACCOUNT_LABEL}, a.balance_cents, a.balance_at, COALESCE(a.balance_currency, a.currency)
             FROM accounts a WHERE a.balance_cents IS NOT NULL ORDER BY a.created_at, a.uid"
        );
        let rows: Vec<(String, String, Money, String, String)> = self.rows(&sql, [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?;
        rows.into_iter()
            .map(|(uid, label, balance, as_of, currency)| {
                let daily = if currency == BASE_CURRENCY {
                    let days: Vec<(NaiveDate, Money)> = self.rows(
                        "SELECT booking_date, SUM(amount_cents) FROM transactions
                         WHERE account_uid = ?1 AND currency = ?2 AND funded_by IS NULL
                         GROUP BY booking_date",
                        params![uid, currency],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )?;
                    Some(days.into_iter().collect::<BTreeMap<_, _>>())
                } else {
                    None
                };
                Ok(AccountLedger {
                    label,
                    balance,
                    currency,
                    as_of,
                    daily,
                })
            })
            .collect()
    }

    /// Consent expiry and latest sync of every account.
    pub fn account_health(&self) -> Result<Vec<AccountHealth>> {
        let sql = format!(
            "SELECT {ACCOUNT_LABEL}, a.valid_until,
                (SELECT ok FROM sync_log s WHERE s.account_uid = a.uid ORDER BY s.id DESC LIMIT 1),
                (SELECT message FROM sync_log s WHERE s.account_uid = a.uid ORDER BY s.id DESC LIMIT 1),
                (SELECT MAX(at) FROM sync_log s WHERE s.account_uid = a.uid AND s.ok = 1)
             FROM accounts a ORDER BY a.created_at, a.uid"
        );
        self.rows(&sql, [], |r| {
            Ok(AccountHealth {
                label: r.get(0)?,
                consent_valid_until: r.get(1)?,
                last_ok: r.get::<_, Option<i64>>(2)?.map(|v| v != 0),
                last_message: r.get(3)?,
                last_success_at: r.get(4)?,
            })
        })
    }

    /// Remember an authorization in progress (and forget those older than a day).
    pub fn save_pending_auth(&self, state: &str, bank: &str, country: &str) -> Result<()> {
        self.exec(
            "DELETE FROM pending_auth WHERE created_at < datetime('now', '-1 day')",
            [],
        )?;
        self.exec(
            "INSERT INTO pending_auth (state, aspsp, country) VALUES (?1, ?2, ?3)",
            params![state, bank, country],
        )?;
        Ok(())
    }

    /// The (bank, country) of an authorization in progress.
    pub fn pending_auth(&self, state: &str) -> Result<Option<(String, String)>> {
        self.row(
            "SELECT aspsp, country FROM pending_auth WHERE state = ?1",
            [state],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }

    pub fn delete_pending_auth(&self, state: &str) -> Result<()> {
        self.exec("DELETE FROM pending_auth WHERE state = ?1", [state])?;
        Ok(())
    }
}
