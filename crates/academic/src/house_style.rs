//! How a template sets its tables and figures, learnt from the sample article it comes with, and
//! put on the tables and figures of a manuscript converted from Word.
//!
//! A journal's template often styles tables in the article, not in its class: a font switch and
//! `\captionsetup` before each table, shaded columns in its column specification, a note in a
//! smaller face after it, and long tables on a page of their own. The sample's first table shows
//! the journal's way; Pandoc's tables from Word are plain. This learns the first and applies it to
//! the second, so the manuscript's tables look like the template's.

use serde::{Deserialize, Serialize};

use crate::template_import::{group_end, mask_comments};

/// Where a project keeps its learnt style: written from the template's sample body before the
/// first manuscript replaces it, and read by every later import.
pub const FILE: &str = "psg-house-style.toml";

/// Table text longer than this (bytes) goes on the template's own table page, when it has one:
/// the sample's short tables sit in the text, its long ones on a page of their own.
const LONG_TABLE: usize = 2500;
/// The most tables or figures restyled.
const MAX_ITEMS: usize = 500;

/// Column alignment commands: the template's own replaces Pandoc's `\raggedright`; the rest of a
/// column prefix (a colour, a font) is put on every column.
const ALIGNMENT: &[&str] = &["\\RaggedRight", "\\raggedright", "\\Centering", "\\centering", "\\RaggedLeft", "\\raggedleft", "\\arraybackslash"];

/// Environments around a sample table that are no page of their own.
const NOT_A_PAGE: &[&str] =
    &["table", "table*", "center", "minipage", "small", "footnotesize", "landscape", "sidewaystable", "threeparttable", "lrbox"];

/// What the template's sample does with its tables and figures.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HouseStyle {
    /// The settings before the sample's table (`\captionsetup`, the font, rule spacing …), one per
    /// line.
    pub table_setup: String,
    /// What each column of the sample's table gets besides its alignment (`\columncolor{…}`).
    pub column_extra: String,
    /// The sample's column alignment for text (`\RaggedRight`), or empty.
    pub column_align: String,
    /// The environment the sample puts a long table in (a landscape page of its own), or empty.
    pub table_page: String,
    /// The `\captionsetup{…}` before the sample's figure caption, or empty.
    pub figure_setup: String,
    /// How the sample opens a note under a table (`{\footnotesize\rmfamily \emph{Note.}`) and
    /// closes it (`}`); empty when it has none.
    pub note_open: String,
    pub note_close: String,
}

impl HouseStyle {
    pub fn is_empty(&self) -> bool {
        *self == HouseStyle::default()
    }
}

/// `\begin{name}` at or after `from`: where it starts, the name, and where the command ends.
fn next_begin(masked: &str, from: usize) -> Option<(usize, String, usize)> {
    let at = from.checked_add(masked.get(from..)?.find("\\begin{")?)?;
    let open = at + "\\begin".len();
    let close = group_end(masked, open)?;
    let name = masked.get(open + 1..close - 1)?.trim().to_owned();
    Some((at, name, close))
}

/// The `\end{name}` that closes the `\begin{name}` ending at `from`, counting nested ones.
fn matching_end(masked: &str, name: &str, from: usize) -> Option<(usize, usize)> {
    let (begin, end) = (format!("\\begin{{{name}}}"), format!("\\end{{{name}}}"));
    let mut depth = 1usize;
    let mut at = from;
    loop {
        let rest = masked.get(at..)?;
        let next_end = rest.find(&end)?;
        match rest.find(&begin) {
            Some(b) if b < next_end => {
                depth = depth.saturating_add(1);
                at = at + b + begin.len();
            }
            _ => {
                depth -= 1;
                let start = at + next_end;
                if depth == 0 {
                    return Some((start, start + end.len()));
                }
                at = start + end.len();
            }
        }
    }
}

/// The environments around `at`, innermost first: (name, start of `\begin`, end of the `\begin`
/// command and its arguments, start of `\end`).
fn around(masked: &str, at: usize) -> Vec<(String, usize, usize, usize)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some((start, name, after)) = next_begin(masked, from) {
        if start >= at {
            break;
        }
        if name != "document"
            && let Some((end, _)) = matching_end(masked, &name, after)
            && end > at
        {
            // Past the environment's own arguments ([H], {\textwidth} …).
            let mut args = after;
            while let Some(b) = masked.as_bytes().get(args).filter(|b| matches!(b, b'[' | b'{')) {
                let _ = b;
                match group_end(masked, args) {
                    Some(e) => args = e,
                    None => break,
                }
            }
            out.push((name, start, args, end));
        }
        from = after;
    }
    out.reverse();
    out
}

