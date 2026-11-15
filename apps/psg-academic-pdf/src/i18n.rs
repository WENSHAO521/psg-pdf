//! The interface language of PSG Academic PDF.
//!
//! English strings are the keys. Translations live in `i18n/psg-academic/<code>.tsv` at the
//! repository root (`context <TAB> English <TAB> translation`, like craft-i18n but without escapes), are
//! embedded at build time, and a missing row shows in English. The language follows the system
//! unless the person picks one in Settings, which is saved in the data folder.

use std::collections::HashMap;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The languages offered: code, the name in that language.
pub const LANGUAGES: [(&str, &str); 6] =
    [("en", "English"), ("zh-hans", "简体中文"), ("zh-hant", "繁體中文"), ("ja", "日本語"), ("ko", "한국어"), ("de", "Deutsch")];

static CATALOGS: [(&str, &str); 5] = [
    ("zh-hans", include_str!("../../../i18n/psg-academic/zh-hans.tsv")),
    ("zh-hant", include_str!("../../../i18n/psg-academic/zh-hant.tsv")),
    ("ja", include_str!("../../../i18n/psg-academic/ja.tsv")),
    ("ko", include_str!("../../../i18n/psg-academic/ko.tsv")),
    ("de", include_str!("../../../i18n/psg-academic/de.tsv")),
];

/// The index into [`LANGUAGES`] in use.
static CURRENT: AtomicUsize = AtomicUsize::new(0);

/// Use the language with `code` (one of [`LANGUAGES`]); an unknown code means English.
pub fn set(code: &str) {
    CURRENT.store(LANGUAGES.iter().position(|(c, _)| *c == code).unwrap_or(0), Ordering::Relaxed);
}

/// The code of the language in use.
pub fn current() -> &'static str {
    LANGUAGES.get(CURRENT.load(Ordering::Relaxed)).map_or("en", |(c, _)| c)
}

/// The language for a BCP 47 locale such as `zh-CN`, `zh-Hant-TW`, `ja-JP` or `de-AT`.
pub fn for_locale(locale: &str) -> &'static str {
    let l = locale.replace('_', "-").to_ascii_lowercase();
    let primary = l.split(['-', '.']).next().unwrap_or_default();
    match primary {
        "zh" if ["hant", "-tw", "-hk", "-mo"].iter().any(|t| l.contains(t)) => "zh-hant",
        "zh" => "zh-hans",
        "ja" => "ja",
        "ko" => "ko",
        "de" => "de",
        _ => "en",
    }
}

/// The system's language, as far as it is one of ours.
pub fn system() -> &'static str {
    sys_locale::get_locale().map_or("en", |l| for_locale(&l))
}

fn parse(text: &'static str) -> HashMap<&'static str, &'static str> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let mut cols = l.splitn(3, '\t');
            let (_context, en, tr) = (cols.next()?, cols.next()?, cols.next()?);
            (!tr.is_empty()).then_some((en, tr))
        })
        .collect()
}

fn catalog(code: &str) -> Option<&'static HashMap<&'static str, &'static str>> {
    static PARSED: OnceLock<Vec<(&'static str, HashMap<&'static str, &'static str>)>> = OnceLock::new();
    PARSED.get_or_init(|| CATALOGS.iter().map(|(c, text)| (*c, parse(text))).collect()).iter().find(|(c, _)| *c == code).map(|(_, m)| m)
}

/// `english` in the current language (unchanged when there is no translation).
pub fn t(english: &'static str) -> &'static str {
    catalog(current()).and_then(|m| m.get(english).copied()).unwrap_or(english)
}

/// [`t`] with `{name}` placeholders filled in one pass (inserted values are never read again).
pub fn tf(english: &'static str, args: &[(&str, &str)]) -> String {
    let template = t(english);
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some((before, after)) = rest.split_once('{') {
        out.push_str(before);
        match after.split_once('}') {
            Some((name, tail)) => {
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
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn locales_map_to_our_languages() {
        assert_eq!(for_locale("zh-CN"), "zh-hans");
        assert_eq!(for_locale("zh_Hans_SG"), "zh-hans");
        assert_eq!(for_locale("zh-TW"), "zh-hant");
        assert_eq!(for_locale("zh-Hant-HK"), "zh-hant");
        assert_eq!(for_locale("ja-JP"), "ja");
        assert_eq!(for_locale("ko-KR"), "ko");
        assert_eq!(for_locale("de-AT"), "de");
        assert_eq!(for_locale("fr-FR"), "en");
        assert_eq!(for_locale(""), "en");
    }

    /// Every catalog row keeps the English placeholders, and no English key appears twice.
    #[test]
    fn catalogs_are_well_formed() {
        let names = |s: &str| {
            let mut v: Vec<String> = s
                .split('{')
                .skip(1)
                .filter_map(|p| p.split_once('}').map(|(n, _)| n.to_owned()))
                // Placeholders are identifiers; other braces are TeX in a hint.
                .filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
                .collect();
            v.sort();
            v
        };
        for (code, text) in CATALOGS {
            let mut seen = std::collections::HashSet::new();
            for (n, line) in text.lines().enumerate().filter(|(_, l)| !l.starts_with('#') && !l.trim().is_empty()) {
                let cols: Vec<&str> = line.split('\t').collect();
                assert_eq!(cols.len(), 3, "{code}.tsv:{}: three columns", n + 1);
                assert_eq!(names(cols[1]), names(cols[2]), "{code}.tsv:{}: placeholders", n + 1);
                assert!(seen.insert(cols[1]), "{code}.tsv:{}: {} twice", n + 1, cols[1]);
            }
        }
    }

    /// Every language translates the same strings.
    #[test]
    fn catalogs_cover_the_same_strings() {
        let keys = |code: &str| {
            let mut v: Vec<&str> = catalog(code).unwrap().keys().copied().collect();
            v.sort_unstable();
            v
        };
        let zh = keys("zh-hans");
        assert!(zh.len() > 100, "{}", zh.len());
        for (code, _) in CATALOGS {
            assert_eq!(keys(code), zh, "{code}");
        }
    }

    #[test]
    fn translation_and_placeholders() {
        set("zh-hans");
        assert_eq!(t("Compile"), "编译");
        assert_eq!(tf("Opened {path}.", &[("path", "/x/{y}")]), "已打开 /x/{y}。");
        set("en");
        assert_eq!(t("Compile"), "Compile");
        assert_eq!(tf("Opened {path}.", &[("path", "a")]), "Opened a.");
        set("xx");
        assert_eq!(current(), "en");
    }
}
