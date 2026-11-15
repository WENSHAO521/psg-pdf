//! Reading a BibLaTeX `.bib` file well enough to check it before compiling: the entries, their
//! fields, and which citations in the manuscript have no entry (or which entries are never cited).
//!
//! This is a checker, not a replacement for Biber: it reads what Biber reads (entries in braces or
//! parentheses, `{…}`, `"…"` and bare values joined with `#`), skips `@comment`, `@preamble` and
//! `@string`, and reports what would make Biber or the journal unhappy, with the line it is on.

use serde::Serialize;

use crate::diagnostics::Severity;

/// The most entries read from one file; a larger file is reported as too large.
pub const MAX_ENTRIES: usize = 20_000;
/// The most problems kept from one check.
pub const MAX_ISSUES: usize = 500;
/// Brace nesting deeper than this is treated as malformed (bounds the value reader).
const MAX_DEPTH: usize = 64;

/// One `@type{key, field = value, …}` entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BibEntry {
    /// Lowercase entry type (`article`, `book`, …).
    pub kind: String,
    pub key: String,
    /// Field names in lowercase, values with their outer braces or quotes removed.
    pub fields: Vec<(String, String)>,
    /// The 1-based line the entry starts on.
    pub line: u32,
}

impl BibEntry {
    /// The value of `name` (lowercase), if the entry has it and it isn't blank.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, v)| v.trim()).filter(|v| !v.is_empty())
    }

    /// A one-line description for lists: author (or editor), year, title.
    pub fn summary(&self) -> String {
        let who = self.field("author").or_else(|| self.field("editor")).unwrap_or("—");
        let year = self.field("year").or_else(|| self.field("date")).unwrap_or("n.d.");
        let title = self.field("title").unwrap_or("(untitled)");
        format!("{who} ({year}). {title}")
    }
}

/// A problem with the bibliography or with a citation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BibIssue {
    pub severity: Severity,
    pub key: Option<String>,
    /// The 1-based line in the `.bib` file, when the problem is in it.
    pub line: Option<u32>,
    pub message: String,
}

/// The fields each common entry type needs, as alternatives (`journal` or `journaltitle`).
fn required(kind: &str) -> &'static [&'static [&'static str]] {
    const AUTHOR: &[&str] = &["author"];
    const AUTHOR_OR_EDITOR: &[&str] = &["author", "editor"];
    const TITLE: &[&str] = &["title"];
    const DATE: &[&str] = &["year", "date"];
    match kind {
        "article" => &[AUTHOR, TITLE, &["journal", "journaltitle"], DATE],
        "book" | "mvbook" => &[AUTHOR_OR_EDITOR, TITLE, DATE],
        "inbook" | "incollection" | "inproceedings" | "conference" => &[AUTHOR, TITLE, &["booktitle"], DATE],
        "thesis" | "phdthesis" | "mastersthesis" => &[AUTHOR, TITLE, &["institution", "school"], DATE],
        "report" | "techreport" => &[AUTHOR, TITLE, &["institution"], DATE],
        "online" | "electronic" | "www" => &[TITLE, &["url", "doi"]],
        _ => &[TITLE],
    }
}

