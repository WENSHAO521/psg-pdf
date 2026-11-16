//! Catalogs and the lookup of strings in the current language.
//!
//! A catalog is a UTF-8 text file of rows `context <TAB> English <TAB> translation`. Lines
//! starting with `#` and blank lines are comments. There are no escapes: a backslash is a
//! backslash, so TeX in a hint is written as it is shown. The context is usually empty; it tells
//! apart one English word that needs two translations, and marks plural rows:
//!
//! ```text
//! ␉Compile␉编译
//! @one␉{n} error␉{n} erreur
//! @other␉{n} error␉{n} erreurs
//! ```
//!
//! (`␉` stands for a tab.)
//!
//! A plural row is keyed by the English singular and holds the form for that category. A language
//! with one form (Chinese, Japanese, Korean, Vietnamese) has only the `@other` row.
//!
//! A missing row falls back to English, never to another language: a Traditional Chinese reader
//! sees English rather than Simplified characters, and the checks ([`crate::check`]) report the gap.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::plural::{Plural, plural};
use crate::{LANGUAGE_COUNT, LANGUAGES, Language, fill, language};

/// Something wrong with a catalog row. The row is skipped, so its string shows in English.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub code: &'static str,
    /// 1-based line number.
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.tsv:{}: {}", self.code, self.line, self.message)
    }
}

/// One row of a catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Row {
    pub context: &'static str,
    pub english: &'static str,
    pub translation: &'static str,
    pub line: usize,
}

/// The rows of one language.
#[derive(Debug, Default)]
pub struct Catalog {
    code: &'static str,
    rows: Vec<Row>,
    index: HashMap<(&'static str, &'static str), usize>,
}

impl Catalog {
    /// Parse `text` as the catalog of `code`. Rows that can't be used are left out and reported:
    /// a wrong number of columns, an empty translation, a placeholder the English doesn't have
    /// (or lacks one it has), a second row for the same key, or an unknown plural context.
    pub fn parse(code: &'static str, text: &'static str) -> (Catalog, Vec<Problem>) {
        let mut catalog = Catalog { code, ..Catalog::default() };
        let mut problems = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let line_no = i.saturating_add(1);
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let mut problem = |message: String| problems.push(Problem { code, line: line_no, message });
            let cols: Vec<&'static str> = line.split('\t').collect();
            let [context, english, translation] = cols[..] else {
                problem(format!("{} columns, expected 3 (context, English, translation)", cols.len()));
                continue;
            };
            if english.is_empty() || translation.trim().is_empty() {
                problem("empty English or translation".into());
                continue;
            }
            if context.starts_with('@') && !matches!(context, "@one" | "@other") {
                problem(format!("unknown plural context {context:?} (use @one or @other)"));
                continue;
            }
            if crate::placeholders(english) != crate::placeholders(translation) {
                problem(format!("placeholders differ: {:?} vs {:?}", crate::placeholders(english), crate::placeholders(translation)));
                continue;
            }
            if catalog.index.contains_key(&(context, english)) {
                problem(format!("{english:?} appears twice"));
                continue;
            }
            catalog.index.insert((context, english), catalog.rows.len());
            catalog.rows.push(Row { context, english, translation, line: line_no });
        }
        (catalog, problems)
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The translation of `english` in `context`, falling back to the plain row (empty context).
    pub fn get(&self, context: &str, english: &str) -> Option<&'static str> {
        let exact = self.index.get(&(context, english));
        let plain = || if context.is_empty() { None } else { self.index.get(&("", english)) };
        exact.or_else(plain).and_then(|&i| self.rows.get(i)).map(|r| r.translation)
    }

    /// The exact row for `(context, english)`, without the plain fallback.
    pub fn row(&self, context: &str, english: &str) -> Option<&Row> {
        self.index.get(&(context, english)).and_then(|&i| self.rows.get(i))
    }
}

