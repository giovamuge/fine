//! Rule-based categorization that learns from every manual/AI decision.
//!
//! A rule is a lowercase substring matched against the padded, normalized text
//! " merchant counterparty description ". The longest matching pattern wins.

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use serde::Serialize;

use crate::db;

const NOISE_PREFIXES: &[&str] = &[
    "pagamento pos",
    "pagamento carta",
    "pagamento tramite pos",
    "pag. pos",
    "pagam. pos",
    "operazione carta",
    "acquisto carta",
    "addebito sdd",
    "addebito diretto",
    "addebito",
    "bonifico sepa",
    "bonifico istantaneo",
    "bonifico a vostro favore",
    "bonifico disposto a favore di",
    "bonifico a favore di",
    "bonifico da",
    "bonifico",
    "vostra disposizione",
    "acquisto",
    "pagamento",
    "pos",
];

/// Lowercase, collapse whitespace.
pub fn normalize_text(s: &str) -> String {
    s.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Derive a stable merchant key from counterparty / description,
/// e.g. "PAGAMENTO POS 12/03 ESSELUNGA MILANO CARTA *1234" -> "esselunga milano".
pub fn normalize_merchant(counterparty: Option<&str>, description: Option<&str>) -> String {
    let source = counterparty
        .filter(|c| !c.trim().is_empty())
        .or(description)
        .unwrap_or("");
    let mut text: String = source
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '&' {
                c
            } else {
                ' '
            }
        })
        .collect();
    text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    loop {
        let before = text.len();
        for p in NOISE_PREFIXES {
            if let Some(rest) = text.strip_prefix(p)
                && (rest.is_empty() || rest.starts_with(' '))
            {
                text = rest.trim_start().to_string();
            }
        }
        if text.len() == before {
            break;
        }
    }
    let words: Vec<&str> = text
        .split_whitespace()
        // drop dates, card numbers, codes: tokens with digits that are not mostly letters
        .filter(|w| w.chars().filter(|c| c.is_ascii_digit()).count() * 2 < w.chars().count())
        .filter(|w| {
            !matches!(
                *w,
                "carta" | "ore" | "del" | "il" | "n." | "num" | "c" | "eur"
            )
        })
        .take(4)
        .collect();
    words.join(" ")
}

