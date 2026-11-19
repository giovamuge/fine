//! Exchange rates. Analyses work in the base currency: a foreign amount is converted at the ECB
//! rate of its booking day, or of the last business day before it (weekends and holidays have no
//! rate). A rate older than `RATE_MAX_AGE_DAYS` does not count: the amount stays unconverted.

use chrono::NaiveDate;
use rusqlite::params;

use super::Db;
use crate::domain::BASE_CURRENCY;
use crate::error::Result;

/// How far back a rate may be from the day it is used for.
pub const RATE_MAX_AGE_DAYS: u32 = 7;

/// Setting: currencies the rate service does not know (comma separated), not asked again.
const UNSUPPORTED: &str = "fx_unsupported";

/// SQL subquery: the rate of `currency` for `day` (both SQL expressions), or NULL.
fn rate_sql(currency: &str, day: &str) -> String {
    format!(
        "(SELECT r.rate FROM fx_rates r WHERE r.currency = {currency} AND r.date <= {day}
            AND r.date >= date({day}, '-{RATE_MAX_AGE_DAYS} days') ORDER BY r.date DESC LIMIT 1)"
    )
}

/// SQL expression: `cents` (in `currency`, on day `day`) converted to base-currency cents,
/// rounded to the cent; NULL when there is no rate.
pub(super) fn base_cents_sql(cents: &str, currency: &str, day: &str) -> String {
    format!(
        "CASE WHEN {currency} = '{BASE_CURRENCY}' THEN {cents}
              ELSE CAST(ROUND({cents} / {}) AS INTEGER) END",
        rate_sql(currency, day)
    )
}

/// SQL expression on transaction alias `t`: its amount in base-currency cents, or NULL.
pub(super) fn tx_base_cents_sql() -> String {
    base_cents_sql("t.amount_cents", "t.currency", "t.booking_date")
}

/// A currency code worth asking a rate for (ISO 4217 letters; XXX means "no currency").
pub fn is_rateable(currency: &str) -> bool {
    currency.len() == 3 && currency.bytes().all(|b| b.is_ascii_uppercase()) && currency != "XXX"
}

