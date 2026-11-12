//! Formal publication metadata of an accepted article, and the `psg-metadata.tex` written from it.
//!
//! Every value is typed by a person and goes into a TeX file, so every value is escaped: a title
//! containing `\input{/etc/passwd}` or `\write18` is typeset as those characters, never run.

use serde::{Deserialize, Serialize};

/// The longest a single metadata field may be (characters). An abstract is the longest field.
pub const MAX_FIELD: usize = 20_000;
/// The most authors, affiliations and keywords kept.
pub const MAX_LIST: usize = 200;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Author {
    pub name: String,
    /// 1-based indices into [`PublicationMetadata::affiliations`].
    pub affiliations: Vec<usize>,
    pub orcid: String,
    pub email: String,
    pub corresponding: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PublicationMetadata {
    pub journal: String,
    pub journal_abbrev: String,
    pub issn: String,
    pub volume: String,
    pub issue: String,
    pub year: String,
    /// Page range (`101--115`) or empty.
    pub pages: String,
    pub article_number: String,
    pub doi: String,
    pub article_type: String,
    pub title: String,
    pub short_title: String,
    pub authors: Vec<Author>,
    pub affiliations: Vec<String>,
    pub abstract_text: String,
    pub keywords: Vec<String>,
    /// ISO 8601 calendar dates (`2026-03-02`) or empty.
    pub received: String,
    pub revised: String,
    pub accepted: String,
    pub published: String,
    pub licence: String,
    pub copyright: String,
}

impl Default for PublicationMetadata {
    fn default() -> Self {
        Self {
            journal: String::new(),
            journal_abbrev: String::new(),
            issn: String::new(),
            volume: String::new(),
            issue: String::new(),
            year: String::new(),
            pages: String::new(),
            article_number: String::new(),
            doi: String::new(),
            article_type: "Research Article".to_owned(),
            title: String::new(),
            short_title: String::new(),
            authors: Vec::new(),
            affiliations: Vec::new(),
            abstract_text: String::new(),
            keywords: Vec::new(),
            received: String::new(),
            revised: String::new(),
            accepted: String::new(),
            published: String::new(),
            licence: "This article is distributed under the terms of the Creative Commons Attribution 4.0 International licence (CC BY 4.0)."
                .to_owned(),
            copyright: String::new(),
        }
    }
}

/// A metadata problem worth telling the person about before publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataIssue {
    pub field: &'static str,
    pub message: String,
    /// `true` blocks compilation; `false` is advice.
    pub blocking: bool,
}

impl PublicationMetadata {
    /// The sample values of the bundled template, a useful starting point for a new project.
    pub fn sample() -> Self {
        Self {
            journal: "PSG Journal of Example Studies".into(),
            journal_abbrev: "PSG J. Ex. Stud.".into(),
            issn: "0000-0000".into(),
            volume: "1".into(),
            issue: "1".into(),
            year: "2026".into(),
            pages: "1--4".into(),
            doi: "10.0000/psg.example.0001".into(),
            title: "A Sample Article Typeset with the PSG Research Article Template".into(),
            short_title: "A Sample PSG Article".into(),
            authors: vec![
                Author { name: "Alex Example".into(), affiliations: vec![1], email: "alex@example.org".into(), corresponding: true, ..Author::default() },
                Author { name: "Robin Sample".into(), affiliations: vec![2], ..Author::default() },
            ],
            affiliations: vec!["Department of Examples, Example University".into(), "Institute of Samples".into()],
            abstract_text: "This abstract shows where the abstract appears. The article demonstrates the title block, the running headers, mathematics, a table and a bibliography processed by Biber.".into(),
            keywords: vec!["template".into(), "typesetting".into(), "pdfLaTeX".into()],
            received: "2026-01-10".into(),
            accepted: "2026-03-02".into(),
            published: "2026-04-15".into(),
            copyright: "© 2026 The Authors".into(),
            ..Self::default()
        }
    }

