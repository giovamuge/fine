//! Budget rules: matching, validation and readable descriptions.

use chrono::Datelike;

use super::{Direction, Rule};
use crate::domain::category::{self, CategoryKind};
use crate::domain::money::Money;
use crate::domain::transaction::Tx;
use crate::domain::{tag, text};
use crate::error::Result;
use crate::i18n;
use crate::tr;

const MAX_DEPTH: usize = 6;
const MAX_CONDITIONS: usize = 50;

/// What validating a rule needs to know about the user's data.
#[derive(Debug, Default)]
pub struct ValidationContext {
    /// Existing category names (canonical spelling).
    pub categories: Vec<String>,
    /// Lowercase "label name bank iban" of every account.
    pub accounts: Vec<String>,
}

impl ValidationContext {
    /// The canonical spelling of a category (names are case-insensitive, like in the database).
    pub fn category(&self, name: &str) -> Result<&str> {
        self.categories
            .iter()
            .find(|c| c.eq_ignore_ascii_case(name))
            .map(String::as_str)
            .ok_or_else(|| category::not_found(name))
    }
}

/// What a rule sees of a transaction.
pub(super) struct Subject<'a> {
    pub tx: &'a Tx,
    /// The transaction pays a weekly/monthly planned expense.
    pub fixed: bool,
}

fn non_empty<T>(v: &[T], what: &str) -> Result<()> {
    if v.is_empty() {
        crate::invalid!("condizione {what} vuota: indica almeno un valore");
    }
    Ok(())
}

