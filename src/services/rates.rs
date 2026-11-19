//! Keeping the exchange rates the stored data needs.

use std::collections::HashSet;

use chrono::{Days, Utc};

use crate::error::Result;
use crate::rates::Client;
use crate::storage::{RATE_MAX_AGE_DAYS, Store};

/// Fetch the rates that foreign transactions and balances are missing, then pair PayPal purchases
/// that can now be converted. Returns how many rates were added. Without missing rates nothing is
/// asked to the service.
pub async fn update(store: &mut Store, client: &Client) -> Result<usize> {
    let missing = store.db().missing_rates()?;
    let (Some(first), Some(last)) = (
        missing.iter().map(|m| m.1).min(),
        missing.iter().map(|m| m.2).max(),
    ) else {
        return Ok(0);
    };
    // From a week before, so the business day preceding the first date comes too.
    let from = first - Days::new(u64::from(RATE_MAX_AGE_DAYS));
    let to = last.min(Utc::now().date_naive());
    if from > to {
        return Ok(0);
    }
    let currencies: Vec<String> = missing.into_iter().map(|m| m.0).collect();
    let rates = client.rates(from, to, &currencies).await?;
    let published: HashSet<&str> = rates.iter().map(|r| r.0.as_str()).collect();
    // A currency absent from a non-empty answer is one the ECB does not publish.
    let unsupported: Vec<String> = if rates.is_empty() {
        vec![]
    } else {
        currencies
            .iter()
            .filter(|c| !published.contains(c.as_str()))
            .cloned()
            .collect()
    };
    store.write(|db| {
        if !unsupported.is_empty() {
            db.add_unsupported_currencies(&unsupported)?;
        }
        let added = db.save_rates(&rates)?;
        if added > 0 {
            db.reconcile_paypal()?;
        }
        Ok(added)
    })
}
