//! A typesetting project: one directory holding a template's files, the imported manuscript,
//! its figures and bibliography, the metadata, and the compiled PDF.
//!
//! Compilation happens in this directory and nowhere else (see [`crate::compile`]). Imported
//! files are copied in under safe names; the originals are never touched.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::Error;
use crate::compile::{self, CompileOptions, CompileReport};
use crate::diagnostics::{Diagnostic, Severity};
use crate::metadata::PublicationMetadata;
use crate::paths::{sanitize_name, write_atomic};
use crate::template::Template;
use crate::tex::TexEnvironment;

/// The project file in a project directory.
pub const PROJECT_FILE: &str = "psg-project.toml";
/// The largest manuscript, bibliography or figure imported (bytes).
pub const MAX_IMPORT: u64 = 64 * 1024 * 1024;
/// File types that may be imported next to the manuscript.
pub const ASSET_EXTENSIONS: &[&str] = &["bib", "png", "jpg", "jpeg", "pdf", "eps", "tex", "csv"];

/// Typesetting settings saved with the project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The compilation time limit, in seconds.
    pub timeout_secs: u64,
    /// Rebuild everything on every compilation.
    pub force_rebuild: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { timeout_secs: compile::DEFAULT_TIMEOUT.as_secs(), force_rebuild: false }
    }
}

/// What is saved in [`PROJECT_FILE`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFile {
    pub template: String,
    pub main: String,
    pub body: String,
    pub metadata_file: String,
    pub uses_biber: bool,
    /// The template's engine. Projects from before XeLaTeX templates have none: pdfLaTeX.
    #[serde(default)]
    pub engine: crate::Engine,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub metadata: PublicationMetadata,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub dir: PathBuf,
    pub file: ProjectFile,
}

impl Project {
    /// Start a project in `dir` (created if needed; must be empty if it exists) from `template`.
    pub fn create(dir: &Path, template: &Template, metadata: PublicationMetadata) -> Result<Project, Error> {
        if dir.exists() {
            let mut entries = std::fs::read_dir(dir).map_err(|e| Error::io(dir, e))?;
            if entries.next().is_some() {
                return Err(Error::Project(format!("{} is not empty; choose an empty folder for a new project", dir.display())));
            }
        } else {
            std::fs::create_dir_all(dir).map_err(|e| Error::io(dir, e))?;
        }
        template.materialize(dir)?;
        let m = &template.manifest;
        let project = Project {
            dir: dir.to_path_buf(),
            file: ProjectFile {
                template: m.id.clone(),
                main: m.main.clone(),
                body: m.body.clone(),
                metadata_file: m.metadata.clone(),
                uses_biber: m.uses_biber(),
                engine: m.engine(),
                settings: Settings::default(),
                metadata,
            },
        };
        project.save()?;
        project.write_metadata()?;
        Ok(project)
    }

    /// Open the project saved in `dir`.
    pub fn open(dir: &Path) -> Result<Project, Error> {
        let path = dir.join(PROJECT_FILE);
        let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        let file: ProjectFile = toml::from_str(&text).map_err(|e| Error::Project(format!("{}: {e}", path.display())))?;
        for name in [&file.main, &file.body, &file.metadata_file] {
            if crate::paths::safe_relative_name(name).is_none() {
                return Err(Error::Project(format!("{}: \"{name}\" is not a plain file name", path.display())));
            }
        }
        Ok(Project { dir: dir.to_path_buf(), file })
    }

    /// Save [`PROJECT_FILE`].
    pub fn save(&self) -> Result<(), Error> {
        let text = toml::to_string_pretty(&self.file).map_err(|e| Error::Project(format!("could not save the project: {e}")))?;
        write_atomic(&self.dir.join(PROJECT_FILE), text.as_bytes())
    }

    /// Write the metadata file the template reads. Blocking metadata problems are refused.
    pub fn write_metadata(&self) -> Result<(), Error> {
        if let Some(issue) = self.file.metadata.validate().into_iter().find(|i| i.blocking) {
            return Err(Error::Metadata(format!("{}: {}", issue.field, issue.message)));
        }
        write_atomic(&self.dir.join(&self.file.metadata_file), self.file.metadata.to_latex().as_bytes())
    }