    /// Check the metadata. Blocking issues make the generated file unusable; the others are
    /// things a production editor would want to fix before publication.
    pub fn validate(&self) -> Vec<MetadataIssue> {
        let mut issues = Vec::new();
        let mut issue = |field: &'static str, message: String, blocking: bool| issues.push(MetadataIssue { field, message, blocking });
        if self.title.trim().is_empty() {
            issue("title", "The article needs a title.".into(), true);
        }
        if self.journal.trim().is_empty() {
            issue("journal", "The journal name is empty.".into(), false);
        }
        if self.authors.iter().all(|a| a.name.trim().is_empty()) {
            issue("authors", "The article has no authors.".into(), false);
        }
        for (field, value) in self.text_fields() {
            if value.chars().count() > MAX_FIELD {
                issue(field, format!("Longer than {MAX_FIELD} characters."), true);
            }
        }
        for list in [self.authors.len(), self.affiliations.len(), self.keywords.len()] {
            if list > MAX_LIST {
                issue("lists", format!("More than {MAX_LIST} authors, affiliations or keywords."), true);
            }
        }
        if !self.doi.trim().is_empty() && !is_doi(self.doi.trim()) {
            issue("doi", "A DOI starts with \"10.\", then a registrant code, a slash and a suffix (10.1234/abc).".into(), false);
        }
        if !self.issn.trim().is_empty() && !is_issn(self.issn.trim()) {
            issue("issn", "An ISSN is eight characters written 1234-567X, with a valid check digit.".into(), false);
        }
        for (field, value) in [("received", &self.received), ("revised", &self.revised), ("accepted", &self.accepted), ("published", &self.published)]
        {
            if !value.trim().is_empty() && chrono::NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d").is_err() {
                issue(field, format!("\"{}\" is not a date written YYYY-MM-DD.", value.trim()), false);
            }
        }
        for (i, author) in self.authors.iter().enumerate() {
            for &a in &author.affiliations {
                if a == 0 || a > self.affiliations.len() {
                    issue("authors", format!("Author {} names affiliation {a}, which does not exist.", i + 1), false);
                }
            }
            if !author.orcid.trim().is_empty() && !is_orcid(author.orcid.trim()) {
                issue("authors", format!("Author {}'s ORCID iD should look like 0000-0002-1825-0097.", i + 1), false);
            }
        }
        issues
    }

    fn text_fields(&self) -> Vec<(&'static str, &str)> {
        vec![
            ("journal", &self.journal),
            ("journal_abbrev", &self.journal_abbrev),
            ("title", &self.title),
            ("short_title", &self.short_title),
            ("abstract", &self.abstract_text),
            ("licence", &self.licence),
            ("copyright", &self.copyright),
            ("doi", &self.doi),
            ("pages", &self.pages),
        ]
    }

    /// The `psg-metadata.tex` file for the templates' `\psg…` setters. Every value is escaped.
    pub fn to_latex(&self) -> String {
        let mut out = String::from("% Publication metadata written by PSG Academic PDF. Edit it in the app: changes here are overwritten.\n");
        let mut set = |macro_name: &str, value: String| {
            out.push('\\');
            out.push_str(macro_name);
            out.push('{');
            out.push_str(&value);
            out.push_str("}\n");
        };
        set("psgJournal", escape(&self.journal));
        set("psgJournalAbbrev", escape(&self.journal_abbrev));
        set("psgISSN", escape(&self.issn));
        set("psgVolume", escape(&self.volume));
        set("psgIssue", escape(&self.issue));
        set("psgYear", escape(&self.year));
        set("psgPages", escape_pages(&self.pages));
        set("psgArticleNumber", escape(&self.article_number));
        set("psgDOI", escape_url_part(self.doi.trim()));
        set("psgArticleType", escape(&self.article_type));
        set("psgTitle", escape(&self.title));
        set("psgShortTitle", escape(&self.short_title));
        set("psgAuthors", self.author_line());
        set("psgAffiliations", self.affiliation_block());
        set("psgCorrespondence", self.correspondence());
        set("psgAbstract", escape_paragraphs(&self.abstract_text));
        set("psgKeywords", self.keywords.iter().take(MAX_LIST).map(|k| escape(k)).filter(|k| !k.is_empty()).collect::<Vec<_>>().join("; "));
        set("psgReceived", escape(&self.received));
        set("psgRevised", escape(&self.revised));
        set("psgAccepted", escape(&self.accepted));
        set("psgPublished", escape(&self.published));
        set("psgLicence", escape(&self.licence));
        set("psgCopyright", escape(&self.copyright));
        // PDF document information: plain text, no markup.
        set("psgPdfTitle", escape(&self.title));
        set("psgPdfAuthor", self.named_authors().map(escape).collect::<Vec<_>>().join(", "));
        set("psgPdfKeywords", self.keywords.iter().take(MAX_LIST).map(|k| escape(k)).filter(|k| !k.is_empty()).collect::<Vec<_>>().join(", "));
        out
    }

    fn named_authors(&self) -> impl Iterator<Item = &str> {
        self.authors.iter().take(MAX_LIST).map(|a| a.name.trim()).filter(|n| !n.is_empty())
    }

    fn author_line(&self) -> String {
        let authors: Vec<String> = self
            .authors
            .iter()
            .take(MAX_LIST)
            .filter(|a| !a.name.trim().is_empty())
            .map(|a| {
                let mut marks: Vec<String> =
                    a.affiliations.iter().filter(|&&i| i >= 1 && i <= self.affiliations.len()).map(usize::to_string).collect();
                if a.corresponding {
                    marks.push("*".into());
                }
                let mut s = escape(a.name.trim());
                if !marks.is_empty() {
                    s.push_str(&format!("\\psgAffMark{{{}}}", escape(&marks.join(","))));
                }
                s
            })
            .collect();
        match authors.len() {
            0 => String::new(),
            1 => authors.concat(),
            n => {
                let (head, last) = authors.split_at(n - 1);
                format!("{} and {}", head.join(", "), last.concat())
            }
        }
    }

    fn affiliation_block(&self) -> String {
        self.affiliations
            .iter()
            .take(MAX_LIST)
            .enumerate()
            .map(|(i, a)| format!("\\psgAffMark{{{}}}{}", i + 1, escape(a)))
            .collect::<Vec<_>>()
            .join("\\par ")
    }

    fn correspondence(&self) -> String {
        let emails: Vec<String> =
            self.authors.iter().filter(|a| a.corresponding && !a.email.trim().is_empty()).map(|a| escape(a.email.trim())).collect();
        if emails.is_empty() { String::new() } else { format!("\\psgAffMark{{*}}Corresponding author: {}", emails.join(", ")) }
    }
}

