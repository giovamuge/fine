//! Incremental sync from Enable Banking into SQLite.
//!
//! - Only booked transactions are stored (pending ones change id/amount when booked).
//! - Each sync re-reads a 7-day overlap and deduplicates, so nothing is missed.
//! - At most `MAX_SYNCS_PER_DAY` syncs per account in a rolling 24h (PSD2 unattended limit).

use anyhow::Result;
use chrono::{Days, NaiveDate, Utc};
use rusqlite::{Connection, params};
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

use crate::categorize::{apply_rules, normalize_merchant};
use crate::enablebanking::{Client, preferred_balance};
use crate::money::parse_cents;

pub const MAX_SYNCS_PER_DAY: i64 = 4;
const OVERLAP_DAYS: u64 = 7;
/// First sync window. 90 days is what virtually every PSD2 bank allows without the user present;
/// asking for more and retrying costs daily accesses (failed attempts count against the PSD2 limit).
pub const INITIAL_DAYS: u64 = 90;
/// Data older than this is refreshed automatically by status, MCP and dashboard.
pub const STALE_HOURS: i64 = 6;

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
}

#[derive(Debug, PartialEq)]
pub struct ParsedTx {
    pub booking_date: String,
    pub amount_cents: i64,
    pub currency: String,
    pub counterparty: Option<String>,
    pub counterparty_iban: Option<String>,
    pub description: Option<String>,
    pub entry_reference: Option<String>,
}

fn str_at<'a>(v: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut cur = v;
    for p in path {
        cur = cur.get(p)?;
    }
    cur.as_str().filter(|s| !s.trim().is_empty())
}

/// Parse one Enable Banking transaction. Returns None for non-booked or unusable entries.
pub fn parse_transaction(v: &Value) -> Option<ParsedTx> {
    if let Some(status) = str_at(v, &["status"])
        && status != "BOOK"
    {
        return None;
    }
    let amount = str_at(v, &["transaction_amount", "amount"])?;
    let currency = str_at(v, &["transaction_amount", "currency"])
        .unwrap_or("EUR")
        .to_string();
    let mut cents = parse_cents(amount).ok()?;
    let debit = match str_at(v, &["credit_debit_indicator"]) {
        Some("DBIT") => true,
        Some("CRDT") => false,
        _ => cents < 0,
    };
    cents = if debit { -cents.abs() } else { cents.abs() };
    let date = str_at(v, &["booking_date"])
        .or_else(|| str_at(v, &["value_date"]))
        .or_else(|| str_at(v, &["transaction_date"]))?;
    let date = date.get(..10)?.to_string();
    NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok()?;
    let (party, account) = if debit {
        ("creditor", "creditor_account")
    } else {
        ("debtor", "debtor_account")
    };
    let description = v
        .get("remittance_information")
        .and_then(|r| r.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|s| !s.trim().is_empty());
    Some(ParsedTx {
        booking_date: date,
        amount_cents: cents,
        currency,
        counterparty: str_at(v, &[party, "name"]).map(|s| s.trim().to_string()),
        counterparty_iban: str_at(v, &[account, "iban"]).map(|s| s.replace(' ', "").to_uppercase()),
        description,
        entry_reference: str_at(v, &["entry_reference"]).map(str::to_string),
    })
}

/// Insert parsed transactions, deduplicating. Returns the number of new rows.
pub fn store_transactions(
    conn: &mut Connection,
    account_uid: &str,
    raw: &[Value],
) -> Result<usize> {
    let own_ibans: HashSet<String> = conn
        .prepare("SELECT iban FROM accounts WHERE iban IS NOT NULL")?
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(|r| r.ok())
        .map(|s| s.replace(' ', "").to_uppercase())
        .collect();
    let transfer_cat: Option<i64> = conn
        .query_row(
            "SELECT id FROM categories WHERE kind = 'transfer' ORDER BY id LIMIT 1",
            [],
            |r| r.get(0),
        )
        .ok();
    let tx = conn.transaction()?;
    let mut occurrences: HashMap<String, usize> = HashMap::new();
    let mut inserted = 0;
    for v in raw {
        let Some(p) = parse_transaction(v) else {
            continue;
        };
        let dedup_key = match &p.entry_reference {
            Some(r) => format!("{account_uid}|ref|{r}"),
            None => {
                let base = format!(
                    "{account_uid}|{}|{}|{}|{}",
                    p.booking_date,
                    p.amount_cents,
                    p.counterparty.as_deref().unwrap_or(""),
                    p.description.as_deref().unwrap_or("")
                );
                let n = occurrences.entry(base.clone()).or_insert(0);
                *n += 1;
                format!("{base}#{n}")
            }
        };
        let merchant = normalize_merchant(p.counterparty.as_deref(), p.description.as_deref());
        let is_transfer = p
            .counterparty_iban
            .as_ref()
            .is_some_and(|i| own_ibans.contains(i));
        let (cat, by) = match (is_transfer, transfer_cat) {
            (true, Some(c)) => (Some(c), Some("transfer")),
            _ => (None, None),
        };
        inserted += tx.execute(
            "INSERT OR IGNORE INTO transactions
             (account_uid, booking_date, amount_cents, currency, counterparty, description, merchant,
              entry_reference, dedup_key, category_id, categorized_by, raw_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                account_uid,
                p.booking_date,
                p.amount_cents,
                p.currency,
                p.counterparty,
                p.description,
                merchant,
                p.entry_reference,
                dedup_key,
                cat,
                by,
                v.to_string()
            ],
        )?;
    }
    tx.commit()?;
    Ok(inserted)
}