/// Learn the style from a template's sample article body. `None` when it has no table or figure
/// to learn from.
pub fn learn(sample: &str) -> Option<HouseStyle> {
    let masked = mask_comments(sample);
    let mut style = HouseStyle::default();
    let table = ["\\begin{longtable}", "\\begin{tabular}"].iter().filter_map(|t| masked.find(t)).min();
    if let Some(table) = table {
        let envs = around(&masked, table);
        let inner = envs.first();
        // The settings between the innermost environment's start and the table.
        if let Some((_, _, args, _)) = inner
            && let Some(text) = sample.get(*args..table)
        {
            let lines: Vec<&str> = text
                .lines()
                .map(str::trim)
                .filter(|l| l.starts_with('\\'))
                .filter(|l| {
                    !["\\caption{", "\\caption[", "\\captionof", "\\label", "\\centering", "\\begin", "\\end", "\\par", "\\vspace", "\\noindent"]
                        .iter()
                        .any(|c| l.starts_with(c))
                })
                .collect();
            style.table_setup = lines.join("\n");
        }
        if let Some((name, ..)) = envs.iter().find(|(name, ..)| !NOT_A_PAGE.contains(&name.as_str())) {
            style.table_page = name.clone();
        }
        // The column specification's first `>{…}`.
        let env_end = masked.get(table..).and_then(|r| r.find('}')).map(|e| table + e + 1);
        let mut spec_at = env_end.map(|e| skip_space(&masked, e));
        if let Some(at) = spec_at
            && masked.as_bytes().get(at) == Some(&b'[')
        {
            spec_at = group_end(&masked, at).map(|e| skip_space(&masked, e));
        }
        if let Some(at) = spec_at
            && masked.as_bytes().get(at) == Some(&b'{')
            && let Some(spec_end) = group_end(&masked, at)
            && let Some(gt) = masked.get(at..spec_end).and_then(|s| s.find(">{")).map(|g| at + g + 1)
            && let Some(gt_end) = group_end(&masked, gt)
            && let Some(prefix) = sample.get(gt + 1..gt_end - 1)
        {
            let mut extra = prefix.to_owned();
            for a in ALIGNMENT {
                if style.column_align.is_empty() && *a != "\\arraybackslash" && prefix.contains(a) {
                    style.column_align = (*a).to_owned();
                }
                extra = extra.replace(a, "");
            }
            style.column_extra = extra.trim().to_owned();
        }
        // The note after the table, inside the same environment.
        let table_name = if masked.get(table..).is_some_and(|r| r.starts_with("\\begin{longtable}")) { "longtable" } else { "tabular" };
        let after_table = matching_end(&masked, table_name, table + table_name.len() + 8).map(|(_, e)| e);
        let limit = inner.map(|(_, _, _, end)| *end).unwrap_or(masked.len());
        if let Some(after) = after_table
            && let Some(region) = sample.get(after..limit.max(after))
            && let Some((open, close)) = note_format(region)
        {
            style.note_open = open;
            style.note_close = close;
        }
    }
    // The figure caption's settings: the \captionsetup nearest before the first figure caption.
    let in_figure = masked.find("\\begin{figure}").and_then(|f| masked.get(f..)?.find("\\caption{").map(|c| f + c));
    let caption = [masked.find("\\captionof{figure}"), in_figure].into_iter().flatten().min();
    if let Some(caption) = caption
        && let Some(setup) = masked.get(..caption).and_then(|before| before.rfind("\\captionsetup"))
        && let Some(open) = masked.get(setup..).and_then(|r| r.find('{')).map(|o| setup + o)
        && let Some(end) = group_end(&masked, open)
        // Only the settings that belong to that figure: no table or figure between them.
        && !masked.get(end..caption).is_some_and(|between| between.contains("\\end{longtable}") || between.contains("\\end{tabular}"))
        && let Some(text) = sample.get(setup..end)
    {
        style.figure_setup = text.to_owned();
    }
    (!style.is_empty()).then_some(style)
}

fn skip_space(masked: &str, mut at: usize) -> usize {
    while masked.as_bytes().get(at).is_some_and(|b| b.is_ascii_whitespace()) {
        at += 1;
    }
    at
}

