//! PayPal reconciliation. A PayPal purchase paid with the linked bank account shows up twice:
//! on the PayPal account (with the real merchant) and on the bank as a "PayPal Europe" direct
//! debit for the same amount, a few days later. The PayPal side keeps the expense; the bank debit
//! becomes a transfer (bank -> PayPal) and the PayPal purchase records which debit funded it
//! (`transactions.funded_by`), so the money is counted once and PayPal's balance does not move.

use anyhow::Result;
use chrono::NaiveDate;
use rusqlite::{Connection, params};

/// PayPal Europe's SEPA creditor identifier, written in the remittance of its direct debits
/// ("Addebito SDD ... da LU96ZZZ0000000000000000058 Mand ..."): the surest sign of a PayPal debit.
const PAYPAL_CREDITOR_ID: &str = "lu96zzz0000000000000000058";

/// How many days after the PayPal purchase the bank debit may be booked.
const MAX_DAYS: i64 = 7;

/// Pairs unreconciled PayPal purchases with PayPal debits on the other accounts: same currency,
/// same amount, bank debit booked 0..=MAX_DAYS days later. A debit is PayPal's when its text has
/// PayPal's creditor identifier or names PayPal; debits with the identifier are preferred, then
/// the closest date. A bank debit the user
/// categorized by hand is left alone. Returns the number of new pairs.
pub fn paypal(conn: &Connection) -> Result<usize> {
    let Some(transfer_cat) = conn
        .query_row(
            "SELECT id FROM categories WHERE kind = 'transfer' ORDER BY id LIMIT 1",
            [],
            |r| r.get::<_, i64>(0),
        )
        .ok()
    else {
        return Ok(0);
    };
    let purchases: Vec<(i64, String, i64, String)> = conn
        .prepare(
            "SELECT t.id, t.booking_date, t.amount_cents, t.currency
             FROM transactions t JOIN accounts a ON a.uid = t.account_uid
             WHERE a.aspsp LIKE 'paypal%' AND t.amount_cents < 0 AND t.funded_by IS NULL
             ORDER BY t.booking_date, t.id",
        )?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    let mut paired = 0;
    for (id, date, cents, currency) in purchases {
        let Ok(day) = NaiveDate::parse_from_str(&date, "%Y-%m-%d") else {
            continue;
        };
        let last = (day + chrono::Duration::days(MAX_DAYS)).to_string();
        let debit: Option<i64> = conn
            .query_row(
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
                params![cents, currency, date, last, PAYPAL_CREDITOR_ID],
                |r| r.get(0),
            )
            .ok();
        let Some(debit) = debit else { continue };
        conn.execute(
            "UPDATE transactions SET funded_by = ?1 WHERE id = ?2",
            params![debit, id],
        )?;
        conn.execute(
            "UPDATE transactions SET category_id = ?1, categorized_by = 'paypal' WHERE id = ?2",
            params![transfer_cat, debit],
        )?;
        paired += 1;
    }
    Ok(paired)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx(conn: &Connection, id: i64, acc: &str, date: &str, cents: i64, desc: &str) {
        conn.execute(
            "INSERT INTO transactions (id, account_uid, booking_date, amount_cents, currency, description, merchant, dedup_key)
             VALUES (?1, ?2, ?3, ?4, 'EUR', ?5, lower(?5), ?1)",
            params![id, acc, date, cents, desc],
        )
        .unwrap();
    }

    #[test]
    fn pairs_paypal_purchases_with_bank_debits() {
        let conn = crate::db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO accounts (uid, currency, aspsp) VALUES ('pp', 'XXX', 'PayPal'), ('bank', 'EUR', 'FinecoBank');",
        )
        .unwrap();
        tx(&conn, 1, "pp", "2026-10-25", -4900, "Hostinger");
        tx(&conn, 2, "pp", "2026-10-23", -2099, "Spotify"); // paid with PayPal balance
        tx(&conn, 3, "pp", "2026-10-01", -1500, "Old");
        tx(
            &conn,
            10,
            "bank",
            "2026-10-27",
            -4900,
            "PayPal Europe Addebito SDD",
        );
        tx(&conn, 11, "bank", "2026-10-27", -2099, "Netflix"); // not a PayPal debit
        tx(
            &conn,
            12,
            "bank",
            "2026-10-20",
            -1500,
            "PayPal Europe Addebito SDD",
        ); // too late
        tx(&conn, 4, "pp", "2026-11-02", -2190, "AliExpress");
        tx(
            &conn,
            13,
            "bank",
            "2026-11-03",
            -2190,
            "Rimborso paypal amico",
        ); // names PayPal only
        tx(
            &conn,
            14,
            "bank",
            "2026-11-05",
            -2190,
            "Addebito SDD da LU96ZZZ0000000000000000058 Mand X",
        );
        assert_eq!(paypal(&conn).unwrap(), 2);
        assert_eq!(paypal(&conn).unwrap(), 0, "idempotent");
        let funded: Option<i64> = conn
            .query_row("SELECT funded_by FROM transactions WHERE id = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(funded, Some(10));
        let funded: Option<i64> = conn
            .query_row("SELECT funded_by FROM transactions WHERE id = 4", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            funded,
            Some(14),
            "the creditor identifier wins over a closer date"
        );
        let (kind, by): (String, String) = conn
            .query_row(
                "SELECT c.kind, t.categorized_by FROM transactions t JOIN categories c ON c.id = t.category_id WHERE t.id = 10",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((kind.as_str(), by.as_str()), ("transfer", "paypal"));
    }
}
