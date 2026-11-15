//! DOCX import through Pandoc: the interface reserved for later phases, with a first,
//! experimental implementation.
//!
//! Status: experimental. It converts the body text, headings, lists, tables, equations and
//! images Pandoc understands; it does not map Word styles to the journal's, nor read Word
//! citations into BibLaTeX. The app offers it only when `pandoc` is installed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::Error;
use crate::paths::write_atomic;
use crate::process;
use crate::project::{MAX_IMPORT, Project};

/// The time limit for one conversion.
pub const TIMEOUT: Duration = Duration::from_secs(120);

/// Converts a word-processor manuscript into the project's LaTeX body. Later importers (a
/// native DOCX reader, a JATS reader) implement the same trait.
pub trait ManuscriptImporter {
    /// A short name for menus and logs.
    fn name(&self) -> &str;
    /// The file extensions it reads, lowercase without the dot.
    fn extensions(&self) -> &[&str];
    /// Convert `source` into `project`'s body file. Returns a note for the log.
    fn import(&self, source: &Path, project: &Project, cancel: Option<&Arc<AtomicBool>>) -> Result<String, Error>;
}

/// [`ManuscriptImporter`] for `.docx`, running the local `pandoc`.
pub struct PandocImporter {
    pub pandoc: PathBuf,
}

/// The copy of the manuscript Pandoc reads, inside the project.
const DOCX_COPY: &str = "manuscript-source.docx";
/// What Pandoc writes; moved to the body file once it succeeds.
const CONVERTED: &str = "manuscript-from-docx.tex";

impl ManuscriptImporter for PandocImporter {
    fn name(&self) -> &str {
        "Pandoc (DOCX)"
    }

    fn extensions(&self) -> &[&str] {
        &["docx"]
    }

    fn import(&self, source: &Path, project: &Project, cancel: Option<&Arc<AtomicBool>>) -> Result<String, Error> {
        let meta = std::fs::metadata(source).map_err(|e| Error::io(source, e))?;
        if !meta.is_file() || meta.len() > MAX_IMPORT {
            return Err(Error::Project(format!("{} is not a file of at most {} MB", source.display(), MAX_IMPORT / 1024 / 1024)));
        }
        let bytes = std::fs::read(source).map_err(|e| Error::io(source, e))?;
        // A .docx is a ZIP archive.
        if !bytes.starts_with(b"PK") {
            return Err(Error::Project(format!("{} is not a .docx file", source.display())));
        }
        write_atomic(&project.dir.join(DOCX_COPY), &bytes)?;
        let mut cmd = Command::new(&self.pandoc);
        cmd.current_dir(&project.dir);
        // --sandbox: readers and writers may only touch the files named here (Pandoc ≥ 2.15).
        cmd.args(["--sandbox", "--from=docx", "--to=latex", "--wrap=preserve", "--extract-media=media", "-o", CONVERTED, DOCX_COPY]);
        let run = process::run(cmd, TIMEOUT, cancel).map_err(|e| Error::Compile(format!("could not start pandoc: {e}")))?;
        if !run.success() {
            let why = if run.timed_out {
                "it took too long".to_owned()
            } else if run.cancelled {
                "it was cancelled".to_owned()
            } else {
                run.stderr.trim().to_owned()
            };
            return Err(Error::Project(format!("Pandoc could not convert {}: {why}", source.display())));
        }
        let converted = project.dir.join(CONVERTED);
        let body = std::fs::read(&converted).map_err(|e| Error::io(&converted, e))?;
        write_atomic(&project.dir.join(&project.file.body), &body)?;
        let _ = std::fs::remove_file(&converted);
        let warnings = run.stderr.lines().filter(|l| !l.trim().is_empty()).count();
        let mut note =
            format!("Converted {} with Pandoc into {} ({warnings} warning line(s)); figures are in media/.", source.display(), project.file.body);
        for problem in review_converted(&String::from_utf8_lossy(&body), &project.dir) {
            note.push_str("\nTo fix before compiling: ");
            note.push_str(&problem);
        }
        Ok(note)
    }
}

/// Image formats pdfLaTeX's graphicx includes.
const PDFLATEX_IMAGES: &[&str] = &["png", "jpg", "jpeg", "pdf"];
/// The most distinct characters named in one note.
const MAX_CHARACTERS: usize = 12;
/// The most media files looked at.
const MAX_MEDIA: usize = 2000;