/// How a note in `region` opens and closes: the text up to "Note." (and the brace that ends its
/// emphasis), and the braces left open. `None` without a note, or with one opened by an
/// environment.
fn note_format(region: &str) -> Option<(String, String)> {
    let line = region.lines().find(|l| l.contains("Note"))?;
    let at = line.find("Note")?;
    let mut end = at + "Note".len();
    while let Some(c) = line.get(end..).and_then(|r| r.chars().next()).filter(|c| matches!(c, '.' | ':' | '}')) {
        end += c.len_utf8();
    }
    let open = line.get(..end)?.trim();
    if open.contains("\\begin") || open.len() > 200 {
        return None;
    }
    let mut depth = 0i32;
    let mut escaped = false;
    for c in open.chars() {
        match c {
            '\\' if !escaped => {
                escaped = true;
                continue;
            }
            '{' if !escaped => depth += 1,
            '}' if !escaped => depth -= 1,
            _ => {}
        }
        escaped = false;
    }
    let depth = usize::try_from(depth).ok()?;
    Some((open.to_owned(), "}".repeat(depth)))
}

/// A note paragraph Word wrote under a table or figure: "Note." or "\emph{Note.}" and the rest.
/// Returns the paragraph's end and its text after the marker.
fn note_after(body: &str, from: usize) -> Option<(usize, usize, String)> {
    let start = from + (body.get(from..)?.len() - body.get(from..)?.trim_start().len());
    let rest = body.get(start..)?;
    let marker = ["Note. ", "Note: ", "\\emph{Note.} ", "\\emph{Note}. ", "\\textit{Note.} ", "\\textit{Note}. ", "\\textbf{Note.} "]
        .iter()
        .find(|m| rest.starts_with(*m))?;
    let end = rest.find("\n\n").map_or(body.len(), |e| start + e);
    let text = body.get(start + marker.len()..end)?.trim().to_owned();
    Some((start, end, text))
}

/// `\caption{\textbf{X}}` becomes `\caption{X}` in `text`: the template sets its caption's face.
fn plain_captions(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("\\caption{\\textbf{") {
        let open = at + "\\caption".len();
        let bold_open = open + 1 + "\\textbf".len();
        match (group_end(rest, open), group_end(rest, bold_open)) {
            (Some(cap_end), Some(bold_end)) if bold_end + 1 == cap_end => {
                out.push_str(rest.get(..open + 1).unwrap_or(""));
                out.push_str(rest.get(bold_open + 1..bold_end - 1).unwrap_or(""));
                out.push('}');
                rest = rest.get(cap_end..).unwrap_or("");
            }
            _ => {
                out.push_str(rest.get(..bold_open).unwrap_or(""));
                rest = rest.get(bold_open..).unwrap_or("");
            }
        }
    }
    out.push_str(rest);
    out
}

/// A Pandoc column specification with the template's column prefix: its alignment in place of
/// `\raggedright`, and its extra (a colour) on every column.
fn restyle_columns(spec: &str, style: &HouseStyle) -> String {
    let mut out = String::with_capacity(spec.len() * 2);
    let bytes = spec.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes.get(i) {
            Some(b'>') if bytes.get(i + 1) == Some(&b'{') => {
                let Some(end) = group_end(spec, i + 1) else { break };
                let mut inner = spec.get(i + 2..end - 1).unwrap_or("").to_owned();
                if !style.column_align.is_empty() {
                    inner = inner.replace("\\raggedright", &style.column_align);
                }
                out.push_str(">{");
                out.push_str(&style.column_extra);
                out.push_str(&inner);
                out.push('}');
                // The column type it applies to (p{…}, l …) follows unchanged.
                i = end;
                let col = skip_space(spec, i);
                out.push_str(spec.get(i..col).unwrap_or(""));
                i = col;
                if let Some(c) = bytes.get(i) {
                    out.push(char::from(*c));
                    i += 1;
                    if matches!(c, b'p' | b'm' | b'b') && bytes.get(i) == Some(&b'{') {
                        let Some(arg_end) = group_end(spec, i) else { break };
                        out.push_str(spec.get(i..arg_end).unwrap_or(""));
                        i = arg_end;
                    }
                }
            }
            Some(b'@') if bytes.get(i + 1) == Some(&b'{') => {
                let Some(end) = group_end(spec, i + 1) else { break };
                out.push_str(spec.get(i..end).unwrap_or(""));
                i = end;
            }
            Some(c @ (b'l' | b'c' | b'r')) if !style.column_extra.is_empty() => {
                out.push_str(">{");
                out.push_str(&style.column_extra);
                out.push('}');
                out.push(char::from(*c));
                i += 1;
            }
            Some(c) => {
                out.push(char::from(*c));
                i += 1;
            }
            None => break,
        }
    }
    if i < bytes.len() {
        out.push_str(spec.get(i..).unwrap_or(""));
    }
    out
}

