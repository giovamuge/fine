//! Dynamic budgets: named budgets whose rules live in the database.
//!
//! A budget has a goal (spend at most / save at least / earn at least), a rule that picks the
//! transactions that count (a tree of `all` / `any` / `not` over typed conditions), a period
//! (recurring week/month/quarter/year with a custom start, the last N days, or fixed dates),
//! an amount, and options: rollover, amortized or cash basis, alert thresholds, and whether its
//! unspent part is reserved out of the month's "available".
//!
//! Rules are structured data, not formulas: they are validated when saved and evaluated in Rust,
//! so a budget written by the AI can never break the analysis. Category names are kept in sync
//! when categories are renamed or merged; a rule naming a deleted category is reported as a problem.

use crate::tr;
use anyhow::{Result, bail};
use chrono::{Datelike, Days, Months, NaiveDate};
use rusqlite::{Connection, OptionalExtension, params};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::analysis::{self, Kind, Planned, Tx};
use crate::categorize::normalize_text;
use crate::money::{eur, eur_to_cents, fmt_eur};

const MAX_DEPTH: usize = 6;
const MAX_CONDITIONS: usize = 50;
/// Previous periods looked at for rollover and history.
const LOOKBACK: usize = 12;
const HISTORY: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Goal {
    /// Spend at most `amount` (expenses; refunds reduce the total)
    #[default]
    MaxSpend,
    /// Put aside at least `amount` (net transfers to "saving" categories)
    MinSave,
    /// Receive at least `amount` (income)
    MinIncome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Period {
    /// Every week. `start_weekday`: 1 = Monday (default) ... 7 = Sunday
    Week {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_weekday: Option<u32>,
    },
    /// Every month. `start_day`: 1-28 (default 1), e.g. 27 for a month that starts on payday
    Month {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_day: Option<u32>,
    },
    /// Calendar quarters
    Quarter,
    /// Every year. `start_month`: 1-12 (default 1 = January)
    Year {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_month: Option<u32>,
    },
    /// The last `days` days, ending today (a moving window)
    Rolling { days: u32 },
    /// Fixed dates, e.g. a trip or a project: `from` and `to` as YYYY-MM-DD, inclusive
    Custom { from: String, to: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rollover {
    #[default]
    None,
    /// What is left unspent is added to the next period
    Surplus,
    /// Overspending is taken from the next period
    Deficit,
    /// Both
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// Like the rest of fine: amortized purchases count 1/N per month and payments of annual
    /// planned expenses are left out (they have their own sinking fund)
    #[default]
    Amortized,
    /// Every matching transaction counts in full on the day it was booked
    Cash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// Money out
    Out,
    /// Money in
    In,
}

/// Which transactions count. Combine conditions with `all`, `any`, `not`.
/// Examples: `{"category": ["Ristoranti e bar"]}`;
/// `{"all": [{"category": ["Ristoranti e bar", "Svago"]}, {"not": {"tag": ["lavoro"]}}]}`;
/// `{"text": ["amazon"]}`; `{"not": {"category": ["Spesa alimentare"]}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// Every condition must match
    All(Vec<Rule>),
    /// At least one condition must match
    Any(Vec<Rule>),
    /// The condition must not match
    Not(Box<Rule>),
    /// The transaction's category is one of these (existing category names)
    Category(Vec<String>),
    /// The category kind is one of these: expense, income, transfer, saving
    CategoryKind(Vec<Kind>),
    /// The transaction has at least one of these tags
    Tag(Vec<String>),
    /// Merchant, counterparty or description contains one of these texts
    Text(Vec<String>),
    /// Booked on an account whose name, bank or IBAN contains one of these texts
    Account(Vec<String>),
    /// Absolute amount in euro between `min` and `max` (inclusive; either may be omitted)
    Amount {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    /// "out" (money out) or "in" (money in, e.g. refunds)
    Direction(Direction),
    /// Day of the week, 1 = Monday ... 7 = Sunday
    Weekday(Vec<u32>),
    /// Day of the month between `from` and `to` (1-31, inclusive)
    DayOfMonth { from: u32, to: u32 },
    /// true: only uncategorized transactions; false: only categorized ones
    Uncategorized(bool),
    /// true: only payments of weekly/monthly planned expenses (rent, bills); false: only the others
    Fixed(bool),
}

fn default_alerts() -> Vec<u32> {
    vec![80, 100]
}
fn yes() -> bool {
    true
}
fn is_true(b: &bool) -> bool {
    *b
}

/// A budget definition, as written by the AI or the dashboard and stored in the database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Budget {
    /// Unique name, e.g. "Svago", "Viaggio Londra", "Amazon"
    pub name: String,
    /// "max_spend" (default), "min_save" or "min_income"
    #[serde(default)]
    pub goal: Goal,
    /// Which transactions count. Omit to count all expenses (max_spend), all savings
    /// (min_save) or all income (min_income).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<Rule>,
    pub period: Period,
    /// Limit (max_spend) or target (min_save, min_income) in euro per period
    pub amount: f64,
    /// Carry what is left (or the overspending) into the next period; max_spend with a
    /// recurring period only
    #[serde(default)]
    pub rollover: Rollover,
    /// Maximum carried amount in euro (either way)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollover_cap: Option<f64>,
    /// "amortized" (default) or "cash"
    #[serde(default)]
    pub basis: Basis,
    /// Alert when these percentages of the amount are used (default [80, 100])
    #[serde(default = "default_alerts")]
    pub alerts: Vec<u32>,
    /// Also alert when spending runs ahead of the time elapsed in the period (default true)
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub pace_alert: bool,
    /// Treat the unspent part as already committed: it is taken out of the month's
    /// "available" (default false)
    #[serde(default)]
    pub reserve: bool,
    /// Paused budgets are kept but not evaluated or alerted on (default true)
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Periods

fn ymd(y: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(y, m, d).expect("valid date")
}

pub fn parse_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
        .map_err(|_| anyhow::anyhow!("data non valida {s:?}: usa il formato AAAA-MM-GG"))
}

impl Period {
    fn recurring(&self) -> bool {
        matches!(
            self,
            Self::Week { .. } | Self::Month { .. } | Self::Quarter | Self::Year { .. }
        )
    }

    /// The period containing `d` (for `custom`, its fixed dates).
    fn range(&self, d: NaiveDate) -> Result<(NaiveDate, NaiveDate)> {
        Ok(match *self {
            Self::Week { start_weekday } => {
                let sw = start_weekday.unwrap_or(1);
                let back = (d.weekday().number_from_monday() + 7 - sw) % 7;
                let start = d - Days::new(back as u64);
                (start, start + Days::new(6))
            }
            Self::Month { start_day } => {
                let sd = start_day.unwrap_or(1);
                let base = if d.day() >= sd { d } else { d - Months::new(1) };
                let start = ymd(base.year(), base.month(), sd);
                (start, start + Months::new(1) - Days::new(1))
            }
            Self::Quarter => {
                let start = ymd(d.year(), (d.month() - 1) / 3 * 3 + 1, 1);
                (start, start + Months::new(3) - Days::new(1))
            }
            Self::Year { start_month } => {
                let sm = start_month.unwrap_or(1);
                let y = if d.month() >= sm {
                    d.year()
                } else {
                    d.year() - 1
                };
                let start = ymd(y, sm, 1);
                (start, start + Months::new(12) - Days::new(1))
            }
            Self::Rolling { days } => (d - Days::new(days.max(1) as u64 - 1), d),
            Self::Custom { ref from, ref to } => (parse_date(from)?, parse_date(to)?),
        })
    }

    fn label(&self) -> String {
        let en = crate::i18n::en();
        let days: [&str; 7] = if en {
            [
                "Monday",
                "Tuesday",
                "Wednesday",
                "Thursday",
                "Friday",
                "Saturday",
                "Sunday",
            ]
        } else {
            [
                "lunedì",
                "martedì",
                "mercoledì",
                "giovedì",
                "venerdì",
                "sabato",
                "domenica",
            ]
        };
        let months: [&str; 12] = if en {
            [
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ]
        } else {
            [
                "gennaio",
                "febbraio",
                "marzo",
                "aprile",
                "maggio",
                "giugno",
                "luglio",
                "agosto",
                "settembre",
                "ottobre",
                "novembre",
                "dicembre",
            ]
        };
        match self {
            Self::Week {
                start_weekday: None | Some(1),
            } => tr!("ogni settimana", "every week"),
            Self::Week {
                start_weekday: Some(w),
            } => {
                let day = days[(*w as usize).clamp(1, 7) - 1];
                tr!("ogni settimana, da {day}", "every week, from {day}")
            }
            Self::Month {
                start_day: None | Some(1),
            } => tr!("ogni mese", "every month"),
            Self::Month { start_day: Some(d) } => {
                tr!("ogni mese, dal giorno {d}", "every month, from day {d}")
            }
            Self::Quarter => tr!("ogni trimestre", "every quarter"),
            Self::Year {
                start_month: None | Some(1),
            } => tr!("ogni anno", "every year"),
            Self::Year {
                start_month: Some(m),
            } => {
                let month = months[(*m as usize).clamp(1, 12) - 1];
                tr!("ogni anno, da {month}", "every year, from {month}")
            }
            Self::Rolling { days } => tr!("ultimi {days} giorni", "last {days} days"),
            Self::Custom { from, to } => match (parse_date(from), parse_date(to)) {
                (Ok(f), Ok(t)) => {
                    let (f, t) = (f.format("%d/%m/%Y"), t.format("%d/%m/%Y"));
                    tr!("dal {f} al {t}", "from {f} to {t}")
                }
                _ => tr!("dal {from} al {to}", "from {from} to {to}"),
            },
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Rules

/// What a rule sees of a transaction.
struct Subject<'a> {
    tx: &'a Tx,
    fixed: bool,
}

fn squash(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

impl Rule {
    fn matches(&self, s: &Subject) -> bool {
        let tx = s.tx;
        match self {
            Self::All(v) => v.iter().all(|r| r.matches(s)),
            Self::Any(v) => v.iter().any(|r| r.matches(s)),
            Self::Not(r) => !r.matches(s),
            Self::Category(names) => tx
                .category
                .as_deref()
                .is_some_and(|c| names.iter().any(|n| n.to_lowercase() == c.to_lowercase())),
            Self::CategoryKind(kinds) => kinds.contains(&tx.kind),
            Self::Tag(tags) => tags.iter().any(|t| tx.tags.contains(t)),
            Self::Text(texts) => {
                let hay = format!(
                    " {} {} {} ",
                    tx.merchant,
                    normalize_text(tx.counterparty.as_deref().unwrap_or("")),
                    normalize_text(tx.description.as_deref().unwrap_or(""))
                );
                texts.iter().any(|t| hay.contains(&normalize_text(t)))
            }
            Self::Account(names) => {
                let hay = squash(&tx.account);
                names.iter().any(|n| hay.contains(&squash(n)))
            }
            Self::Amount { min, max } => {
                let a = tx.cents.abs();
                min.is_none_or(|m| a >= (m * 100.0).round() as i64)
                    && max.is_none_or(|m| a <= (m * 100.0).round() as i64)
            }
            Self::Direction(Direction::Out) => tx.cents < 0,
            Self::Direction(Direction::In) => tx.cents > 0,
            Self::Weekday(days) => days.contains(&tx.date.weekday().number_from_monday()),
            Self::DayOfMonth { from, to } => (*from..=*to).contains(&tx.date.day()),
            Self::Uncategorized(b) => tx.category.is_none() == *b,
            Self::Fixed(b) => s.fixed == *b,
        }
    }

    fn walk(&self, f: &mut impl FnMut(&Rule)) {
        f(self);
        match self {
            Self::All(v) | Self::Any(v) => v.iter().for_each(|r| r.walk(f)),
            Self::Not(r) => r.walk(f),
            _ => {}
        }
    }

    fn walk_mut(&mut self, f: &mut impl FnMut(&mut Rule)) {
        f(self);
        match self {
            Self::All(v) | Self::Any(v) => v.iter_mut().for_each(|r| r.walk_mut(f)),
            Self::Not(r) => r.walk_mut(f),
            _ => {}
        }
    }

    /// Readable description in the current language, e.g. "categoria Ristoranti e bar, senza tag #lavoro".
    pub fn describe(&self) -> String {
        let or = tr!(" o ", " or ");
        let list = |v: &[String], quote: bool| -> String {
            let items: Vec<String> = v
                .iter()
                .map(|s| {
                    if !quote {
                        s.clone()
                    } else {
                        tr!("«{s}»", "“{s}”")
                    }
                })
                .collect();
            items.join(&or)
        };
        let tags = |v: &[String]| {
            v.iter()
                .map(|t| format!("#{t}"))
                .collect::<Vec<_>>()
                .join(&or)
        };
        fn nested(r: &Rule) -> String {
            match r {
                Rule::All(v) | Rule::Any(v) if v.len() > 1 => format!("({})", r.describe()),
                _ => r.describe(),
            }
        }
        match self {
            Self::All(v) => v.iter().map(nested).collect::<Vec<_>>().join(", "),
            Self::Any(v) => v
                .iter()
                .map(nested)
                .collect::<Vec<_>>()
                .join(&tr!(" oppure ", " or ")),
            Self::Not(r) => match r.as_ref() {
                Rule::Category(v) => {
                    let l = list(v, false);
                    tr!("esclusa categoria {l}", "excluding category {l}")
                }
                Rule::Tag(v) => {
                    let l = tags(v);
                    tr!("senza tag {l}", "without tag {l}")
                }
                Rule::Text(v) => {
                    let l = list(v, true);
                    tr!("escluso testo {l}", "excluding text {l}")
                }
                other => {
                    let d = other.describe();
                    tr!("non ({d})", "not ({d})")
                }
            },
            Self::Category(v) => {
                let l = list(v, false);
                tr!("categoria {l}", "category {l}")
            }
            Self::CategoryKind(v) => {
                let l = v
                    .iter()
                    .map(|k| match k {
                        Kind::Expense => tr!("spesa", "expense"),
                        Kind::Income => tr!("entrata", "income"),
                        Kind::Transfer => tr!("giroconto", "transfer"),
                        Kind::Saving => tr!("risparmio", "saving"),
                    })
                    .collect::<Vec<_>>()
                    .join(&or);
                tr!("tipo {l}", "type {l}")
            }
            Self::Tag(v) => format!("tag {}", tags(v)),
            Self::Text(v) => {
                let l = list(v, true);
                tr!("testo {l}", "text {l}")
            }
            Self::Account(v) => {
                let l = list(v, true);
                tr!("conto {l}", "account {l}")
            }
            Self::Amount { min, max } => match (min, max) {
                (Some(a), Some(b)) => {
                    let (a, b) = (fmt_eur(*a), fmt_eur(*b));
                    tr!("importo tra {a} e {b}", "amount between {a} and {b}")
                }
                (Some(a), None) => {
                    let a = fmt_eur(*a);
                    tr!("importo da {a}", "amount from {a}")
                }
                (None, Some(b)) => {
                    let b = fmt_eur(*b);
                    tr!("importo fino a {b}", "amount up to {b}")
                }
                (None, None) => tr!("qualsiasi importo", "any amount"),
            },
            Self::Direction(Direction::Out) => tr!("solo uscite", "money out only"),
            Self::Direction(Direction::In) => {
                tr!("solo entrate e rimborsi", "money in and refunds only")
            }
            Self::Weekday(v) => {
                let d: [&str; 7] = if crate::i18n::en() {
                    ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                } else {
                    ["lun", "mar", "mer", "gio", "ven", "sab", "dom"]
                };
                let l = v
                    .iter()
                    .map(|x| d[(*x as usize).clamp(1, 7) - 1])
                    .collect::<Vec<_>>()
                    .join(", ");
                tr!("di {l}", "on {l}")
            }
            Self::DayOfMonth { from, to } => tr!(
                "dal giorno {from} al {to} del mese",
                "from day {from} to {to} of the month"
            ),
            Self::Uncategorized(true) => tr!("da categorizzare", "uncategorized"),
            Self::Uncategorized(false) => tr!("già categorizzati", "already categorized"),
            Self::Fixed(true) => tr!(
                "solo spese fisse pianificate",
                "planned fixed expenses only"
            ),
            Self::Fixed(false) => tr!(
                "escluse le spese fisse pianificate",
                "excluding planned fixed expenses"
            ),
        }
    }
}

fn check_list<T>(v: &[T], what: &str) -> Result<()> {
    if v.is_empty() {
        bail!("condizione {what} vuota: indica almeno un valore");
    }
    Ok(())
}

/// Validate a rule in place: canonical category names, normalized tags, bounds.
fn validate_rule(conn: &Connection, rule: &mut Rule) -> Result<()> {
    fn depth(r: &Rule) -> usize {
        match r {
            Rule::All(v) | Rule::Any(v) => 1 + v.iter().map(depth).max().unwrap_or(0),
            Rule::Not(r) => 1 + depth(r),
            _ => 1,
        }
    }
    if depth(rule) > MAX_DEPTH {
        bail!("regola troppo annidata (massimo {MAX_DEPTH} livelli)");
    }
    let mut count = 0;
    rule.walk(&mut |_| count += 1);
    if count > MAX_CONDITIONS {
        bail!("regola troppo lunga (massimo {MAX_CONDITIONS} condizioni)");
    }
    let accounts: Vec<String> = conn
        .prepare(
            "SELECT lower(COALESCE(label, '') || ' ' || COALESCE(name, '') || ' ' || COALESCE(aspsp, '')
                    || ' ' || replace(COALESCE(iban, ''), ' ', '')) FROM accounts",
        )?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    let mut err: Option<anyhow::Error> = None;
    rule.walk_mut(&mut |r| {
        if err.is_some() {
            return;
        }
        let res: Result<()> = (|| {
            match r {
                Rule::All(v) | Rule::Any(v) => check_list(v, "all/any")?,
                Rule::Category(v) => {
                    check_list(v, "category")?;
                    for name in v.iter_mut() {
                        let id = crate::db::category_id(conn, name.trim())?;
                        *name = conn.query_row(
                            "SELECT name FROM categories WHERE id = ?1",
                            [id],
                            |r| r.get(0),
                        )?;
                    }
                    v.dedup();
                }
                Rule::CategoryKind(v) => check_list(v, "category_kind")?,
                Rule::Tag(v) => {
                    check_list(v, "tag")?;
                    for t in v.iter_mut() {
                        *t = crate::tags::normalize(t)?;
                    }
                }
                Rule::Text(v) => {
                    check_list(v, "text")?;
                    for t in v.iter_mut() {
                        *t = t.trim().to_string();
                        if normalize_text(t).trim().chars().count() < 2 {
                            bail!("testo troppo corto nella regola: {t:?}");
                        }
                    }
                }
                Rule::Account(v) => {
                    check_list(v, "account")?;
                    for a in v.iter_mut() {
                        *a = a.trim().to_string();
                        let q = squash(a);
                        if q.is_empty() || !accounts.iter().any(|h| squash(h).contains(&q)) {
                            bail!("nessun conto corrisponde a {a:?}: usa il nome, la banca o l'IBAN di un conto collegato");
                        }
                    }
                }
                Rule::Amount { min, max } => {
                    if min.is_none() && max.is_none() {
                        bail!("condizione amount senza min né max");
                    }
                    for v in [*min, *max].into_iter().flatten() {
                        if !v.is_finite() || v < 0.0 {
                            bail!("importo non valido nella regola: {v}");
                        }
                    }
                    if let (Some(a), Some(b)) = (min, max)
                        && a > b
                    {
                        bail!("nella regola amount, min ({a}) è maggiore di max ({b})");
                    }
                }
                Rule::Weekday(v) => {
                    check_list(v, "weekday")?;
                    if v.iter().any(|d| !(1..=7).contains(d)) {
                        bail!("giorno della settimana non valido: usa 1 (lunedì) ... 7 (domenica)");
                    }
                }
                Rule::DayOfMonth { from, to } => {
                    if !(1..=31).contains(from) || !(1..=31).contains(to) || from > to {
                        bail!("giorni del mese non validi: from e to tra 1 e 31, from ≤ to");
                    }
                }
                Rule::Not(_) | Rule::Direction(_) | Rule::Uncategorized(_) | Rule::Fixed(_) => {}
            }
            Ok(())
        })();
        if let Err(e) = res {
            err = Some(e);
        }
    });
    match err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Validate and normalize a budget definition.
pub fn validate(conn: &Connection, b: &mut Budget) -> Result<()> {
    b.name = b.name.trim().to_string();
    if b.name.is_empty() || b.name.chars().count() > 60 {
        bail!(tr!(
            "il nome del budget deve avere da 1 a 60 caratteri",
            "the budget name must be 1 to 60 characters long"
        ));
    }
    if !b.amount.is_finite() || b.amount <= 0.0 {
        bail!(tr!(
            "l'importo del budget deve essere maggiore di zero",
            "the budget amount must be greater than zero"
        ));
    }
    eur_to_cents(b.amount)?;
    match &mut b.period {
        Period::Week { start_weekday } => {
            if start_weekday.is_some_and(|d| !(1..=7).contains(&d)) {
                bail!("start_weekday va da 1 (lunedì) a 7 (domenica)");
            }
        }
        Period::Month { start_day } => {
            if start_day.is_some_and(|d| !(1..=28).contains(&d)) {
                bail!("start_day va da 1 a 28 (per i mesi corti)");
            }
        }
        Period::Quarter => {}
        Period::Year { start_month } => {
            if start_month.is_some_and(|m| !(1..=12).contains(&m)) {
                bail!("start_month va da 1 a 12");
            }
        }
        Period::Rolling { days } => {
            if !(1..=366).contains(days) {
                bail!("rolling: days va da 1 a 366");
            }
        }
        Period::Custom { from, to } => {
            let (f, t) = (parse_date(from)?, parse_date(to)?);
            if f > t {
                bail!("il periodo finisce prima di iniziare ({from} → {to})");
            }
            if (t - f).num_days() > 3 * 366 {
                bail!("periodo troppo lungo (massimo 3 anni)");
            }
            *from = f.to_string();
            *to = t.to_string();
        }
    }
    if b.rollover != Rollover::None {
        if b.goal != Goal::MaxSpend {
            bail!("il riporto vale solo per i budget di spesa (goal max_spend)");
        }
        if !b.period.recurring() {
            bail!("il riporto vale solo per periodi che si ripetono (week, month, quarter, year)");
        }
    }
    if let Some(cap) = b.rollover_cap
        && (!cap.is_finite() || cap <= 0.0)
    {
        bail!("rollover_cap deve essere maggiore di zero");
    }
    if b.reserve && b.goal != Goal::MaxSpend {
        bail!("reserve vale solo per i budget di spesa (goal max_spend)");
    }
    if b.alerts.iter().any(|a| !(1..=500).contains(a)) {
        bail!("le soglie di avviso sono percentuali tra 1 e 500");
    }
    b.alerts.sort_unstable();
    b.alerts.dedup();
    if let Some(rule) = &mut b.rule {
        validate_rule(conn, rule)?;
    }
    if let Some(n) = &mut b.note {
        *n = n.trim().to_string();
        if n.chars().count() > 500 {
            bail!("nota troppo lunga (massimo 500 caratteri)");
        }
        if n.is_empty() {
            b.note = None;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Storage

struct Stored {
    id: i64,
    budget: Budget,
    created: NaiveDate,
}

fn load_all(conn: &Connection) -> Result<Vec<Stored>> {
    let mut st = conn.prepare(
        "SELECT id, name, definition, substr(created_at, 1, 10) FROM budgets ORDER BY name",
    )?;
    let rows = st.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, def, created) = row?;
        let mut budget: Budget = serde_json::from_str(&def)
            .map_err(|e| anyhow::anyhow!("budget {name:?} illeggibile nel database: {e}"))?;
        budget.name = name;
        out.push(Stored {
            id,
            budget,
            created: parse_date(&created).unwrap_or_default(),
        });
    }
    Ok(out)
}

/// Create or replace a budget by name. `replaces` renames an existing budget.
pub fn upsert(conn: &Connection, mut b: Budget, replaces: Option<&str>) -> Result<Budget> {
    validate(conn, &mut b)?;
    let def = serde_json::to_string(&b)?;
    let old = replaces.map(str::trim).filter(|r| !r.is_empty());
    if let Some(old) = old {
        let id: Option<i64> = conn
            .query_row("SELECT id FROM budgets WHERE name = ?1", [old], |r| {
                r.get(0)
            })
            .optional()?;
        let Some(id) = id else {
            bail!("budget {old:?} non trovato");
        };
        let clash: Option<i64> = conn
            .query_row(
                "SELECT id FROM budgets WHERE name = ?1 AND id <> ?2",
                params![b.name, id],
                |r| r.get(0),
            )
            .optional()?;
        if clash.is_some() {
            bail!(tr!(
                "esiste già un budget {:?}",
                "a budget {:?} already exists",
                b.name
            ));
        }
        conn.execute(
            "UPDATE budgets SET name = ?1, definition = ?2 WHERE id = ?3",
            params![b.name, def, id],
        )?;
    } else {
        conn.execute(
            "INSERT INTO budgets (name, definition) VALUES (?1, ?2)
             ON CONFLICT(name) DO UPDATE SET name = excluded.name, definition = excluded.definition",
            params![b.name, def],
        )?;
    }
    Ok(b)
}

pub fn delete(conn: &Connection, name: &str) -> Result<()> {
    if conn.execute("DELETE FROM budgets WHERE name = ?1", [name.trim()])? == 0 {
        bail!("budget {name:?} non trovato");
    }
    Ok(())
}

/// Keep rules in sync when a category is renamed or merged into another (`new`).
pub fn rename_category(conn: &Connection, old: &str, new: &str) -> Result<()> {
    for s in load_all(conn)? {
        let mut b = s.budget.clone();
        if let Some(rule) = &mut b.rule {
            rule.walk_mut(&mut |r| {
                if let Rule::Category(v) = r {
                    for n in v.iter_mut() {
                        if n.to_lowercase() == old.to_lowercase() {
                            *n = new.to_string();
                        }
                    }
                    let mut seen = HashSet::new();
                    v.retain(|n| seen.insert(n.to_lowercase()));
                }
            });
        }
        if b != s.budget {
            conn.execute(
                "UPDATE budgets SET definition = ?1 WHERE id = ?2",
                params![serde_json::to_string(&b)?, s.id],
            )?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Evaluation

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// The period has not started yet (fixed dates in the future)
    Upcoming,
    Active,
    /// Fixed dates in the past
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    // max_spend
    Ok,
    Attenzione,
    Sforato,
    // min_save / min_income
    InLinea,
    Indietro,
    Raggiunto,
    Mancato,
    /// Paused
    Sospeso,
}

#[derive(Debug, Clone, Serialize)]
pub struct PeriodValue {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub limit: f64,
    pub value: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CountedTx {
    pub id: i64,
    pub date: NaiveDate,
    /// What it adds to the budget (spent, saved or received; negative for refunds/withdrawals)
    pub amount: f64,
    pub merchant: String,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BudgetStatus {
    pub id: i64,
    pub name: String,
    pub goal: Goal,
    pub period_label: String,
    /// Readable description of the rule
    pub rule_summary: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub phase: Phase,
    /// Base amount per period
    pub amount: f64,
    /// Carried over from previous periods (rollover)
    pub carried: f64,
    /// Effective limit or target for this period (amount + carried)
    pub limit: f64,
    /// Spent (max_spend), saved (min_save) or received (min_income) so far
    pub value: f64,
    /// What is left to spend, or still missing to the target
    pub remaining: f64,
    /// value / limit
    pub used: f64,
    /// Fraction of the period elapsed
    pub elapsed: f64,
    /// Linear projection at the end of the period
    pub projected: Option<f64>,
    /// Daily allowance until the end of the period (max_spend)
    pub per_day: Option<f64>,
    pub state: State,
    /// Alert thresholds (percent) already reached
    pub thresholds_reached: Vec<u32>,
    /// Amount taken out of this month's "available" (reserve)
    pub reserved: f64,
    /// Previous periods, oldest first (recurring budgets)
    pub history: Vec<PeriodValue>,
    /// Things to fix, e.g. a category in the rule that no longer exists
    pub problems: Vec<String>,
    /// The full definition, for editing
    pub budget: Budget,
    /// Transactions that count in the current period (detail and preview only)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transactions: Option<Vec<CountedTx>>,
}

struct Ctx {
    txs: Vec<Tx>,
    planned: Vec<Planned>,
    categories: HashSet<String>,
    today: NaiveDate,
}

impl Ctx {
    fn load(conn: &Connection, from: NaiveDate, today: NaiveDate) -> Result<Self> {
        let categories = conn
            .prepare("SELECT lower(name) FROM categories")?
            .query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(Self {
            txs: analysis::load(conn, from, today)?,
            planned: analysis::load_planned(conn)?,
            categories,
            today,
        })
    }

    /// Transactions counting towards `b` in [from, to], with their value in cents.
    fn counted(&self, b: &Budget, from: NaiveDate, to: NaiveDate) -> Vec<(&Tx, NaiveDate, i64)> {
        let ok = |tx: &Tx, fixed: bool| {
            b.rule
                .as_ref()
                .is_none_or(|r| r.matches(&Subject { tx, fixed }))
        };
        let in_range = |d: NaiveDate| d >= from && d <= to;
        match (b.goal, b.basis) {
            (Goal::MaxSpend, Basis::Amortized) => {
                let by_id: HashMap<i64, &Tx> = self.txs.iter().map(|t| (t.id, t)).collect();
                analysis::expense_entries(&self.txs, &self.planned, from, to)
                    .into_iter()
                    .filter_map(|e| {
                        let tx = by_id[&e.tx_id];
                        ok(tx, e.fixed).then_some((tx, e.date, e.spent))
                    })
                    .collect()
            }
            (Goal::MaxSpend, Basis::Cash) => self
                .txs
                .iter()
                .filter(|t| t.kind == Kind::Expense && in_range(t.date))
                .filter(|t| ok(t, analysis::is_fixed(t, &self.planned)))
                .map(|t| (t, t.date, -t.cents))
                .collect(),
            (Goal::MinSave, _) => self
                .txs
                .iter()
                .filter(|t| t.kind == Kind::Saving && in_range(t.date) && ok(t, false))
                .map(|t| (t, t.date, -t.cents))
                .collect(),
            (Goal::MinIncome, _) => self
                .txs
                .iter()
                .filter(|t| t.kind == Kind::Income && in_range(t.date) && ok(t, false))
                .map(|t| (t, t.date, t.cents))
                .collect(),
        }
    }

    fn total(&self, b: &Budget, from: NaiveDate, to: NaiveDate) -> i64 {
        self.counted(b, from, to).iter().map(|c| c.2).sum()
    }

    fn evaluate(
        &self,
        id: i64,
        b: &Budget,
        created: NaiveDate,
        with_txs: bool,
    ) -> Result<BudgetStatus> {
        let today = self.today;
        let (start, end) = b.period.range(today)?;
        let phase = if today < start {
            Phase::Upcoming
        } else if today > end {
            Phase::Ended
        } else {
            Phase::Active
        };
        let amount = eur_to_cents(b.amount)?;

        // Previous periods (oldest first) for history and rollover.
        let mut previous = Vec::new();
        if b.period.recurring() {
            let mut d = start;
            for _ in 0..LOOKBACK {
                let (ps, pe) = b.period.range(d - Days::new(1))?;
                previous.push((ps, pe, self.total(b, ps, pe)));
                d = ps;
            }
            previous.reverse();
        }
        let first_counted = b.period.range(created)?.0;
        let mut carry = 0i64;
        let cap = b.rollover_cap.map(eur_to_cents).transpose()?;
        let mut history = Vec::new();
        for &(ps, pe, value) in &previous {
            let limit = amount + carry;
            if b.rollover != Rollover::None && ps >= first_counted {
                let diff = limit - value;
                carry = match b.rollover {
                    Rollover::Surplus => diff.max(0),
                    Rollover::Deficit => diff.min(0),
                    Rollover::Both => diff,
                    Rollover::None => 0,
                };
                if let Some(c) = cap {
                    carry = carry.clamp(-c, c);
                }
            }
            history.push(PeriodValue {
                start: ps,
                end: pe,
                limit: eur(limit),
                value: eur(value),
            });
        }
        let history = history.split_off(history.len().saturating_sub(HISTORY));
        let limit = amount + carry;

        let upto = today.min(end);
        let counted = if phase == Phase::Upcoming {
            vec![]
        } else {
            self.counted(b, start, upto)
        };
        let value: i64 = counted.iter().map(|c| c.2).sum();
        let total_days = (end - start).num_days() + 1;
        let elapsed_days = match phase {
            Phase::Upcoming => 0,
            Phase::Ended => total_days,
            Phase::Active => (today - start).num_days() + 1,
        };
        let rolling = matches!(b.period, Period::Rolling { .. });
        let elapsed = if rolling {
            1.0
        } else {
            elapsed_days as f64 / total_days as f64
        };
        let used = value as f64 / limit.max(1) as f64;
        let days_left = (end - today).num_days() + 1;
        let projected = (phase == Phase::Active && !rolling && elapsed_days > 0)
            .then(|| eur(value * total_days / elapsed_days));
        let thresholds_reached: Vec<u32> = b
            .alerts
            .iter()
            .copied()
            .filter(|t| used * 100.0 >= *t as f64)
            .collect();
        let pace_ahead = b.pace_alert
            && phase == Phase::Active
            && !rolling
            && used > elapsed + 0.15
            && used > 0.3;

        let (remaining, per_day, state) = match b.goal {
            Goal::MaxSpend => {
                let remaining = limit - value;
                let per_day = (phase != Phase::Ended && !rolling)
                    .then(|| eur(remaining.max(0) / days_left.min(total_days).max(1)));
                let state = if value > limit {
                    State::Sforato
                } else if pace_ahead || thresholds_reached.iter().any(|t| *t < 100) {
                    State::Attenzione
                } else {
                    State::Ok
                };
                (remaining, per_day, state)
            }
            Goal::MinSave | Goal::MinIncome => {
                let state = if value >= limit {
                    State::Raggiunto
                } else if phase == Phase::Ended {
                    State::Mancato
                } else if b.pace_alert && !rolling && elapsed > 0.3 && used < elapsed - 0.15 {
                    State::Indietro
                } else {
                    State::InLinea
                };
                ((limit - value).max(0), None, state)
            }
        };
        let state = if b.enabled { state } else { State::Sospeso };

        // Reserve: the unspent part falling in the rest of this month is already committed.
        let reserved = if b.enabled
            && b.reserve
            && b.goal == Goal::MaxSpend
            && phase != Phase::Ended
            && remaining > 0
        {
            let from = today.max(start);
            let month_end = analysis::month_end(today);
            if from > month_end {
                0
            } else {
                let in_month = (end.min(month_end) - from).num_days() + 1;
                let left = (end - from).num_days() + 1;
                remaining * in_month / left.max(1)
            }
        } else {
            0
        };

        let mut problems = Vec::new();
        if let Some(rule) = &b.rule {
            rule.walk(&mut |r| {
                if let Rule::Category(v) = r {
                    for n in v {
                        if !self.categories.contains(&n.to_lowercase()) {
                            problems.push(tr!(
                                "la categoria «{n}» non esiste più: modifica la regola",
                                "the category “{n}” no longer exists: edit the rule"
                            ));
                        }
                    }
                }
            });
        }

        let transactions = with_txs.then(|| {
            let mut v: Vec<CountedTx> = counted
                .iter()
                .map(|(tx, date, cents)| CountedTx {
                    id: tx.id,
                    date: *date,
                    amount: eur(*cents),
                    merchant: tx.merchant.clone(),
                    category: tx.category.clone(),
                })
                .collect();
            v.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
            v
        });

        Ok(BudgetStatus {
            id,
            name: b.name.clone(),
            goal: b.goal,
            period_label: b.period.label(),
            rule_summary: rule_summary(b),
            start,
            end,
            phase,
            amount: b.amount,
            carried: eur(carry),
            limit: eur(limit),
            value: eur(value),
            remaining: eur(remaining),
            used: (used * 1000.0).round() / 1000.0,
            elapsed: (elapsed * 1000.0).round() / 1000.0,
            projected,
            per_day,
            state,
            thresholds_reached,
            reserved: eur(reserved),
            history,
            problems,
            budget: b.clone(),
            transactions,
        })
    }
}

pub fn rule_summary(b: &Budget) -> String {
    match &b.rule {
        Some(r) => r.describe(),
        None => match b.goal {
            Goal::MaxSpend => tr!("tutte le spese", "all expenses"),
            Goal::MinSave => tr!(
                "tutti i movimenti verso il risparmio",
                "all transfers to savings"
            ),
            Goal::MinIncome => tr!("tutte le entrate", "all income"),
        },
    }
}

/// Earliest date needed to evaluate these budgets (current period plus lookback).
fn earliest(budgets: &[&Budget], today: NaiveDate) -> Result<NaiveDate> {
    let mut from = today;
    for b in budgets {
        let (mut start, _) = b.period.range(today)?;
        if b.period.recurring() {
            for _ in 0..LOOKBACK {
                start = b.period.range(start - Days::new(1))?.0;
            }
        }
        from = from.min(start);
    }
    Ok(from)
}

/// Status of every budget.
pub fn statuses(conn: &Connection, today: NaiveDate) -> Result<Vec<BudgetStatus>> {
    let all = load_all(conn)?;
    if all.is_empty() {
        return Ok(vec![]);
    }
    let refs: Vec<&Budget> = all.iter().map(|s| &s.budget).collect();
    let ctx = Ctx::load(conn, earliest(&refs, today)?, today)?;
    all.iter()
        .map(|s| ctx.evaluate(s.id, &s.budget, s.created, false))
        .collect()
}

/// One budget with the transactions that count in its current period.
pub fn status_of(conn: &Connection, name: &str, today: NaiveDate) -> Result<BudgetStatus> {
    let Some(s) = load_all(conn)?
        .into_iter()
        .find(|s| s.budget.name.to_lowercase() == name.trim().to_lowercase())
    else {
        bail!("budget {name:?} non trovato");
    };
    let ctx = Ctx::load(conn, earliest(&[&s.budget], today)?, today)?;
    ctx.evaluate(s.id, &s.budget, s.created, true)
}

/// Evaluate a definition without saving it (to check a rule before creating the budget).
pub fn preview(conn: &Connection, mut b: Budget, today: NaiveDate) -> Result<BudgetStatus> {
    validate(conn, &mut b)?;
    let ctx = Ctx::load(conn, earliest(&[&b], today)?, today)?;
    ctx.evaluate(0, &b, today, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::tests::{add_tx, d};
    use crate::db;

    fn budget(name: &str, rule: Option<Rule>, period: Period, amount: f64) -> Budget {
        serde_json::from_value(serde_json::json!({
            "name": name, "period": period, "amount": amount, "rule": rule
        }))
        .unwrap()
    }

    fn month() -> Period {
        Period::Month { start_day: None }
    }

    fn tag(conn: &mut Connection, tx: i64, t: &str) {
        crate::tags::tag(conn, &[tx], &[t.to_string()], false, None).unwrap();
    }

    #[test]
    fn periods() {
        let t = d("2026-11-12"); // Thursday
        assert_eq!(
            Period::Week {
                start_weekday: None
            }
            .range(t)
            .unwrap(),
            (d("2026-11-09"), d("2026-11-15"))
        );
        assert_eq!(
            Period::Week {
                start_weekday: Some(6)
            }
            .range(t)
            .unwrap(),
            (d("2026-11-07"), d("2026-11-13"))
        );
        assert_eq!(
            Period::Month {
                start_day: Some(27)
            }
            .range(t)
            .unwrap(),
            (d("2026-10-27"), d("2026-11-26"))
        );
        assert_eq!(
            Period::Month {
                start_day: Some(27)
            }
            .range(d("2026-11-27"))
            .unwrap(),
            (d("2026-11-27"), d("2026-12-26"))
        );
        assert_eq!(
            Period::Quarter.range(t).unwrap(),
            (d("2026-10-01"), d("2026-12-31"))
        );
        assert_eq!(
            Period::Year {
                start_month: Some(4)
            }
            .range(d("2026-02-01"))
            .unwrap(),
            (d("2025-04-01"), d("2026-03-31"))
        );
        assert_eq!(
            Period::Rolling { days: 30 }.range(t).unwrap(),
            (d("2026-10-14"), t)
        );
    }

    #[test]
    fn nested_rules_with_tags_text_and_amounts() {
        let mut conn = db::open_in_memory().unwrap();
        let a = add_tx(
            &conn,
            "2026-11-02",
            -3000,
            "trattoria",
            Some("Ristoranti e bar"),
        );
        let b = add_tx(
            &conn,
            "2026-11-03",
            -5000,
            "pizzeria",
            Some("Ristoranti e bar"),
        );
        add_tx(&conn, "2026-11-04", -2000, "cinema odeon", Some("Svago"));
        add_tx(
            &conn,
            "2026-11-05",
            -9000,
            "esselunga",
            Some("Spesa alimentare"),
        );
        tag(&mut conn, b, "lavoro");
        let _ = a;
        // Restaurants + leisure, without work lunches.
        let svago: Rule = serde_json::from_value(serde_json::json!({"all": [
            {"category": ["ristoranti e bar", "Svago"]},
            {"not": {"tag": ["#Lavoro"]}}
        ]}))
        .unwrap();
        let s = preview(
            &conn,
            budget("Svago", Some(svago), month(), 100.0),
            d("2026-11-10"),
        )
        .unwrap();
        assert_eq!(s.value, 50.0);
        assert_eq!(s.transactions.as_ref().unwrap().len(), 2);
        // Names were canonicalized and tags normalized.
        assert!(s.rule_summary.contains("Ristoranti e bar"));
        assert!(s.rule_summary.contains("#lavoro"));

        let big: Rule = serde_json::from_value(serde_json::json!({"any": [
            {"text": ["Esselunga"]},
            {"amount": {"min": 40}}
        ]}))
        .unwrap();
        let s = preview(
            &conn,
            budget("Grandi", Some(big), month(), 500.0),
            d("2026-11-10"),
        )
        .unwrap();
        assert_eq!(s.value, 140.0);

        // Everything except groceries.
        let not_food: Rule =
            serde_json::from_value(serde_json::json!({"not": {"category": ["Spesa alimentare"]}}))
                .unwrap();
        let s = preview(
            &conn,
            budget("Variabili", Some(not_food), month(), 500.0),
            d("2026-11-10"),
        )
        .unwrap();
        assert_eq!(s.value, 100.0);

        // Weekday and day of month: 2026-11-02 is a Monday.
        let monday: Rule = serde_json::from_value(serde_json::json!({"weekday": [1]})).unwrap();
        let s = preview(
            &conn,
            budget("Lunedì", Some(monday), month(), 500.0),
            d("2026-11-10"),
        )
        .unwrap();
        assert_eq!(s.value, 30.0);
        let early: Rule =
            serde_json::from_value(serde_json::json!({"day_of_month": {"from": 4, "to": 5}}))
                .unwrap();
        let s = preview(
            &conn,
            budget("Inizio", Some(early), month(), 500.0),
            d("2026-11-10"),
        )
        .unwrap();
        assert_eq!(s.value, 110.0);
    }

    #[test]
    fn validation_errors_are_clear() {
        let conn = db::open_in_memory().unwrap();
        let bad = |rule: serde_json::Value, period: Period| {
            let r: Rule = serde_json::from_value(rule).unwrap();
            preview(&conn, budget("x", Some(r), period, 10.0), d("2026-11-10"))
                .unwrap_err()
                .to_string()
        };
        assert!(
            bad(serde_json::json!({"category": ["Inesistente"]}), month()).contains("Inesistente")
        );
        assert!(bad(serde_json::json!({"account": ["fineco"]}), month()).contains("conto"));
        assert!(
            bad(
                serde_json::json!({"amount": {"min": 50, "max": 10}}),
                month()
            )
            .contains("min")
        );
        assert!(bad(serde_json::json!({"all": []}), month()).contains("vuota"));
        assert!(bad(serde_json::json!({"text": ["x"]}), month()).contains("corto"));
        let custom = Period::Custom {
            from: "2026-08-07".into(),
            to: "2026-08-01".into(),
        };
        assert!(bad(serde_json::json!({"direction": "out"}), custom).contains("finisce prima"));
        let mut b = budget("Risparmio", None, Period::Rolling { days: 30 }, 10.0);
        b.goal = Goal::MinSave;
        b.rollover = Rollover::Surplus;
        assert!(
            upsert(&conn, b, None)
                .unwrap_err()
                .to_string()
                .contains("riporto")
        );
    }

    #[test]
    fn pace_thresholds_and_overspending() {
        let conn = db::open_in_memory().unwrap();
        let r: Rule =
            serde_json::from_value(serde_json::json!({"category": ["Ristoranti e bar"]})).unwrap();
        let mut b = budget("Ristoranti", Some(r), month(), 200.0);
        b.alerts = vec![100];
        upsert(&conn, b, None).unwrap();
        add_tx(
            &conn,
            "2026-11-02",
            -14000,
            "trattoria",
            Some("Ristoranti e bar"),
        );
        // 70% used at day 10/30 -> pace warning
        let s = &statuses(&conn, d("2026-11-10")).unwrap()[0];
        assert_eq!(s.state, State::Attenzione);
        assert_eq!(s.per_day, Some(2.85)); // 60 € left over 21 days
        assert_eq!(s.projected, Some(420.0));
        add_tx(
            &conn,
            "2026-11-09",
            -7000,
            "pizzeria",
            Some("Ristoranti e bar"),
        );
        let s = &statuses(&conn, d("2026-11-10")).unwrap()[0];
        assert_eq!(s.state, State::Sforato);
        assert_eq!(s.thresholds_reached, vec![100]);
    }

    #[test]
    fn rollover_with_cap_starts_when_the_budget_was_created() {
        let conn = db::open_in_memory().unwrap();
        let r: Rule =
            serde_json::from_value(serde_json::json!({"category": ["Ristoranti e bar"]})).unwrap();
        let mut b = budget("Ristoranti", Some(r), month(), 100.0);
        b.rollover = Rollover::Both;
        b.rollover_cap = Some(50.0);
        upsert(&conn, b, None).unwrap();
        conn.execute("UPDATE budgets SET created_at = '2026-08-15 10:00:00'", [])
            .unwrap();
        add_tx(&conn, "2026-07-10", -1000, "bar", Some("Ristoranti e bar")); // before: ignored
        add_tx(&conn, "2026-08-10", -4000, "bar", Some("Ristoranti e bar")); // +60, capped at 50
        add_tx(&conn, "2026-09-10", -17000, "bar", Some("Ristoranti e bar")); // limit 150 -> -20
        add_tx(&conn, "2026-10-10", -5000, "bar", Some("Ristoranti e bar")); // limit 80 -> +30
        let s = &statuses(&conn, d("2026-11-05")).unwrap()[0];
        assert_eq!(s.carried, 30.0);
        assert_eq!(s.limit, 130.0);
        assert_eq!(s.history.len(), HISTORY);
        assert_eq!(s.history.last().unwrap().limit, 80.0);
    }

    #[test]
    fn cash_basis_counts_amortized_purchases_in_full() {
        let conn = db::open_in_memory().unwrap();
        let id = add_tx(
            &conn,
            "2026-11-03",
            -120000,
            "apple store",
            Some("Elettronica"),
        );
        conn.execute(
            "INSERT INTO amortizations (transaction_id, months) VALUES (?1, 12)",
            [id],
        )
        .unwrap();
        let r: Rule =
            serde_json::from_value(serde_json::json!({"category": ["Elettronica"]})).unwrap();
        let mut b = budget("Tech", Some(r), month(), 2000.0);
        assert_eq!(
            preview(&conn, b.clone(), d("2026-11-10")).unwrap().value,
            100.0
        );
        b.basis = Basis::Cash;
        assert_eq!(preview(&conn, b, d("2026-11-10")).unwrap().value, 1200.0);
    }

    #[test]
    fn savings_and_trip_budgets() {
        let mut conn = db::open_in_memory().unwrap();
        add_tx(
            &conn,
            "2026-11-01",
            -30000,
            "giroconto deposito",
            Some("Risparmi e investimenti"),
        );
        add_tx(
            &conn,
            "2026-11-08",
            5000,
            "prelievo deposito",
            Some("Risparmi e investimenti"),
        );
        let mut save = budget("Metti da parte", None, month(), 200.0);
        save.goal = Goal::MinSave;
        let s = preview(&conn, save, d("2026-11-10")).unwrap();
        assert_eq!(s.value, 250.0);
        assert_eq!(s.state, State::Raggiunto);

        let trip: Rule =
            serde_json::from_value(serde_json::json!({"tag": ["vacanza-londra"]})).unwrap();
        let p = Period::Custom {
            from: "2026-08-01".into(),
            to: "2026-08-07".into(),
        };
        let t = add_tx(&conn, "2026-08-03", -30000, "hotel", Some("Viaggi"));
        tag(&mut conn, t, "vacanza-londra");
        add_tx(&conn, "2026-08-04", -2000, "pub", Some("Ristoranti e bar"));
        let s = preview(
            &conn,
            budget("Londra", Some(trip.clone()), p.clone(), 800.0),
            d("2026-11-10"),
        )
        .unwrap();
        assert_eq!(s.phase, Phase::Ended);
        assert_eq!(s.value, 300.0);
        assert_eq!(s.per_day, None);
        let s = preview(
            &conn,
            budget("Londra", Some(trip), p, 800.0),
            d("2026-07-01"),
        )
        .unwrap();
        assert_eq!(s.phase, Phase::Upcoming);
        assert_eq!(s.value, 0.0);
    }

    #[test]
    fn reserve_takes_the_unspent_part_of_this_month() {
        let conn = db::open_in_memory().unwrap();
        let p = Period::Custom {
            from: "2026-11-21".into(),
            to: "2026-12-10".into(),
        };
        let mut b = budget("Viaggio", None, p, 200.0);
        b.reserve = true;
        // From 21/11 to 10/12: 20 days, 10 of them in November.
        let s = preview(&conn, b, d("2026-11-12")).unwrap();
        assert_eq!(s.reserved, 100.0);
    }

    #[test]
    fn category_rename_and_merge_update_rules() {
        let mut conn = db::open_in_memory().unwrap();
        let r: Rule =
            serde_json::from_value(serde_json::json!({"category": ["Ristoranti e bar", "Svago"]}))
                .unwrap();
        upsert(&conn, budget("Svago", Some(r), month(), 100.0), None).unwrap();
        crate::ops::update_category(
            &conn,
            "ristoranti e bar",
            &crate::ops::CategoryPatch {
                rename: Some("Locali".into()),
                ..Default::default()
            },
        )
        .unwrap();
        crate::ops::delete_category(&mut conn, "Svago", Some("Locali")).unwrap();
        let s = &statuses(&conn, d("2026-11-10")).unwrap()[0];
        assert_eq!(s.budget.rule, Some(Rule::Category(vec!["Locali".into()])));
        assert!(s.problems.is_empty());
        crate::ops::delete_category(&mut conn, "Locali", None).unwrap();
        assert_eq!(
            statuses(&conn, d("2026-11-10")).unwrap()[0].problems.len(),
            1
        );
    }

    #[test]
    fn upsert_renames_and_deletes() {
        let conn = db::open_in_memory().unwrap();
        upsert(&conn, budget("A", None, month(), 100.0), None).unwrap();
        upsert(&conn, budget("B", None, month(), 50.0), Some("A")).unwrap();
        upsert(&conn, budget("B", None, month(), 70.0), None).unwrap();
        let all = statuses(&conn, d("2026-11-10")).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].amount, 70.0);
        delete(&conn, "b").unwrap();
        assert!(statuses(&conn, d("2026-11-10")).unwrap().is_empty());
    }
}
