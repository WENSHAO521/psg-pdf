//! The article's own numbers that a journal's template types in: its volume, issue, year,
//! first page and DOI, as `\documentclass` options (`volume=2, firstpage=43`) or preamble
//! commands (`\doi{…}`, `\volume{…}`). A journal hands out the last article it published as its
//! template, so these belong to that article: the project takes them as its metadata's starting
//! values, and every metadata write puts the metadata's values back into the main file, so the
//! next article never goes out with the previous one's DOI.

use crate::metadata::PublicationMetadata;
use crate::template_import::{calls, find_call, mask_comments};

/// A value the main file types, by the metadata field it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Volume,
    Issue,
    Year,
    FirstPage,
    Doi,
}

impl Field {
    /// The metadata's value for this field: the first page is the start of `pages`.
    fn in_metadata(self, m: &PublicationMetadata) -> String {
        match self {
            Field::Volume => m.volume.clone(),
            Field::Issue => m.issue.clone(),
            Field::Year => m.year.clone(),
            Field::FirstPage => m.pages.split(['-', '–', ' ']).next().unwrap_or("").trim().to_owned(),
            Field::Doi => m.doi.clone(),
        }
    }

    /// Put the template's value into the metadata.
    fn set_in(self, value: &str, m: &mut PublicationMetadata) {
        let value = value.trim().to_owned();
        match self {
            Field::Volume => m.volume = value,
            Field::Issue => m.issue = value,
            Field::Year => m.year = value,
            Field::FirstPage => m.pages = value,
            Field::Doi => m.doi = value,
        }
    }

    /// The metadata field's name, as a metadata file writes it.
    pub fn key(self) -> &'static str {
        match self {
            Field::Volume => "volume",
            Field::Issue => "issue",
            Field::Year => "year",
            Field::FirstPage => "pages",
            Field::Doi => "doi",
        }
    }

    /// The name people see, for the report.
    pub fn label(self) -> &'static str {
        match self {
            Field::Volume => "volume",
            Field::Issue => "issue",
            Field::Year => "year",
            Field::FirstPage => "first page",
            Field::Doi => "DOI",
        }
    }
}

/// `\documentclass` option keys, and the field each is.
const OPTIONS: &[(&str, Field)] = &[
    ("volume", Field::Volume),
    ("vol", Field::Volume),
    ("issue", Field::Issue),
    ("number", Field::Issue),
    ("year", Field::Year),
    ("firstpage", Field::FirstPage),
    ("startpage", Field::FirstPage),
    ("doi", Field::Doi),
];

/// One-argument preamble commands, and the field each is. `\year` is TeX's own register and is
/// never one of them.
const COMMANDS: &[(&str, Field)] = &[
    ("doi", Field::Doi),
    ("volume", Field::Volume),
    ("issue", Field::Issue),
    ("pubyear", Field::Year),
    ("copyrightyear", Field::Year),
    ("firstpage", Field::FirstPage),
];

/// Where a value is typed in the main file: its byte range and field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub field: Field,
    pub from: usize,
    pub to: usize,
}

/// The values the main file types before `\begin{document}`, in file order, empty ones too.
/// Only plain values are taken: one with a brace, backslash or `#` is the template's own macro,
/// not a number.
pub fn find(main: &str) -> Vec<Found> {
    let masked = mask_comments(main);
    let begin = masked.find("\\begin{document}").unwrap_or(masked.len());
    // An empty value is found too, so a value the metadata cleared can be written again.
    let plain = |v: &str| !v.contains(['{', '}', '\\', '#']);
    let mut out = Vec::new();
    if let Some(class) = find_call(&masked, "documentclass", 0, 1).filter(|c| c.start < begin) {
        for &(a, b) in &class.optional {
            // From the masked text, so a comment between options is no part of a key; its byte
            // offsets are the main file's.
            let Some(options) = masked.get(a..b) else { continue };
            let mut at = a;
            for item in options.split(',') {
                let item_start = at;
                at = at.saturating_add(item.len()).saturating_add(1);
                let Some((key, value)) = item.split_once('=') else { continue };
                let Some(&(_, field)) = OPTIONS.iter().find(|(k, _)| key.trim().eq_ignore_ascii_case(k)) else { continue };
                if !plain(value) {
                    continue;
                }
                // The value without the spaces and line ends around it.
                let value_start = item_start.saturating_add(key.len()).saturating_add(1);
                let lead = value.len() - value.trim_start().len();
                let from = value_start.saturating_add(lead);
                let to = from.saturating_add(value.trim().len());
                if out.iter().all(|f: &Found| f.field != field) {
                    out.push(Found { field, from, to });
                }
            }
        }
    }
    for &(name, field) in COMMANDS {
        for call in calls(&masked, name, 0, begin, 1) {
            let Some(&(from, to)) = call.mandatory.first() else { continue };
            if call.optional.is_empty() && main.get(from..to).is_some_and(plain) && out.iter().all(|f: &Found| f.field != field) {
                out.push(Found { field, from, to });
            }
        }
    }
    out.sort_by_key(|f| f.from);
    out
}