impl Db<'_> {
    /// Store rates: (currency, day, units per 1 base). Returns how many were new or changed.
    pub fn save_rates(&self, rates: &[(String, NaiveDate, f64)]) -> Result<usize> {
        let mut n = 0;
        for (currency, day, rate) in rates {
            n += self.exec(
                "INSERT INTO fx_rates (currency, date, rate) VALUES (?1, ?2, ?3)
                 ON CONFLICT(currency, date) DO UPDATE SET rate = excluded.rate
                 WHERE rate <> excluded.rate",
                params![currency, day, rate],
            )?;
        }
        Ok(n)
    }

    /// For every foreign currency that has transactions or a balance without a rate: the first
    /// and last day a rate is missing for. Currencies the service does not know are left out.
    pub fn missing_rates(&self) -> Result<Vec<(String, NaiveDate, NaiveDate)>> {
        let sql = format!(
            "SELECT currency, MIN(day), MAX(day) FROM (
                 SELECT t.currency AS currency, t.booking_date AS day FROM transactions t
                 WHERE t.currency <> '{BASE_CURRENCY}' AND {tx} IS NULL
                 UNION ALL
                 SELECT COALESCE(a.balance_currency, a.currency), substr(a.balance_at, 1, 10)
                 FROM accounts a
                 WHERE a.balance_cents IS NOT NULL AND a.balance_at IS NOT NULL
                   AND COALESCE(a.balance_currency, a.currency) <> '{BASE_CURRENCY}'
                   AND {balance} IS NULL)
             GROUP BY currency ORDER BY currency",
            tx = rate_sql("t.currency", "t.booking_date"),
            balance = rate_sql(
                "COALESCE(a.balance_currency, a.currency)",
                "substr(a.balance_at, 1, 10)"
            ),
        );
        let unsupported = self.unsupported_currencies()?;
        Ok(self
            .rows(&sql, [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .into_iter()
            .filter(|(c, _, _): &(String, NaiveDate, NaiveDate)| {
                is_rateable(c) && !unsupported.contains(c)
            })
            .collect())
    }

    pub fn unsupported_currencies(&self) -> Result<Vec<String>> {
        Ok(self
            .setting(UNSUPPORTED)?
            .map(|s| {
                s.split(',')
                    .filter(|c| !c.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default())
    }

    pub fn add_unsupported_currencies(&self, currencies: &[String]) -> Result<()> {
        let mut all = self.unsupported_currencies()?;
        all.extend(currencies.iter().cloned());
        all.sort();
        all.dedup();
        self.set_setting(UNSUPPORTED, &all.join(","))
    }

    /// Foreign-currency transactions booked in [from, to] that have no rate (left out of the
    /// analyses).
    pub fn unconverted_count(&self, from: NaiveDate, to: NaiveDate) -> Result<i64> {
        let sql = format!(
            "SELECT COUNT(*) FROM transactions t
             WHERE t.booking_date BETWEEN ?1 AND ?2 AND {} IS NULL",
            tx_base_cents_sql()
        );
        self.value(&sql, params![from, to])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::testing::d;
    use crate::storage::Store;

    fn add(store: &Store, id: i64, date: &str, cents: i64, currency: &str) {
        store
            .conn()
            .execute_batch("INSERT OR IGNORE INTO accounts (uid, currency) VALUES ('a', 'EUR')")
            .unwrap();
        store
            .conn()
            .execute(
                "INSERT INTO transactions (id, account_uid, booking_date, amount_cents, currency, merchant, dedup_key)
                 VALUES (?1, 'a', ?2, ?3, ?4, 'x', ?1)",
                params![id, date, cents, currency],
            )
            .unwrap();
    }

    fn base_cents(store: &Store, id: i64) -> Option<i64> {
        let sql = format!(
            "SELECT {} FROM transactions t WHERE t.id = ?1",
            tx_base_cents_sql()
        );
        store.conn().query_row(&sql, [id], |r| r.get(0)).unwrap()
    }

    #[test]
    fn converts_at_the_last_business_day_and_reports_what_is_missing() {
        let store = Store::open_in_memory().unwrap();
        add(&store, 1, "2026-10-01", -62432, "GBP"); // a Thursday
        add(&store, 2, "2026-10-04", -1000, "USD"); // a Sunday
        add(&store, 3, "2026-10-04", -500, "EUR");
        add(&store, 4, "2026-10-04", -500, "XXX");
        let db = store.db();
        assert_eq!(base_cents(&store, 3), Some(-500));
        assert_eq!(base_cents(&store, 1), None);
        let missing = db.missing_rates().unwrap();
        assert_eq!(
            missing,
            vec![
                ("GBP".into(), d("2026-10-01"), d("2026-10-01")),
                ("USD".into(), d("2026-10-04"), d("2026-10-04"))
            ]
        );

        db.save_rates(&[
            ("GBP".into(), d("2026-10-01"), 0.85373),
            ("USD".into(), d("2026-10-02"), 1.1225),
        ])
        .unwrap();
        assert_eq!(base_cents(&store, 1), Some(-73129)); // 624.32 GBP / 0.85373 = 731.285
        assert_eq!(base_cents(&store, 2), Some(-891)); // Friday's rate for Sunday
        assert!(db.missing_rates().unwrap().is_empty());
        assert_eq!(
            db.unconverted_count(d("2026-10-01"), d("2026-10-31"))
                .unwrap(),
            1
        ); // XXX

        // A rate more than a week old does not count.
        add(&store, 5, "2026-10-20", -1000, "USD");
        assert_eq!(base_cents(&store, 5), None);
        db.add_unsupported_currencies(&["USD".into()]).unwrap();
        assert!(db.missing_rates().unwrap().is_empty());
    }
}
