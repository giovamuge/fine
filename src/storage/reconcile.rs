//! PayPal reconciliation. A PayPal purchase paid with the linked bank account shows up twice:
//! on the PayPal account (with the real merchant) and on the bank as a "PayPal Europe" direct
//! debit for the same amount, a few days later. The PayPal side keeps the expense; the bank debit
//! becomes a transfer (bank -> PayPal) and the PayPal purchase records which debit funded it
//! (`transactions.funded_by`), so the money is counted once and PayPal's balance does not move.

use chrono::{Days, NaiveDate};
use rusqlite::params;

use super::Db;
use crate::domain::Money;
use crate::domain::transaction::CategorizedBy;
use crate::error::Result;

/// PayPal Europe's SEPA creditor identifier, written in the remittance of its direct debits
/// ("Addebito SDD ... da LU96ZZZ0000000000000000058 Mand ..."): the surest sign of a PayPal debit.
const PAYPAL_CREDITOR_ID: &str = "lu96zzz0000000000000000058";

/// How many days after the PayPal purchase the bank debit may be booked.
const MAX_DAYS: u64 = 7;

impl Db<'_> {
    /// Pairs unreconciled PayPal purchases with PayPal debits on the other accounts: same
    /// currency, same amount, bank debit booked 0..=MAX_DAYS days later. A debit is PayPal's when
    /// its text has PayPal's creditor identifier or names PayPal; debits with the identifier are
    /// preferred, then the closest date. A bank debit the user categorized by hand is left alone.
    /// Returns the number of new pairs.
    pub fn reconcile_paypal(&self) -> Result<usize> {
        let Some(transfer) = self.transfer_category_id()? else {
            return Ok(0);
        };
        let purchases: Vec<(i64, NaiveDate, Money, String)> = self.rows(
            "SELECT t.id, t.booking_date, t.amount_cents, t.currency
             FROM transactions t JOIN accounts a ON a.uid = t.account_uid
             WHERE a.aspsp LIKE 'paypal%' AND t.amount_cents < 0 AND t.funded_by IS NULL
             ORDER BY t.booking_date, t.id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let mut paired = 0;
        for (id, date, amount, currency) in purchases {
            let debit: Option<i64> = self.row(
                "SELECT t.id FROM transactions t JOIN accounts a ON a.uid = t.account_uid
                 WHERE a.aspsp NOT LIKE 'paypal%'
                   AND t.amount_cents = ?1 AND t.currency = ?2
                   AND t.booking_date BETWEEN ?3 AND ?4
                   AND (instr(lower(COALESCE(t.description, '')), ?5) > 0
                        OR lower(COALESCE(t.counterparty, '') || ' ' || COALESCE(t.description, '')
                                 || ' ' || t.merchant) LIKE '%paypal%')
                   AND COALESCE(t.categorized_by, '') <> 'manual'
                   AND NOT EXISTS (SELECT 1 FROM transactions p WHERE p.funded_by = t.id)
                 ORDER BY instr(lower(COALESCE(t.description, '')), ?5) > 0 DESC, t.booking_date, t.id
                 LIMIT 1",
                params![amount, currency, date, date + Days::new(MAX_DAYS), PAYPAL_CREDITOR_ID],
                |r| r.get(0),
            )?;
            let Some(debit) = debit else { continue };
            self.exec(
                "UPDATE transactions SET funded_by = ?1 WHERE id = ?2",
                params![debit, id],
            )?;
            self.set_category(debit, transfer, CategorizedBy::Paypal)?;
            paired += 1;
        }
        Ok(paired)
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::Store;
    use rusqlite::params;

    fn tx(store: &Store, id: i64, acc: &str, date: &str, cents: i64, desc: &str) {
        store
            .conn()
            .execute(
                "INSERT INTO transactions (id, account_uid, booking_date, amount_cents, currency, description, merchant, dedup_key)
                 VALUES (?1, ?2, ?3, ?4, 'EUR', ?5, lower(?5), ?1)",
                params![id, acc, date, cents, desc],
            )
            .unwrap();
    }

    #[test]
    fn pairs_paypal_purchases_with_bank_debits() {
        let store = Store::open_in_memory().unwrap();
        store
            .conn()
            .execute_batch(
                "INSERT INTO accounts (uid, currency, aspsp) VALUES ('pp', 'XXX', 'PayPal'), ('bank', 'EUR', 'FinecoBank');",
            )
            .unwrap();
        tx(&store, 1, "pp", "2026-10-25", -4900, "Hostinger");
        tx(&store, 2, "pp", "2026-10-23", -2099, "Spotify"); // paid with PayPal balance
        tx(&store, 3, "pp", "2026-10-01", -1500, "Old");
        tx(
            &store,
            10,
            "bank",
            "2026-10-27",
            -4900,
            "PayPal Europe Addebito SDD",
        );
        tx(&store, 11, "bank", "2026-10-27", -2099, "Netflix"); // not a PayPal debit
        tx(
            &store,
            12,
            "bank",
            "2026-10-20",
            -1500,
            "PayPal Europe Addebito SDD",
        ); // too late
        tx(&store, 4, "pp", "2026-11-02", -2190, "AliExpress");
        tx(
            &store,
            13,
            "bank",
            "2026-11-03",
            -2190,
            "Rimborso paypal amico",
        ); // names PayPal only
        tx(
            &store,
            14,
            "bank",
            "2026-11-05",
            -2190,
            "Addebito SDD da LU96ZZZ0000000000000000058 Mand X",
        );
        let db = store.db();
        assert_eq!(db.reconcile_paypal().unwrap(), 2);
        assert_eq!(db.reconcile_paypal().unwrap(), 0, "idempotent");
        let funded = |id: i64| -> Option<i64> {
            store
                .conn()
                .query_row(
                    "SELECT funded_by FROM transactions WHERE id = ?1",
                    [id],
                    |r| r.get(0),
                )
                .unwrap()
        };
        assert_eq!(funded(1), Some(10));
        assert_eq!(
            funded(4),
            Some(14),
            "the creditor identifier wins over a closer date"
        );
        let (kind, by): (String, String) = store
            .conn()
            .query_row(
                "SELECT c.kind, t.categorized_by FROM transactions t JOIN categories c ON c.id = t.category_id WHERE t.id = 10",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((kind.as_str(), by.as_str()), ("transfer", "paypal"));
    }
}
