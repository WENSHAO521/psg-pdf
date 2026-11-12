//! Finding the local TeX installation: pdflatex, latexmk, biber and (optionally) pandoc.
//!
//! Nothing is downloaded or installed: PSG Academic PDF uses what TeX Live (or MiKTeX) put on
//! the `PATH`, and says plainly what is missing.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use crate::process;

/// One external program and what was found for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    /// The program's name (`pdflatex`).
    pub name: &'static str,
    /// Where it was found on the search path.
    pub path: Option<PathBuf>,
    /// The first line of its `--version` output.
    pub version: Option<String>,
}

impl Tool {
    pub fn found(&self) -> bool {
        self.path.is_some()
    }

    fn locate(name: &'static str, search_path: Option<&OsStr>, with_version: bool) -> Self {
        let path = find_program(name, search_path);
        let version = if with_version { path.as_deref().and_then(process::version_line) } else { None };
        Self { name, path, version }
    }
}

/// The programs a compilation needs, as found on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TexEnvironment {
    pub pdflatex: Tool,
    pub latexmk: Tool,
    pub biber: Tool,
    /// Optional: converts DOCX manuscripts (see [`crate::pandoc`]).
    pub pandoc: Tool,
}

impl TexEnvironment {
    /// Look for the programs on the process's `PATH` and ask each for its version.
    pub fn detect() -> Self {
        Self::detect_in(std::env::var_os("PATH").as_deref(), true)
    }

    /// Look for the programs on `search_path` (a `PATH`-style list). `with_versions` runs each
    /// program found with `--version`.
    pub fn detect_in(search_path: Option<&OsStr>, with_versions: bool) -> Self {
        Self {
            pdflatex: Tool::locate("pdflatex", search_path, with_versions),
            latexmk: Tool::locate("latexmk", search_path, with_versions),
            biber: Tool::locate("biber", search_path, with_versions),
            pandoc: Tool::locate("pandoc", search_path, with_versions),
        }
    }

    /// The required programs that were not found (pandoc is optional and never listed).
    pub fn missing(&self, needs_biber: bool) -> Vec<&'static str> {
        let mut missing = Vec::new();
        for tool in [&self.pdflatex, &self.latexmk] {
            if !tool.found() {
                missing.push(tool.name);
            }
        }
        if needs_biber && !self.biber.found() {
            missing.push(self.biber.name);
        }
        missing
    }

    /// The directory holding pdflatex, which latexmk must find first on its `PATH`.
    pub fn tex_bin_dir(&self) -> Option<&Path> {
        self.pdflatex.path.as_deref().and_then(Path::parent)
    }
}

/// The names a program may have on this platform.
fn candidates(name: &str) -> Vec<String> {
    if cfg!(windows) { ["exe", "bat", "cmd"].iter().map(|ext| format!("{name}.{ext}")).collect() } else { vec![name.to_owned()] }
}

/// Find `name` in the directories of `search_path`. Relative directories are skipped, so a
/// document folder that happens to be the working directory can't supply a fake `pdflatex`.
pub fn find_program(name: &str, search_path: Option<&OsStr>) -> Option<PathBuf> {
    let search_path = search_path?;
    for dir in std::env::split_paths(search_path) {
        if !dir.is_absolute() {
            continue;
        }
        for file in candidates(name) {
            let candidate = dir.join(file);
            if is_executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[allow(clippy::unwrap_used)]
    fn fake_program(dir: &Path, name: &str) {
        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\necho \"fake $0 1.0\"\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn finds_programs_on_the_search_path_with_their_versions() {
        let dir = tempfile::tempdir().unwrap();
        fake_program(dir.path(), "pdflatex");
        fake_program(dir.path(), "latexmk");
        let env = TexEnvironment::detect_in(Some(dir.path().as_os_str()), true);
        assert!(env.pdflatex.found());
        assert!(env.pdflatex.version.as_deref().is_some_and(|v| v.contains("1.0")));
        assert!(!env.biber.found());
        assert_eq!(env.missing(true), vec!["biber"]);
        assert!(env.missing(false).is_empty());
        assert_eq!(env.tex_bin_dir(), Some(dir.path()));
    }

    #[test]
    fn nothing_found_without_a_search_path() {
        let env = TexEnvironment::detect_in(None, false);
        assert_eq!(env.missing(true), vec!["pdflatex", "latexmk", "biber"]);
    }

    #[test]
    fn relative_directories_and_non_executables_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("latexmk"), "not executable").unwrap();
        let search = std::env::join_paths([PathBuf::from("relative/bin"), dir.path().to_path_buf()]).unwrap();
        assert_eq!(find_program("latexmk", Some(&search)), None);
    }
}
