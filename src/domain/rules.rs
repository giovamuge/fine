//! Pattern rules: a lowercase substring matched against a transaction's padded, normalized text
//! (see `text::haystack`). Categorization rules pick one category (the longest match wins);
//! tag rules add every matching tag.

use super::text;

/// A pattern pointing at a target (a category or a tag id).
#[derive(Debug, Clone)]
pub struct PatternRule {
    pub id: i64,
    pub pattern: String,
    pub target: i64,
}

impl PatternRule {
    pub fn matches(&self, haystack: &str) -> bool {
        haystack.contains(self.pattern.as_str())
    }
}

/// The rule with the longest matching pattern, if any.
pub fn best_match<'r>(rules: &'r [PatternRule], haystack: &str) -> Option<&'r PatternRule> {
    rules
        .iter()
        .filter(|r| r.matches(haystack))
        .max_by_key(|r| r.pattern.trim().len())
}

/// The pattern to learn: the one given (normalized) or the transaction's merchant.
/// Too short a pattern would match almost everything.
pub fn learnable_pattern(given: Option<&str>, merchant: Option<String>) -> crate::Result<String> {
    let p = match given {
        Some(p) => text::normalize(p),
        None => merchant.unwrap_or_default(),
    };
    if text::significant_chars(&p) < 3 {
        crate::invalid!("pattern troppo corto per una regola: {p:?}");
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: i64, pattern: &str) -> PatternRule {
        PatternRule {
            id,
            pattern: pattern.into(),
            target: id,
        }
    }

    #[test]
    fn longest_pattern_wins() {
        let rules = [rule(1, "uber"), rule(2, "uber eats")];
        let hay = text::haystack("uber eats ordine", None, None);
        assert_eq!(best_match(&rules, &hay).map(|r| r.id), Some(2));
        assert!(best_match(&rules, " lyft ").is_none());
    }

    #[test]
    fn short_patterns_are_rejected() {
        assert!(learnable_pattern(Some("ab"), None).is_err());
        assert_eq!(
            learnable_pattern(None, Some("esselunga".into())).unwrap(),
            "esselunga"
        );
    }
}
