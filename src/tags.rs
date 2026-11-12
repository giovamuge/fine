//! Free-form tags: any number per transaction, orthogonal to the single category.
//! The category drives budgets and the "available" figure; tags group transactions in
//! any other way the user likes (a trip, a project, a person, "rimborsabile", ...).
//! Like categories, tags can learn a rule (pattern -> tag) that tags future transactions.

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params, params_from_iter, types::Value};
use serde::Serialize;

use crate::categorize::{haystack, normalize_text};
use crate::db;
use crate::money::eur;

/// Transactions with an id above this have not been through the tag rules yet.
const APPLIED_UP_TO: &str = "tag_rules_applied_up_to";

/// "#Vacanza  Londra" -> "vacanza-londra". Commas are not allowed (they separate tags).
pub fn normalize(name: &str) -> Result<String> {
    let n = name
        .trim()
        .trim_start_matches('#')
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");
    if n.is_empty() {
        bail!("tag vuoto");
    }
    if n.contains(',') {
        bail!("un tag non può contenere virgole: {name:?}");
    }
    if n.chars().count() > 40 {
        bail!("tag troppo lungo (max 40 caratteri): {name:?}");
    }
    Ok(n)
}

fn ensure_tag(conn: &Connection, name: &str) -> Result<i64> {
    conn.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [name])?;
    Ok(conn.query_row("SELECT id FROM tags WHERE name = ?1", [name], |r| r.get(0))?)
}

#[derive(Debug, Serialize)]
pub struct TagOutcome {
    /// New (transaction, tag) links.
    pub tagged: usize,
    pub rule_pattern: Option<String>,
    /// Other transactions tagged by the new rule.
    pub auto_tagged: usize,
}

/// Add `tags` to transactions; optionally learn a rule so matching transactions
/// (past and future) get the same tags.
pub fn tag(
    conn: &mut Connection,
    ids: &[i64],
    tags: &[String],
    learn_rule: bool,
    pattern: Option<&str>,
) -> Result<TagOutcome> {
    if ids.is_empty() {
        bail!("nessuna transazione indicata");
    }
    if tags.is_empty() {
        bail!("nessun tag indicato");
    }
    let names = tags
        .iter()
        .map(|t| normalize(t))
        .collect::<Result<Vec<_>>>()?;
    let tx = conn.transaction()?;
    let tag_ids = names
        .iter()
        .map(|n| ensure_tag(&tx, n))
        .collect::<Result<Vec<_>>>()?;
    let mut tagged = 0;
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
        for tag in &tag_ids {
            tagged += tx.execute(
                "INSERT OR IGNORE INTO transaction_tags (transaction_id, tag_id, source) VALUES (?1, ?2, 'manual')",
                params![id, tag],
            )?;
        }
    }
    let mut rule_pattern = None;
    let mut auto_tagged = 0;
    if learn_rule {
        let p = match pattern {
            Some(p) => normalize_text(p),
            None => first_merchant.unwrap_or_default(),
        };
        if p.chars().filter(|c| c.is_alphanumeric()).count() < 3 {
            bail!("pattern troppo corto per una regola: {p:?}");
        }
        for tag in &tag_ids {
            tx.execute(
                "INSERT OR IGNORE INTO tag_rules (pattern, tag_id) VALUES (?1, ?2)",
                params![p, tag],
            )?;
        }
        // A new rule also applies to the history.
        auto_tagged = apply_rules_after(&tx, 0, Some(&p))?;
        rule_pattern = Some(p);
    }
    tx.commit()?;
    Ok(TagOutcome {
        tagged,
        rule_pattern,
        auto_tagged,
    })
}

/// Remove `tags` from transactions. Returns how many links were removed.
pub fn untag(conn: &mut Connection, ids: &[i64], tags: &[String]) -> Result<usize> {
    let tx = conn.transaction()?;
    let mut removed = 0;
    for t in tags {
        let name = normalize(t)?;
        for id in ids {
            removed += tx.execute(
                "DELETE FROM transaction_tags WHERE transaction_id = ?1
                 AND tag_id = (SELECT id FROM tags WHERE name = ?2)",
                params![id, name],
            )?;
        }
    }
    tx.commit()?;
    Ok(removed)
}

