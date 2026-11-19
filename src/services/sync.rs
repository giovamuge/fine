//! Incremental sync from Enable Banking into the store.
//!
//! - Only booked transactions are stored (pending ones change id/amount when booked).
//! - Each sync re-reads a 7-day overlap and deduplicates, so nothing is missed.
//! - At most `MAX_SYNCS_PER_DAY` syncs per account in a rolling 24h (PSD2 unattended limit).

use std::collections::{HashMap, HashSet};

use chrono::{Days, Duration, NaiveDate, Utc};
use serde::Serialize;
use serde_json::value::RawValue;

use crate::bank::{self, BookedTransaction, Client, preferred_balance};
use crate::config::EnableBankingConfig;
use crate::domain::Money;
use crate::domain::text;
use crate::domain::transaction::CategorizedBy;
use crate::error::{Error, Result};
use crate::rates::Client as RatesClient;
use crate::services::rates;
use crate::storage::{Db, NewTransaction, Store};
use crate::tr;

pub const MAX_SYNCS_PER_DAY: i64 = 4;
const OVERLAP_DAYS: u64 = 7;
/// First sync window. 90 days is what virtually every PSD2 bank allows without the user present;
/// asking for more and retrying costs daily accesses (failed attempts count against the PSD2 limit).
pub const INITIAL_DAYS: u64 = 90;
/// Data older than this is refreshed automatically by status, MCP and dashboard.
pub const STALE_HOURS: i64 = 6;
/// Shorter windows tried when a bank rejects the requested history length
/// (Enable Banking error WRONG_TRANSACTIONS_PERIOD). Kept short: every attempt uses daily quota.
const FALLBACK_DAYS: [u64; 2] = [90, 30];

#[derive(Debug, Serialize)]
pub struct AccountSync {
    pub account: String,
    pub inserted: usize,
    pub skipped: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SyncReport {
    pub accounts: Vec<AccountSync>,
    pub auto_categorized: usize,
    pub auto_tagged: usize,
    /// The exchange rates foreign transactions need could not be updated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rates_error: Option<String>,
}

impl SyncReport {
    pub fn inserted(&self) -> usize {
        self.accounts.iter().map(|a| a.inserted).sum()
    }

