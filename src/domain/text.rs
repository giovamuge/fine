//! Text normalization shared by rules, tags, budgets and planned items. Bank text is noisy
//! ("PAGAMENTO POS 12/03 ESSELUNGA MILANO CARTA *1234"): everything that matches on it goes
//! through the same functions so a pattern means the same thing everywhere.

/// Prefixes banks put before the merchant name, longest first where they overlap.
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

const NOISE_WORDS: &[&str] = &["carta", "ore", "del", "il", "n.", "num", "c", "eur"];

/// Lowercase, collapse whitespace.
pub fn normalize(s: &str) -> String {
    s.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lowercase without any whitespace, for loose matching of account names and IBANs.
pub fn squash(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// A stable merchant key from counterparty / description,
/// e.g. "PAGAMENTO POS 12/03 ESSELUNGA MILANO CARTA *1234" -> "esselunga milano".
pub fn merchant_key(counterparty: Option<&str>, description: Option<&str>) -> String {
    let source = counterparty
        .filter(|c| !c.trim().is_empty())
        .or(description)
        .unwrap_or("");
    let cleaned: String = source
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
    let mut text = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    loop {
        let before = text.len();
        for p in NOISE_PREFIXES {
            if let Some(rest) = text.strip_prefix(p)
                && (rest.is_empty() || rest.starts_with(' '))
            {
                text = rest.trim_start().to_owned();
            }
        }
        if text.len() == before {
            break;
        }
    }
    text.split_whitespace()
        // drop dates, card numbers, codes: tokens with digits that are not mostly letters
        .filter(|w| w.chars().filter(char::is_ascii_digit).count() * 2 < w.chars().count())
        .filter(|w| !NOISE_WORDS.contains(w))
        .take(4)
        .collect::<Vec<_>>()
        .join(" ")
}

/// The text a pattern is matched against: " merchant counterparty description ", normalized and
/// padded so a pattern like " eni " only matches the whole word.
pub fn haystack(merchant: &str, counterparty: Option<&str>, description: Option<&str>) -> String {
    format!(
        " {} {} {} ",
        merchant,
        normalize(counterparty.unwrap_or("")),
        normalize(description.unwrap_or(""))
    )
}

/// Letters and digits in a pattern: too few would match almost everything.
pub fn significant_chars(pattern: &str) -> usize {
    pattern.chars().filter(|c| c.is_alphanumeric()).count()
}

/// Trim and cut to `max` characters (prompts and previews of untrusted bank text).
pub fn clip(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_merchants() {
        assert_eq!(
            merchant_key(
                None,
                Some("PAGAMENTO POS 12/03/2026 ORE 12:31 ESSELUNGA MILANO CARTA *1234")
            ),
            "esselunga milano"
        );
        assert_eq!(
            merchant_key(Some("Netflix.com"), Some("abbonamento")),
            "netflix.com"
        );
        assert_eq!(
            merchant_key(None, Some("ADDEBITO SDD ENEL ENERGIA SPA 0012345")),
            "enel energia spa"
        );
    }

    #[test]
    fn haystack_pads_words() {
        assert_eq!(haystack("eni", Some(" ENI  Spa"), None), " eni eni spa  ");
    }
}
