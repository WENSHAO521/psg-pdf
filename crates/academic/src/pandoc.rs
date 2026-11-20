//! DOCX import through Pandoc: the interface reserved for later phases, with a first,
//! experimental implementation.
//!
//! Pandoc reads the Word file into its document tree, [`crate::docx`] takes the title, authors,
//! affiliations, abstract and keywords out of it into the project's metadata and turns captioned
//! pictures and tables into numbered figures and tables, and Pandoc writes the rest as the body.
//! Citations a reference manager put into the Word file become LaTeX citations, with their
//! entries added to the template's bibliography ([`crate::citations`]). The app offers it only
//! when `pandoc` is installed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::paths::write_atomic;
use crate::process;
use crate::project::{MAX_IMPORT, Project};
use crate::{Engine, Error};

/// The time limit for one conversion.
pub const TIMEOUT: Duration = Duration::from_secs(120);

/// The oldest Pandoc with `--sandbox`, which keeps a conversion from reading or writing any file
/// but the ones named. An older Pandoc is refused rather than run without it.
pub const MIN_PANDOC: (u32, u32) = (2, 15);

/// Whether the Pandoc whose `--version` first line is `version_line` (`pandoc 3.1.3`) can convert
/// sandboxed; `None` when the line names no version.
pub fn sandbox_supported(version_line: &str) -> Option<bool> {
    let number = version_line.split_whitespace().find(|w| w.starts_with(|c: char| c.is_ascii_digit()))?;
    let mut parts = number.split('.').map(|p| p.chars().take_while(char::is_ascii_digit).collect::<String>().parse::<u32>().ok());
    let major = parts.next().flatten()?;
    let minor = parts.next().flatten().unwrap_or(0);
    Some((major, minor) >= MIN_PANDOC)
}

fn too_old(version: &str) -> Error {
    Error::Project(format!(
        "{version} is too old for DOCX import: it needs Pandoc {}.{} or newer, which converts without access to any other file. Install a newer Pandoc from pandoc.org.",
        MIN_PANDOC.0, MIN_PANDOC.1
    ))
}

/// Converts a word-processor manuscript into the project's LaTeX body. Later importers (a
/// native DOCX reader, a JATS reader) implement the same trait.
pub trait ManuscriptImporter {
    /// A short name for menus and logs.
    fn name(&self) -> &str;
    /// The file extensions it reads, lowercase without the dot.
    fn extensions(&self) -> &[&str];
    /// Convert `source` into `project`'s body file, and take the article's front matter into its
    /// metadata where the manuscript has it. Returns a note for the log.
    fn import(&self, source: &Path, project: &mut Project, cancel: Option<&Arc<AtomicBool>>) -> Result<String, Error>;
}

/// [`ManuscriptImporter`] for `.docx`, running the local `pandoc`.
pub struct PandocImporter {
    pub pandoc: PathBuf,
}

/// The copy of the manuscript Pandoc reads, inside the project.
const DOCX_COPY: &str = "manuscript-source.docx";
/// What Pandoc writes; moved to the body file once it succeeds.
const CONVERTED: &str = "manuscript-from-docx.tex";
/// Pandoc's document tree of the manuscript, between reading and writing.
const TREE: &str = "manuscript-from-docx.json";
/// The largest document tree read back (bytes); the pictures are files beside it, not in it.
const MAX_TREE: u64 = 256 * 1024 * 1024;

impl ManuscriptImporter for PandocImporter {
    fn name(&self) -> &str {
        "Pandoc (DOCX)"
    }

    fn extensions(&self) -> &[&str] {
        &["docx"]
    }