/// What was restyled.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Applied {
    pub tables: usize,
    pub on_own_page: usize,
    pub figures: usize,
    pub notes: usize,
}

/// `body` (Pandoc's LaTeX) with its tables, figures and their notes set the template's way.
pub fn apply(body: &str, style: &HouseStyle) -> (String, Applied) {
    let mut applied = Applied::default();
    let body = plain_captions(body);
    let note = |text: &str| format!("\\par\\noindent{} {text}{}\n", style.note_open, style.note_close);
    let mut out = String::with_capacity(body.len() + 4096);
    let mut at = 0;
    while applied.tables + applied.figures < MAX_ITEMS {
        let next_table = body.get(at..).and_then(|r| r.find("\\begin{longtable}")).map(|t| (at + t, true));
        let next_figure = body.get(at..).and_then(|r| r.find("\\begin{figure}")).map(|f| (at + f, false));
        let Some((start, is_table)) = [next_table, next_figure].into_iter().flatten().min() else { break };
        out.push_str(body.get(at..start).unwrap_or(""));
        let name = if is_table { "longtable" } else { "figure" };
        let Some((_, end)) = matching_end(&body, name, start + "\\begin{}".len() + name.len()) else {
            at = start;
            break;
        };
        let item = body.get(start..end).unwrap_or("");
        let found_note = (!style.note_open.is_empty()).then(|| note_after(&body, end)).flatten();
        if is_table {
            // The column specification: after \begin{longtable} and its optional [].
            let mut spec_at = skip_space(&body, start + "\\begin{longtable}".len());
            if body.as_bytes().get(spec_at) == Some(&b'[') {
                spec_at = group_end(&body, spec_at).map_or(spec_at, |e| skip_space(&body, e));
            }
            let restyled = match group_end(&body, spec_at).filter(|_| body.as_bytes().get(spec_at) == Some(&b'{')) {
                Some(spec_end) => format!(
                    "{}{{{}}}{}",
                    body.get(start..spec_at).unwrap_or(""),
                    restyle_columns(body.get(spec_at + 1..spec_end - 1).unwrap_or(""), style),
                    body.get(spec_end..end).unwrap_or("")
                ),
                None => item.to_owned(),
            };
            let own_page = !style.table_page.is_empty() && item.len() > LONG_TABLE;
            let (open, close) = if own_page {
                (format!("\\begin{{{}}}\n", style.table_page), format!("\\end{{{}}}\n", style.table_page))
            } else {
                ("{".to_owned(), "}\n".to_owned())
            };
            out.push_str(&open);
            if !style.table_setup.is_empty() {
                out.push_str(&style.table_setup);
                out.push('\n');
            }
            out.push_str(&restyled);
            out.push('\n');
            if let Some((_, _, text)) = &found_note {
                out.push_str(&note(text));
                applied.notes += 1;
            }
            out.push_str(&close);
            applied.tables += 1;
            applied.on_own_page += usize::from(own_page);
        } else {
            let after_options = {
                let open = start + "\\begin{figure}".len();
                if body.as_bytes().get(open) == Some(&b'[') { group_end(&body, open).unwrap_or(open) } else { open }
            };
            out.push_str(body.get(start..after_options).unwrap_or(""));
            if !style.figure_setup.is_empty() {
                out.push('\n');
                out.push_str(&style.figure_setup);
            }
            out.push_str(body.get(after_options..end).unwrap_or(""));
            out.push('\n');
            if let Some((_, _, text)) = &found_note {
                out.push_str(&note(text));
                applied.notes += 1;
            }
            applied.figures += 1;
        }
        at = found_note.map_or(end, |(_, note_end, _)| note_end);
    }
    out.push_str(body.get(at..).unwrap_or(""));
    (out, applied)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const SAMPLE: &str = r"Text before.

\begin{WidePage}
\captionsetup{font={small,sf},skip=8pt}
\small\sffamily
\setlength{\tabcolsep}{4pt}
\begin{longtable}{>{\columncolor{tbrowcolor}\RaggedRight\arraybackslash}p{0.3\linewidth}>{\columncolor{tbrowcolor}\RaggedRight\arraybackslash}p{0.6\linewidth}}
\caption{Sample}\label{tab:1}\\
\toprule
A & B \\
\end{longtable}
{\footnotesize\rmfamily \emph{Note.} The sample's note.}
\end{WidePage}

\begin{figure}[H]
\captionsetup{font={small,sf},justification=centering}
\includegraphics{x}
\caption{A figure}
\end{figure}
";

    #[test]
    fn the_samples_table_style_is_learnt() {
        let s = learn(SAMPLE).unwrap();
        assert_eq!(s.table_setup, "\\captionsetup{font={small,sf},skip=8pt}\n\\small\\sffamily\n\\setlength{\\tabcolsep}{4pt}");
        assert_eq!(s.column_extra, "\\columncolor{tbrowcolor}");
        assert_eq!(s.column_align, "\\RaggedRight");
        assert_eq!(s.table_page, "WidePage");
        assert_eq!((s.note_open.as_str(), s.note_close.as_str()), ("{\\footnotesize\\rmfamily \\emph{Note.}", "}"));
        assert_eq!(s.figure_setup, "\\captionsetup{font={small,sf},justification=centering}");
        assert_eq!(learn("Only text.\n"), None);
    }

    const PANDOC: &str = "Before.\n\n\\begin{longtable}[]{@{}\n  >{\\raggedright\\arraybackslash}p{(\\columnwidth - 2\\tabcolsep) * \\real{0.5}}\n  >{\\raggedright\\arraybackslash}p{(\\columnwidth - 2\\tabcolsep) * \\real{0.5}}@{}}\n\\caption{\\textbf{Related approaches}}\\tabularnewline\n\\toprule\\noalign{}\nA & B \\\\\n\\end{longtable}\n\nNote. The approaches differ.\nSecond line.\n\nAfter.\n\n\\begin{figure}[!htbp]\n\\centering\n\\includegraphics{media/a.png}\n\\caption{\\textbf{A figure}}\\label{fig:word-1}\n\\end{figure}\n\nNote. Coordinates are drawn.\n\nEnd.\n";

    #[test]
    fn word_tables_and_figures_take_the_template_style() {
        let style = learn(SAMPLE).unwrap();
        let (out, applied) = apply(PANDOC, &style);
        assert_eq!(applied, Applied { tables: 1, on_own_page: 0, figures: 1, notes: 2 });
        assert!(
            out.contains("{\\captionsetup{font={small,sf},skip=8pt}\n\\small\\sffamily\n\\setlength{\\tabcolsep}{4pt}\n\\begin{longtable}[]{@{}"),
            "{out}"
        );
        assert!(out.contains(">{\\columncolor{tbrowcolor}\\RaggedRight\\arraybackslash}p{(\\columnwidth - 2\\tabcolsep) * \\real{0.5}}"), "{out}");
        assert!(out.contains("\\caption{Related approaches}\\tabularnewline"), "the template sets the caption's face: {out}");
        assert!(
            out.contains(
                "\\end{longtable}\n\\par\\noindent{\\footnotesize\\rmfamily \\emph{Note.} The approaches differ.\nSecond line.}\n}\n\n\nAfter."
            ),
            "the note goes inside, in the template's face: {out}"
        );
        assert!(out.contains("\\begin{figure}[!htbp]\n\\captionsetup{font={small,sf},justification=centering}\n\\centering"), "{out}");
        assert!(out.contains("\\caption{A figure}\\label{fig:word-1}"), "{out}");
        assert!(out.contains("\\end{figure}\n\\par\\noindent{\\footnotesize\\rmfamily \\emph{Note.} Coordinates are drawn.}\n\n\nEnd."), "{out}");
    }

    #[test]
    fn a_long_table_goes_on_the_templates_table_page() {
        let style = learn(SAMPLE).unwrap();
        let rows = "Cell & Cell \\\\\n".repeat(300);
        let long = PANDOC.replace("A & B \\\\\n", &rows);
        let (out, applied) = apply(&long, &style);
        assert_eq!(applied.on_own_page, 1);
        assert!(out.contains("\\begin{WidePage}\n\\captionsetup") && out.contains("\\end{WidePage}\n"), "{out}");
    }

    #[test]
    fn simple_columns_get_the_colour_and_unclosed_text_is_left() {
        let style = HouseStyle { column_extra: "\\columncolor{x}".into(), ..HouseStyle::default() };
        assert_eq!(restyle_columns("@{}lr@{}", &style), "@{}>{\\columncolor{x}}l>{\\columncolor{x}}r@{}");
        let (out, applied) = apply("\\begin{longtable}{ll}\nno end", &style);
        assert_eq!(applied.tables, 0);
        assert_eq!(out, "\\begin{longtable}{ll}\nno end");
    }
}
