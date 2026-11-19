//! Exchange rates from Frankfurter (<https://frankfurter.dev>): the European Central Bank's
//! daily reference rates, free and without a key. Only currency codes and dates are sent.

use std::collections::BTreeMap;
use std::time::Duration;

use chrono::NaiveDate;
use reqwest::StatusCode;
use serde::Deserialize;

use crate::domain::BASE_CURRENCY;
use crate::error::{Error, Result};

pub const DEFAULT_BASE_URL: &str = "https://api.frankfurter.dev/v1";

/// A rate: units of `currency` for one unit of the base currency on `date`.
pub type Rate = (String, NaiveDate, f64);

pub struct Client {
    http: reqwest::Client,
    base: String,
}

#[derive(Deserialize)]
struct Series {
    rates: BTreeMap<NaiveDate, BTreeMap<String, f64>>,
}

impl Default for Client {
    fn default() -> Self {
        Self::with_base_url(DEFAULT_BASE_URL)
    }
}

impl Client {
    pub fn with_base_url(base: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(concat!("fine/", env!("FINE_VERSION")))
            .build()
            .expect("static HTTP client configuration");
        Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    /// Every published rate of `currencies` between `from` and `to` (business days only; the
    /// series starts at the last business day on or before `from`). Currencies the ECB does not
    /// publish are simply absent. Empty when nothing is published yet for the period.
    pub async fn rates(
        &self,
        from: NaiveDate,
        to: NaiveDate,
        currencies: &[String],
    ) -> Result<Vec<Rate>> {
        if currencies.is_empty() {
            return Ok(vec![]);
        }
        let url = format!("{}/{from}..{to}", self.base);
        let resp = self
            .http
            .get(url)
            .query(&[("base", BASE_CURRENCY), ("symbols", &currencies.join(","))])
            .send()
            .await?;
        match resp.status() {
            // The whole period is after the last published day (e.g. today before 16:00 CET).
            StatusCode::NOT_FOUND => return Ok(vec![]),
            s if !s.is_success() => {
                return Err(Error::Rates(format!(
                    "il servizio dei cambi ha risposto {s}"
                )));
            }
            _ => {}
        }
        let series: Series = resp.json().await.map_err(|e| {
            Error::Rates(format!("risposta del servizio dei cambi non valida: {e}"))
        })?;
        Ok(series
            .rates
            .into_iter()
            .flat_map(|(date, by_currency)| {
                by_currency
                    .into_iter()
                    .filter(|(_, rate)| rate.is_finite() && *rate > 0.0)
                    .map(move |(currency, rate)| (currency, date, rate))
            })
            .collect())
    }
}