/// Take the main file's values into `metadata`. Returns what was taken, for the report.
pub fn take(main: &str, metadata: &mut PublicationMetadata) -> Vec<(Field, String)> {
    let mut taken = Vec::new();
    for found in find(main) {
        let Some(value) = main.get(found.from..found.to).filter(|v| !v.trim().is_empty()) else { continue };
        found.field.set_in(value, metadata);
        taken.push((found.field, value.trim().to_owned()));
    }
    taken
}

/// Whether `value` may be typed where the template typed its own: in a class option it may not
/// hold `,` `=` or braces, and nowhere a backslash, `#`, `%` or a line end.
fn writable(value: &str) -> bool {
    !value.contains(['{', '}', '\\', '#', '%', ',', '=', '\n', '\r'])
}

/// `main` with the metadata's values where the template typed its own, or `None` when nothing
/// changes. A metadata value that can't be typed there safely leaves the template's.
pub fn write(main: &str, metadata: &PublicationMetadata) -> Option<String> {
    let mut out = main.to_owned();
    let mut changed = false;
    for found in find(main).into_iter().rev() {
        let value = found.field.in_metadata(metadata);
        let value = value.trim();
        if !writable(value) || main.get(found.from..found.to) == Some(value) {
            continue;
        }
        if out.is_char_boundary(found.from) && out.is_char_boundary(found.to) {
            out.replace_range(found.from..found.to, value);
            changed = true;
        }
    }
    changed.then_some(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const MAIN: &str = "\\documentclass[\n  journal=afs,\n  year=2026,\n  volume=2,\n  issue=2,\n  firstpage=43,\n]{cup-journal}\n% \\doi{10.1/commented}\n\\doi{10.63802/afs.V2.I2.366}\n\\newcommand\\volume[1]{x}\n\\begin{document}\n\\doi{10.9/in-the-text}\n\\end{document}\n";

    #[test]
    fn the_previous_articles_numbers_are_found_and_taken() {
        let found: Vec<Field> = find(MAIN).iter().map(|f| f.field).collect();
        assert_eq!(found, [Field::Year, Field::Volume, Field::Issue, Field::FirstPage, Field::Doi]);
        let mut m = PublicationMetadata::sample();
        let taken = take(MAIN, &mut m);
        assert_eq!(taken.len(), 5);
        assert_eq!((m.volume.as_str(), m.issue.as_str(), m.year.as_str()), ("2", "2", "2026"));
        assert_eq!((m.pages.as_str(), m.doi.as_str()), ("43", "10.63802/afs.V2.I2.366"));
    }

    #[test]
    fn the_metadata_goes_back_where_the_template_typed_it() {
        let mut m = PublicationMetadata::sample();
        take(MAIN, &mut m);
        assert_eq!(write(MAIN, &m), None, "unchanged metadata leaves the file as it is");
        m.volume = "3".into();
        m.issue = "1".into();
        m.pages = "1--20".into();
        m.doi = "10.63802/afs.V3.I1.401".into();
        let out = write(MAIN, &m).unwrap();
        assert!(out.contains("  volume=3,\n  issue=1,\n  firstpage=1,\n"), "{out}");
        assert!(out.contains("\n\\doi{10.63802/afs.V3.I1.401}\n"), "{out}");
        assert!(out.contains("% \\doi{10.1/commented}") && out.contains("\\doi{10.9/in-the-text}"), "comments and the text are left");
        m.volume = "3,draft".into();
        assert!(write(MAIN, &m).unwrap().contains("volume=2,"), "a value that would break the options is not written");
    }

    #[test]
    fn a_cleared_value_can_be_set_again() {
        let mut m = PublicationMetadata::sample();
        take(MAIN, &mut m);
        m.doi.clear();
        let cleared = write(MAIN, &m).unwrap();
        assert!(cleared.contains("\n\\doi{}\n"), "{cleared}");
        let mut again = PublicationMetadata::sample();
        take(&cleared, &mut again);
        assert_eq!(again.doi, PublicationMetadata::sample().doi, "an empty value is not taken as the article's");
        m.doi = "10.63802/afs.V3.I1.402".into();
        assert!(write(&cleared, &m).unwrap().contains("\n\\doi{10.63802/afs.V3.I1.402}\n"));
    }

    #[test]
    fn a_comment_between_class_options_is_no_part_of_a_key() {
        let main = "\\documentclass[volume=2, % the previous volume\n  issue=3,\n]{cup-journal}\n\\begin{document}\n\\end{document}\n";
        let fields: Vec<Field> = find(main).iter().map(|f| f.field).collect();
        assert_eq!(fields, [Field::Volume, Field::Issue]);
        let mut m = PublicationMetadata::sample();
        take(main, &mut m);
        m.issue = "5".into();
        let out = write(main, &m).unwrap();
        assert!(out.contains("volume=2, % the previous volume\n  issue=5,"), "{out}");
    }

    #[test]
    fn a_template_without_numbers_is_left_alone() {
        let main = "\\documentclass[11pt]{article}\n\\begin{document}\n\\end{document}\n";
        assert!(find(main).is_empty());
        assert_eq!(write(main, &PublicationMetadata::sample()), None);
    }
}