    /// "account: error" for every account that failed.
    pub fn errors(&self) -> Vec<String> {
        self.accounts
            .iter()
            .filter_map(|a| a.error.as_ref().map(|e| format!("{}: {e}", a.account)))
            .collect()
    }
}

/// Sync every account (within the daily limit), then reconcile, categorize and tag.
pub async fn sync_all(store: &mut Store, client: &Client) -> Result<SyncReport> {
    let today = Utc::now().date_naive();
    let mut accounts = vec![];
    let labels = store.db().account_labels()?;
    for (uid, label) in labels {
        let used = store
            .db()
            .syncs_reaching_bank_since(&uid, Utc::now() - Duration::hours(24))?;
        if used >= MAX_SYNCS_PER_DAY {
            accounts.push(AccountSync {
                account: label,
                inserted: 0,
                skipped: Some(tr!(
                    "già {used} sincronizzazioni nelle ultime 24h (limite PSD2)",
                    "already {used} syncs in the last 24h (PSD2 limit)"
                )),
                error: None,
            });
            continue;
        }
        let from = store
            .db()
            .last_booking_date(&uid)?
            .map(|d| d - Days::new(OVERLAP_DAYS))
            .unwrap_or(today - Days::new(INITIAL_DAYS));
        let result = sync_account(store, client, &uid, from, today).await;
        let error = result.as_ref().err().map(Error::chain);
        store.db().log_sync(&uid, error.as_deref())?;
        accounts.push(AccountSync {
            account: label,
            inserted: result.unwrap_or(0),
            skipped: None,
            error,
        });
    }
    // Foreign transactions are counted once their exchange rate is known (no PSD2 cost).
    let rates_error = rates::update(store, &RatesClient::default())
        .await
        .err()
        .map(|e| e.chain());
    let (auto_categorized, auto_tagged) = store.write(|db| {
        db.reconcile_paypal()?;
        Ok((db.apply_category_rules()?, db.apply_tag_rules_to_new()?))
    })?;
    Ok(SyncReport {
        accounts,
        auto_categorized,
        auto_tagged,
        rates_error,
    })
}

/// Start dates to try, longest first: the wanted one, then shorter fallback windows.
pub fn candidate_starts(from: NaiveDate, today: NaiveDate) -> Vec<NaiveDate> {
    std::iter::once(from)
        .chain(
            FALLBACK_DAYS
                .iter()
                .map(|d| today - Days::new(*d))
                .filter(|d| *d > from),
        )
        .collect()
}

async fn fetch_transactions(
    client: &Client,
    uid: &str,
    from: NaiveDate,
    today: NaiveDate,
) -> std::result::Result<Vec<Box<RawValue>>, bank::Error> {
    let mut last_err = None;
    for start in candidate_starts(from, today) {
        match client
            .transactions(uid, &start.format("%Y-%m-%d").to_string())
            .await
        {
            Ok(raw) => return Ok(raw),
            Err(e) if e.is_wrong_period() => last_err = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last_err.expect("at least one attempt"))
}

async fn sync_account(
    store: &mut Store,
    client: &Client,
    uid: &str,
    from: NaiveDate,
    today: NaiveDate,
) -> Result<usize> {
    let raw = fetch_transactions(client, uid, from, today).await?;
    let inserted = store.write(|db| store_transactions(db, uid, &raw))?;
    let balances = client.balances(uid).await?;
    if let Some(b) = preferred_balance(&balances) {
        let amount = Money::parse(&b.balance_amount.amount)?;
        store
            .db()
            .set_balance(uid, amount, &b.balance_amount.currency)?;
    }
    Ok(inserted)
}

/// Store the bank's transactions, deduplicating. Returns the number of new rows.
/// Transfers to/from the user's own accounts are categorized as such right away.
pub fn store_transactions(db: Db<'_>, account_uid: &str, raw: &[Box<RawValue>]) -> Result<usize> {
    let own_ibans: HashSet<String> = db.own_ibans()?;
    let transfer = db.transfer_category_id()?;
    let mut occurrences: HashMap<String, usize> = HashMap::new();
    let mut inserted = 0;
    for json in raw {
        let Some(t) = BookedTransaction::parse(json.get()) else {
            continue;
        };
        let dedup_key = match &t.entry_reference {
            Some(r) => format!("{account_uid}|ref|{r}"),
            None => {
                // Identical transactions on the same day are told apart by their position.
                let base = format!(
                    "{account_uid}|{}|{}|{}|{}",
                    t.booking_date,
                    t.amount.cents(),
                    t.counterparty.as_deref().unwrap_or(""),
                    t.description.as_deref().unwrap_or("")
                );
                let n = occurrences.entry(base.clone()).or_default();
                *n += 1;
                format!("{base}#{n}")
            }
        };
        let merchant = text::merchant_key(t.counterparty.as_deref(), t.description.as_deref());
        let is_transfer = t
            .counterparty_iban
            .as_ref()
            .is_some_and(|i| own_ibans.contains(i));
        let category = transfer
            .filter(|_| is_transfer)
            .map(|c| (c, CategorizedBy::Transfer));
        let new = db.insert_transaction(&NewTransaction {
            account_uid,
            booking_date: t.booking_date,
            amount: t.amount,
            currency: &t.currency,
            counterparty: t.counterparty.as_deref(),
            description: t.description.as_deref(),
            merchant: &merchant,
            entry_reference: t.entry_reference.as_deref(),
            dedup_key: &dedup_key,
            category,
            raw_json: json.get(),
        })?;
        inserted += usize::from(new);
    }
    Ok(inserted)
}

/// True if there are accounts and none was synced successfully in the last `STALE_HOURS`.
pub fn is_stale(store: &Store) -> Result<bool> {
    let db = store.db();
    if db.account_count()? == 0 {
        return Ok(false);
    }
    Ok(db.successful_syncs_since(Utc::now() - Duration::hours(STALE_HOURS))? == 0)
}

/// Sync only if data is stale and Enable Banking is configured; otherwise just fill in the
/// exchange rates foreign transactions are missing. Never fails the caller: returns a short
/// message describing what happened (or why it could not), `None` when there was nothing to do.
pub async fn sync_if_stale(store: &mut Store, eb: Option<&EnableBankingConfig>) -> Option<String> {
    if let Some(eb) = eb {
        match is_stale(store) {
            Ok(true) => return Some(sync_now(store, eb).await),
            Ok(false) => {}
            Err(e) => return Some(format!("controllo sincronizzazione fallito: {}", e.chain())),
        }
    }
    match rates::update(store, &RatesClient::default()).await {
        Ok(0) => None,
        Ok(n) => Some(format!("cambi aggiornati: {n}")),
        Err(e) => Some(format!("cambi non aggiornati: {}", e.chain())),
    }
}

async fn sync_now(store: &mut Store, eb: &EnableBankingConfig) -> String {
    let run = async {
        let client = Client::new(eb).map_err(Error::from)?;
        sync_all(store, &client).await
    };
    match run.await {
        Ok(r) => {
            let (new, errors) = (r.inserted(), r.errors());
            let mut msg = if errors.is_empty() {
                format!("sincronizzato: {new} nuove transazioni")
            } else {
                format!(
                    "sincronizzazione parziale ({new} nuove); errori: {}",
                    errors.join("; ")
                )
            };
            if let Some(e) = r.rates_error {
                msg.push_str(&format!("; cambi non aggiornati: {e}"));
            }
            msg
        }
        Err(e) => format!("sincronizzazione non riuscita: {}", e.chain()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn raw(values: &[serde_json::Value]) -> Vec<Box<RawValue>> {
        values
            .iter()
            .map(|v| RawValue::from_string(v.to_string()).unwrap())
            .collect()
    }

    #[test]
    fn dedups_identical_same_day_transactions_consistently() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .conn()
            .execute(
                "INSERT INTO accounts (uid, currency) VALUES ('a', 'EUR')",
                [],
            )
            .unwrap();
        let coffee = json!({"transaction_amount": {"amount": "1.20", "currency": "EUR"}, "credit_debit_indicator": "DBIT", "booking_date": "2026-03-01", "creditor": {"name": "BAR"}});
        let batch = raw(&[coffee.clone(), coffee]);
        assert_eq!(
            store
                .write(|db| store_transactions(db, "a", &batch))
                .unwrap(),
            2
        );
        // Re-sync with overlap: same two coffees, nothing new
        assert_eq!(
            store
                .write(|db| store_transactions(db, "a", &batch))
                .unwrap(),
            0
        );
    }

    #[test]
    fn detects_transfers_between_own_accounts() {
        let mut store = Store::open_in_memory().unwrap();
        store
            .conn()
            .execute(
                "INSERT INTO accounts (uid, currency, iban) VALUES ('a', 'EUR', 'IT00A'), ('b', 'EUR', 'IT00B')",
                [],
            )
            .unwrap();
        let t = json!({"transaction_amount": {"amount": "100", "currency": "EUR"}, "credit_debit_indicator": "DBIT", "booking_date": "2026-03-01", "creditor_account": {"iban": "IT00 B"}});
        store
            .write(|db| store_transactions(db, "a", &raw(&[t])))
            .unwrap();
        let by: String = store
            .conn()
            .query_row("SELECT categorized_by FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(by, "transfer");
    }
}
