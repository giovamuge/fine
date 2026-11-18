//! Language of the text written for people (alerts, budget descriptions). Italian everywhere by
//! default (CLI, MCP, notifications); the web dashboard asks for English per request with the
//! `X-Fine-Lang` header, which the web layer turns into the language of the code serving it.

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
            Self::En
        } else {
            Self::It
        }
    }

    /// The language of the current request (task), else of the current thread.
    pub fn current() -> Self {
        TASK_LANG
            .try_with(|l| *l)
            .unwrap_or_else(|_| THREAD_LANG.with(Cell::get))
    }

    /// For blocking work on a pooled thread: make this the thread's language.
    pub fn enter(self) {
        THREAD_LANG.with(|l| l.set(self));
    }

    /// Runs a future with this language (async code that may move between threads).
    pub async fn scope<F: Future>(self, f: F) -> F::Output {
        TASK_LANG.scope(self, f).await
    }
}

thread_local! {
    static THREAD_LANG: Cell<Lang> = const { Cell::new(Lang::It) };
}

tokio::task_local! {
    static TASK_LANG: Lang;
}

pub fn is_en() -> bool {
    Lang::current() == Lang::En
}

/// `tr!("Italian {x}", "English {x}", ...)`: `format!` with the text of the current language.
#[macro_export]
macro_rules! tr {
    ($it:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::is_en() {
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
        Lang::En.enter();
        assert_eq!(tr!("{n} giorni", "{n} days"), "3 days");
        Lang::It.enter();
        assert_eq!(Lang::parse("en-GB"), Lang::En);
        assert_eq!(Lang::parse("it"), Lang::It);
    }
}
