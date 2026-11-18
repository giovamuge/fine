//! Categories.

use rusqlite::params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Db;
use crate::domain::category::{self, CategoryKind};
use crate::error::Result;

#[derive(Debug, Serialize)]
pub struct CategoryView {
    pub name: String,
    pub kind: CategoryKind,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub transactions: i64,
    pub rules: i64,
}

/// Changes to a category; `None` leaves the field as it is, an empty icon/color clears it.
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct CategoryPatch {
    /// New name
    pub rename: Option<String>,
    /// "expense", "income", "transfer" or "saving"
    pub kind: Option<String>,
    /// An emoji ("" clears it)
    pub icon: Option<String>,
    /// One of slate, red, orange, amber, lime, green, teal, sky, blue, indigo, violet, pink ("" clears it)
    pub color: Option<String>,
}

impl Db<'_> {
    /// The id of a category by name (case-insensitive).
    pub fn category_id(&self, name: &str) -> Result<i64> {
        self.find_category(name)?
            .ok_or_else(|| category::not_found(name))
    }

    pub fn find_category(&self, name: &str) -> Result<Option<i64>> {
        self.row(
            "SELECT id FROM categories WHERE name = ?1",
            [name.trim()],
            |r| r.get(0),
        )
    }

    pub fn category_name(&self, id: i64) -> Result<String> {
        self.value("SELECT name FROM categories WHERE id = ?1", [id])
    }

    pub fn list_categories(&self) -> Result<Vec<CategoryView>> {
        self.rows(
            "SELECT c.name, c.kind, c.icon, c.color,
                    (SELECT COUNT(*) FROM transactions t WHERE t.category_id = c.id),
                    (SELECT COUNT(*) FROM rules r WHERE r.category_id = c.id)
             FROM categories c ORDER BY c.kind, c.name",
            [],
            |r| {
                Ok(CategoryView {
                    name: r.get(0)?,
                    kind: r.get(1)?,
                    icon: r.get(2)?,
                    color: r.get(3)?,
                    transactions: r.get(4)?,
                    rules: r.get(5)?,
                })
            },
        )
    }

    /// (id, name, kind) of every category, by name.
    pub fn categories(&self) -> Result<Vec<(i64, String, CategoryKind)>> {
        self.rows(
            "SELECT id, name, kind FROM categories ORDER BY name",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
    }

    pub fn category_names(&self) -> Result<Vec<String>> {
        self.rows("SELECT name FROM categories ORDER BY name", [], |r| {
            r.get(0)
        })
    }

    /// The category transfers between own accounts go to.
    pub fn transfer_category_id(&self) -> Result<Option<i64>> {
        self.row(
            "SELECT id FROM categories WHERE kind = 'transfer' ORDER BY id LIMIT 1",
            [],
            |r| r.get(0),
        )
    }

    /// Create a category, or change the kind of an existing one. Returns whether it is new.
    pub fn upsert_category(&self, name: &str, kind: CategoryKind) -> Result<bool> {
        let created = self.find_category(name)?.is_none();
        self.exec(
            "INSERT INTO categories (name, kind) VALUES (?1, ?2) ON CONFLICT(name) DO UPDATE SET kind = excluded.kind",
            params![name, kind],
        )?;
        Ok(created)
    }

    pub fn rename_category(&self, id: i64, name: &str) -> Result<()> {
        self.exec(
            "UPDATE categories SET name = ?1 WHERE id = ?2",
            params![name, id],
        )?;
        Ok(())
    }

    pub fn set_category_kind(&self, id: i64, kind: CategoryKind) -> Result<()> {
        self.exec(
            "UPDATE categories SET kind = ?1 WHERE id = ?2",
            params![kind, id],
        )?;
        Ok(())
    }

    pub fn set_category_icon(&self, id: i64, icon: Option<&str>) -> Result<()> {
        self.exec(
            "UPDATE categories SET icon = ?1 WHERE id = ?2",
            params![icon, id],
        )?;
        Ok(())
    }

    pub fn set_category_color(&self, id: i64, color: Option<&str>) -> Result<()> {
        self.exec(
            "UPDATE categories SET color = ?1 WHERE id = ?2",
            params![color, id],
        )?;
        Ok(())
    }

    pub fn category_transaction_count(&self, id: i64) -> Result<i64> {
        self.value(
            "SELECT COUNT(*) FROM transactions WHERE category_id = ?1",
            [id],
        )
    }

    /// Move transactions, rules and planned items of category `from` to `to` (a merge).
    pub fn move_category_contents(&self, from: i64, to: i64) -> Result<()> {
        for sql in [
            "UPDATE transactions SET category_id = ?1 WHERE category_id = ?2",
            "UPDATE rules SET category_id = ?1 WHERE category_id = ?2",
            "UPDATE planned SET category_id = ?1 WHERE category_id = ?2",
        ] {
            self.exec(sql, params![to, from])?;
        }
        Ok(())
    }

    /// Leave the transactions of a category without one (the AI may look at them again).
    pub fn uncategorize(&self, category_id: i64) -> Result<()> {
        self.exec(
            "UPDATE transactions SET category_id = NULL, categorized_by = NULL, ai_checked_at = NULL
             WHERE category_id = ?1",
            [category_id],
        )?;
        Ok(())
    }

    pub fn delete_category(&self, id: i64) -> Result<()> {
        self.exec("DELETE FROM categories WHERE id = ?1", [id])?;
        Ok(())
    }
}
