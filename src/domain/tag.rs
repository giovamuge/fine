//! Tags: free-form labels, any number per transaction, orthogonal to the single category.
//! The category drives budgets and the "available" figure; tags group transactions in any other
//! way the user likes (a trip, a project, a person, "rimborsabile", ...).

use crate::error::Result;

const MAX_LEN: usize = 40;

/// The canonical tag name: "#Vacanza  Londra" -> "vacanza-londra".
/// Commas are not allowed (they separate tags in lists).
pub fn normalize(name: &str) -> Result<String> {
    let n = name
        .trim()
        .trim_start_matches('#')
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");
    if n.is_empty() {
        crate::invalid!("tag vuoto");
    }
    if n.contains(',') {
        crate::invalid!("un tag non può contenere virgole: {name:?}");
    }
    if n.chars().count() > MAX_LEN {
        crate::invalid!("tag troppo lungo (max {MAX_LEN} caratteri): {name:?}");
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_names() {
        assert_eq!(normalize("  #Vacanza  Londra ").unwrap(), "vacanza-londra");
        assert!(normalize("a,b").is_err());
        assert!(normalize(" # ").is_err());
    }
}
