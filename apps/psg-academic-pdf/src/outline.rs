//! The article's structure for the Manuscript area: the sectioning commands of the body
//! (`\section`, `\subsection`, `\subsubsection`, starred or not), with their lines, so a click
//! goes to the heading. Commented-out lines are skipped.

/// One heading of the manuscript.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    /// 1 for `\section`, 2 for `\subsection`, 3 for `\subsubsection`.
    pub level: u8,
    pub title: String,
    /// 1-based line number.
    pub line: u32,
}

/// The most headings listed; a body with more is not an article's outline.
pub const MAX_HEADINGS: usize = 500;

/// The headings of `body`, in order.
pub fn headings(body: &str) -> Vec<Heading> {
    let mut out = Vec::new();
    for (n, raw) in body.lines().enumerate() {
        if out.len() >= MAX_HEADINGS {
            break;
        }
        let line = strip_comment(raw).trim_start();
        let Some(rest) = line.strip_prefix('\\') else { continue };
        let (level, rest) = if let Some(r) = rest.strip_prefix("subsubsection") {
            (3, r)
        } else if let Some(r) = rest.strip_prefix("subsection") {
            (2, r)
        } else if let Some(r) = rest.strip_prefix("section") {
            (1, r)
        } else {
            continue;
        };
        let rest = rest.strip_prefix('*').unwrap_or(rest).trim_start();
        // An optional short title in brackets comes before the title itself.
        let rest = if rest.starts_with('[') { rest.find(']').and_then(|i| rest.get(i + 1..)).unwrap_or("").trim_start() } else { rest };
        let Some(title) = braced(rest) else { continue };
        let title = tidy(&title);
        if title.is_empty() {
            continue;
        }
        let Ok(line) = u32::try_from(n + 1) else { break };
        out.push(Heading { level, title, line });
    }
    out
}

/// `text` up to an unescaped `%`.
fn strip_comment(text: &str) -> &str {
    let mut prev = '\0';
    for (i, c) in text.char_indices() {
        if c == '%' && prev != '\\' {
            return text.get(..i).unwrap_or(text);
        }
        prev = c;
    }
    text
}

/// The contents of the brace group `text` starts with (nested braces kept), if it closes on
/// this line.
fn braced(text: &str) -> Option<String> {
    let inner = text.strip_prefix('{')?;
    let mut depth = 1usize;
    for (i, c) in inner.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return inner.get(..i).map(str::to_owned);
                }
            }
            _ => {}
        }
    }
    None
}

/// A heading as people read it: common markup dropped, spaces collapsed.
fn tidy(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut chars = title.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                // Drop a command's name (\emph, \textit); keep an escaped character (\&).
                if chars.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                    while chars.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
                        chars.next();
                    }
                } else if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            '{' | '}' => {}
            '~' => out.push(' '),
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_with_levels_lines_and_markup() {
        let body = "Intro text\n\\section{Introduction}\n% \\section{Old}\n  \\subsection*{Data \\& \\emph{Methods}}\n\\subsubsection[Short]{A long~title}\n\\section{}\n\\section{Unclosed\n\\section{第二节 结果}";
        let h = headings(body);
        let got: Vec<(u8, &str, u32)> = h.iter().map(|h| (h.level, h.title.as_str(), h.line)).collect();
        assert_eq!(got, vec![(1, "Introduction", 2), (2, "Data & Methods", 4), (3, "A long title", 5), (1, "第二节 结果", 8)]);
    }

    #[test]
    fn escaped_percent_is_not_a_comment() {
        assert_eq!(headings("\\section{50\\% done} % note").first().map(|h| h.title.clone()), Some("50% done".into()));
    }

    #[test]
    fn hostile_input_is_bounded() {
        let body = "\\section{x}\n".repeat(MAX_HEADINGS + 50);
        assert_eq!(headings(&body).len(), MAX_HEADINGS);
        assert!(headings(&"{".repeat(100_000)).is_empty());
    }
}
