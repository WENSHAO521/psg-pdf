//! The checks every catalog passes before a language counts as complete.
//!
//! - **Missing rows:** every string the code looks up ([`source_keys`]) has a row, and a count has
//!   a row for each plural category of the language ([`crate::plural::categories`]).
//! - **Stale rows:** no row is left for a string the code no longer uses.
//! - **Well-formed rows:** [`crate::Catalog::parse`] found nothing wrong (columns, placeholders,
//!   duplicates).
//! - **Terminology:** a row whose English uses a glossary term uses the glossary's translation of
//!   it, and a protected name (LaTeX, BibTeX, DOI, ORCID, the product names) stays as written
//!   ([`Glossary`]).
//!
//! The apps' tests run [`report`] for every language and fail on any finding, so a language with
//! gaps can't be released as complete.

use std::collections::BTreeSet;

use crate::catalog::Catalog;
use crate::plural::{Plural, categories};

/// One string the code looks up.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key {
    pub context: String,
    pub english: String,
    /// For a count: the English plural form. The key ([`Key::english`]) is the singular.
    pub other: Option<String>,
}

/// The strings `src` (Rust source) passes as string literals to the lookup functions `t`, `tf`,
/// `ui_text` (the shared widgets), `mark` (text a lower crate words for the interface), `tc` (context first) and `tn` (count first, then singular and
/// plural).
pub fn source_keys(src: &str) -> Vec<Key> {
    let mut keys = Vec::new();
    let mut i = 0;
    while let Some(off) = src.get(i..).and_then(|s| s.find('(')) {
        let open = i.saturating_add(off);
        i = open.saturating_add(1);
        let name_start =
            src.get(..open).map_or(open, |s| s.rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).map_or(0, |p| p.saturating_add(1)));
        let Some(name) = src.get(name_start..open) else { continue };
        // A path (`i18n::t(`) or a method (`bundle.t(`) counts; `my_t(` is another function.
        let after = |s: usize| src.get(s..).unwrap_or_default();
        match name {
            "t" | "tf" | "ui_text" | "mark" => {
                if let Some((english, _)) = literal(after(i)) {
                    keys.push(Key { context: String::new(), english, other: None });
                }
            }
            "tc" => {
                if let Some((context, used)) = literal(after(i)) {
                    if let Some((english, _)) = next_literal(after(i.saturating_add(used))) {
                        keys.push(Key { context, english, other: None });
                    }
                }
            }
            "tn" => {
                // The count is any expression: skip to the first literal at this call's depth.
                if let Some(start) = first_literal_in_call(after(i)) {
                    let s = i.saturating_add(start);
                    if let Some((one, used)) = literal(after(s)) {
                        if let Some((other, _)) = next_literal(after(s.saturating_add(used))) {
                            keys.push(Key { context: String::new(), english: one, other: Some(other) });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

/// Where the first string literal of a call's arguments starts in `s` (just after the `(`), if
/// one comes before the call closes.
fn first_literal_in_call(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '"' if depth == 0 => return Some(i),
            '(' | '[' | '{' => depth = depth.saturating_add(1),
            ')' | ']' | '}' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// The literal after a `,` (and whitespace) at the start of `s`.
fn next_literal(s: &str) -> Option<(String, usize)> {
    let rest = s.trim_start();
    let rest = rest.strip_prefix(',')?;
    let skipped = s.len().saturating_sub(rest.len());
    literal(rest).map(|(v, n)| (v, n.saturating_add(skipped)))
}

/// A Rust string literal (`"…"`, after optional whitespace) at the start of `s`, unescaped, and
/// how many bytes it took. Raw strings and anything that isn't a literal give `None`.
fn literal(s: &str) -> Option<(String, usize)> {
    let trimmed = s.trim_start();
    let lead = s.len().saturating_sub(trimmed.len());
    let body = trimmed.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, lead.saturating_add(i).saturating_add(2))),
            '\\' => match chars.next()?.1 {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                '0' => out.push('\0'),
                '\\' => out.push('\\'),
                '"' => out.push('"'),
                '\'' => out.push('\''),
                'u' => {
                    let mut hex = String::new();
                    for (_, h) in chars.by_ref() {
                        match h {
                            '{' => {}
                            '}' => break,
                            h => hex.push(h),
                        }
                    }
                    out.push(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32)?);
                }
                // A line continuation: the newline and the next line's leading whitespace go.
                '\n' => {
                    let rest = body.get(i.saturating_add(2)..)?;
                    let skip = rest.len().saturating_sub(rest.trim_start().len());
                    for _ in 0..rest.get(..skip)?.chars().count() {
                        chars.next();
                    }
                }
                _ => return None,
            },
            c => out.push(c),
        }
    }
    None
}

/// The academic publishing glossary: for each English term, how each language writes it.
///
/// `i18n/glossary.tsv` has a header row of language codes (`en`, then the others) and a row per
/// term. A cell may list alternatives separated by `|` (a Korean or German compound may take a
/// different form); `=` means the term is a name that stays exactly as written in English; an
/// empty cell sets no rule for that language. Matching is case-insensitive, by whole word in the
/// English (an English plural `-s` still matches), and by substring in the translation, so a
/// German compound such as "Literaturverzeichnisdatei" still uses "Literaturverzeichnis".
#[derive(Debug, Default)]
pub struct Glossary {
    codes: Vec<String>,
    terms: Vec<(String, Vec<String>)>,
}

impl Glossary {
    pub fn parse(text: &str) -> Result<Glossary, String> {
        let mut lines = text.lines().map(|l| l.strip_suffix('\r').unwrap_or(l)).filter(|l| !l.starts_with('#') && !l.trim().is_empty());
        let header: Vec<String> = lines.next().ok_or("the glossary has no header row")?.split('\t').map(str::to_owned).collect();
        if header.first().map(String::as_str) != Some("en") {
            return Err("the glossary's first column must be `en`".into());
        }
        let mut terms = Vec::new();
        for line in lines {
            let cells: Vec<String> = line.split('\t').map(str::to_owned).collect();
            if cells.len() != header.len() {
                return Err(format!("{:?}: {} cells, the header has {}", cells.first(), cells.len(), header.len()));
            }
            let mut cells = cells.into_iter();
            let english = cells.next().unwrap_or_default();
            terms.push((english, cells.collect()));
        }
        Ok(Glossary { codes: header.into_iter().skip(1).collect(), terms })
    }

    /// The English terms, in file order.
    pub fn terms(&self) -> impl Iterator<Item = &str> {
        self.terms.iter().map(|(t, _)| t.as_str())
    }

    /// The languages the glossary has a column for.
    pub fn codes(&self) -> &[String] {
        &self.codes
    }

    /// What is wrong with translating `english` as `translation` in `code`, one line per term.
    pub fn violations(&self, code: &str, english: &str, translation: &str) -> Vec<String> {
        let Some(col) = self.codes.iter().position(|c| c == code) else { return Vec::new() };
        // `{error}` is a value, not the word "error".
        let english = &without_placeholders(english);
        let translation = &without_placeholders(translation);
        let mut out = Vec::new();
        for (term, cells) in &self.terms {
            let Some(cell) = cells.get(col).map(|c| c.trim()).filter(|c| !c.is_empty()) else { continue };
            if !contains_word(english, term) {
                continue;
            }
            if cell == "=" {
                // As the English writes it here: `biber` the command stays lowercase.
                let written = occurrence(english, term).unwrap_or(term);
                if !translation.contains(written) {
                    out.push(format!("{written:?} is a name and must stay as written"));
                }
                continue;
            }
            let lower = translation.to_lowercase();
            if !cell.split('|').map(str::trim).any(|alt| lower.contains(&alt.to_lowercase())) {
                out.push(format!("{term:?} should be translated as {cell:?}"));
            }
        }
        out
    }
}

/// The first whole-word occurrence of `term` in `text` (any case), as `text` writes it.
fn occurrence<'a>(text: &'a str, term: &str) -> Option<&'a str> {
    let lower_term = term.to_lowercase();
    text.char_indices().map(|(i, _)| i).find_map(|i| {
        let candidate = text.get(i..i.saturating_add(term.len()))?;
        let before = text.get(..i).and_then(|s| s.chars().next_back());
        (candidate.to_lowercase() == lower_term && !before.is_some_and(char::is_alphanumeric)).then_some(candidate)
    })
}

