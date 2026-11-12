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
        Ok(format!("Converted {} with Pandoc into {} ({warnings} warning line(s)); figures are in media/.", source.display(), project.file.body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::PublicationMetadata;
    use crate::template::Template;

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