fn syncs_last_24h(conn: &Connection, uid: &str) -> Result<i64> {
    let since = (Utc::now() - chrono::Duration::hours(24)).to_rfc3339();
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM sync_log WHERE account_uid = ?1 AND at >= ?2",
        params![uid, since],
        |r| r.get(0),
    )?)
}

pub async fn sync_all(conn: &mut Connection, client: &Client) -> Result<SyncReport> {
    let today = Utc::now().date_naive();
    let accounts: Vec<(String, String)> = conn
        .prepare(&format!(
            "SELECT a.uid, {} FROM accounts a ORDER BY a.created_at, a.uid",
            crate::db::ACCOUNT_LABEL
        ))?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let mut report = SyncReport {
        accounts: vec![],
        auto_categorized: 0,
    };
    for (uid, label) in accounts {
        let used = syncs_last_24h(conn, &uid)?;
        if used >= MAX_SYNCS_PER_DAY {
            report.accounts.push(AccountSync {
                account: label,
                inserted: 0,
                skipped: Some(format!(
                    "già {used} sincronizzazioni nelle ultime 24h (limite PSD2)"
                )),
                error: None,
            });
            continue;
        }
        let last: Option<String> = conn.query_row(
            "SELECT MAX(booking_date) FROM transactions WHERE account_uid = ?1",
            [&uid],
            |r| r.get(0),
        )?;
        let from = last
            .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
            .map(|d| d - Days::new(OVERLAP_DAYS))
            .unwrap_or(today - Days::new(INITIAL_DAYS));
        let result = sync_account(conn, client, &uid, from, today).await;
        let (ok, msg) = match &result {
            Ok(_) => (1, None),
            Err(e) => (0, Some(format!("{e:#}"))),
        };
        conn.execute(
            "INSERT INTO sync_log (account_uid, at, ok, message) VALUES (?1, ?2, ?3, ?4)",
            params![uid, Utc::now().to_rfc3339(), ok, msg],
        )?;
        report.accounts.push(AccountSync {
            account: label,
            inserted: *result.as_ref().unwrap_or(&0),
            skipped: None,
            error: msg,
        });
    }
    report.auto_categorized = apply_rules(conn)?;
    Ok(report)
}

/// Shorter windows tried when a bank rejects the requested history length
/// (Enable Banking error WRONG_TRANSACTIONS_PERIOD). Kept short: every attempt uses daily quota.
const FALLBACK_DAYS: [u64; 2] = [90, 30];

/// Start dates to try, longest first: the wanted one, then shorter fallback windows.
pub fn candidate_starts(from: NaiveDate, today: NaiveDate) -> Vec<NaiveDate> {
    let mut out = vec![from];
    out.extend(
        FALLBACK_DAYS
            .iter()
            .map(|d| today - Days::new(*d))
            .filter(|d| *d > from),
    );
    out
}

