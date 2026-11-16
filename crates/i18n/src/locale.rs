//! Which language a system locale means, and the saved preference that can override it.

use crate::{LANGUAGES, Language, language};

/// The language for a system locale (`zh_TW.UTF-8`, `zh-Hant-HK`, `pt-PT`, `de_AT@euro`, `fr-CA`),
/// or `None` when it is none of ours (or the neutral `C` / `POSIX`).
///
/// - Chinese written in Traditional characters (`zh-Hant…`) or for Taiwan, Hong Kong and Macau
///   (`zh-TW`, `zh-HK`, `zh-MO`) is Traditional Chinese, unless it names Simplified characters
///   (`zh-Hans-HK`); every other Chinese locale is Simplified.
/// - Every Portuguese locale uses the Brazilian catalog, every German, French and Spanish one the
///   one catalog for that language.
pub fn for_locale(tag: &str) -> Option<&'static Language> {
    // `de_AT.UTF-8@euro` → `de-at`: drop the encoding and modifier, unify separators.
    let base = tag.split(['.', '@']).next().unwrap_or_default().trim().replace('_', "-").to_ascii_lowercase();
    let mut parts = base.split('-');
    let primary = parts.next().unwrap_or_default();
    let code = match primary {
        "zh" => {
            let parts: Vec<&str> = parts.collect();
            // An explicit script wins over the region: `zh-Hans-HK` is Simplified.
            if !parts.contains(&"hans") && parts.iter().any(|p| matches!(*p, "hant" | "tw" | "hk" | "mo")) { "zh-hant" } else { "zh-hans" }
        }
        "pt" => "pt-br",
        "en" | "ko" | "ja" | "de" | "fr" | "es" | "vi" => primary,
        _ => return None,
    };
    language(code)
}

/// The first of the system's preferred languages that is one of ours, else English.
pub fn system_language() -> &'static Language {
    first_match(sys_locale::get_locales())
}

/// The first of `tags` (most preferred first) that [`for_locale`] knows, else English.
pub fn first_match(tags: impl IntoIterator<Item = String>) -> &'static Language {
    tags.into_iter().find_map(|t| for_locale(&t)).unwrap_or(&LANGUAGES[0])
}

/// The saved `language` preference: follow the system, or one language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Preference {
    /// Follow the system's preferred languages (the default).
    #[default]
    Auto,
    /// Always this language.
    Fixed(&'static Language),
}

impl Preference {
    /// The value written to the settings file.
    pub const AUTO: &'static str = "auto";

    /// A saved value: `auto`, or a language code in any case. Anything else (a language a later
    /// version offered, a typo, an empty file) follows the system, so a bad settings file can
    /// never leave the interface unreadable.
    pub fn parse(value: &str) -> Preference {
        language(value.trim()).map_or(Preference::Auto, Preference::Fixed)
    }

    /// The value to save.
    pub fn as_str(self) -> &'static str {
        match self {
            Preference::Auto => Self::AUTO,
            Preference::Fixed(l) => l.code,
        }
    }

    /// The language to show, asking the system for [`Preference::Auto`].
    pub fn resolve(self) -> &'static Language {
        self.resolve_with(system_language)
    }

    /// [`Preference::resolve`] with the system's language from `system`, for tests.
    pub fn resolve_with(self, system: impl FnOnce() -> &'static Language) -> &'static Language {
        match self {
            Preference::Auto => system(),
            Preference::Fixed(l) => l,
        }
    }
}