    fn import(&self, source: &Path, project: &mut Project, cancel: Option<&Arc<AtomicBool>>) -> Result<String, Error> {
        let meta = std::fs::metadata(source).map_err(|e| Error::io(source, e))?;
        if !meta.is_file() || meta.len() > MAX_IMPORT {
            return Err(Error::Project(format!("{} is not a file of at most {} MB", source.display(), MAX_IMPORT / 1024 / 1024)));
        }
        let bytes = std::fs::read(source).map_err(|e| Error::io(source, e))?;
        // A .docx is a ZIP archive.
        if !bytes.starts_with(b"PK") {
            return Err(Error::Project(format!("{} is not a .docx file", source.display())));
        }
        if let Some(version) = process::version_line(&self.pandoc)
            && sandbox_supported(&version) == Some(false)
        {
            return Err(too_old(&version));
        }
        project.invalidate_pdf()?;
        write_atomic(&project.dir.join(DOCX_COPY), &bytes)?;
        // Read the manuscript into Pandoc's document tree, with its pictures extracted to media/.
        let read = self.pandoc(project, &["--from=docx+citations", "--to=json", "--extract-media=media", "-o", TREE, DOCX_COPY], source, cancel)?;
        let tree_path = project.dir.join(TREE);
        let mut doc = read_tree(&tree_path)?;
        let (front, changes) = crate::docx::restructure(&mut doc);
        let cited = self.take_citations(project, &mut doc, source, cancel)?;
        let tree = serde_json::to_vec(&doc).map_err(|e| Error::Project(format!("could not write the document tree: {e}")))?;
        write_atomic(&tree_path, &tree)?;
        drop(doc);
        // Write the body as LaTeX.
        let write = self.pandoc(project, &["--from=json", "--to=latex", "--wrap=preserve", "-o", CONVERTED, TREE], source, cancel)?;
        let converted = project.dir.join(CONVERTED);
        let body = std::fs::read(&converted).map_err(|e| Error::io(&converted, e))?;
        write_atomic(&project.dir.join(&project.file.body), &body)?;
        let _ = std::fs::remove_file(&converted);
        let _ = std::fs::remove_file(&tree_path);
        let taken = project.apply_front_matter(&front)?;
        let adapted = project.adapt_engine_to_text()?;
        let warnings = read.saturating_add(write);
        let mut note = format!(
            "Converted {} with Pandoc into {} ({warnings} warning line(s)): {} figure(s) and {} table(s) with captions; figures are in media/.",
            source.display(),
            project.file.body,
            changes.figures,
            changes.tables
        );
        if let Some(cited) = &cited {
            note.push(' ');
            note.push_str(cited);
        }
        if changes.typed_heading_numbers > 0 {
            note.push_str(&format!(" {} heading(s) keep the numbers typed in Word.", changes.typed_heading_numbers));
        }
        if !taken.is_empty() {
            note.push_str(&format!(" From the manuscript's front matter: {}.", taken.join(", ")));
        }
        if let Some(adapted) = adapted {
            note.push(' ');
            note.push_str(&adapted);
        }
        for problem in review_converted(&String::from_utf8_lossy(&body), &project.dir, project.file.engine) {
            note.push_str("\nTo fix before compiling: ");
            note.push_str(&problem);
        }
        Ok(note)
    }
}

/// Where Pandoc writes the bibliography entries of the Word citations, before they are added to
/// the template's bibliography file.
const REFERENCES_TREE: &str = "manuscript-references.json";
const REFERENCES_BIB: &str = "manuscript-references.bib";

/// How the template's main file cites, and the bibliography file it reads: `\addbibresource{x.bib}`
/// (BibLaTeX) or `\bibliography{x}` (BibTeX). `None` when it has neither.
pub fn citation_setup(main: &str) -> Option<(crate::citations::Style, String)> {
    use crate::template_import::{find_call, mask_comments};
    let masked = mask_comments(main);
    let first_arg = |name: &str| {
        let call = find_call(&masked, name, 0, 1)?;
        let (a, b) = *call.mandatory.first()?;
        let list = main.get(a..b)?;
        let first = list.split(',').next()?.trim();
        crate::paths::safe_relative_name(first).map(|_| first.to_owned())
    };
    if let Some(file) = first_arg("addbibresource") {
        return Some((crate::citations::Style::Biblatex, file));
    }
    let file = first_arg("bibliography")?;
    let file = if file.to_ascii_lowercase().ends_with(".bib") { file } else { format!("{file}.bib") };
    Some((crate::citations::Style::Bibtex, file))
}

