//! Turning a TeX `.log` (and Biber's `.blg`) into a list of problems a person can act on.

use serde::Serialize;

/// The most problems kept from one log; a broken document can produce thousands.
pub const MAX_DIAGNOSTICS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Severity {
    Error,
    Warning,
}

/// One problem found in a log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub severity: Severity,
    /// The source file, as TeX names it (`./body.tex`).
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        match (&self.file, self.line) {
            (Some(file), Some(line)) => write!(f, "{file}:{line}: {kind}: {}", self.message),
            (Some(file), None) => write!(f, "{file}: {kind}: {}", self.message),
            _ => write!(f, "{kind}: {}", self.message),
        }
    }
}

impl Diagnostic {
    /// What usually causes this problem and what to do about it, for the common ones.
    pub fn hint(&self) -> Option<&'static str> {
        let m = self.message.as_str();
        let has = |s: &str| m.contains(s);
        Some(if has("Undefined control sequence") {
            "A command is misspelt, or needs a package the journal template doesn't load. Check the command on this line, or remove it."
        } else if has("not set up for use with LaTeX") || has("Unicode character") {
            "pdfLaTeX's fonts have no glyph for this character. Replace it, or write it as a LaTeX command (a Greek letter as $\\alpha$, for example)."
        } else if has("File `") && has("not found") {
            "A file the manuscript uses is not in the project. Add it with \"Bibliography / figures…\" and check the spelling: file names are case-sensitive."
        } else if has("Missing $ inserted") {
            "Mathematics outside a formula, or a stray _ or ^ in text. Put formulas in $…$, and write \\_ for an underscore in text."
        } else if has("Missing \\begin{document}") {
            "Text before \\begin{document}: the body file must hold the article text only. Import the manuscript again so its preamble is removed."
        } else if has("Runaway argument") || has("Too many }'s") || has("Extra }") || has("Missing } inserted") {
            "Unbalanced braces: a { without its } (or the other way round) near this line."
        } else if has("Citation") && has("undefined") {
            "This citation key is not in the bibliography. Add the entry to the .bib file, or correct the key."
        } else if has("Reference") && has("undefined") {
            "No \\label with this name. Check the spelling of the \\ref and the \\label."
        } else if has("There were undefined references") {
            "Some citations or cross-references could not be resolved; the warnings above name them."
        } else if has("Overfull \\hbox") {
            "A line sticks out into the margin. Rephrase it, allow a hyphenation with \\-, or break a long URL or formula."
        } else if has("Rerun to get") || has("Label(s) may have changed") || has("Please rerun") {
            "latexmk normally reruns until this goes away. If it stays, turn on \"Rebuild everything each time\" and compile again."
        } else if self.file.as_deref() == Some("biber") && (has("Cannot find") || has("datasource")) {
            "Biber could not read the bibliography file. Check that the .bib file named in main.tex is in the project."
        } else if self.file.as_deref() == Some("biber") && has("BibTeX subsystem") {
            "The .bib file has a syntax error near the entry named here: a missing comma, brace or quote."
        } else if has("Emergency stop") || has("job aborted") || has("Fatal error occurred") {
            "TeX stopped at a fatal error. Fix the first error in the list; the later ones often follow from it."
        } else {
            return None;
        })
    }
}

/// Source extensions a `file:line: message` line may name (`-file-line-error` output).
const SOURCE_EXTENSIONS: &[&str] = &[".tex", ".sty", ".cls", ".bbl", ".def", ".cfg", ".clo", ".ltx", ".aux"];

/// Parse a `-file-line-error` style line: `./body.tex:12: Undefined control sequence.`
fn file_line_error(line: &str) -> Option<(String, u32, String)> {
    for ext in SOURCE_EXTENSIONS {
        let mut search_from = 0;
        while let Some(found) = line.get(search_from..).and_then(|rest| rest.find(ext)) {
            let ext_end = search_from + found + ext.len();
            let after = line.get(ext_end..)?;
            if let Some(rest) = after.strip_prefix(':') {
                let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                if !digits.is_empty()
                    && let Some(message) = rest.get(digits.len()..).and_then(|m| m.strip_prefix(": "))
                {
                    let file = line.get(..ext_end)?.trim().to_owned();
                    let n = digits.parse().ok()?;
                    return Some((file, n, message.trim().to_owned()));
                }
            }
            search_from = ext_end;
        }
    }
    None
}

/// The input line number TeX reports after an error (`l.12 \foo`).
fn context_line(line: &str) -> Option<u32> {
    let rest = line.strip_prefix("l.")?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// Extract errors and warnings from a pdfTeX log.
pub fn parse_tex_log(log: &str) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = Vec::new();
    let lines: Vec<&str> = log.lines().collect();
    let mut i = 0;
    while let Some(line) = lines.get(i) {
        if out.len() >= MAX_DIAGNOSTICS {
            break;
        }
        if let Some((file, n, message)) = file_line_error(line) {
            push_unique(&mut out, Diagnostic { severity: Severity::Error, file: Some(file), line: Some(n), message });
        } else if let Some(message) = line.strip_prefix("! ") {
            // Without -file-line-error: the line number follows a few lines later as `l.<n>`.
            let n = lines.iter().skip(i + 1).take(12).find_map(|l| context_line(l));
            push_unique(&mut out, Diagnostic { severity: Severity::Error, file: None, line: n, message: message.trim().to_owned() });
        } else if let Some(warning) = warning_text(line) {
            // Warnings continue on following lines indented to line up with the text.
            let mut message = warning.to_owned();
            let mut j = i + 1;
            while let Some(next) = lines.get(j) {
                // `(biblatex)    more text` or `    more text`; `(./file.tex` opens a file instead.
                let package_cont = next.starts_with('(') && next.contains(")  ");
                if next.trim().is_empty() || !(next.starts_with(' ') || package_cont) {
                    break;
                }
                let next: &str = next;
                let cont = if package_cont { next.split_once(')').map_or(next, |(_, rest)| rest) } else { next };
                message.push(' ');
                message.push_str(cont.trim());
                j += 1;
            }
            let line_no = ["on input line ", "at lines ", "at line "].iter().find_map(|marker| {
                let rest = message.get(message.rfind(marker)? + marker.len()..)?;
                let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
                digits.parse().ok()
            });
            push_unique(&mut out, Diagnostic { severity: Severity::Warning, file: None, line: line_no, message });
            i = j;
            continue;
        }
        i += 1;
    }
    out
}

