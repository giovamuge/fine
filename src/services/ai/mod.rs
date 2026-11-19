//! Background AI categorization through the user's own subscription: fine runs the
//! Claude Code or Codex CLI headless, with no tools, and asks for a structured answer. Bank text
//! is untrusted, so the model only ever *returns data*; fine validates every category before
//! applying it. The worst a malicious description can do is get its own transaction
//! miscategorized.

mod backend;
mod prompt;

use std::collections::HashMap;

use serde::Serialize;

pub use backend::Backend;
use prompt::{Answer, Group};

use crate::domain::text;
use crate::domain::transaction::CategorizedBy;
use crate::error::Result;
use crate::storage::Store;

#[derive(Debug, Default, Serialize)]
pub struct AiOutcome {
    pub backend: String,
    /// Transactions the AI looked at.
    pub examined: usize,
    /// Transactions it categorized (directly or through a rule it created).
    pub categorized: usize,
    pub rules_created: Vec<String>,
}

/// Categorize, with the AI, the uncategorized transactions it has not seen yet.
/// Returns `Ok(None)` when there is nothing to do.
pub async fn categorize_pending(store: &mut Store, backend: &Backend) -> Result<Option<AiOutcome>> {
    let (groups, input) = {
        let db = store.db();
        let groups = prompt::pending_groups(db)?;
        if groups.is_empty() {
            return Ok(None);
        }
        let categories: Vec<_> = db
            .categories()?
            .into_iter()
            .map(|(_, n, k)| (n, k))
            .collect();
        let input = prompt::prompt(&categories, &groups)?;
        (groups, input)
    };
    let answer: Answer = backend.ask(&input, &prompt::schema()).await?;
    let mut out = apply(store, &groups, answer)?;
    out.backend = backend.name().to_owned();
    Ok(Some(out))
}

/// Apply the model's answer. Only known categories and keys are accepted; AI rules never
/// override a rule that already exists (seeded or taught by the user).
fn apply(store: &mut Store, groups: &[Group], answer: Answer) -> Result<AiOutcome> {
    let by_key: HashMap<&str, &Group> = groups.iter().map(|g| (g.key.as_str(), g)).collect();
    store.write(|db| {
        let categories = db.categories()?;
        let category_id = |name: &str| {
            let name = name.trim();
            categories
                .iter()
                .find(|(_, n, _)| n.eq_ignore_ascii_case(name))
                .map(|(id, _, _)| *id)
        };
        let mut out = AiOutcome::default();
        for a in answer.assignments {
            let (Some(group), Some(category)) = (
                by_key.get(a.key.as_str()),
                a.category.as_deref().and_then(category_id),
            ) else {
                continue;
            };
            for &id in &group.ids {
                out.categorized += db.set_category_if_none(id, category, CategorizedBy::Ai)?;
            }
            let pattern = text::normalize(&group.key);
            if a.learn_rule
                && !pattern.starts_with('#')
                && text::significant_chars(&pattern) >= 4
                && db.add_ai_rule(&pattern, category)?
            {
                out.rules_created.push(pattern);
            }
        }
        let now = chrono::Utc::now().to_rfc3339();
        for id in groups.iter().flat_map(|g| &g.ids) {
            db.mark_ai_checked(*id, &now)?;
        }
        out.categorized += db.apply_category_rules()?;
        out.examined = groups.iter().map(|g| g.ids.len()).sum();
        Ok(out)
    })
}

/// Let transactions the AI already skipped be proposed again (e.g. after adding categories).
pub fn reset_checked(store: &mut Store) -> Result<usize> {
    store.db().reset_ai_checked()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::testing::{add_tx_with_id, category_of};
    use serde_json::json;

    #[test]
    fn applies_only_valid_answers_and_never_asks_twice() {
        let mut store = Store::open_in_memory().unwrap();
        add_tx_with_id(&store, 1, "github inc.", -1000, "2026-01-01");
        add_tx_with_id(&store, 2, "github inc.", -1000, "2026-01-01");
        add_tx_with_id(&store, 3, "chiara pini", -1000, "2026-01-01");
        add_tx_with_id(&store, 4, "negozio strano", -1000, "2026-01-01");
        let groups = prompt::pending_groups(store.db()).unwrap();
        assert_eq!(groups[0].key, "github inc.");
        assert_eq!(groups[0].ids.len(), 2);

        let answer: Answer = serde_json::from_value(json!({ "assignments": [
            { "key": "github inc.", "category": "abbonamenti", "learn_rule": true },
            { "key": "chiara pini", "category": null, "learn_rule": false },
            { "key": "negozio strano", "category": "Categoria inventata", "learn_rule": true },
            { "key": "chiave inventata", "category": "Svago", "learn_rule": true }
        ]}))
        .unwrap();
        let out = apply(&mut store, &groups, answer).unwrap();

        assert_eq!(out.examined, 4);
        assert_eq!(out.categorized, 2);
        assert_eq!(out.rules_created, vec!["github inc.".to_owned()]);
        assert_eq!(category_of(&store, 1).as_deref(), Some("Abbonamenti"));
        assert_eq!(category_of(&store, 3), None);
        assert_eq!(category_of(&store, 4), None);
        // Already examined: not sent again on the next run.
        assert!(prompt::pending_groups(store.db()).unwrap().is_empty());
        // A new transaction from a learned merchant is categorized by the rule, without AI.
        add_tx_with_id(&store, 5, "github inc.", -1000, "2026-01-01");
        crate::services::categories::apply_rules(&mut store).unwrap();
        assert_eq!(category_of(&store, 5).as_deref(), Some("Abbonamenti"));
        // Reset makes the skipped ones eligible again.
        assert_eq!(reset_checked(&mut store).unwrap(), 2);
    }
}