impl PandocImporter {
    /// Turn the Word citations in `doc` into the template's kind of citation and add their entries
    /// to its bibliography file. A note for the log, or `None` when there were none (or the
    /// template has no bibliography, when Word's own citation text stays).
    fn take_citations(
        &self,
        project: &Project,
        doc: &mut serde_json::Value,
        source: &Path,
        cancel: Option<&Arc<AtomicBool>>,
    ) -> Result<Option<String>, Error> {
        let main_path = project.dir.join(&project.file.main);
        let main = std::fs::read_to_string(&main_path).map_err(|e| Error::io(&main_path, e))?;
        let Some((style, bib_name)) = citation_setup(&main) else { return Ok(None) };
        let bib_path = project.dir.join(&bib_name);
        let existing = std::fs::read_to_string(&bib_path).unwrap_or_default();
        let taken = crate::bib::parse(&existing).0.into_iter().map(|e| e.key).collect();
        let Some(converted) = crate::citations::convert(doc, style, &taken) else { return Ok(None) };
        let tree = serde_json::to_vec(&converted.references).map_err(|e| Error::Project(format!("could not write the references: {e}")))?;
        write_atomic(&project.dir.join(REFERENCES_TREE), &tree)?;
        let to = match style {
            crate::citations::Style::Biblatex => "--to=biblatex",
            crate::citations::Style::Bibtex => "--to=bibtex",
        };
        let written = self.pandoc(project, &["--from=json", to, "-o", REFERENCES_BIB, REFERENCES_TREE], source, cancel);
        let _ = std::fs::remove_file(project.dir.join(REFERENCES_TREE));
        written?;
        let new_path = project.dir.join(REFERENCES_BIB);
        let entries = std::fs::read_to_string(&new_path).map_err(|e| Error::io(&new_path, e))?;
        let _ = std::fs::remove_file(&new_path);
        let mut bib = existing;
        if !bib.is_empty() && !bib.ends_with('\n') {
            bib.push('\n');
        }
        bib.push_str("\n% Added by PSG Academic PDF from the Word manuscript's citations.\n");
        bib.push_str(&entries);
        write_atomic(&bib_path, bib.as_bytes())?;
        let mut note = format!(
            "{} citation(s) from the reference manager became {}, with {} bibliography entr{} added to {bib_name}",
            converted.citations,
            match style {
                crate::citations::Style::Biblatex => "\\parencite/\\textcite",
                crate::citations::Style::Bibtex => "\\cite",
            },
            converted.keys.len(),
            if converted.keys.len() == 1 { "y" } else { "ies" },
        );
        note.push_str(if converted.removed_list { "; Word's reference list was left out, as the template prints the bibliography." } else { "." });
        Ok(Some(note))
    }

    /// Run Pandoc sandboxed in the project with `args`. Returns the number of warning lines.
    fn pandoc(&self, project: &Project, args: &[&str], source: &Path, cancel: Option<&Arc<AtomicBool>>) -> Result<usize, Error> {
        let mut cmd = Command::new(&self.pandoc);
        cmd.current_dir(&project.dir);
        // --sandbox: readers and writers may only touch the files named here (Pandoc ≥ 2.15).
        cmd.arg("--sandbox").args(args);
        let run = process::run(cmd, TIMEOUT, cancel).map_err(|e| Error::Compile(format!("could not start pandoc: {e}")))?;
        if !run.success() {
            let why = if run.timed_out {
                "it took too long".to_owned()
            } else if run.cancelled {
                "it was cancelled".to_owned()
            } else if run.stderr.contains("--sandbox") {
                return Err(too_old("This Pandoc"));
            } else {
                run.stderr.trim().to_owned()
            };
            return Err(Error::Project(format!("Pandoc could not convert {}: {why}", source.display())));
        }
        Ok(run.stderr.lines().filter(|l| !l.trim().is_empty()).count())
    }
}