/// Whether pdfLaTeX, with the templates' T1 fonts and LaTeX's UTF-8 input, typesets `c` without
/// extra packages: ASCII, Latin-1 and Latin Extended-A, and the common typographic punctuation.
fn pdflatex_can_set(c: char) -> bool {
    matches!(c as u32,
        0x09 | 0x0A | 0x0D | 0x20..=0x7E
        | 0xA0..=0x17F
        | 0x192 | 0x218..=0x21B | 0x2C6 | 0x2C7 | 0x2D8..=0x2DD
        | 0x2013 | 0x2014 | 0x2018..=0x201E | 0x2020..=0x2022 | 0x2026 | 0x2030 | 0x2039 | 0x203A
        | 0x20AC | 0x2122)
}

/// What a converted body holds that pdfLaTeX can't typeset as it is, each as a sentence saying
/// what to do: characters the fonts have no glyph for, and figures in formats pdfLaTeX can't
/// include (Word's EMF and WMF drawings, TIFF scans). `project_dir` holds Pandoc's `media/`.
pub fn review_converted(body: &str, project_dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars: Vec<(char, usize)> = Vec::new();
    let mut more = 0usize;
    for (n, line) in body.lines().enumerate() {
        for c in line.chars().filter(|c| !pdflatex_can_set(*c)) {
            if chars.iter().any(|(seen, _)| *seen == c) {
                continue;
            }
            if chars.len() < MAX_CHARACTERS {
                chars.push((c, n.saturating_add(1)));
            } else {
                more = more.saturating_add(1);
            }
        }
    }
    if !chars.is_empty() {
        let list: Vec<String> = chars.iter().map(|(c, line)| format!("{c} (U+{:04X}, line {line})", *c as u32)).collect();
        let more = if more > 0 { format!(" and {more} more") } else { String::new() };
        out.push(format!(
            "pdfLaTeX's fonts have no glyph for {}{more}. Replace them, or write them as LaTeX commands (Greek letters as $\\alpha$ …).",
            list.join(", ")
        ));
    }
    let mut stack = vec![(project_dir.join("media"), 0u8)];
    let mut seen = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            seen = seen.saturating_add(1);
            if seen > MAX_MEDIA {
                return out;
            }
            let path = entry.path();
            if path.is_dir() {
                if depth < 4 {
                    stack.push((path, depth.saturating_add(1)));
                }
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
            if !PDFLATEX_IMAGES.contains(&ext.as_str()) {
                let shown = path.strip_prefix(project_dir).unwrap_or(&path).display().to_string();
                out.push(format!("{shown}: pdfLaTeX can't include .{ext} images; convert it to PDF or PNG and keep the same name and folder."));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::PublicationMetadata;
    use crate::template::Template;

    #[test]
    fn what_pdflatex_cannot_set_is_named_with_its_line() {
        let dir = tempfile::tempdir().unwrap();
        let body = "caf\u{e9} \u{2014} \u{201c}fine\u{201d}\n\u{3b1} and \u{4e2d}\u{6587}\n\u{3b1} again\n";
        let problems = review_converted(body, dir.path());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("\u{3b1} (U+03B1, line 2)"), "{}", problems[0]);
        assert!(problems[0].contains("U+4E2D") && problems[0].contains("U+6587"));
        assert!(!problems[0].contains("U+00E9"), "Latin-1 is fine");
        assert!(review_converted("plain ASCII, na\u{ef}ve r\u{e9}sum\u{e9}\n", dir.path()).is_empty());
    }

    #[test]
    fn many_characters_are_summarised() {
        let dir = tempfile::tempdir().unwrap();
        let body: String = ('\u{4e00}'..='\u{4e20}').collect();
        let problems = review_converted(&body, dir.path());
        assert!(problems[0].contains(&format!(" and {} more", 0x21 - MAX_CHARACTERS)), "{}", problems[0]);
    }

    #[test]
    fn figures_pdflatex_cannot_include_are_named() {
        let dir = tempfile::tempdir().unwrap();
        let media = dir.path().join("media").join("media");
        std::fs::create_dir_all(&media).unwrap();
        for f in ["image1.png", "image2.emf", "image3.JPG", "scan.tiff"] {
            std::fs::write(media.join(f), "x").unwrap();
        }
        let mut problems = review_converted("", dir.path());
        problems.sort();
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].contains("image2.emf") && problems[0].contains(".emf"));
        assert!(problems[1].contains("scan.tiff"));
    }

    #[test]
    fn a_non_docx_file_is_refused_before_running_pandoc() {
        let root = tempfile::tempdir().unwrap();
        let p = Project::create(&root.path().join("p"), &Template::builtin().remove(0), PublicationMetadata::sample()).unwrap();
        let src = root.path().join("x.docx");
        std::fs::write(&src, "plain text").unwrap();
        let importer = PandocImporter { pandoc: PathBuf::from("/nonexistent/pandoc") };
        assert!(importer.import(&src, &p, None).is_err());
    }
}