/// `10.<registrant>/<suffix>`: what Crossref and DataCite register.
pub fn is_doi(doi: &str) -> bool {
    let Some(rest) = doi.strip_prefix("10.") else { return false };
    let Some((registrant, suffix)) = rest.split_once('/') else { return false };
    !registrant.is_empty()
        && registrant.chars().all(|c| c.is_ascii_digit() || c == '.')
        && !suffix.trim().is_empty()
        && !doi.chars().any(char::is_whitespace)
}

/// `1234-567X` with the ISSN check digit (ISO 3297).
pub fn is_issn(issn: &str) -> bool {
    let chars: Vec<char> = issn.chars().collect();
    if chars.len() != 9 || chars.get(4) != Some(&'-') {
        return false;
    }
    let digits: Vec<char> = chars.iter().copied().filter(|&c| c != '-').collect();
    let mut sum = 0u32;
    for (i, c) in digits.iter().take(7).enumerate() {
        let Some(d) = c.to_digit(10) else { return false };
        sum += d * (8 - i as u32);
    }
    let check = (11 - sum % 11) % 11;
    match digits.get(7) {
        Some('X' | 'x') => check == 10,
        Some(c) => c.to_digit(10) == Some(check),
        None => false,
    }
}

/// `0000-0002-1825-0097` with the ISO 7064 11,2 check character ORCID uses.
pub fn is_orcid(orcid: &str) -> bool {
    let id = orcid.strip_prefix("https://orcid.org/").unwrap_or(orcid);
    let chars: Vec<char> = id.chars().collect();
    if chars.len() != 19 || [4, 9, 14].iter().any(|&i| chars.get(i) != Some(&'-')) {
        return false;
    }
    let digits: Vec<char> = chars.into_iter().filter(|&c| c != '-').collect();
    let mut total = 0u32;
    for c in digits.iter().take(15) {
        let Some(d) = c.to_digit(10) else { return false };
        total = (total + d) * 2;
    }
    let check = (12 - total % 11) % 11;
    match digits.get(15) {
        Some('X') => check == 10,
        Some(c) => c.to_digit(10) == Some(check),
        None => false,
    }
}

/// Escape text for LaTeX: every character TeX treats specially becomes a command that prints
/// it, and control characters are dropped. Paragraph breaks are not kept (see [`escape_paragraphs`]).
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut last_space = false;
    for c in text.chars().take(MAX_FIELD) {
        let piece: &str = match c {
            '\\' => "\\textbackslash{}",
            '{' => "\\{",
            '}' => "\\}",
            '$' => "\\$",
            '&' => "\\&",
            '#' => "\\#",
            '%' => "\\%",
            '_' => "\\_",
            '~' => "\\textasciitilde{}",
            '^' => "\\textasciicircum{}",
            '<' => "\\textless{}",
            '>' => "\\textgreater{}",
            '|' => "\\textbar{}",
            '"' => "\\textquotedbl{}",
            '©' => "\\textcopyright{}",
            c if c.is_whitespace() => {
                if !last_space {
                    out.push(' ');
                }
                last_space = true;
                continue;
            }
            c if c.is_control() => continue,
            c => {
                out.push(c);
                last_space = false;
                continue;
            }
        };
        out.push_str(piece);
        last_space = false;
    }
    out.trim().to_owned()
}

/// [`escape`], keeping blank-line paragraph breaks as `\par`.
pub fn escape_paragraphs(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n");
    normalized.split("\n\n").map(escape).filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\\par ")
}