/// Apply tag rules (all of them, or only `only_pattern`) to transactions with id > `after`.
fn apply_rules_after(conn: &Connection, after: i64, only_pattern: Option<&str>) -> Result<usize> {
    let mut sql = String::from("SELECT id, pattern, tag_id FROM tag_rules");
    let mut args: Vec<Value> = vec![];
    if let Some(p) = only_pattern {
        sql.push_str(" WHERE pattern = ?");
        args.push(p.to_string().into());
    }
    let rules: Vec<(i64, String, i64)> = conn
        .prepare(&sql)?
        .query_map(params_from_iter(args), |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<Result<_, _>>()?;
    if rules.is_empty() {
        return Ok(0);
    }
    let txs: Vec<(i64, String, Option<String>, Option<String>)> = conn
        .prepare("SELECT id, merchant, counterparty, description FROM transactions WHERE id > ?1")?
        .query_map([after], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<_, _>>()?;
    let mut count = 0;
    for (id, merchant, cp, desc) in txs {
        let hay = haystack(&merchant, cp.as_deref(), desc.as_deref());
        for (rule_id, pattern, tag) in &rules {
            if hay.contains(pattern.as_str())
                && conn.execute(
                    "INSERT OR IGNORE INTO transaction_tags (transaction_id, tag_id, source) VALUES (?1, ?2, 'rule')",
                    params![id, tag],
                )? == 1
            {
                conn.execute("UPDATE tag_rules SET hits = hits + 1 WHERE id = ?1", [rule_id])?;
                count += 1;
            }
        }
    }
    Ok(count)
}

/// Apply tag rules to transactions stored since the last call (run after every sync).
/// Only new transactions: a tag the user removed by hand is not put back.
pub fn apply_rules_to_new(conn: &Connection) -> Result<usize> {
    let after: i64 = db::get_setting(conn, APPLIED_UP_TO)?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let count = apply_rules_after(conn, after, None)?;
    let max: Option<i64> = conn.query_row("SELECT MAX(id) FROM transactions", [], |r| r.get(0))?;
    db::set_setting(conn, APPLIED_UP_TO, &max.unwrap_or(after).to_string())?;
    Ok(count)
}

#[derive(Debug, Serialize)]
pub struct TagView {
    pub name: String,
    /// Transactions with this tag in the period.
    pub transactions: i64,
    /// Money out in the period (EUR, positive number).
    pub spent: f64,
    /// Money in in the period (EUR).
    pub received: f64,
    /// Patterns that add this tag automatically.
    pub rules: Vec<String>,
}

/// Every tag with its totals between `from` and `to` (YYYY-MM-DD, inclusive; None = all time).
pub fn list_tags(conn: &Connection, from: Option<&str>, to: Option<&str>) -> Result<Vec<TagView>> {
    let mut st = conn.prepare(
        "SELECT g.name,
                COUNT(t.id),
                COALESCE(SUM(CASE WHEN t.amount_cents < 0 AND t.currency = ?3 THEN -t.amount_cents END), 0),
                COALESCE(SUM(CASE WHEN t.amount_cents > 0 AND t.currency = ?3 THEN t.amount_cents END), 0),
                (SELECT group_concat(r.pattern, char(31)) FROM tag_rules r WHERE r.tag_id = g.id)
         FROM tags g
         LEFT JOIN transaction_tags tt ON tt.tag_id = g.id
         LEFT JOIN transactions t ON t.id = tt.transaction_id
              AND (?1 IS NULL OR t.booking_date >= ?1) AND (?2 IS NULL OR t.booking_date <= ?2)
         GROUP BY g.id
         ORDER BY 3 DESC, g.name",
    )?;
    let rows = st.query_map(params![from, to, db::BASE_CURRENCY], |r| {
        Ok(TagView {
            name: r.get(0)?,
            transactions: r.get(1)?,
            spent: eur(r.get(2)?),
            received: eur(r.get(3)?),
            rules: r
                .get::<_, Option<String>>(4)?
                .map(|s| s.split('\u{1f}').map(String::from).collect())
                .unwrap_or_default(),
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

/// Delete a tag everywhere, with its rules.
pub fn delete_tag(conn: &Connection, name: &str) -> Result<()> {
    let name = normalize(name)?;
    let id: Option<i64> = conn
        .query_row("SELECT id FROM tags WHERE name = ?1", [&name], |r| r.get(0))
        .optional()?;
    let Some(id) = id else {
        bail!("tag {name:?} inesistente");
    };
    conn.execute("DELETE FROM tags WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn insert(conn: &Connection, id: i64, merchant: &str, cents: i64, date: &str) {
        conn.execute(
            "INSERT OR IGNORE INTO accounts (uid, currency) VALUES ('a', 'EUR')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transactions (id, account_uid, booking_date, amount_cents, currency, counterparty, merchant, dedup_key)
             VALUES (?1, 'a', ?4, ?3, 'EUR', ?2, ?2, ?1)",
            params![id, merchant, cents, date],
        )
        .unwrap();
    }

    fn tags_of(conn: &Connection, id: i64) -> Vec<String> {
        conn.prepare("SELECT g.name FROM transaction_tags tt JOIN tags g ON g.id = tt.tag_id WHERE tt.transaction_id = ?1 ORDER BY g.name")
            .unwrap()
            .query_map([id], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn normalizes_names() {
        assert_eq!(normalize("  #Vacanza  Londra ").unwrap(), "vacanza-londra");
        assert!(normalize("a,b").is_err());
        assert!(normalize(" # ").is_err());
    }

    #[test]
    fn many_tags_rules_and_totals() {
        let mut conn = db::open_in_memory().unwrap();
        insert(&conn, 1, "ryanair limited", -8000, "2026-08-01");
        insert(&conn, 2, "hotel londra", -30000, "2026-08-02");
        insert(&conn, 3, "chiara pini", 2000, "2026-08-03");
        insert(&conn, 4, "ryanair limited", -5000, "2026-01-10");

        let out = tag(
            &mut conn,
            &[1, 2],
            &["Vacanza Londra".into(), "viaggi".into()],
            false,
            None,
        )
        .unwrap();
        assert_eq!(out.tagged, 4);
        tag(
            &mut conn,
            &[3],
            &["vacanza-londra".into(), "rimborsi".into()],
            false,
            None,
        )
        .unwrap();
        assert_eq!(tags_of(&conn, 3), vec!["rimborsi", "vacanza-londra"]);

        // A rule tags the history too, and later new transactions.
        let out = tag(&mut conn, &[1], &["voli".into()], true, Some("ryanair")).unwrap();
        assert_eq!(out.auto_tagged, 1); // transaction 4
        apply_rules_to_new(&conn).unwrap();
        insert(&conn, 5, "ryanair limited", -4000, "2026-09-01");
        assert_eq!(apply_rules_to_new(&conn).unwrap(), 1);
        assert_eq!(tags_of(&conn, 5), vec!["voli"]);
        // Removing a tag by hand sticks: rules only touch new transactions.
        untag(&mut conn, &[5], &["voli".into()]).unwrap();
        assert_eq!(apply_rules_to_new(&conn).unwrap(), 0);
        assert!(tags_of(&conn, 5).is_empty());

        let all = list_tags(&conn, Some("2026-08-01"), Some("2026-08-31")).unwrap();
        let trip = all.iter().find(|t| t.name == "vacanza-londra").unwrap();
        assert_eq!(trip.transactions, 3);
        assert_eq!(trip.spent, 380.0);
        assert_eq!(trip.received, 20.0);
        let voli = all.iter().find(|t| t.name == "voli").unwrap();
        assert_eq!(voli.rules, vec!["ryanair"]);
        assert_eq!(voli.transactions, 1); // only the August one in this period

        delete_tag(&conn, "viaggi").unwrap();
        assert_eq!(tags_of(&conn, 1), vec!["vacanza-londra", "voli"]);
    }
}