/// The catalogs of one app and the language it is showing.
#[derive(Debug)]
pub struct Bundle {
    /// Per [`LANGUAGES`] index; English has none.
    catalogs: [Option<Catalog>; LANGUAGE_COUNT],
    current: AtomicUsize,
}

impl Bundle {
    /// The bundle of `(code, catalog text)` pairs. A code that isn't one of [`LANGUAGES`] is
    /// reported and left out.
    pub fn new(sources: &[(&'static str, &'static str)]) -> (Bundle, Vec<Problem>) {
        let mut catalogs: [Option<Catalog>; LANGUAGE_COUNT] = Default::default();
        let mut problems = Vec::new();
        for &(code, text) in sources {
            let Some(i) = LANGUAGES.iter().position(|l| l.code == code) else {
                problems.push(Problem { code: "?", line: 0, message: format!("{code} is not one of the offered languages") });
                continue;
            };
            let (catalog, mut p) = Catalog::parse(LANGUAGES[i].code, text);
            problems.append(&mut p);
            if let Some(slot) = catalogs.get_mut(i) {
                *slot = Some(catalog);
            }
        }
        (Bundle { catalogs, current: AtomicUsize::new(0) }, problems)
    }

    /// Show `lang` from now on. Every lookup reads it, so the next frame is in the new language.
    pub fn set_language(&self, lang: &Language) {
        let i = LANGUAGES.iter().position(|l| l.code == lang.code).unwrap_or(0);
        self.current.store(i, Ordering::Relaxed);
    }

    /// The language being shown.
    pub fn language(&self) -> &'static Language {
        LANGUAGES.get(self.current.load(Ordering::Relaxed)).unwrap_or(&LANGUAGES[0])
    }

    /// The catalog of `code`, if it was given one.
    pub fn catalog(&self, code: &str) -> Option<&Catalog> {
        let i = LANGUAGES.iter().position(|l| l.code == code)?;
        self.catalogs.get(i)?.as_ref()
    }

    fn current_catalog(&self) -> Option<&Catalog> {
        self.catalogs.get(self.current.load(Ordering::Relaxed))?.as_ref()
    }

    /// `english` in the current language; `None` when it has no row (or the language is English).
    pub fn lookup(&self, context: &str, english: &str) -> Option<&'static str> {
        self.current_catalog()?.get(context, english)
    }

    /// `english` in the current language, or unchanged without a row.
    pub fn t(&self, english: &'static str) -> &'static str {
        self.lookup("", english).unwrap_or(english)
    }

    /// [`Bundle::t`] for a word that needs a context to tell its senses apart.
    pub fn tc(&self, context: &str, english: &'static str) -> &'static str {
        self.lookup(context, english).unwrap_or(english)
    }

    /// [`Bundle::t`] with the `{name}` placeholders filled.
    pub fn tf(&self, english: &'static str, args: &[(&str, &str)]) -> String {
        fill(self.t(english), args)
    }

    /// A count: `one` (English singular, the key) or `other` by the plural rule of the current
    /// language, with `{n}` and the other placeholders filled.
    pub fn tn(&self, n: u64, one: &'static str, other: &'static str, args: &[(&str, &str)]) -> String {
        let lang = self.language();
        let category = plural(lang.code, n);
        let template = self
            .current_catalog()
            .and_then(|c| c.row(category.context(), one).or_else(|| c.row(Plural::Other.context(), one)))
            .map(|r| r.translation)
            .unwrap_or(if plural(ENGLISH_CODE, n) == Plural::One { one } else { other });
        let n = n.to_string();
        let mut all: Vec<(&str, &str)> = vec![("n", n.as_str())];
        all.extend_from_slice(args);
        fill(template, &all)
    }
}

const ENGLISH_CODE: &str = "en";

/// The language for a saved code, falling back to English: a convenience for [`Bundle::set_language`].
pub fn language_or_english(code: &str) -> &'static Language {
    language(code).unwrap_or(&LANGUAGES[0])
}
