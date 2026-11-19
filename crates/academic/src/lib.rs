//! psg-academic — the typesetting core of PSG Academic PDF.
//!
//! PSG Academic PDF typesets *accepted* articles for publication: a journal template, the
//! publication metadata and the author's manuscript go in, a PDF comes out. Everything runs on
//! this machine with the local TeX installation (pdfLaTeX through latexmk, with BibLaTeX and
//! Biber); nothing is uploaded.
//!
//! - [`managed`] installs, updates and completes PSG's own TeX (TinyTeX), so nobody installs TeX.
//! - [`tex`] finds pdflatex, latexmk, biber and pandoc.
//! - [`compile`] runs latexmk in the project directory with shell escape off, kpathsea's paranoid
//!   file access, a time limit and cancellation, and turns the logs into [`diagnostics`].
//! - [`template`] reads journal templates; [`metadata`] writes the metadata file they read.
//! - [`bib`] checks the bibliography against the citations; [`scripts`] finds text the
//!   pdfLaTeX templates can't set (Chinese, Japanese, Korean, …) and says what to do.
//! - [`project`] keeps it together in one directory; [`pandoc`] is the (experimental) DOCX import.
//!
//! This crate has no user interface; the app is `apps/psg-academic-pdf`.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod bib;

/// Marks a sentence the apps show in the interface language (the hints under a diagnostic, the
/// advice for a script pdfLaTeX can't set). It returns `english` unchanged: the English is the
/// catalog key, the app translates it when it draws it, and the translation tests find every
/// marked string in the source. This crate itself never translates anything.
pub const fn mark(english: &'static str) -> &'static str {
    english
}
pub mod compile;
pub mod diagnostics;
pub mod managed;
pub mod metadata;
pub mod pandoc;
pub mod paths;
pub mod process;
pub mod project;
mod project_revision;
pub mod scripts;
pub mod template;
pub mod tex;

use std::path::Path;

pub use compile::{CompileOptions, CompileReport, Outcome};
pub use diagnostics::{Diagnostic, Severity};
pub use metadata::{Author, PublicationMetadata};
pub use project::Project;
pub use template::Template;
pub use tex::TexEnvironment;

/// Everything that can go wrong, with a message a person can act on.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error(
        "TeX is not ready: {0} not found. PSG Academic PDF installs its own TeX; wait for it to finish (Settings ▸ TeX installation), or press Install again there."
    )]
    TexMissing(String),
    #[error("{0}")]
    Template(String),
    #[error("{0}")]
    Project(String),
    #[error("metadata: {0}")]
    Metadata(String),
    #[error("{0}")]
    Compile(String),
}

impl Error {
    pub(crate) fn io(path: &Path, source: std::io::Error) -> Self {
        Error::Io { path: path.display().to_string(), source }
    }
}