/// Read Pandoc's JSON document tree. serde_json refuses nesting deeper than 128 levels, which
/// bounds every later walk.
fn read_tree(path: &Path) -> Result<serde_json::Value, Error> {
    let len = std::fs::metadata(path).map_err(|e| Error::io(path, e))?.len();
    if len > MAX_TREE {
        return Err(Error::Project(format!("the converted manuscript is larger than {} MB", MAX_TREE / 1024 / 1024)));
    }
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    serde_json::from_slice(&bytes).map_err(|e| Error::Project(format!("could not read Pandoc's document tree: {e}")))
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

/// Whether XeLaTeX, with the templates' Latin Modern and the Noto CJK faces, typesets `c`: what
/// pdfLaTeX sets, and Chinese and Japanese with their punctuation and full-width forms.
fn xelatex_can_set(c: char) -> bool {
    pdflatex_can_set(c)
        || matches!(c as u32,
            0xB7 | 0x2025 | 0x2027 | 0x2E80..=0x2FDF | 0x3000..=0x303F | 0x3040..=0x30FF | 0x31F0..=0x31FF
            | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFFEF
            | 0x20000..=0x2FA1F)
}

/// What a converted body holds that the template's engine can't typeset as it is, each as a
/// sentence saying what to do: characters the fonts have no glyph for, and figures in formats
/// the engine can't include (Word's EMF and WMF drawings, TIFF scans). `project_dir` holds
/// Pandoc's `media/`.
pub fn review_converted(body: &str, project_dir: &Path, engine: Engine) -> Vec<String> {
    let can_set = match engine {
        Engine::Pdflatex => pdflatex_can_set,
        Engine::Xelatex => xelatex_can_set,
    };
    let mut out = Vec::new();
    let mut chars: Vec<(char, usize)> = Vec::new();
    let mut more = 0usize;
    for (n, line) in body.lines().enumerate() {
        for c in line.chars().filter(|c| !can_set(*c)) {
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
            "{}'s fonts have no glyph for {}{more}. Replace them, or write them as LaTeX commands (Greek letters as $\\alpha$ …).",
            engine.title(),
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
                out.push(format!(
                    "{shown}: {} can't include .{ext} images; convert it to PDF or PNG and keep the same name and folder.",
                    engine.title()
                ));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_says_how_it_cites() {
        use crate::citations::Style;
        let biblatex = "\\usepackage{biblatex}\n% \\bibliography{old}\n\\addbibresource{references.bib}\n";
        assert_eq!(citation_setup(biblatex), Some((Style::Biblatex, "references.bib".into())));
        let bibtex = "\\bibliographystyle{plain}\n\\bibliography{refs,more}\n";
        assert_eq!(citation_setup(bibtex), Some((Style::Bibtex, "refs.bib".into())));
        assert_eq!(citation_setup("\\bibliography{../../etc/x}"), None, "outside the project");
        assert_eq!(citation_setup("\\begin{document}\\end{document}"), None);
    }
    use crate::metadata::PublicationMetadata;
    use crate::template::Template;

    #[test]
    fn what_pdflatex_cannot_set_is_named_with_its_line() {
        let dir = tempfile::tempdir().unwrap();
        let body = "caf\u{e9} \u{2014} \u{201c}fine\u{201d}\n\u{3b1} and \u{4e2d}\u{6587}\n\u{3b1} again\n";
        let problems = review_converted(body, dir.path(), Engine::Pdflatex);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("\u{3b1} (U+03B1, line 2)"), "{}", problems[0]);
        assert!(problems[0].contains("U+4E2D") && problems[0].contains("U+6587"));
        assert!(!problems[0].contains("U+00E9"), "Latin-1 is fine");
        let xe = review_converted(body, dir.path(), Engine::Xelatex);
        assert_eq!(xe.len(), 1, "{xe:?}");
        assert!(xe[0].starts_with("XeLaTeX's fonts") && xe[0].contains("U+03B1") && !xe[0].contains("U+4E2D"), "{}", xe[0]);
        assert!(review_converted("中文，「引號」與全形！\n", dir.path(), Engine::Xelatex).is_empty());
        assert!(review_converted("plain ASCII, na\u{ef}ve r\u{e9}sum\u{e9}\n", dir.path(), Engine::Pdflatex).is_empty());
    }

    #[test]
    fn many_characters_are_summarised() {
        let dir = tempfile::tempdir().unwrap();
        let body: String = ('\u{4e00}'..='\u{4e20}').collect();
        let problems = review_converted(&body, dir.path(), Engine::Pdflatex);
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
        let mut problems = review_converted("", dir.path(), Engine::Pdflatex);
        problems.sort();
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].contains("image2.emf") && problems[0].contains(".emf"));
        assert!(problems[1].contains("scan.tiff"));
    }

    #[test]
    fn only_a_pandoc_that_can_sandbox_is_used() {
        assert_eq!(sandbox_supported("pandoc 2.9.2.1"), Some(false));
        assert_eq!(sandbox_supported("pandoc 2.14"), Some(false));
        assert_eq!(sandbox_supported("pandoc 2.15"), Some(true));
        assert_eq!(sandbox_supported("pandoc.exe 3.1.3"), Some(true));
        assert_eq!(sandbox_supported("pandoc 10"), Some(true));
        assert_eq!(sandbox_supported("pandoc"), None);
        assert!(too_old("pandoc 2.9.2.1").to_string().contains("2.15 or newer"));
    }

    #[test]
    fn a_non_docx_file_is_refused_before_running_pandoc() {
        let root = tempfile::tempdir().unwrap();
        let mut p = Project::create(&root.path().join("p"), &Template::builtin().remove(0), PublicationMetadata::sample()).unwrap();
        let src = root.path().join("x.docx");
        std::fs::write(&src, "plain text").unwrap();
        let importer = PandocImporter { pandoc: PathBuf::from("/nonexistent/pandoc") };
        assert!(importer.import(&src, &mut p, None).is_err());
    }
}
