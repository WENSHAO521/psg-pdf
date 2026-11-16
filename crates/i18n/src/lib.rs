//! The interface languages of PSG PDF and PSG Academic PDF, in one place.
//!
//! Both products offer the same ten languages ([`LANGUAGES`]): English, Simplified Chinese,
//! Traditional Chinese, Korean, Japanese, German, French, Spanish, Brazilian Portuguese and
//! Vietnamese. This crate owns what the two apps share about them:
//!
//! - which languages there are, their names, and their plural rules ([`Language`], [`plural`]);
//! - how a system locale such as `zh_TW.UTF-8` or `pt-PT` picks one ([`for_locale`],
//!   [`system_language`]), and the saved preference that can override it ([`Preference`]);
//! - catalogs of `context <TAB> English <TAB> translation` rows ([`Catalog`]) and a [`Bundle`]
//!   that looks strings up in the current language, with `{name}` placeholders ([`fill`]),
//!   plural forms ([`Bundle::tn`]) and English as the fallback for a missing row;
//! - the academic publishing glossary and the checks every catalog must pass ([`check`]):
//!   missing and stale rows, placeholders, duplicates, terminology and protected names;
//! - a hook for the shared design-system widgets ([`ui_text`]), so a component in
//!   `psg-desktop-ui` speaks whichever app's language it is drawn in, and the list of the shared
//!   updater's strings ([`updater::TEXT`]) that both apps translate.
//!
//! English text in the code is the key. Only interface text is translated: manuscripts, LaTeX
//! source, bibliographies, metadata, file names, command ids and log output never pass through
//! here, so switching the interface language can't change a document.
//!
//! PSG PDF's large engine catalog stays in craft-i18n (see `crates/ui-egui/src/i18n.rs`), which
//! it shares with upstream; the strings PSG adds sit in `i18n/psg/<code>.tsv`. PSG Academic PDF's
//! catalogs are `i18n/psg-academic/<code>.tsv`. Both are checked against [`LANGUAGES`] and the
//! glossary (`i18n/glossary.tsv`) by the tests in each app.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod catalog;
pub mod check;
pub mod locale;
pub mod plural;
pub mod ui;
pub mod updater;

pub use catalog::{Bundle, Catalog, Problem};
pub use locale::{Preference, for_locale, system_language};
pub use plural::{Plural, plural};
pub use ui::{install_ui_translator, ui_text};
pub use updater::mark;

/// One interface language both products offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    /// Lowercase BCP 47 code: the preference value, the catalog file name (`<code>.tsv`) and the
    /// craft-i18n locale.
    pub code: &'static str,
    /// The name in the language itself, as the language picker shows it.
    pub native_name: &'static str,
    /// The English name, for logs, documentation and screen readers.
    pub english_name: &'static str,
}

/// How many languages there are.
pub const LANGUAGE_COUNT: usize = 10;

/// The languages, English first: it is the source language and the fallback.
pub const LANGUAGES: [Language; LANGUAGE_COUNT] = [
    Language { code: "en", native_name: "English", english_name: "English" },
    Language { code: "zh-hans", native_name: "简体中文", english_name: "Chinese (Simplified)" },
    Language { code: "zh-hant", native_name: "繁體中文", english_name: "Chinese (Traditional)" },
    Language { code: "ko", native_name: "한국어", english_name: "Korean" },
    Language { code: "ja", native_name: "日本語", english_name: "Japanese" },
    Language { code: "de", native_name: "Deutsch", english_name: "German" },
    Language { code: "fr", native_name: "Français", english_name: "French" },
    Language { code: "es", native_name: "Español", english_name: "Spanish" },
    Language { code: "pt-br", native_name: "Português (Brasil)", english_name: "Portuguese (Brazil)" },
    Language { code: "vi", native_name: "Tiếng Việt", english_name: "Vietnamese" },
];

/// English, the source language.
pub const ENGLISH: &Language = &LANGUAGES[0];

/// The languages that have catalogs: all but English.
pub fn translated() -> impl Iterator<Item = &'static Language> {
    LANGUAGES.iter().filter(|l| l.code != ENGLISH.code)
}

/// The language with `code` (any case), if it is one of [`LANGUAGES`].
pub fn language(code: &str) -> Option<&'static Language> {
    LANGUAGES.iter().find(|l| l.code.eq_ignore_ascii_case(code))
}

/// `template` with each `{name}` in `args` replaced by its value, in one pass: an inserted value
/// is never read as a template, so a file name containing `{path}` stays as it is. A placeholder
/// `args` lacks, and any other brace (TeX in a hint), stays as written.
pub fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some((before, after)) = rest.split_once('{') {
        out.push_str(before);
        let Some((name, tail)) = after.split_once('}') else {
            out.push('{');
            rest = after;
            break;
        };
        match args.iter().find(|(n, _)| *n == name) {
            Some((_, v)) => out.push_str(v),
            None => {
                out.push('{');
                out.push_str(name);
                out.push('}');
            }
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

/// The `{name}` placeholders of `text`, sorted: identifiers of ASCII lowercase letters, digits and
/// `_`. Other braces (`\textbf{…}` in a hint) are not placeholders.
pub fn placeholders(text: &str) -> Vec<&str> {
    let mut v: Vec<&str> = text
        .split('{')
        .skip(1)
        .filter_map(|p| p.split_once('}').map(|(n, _)| n))
        .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
        .collect();
    v.sort_unstable();
    v
}

#[cfg(test)]
mod tests;