    /// Use the LaTeX manuscript at `source` as the article body. A complete document is cut down
    /// to what lies between `\begin{document}` and `\end{document}`, without `\maketitle`, since
    /// the template sets the title block from the metadata. Returns what was done, for the log.
    pub fn import_manuscript(&self, source: &Path) -> Result<String, Error> {
        let text = read_import_text(source)?;
        let (body, note) = extract_body(&text);
        self.write_body(&body)?;
        Ok(format!("Imported {} as {}{note}", source.display(), self.file.body))
    }

    /// Use the front matter read from a Word manuscript as the article's metadata. What the
    /// manuscript gives replaces what the project has; the sample article's own title, authors,
    /// abstract and keywords never survive next to an imported manuscript. The journal's fields
    /// (name, volume, DOI, dates) are not in a manuscript and stay as they are. Saves the project
    /// and rewrites the metadata file. Returns what was taken, for the log.
    pub fn apply_front_matter(&mut self, front: &crate::docx::WordFrontMatter) -> Result<Vec<String>, Error> {
        let sample = PublicationMetadata::sample();
        let m = &mut self.file.metadata;
        let mut taken = Vec::new();
        let mut take = |field: &str, found: bool, is_sample: bool| {
            if found {
                taken.push(field.to_owned());
            }
            found || is_sample
        };
        if take("title", !front.title.is_empty(), m.title == sample.title) {
            m.title = if front.subtitle.is_empty() { front.title.clone() } else { format!("{}: {}", front.title, front.subtitle) };
            m.short_title.clear();
        }
        if take("authors", !front.authors.is_empty(), m.authors == sample.authors) {
            m.authors = front
                .authors
                .iter()
                .map(|a| crate::metadata::Author {
                    name: a.name.clone(),
                    affiliations: a.affiliations.clone(),
                    corresponding: a.corresponding,
                    ..crate::metadata::Author::default()
                })
                .collect();
        }
        if take("affiliations", !front.affiliations.is_empty(), m.affiliations == sample.affiliations) {
            m.affiliations.clone_from(&front.affiliations);
        }
        // An author numbered past the affiliations Word listed would only be a warning; drop it.
        let count = m.affiliations.len();
        for a in &mut m.authors {
            a.affiliations.retain(|&i| i >= 1 && i <= count);
        }
        if take("abstract", !front.abstract_text.is_empty(), m.abstract_text == sample.abstract_text) {
            m.abstract_text.clone_from(&front.abstract_text);
        }
        if take("keywords", !front.keywords.is_empty(), m.keywords == sample.keywords) {
            m.keywords.clone_from(&front.keywords);
        }
        if !front.language.is_empty() && front.language != m.language {
            let english = PublicationMetadata::default();
            let localized = |lang: &str| match lang {
                "zh-hant" => ("研究論文", "本文依創用 CC 姓名標示 4.0 國際授權條款（CC BY 4.0）發布。"),
                "zh-hans" => ("研究论文", "本文依据知识共享署名 4.0 国际许可协议（CC BY 4.0）发布。"),
                "ja" => ("研究論文", "本論文はクリエイティブ・コモンズ 表示 4.0 国際ライセンス（CC BY 4.0）の下で公開されています。"),
                _ => (english.article_type.as_str(), english.licence.as_str()),
            };
            // Only the defaults follow the language; what someone typed stays.
            let defaults: Vec<(&str, &str)> = ["en", "zh-hant", "zh-hans", "ja"].into_iter().map(localized).collect();
            let (article_type, licence) = localized(&front.language);
            if defaults.iter().any(|(t, _)| *t == m.article_type) {
                m.article_type = article_type.to_owned();
            }
            if defaults.iter().any(|(_, l)| *l == m.licence) {
                m.licence = licence.to_owned();
            }
            m.language.clone_from(&front.language);
            taken.push(format!("language ({})", front.language));
        }
        if m.title.trim().is_empty() {
            // The title is required to compile; say where to put it rather than fail later.
            m.title = "Untitled".to_owned();
        }
        self.save()?;
        self.write_metadata()?;
        Ok(taken)
    }