/// Read every entry of `text`. Problems that stop an entry from being read (a missing key, an
/// unclosed brace) come back as issues, and reading carries on at the next `@`.
pub fn parse(text: &str) -> (Vec<BibEntry>, Vec<BibIssue>) {
    let mut entries = Vec::new();
    let mut issues = Vec::new();
    let mut r = Reader { text, pos: 0 };
    while let Some(at) = r.text.get(r.pos..).and_then(|rest| rest.find('@')) {
        r.pos += at + 1;
        let line = r.line();
        if entries.len() >= MAX_ENTRIES {
            push(
                &mut issues,
                BibIssue {
                    severity: Severity::Error,
                    key: None,
                    line: Some(line),
                    message: format!("more than {MAX_ENTRIES} entries; the rest were not checked"),
                },
            );
            break;
        }
        let kind = r.word().to_ascii_lowercase();
        if kind.is_empty() {
            continue;
        }
        r.skip_space();
        let close = match r.peek() {
            Some('{') => '}',
            Some('(') => ')',
            _ => {
                push(&mut issues, issue_at(line, None, format!("@{kind} is not followed by {{ or (")));
                continue;
            }
        };
        r.pos += 1;
        if matches!(kind.as_str(), "comment" | "preamble" | "string") {
            if r.skip_group(close).is_err() {
                push(&mut issues, issue_at(line, None, format!("@{kind} is never closed")));
                break;
            }
            continue;
        }
        r.skip_space();
        let key: String = r.until(&[',', close]).trim().to_owned();
        if key.is_empty() {
            push(&mut issues, issue_at(line, None, format!("an @{kind} entry has no citation key")));
        } else if key.chars().any(|c| c.is_whitespace() || matches!(c, '{' | '}' | '"' | '#' | '%' | '\'' | '(' | ')' | '=' | '\\')) {
            push(&mut issues, issue_at(line, Some(&key), format!("the key \"{key}\" has a space or a character Biber doesn't allow in keys")));
        }
        let mut fields = Vec::new();
        let mut broken = false;
        loop {
            r.skip_space();
            match r.peek() {
                Some(',') => {
                    r.pos += 1;
                    continue;
                }
                Some(c) if c == close => {
                    r.pos += 1;
                    break;
                }
                None => {
                    broken = true;
                    break;
                }
                _ => {}
            }
            let name = r.word().to_ascii_lowercase();
            r.skip_space();
            if name.is_empty() || r.peek() != Some('=') {
                broken = true;
                break;
            }
            r.pos += 1;
            match r.value(close) {
                Ok(value) => fields.push((name, value)),
                Err(()) => {
                    broken = true;
                    break;
                }
            }
        }
        if broken {
            push(
                &mut issues,
                issue_at(
                    line,
                    Some(key.as_str()).filter(|k| !k.is_empty()),
                    format!("the @{kind} entry is malformed (an unclosed brace or quote, or a field without \"=\")"),
                ),
            );
            // Carry on at the next line that starts an entry.
            continue;
        }
        if !key.is_empty() {
            entries.push(BibEntry { kind, key, fields, line });
        }
    }
    (entries, issues)
}

fn issue_at(line: u32, key: Option<&str>, message: String) -> BibIssue {
    BibIssue { severity: Severity::Error, key: key.map(str::to_owned), line: Some(line), message }
}

fn push(issues: &mut Vec<BibIssue>, issue: BibIssue) {
    if issues.len() < MAX_ISSUES {
        issues.push(issue);
    }
}

