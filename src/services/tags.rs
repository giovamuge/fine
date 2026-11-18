//! Tagging transactions, by hand or with rules that also tag future transactions.

use serde::Serialize;

use crate::domain::rules::learnable_pattern;
use crate::domain::tag;
use crate::error::Result;
use crate::storage::{Store, TagView};

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
    store: &mut Store,
    ids: &[i64],
    tags: &[String],
    learn_rule: bool,
    pattern: Option<&str>,
) -> Result<TagOutcome> {
    if ids.is_empty() {
        crate::invalid!("nessuna transazione indicata");
    }
    if tags.is_empty() {
        crate::invalid!("nessun tag indicato");
    }
    let names = tags
        .iter()
        .map(|t| tag::normalize(t))
        .collect::<Result<Vec<_>>>()?;
    store.write(|db| {
        let tag_ids = names
            .iter()
            .map(|n| db.ensure_tag(n))
            .collect::<Result<Vec<_>>>()?;
        let mut tagged = 0;
        let mut first_merchant = None;
        for &id in ids {
            first_merchant.get_or_insert(db.merchant_of(id)?);
            for &tag_id in &tag_ids {
                tagged += usize::from(db.tag_transaction(id, tag_id)?);
            }
        }
        let (rule_pattern, auto_tagged) = if learn_rule {
            let p = learnable_pattern(pattern, first_merchant)?;
            for &tag_id in &tag_ids {
                db.add_tag_rule(&p, tag_id)?;
            }
            // A new rule also applies to the history.
            let auto = db.apply_tag_rules_after(0, Some(&p))?;
            (Some(p), auto)
        } else {
            (None, 0)
        };
        Ok(TagOutcome {
            tagged,
            rule_pattern,
            auto_tagged,
        })
    })
}

/// Remove `tags` from transactions. Returns how many links were removed.
pub fn untag(store: &mut Store, ids: &[i64], tags: &[String]) -> Result<usize> {
    store.write(|db| {
        let mut removed = 0;
        for t in tags {
            let name = tag::normalize(t)?;
            for &id in ids {
                removed += db.untag_transaction(id, &name)?;
            }
        }
        Ok(removed)
    })
}

/// Every tag with its totals between `from` and `to` (YYYY-MM-DD, inclusive; None = all time).
pub fn list(store: &Store, from: Option<&str>, to: Option<&str>) -> Result<Vec<TagView>> {
    store.db().list_tags(from, to)
}

/// Delete a tag everywhere, with its rules.
pub fn delete(store: &mut Store, name: &str) -> Result<()> {
    store.db().delete_tag(&tag::normalize(name)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::testing::{add_tx_with_id, tags_of};

    #[test]
    fn many_tags_rules_and_totals() {
        let mut store = Store::open_in_memory().unwrap();
        add_tx_with_id(&store, 1, "ryanair limited", -8000, "2026-08-01");
        add_tx_with_id(&store, 2, "hotel londra", -30000, "2026-08-02");
        add_tx_with_id(&store, 3, "chiara pini", 2000, "2026-08-03");
        add_tx_with_id(&store, 4, "ryanair limited", -5000, "2026-01-10");

        let out = tag(
            &mut store,
            &[1, 2],
            &["Vacanza Londra".into(), "viaggi".into()],
            false,
            None,
        )
        .unwrap();
        assert_eq!(out.tagged, 4);
        tag(
            &mut store,
            &[3],
            &["vacanza-londra".into(), "rimborsi".into()],
            false,
            None,
        )
        .unwrap();
        assert_eq!(tags_of(&store, 3), vec!["rimborsi", "vacanza-londra"]);

        // A rule tags the history too, and later new transactions.
        let out = tag(&mut store, &[1], &["voli".into()], true, Some("ryanair")).unwrap();
        assert_eq!(out.auto_tagged, 1); // transaction 4
        store.db().apply_tag_rules_to_new().unwrap();
        add_tx_with_id(&store, 5, "ryanair limited", -4000, "2026-09-01");
        assert_eq!(store.db().apply_tag_rules_to_new().unwrap(), 1);
        assert_eq!(tags_of(&store, 5), vec!["voli"]);
        // Removing a tag by hand sticks: rules only touch new transactions.
        untag(&mut store, &[5], &["voli".into()]).unwrap();
        assert_eq!(store.db().apply_tag_rules_to_new().unwrap(), 0);
        assert!(tags_of(&store, 5).is_empty());

        let all = list(&store, Some("2026-08-01"), Some("2026-08-31")).unwrap();
        let trip = all.iter().find(|t| t.name == "vacanza-londra").unwrap();
        assert_eq!(trip.transactions, 3);
        assert_eq!(trip.spent.to_eur(), 380.0);
        assert_eq!(trip.received.to_eur(), 20.0);
        let voli = all.iter().find(|t| t.name == "voli").unwrap();
        assert_eq!(voli.rules, vec!["ryanair"]);
        assert_eq!(voli.transactions, 1); // only the August one in this period

        delete(&mut store, "viaggi").unwrap();
        assert_eq!(tags_of(&store, 1), vec!["vacanza-londra", "voli"]);
    }
}