    /// A pdfLaTeX template given a Chinese or Japanese manuscript can't typeset it: switch the
    /// project to XeLaTeX and load xeCJK (with the Noto CJK faces PSG Academic PDF installs)
    /// right after `\documentclass` in the main file. Returns what was done, or `None` when
    /// nothing needed to change.
    pub fn adapt_engine_to_text(&mut self) -> Result<Option<String>, Error> {
        if self.file.engine != crate::Engine::Pdflatex {
            return Ok(None);
        }
        let body = self.read_body()?;
        let m = &self.file.metadata;
        let cjk = |t: &str| t.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF));
        if !(cjk(&body) || cjk(&m.title) || cjk(&m.abstract_text) || m.authors.iter().any(|a| cjk(&a.name)) || m.keywords.iter().any(|k| cjk(k))) {
            return Ok(None);
        }
        let path = self.dir.join(&self.file.main);
        let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
        let masked = crate::template_import::mask_comments(&text);
        let Some(class) = crate::template_import::find_call(&masked, "documentclass", 0, 1) else { return Ok(None) };
        let mut patched = text.clone();
        if !patched.is_char_boundary(class.end) {
            return Ok(None);
        }
        patched.insert_str(class.end, crate::template_import::XECJK_FOR_PDFLATEX);
        self.invalidate_pdf()?;
        write_atomic(&path, patched.as_bytes())?;
        self.file.engine = crate::Engine::Xelatex;
        self.save()?;
        Ok(Some(format!(
            "The manuscript has Chinese or Japanese text, which pdfLaTeX can't set: {} now loads xeCJK and the project is typeset with XeLaTeX.",
            self.file.main
        )))
    }

    /// Copy a bibliography, figure or other supporting file into the project under a safe name.
    /// Returns the name it got (what `\includegraphics` or `\addbibresource` should use).
    pub fn import_asset(&self, source: &Path) -> Result<String, Error> {
        let original = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = original.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
        if !ASSET_EXTENSIONS.contains(&ext.as_str()) {
            return Err(Error::Project(format!("{original}: only {} files can be added", ASSET_EXTENSIONS.join(", "))));
        }
        let name = sanitize_name(&original).ok_or_else(|| Error::Project(format!("\"{original}\" can't be turned into a safe file name")))?;
        let reserved = [&self.file.main, &self.file.body, &self.file.metadata_file];
        if reserved.contains(&&name) || name == PROJECT_FILE {
            return Err(Error::Project(format!("{name} would replace one of the template's own files; rename it first")));
        }
        let bytes = read_import(source)?;
        self.invalidate_pdf()?;
        write_atomic(&self.dir.join(&name), &bytes)?;
        Ok(name)
    }

    pub fn compile_options(&self) -> CompileOptions {
        CompileOptions {
            main: self.file.main.clone(),
            timeout: Duration::from_secs(self.file.settings.timeout_secs),
            force: self.file.settings.force_rebuild,
            needs_biber: self.file.uses_biber,
            engine: self.file.engine,
        }
    }

    /// Write the metadata and compile.
    pub fn compile(&self, env: &TexEnvironment, cancel: Option<&Arc<AtomicBool>>) -> Result<CompileReport, Error> {
        self.invalidate_pdf()?;
        let inputs = crate::project_revision::sources(self).ok();
        self.write_metadata()?;
        let report = compile::compile(env, &self.dir, &self.compile_options(), cancel)?;
        if report.outcome == compile::Outcome::Succeeded
            && report.errors().next().is_none()
            && let Some(inputs) = inputs
            && let Err(e) = crate::project_revision::record(self, inputs)
        {
            log::warn!("could not verify the current PDF for {}: {e}", self.dir.display());
        }
        Ok(report)
    }

    /// The manuscript body (the file an import replaces), as text.
    pub fn read_body(&self) -> Result<String, Error> {
        read_import_text(&self.dir.join(&self.file.body))
    }

    /// Replace the manuscript body with `text` (atomically), as the source editor saves it.
    pub fn write_body(&self, text: &str) -> Result<(), Error> {
        if text.len() as u64 > MAX_IMPORT {
            return Err(Error::Project(format!("the body is larger than {} MB", MAX_IMPORT / 1024 / 1024)));
        }
        self.invalidate_pdf()?;
        write_atomic(&self.dir.join(&self.file.body), text.as_bytes())
    }

    /// The `.bib` files in the project folder, by name.
    pub fn bib_files(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.dir)
            .map(|list| {
                list.flatten()
                    .filter_map(|e| e.file_name().into_string().ok())
                    .filter(|n| n.to_ascii_lowercase().ends_with(".bib") && crate::paths::safe_relative_name(n).is_some())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    /// Every bibliography entry of the project's `.bib` files, with the file each is in, and the
    /// problems found in them and in the manuscript's citations.
    pub fn bibliography(&self) -> (Vec<(String, crate::bib::BibEntry)>, Vec<Diagnostic>) {
        let mut entries = Vec::new();
        let mut parse_issues = Vec::new();
        let mut problems = Vec::new();
        for name in self.bib_files() {
            match read_import_text(&self.dir.join(&name)) {
                Ok(text) => {
                    let (found, issues) = crate::bib::parse(&text);
                    parse_issues.extend(issues.into_iter().map(|i| (name.clone(), i)));
                    entries.extend(found.into_iter().map(|e| (name.clone(), e)));
                }
                Err(e) => problems.push(Diagnostic { severity: Severity::Error, file: Some(name.clone()), line: None, message: e.to_string() }),
            }
        }
        let body = self.read_body().unwrap_or_default();
        let cited = crate::bib::cited_keys(&body);
        // The first body line that mentions a key, for a citation with no entry.
        let cited_on = |key: &str| body.lines().position(|l| l.contains("cite") && l.contains(key)).and_then(|n| u32::try_from(n + 1).ok());
        let plain: Vec<crate::bib::BibEntry> = entries.iter().map(|(_, e)| e.clone()).collect();
        // Issues of the check carry the entry's line; find its file by key (or the parse issue's own).
        let file_of =
            |key: &Option<String>| key.as_ref().and_then(|k| entries.iter().find(|(_, e)| e.key.eq_ignore_ascii_case(k)).map(|(f, _)| f.clone()));
        for (file, issue) in &parse_issues {
            problems.push(Diagnostic { severity: issue.severity, file: Some(file.clone()), line: issue.line, message: issue.message.clone() });
        }
        for issue in crate::bib::check(&plain, Vec::new(), &cited) {
            let (file, line) = match issue.line {
                None => (Some(self.file.body.clone()), issue.key.as_deref().and_then(cited_on)),
                Some(line) => (file_of(&issue.key), Some(line)),
            };
            problems.push(Diagnostic { severity: issue.severity, file, line, message: issue.message });
        }
        (entries, problems)
    }

    /// Problems to fix before compiling: writing systems the pdfLaTeX templates can't set (in the
    /// body and the metadata), and bibliography problems when the template uses Biber.
    pub fn preflight(&self) -> Vec<Diagnostic> {
        let mut out = Vec::new();
        match self.read_body() {
            Ok(body) => {
                for f in crate::scripts::scan(&body, &self.file.body, true).into_iter().filter(|f| !self.file.engine.sets(f.script)) {
                    out.push(Diagnostic { severity: f.severity(), file: Some(self.file.body.clone()), line: f.line, message: f.summary() });
                }
            }
            Err(e) => out.push(Diagnostic { severity: Severity::Error, file: Some(self.file.body.clone()), line: None, message: e.to_string() }),
        }
        let m = &self.file.metadata;
        let mut fields: Vec<(String, &str)> = vec![
            ("title".into(), m.title.as_str()),
            ("short title".into(), m.short_title.as_str()),
            ("journal".into(), m.journal.as_str()),
            ("abstract".into(), m.abstract_text.as_str()),
        ];
        fields.extend(m.keywords.iter().map(|k| ("keywords".to_owned(), k.as_str())));
        fields.extend(m.authors.iter().enumerate().map(|(i, a)| (format!("author {}", i + 1), a.name.as_str())));
        fields.extend(m.affiliations.iter().enumerate().map(|(i, a)| (format!("affiliation {}", i + 1), a.as_str())));
        let mut seen = std::collections::HashSet::new();
        for (field, value) in fields {
            for f in crate::scripts::scan(value, &format!("metadata: {field}"), false).into_iter().filter(|f| !self.file.engine.sets(f.script)) {
                // One finding per script and field name (keywords and authors come in lists).
                if seen.insert((f.script, field.clone())) {
                    out.push(Diagnostic { severity: f.severity(), file: None, line: None, message: f.message() });
                }
            }
        }
        if self.file.uses_biber {
            out.extend(self.bibliography().1);
        }
        out
    }

    /// The compiled PDF, if there is one.
    pub fn pdf_path(&self) -> PathBuf {
        self.dir.join(format!("{}.pdf", self.file.main.trim_end_matches(".tex")))
    }

    /// Whether the PDF belongs to the current saved inputs of a successful compilation.
    pub fn pdf_is_current(&self) -> bool {
        crate::project_revision::is_current(self)
    }

    /// Remove the certification before changing inputs outside the project methods.
    pub fn invalidate_pdf(&self) -> Result<(), Error> {
        crate::project_revision::forget(self)
    }

    /// Copy the compiled PDF to `dest` (atomically).
    pub fn export_pdf(&self, dest: &Path) -> Result<(), Error> {
        let pdf = self.pdf_path();
        let bytes = std::fs::read(&pdf).map_err(|e| Error::io(&pdf, e))?;
        if !bytes.starts_with(b"%PDF-") {
            return Err(Error::Project(format!("{} is not a PDF", pdf.display())));
        }
        write_atomic(dest, &bytes)
    }
}

fn read_import(source: &Path) -> Result<Vec<u8>, Error> {
    let meta = std::fs::metadata(source).map_err(|e| Error::io(source, e))?;
    if !meta.is_file() {
        return Err(Error::Project(format!("{} is not a file", source.display())));
    }
    if meta.len() > MAX_IMPORT {
        return Err(Error::Project(format!("{} is larger than {} MB", source.display(), MAX_IMPORT / 1024 / 1024)));
    }
    std::fs::read(source).map_err(|e| Error::io(source, e))
}

fn read_import_text(source: &Path) -> Result<String, Error> {
    let bytes = read_import(source)?;
    let text =
        String::from_utf8(bytes).map_err(|_| Error::Project(format!("{} is not UTF-8 text; save it as UTF-8 and try again", source.display())))?;
    Ok(text.strip_prefix('\u{feff}').map(str::to_owned).unwrap_or(text))
}

/// The body of a LaTeX manuscript, and a note on what was removed.
pub fn extract_body(text: &str) -> (String, String) {
    let Some(begin) = text.find("\\begin{document}") else {
        return (text.to_owned(), String::new());
    };
    let start = begin + "\\begin{document}".len();
    let end = text.get(start..).and_then(|rest| rest.find("\\end{document}")).map_or(text.len(), |e| start + e);
    let inner = text.get(start..end).unwrap_or_default();
    let mut removed = Vec::new();
    let mut body = String::with_capacity(inner.len());
    for line in inner.lines() {
        let t = line.trim_start();
        if t.starts_with("\\maketitle") {
            removed.push("\\maketitle");
            continue;
        }
        if t.starts_with("\\printbibliography") {
            removed.push("\\printbibliography");
            continue;
        }
        body.push_str(line);
        body.push('\n');
    }
    let mut note = " (the preamble was dropped: the template provides it".to_owned();
    if !removed.is_empty() {
        note.push_str(&format!("; removed {}", removed.join(", ")));
    }
    note.push(')');
    (body, note)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::unwrap_used)]
    fn new_project() -> (tempfile::TempDir, Project) {
        let root = tempfile::tempdir().unwrap();
        let t = Template::builtin().remove(0);
        let p = Project::create(&root.path().join("p"), &t, PublicationMetadata::sample()).unwrap();
        (root, p)
    }

    #[test]
    fn create_copies_the_template_and_saves() {
        let (_root, p) = new_project();
        for f in ["main.tex", "psg-article.cls", "body.tex", "references.bib", "psg-metadata.tex", PROJECT_FILE] {
            assert!(p.dir.join(f).is_file(), "{f}");
        }
        let again = Project::open(&p.dir).unwrap();
        assert_eq!(again.file, p.file);
        let meta = std::fs::read_to_string(p.dir.join("psg-metadata.tex")).unwrap();
        assert!(meta.contains("\\psgTitle{A Sample Article"));
    }

    #[test]
    fn create_refuses_a_folder_with_files() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("x"), "").unwrap();
        assert!(Project::create(root.path(), &Template::builtin().remove(0), PublicationMetadata::default()).is_err());
    }

    #[test]
    fn importing_a_full_document_keeps_its_body() {
        let (root, p) = new_project();
        let src = root.path().join("paper.tex");
        std::fs::write(
            &src,
            "\\documentclass{article}\n\\usepackage{x}\n\\begin{document}\n\\maketitle\nHello.\n\\printbibliography\n\\end{document}\nafter\n",
        )
        .unwrap();
        let note = p.import_manuscript(&src).unwrap();
        assert!(note.contains("\\maketitle"));
        assert_eq!(std::fs::read_to_string(p.dir.join("body.tex")).unwrap().trim(), "Hello.");
    }

    #[test]
    fn assets_get_safe_names_and_cannot_replace_template_files() {
        let (root, p) = new_project();
        let fig = root.path().join("Figure 1.PNG");
        std::fs::write(&fig, b"png").unwrap();
        assert_eq!(p.import_asset(&fig).unwrap(), "Figure_1.PNG");
        let body = root.path().join("body.tex");
        std::fs::write(&body, "x").unwrap();
        assert!(p.import_asset(&body).is_err());
        let rc = root.path().join("latexmkrc");
        std::fs::write(&rc, "system('x')").unwrap();
        assert!(p.import_asset(&rc).is_err(), "no extension, not importable");
    }

    #[test]
    fn blocking_metadata_is_not_written() {
        let (_root, mut p) = new_project();
        p.file.metadata.title.clear();
        assert!(matches!(p.write_metadata(), Err(Error::Metadata(_))));
    }

    #[test]
    fn preflight_names_cjk_text_and_missing_citations() {
        let (_root, mut p) = new_project();
        assert!(p.preflight().iter().all(|d| d.severity != crate::Severity::Error), "the sample project is clean: {:?}", p.preflight());
        p.write_body("Intro.\n\n张三 argues \\parencite{nobody2020}.\n").unwrap();
        p.file.metadata.title = "论文 Title".into();
        assert_eq!(p.file.engine, crate::Engine::Xelatex, "the PSG templates set Chinese with XeLaTeX");
        assert!(!p.preflight().iter().any(|d| d.to_string().contains("Chinese")), "XeLaTeX sets Chinese: {:?}", p.preflight());
        p.file.engine = crate::Engine::Pdflatex;
        let found = p.preflight();
        let text: Vec<String> = found.iter().map(ToString::to_string).collect();
        assert!(text.iter().any(|t| t.starts_with("body.tex:3: error: 2 characters in Chinese")), "{text:?}");
        assert!(text.iter().any(|t| t.contains("metadata: title: 2 characters in Chinese")), "{text:?}");
        assert!(text.iter().any(|t| t.contains("\"nobody2020\" is cited but no bibliography entry")), "{text:?}");
        assert_eq!(p.read_body().unwrap(), "Intro.\n\n张三 argues \\parencite{nobody2020}.\n");
        let (entries, _) = p.bibliography();
        assert!(!entries.is_empty() && entries.iter().all(|(f, _)| f == "references.bib"));
    }

    #[test]
    fn word_front_matter_replaces_the_sample_article() {
        use crate::docx::{WordAuthor, WordFrontMatter};
        let (_root, mut p) = new_project();
        let front = WordFrontMatter {
            title: "數位人文研究".into(),
            authors: vec![WordAuthor { name: "王小明".into(), affiliations: vec![1, 3], corresponding: true }],
            affiliations: vec!["國立臺灣大學".into()],
            keywords: vec!["數位人文".into()],
            language: "zh-hant".into(),
            ..WordFrontMatter::default()
        };
        let taken = p.apply_front_matter(&front).unwrap();
        assert_eq!(taken, ["title", "authors", "affiliations", "keywords", "language (zh-hant)"]);
        let m = &p.file.metadata;
        assert_eq!(m.title, "數位人文研究");
        assert_eq!(m.authors.len(), 1);
        assert_eq!(m.authors[0].affiliations, [1], "an affiliation Word did not list is dropped");
        assert!(m.abstract_text.is_empty(), "the sample abstract must not stay next to an imported article");
        assert_eq!(m.article_type, "研究論文");
        assert!(m.licence.contains("CC BY 4.0") && m.licence.contains("創用"));
        assert_eq!(m.journal, PublicationMetadata::sample().journal, "journal fields are not in a manuscript");
        let written = std::fs::read_to_string(p.dir.join(&p.file.metadata_file)).unwrap();
        assert!(written.contains("\\providecommand\\psgLanguage[1]{}\n") && written.contains("\\psgLanguage{zh-hant}"), "{written}");
        assert!(written.contains("\\psgCorrespondence{}"), "no e-mail, no correspondence line");
        assert_eq!(Project::open(&p.dir).unwrap().file.metadata, p.file.metadata);

        // What someone typed stays when a later manuscript has no front matter.
        p.file.metadata.abstract_text = "Typed by hand.".into();
        p.apply_front_matter(&WordFrontMatter { language: "zh-hant".into(), ..WordFrontMatter::default() }).unwrap();
        assert_eq!(p.file.metadata.abstract_text, "Typed by hand.");
        assert_eq!(p.file.metadata.title, "數位人文研究");
    }

    #[test]
    fn a_pdflatex_template_given_chinese_switches_to_xelatex() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("t");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join("template.toml"), "id = \"x\"\nname = \"X\"\nengine = \"pdflatex\"\nbibliography = \"none\"\nmain = \"main.tex\"\nbody = \"body.tex\"\nmetadata = \"m.tex\"\nfiles = [\"main.tex\", \"body.tex\", \"m.tex\"]\n").unwrap();
        std::fs::write(
            dir.join("main.tex"),
            "% \\documentclass{book}\n\\documentclass[11pt]{article}\n\\usepackage[T1]{fontenc}\n\\begin{document}\\input{body}\\end{document}\n",
        )
        .unwrap();
        std::fs::write(dir.join("body.tex"), "").unwrap();
        std::fs::write(dir.join("m.tex"), "").unwrap();
        let t = Template::from_dir(&dir).unwrap();
        let mut p = Project::create(&root.path().join("p"), &t, PublicationMetadata::sample()).unwrap();
        assert_eq!(p.adapt_engine_to_text().unwrap(), None, "English stays with pdfLaTeX");
        p.write_body("中文正文。\n").unwrap();
        assert!(p.adapt_engine_to_text().unwrap().is_some());
        assert_eq!(Project::open(&p.dir).unwrap().file.engine, crate::Engine::Xelatex);
        let main = std::fs::read_to_string(p.dir.join("main.tex")).unwrap();
        assert!(main.starts_with("% \\documentclass{book}\n\\documentclass[11pt]{article}\n%% Added by PSG Academic PDF"), "{main}");
        assert!(main.contains("\\usepackage{xeCJK}\n"));
        assert_eq!(p.adapt_engine_to_text().unwrap(), None, "done once");
    }

    #[test]
    fn export_needs_a_pdf() {
        let (root, p) = new_project();
        assert!(p.export_pdf(&root.path().join("out.pdf")).is_err());
        std::fs::write(p.pdf_path(), b"%PDF-1.5\n").unwrap();
        p.export_pdf(&root.path().join("out.pdf")).unwrap();
        assert_eq!(std::fs::read(root.path().join("out.pdf")).unwrap(), b"%PDF-1.5\n");
    }
}