impl Rule {
    pub(super) fn matches(&self, s: &Subject) -> bool {
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
                let hay = tx.haystack();
                texts.iter().any(|t| hay.contains(&text::normalize(t)))
            }
            Self::Account(names) => {
                let hay = text::squash(&tx.account);
                names.iter().any(|n| hay.contains(&text::squash(n)))
            }
            Self::Amount { min, max } => {
                let a = tx.amount.abs();
                min.is_none_or(|m| a >= Money::from_eur_lossy(m))
                    && max.is_none_or(|m| a <= Money::from_eur_lossy(m))
            }
            Self::Direction(Direction::Out) => tx.amount.is_negative(),
            Self::Direction(Direction::In) => tx.amount.is_positive(),
            Self::Weekday(days) => days.contains(&tx.date.weekday().number_from_monday()),
            Self::DayOfMonth { from, to } => (*from..=*to).contains(&tx.date.day()),
            Self::Uncategorized(b) => tx.category.is_none() == *b,
            Self::Fixed(b) => s.fixed == *b,
        }
    }

    /// Every node, depth first (the rule itself first).
    pub fn walk(&self, f: &mut impl FnMut(&Rule)) {
        f(self);
        match self {
            Self::All(v) | Self::Any(v) => v.iter().for_each(|r| r.walk(f)),
            Self::Not(r) => r.walk(f),
            _ => {}
        }
    }

    /// Every node, depth first, stopping at the first error.
    fn try_walk_mut(&mut self, f: &mut impl FnMut(&mut Rule) -> Result<()>) -> Result<()> {
        f(self)?;
        match self {
            Self::All(v) | Self::Any(v) => v.iter_mut().try_for_each(|r| r.try_walk_mut(f)),
            Self::Not(r) => r.try_walk_mut(f),
            _ => Ok(()),
        }
    }

    fn depth(&self) -> usize {
        match self {
            Self::All(v) | Self::Any(v) => 1 + v.iter().map(Self::depth).max().unwrap_or(0),
            Self::Not(r) => 1 + r.depth(),
            _ => 1,
        }
    }

    /// Category names this rule refers to.
    pub fn categories(&self) -> Vec<&str> {
        fn collect<'a>(r: &'a Rule, out: &mut Vec<&'a str>) {
            match r {
                Rule::All(v) | Rule::Any(v) => v.iter().for_each(|r| collect(r, out)),
                Rule::Not(r) => collect(r, out),
                Rule::Category(v) => out.extend(v.iter().map(String::as_str)),
                _ => {}
            }
        }
        let mut out = vec![];
        collect(self, &mut out);
        out
    }

    /// Validate in place: canonical category names, normalized tags, bounds.
    pub(super) fn validate(&mut self, ctx: &ValidationContext) -> Result<()> {
        if self.depth() > MAX_DEPTH {
            crate::invalid!("regola troppo annidata (massimo {MAX_DEPTH} livelli)");
        }
        let mut count = 0;
        self.walk(&mut |_| count += 1);
        if count > MAX_CONDITIONS {
            crate::invalid!("regola troppo lunga (massimo {MAX_CONDITIONS} condizioni)");
        }
        self.try_walk_mut(&mut |r| r.validate_node(ctx))
    }

    fn validate_node(&mut self, ctx: &ValidationContext) -> Result<()> {
        match self {
            Self::All(v) | Self::Any(v) => non_empty(v, "all/any")?,
            Self::Category(v) => {
                non_empty(v, "category")?;
                for name in v.iter_mut() {
                    *name = ctx.category(name.trim())?.to_owned();
                }
                v.dedup();
            }
            Self::CategoryKind(v) => non_empty(v, "category_kind")?,
            Self::Tag(v) => {
                non_empty(v, "tag")?;
                for t in v.iter_mut() {
                    *t = tag::normalize(t)?;
                }
            }
            Self::Text(v) => {
                non_empty(v, "text")?;
                for t in v.iter_mut() {
                    *t = t.trim().to_owned();
                    if text::normalize(t).trim().chars().count() < 2 {
                        crate::invalid!("testo troppo corto nella regola: {t:?}");
                    }
                }
            }
            Self::Account(v) => {
                non_empty(v, "account")?;
                for a in v.iter_mut() {
                    *a = a.trim().to_owned();
                    let q = text::squash(a);
                    if q.is_empty() || !ctx.accounts.iter().any(|h| text::squash(h).contains(&q)) {
                        crate::invalid!(
                            "nessun conto corrisponde a {a:?}: usa il nome, la banca o l'IBAN di un conto collegato"
                        );
                    }
                }
            }
            Self::Amount { min, max } => {
                if min.is_none() && max.is_none() {
                    crate::invalid!("condizione amount senza min né max");
                }
                for v in [*min, *max].into_iter().flatten() {
                    if !v.is_finite() || v < 0.0 {
                        crate::invalid!("importo non valido nella regola: {v}");
                    }
                }
                if let (Some(a), Some(b)) = (min, max)
                    && a > b
                {
                    crate::invalid!("nella regola amount, min ({a}) è maggiore di max ({b})");
                }
            }
            Self::Weekday(v) => {
                non_empty(v, "weekday")?;
                if v.iter().any(|d| !(1..=7).contains(d)) {
                    crate::invalid!(
                        "giorno della settimana non valido: usa 1 (lunedì) ... 7 (domenica)"
                    );
                }
            }
            Self::DayOfMonth { from, to } => {
                if !(1..=31).contains(from) || !(1..=31).contains(to) || from > to {
                    crate::invalid!("giorni del mese non validi: from e to tra 1 e 31, from ≤ to");
                }
            }
            Self::Not(_) | Self::Direction(_) | Self::Uncategorized(_) | Self::Fixed(_) => {}
        }
        Ok(())
    }

    /// Follow a category rename or merge, keeping names unique.
    pub(super) fn rename_category(&mut self, old: &str, new: &str) {
        let _ = self.try_walk_mut(&mut |r| {
            if let Self::Category(v) = r {
                for n in v.iter_mut() {
                    if n.to_lowercase() == old.to_lowercase() {
                        *n = new.to_owned();
                    }
                }
                let mut seen = std::collections::HashSet::new();
                v.retain(|n| seen.insert(n.to_lowercase()));
            }
            Ok(())
        });
    }

    /// Readable description in the current language, e.g. "categoria Ristoranti e bar, senza tag #lavoro".
    pub fn describe(&self) -> String {
        let or = tr!(" o ", " or ");
        let list = |v: &[String], quote: bool| -> String {
            v.iter()
                .map(|s| {
                    if quote {
                        tr!("«{s}»", "“{s}”")
                    } else {
                        s.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(&or)
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
                Self::Category(v) => {
                    let l = list(v, false);
                    tr!("esclusa categoria {l}", "excluding category {l}")
                }
                Self::Tag(v) => {
                    let l = tags(v);
                    tr!("senza tag {l}", "without tag {l}")
                }
                Self::Text(v) => {
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
                        CategoryKind::Expense => tr!("spesa", "expense"),
                        CategoryKind::Income => tr!("entrata", "income"),
                        CategoryKind::Transfer => tr!("giroconto", "transfer"),
                        CategoryKind::Saving => tr!("risparmio", "saving"),
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
            Self::Amount { min, max } => {
                let eur = |x: &f64| Money::from_eur_lossy(*x);
                match (min, max) {
                    (Some(a), Some(b)) => {
                        let (a, b) = (eur(a), eur(b));
                        tr!("importo tra {a} e {b}", "amount between {a} and {b}")
                    }
                    (Some(a), None) => {
                        let a = eur(a);
                        tr!("importo da {a}", "amount from {a}")
                    }
                    (None, Some(b)) => {
                        let b = eur(b);
                        tr!("importo fino a {b}", "amount up to {b}")
                    }
                    (None, None) => tr!("qualsiasi importo", "any amount"),
                }
            }
            Self::Direction(Direction::Out) => tr!("solo uscite", "money out only"),
            Self::Direction(Direction::In) => {
                tr!("solo entrate e rimborsi", "money in and refunds only")
            }
            Self::Weekday(v) => {
                let names: [&str; 7] = if i18n::is_en() {
                    ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
                } else {
                    ["lun", "mar", "mer", "gio", "ven", "sab", "dom"]
                };
                let l = v
                    .iter()
                    .map(|x| names[(*x as usize).clamp(1, 7) - 1])
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
