//! The interface language of PSG Academic PDF, on the shared `psg-i18n` crate.
//!
//! English strings are the keys. Translations live in `i18n/psg-academic/<code>.tsv` at the
//! repository root (`context <TAB> English <TAB> translation`, no escapes; see
//! `psg_i18n::catalog`), are embedded at build time, and a missing row shows in English. The
//! language follows the system unless the person picks one in Settings, which is saved in the
//! data folder (`language.txt`: `auto` or a code) and applies on the next frame.
//!
//! Only interface text goes through here. The manuscript, its LaTeX source, the bibliography,
//! the article metadata and the compiler's log are never translated, so switching the language
//! can't change what is typeset.

use std::sync::LazyLock;

use psg_i18n::{Bundle, Language, Preference};

/// The catalog of each translated language, in [`psg_i18n::LANGUAGES`] order.
pub static CATALOGS: [(&str, &str); 9] = [
    ("zh-hans", include_str!("../../../i18n/psg-academic/zh-hans.tsv")),
    ("zh-hant", include_str!("../../../i18n/psg-academic/zh-hant.tsv")),
    ("ko", include_str!("../../../i18n/psg-academic/ko.tsv")),
    ("ja", include_str!("../../../i18n/psg-academic/ja.tsv")),
    ("de", include_str!("../../../i18n/psg-academic/de.tsv")),
    ("fr", include_str!("../../../i18n/psg-academic/fr.tsv")),
    ("es", include_str!("../../../i18n/psg-academic/es.tsv")),
    ("pt-br", include_str!("../../../i18n/psg-academic/pt-br.tsv")),
    ("vi", include_str!("../../../i18n/psg-academic/vi.tsv")),
];

static BUNDLE: LazyLock<Bundle> = LazyLock::new(|| {
    let (bundle, problems) = Bundle::new(&CATALOGS);
    for p in problems {
        log::warn!("interface catalog: {p}");
    }
    bundle
});

/// Show the language the saved preference names (`auto`, a code, or anything else = auto), and
/// make the shared design-system widgets speak it too.
pub fn apply(pref: Preference) {
    set_language(pref.resolve());
}

/// Show `lang` from now on.
pub fn set_language(lang: &'static Language) {
    BUNDLE.set_language(lang);
    psg_i18n::install_ui_translator(|s| lookup(s).unwrap_or(s).to_owned());
}

/// The language being shown.
pub fn language() -> &'static Language {
    BUNDLE.language()
}

/// The code of the language being shown.
pub fn current() -> &'static str {
    language().code
}

/// `english` in the current language, for text that isn't `'static`.
fn lookup(english: &str) -> Option<&'static str> {
    BUNDLE.lookup("", english)
}

/// One of the updater's strings ([`psg_i18n::updater::TEXT`]) in the current language, its
/// `{name}` values filled: the translator craft-update's prompt and dialogs call as they draw.
pub fn updater_text(english: &str, args: &[(&str, &str)]) -> String {
    psg_i18n::fill(lookup(english).unwrap_or(english), args)
}

/// `english` in the current language (unchanged when there is no translation).
pub fn t(english: &'static str) -> &'static str {
    BUNDLE.t(english)
}

/// [`t`] for a word whose senses need a `context` to tell apart.
#[allow(dead_code)]
pub fn tc(context: &str, english: &'static str) -> &'static str {
    BUNDLE.tc(context, english)
}

/// [`t`] with `{name}` placeholders filled in one pass (inserted values are never read again).
pub fn tf(english: &'static str, args: &[(&str, &str)]) -> String {
    BUNDLE.tf(english, args)
}

/// A count: the singular `one` (the key) or plural `other`, by the plural rule of the current
/// language, with `{n}` and `args` filled.
pub fn tn(n: usize, one: &'static str, other: &'static str, args: &[(&str, &str)]) -> String {
    BUNDLE.tn(u64::try_from(n).unwrap_or(u64::MAX), one, other, args)
}

#[cfg(test)]
mod tests;
