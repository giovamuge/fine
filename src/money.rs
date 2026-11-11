//! Money is always stored as integer cents (i64). Never use floats for arithmetic.

use anyhow::{Context, Result, bail};

/// Parse a decimal amount string like "12.34", "-5", "1234.5" into cents.
pub fn parse_cents(s: &str) -> Result<i64> {
    let s = s.trim();
    let (neg, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (int_part, frac_part) = match digits.split_once('.') {
        Some((i, f)) => (i, f),
        None => (digits, ""),
    };
    if int_part.is_empty() && frac_part.is_empty() {
        bail!("importo vuoto: {s:?}");
    }
    if !int_part.chars().all(|c| c.is_ascii_digit())
        || !frac_part.chars().all(|c| c.is_ascii_digit())
    {
        bail!("importo non valido: {s:?}");
    }
    if frac_part.len() > 2 && frac_part[2..].chars().any(|c| c != '0') {
        bail!("importo con più di due decimali: {s:?}");
    }
    let int: i64 = if int_part.is_empty() {
        0
    } else {
        int_part.parse().context("importo troppo grande")?
    };
    let mut frac = frac_part.chars().take(2).collect::<String>();
    while frac.len() < 2 {
        frac.push('0');
    }
    let cents = int
        .checked_mul(100)
        .and_then(|v| v.checked_add(frac.parse::<i64>().unwrap_or(0)))
        .context("importo troppo grande")?;
    Ok(if neg { -cents } else { cents })
}

/// Convert a user-facing euro amount (e.g. from MCP JSON) into cents, rounding to the nearest cent.
pub fn eur_to_cents(eur: f64) -> Result<i64> {
    if !eur.is_finite() || eur.abs() > 1e13 {
        bail!("importo non valido: {eur}");
    }
    Ok((eur * 100.0).round() as i64)
}

/// Cents as a float in euro, for JSON output only (display, not arithmetic).
pub fn eur(cents: i64) -> f64 {
    cents as f64 / 100.0
}

/// Italian formatting: 1.234,56 €
pub fn fmt(cents: i64) -> String {
    let neg = cents < 0;
    let abs = cents.unsigned_abs();
    let int = (abs / 100).to_string();
    let mut grouped = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i).is_multiple_of(3) {
            grouped.push('.');
        }
        grouped.push(ch);
    }
    format!(
        "{}{},{:02} €",
        if neg { "-" } else { "" },
        grouped,
        abs % 100
    )
}

/// Format a display euro value (from JSON-facing f64) in Italian style.
pub fn fmt_eur(eur: f64) -> String {
    fmt((eur * 100.0).round() as i64)
}

/// Split `total` into `n` parts that sum exactly to `total` (remainder goes to the first parts).
pub fn split(total: i64, n: i64) -> Vec<i64> {
    if n <= 0 {
        return vec![];
    }
    let base = total / n;
    let rem = total % n;
    (0..n)
        .map(|i| base + if i < rem.abs() { rem.signum() } else { 0 })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        assert_eq!(parse_cents("12.34").unwrap(), 1234);
        assert_eq!(parse_cents("-5").unwrap(), -500);
        assert_eq!(parse_cents("0.5").unwrap(), 50);
        assert_eq!(parse_cents("1234.500").unwrap(), 123450);
        assert!(parse_cents("1.234").is_err());
        assert!(parse_cents("abc").is_err());
        assert!(parse_cents("").is_err());
    }

    #[test]
    fn formats() {
        assert_eq!(fmt(123456), "1.234,56 €");
        assert_eq!(fmt(-5), "-0,05 €");
        assert_eq!(fmt(100000000), "1.000.000,00 €");
    }

    #[test]
    fn splits_exactly() {
        let parts = split(1000, 3);
        assert_eq!(parts, vec![334, 333, 333]);
        assert_eq!(split(-1000, 3).iter().sum::<i64>(), -1000);
    }
}
