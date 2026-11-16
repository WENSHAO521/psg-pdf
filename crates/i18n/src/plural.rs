//! Plural forms of whole counts, by the CLDR rules of each language.
//!
//! For the ten languages only two categories matter for integers: German, English and Spanish
//! use the singular for exactly 1; French and Portuguese for 0 and 1; Chinese, Japanese, Korean
//! and Vietnamese have no grammatical plural. (CLDR's `many` for French, Spanish and Portuguese
//! applies to compact millions such as "1 M", which the interface never writes.)

/// A plural category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plural {
    One,
    Other,
}

impl Plural {
    /// The catalog context of rows in this form: `@one` or `@other`.
    pub fn context(self) -> &'static str {
        match self {
            Plural::One => "@one",
            Plural::Other => "@other",
        }
    }
}

/// The category of the count `n` in the language `code`. An unknown code uses the English rule.
pub fn plural(code: &str, n: u64) -> Plural {
    match code {
        "zh-hans" | "zh-hant" | "ja" | "ko" | "vi" => Plural::Other,
        "fr" | "pt-br" => {
            if n <= 1 {
                Plural::One
            } else {
                Plural::Other
            }
        }
        _ => {
            if n == 1 {
                Plural::One
            } else {
                Plural::Other
            }
        }
    }
}

/// The categories `code` uses, so a catalog can be checked for the rows it needs.
pub fn categories(code: &str) -> &'static [Plural] {
    match code {
        "zh-hans" | "zh-hant" | "ja" | "ko" | "vi" => &[Plural::Other],
        _ => &[Plural::One, Plural::Other],
    }
}
