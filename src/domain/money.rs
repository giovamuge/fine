//! Money as integer cents. Arithmetic never touches floats: euros as `f64` appear only at the
//! edges (JSON in and out).

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

use rusqlite::types::{FromSql, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Serialize, Serializer};

use crate::error::{Error, Result};
use crate::i18n;

/// An amount of money in cents (negative = money out). Serialized as euros (`12.5`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Money(i64);

impl Money {
    pub const ZERO: Self = Self(0);

    pub const fn from_cents(cents: i64) -> Self {
        Self(cents)
    }

    pub const fn cents(self) -> i64 {
        self.0
    }

    /// A user-facing euro amount (e.g. from JSON), rounded to the nearest cent.
    pub fn from_eur(eur: f64) -> Result<Self> {
        if !eur.is_finite() || eur.abs() > 1e13 {
            crate::invalid!("importo non valido: {eur}");
        }
        Ok(Self((eur * 100.0).round() as i64))
    }

    /// Like `from_eur` for values already validated (a non-finite value saturates).
    pub fn from_eur_lossy(eur: f64) -> Self {
        Self((eur * 100.0).round() as i64)
    }

    /// Euros as a float, for JSON output only (display, never arithmetic).
    pub fn to_eur(self) -> f64 {
        self.0 as f64 / 100.0
    }

    /// Parse a decimal string like "12.34", "-5", "1234.5" (bank amounts).
    pub fn parse(s: &str) -> Result<Self> {
        let s = s.trim();
        let (neg, digits) = match s.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, s.strip_prefix('+').unwrap_or(s)),
        };
        let (int_part, frac_part) = digits.split_once('.').unwrap_or((digits, ""));
        if int_part.is_empty() && frac_part.is_empty() {
            crate::invalid!("importo vuoto: {s:?}");
        }
        let all_digits = |p: &str| p.bytes().all(|b| b.is_ascii_digit());
        if !all_digits(int_part) || !all_digits(frac_part) {
            crate::invalid!("importo non valido: {s:?}");
        }
        if frac_part.len() > 2 && frac_part[2..].bytes().any(|b| b != b'0') {
            crate::invalid!("importo con più di due decimali: {s:?}");
        }
        let too_big = || Error::invalid("importo troppo grande");
        let int: i64 = if int_part.is_empty() {
            0
        } else {
            int_part.parse().map_err(|_| too_big())?
        };
        let frac = frac_part
            .bytes()
            .chain(std::iter::repeat(b'0'))
            .take(2)
            .fold(0, |acc, b| acc * 10 + i64::from(b - b'0'));
        let cents = int
            .checked_mul(100)
            .and_then(|v| v.checked_add(frac))
            .ok_or_else(too_big)?;
        Ok(Self(if neg { -cents } else { cents }))
    }

    pub const fn abs(self) -> Self {
        Self(self.0.abs())
    }

    pub const fn is_positive(self) -> bool {
        self.0 > 0
    }

    pub const fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub fn max(self, other: Self) -> Self {
        Ord::max(self, other)
    }

    pub fn min(self, other: Self) -> Self {
        Ord::min(self, other)
    }

    /// `self / other` as a fraction (for percentages); 0 when `other` is zero.
    pub fn ratio(self, other: Self) -> f64 {
        if other.0 == 0 {
            0.0
        } else {
            self.0 as f64 / other.0 as f64
        }
    }

    /// Split into `n` parts that sum exactly to `self` (the remainder goes to the first parts).
    pub fn split(self, n: i64) -> Vec<Self> {
        if n <= 0 {
            return vec![];
        }
        let (base, rem) = (self.0 / n, self.0 % n);
        (0..n)
            .map(|i| Self(base + if i < rem.abs() { rem.signum() } else { 0 }))
            .collect()
    }
}

impl Add for Money {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl Sub for Money {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl Neg for Money {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl SubAssign for Money {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Mul<i64> for Money {
    type Output = Self;
    fn mul(self, rhs: i64) -> Self {
        Self(self.0 * rhs)
    }
}

/// Integer division of the cents (truncates towards zero).
impl Div<i64> for Money {
    type Output = Self;
    fn div(self, rhs: i64) -> Self {
        Self(self.0 / rhs)
    }
}

impl Sum for Money {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        Self(iter.map(|m| m.0).sum())
    }
}

impl<'a> Sum<&'a Money> for Money {
    fn sum<I: Iterator<Item = &'a Money>>(iter: I) -> Self {
        iter.copied().sum()
    }
}

/// In the current language: `1.234,56 €` in Italian, `€1,234.56` in English.
impl fmt::Display for Money {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let en = i18n::is_en();
        let abs = self.0.unsigned_abs();
        let int = (abs / 100).to_string();
        let mut grouped = String::with_capacity(int.len() + int.len() / 3);
        for (i, ch) in int.chars().enumerate() {
            if i > 0 && (int.len() - i).is_multiple_of(3) {
                grouped.push(if en { ',' } else { '.' });
            }
            grouped.push(ch);
        }
        let sign = if self.0 < 0 { "-" } else { "" };
        if en {
            write!(f, "{sign}€{grouped}.{:02}", abs % 100)
        } else {
            write!(f, "{sign}{grouped},{:02} €", abs % 100)
        }
    }
}

impl Serialize for Money {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_f64(self.to_eur())
    }
}

impl ToSql for Money {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0))
    }
}

impl FromSql for Money {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        i64::column_result(value).map(Self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<i64> {
        Money::parse(s).map(Money::cents)
    }

    #[test]
    fn parses() {
        assert_eq!(parse("12.34").unwrap(), 1234);
        assert_eq!(parse("-5").unwrap(), -500);
        assert_eq!(parse("0.5").unwrap(), 50);
        assert_eq!(parse("1234.500").unwrap(), 123450);
        assert!(parse("1.234").is_err());
        assert!(parse("abc").is_err());
        assert!(parse("").is_err());
    }

    #[test]
    fn formats() {
        assert_eq!(Money::from_cents(123456).to_string(), "1.234,56 €");
        assert_eq!(Money::from_cents(-5).to_string(), "-0,05 €");
        assert_eq!(Money::from_cents(100000000).to_string(), "1.000.000,00 €");
    }

    #[test]
    fn splits_exactly() {
        let parts: Vec<i64> = Money::from_cents(1000)
            .split(3)
            .into_iter()
            .map(Money::cents)
            .collect();
        assert_eq!(parts, vec![334, 333, 333]);
        assert_eq!(
            Money::from_cents(-1000).split(3).into_iter().sum::<Money>(),
            Money::from_cents(-1000)
        );
    }

    #[test]
    fn serializes_as_euros() {
        assert_eq!(
            serde_json::to_string(&Money::from_cents(1250)).unwrap(),
            "12.5"
        );
    }
}
