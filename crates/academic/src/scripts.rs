//! Which writing systems a manuscript uses, and which of them the pdfLaTeX templates can't set.
//!
//! The journal templates run pdfLaTeX with `\usepackage[T1]{fontenc}` and Latin Modern: Western and
//! Central European Latin letters are fine, but Chinese, Japanese, Korean, Cyrillic, Greek (outside
//! mathematics), Arabic, Hebrew, Thai and Indic letters have no font and stop the compilation with
//! "Unicode character … not set up for use with LaTeX". This check finds them before compiling and
//! says plainly what the limitation is and what can be done, instead of leaving it to the TeX log.

use serde::Serialize;

use crate::diagnostics::Severity;

/// A writing system the templates can't typeset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum Script {
    /// Chinese characters (also used in Japanese and Korean).
    Han,
    /// Japanese hiragana and katakana.
    Kana,
    /// Korean Hangul.
    Hangul,
    Cyrillic,
    Greek,
    Arabic,
    Hebrew,
    Thai,
    /// Devanagari and the other Indic scripts.
    Indic,
}

impl Script {
    /// The script `c` belongs to, if it is one the templates can't set.
    pub fn of(c: char) -> Option<Script> {
        Some(match u32::from(c) {
            0x2E80..=0x2FDF | 0x3005..=0x3007 | 0x3021..=0x3029 | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x3FFFF => {
                Script::Han
            }
            0x3040..=0x30FF | 0x31F0..=0x31FF | 0xFF66..=0xFF9F => Script::Kana,
            0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7AF | 0xD7B0..=0xD7FF => Script::Hangul,
            0x0400..=0x052F | 0x1C80..=0x1C8F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F => Script::Cyrillic,
            0x0370..=0x03FF | 0x1F00..=0x1FFF => Script::Greek,
            0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => Script::Arabic,
            0x0590..=0x05FF | 0xFB1D..=0xFB4F => Script::Hebrew,
            0x0E00..=0x0E7F => Script::Thai,
            0x0900..=0x0DFF => Script::Indic,
            _ => return None,
        })
    }

    /// The English name, for messages.
    pub fn name(self) -> &'static str {
        match self {
            Script::Han => "Chinese characters",
            Script::Kana => "Japanese kana",
            Script::Hangul => "Korean Hangul",
            Script::Cyrillic => "Cyrillic",
            Script::Greek => "Greek",
            Script::Arabic => "Arabic",
            Script::Hebrew => "Hebrew",
            Script::Thai => "Thai",
            Script::Indic => "Indic script",
        }
    }

    /// What the limitation is and what to do, for this script.
    pub fn advice(self) -> &'static str {
        match self {
            Script::Han | Script::Kana | Script::Hangul => {
                "The pdfLaTeX journal templates can't typeset Chinese, Japanese or Korean (they need XeLaTeX or LuaLaTeX with a CJK font, which these templates don't use). Give titles, names and references in Latin script (romanized, with the original in the source data), or prepare the article with a CJK-capable template outside PSG Academic PDF."
            }
            Script::Greek => {
                "The templates have no Greek text font. In formulas write Greek letters as commands ($\\alpha$, $\\Omega$); Greek words in running text must be transliterated."
            }
            Script::Cyrillic => {
                "The templates have no Cyrillic font (T2A encoding). Transliterate Cyrillic names and titles, as most English-language journals require."
            }
            Script::Arabic | Script::Hebrew => {
                "pdfLaTeX can't set right-to-left scripts with these templates. Transliterate the text, or prepare the article with a template for XeLaTeX or LuaLaTeX."
            }
            Script::Thai | Script::Indic => "The templates have no font for this script. Transliterate the text.",
        }
    }
}

/// One script found in one place.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScriptFinding {
    pub script: Script,
    /// Where: a file name (`body.tex`) or a metadata field (`metadata: title`).
    pub place: String,
    /// The 1-based line of the first occurrence, in a file.
    pub line: Option<u32>,
    /// The first character found.
    pub sample: char,
    pub count: usize,
}

impl ScriptFinding {
    pub fn severity(&self) -> Severity {
        Severity::Error
    }

    /// A one-line message with the advice, starting with where it is.
    pub fn message(&self) -> String {
        let at = match self.line {
            Some(l) => format!("{}:{l}", self.place),
            None => self.place.clone(),
        };
        format!("{at}: {}", self.summary())
    }

    /// [`Self::message`] without the place, for a list that shows the file and line already.
    pub fn summary(&self) -> String {
        format!(
            "{} {} (first \"{}\", U+{:04X}). {}",
            self.count,
            if self.count == 1 { format!("character in {}", self.script.name()) } else { format!("characters in {}", self.script.name()) },
            self.sample,
            u32::from(self.sample),
            self.script.advice()
        )
    }
}

/// The scripts in `text` the templates can't set, one finding per script, in order of first use.
/// In a `.tex` file comments are skipped, and Greek inside `$…$` is counted too (a literal α in a
/// formula fails just as in text).
pub fn scan(text: &str, place: &str, tex: bool) -> Vec<ScriptFinding> {
    let mut found: Vec<ScriptFinding> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = if tex { strip_comment(line) } else { line };
        for c in line.chars() {
            let Some(script) = Script::of(c) else { continue };
            match found.iter_mut().find(|f| f.script == script) {
                Some(f) => f.count = f.count.saturating_add(1),
                None => found.push(ScriptFinding {
                    script,
                    place: place.to_owned(),
                    line: tex.then(|| u32::try_from(n + 1).unwrap_or(u32::MAX)),
                    sample: c,
                    count: 1,
                }),
            }
        }
    }
    found
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_european_text_is_fine() {
        assert!(scan("Jürgen Großmann, François, Ñúñez, Łódź — “quotes” – 1–2 ±", "body.tex", true).is_empty());
    }

    #[test]
    fn cjk_and_other_scripts_are_found_with_their_first_line() {
        let text = "Intro.\n张三 and 李四\n% 注释 is a comment\n山田たろう 김민준 Саша α\n";
        let f = scan(text, "body.tex", true);
        let scripts: Vec<Script> = f.iter().map(|f| f.script).collect();
        assert_eq!(scripts, [Script::Han, Script::Kana, Script::Hangul, Script::Cyrillic, Script::Greek]);
        let han = &f[0];
        assert_eq!((han.line, han.sample, han.count), (Some(2), '张', 6));
        assert!(han.message().starts_with("body.tex:2: 6 characters in Chinese characters"), "{}", han.message());
        assert!(han.message().contains("can't typeset Chinese, Japanese or Korean"));
    }

    #[test]
    fn metadata_has_no_lines_and_escaped_percent_is_text() {
        let f = scan("论文标题", "metadata: title", false);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].line, None);
        assert!(f[0].message().starts_with("metadata: title: 4 characters"));
        assert_eq!(scan("50\\% 汉", "body.tex", true).len(), 1);
    }
}