fn warning_text(line: &str) -> Option<&str> {
    // Overfull boxes are text in the margin, which a typesetter fixes; underfull ones are only
    // loose spacing and are left out.
    if line.starts_with("LaTeX Warning:") || line.starts_with("pdfTeX warning") || line.starts_with("Overfull \\hbox") {
        return Some(line);
    }
    if line.starts_with("Package ") && line.contains(" Warning:") {
        return Some(line);
    }
    if line.starts_with("Class ") && line.contains(" Warning:") {
        return Some(line);
    }
    None
}

/// Extract errors and warnings from Biber's `.blg` log (`ERROR - …`, `WARN - …`).
pub fn parse_biber_log(log: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for line in log.lines() {
        if out.len() >= MAX_DIAGNOSTICS {
            break;
        }
        let (severity, message) = if let Some(at) = line.find("ERROR - ") {
            (Severity::Error, line.get(at + 8..))
        } else if let Some(at) = line.find("WARN - ") {
            (Severity::Warning, line.get(at + 7..))
        } else {
            continue;
        };
        if let Some(message) = message {
            push_unique(&mut out, Diagnostic { severity, file: Some("biber".to_owned()), line: None, message: message.trim().to_owned() });
        }
    }
    out
}

fn push_unique(out: &mut Vec<Diagnostic>, d: Diagnostic) {
    if !out.contains(&d) {
        out.push(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_line_errors_are_read() {
        let log = "(./main.tex\n./body.tex:12: Undefined control sequence.\nl.12 \\foo\n";
        let d = parse_tex_log(log);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, Severity::Error);
        assert_eq!(d[0].file.as_deref(), Some("./body.tex"));
        assert_eq!(d[0].line, Some(12));
        assert_eq!(d[0].message, "Undefined control sequence.");
    }

    #[test]
    fn classic_errors_take_the_line_from_the_context() {
        let log = "! Missing $ inserted.\n<inserted text> \n                $\nl.7 a_b\n";
        let d = parse_tex_log(log);
        assert_eq!(d, vec![Diagnostic { severity: Severity::Error, file: None, line: Some(7), message: "Missing $ inserted.".into() }]);
    }

    #[test]
    fn warnings_join_their_continuation_lines() {
        let log = "LaTeX Warning: Citation `x' on page 1 undefined on input line 9.\n\nPackage biblatex Warning: Please (re)run Biber on the file:\n(biblatex)                main\n(biblatex)                and rerun LaTeX afterwards.\n";
        let d = parse_tex_log(log);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].line, Some(9));
        assert!(d[1].message.starts_with("Package biblatex Warning"));
    }

    #[test]
    fn overfull_boxes_are_warnings_with_their_line() {
        let log = "Overfull \\hbox (12.3pt too wide) in paragraph at lines 12--14\n[]\\T1/lmr/m/n/10 averyveryverylongword\n\nUnderfull \\hbox (badness 10000) in paragraph at lines 20--21\n";
        let d = parse_tex_log(log);
        assert_eq!(d.len(), 1, "{d:?}");
        assert_eq!(d[0].severity, Severity::Warning);
        assert_eq!(d[0].line, Some(12));
        assert!(d[0].hint().unwrap().contains("margin"));
    }

    #[test]
    fn common_problems_come_with_a_hint() {
        let error = |m: &str| Diagnostic { severity: Severity::Error, file: None, line: None, message: m.to_owned() };
        for (message, hint) in [
            ("Undefined control sequence.", "misspelt"),
            ("Package inputenc Error: Unicode character α (U+03B1)", "glyph"),
            ("LaTeX Error: File `figure1.png' not found.", "case-sensitive"),
            ("Missing $ inserted.", "underscore"),
            ("LaTeX Warning: Citation 'smith2020' on page 1 undefined on input line 4.", "bibliography"),
            ("LaTeX Warning: Reference `fig:a' on page 2 undefined on input line 9.", "\\label"),
            ("Runaway argument?", "braces"),
            ("Emergency stop.", "first error"),
        ] {
            assert!(error(message).hint().is_some_and(|h| h.contains(hint)), "{message}");
        }
        assert_eq!(error("Something nobody has seen before").hint(), None);
        let biber = Diagnostic { severity: Severity::Error, file: Some("biber".into()), line: None, message: "Cannot find 'references.bib'!".into() };
        assert!(biber.hint().unwrap().contains(".bib"));
    }

    #[test]
    fn odd_lines_never_panic() {
        for log in ["", ":", ".tex:", ".tex:12", ".tex:12:", "x.tex:99999999999999999999: overflow", "l.", "! ", "Package  Warning:"] {
            let _ = parse_tex_log(log);
            let _ = parse_biber_log(log);
        }
    }

    #[test]
    fn biber_errors_are_read() {
        let d =
            parse_biber_log("INFO - This is Biber\nERROR - BibTeX subsystem: references.bib, line 3, syntax error\nWARN - Duplicate entry key 'a'\n");
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].severity, Severity::Error);
        assert_eq!(d[1].severity, Severity::Warning);
    }
}
