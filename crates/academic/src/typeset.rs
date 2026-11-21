//! One step from a manuscript to a PDF: a new project from a template, the manuscript imported
//! (a Word `.docx` through Pandoc, or LaTeX), the journal's metadata laid over what the
//! manuscript gave, the document typeset (installing the TeX packages it needs) and the PDF
//! exported. What happened comes back as a [`Report`] that serialises to JSON, for scripts and
//! agents; it is returned even when a step fails, with the step and the reason.

use std::path::{Path, PathBuf};
use std::time::Instant;

use serde::Serialize;

use crate::managed::ManagedTex;
use crate::pandoc::{ManuscriptImporter, PandocImporter};
use crate::{Diagnostic, Outcome, Project, PublicationMetadata, Template, TexEnvironment};

/// What to typeset.
#[derive(Debug, Clone)]
pub struct Request {
    /// The manuscript: `.docx` or `.tex`.
    pub manuscript: PathBuf,
    pub template: Template,
    /// The project folder to create (it must not exist or be empty).
    pub project_dir: PathBuf,
    /// A TOML or JSON file of metadata fields laid over what the manuscript gave.
    pub metadata: Option<PathBuf>,
    /// Where to copy the PDF.
    pub out: Option<PathBuf>,
    /// The compilation time limit, in seconds.
    pub timeout_secs: Option<u64>,
}

/// One problem, as JSON shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Problem {
    #[serde(flatten)]
    pub diagnostic: Diagnostic,
    pub hint: Option<&'static str>,
}

/// What happened.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Report {
    pub ok: bool,
    /// The step that failed: `project`, `import`, `metadata`, `compile` or `export`.
    pub failed_step: Option<&'static str>,
    pub error: Option<String>,
    pub template: String,
    pub engine: String,
    pub project: String,
    /// The exported PDF, or the project's PDF without `out`.
    pub pdf: Option<String>,
    /// The import's notes, one per line; lines starting "To fix before compiling" need a person.
    pub import: Vec<String>,
    /// The metadata fields the metadata file set.
    pub metadata_set: Vec<String>,
    /// The metadata the PDF was typeset with.
    pub metadata: Option<PublicationMetadata>,
    pub installed_packages: Vec<String>,
    /// `succeeded`, `failed`, `timed_out` or `cancelled`.
    pub outcome: Option<&'static str>,
    pub seconds: f64,
    pub problems: Vec<Problem>,
}

impl Report {
    fn fail(mut self, step: &'static str, error: impl ToString, started: Instant) -> Report {
        self.ok = false;
        self.failed_step = Some(step);
        self.error = Some(error.to_string());
        self.seconds = started.elapsed().as_secs_f64();
        self
    }
}

