//! Dates: calendar periods and the cadence of recurring payments.

use std::str::FromStr;

use chrono::{Datelike, Days, Months, NaiveDate};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::Serialize;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Cadence {
    Week,
    Month,
    Year,
}

impl Cadence {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
        }
    }

    pub fn next(self, d: NaiveDate) -> NaiveDate {
        match self {
            Self::Week => d + Days::new(7),
            Self::Month => d + Months::new(1),
            Self::Year => d + Months::new(12),
        }
    }

    pub fn prev(self, d: NaiveDate) -> NaiveDate {
        match self {
            Self::Week => d - Days::new(7),
            Self::Month => d - Months::new(1),
            Self::Year => d - Months::new(12),
        }
    }

    /// What an amount paid with this cadence costs per month.
    pub fn monthly<T>(self, amount: T) -> T
    where
        T: std::ops::Mul<i64, Output = T> + std::ops::Div<i64, Output = T>,
    {
        match self {
            Self::Week => amount * 52 / 12,
            Self::Month => amount,
            Self::Year => amount / 12,
        }
    }
}

impl FromStr for Cadence {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "week" => Ok(Self::Week),
            "month" => Ok(Self::Month),
            "year" => Ok(Self::Year),
            _ => Err(Error::invalid("cadence non valida: usa week, month o year")),
        }
    }
}

impl ToSql for Cadence {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for Cadence {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value
            .as_str()?
            .parse()
            .map_err(|e: Error| FromSqlError::Other(e.into()))
    }
}

pub fn month_start(d: NaiveDate) -> NaiveDate {
    d.with_day(1).expect("every month has a day 1")
}

pub fn month_end(d: NaiveDate) -> NaiveDate {
    month_start(d) + Months::new(1) - Days::new(1)
}

pub fn week_start(d: NaiveDate) -> NaiveDate {
    d - Days::new(u64::from(d.weekday().num_days_from_monday()))
}

/// Inclusive number of days in [from, to].
pub fn days_between(from: NaiveDate, to: NaiveDate) -> i64 {
    (to - from).num_days() + 1
}

/// A date typed by a person or a client: YYYY-MM-DD.
pub fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| Error::invalid(format!("data non valida (usa YYYY-MM-DD): {s:?}")))
}

/// A month typed by a person or a client (YYYY-MM), as its first day.
pub fn parse_month(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(&format!("{s}-01"), "%Y-%m-%d")
        .map_err(|_| Error::invalid(format!("mese non valido (usa YYYY-MM): {s:?}")))
}

/// Calendar months from `from` to `to` (0 if `to` is in the same month or earlier).
pub fn months_until(from: NaiveDate, to: NaiveDate) -> i64 {
    let months = (to.year() - from.year()) * 12 + to.month() as i32 - from.month() as i32;
    i64::from(months.max(0))
}