struct Reader<'a> {
    text: &'a str,
    /// A byte offset, always on a character boundary.
    pos: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<char> {
        self.text.get(self.pos..).and_then(|s| s.chars().next())
    }

    fn line(&self) -> u32 {
        let before = self.text.get(..self.pos).unwrap_or_default();
        u32::try_from(before.matches('\n').count()).unwrap_or(u32::MAX).saturating_add(1)
    }

    fn skip_space(&mut self) {
        while let Some(c) = self.peek().filter(|c| c.is_whitespace()) {
            self.pos += c.len_utf8();
        }
    }

    /// Letters, digits and `_-:.` (an entry type or a field name).
    fn word(&mut self) -> &str {
        let start = self.pos;
        while let Some(c) = self.peek().filter(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | ':' | '.')) {
            self.pos += c.len_utf8();
        }
        self.text.get(start..self.pos).unwrap_or_default()
    }

    /// Everything up to (not including) the first of `stops`.
    fn until(&mut self, stops: &[char]) -> &str {
        let start = self.pos;
        while let Some(c) = self.peek().filter(|c| !stops.contains(c)) {
            self.pos += c.len_utf8();
        }
        self.text.get(start..self.pos).unwrap_or_default()
    }

    /// Skip to just past the `close` that ends the group just opened, honouring nested braces.
    fn skip_group(&mut self, close: char) -> Result<(), ()> {
        let mut depth = 0usize;
        while let Some(c) = self.peek() {
            self.pos += c.len_utf8();
            match c {
                '{' => depth += 1,
                '}' if depth > 0 => depth -= 1,
                c if c == close && depth == 0 => return Ok(()),
                _ => {}
            }
        }
        Err(())
    }

    /// A field value: parts in braces, in quotes or bare (numbers, `@string` names), joined by `#`.
    fn value(&mut self, close: char) -> Result<String, ()> {
        let mut out = String::new();
        loop {
            self.skip_space();
            match self.peek() {
                Some('{') => {
                    self.pos += 1;
                    out.push_str(&self.braced()?);
                }
                Some('"') => {
                    self.pos += 1;
                    out.push_str(&self.quoted()?);
                }
                Some(c) if c.is_alphanumeric() => out.push_str(self.word()),
                _ => return Err(()),
            }
            self.skip_space();
            match self.peek() {
                Some('#') => self.pos += 1,
                Some(',') => return Ok(out),
                Some(c) if c == close => return Ok(out),
                _ => return Err(()),
            }
        }
    }

    /// The inside of `{…}` (the `{` already read), inner braces kept.
    fn braced(&mut self) -> Result<String, ()> {
        let start = self.pos;
        let mut depth = 0usize;
        while let Some(c) = self.peek() {
            match c {
                '{' => {
                    depth += 1;
                    if depth > MAX_DEPTH {
                        return Err(());
                    }
                }
                '}' if depth == 0 => {
                    let inner = self.text.get(start..self.pos).unwrap_or_default().to_owned();
                    self.pos += 1;
                    return Ok(inner);
                }
                '}' => depth -= 1,
                _ => {}
            }
            self.pos += c.len_utf8();
        }
        Err(())
    }

    /// The inside of `"…"` (the `"` already read); a `"` inside braces doesn't end it.
    fn quoted(&mut self) -> Result<String, ()> {
        let start = self.pos;
        let mut depth = 0usize;
        while let Some(c) = self.peek() {
            match c {
                '{' => depth = (depth + 1).min(MAX_DEPTH + 1),
                '}' => depth = depth.saturating_sub(1),
                '"' if depth == 0 => {
                    let inner = self.text.get(start..self.pos).unwrap_or_default().to_owned();
                    self.pos += 1;
                    return Ok(inner);
                }
                _ => {}
            }
            self.pos += c.len_utf8();
        }
        Err(())
    }
}

/// The citation keys `tex` cites with `\cite`, `\parencite`, `\textcite`, `\autocite`,
/// `\footcite`, `\citeauthor`, `\nocite` and the like (any command whose name contains "cite"),
/// in order of first use. Comments (`%` to the end of the line) are ignored; `\nocite{*}` is not a key.
pub fn cited_keys(tex: &str) -> Vec<String> {
    let mut keys: Vec<String> = Vec::new();
    for line in tex.lines() {
        let line = strip_comment(line);
        let mut rest = line;
        while let Some(i) = rest.find('\\') {
            rest = rest.get(i + 1..).unwrap_or_default();
            let name_len = rest.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(rest.len());
            let name = rest.get(..name_len).unwrap_or_default();
            if !name.to_ascii_lowercase().contains("cite") {
                continue;
            }
            let mut after = rest.get(name_len..).unwrap_or_default().trim_start();
            after = after.strip_prefix('*').unwrap_or(after).trim_start();
            // Up to two optional arguments: [prenote][postnote].
            for _ in 0..2 {
                if let Some(opt) = after.strip_prefix('[') {
                    after = opt.split_once(']').map_or("", |(_, a)| a).trim_start();
                }
            }
            let Some(arg) = after.strip_prefix('{') else { continue };
            let Some((inside, tail)) = arg.split_once('}') else { continue };
            for key in inside.split(',').map(str::trim).filter(|k| !k.is_empty() && *k != "*") {
                if !keys.iter().any(|k| k == key) {
                    keys.push(key.to_owned());
                }
            }
            rest = tail;
        }
    }
    keys
}