/// Typeset `req.manuscript` with `req.template`. `pandoc` is needed for `.docx`.
pub fn typeset(req: &Request, env: &TexEnvironment, tex: Option<&ManagedTex>, pandoc: Option<&Path>) -> Report {
    let started = Instant::now();
    let mut report = Report {
        template: req.template.manifest.id.clone(),
        engine: req.template.manifest.engine().title().to_owned(),
        project: req.project_dir.display().to_string(),
        ..Report::default()
    };
    let mut project = match Project::create(&req.project_dir, &req.template, PublicationMetadata::sample()) {
        Ok(p) => p,
        Err(e) => return report.fail("project", e, started),
    };
    let is_docx = req.manuscript.extension().is_some_and(|e| e.eq_ignore_ascii_case("docx"));
    let note = if is_docx {
        let Some(pandoc) = pandoc else {
            return report.fail("import", "Pandoc is not installed, so a .docx manuscript can't be read; install Pandoc 2.15 or newer", started);
        };
        PandocImporter { pandoc: pandoc.to_path_buf() }.import(&req.manuscript, &mut project, None)
    } else {
        project.import_manuscript(&req.manuscript).and_then(|note| {
            let adapted = project.adapt_engine_to_text()?;
            Ok(adapted.map_or(note.clone(), |a| format!("{note}\n{a}")))
        })
    };
    match note {
        Ok(note) => report.import = note.lines().map(str::to_owned).collect(),
        Err(e) => return report.fail("import", e, started),
    }
    if let Some(path) = &req.metadata {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => return report.fail("metadata", format!("{}: {e}", path.display()), started),
        };
        match project.file.metadata.overlay(&text) {
            Ok(set) => report.metadata_set = set,
            Err(e) => return report.fail("metadata", format!("{}: {e}", path.display()), started),
        }
        if let Err(e) = project.save().and_then(|()| project.write_metadata()) {
            return report.fail("metadata", e, started);
        }
        // The file may have brought the first Chinese or Japanese text (a title, say).
        match project.adapt_engine_to_text() {
            Ok(Some(note)) => report.import.push(note),
            Ok(None) => {}
            Err(e) => return report.fail("metadata", e, started),
        }
    }
    // The template's own volume, issue, pages and DOI are the last article's: say which ones
    // this article still has, unless a metadata file set them.
    let main = project.dir.join(&project.file.main);
    if let Ok(text) = std::fs::read_to_string(&main) {
        let kept: Vec<String> = crate::article_values::find(&text)
            .into_iter()
            .filter(|f| !report.metadata_set.iter().any(|set| set == f.field.key()))
            .filter_map(|f| {
                let typed = text.get(f.from..f.to)?.trim();
                Some(format!("{} {typed}", f.field.label()))
            })
            .collect();
        if !kept.is_empty() {
            report.import.push(format!(
                "Still the template's own, from the article it was made from: {}. Set this article's in the metadata (volume, issue, year, pages, doi).",
                kept.join(", ")
            ));
        }
    }
    report.engine = project.file.engine.title().to_owned();
    if let Some(secs) = req.timeout_secs {
        project.file.settings.timeout_secs = secs;
        if let Err(e) = project.save() {
            return report.fail("project", e, started);
        }
    }
    report.metadata = Some(project.file.metadata.clone());
    let (compiled, installed) = match crate::managed::compile_completing(&project, env, tex, None) {
        Ok(r) => r,
        Err(e) => return report.fail("compile", e, started),
    };
    report.installed_packages = installed;
    report.problems = compiled.diagnostics.iter().map(|d| Problem { diagnostic: d.clone(), hint: d.hint() }).collect();
    report.outcome = Some(match compiled.outcome {
        Outcome::Succeeded => "succeeded",
        Outcome::Failed => "failed",
        Outcome::TimedOut => "timed_out",
        Outcome::Cancelled => "cancelled",
    });
    if compiled.outcome != Outcome::Succeeded {
        let why = compiled.errors().next().map_or_else(|| compiled.summary(), ToString::to_string);
        return report.fail("compile", why, started);
    }
    let pdf = match &req.out {
        Some(out) => match project.export_pdf(out) {
            Ok(()) => out.clone(),
            Err(e) => return report.fail("export", e, started),
        },
        None => project.pdf_path(),
    };
    report.pdf = Some(pdf.display().to_string());
    report.ok = true;
    report.seconds = started.elapsed().as_secs_f64();
    report
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn request(root: &Path, manuscript: &str) -> Request {
        Request {
            manuscript: root.join(manuscript),
            template: Template::builtin().remove(0),
            project_dir: root.join("p"),
            metadata: None,
            out: None,
            timeout_secs: None,
        }
    }

    #[test]
    fn a_docx_without_pandoc_fails_at_the_import_with_the_reason() {
        let root = tempfile::tempdir().unwrap();
        let r = typeset(&request(root.path(), "a.docx"), &TexEnvironment::detect_in(None, false), None, None);
        assert!(!r.ok);
        assert_eq!(r.failed_step, Some("import"));
        assert!(r.error.unwrap().contains("Pandoc"));
        let json = serde_json::to_value(typeset(&request(root.path(), "b.docx"), &TexEnvironment::detect_in(None, false), None, None)).unwrap();
        assert_eq!(json["failed_step"], "project", "the project folder is no longer empty");
    }

    #[test]
    fn chinese_from_the_metadata_file_switches_a_pdflatex_template_to_xelatex() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("m.tex"), "\\section{A}\nText.\n").unwrap();
        std::fs::write(root.path().join("meta.toml"), "title = \"研究\"\n").unwrap();
        let mut req = request(root.path(), "m.tex");
        // A journal's pdfLaTeX template, as the built-in one would be on pdfLaTeX.
        req.template.manifest.engine = "pdflatex".into();
        req.metadata = Some(root.path().join("meta.toml"));
        let r = typeset(&req, &TexEnvironment::detect_in(None, false), None, None);
        assert_eq!(r.engine, "XeLaTeX", "{r:?}");
        assert!(r.import.iter().any(|n| n.contains("XeLaTeX")), "{:?}", r.import);
    }

    #[test]
    fn a_bad_metadata_file_is_reported_before_compiling() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("m.tex"), "\\section{A}\nText.\n").unwrap();
        std::fs::write(root.path().join("meta.toml"), "volumn = \"2\"\n").unwrap();
        let mut req = request(root.path(), "m.tex");
        req.metadata = Some(root.path().join("meta.toml"));
        let r = typeset(&req, &TexEnvironment::detect_in(None, false), None, None);
        assert_eq!(r.failed_step, Some("metadata"), "{:?}", r.error);
        assert!(r.error.unwrap().contains("\"volumn\" is not a metadata field"));
        assert_eq!(r.import.len(), 1);
    }
}