pub(crate) fn haystack(
    merchant: &str,
    counterparty: Option<&str>,
    description: Option<&str>,
) -> String {
    format!(
        " {} {} {} ",
        merchant,
        normalize_text(counterparty.unwrap_or("")),
        normalize_text(description.unwrap_or(""))
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct Rule {
    pub id: i64,
    pub pattern: String,
    pub category: String,
    pub source: String,
    pub hits: i64,
}

pub fn list_rules(conn: &Connection) -> Result<Vec<Rule>> {
    let mut st = conn.prepare(
        "SELECT r.id, r.pattern, c.name, r.source, r.hits FROM rules r JOIN categories c ON c.id = r.category_id ORDER BY c.name, r.pattern",
    )?;
    let rows = st.query_map([], |r| {
        Ok(Rule {
            id: r.get(0)?,
            pattern: r.get(1)?,
            category: r.get(2)?,
            source: r.get(3)?,
            hits: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Apply rules to every uncategorized transaction. Returns how many were categorized.
pub fn apply_rules(conn: &Connection) -> Result<usize> {
    let rules: Vec<(i64, String, i64)> = conn
        .prepare("SELECT id, pattern, category_id FROM rules")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<Result<_, _>>()?;
    if rules.is_empty() {
        return Ok(0);
    }
    let pending: Vec<(i64, String, Option<String>, Option<String>)> = conn
        .prepare("SELECT id, merchant, counterparty, description FROM transactions WHERE category_id IS NULL")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<_, _>>()?;
    let mut count = 0;
    for (id, merchant, cp, desc) in pending {
        let hay = haystack(&merchant, cp.as_deref(), desc.as_deref());
        let best = rules
            .iter()
            .filter(|(_, p, _)| hay.contains(p.as_str()))
            .max_by_key(|(_, p, _)| p.trim().len());
        if let Some((rule_id, _, cat)) = best {
            conn.execute(
                "UPDATE transactions SET category_id = ?1, categorized_by = 'rule' WHERE id = ?2",
                params![cat, id],
            )?;
            conn.execute("UPDATE rules SET hits = hits + 1 WHERE id = ?1", [rule_id])?;
            count += 1;
        }
    }
    Ok(count)
}

#[derive(Debug, Serialize)]
pub struct CategorizeOutcome {
    pub updated: usize,
    pub rule_pattern: Option<String>,
    /// Other uncategorized transactions categorized automatically by the new rule.
    pub auto_categorized: usize,
}

/// Assign `category` to transactions; optionally learn a rule so future ones are automatic.
pub fn categorize(
    conn: &mut Connection,
    ids: &[i64],
    category: &str,
    learn_rule: bool,
    pattern: Option<&str>,
) -> Result<CategorizeOutcome> {
    if ids.is_empty() {
        bail!("nessuna transazione indicata");
    }
    let tx = conn.transaction()?;
    let cat = db::category_id(&tx, category)?;
    let mut updated = 0;
    let mut first_merchant = None;
    for id in ids {
        let merchant: String = tx
            .query_row(
                "SELECT merchant FROM transactions WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .with_context(|| format!("transazione {id} inesistente"))?;
        first_merchant.get_or_insert(merchant);
        updated += tx.execute(
            "UPDATE transactions SET category_id = ?1, categorized_by = 'manual' WHERE id = ?2",
            params![cat, id],
        )?;
    }
    let mut rule_pattern = None;
    let mut auto = 0;
    if learn_rule {
        let p = match pattern {
            Some(p) => normalize_text(p),
            None => first_merchant.unwrap_or_default(),
        };
        if p.chars().filter(|c| c.is_alphanumeric()).count() < 3 {
            bail!("pattern troppo corto per una regola: {p:?}");
        }
        tx.execute(
            "INSERT INTO rules (pattern, category_id, source) VALUES (?1, ?2, 'learned')
             ON CONFLICT(pattern) DO UPDATE SET category_id = excluded.category_id, source = 'learned'",
            params![p, cat],
        )?;
        auto = apply_rules(&tx)?;
        rule_pattern = Some(p);
    }
    tx.commit()?;
    Ok(CategorizeOutcome {
        updated,
        rule_pattern,
        auto_categorized: auto,
    })
}

pub fn delete_rule(conn: &Connection, id: i64) -> Result<()> {
    if conn.execute("DELETE FROM rules WHERE id = ?1", [id])? == 0 {
        bail!("regola {id} inesistente");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_merchants() {
        assert_eq!(
            normalize_merchant(
                None,
                Some("PAGAMENTO POS 12/03/2026 ORE 12:31 ESSELUNGA MILANO CARTA *1234")
            ),
            "esselunga milano"
        );
        assert_eq!(
            normalize_merchant(Some("Netflix.com"), Some("abbonamento")),
            "netflix.com"
        );
        assert_eq!(
            normalize_merchant(None, Some("ADDEBITO SDD ENEL ENERGIA SPA 0012345")),
            "enel energia spa"
        );
    }

    fn insert(conn: &Connection, id: i64, merchant_src: &str) {
        conn.execute(
            "INSERT OR IGNORE INTO accounts (uid, currency) VALUES ('a', 'EUR')",
            [],
        )
        .unwrap();
        let m = normalize_merchant(None, Some(merchant_src));
        conn.execute(
            "INSERT INTO transactions (id, account_uid, booking_date, amount_cents, currency, description, merchant, dedup_key)
             VALUES (?1, 'a', '2026-01-01', -1000, 'EUR', ?2, ?3, ?1)",
            params![id, merchant_src, m],
        )
        .unwrap();
    }

    #[test]
    fn longest_rule_wins_and_learning_spreads() {
        let mut conn = db::open_in_memory().unwrap();
        insert(&conn, 1, "UBER EATS ORDINE");
        insert(&conn, 2, "PALESTRA FITNESS ROMA");
        insert(&conn, 3, "PALESTRA FITNESS ROMA");
        assert_eq!(apply_rules(&conn).unwrap(), 1);
        let cat: String = conn
            .query_row("SELECT c.name FROM transactions t JOIN categories c ON c.id=t.category_id WHERE t.id=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cat, "Ristoranti e bar");

        let out = categorize(&mut conn, &[2], "Svago", true, Some("palestra fitness")).unwrap();
        assert_eq!(out.updated, 1);
        assert_eq!(out.auto_categorized, 1); // transaction 3 learned automatically
    }
}