/// `text` with its `{name}` placeholders removed.
fn without_placeholders(text: &str) -> String {
    let mut out = text.to_owned();
    for name in crate::placeholders(text) {
        out = out.replace(&format!("{{{name}}}"), " ");
    }
    out
}

/// Whether `text` uses `term` as a whole word (case-insensitive; an English plural `s` or `es`
/// after it still counts).
pub fn contains_word(text: &str, term: &str) -> bool {
    let text = text.to_lowercase();
    let term = term.to_lowercase();
    if term.is_empty() {
        return false;
    }
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric());
    let mut from = 0;
    while let Some(off) = text.get(from..).and_then(|s| s.find(&term)) {
        let start = from.saturating_add(off);
        let end = start.saturating_add(term.len());
        let before = text.get(..start).and_then(|s| s.chars().next_back());
        let rest = text.get(end..).unwrap_or_default();
        let rest = rest.strip_prefix("es").filter(|r| !word(r.chars().next())).or_else(|| rest.strip_prefix('s')).unwrap_or(rest);
        if !word(before) && !word(rest.chars().next()) {
            return true;
        }
        from = start.saturating_add(term.chars().next().map_or(1, char::len_utf8));
    }
    false
}

/// What one language's catalog lacks or gets wrong against the strings the code uses.
#[derive(Debug, Default)]
pub struct Report {
    pub code: String,
    /// Strings the code uses without a row, as `context / English` (or `@one / English`).
    pub missing: Vec<String>,
    /// Rows for strings the code doesn't use.
    pub stale: Vec<String>,
    /// Glossary findings, as `line: English: what`.
    pub terminology: Vec<String>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.missing.is_empty() && self.stale.is_empty() && self.terminology.is_empty()
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{}: {} missing, {} stale, {} terminology", self.code, self.missing.len(), self.stale.len(), self.terminology.len())?;
        for (label, list) in [("missing", &self.missing), ("stale", &self.stale), ("terminology", &self.terminology)] {
            for item in list {
                writeln!(f, "  {label}: {item}")?;
            }
        }
        Ok(())
    }
}

