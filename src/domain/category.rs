//! Categories: what kind of money flow they describe and how the dashboard draws them.

use std::fmt;
use std::str::FromStr;

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

// The kind of a category. Transfers between own accounts and savings are never "spending".
// (No doc comment: it would change the JSON schema the MCP clients see.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[schemars(rename = "Kind")]
pub enum CategoryKind {
    Expense,
    Income,
    Transfer,
    Saving,
}

impl CategoryKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Expense => "expense",
            Self::Income => "income",
            Self::Transfer => "transfer",
            Self::Saving => "saving",
        }
    }
}

impl fmt::Display for CategoryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for CategoryKind {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "expense" => Ok(Self::Expense),
            "income" => Ok(Self::Income),
            "transfer" => Ok(Self::Transfer),
            "saving" => Ok(Self::Saving),
            _ => Err(Error::invalid(
                "tipo non valido: usa expense, income, transfer o saving",
            )),
        }
    }
}

impl ToSql for CategoryKind {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.as_str()))
    }
}

impl FromSql for CategoryKind {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        value
            .as_str()?
            .parse()
            .map_err(|e: Error| FromSqlError::Other(e.into()))
    }
}

/// Colors the dashboard knows how to draw.
pub const COLORS: &[&str] = &[
    "slate", "red", "orange", "amber", "lime", "green", "teal", "sky", "blue", "indigo", "violet",
    "pink",
];

/// Empty clears the color; anything else must be one the dashboard knows.
pub fn clean_color(color: &str) -> Result<Option<String>> {
    let color = color.trim();
    if color.is_empty() {
        return Ok(None);
    }
    if !COLORS.contains(&color) {
        crate::invalid!("colore non valido: usa uno tra {}", COLORS.join(", "));
    }
    Ok(Some(color.to_owned()))
}

/// Empty clears the icon; anything else must be a short symbol (an emoji).
pub fn clean_icon(icon: &str) -> Result<Option<String>> {
    let icon = icon.trim();
    if icon.chars().count() > 8 {
        crate::invalid!("icona troppo lunga: usa un'emoji");
    }
    Ok((!icon.is_empty()).then(|| icon.to_owned()))
}

/// The error for a category name that does not exist.
pub fn not_found(name: &str) -> Error {
    Error::not_found(format!(
        "categoria inesistente: {name:?} (usa list_categories o crea la categoria)"
    ))
}

/// A category name as typed by a person: trimmed and not empty.
pub fn clean_name(name: &str) -> Result<&str> {
    let name = name.trim();
    if name.is_empty() {
        crate::invalid!("nome categoria vuoto");
    }
    Ok(name)
}