async fn fetch_transactions(
    client: &Client,
    uid: &str,
    from: NaiveDate,
    today: NaiveDate,
) -> Result<Vec<Value>> {
    let mut last_err = None;
    for start in candidate_starts(from, today) {
        match client
            .transactions(uid, &start.format("%Y-%m-%d").to_string())
            .await
        {
            Ok(raw) => return Ok(raw),
            Err(e) if format!("{e:#}").contains("WRONG_TRANSACTIONS_PERIOD") => last_err = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last_err.expect("at least one attempt"))
}

async fn sync_account(
    conn: &mut Connection,
    client: &Client,
    uid: &str,
    from: NaiveDate,
    today: NaiveDate,
) -> Result<usize> {
    let raw = fetch_transactions(client, uid, from, today).await?;
    let inserted = store_transactions(conn, uid, &raw)?;
    let balances = client.balances(uid).await?;
    if let Some(b) = preferred_balance(&balances) {
        conn.execute(
            "UPDATE accounts SET balance_cents = ?1, balance_at = ?2, balance_currency = ?4 WHERE uid = ?3",
            params![
                parse_cents(&b.balance_amount.amount)?,
                Utc::now().to_rfc3339(),
                uid,
                b.balance_amount.currency
            ],
        )?;
    }
    Ok(inserted)
}

/// True if there are accounts and none was synced successfully in the last `STALE_HOURS`.
pub fn is_stale(conn: &Connection) -> Result<bool> {
    let accounts: i64 = conn.query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))?;
    if accounts == 0 {
        return Ok(false);
    }
    let since = (Utc::now() - chrono::Duration::hours(STALE_HOURS)).to_rfc3339();
    let recent: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sync_log WHERE ok = 1 AND at >= ?1",
        [since],
        |r| r.get(0),
    )?;
    Ok(recent == 0)
}

/// Sync only if data is stale and Enable Banking is configured. Never fails the caller:
/// returns a short message describing what happened (or why it could not sync).
pub async fn sync_if_stale(
    conn: &mut Connection,
    eb: Option<&crate::config::EnableBankingConfig>,
) -> Option<String> {
    let eb = eb?;
    match is_stale(conn) {
        Ok(true) => {}
        Ok(false) => return None,
        Err(e) => return Some(format!("controllo sincronizzazione fallito: {e:#}")),
    }
    let run = async {
        let client = Client::new(eb)?;
        sync_all(conn, &client).await
    };
    Some(match run.await {
        Ok(r) => {
            let new: usize = r.accounts.iter().map(|a| a.inserted).sum();
            let errors: Vec<String> = r
                .accounts
                .iter()
                .filter_map(|a| a.error.as_ref().map(|e| format!("{}: {e}", a.account)))
                .collect();
            if errors.is_empty() {
                format!("sincronizzato: {new} nuove transazioni")
            } else {
                format!(
                    "sincronizzazione parziale ({new} nuove); errori: {}",
                    errors.join("; ")
                )
            }
        }
        Err(e) => format!("sincronizzazione non riuscita: {e:#}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_debit_and_skips_pending() {
        let v = json!({
            "transaction_amount": {"amount": "12.50", "currency": "EUR"},
            "credit_debit_indicator": "DBIT",
            "status": "BOOK",
            "booking_date": "2026-03-01",
            "creditor": {"name": "ESSELUNGA"},
            "remittance_information": ["POS", "MILANO"]
        });
        let p = parse_transaction(&v).unwrap();
        assert_eq!(p.amount_cents, -1250);
        assert_eq!(p.counterparty.as_deref(), Some("ESSELUNGA"));
        assert_eq!(p.description.as_deref(), Some("POS MILANO"));
        let pending = json!({"transaction_amount": {"amount": "1", "currency": "EUR"}, "status": "PDNG", "booking_date": "2026-03-01"});
        assert!(parse_transaction(&pending).is_none());
    }

    #[test]
    fn dedups_identical_same_day_transactions_consistently() {
        let mut conn = crate::db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO accounts (uid, currency) VALUES ('a', 'EUR')",
            [],
        )
        .unwrap();
        let coffee = json!({"transaction_amount": {"amount": "1.20", "currency": "EUR"}, "credit_debit_indicator": "DBIT", "booking_date": "2026-03-01", "creditor": {"name": "BAR"}});
        let batch = vec![coffee.clone(), coffee.clone()];
        assert_eq!(store_transactions(&mut conn, "a", &batch).unwrap(), 2);
        // Re-sync with overlap: same two coffees, nothing new
        assert_eq!(store_transactions(&mut conn, "a", &batch).unwrap(), 0);
    }

    #[test]
    fn detects_transfers_between_own_accounts() {
        let mut conn = crate::db::open_in_memory().unwrap();
        conn.execute("INSERT INTO accounts (uid, currency, iban) VALUES ('a', 'EUR', 'IT00A'), ('b', 'EUR', 'IT00B')", []).unwrap();
        let t = json!({"transaction_amount": {"amount": "100", "currency": "EUR"}, "credit_debit_indicator": "DBIT", "booking_date": "2026-03-01", "creditor_account": {"iban": "IT00 B"}});
        store_transactions(&mut conn, "a", &[t]).unwrap();
        let by: String = conn
            .query_row("SELECT categorized_by FROM transactions", [], |r| r.get(0))
            .unwrap();
        assert_eq!(by, "transfer");
    }
}