/// A page range: digits and letters as typed, with a hyphen or en dash between them written as `--`.
fn escape_pages(pages: &str) -> String {
    let trimmed = pages.trim().replace('–', "-");
    match trimmed.split_once('-') {
        Some((a, b)) => format!("{}--{}", escape(a.trim_end_matches('-')), escape(b.trim_start_matches('-'))),
        None => escape(&trimmed),
    }
}

/// A DOI goes into `\href{https://doi.org/…}`, where `#`, `%`, `{`, `}` and `\` would break the
/// URL or the TeX. A valid DOI suffix may contain `#` and `%`; they are percent-encoded, the
/// rest escaped as text.
fn escape_url_part(doi: &str) -> String {
    let mut out = String::new();
    for c in doi.chars().take(MAX_FIELD) {
        match c {
            '%' => out.push_str("\\%25"),
            '#' => out.push_str("\\%23"),
            '\\' | '{' | '}' | '^' | '~' | '$' | '&' => out.push_str(&format!("\\%{:02X}", c as u32)),
            '_' => out.push_str("\\_"),
            c if c.is_whitespace() || c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_neutralises_tex_commands() {
        let e = escape("\\input{/etc/passwd} \\write18{rm -rf ~} 50% & $x$ #1 a_b ^ |");
        assert!(!e.contains("\\input"), "{e}");
        assert!(!e.contains("\\write18"), "{e}");
        assert!(e.contains("\\textbackslash{}input\\{/etc/passwd\\}"));
        assert!(e.contains("50\\%"));
        assert!(e.contains("\\&"));
        assert!(e.contains("\\$x\\$"));
        assert!(e.contains("\\#1"));
        assert!(e.contains("a\\_b"));
        assert!(e.contains("\\textasciicircum{}"));
    }

    #[test]
    fn escape_drops_control_characters_and_collapses_space() {
        assert_eq!(escape("  a\u{0}\u{7}  b\n\tc  "), "a b c");
    }

    #[test]
    fn every_generated_line_has_balanced_braces() {
        let mut m = PublicationMetadata::sample();
        m.title = "}{\\end{document}".into();
        m.abstract_text = "first {\n\nsecond }".into();
        m.doi = "10.1000/a#b%c{d}".into();
        for line in m.to_latex().lines().skip(1) {
            let mut depth = 0i32;
            let mut chars = line.chars();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => {
                        chars.next();
                    }
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
                assert!(depth >= 0, "unbalanced: {line}");
            }
            assert_eq!(depth, 0, "unbalanced: {line}");
        }
    }

    #[test]
    fn authors_and_affiliations_are_numbered() {
        let latex = PublicationMetadata::sample().to_latex();
        assert!(latex.contains("\\psgAuthors{Alex Example\\psgAffMark{1,*} and Robin Sample\\psgAffMark{2}}"), "{latex}");
        assert!(
            latex.contains("\\psgAffiliations{\\psgAffMark{1}Department of Examples, Example University\\par \\psgAffMark{2}Institute of Samples}")
        );
        assert!(latex.contains("\\psgCorrespondence{\\psgAffMark{*}Corresponding author: alex@example.org}"));
        assert!(latex.contains("\\psgPages{1--4}"));
        assert!(latex.contains("\\psgCopyright{\\textcopyright{} 2026 The Authors}"));
    }

    #[test]
    fn validation() {
        assert!(PublicationMetadata::sample().validate().iter().all(|i| !i.blocking));
        let mut m = PublicationMetadata::sample();
        m.title.clear();
        m.doi = "doi:10.1/x".into();
        m.accepted = "2026-02-30".into();
        m.authors[0].affiliations = vec![9];
        let fields: Vec<&str> = m.validate().iter().map(|i| i.field).collect();
        assert!(fields.contains(&"title") && fields.contains(&"doi") && fields.contains(&"accepted") && fields.contains(&"authors"), "{fields:?}");
        assert!(m.validate().iter().any(|i| i.blocking));
    }

    #[test]
    fn identifiers() {
        assert!(is_doi("10.1000/xyz123"));
        assert!(is_doi("10.1000.10/abc"));
        assert!(!is_doi("10./x"));
        assert!(!is_doi("11.1000/x"));
        assert!(is_issn("0317-8471"));
        assert!(is_issn("2049-3630"));
        assert!(!is_issn("0317-8472"));
        assert!(is_orcid("0000-0002-1825-0097"));
        assert!(is_orcid("https://orcid.org/0000-0001-5109-3700"));
        assert!(!is_orcid("0000-0002-1825-0098"));
    }

    #[test]
    fn metadata_round_trips_through_toml() {
        let m = PublicationMetadata::sample();
        let text = toml::to_string(&m).unwrap();
        let back: PublicationMetadata = toml::from_str(&text).unwrap();
        assert_eq!(back, m);
    }
}
