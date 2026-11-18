//! Language of the text written for people (alerts, budget descriptions). Italian everywhere by
//! default (CLI, MCP, notifications); the web dashboard asks for English per request with the
//! `X-Fine-Lang` header, which `web.rs` turns into the language of the code serving that request.

use std::cell::Cell;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    It,
    En,
}

impl Lang {
    /// "en…" is English, anything else Italian.
    pub fn parse(s: &str) -> Self {
        if s.trim().to_ascii_lowercase().starts_with("en") {
            Lang::En
        } else {
            Lang::It
        }
    }
}

thread_local! {
    static THREAD_LANG: Cell<Lang> = const { Cell::new(Lang::It) };
}

tokio::task_local! {
    /// Set by the web server around each request (async code that may move between threads).
    pub static TASK_LANG: Lang;
}

/// The language of the current request (task), else of the current thread.
pub fn lang() -> Lang {
    TASK_LANG
        .try_with(|l| *l)
        .unwrap_or_else(|_| THREAD_LANG.with(Cell::get))
}

pub fn en() -> bool {
    lang() == Lang::En
}

/// For blocking work on a pooled thread: set the language at the start of every job.
pub fn set_thread(lang: Lang) {
    THREAD_LANG.with(|l| l.set(lang));
}

/// `tr!("Italian {x}", "English {x}", ...)`: `format!` with the text of the current language.
#[macro_export]
macro_rules! tr {
    ($it:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::en() {
            format!($en $(, $arg)*)
        } else {
            format!($it $(, $arg)*)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_language_of_the_thread() {
        let n = 3;
        assert_eq!(tr!("{n} giorni", "{n} days"), "3 giorni");
        set_thread(Lang::En);
        assert_eq!(tr!("{n} giorni", "{n} days"), "3 days");
        set_thread(Lang::It);
        assert_eq!(Lang::parse("en-GB"), Lang::En);
        assert_eq!(Lang::parse("it"), Lang::It);
    }
}