/// Check `catalog` against the `keys` the code uses and the `glossary`.
pub fn report(catalog: &Catalog, keys: &[Key], glossary: &Glossary) -> Report {
    let code = catalog.code();
    let mut used: BTreeSet<(String, String)> = BTreeSet::new();
    let mut r = Report { code: code.to_owned(), ..Report::default() };
    for key in keys {
        if key.other.is_some() {
            for category in categories(code) {
                used.insert((category.context().to_owned(), key.english.clone()));
                if catalog.row(category.context(), &key.english).is_none() {
                    r.missing.push(format!("{} / {}", category.context(), key.english));
                }
            }
            // A one-form language may still carry an `@one` row; it is used, not stale.
            used.insert((Plural::One.context().to_owned(), key.english.clone()));
        } else {
            used.insert((key.context.clone(), key.english.clone()));
            if catalog.get(&key.context, &key.english).is_none() {
                r.missing.push(format!("{} / {}", key.context, key.english));
            }
        }
    }
    for row in catalog.rows() {
        if !used.contains(&(row.context.to_owned(), row.english.to_owned())) {
            r.stale.push(format!("line {}: {}", row.line, row.english));
        }
        for v in glossary.violations(code, row.english, row.translation) {
            r.terminology.push(format!("line {}: {:?}: {v}", row.line, row.english));
        }
    }
    r
}