/// `line` without its comment: everything from the first `%` that isn't escaped as `\%`.
fn strip_comment(line: &str) -> &str {
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match c {
            '\\' => escaped = !escaped,
            '%' if !escaped => return line.get(..i).unwrap_or_default(),
            _ => escaped = false,
        }
    }
    line
}

/// Check `entries` (with the parse `issues` of their file) against the keys the manuscript cites:
/// duplicate keys, missing required fields, malformed DOIs and years, citations with no entry
/// (errors), and entries nothing cites (warnings: Biber leaves them out of the list).
pub fn check(entries: &[BibEntry], parse_issues: Vec<BibIssue>, cited: &[String]) -> Vec<BibIssue> {
    let mut issues = parse_issues;
    let mut seen: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for e in entries {
        let folded = e.key.to_lowercase();
        if let Some(first) = seen.get(&folded) {
            push(
                &mut issues,
                issue_at(e.line, Some(&e.key), format!("the key \"{}\" is used twice (first on line {first}); Biber keeps only one", e.key)),
            );
        } else {
            seen.insert(folded, e.line);
        }
        for alternatives in required(&e.kind) {
            if !alternatives.iter().any(|f| e.field(f).is_some()) {
                push(
                    &mut issues,
                    BibIssue {
                        severity: Severity::Warning,
                        key: Some(e.key.clone()),
                        line: Some(e.line),
                        message: format!("@{} \"{}\" has no {}", e.kind, e.key, alternatives.join(" or ")),
                    },
                );
            }
        }
        if let Some(doi) = e.field("doi") {
            let bare = doi.trim_start_matches("https://doi.org/").trim_start_matches("http://dx.doi.org/").trim_start_matches("doi:");
            if !crate::metadata::is_doi(bare) {
                push(
                    &mut issues,
                    BibIssue {
                        severity: Severity::Warning,
                        key: Some(e.key.clone()),
                        line: Some(e.line),
                        message: format!("\"{}\": the DOI \"{doi}\" doesn't look like 10.<registrant>/<suffix>", e.key),
                    },
                );
            }
        }
        if let Some(year) = e.field("year")
            && !(year.len() == 4 && year.chars().all(|c| c.is_ascii_digit()))
        {
            push(
                &mut issues,
                BibIssue {
                    severity: Severity::Warning,
                    key: Some(e.key.clone()),
                    line: Some(e.line),
                    message: format!("\"{}\": the year \"{year}\" is not four digits (use date = {{…}} for ranges)", e.key),
                },
            );
        }
    }
    for key in cited {
        if !seen.contains_key(&key.to_lowercase()) {
            push(
                &mut issues,
                BibIssue {
                    severity: Severity::Error,
                    key: Some(key.clone()),
                    line: None,
                    message: format!("\"{key}\" is cited but no bibliography entry has that key"),
                },
            );
        }
    }
    let cited_folded: std::collections::HashSet<String> = cited.iter().map(|k| k.to_lowercase()).collect();
    for e in entries {
        if !cited_folded.contains(&e.key.to_lowercase()) {
            push(
                &mut issues,
                BibIssue {
                    severity: Severity::Warning,
                    key: Some(e.key.clone()),
                    line: Some(e.line),
                    message: format!("\"{}\" is never cited, so it won't appear in the references", e.key),
                },
            );
        }
    }
    issues
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
% A comment line
@string{jnl = "Journal of Tests"}
@article{smith2020,
  author = {Smith, Jane and Doe, John},
  title = {On {Testing} Things},
  journal = jnl,
  year = 2020,
  doi = {10.1234/abc.5}
}
@Book(lee_book,
  editor = "Lee, Min-jun",
  title = "A Book with {"}quotes{"} and {braces}",
  date = {2019-03}
)
@inproceedings{wang2021,
  author = {王小明},
  title = {标题},
  year = {2021a},
  doi = {not-a-doi}
}
"#;

    #[test]
    fn entries_and_fields_are_read() {
        let (entries, issues) = parse(SAMPLE);
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(entries.iter().map(|e| e.key.as_str()).collect::<Vec<_>>(), ["smith2020", "lee_book", "wang2021"]);
        let smith = &entries[0];
        assert_eq!(smith.kind, "article");
        assert_eq!(smith.field("title"), Some("On {Testing} Things"));
        assert_eq!(smith.field("year"), Some("2020"));
        assert_eq!(smith.line, 4);
        assert_eq!(entries[1].kind, "book");
        assert_eq!(entries[1].field("editor"), Some("Lee, Min-jun"));
        assert_eq!(entries[2].field("author"), Some("王小明"));
        assert!(entries[0].summary().starts_with("Smith, Jane"));
    }

    #[test]
    fn problems_are_reported_with_their_lines() {
        let (entries, parse_issues) = parse(SAMPLE);
        let cited = cited_keys("As \\textcite{smith2020} and \\parencite[see][12]{Lee_Book, missing} show.\n% \\cite{commented}\n\\nocite{*}");
        assert_eq!(cited, ["smith2020", "Lee_Book", "missing"]);
        let issues = check(&entries, parse_issues, &cited);
        let text: Vec<String> = issues.iter().map(|i| i.message.clone()).collect();
        let has = |s: &str| text.iter().any(|t| t.contains(s));
        assert!(has("\"missing\" is cited but no bibliography entry"), "{text:?}");
        assert!(has("@inproceedings \"wang2021\" has no booktitle"), "{text:?}");
        assert!(has("the DOI \"not-a-doi\""), "{text:?}");
        assert!(has("the year \"2021a\""), "{text:?}");
        assert!(has("\"wang2021\" is never cited"), "{text:?}");
        assert!(!has("\"smith2020\" is never cited") && !has("\"lee_book\" is never cited"), "{text:?}");
        assert!(issues.iter().any(|i| i.severity == Severity::Error && i.key.as_deref() == Some("missing")));
    }

    #[test]
    fn duplicates_and_malformed_entries_are_errors_and_reading_continues() {
        let text = "@article{a, title={One}}\n@article{A, title={Two}}\n@misc{b, title={unclosed}\n@misc{c, title={Three}}\n@misc{, title={no key}}\n@misc{bad key, title={x}}";
        let (entries, issues) = parse(text);
        let issues = check(&entries, issues, &[]);
        let text: Vec<String> = issues.iter().map(|i| i.message.clone()).collect();
        assert!(text.iter().any(|t| t.contains("used twice (first on line 1)")), "{text:?}");
        assert!(text.iter().any(|t| t.contains("has no citation key")), "{text:?}");
        assert!(text.iter().any(|t| t.contains("\"bad key\" has a space")), "{text:?}");
        assert!(entries.iter().any(|e| e.key == "c") || text.iter().any(|t| t.contains("malformed")), "{text:?}");
    }

    #[test]
    fn hostile_input_neither_panics_nor_runs_away() {
        for text in
            ["@", "@article", "@article{", "@article{k,", "@article{k, t = {", "@article{k, t = \"", "@{}", "@article{k, t = {}}}}}", "@comment{"]
        {
            let _ = check(&parse(text).0, parse(text).1, &cited_keys(text));
        }
        let deep = format!("@article{{k, title = {}x{}}}", "{".repeat(10_000), "}".repeat(10_000));
        let (_, issues) = parse(&deep);
        assert!(!issues.is_empty());
        assert!(cited_keys("\\cite[").is_empty() && cited_keys("\\cite{").is_empty() && cited_keys("\\").is_empty());
    }
}
