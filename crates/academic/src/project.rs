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
        write_atomic(&self.dir.join(&self.file.body), body.as_bytes())?;
        Ok(format!("Imported {} as {}{note}", source.display(), self.file.body))
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
        write_atomic(&self.dir.join(&name), &bytes)?;
        Ok(name)
    }

    pub fn compile_options(&self) -> CompileOptions {
        CompileOptions {
            main: self.file.main.clone(),
            timeout: Duration::from_secs(self.file.settings.timeout_secs),
            force: self.file.settings.force_rebuild,
            needs_biber: self.file.uses_biber,
        }
    }

    /// Write the metadata and compile.
    pub fn compile(&self, env: &TexEnvironment, cancel: Option<&Arc<AtomicBool>>) -> Result<CompileReport, Error> {
        self.write_metadata()?;
        compile::compile(env, &self.dir, &self.compile_options(), cancel)
    }

    /// The compiled PDF, if there is one.
    pub fn pdf_path(&self) -> PathBuf {
        self.dir.join(format!("{}.pdf", self.file.main.trim_end_matches(".tex")))
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
    fn export_needs_a_pdf() {
        let (root, p) = new_project();
        assert!(p.export_pdf(&root.path().join("out.pdf")).is_err());
        std::fs::write(p.pdf_path(), b"%PDF-1.5\n").unwrap();
        p.export_pdf(&root.path().join("out.pdf")).unwrap();
        assert_eq!(std::fs::read(root.path().join("out.pdf")).unwrap(), b"%PDF-1.5\n");
    }
}
