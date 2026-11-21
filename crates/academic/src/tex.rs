//! Finding the local TeX installation: pdflatex, latexmk, biber and (optionally) pandoc.
//!
//! PSG's own TeX ([`crate::managed`]), which the app installs itself, comes first; until it is
//! installed, a TeX Live (or MiKTeX) already on the `PATH` is used. Pandoc is only ever looked
//! for on the `PATH`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::process;

fn version_probe(name: &str) -> (&'static str, Duration) {
    match name {
        "latexmk" => ("-v", process::VERSION_TIMEOUT),
        // Standalone Biber unpacks its Perl runtime into a cache on first start. Killing
        // the version check mid-extraction can make the next check start from scratch.
        "biber" => ("--version", Duration::from_secs(120)),
        _ => ("--version", process::VERSION_TIMEOUT),
    }
}

/// One external program and what was found for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tool {
    /// The program's name (`pdflatex`).
    pub name: &'static str,
    /// Where it was found on the search path.
    pub path: Option<PathBuf>,
    /// The first line of its `--version` output.
    pub version: Option<String>,
    /// Why a located program failed its version check.
    pub probe_error: Option<String>,
}

impl Tool {
    /// Whether the program was located and passed its version check, if requested.
    pub fn found(&self) -> bool {
        self.path.is_some() && self.probe_error.is_none()
    }

    fn locate(name: &'static str, search_path: Option<&OsStr>, with_version: bool) -> Self {
        let path = find_program(name, search_path);
        let (version, probe_error) = match path.as_deref().filter(|_| with_version) {
            Some(program) => {
                let (flag, timeout) = version_probe(name);
                match process::probe_version(program, flag, timeout) {
                    Ok(version) => (version, None),
                    Err(error) => (None, Some(error)),
                }
            }
            None => (None, None),
        };
        Self { name, path, version, probe_error }
    }
}

/// The programs a compilation needs, as found on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TexEnvironment {
    pub pdflatex: Tool,
    /// For templates typeset with XeLaTeX ([`crate::Engine::Xelatex`]).
    pub xelatex: Tool,
    pub latexmk: Tool,
    pub biber: Tool,
    /// Optional: converts DOCX manuscripts (see [`crate::pandoc`]).
    pub pandoc: Tool,
    /// pdflatex is PSG's own TeX ([`crate::managed`]), not one found on the `PATH`.
    pub managed: bool,
    /// Folders XeLaTeX looks in for fonts by file name: the CJK fonts PSG installs
    /// ([`crate::managed::ManagedTex::fonts_dir`]). Set by the caller.
    pub font_dirs: Vec<PathBuf>,
}

impl TexEnvironment {
    /// Look for the programs on the process's `PATH` and ask each for its version.
    pub fn detect() -> Self {
        Self::detect_with(None)
    }

    /// Look in PSG's own TeX programs folder `managed_bin` first, then on the `PATH`.
    pub fn detect_with(managed_bin: Option<&Path>) -> Self {
        let path = std::env::var_os("PATH");
        let Some(bin) = managed_bin.filter(|b| b.is_dir()) else { return Self::detect_in(path.as_deref(), true) };
        let mut dirs = vec![bin.to_path_buf()];
        if let Some(path) = &path {
            dirs.extend(std::env::split_paths(path));
        }
        let joined = std::env::join_paths(dirs).ok();
        let mut env = Self::detect_in(joined.as_deref().or(path.as_deref()), true);
        // Pandoc is never part of PSG's TeX.
        env.pandoc = Tool::locate("pandoc", path.as_deref(), true);
        env.managed = env.tex_bin_dir() == Some(bin);
        env
    }

    /// Look for the programs on `search_path` (a `PATH`-style list). `with_versions` runs each
    /// program found with `--version`.
    pub fn detect_in(search_path: Option<&OsStr>, with_versions: bool) -> Self {
        Self {
            pdflatex: Tool::locate("pdflatex", search_path, with_versions),
            xelatex: Tool::locate("xelatex", search_path, with_versions),
            latexmk: Tool::locate("latexmk", search_path, with_versions),
            biber: Tool::locate("biber", search_path, with_versions),
            pandoc: Tool::locate("pandoc", search_path, with_versions),
            managed: false,
            font_dirs: Vec::new(),
        }
    }

    /// [`Self::missing`] for typesetting with `engine`.
    pub fn missing_for(&self, engine: crate::Engine, needs_biber: bool) -> Vec<&'static str> {
        let mut missing = self.missing(needs_biber);
        if engine == crate::Engine::Xelatex && !self.xelatex.found() {
            missing.insert(0, self.xelatex.name);
        }
        missing
    }

    /// The required programs that were missing or failed their version check.
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

#[cfg(test)]
#[test]
fn biber_has_time_to_unpack_before_the_version_check_expires() {
    assert_eq!(version_probe("biber"), ("--version", Duration::from_secs(120)));
    assert_eq!(version_probe("pdflatex"), ("--version", Duration::from_secs(15)));
    assert_eq!(version_probe("latexmk"), ("-v", Duration::from_secs(15)));
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
    fn psg_tex_comes_before_the_path() {
        let managed = tempfile::tempdir().unwrap();
        fake_program(managed.path(), "pdflatex");
        fake_program(managed.path(), "latexmk");
        let env = TexEnvironment::detect_with(Some(managed.path()));
        assert!(env.managed);
        assert_eq!(env.tex_bin_dir(), Some(managed.path()));
        assert!(!TexEnvironment::detect_with(Some(&managed.path().join("missing"))).managed);
    }

    #[test]
    fn nothing_found_without_a_search_path() {
        let env = TexEnvironment::detect_in(None, false);
        assert_eq!(env.missing(true), vec!["pdflatex", "latexmk", "biber"]);
    }

    #[test]
    fn a_launcher_that_reports_an_error_does_not_make_tex_ready() {
        let dir = tempfile::tempdir().unwrap();
        fake_program(dir.path(), "pdflatex");
        fake_program(dir.path(), "latexmk");
        std::fs::write(dir.path().join("latexmk"), "#!/bin/sh\necho 'MiKTeX could not find Perl' >&2\nexit 1\n").unwrap();
        let env = TexEnvironment::detect_in(Some(dir.path().as_os_str()), true);
        assert!(env.latexmk.path.is_some());
        assert_eq!(env.latexmk.version, None);
        assert_eq!(env.latexmk.probe_error.as_deref(), Some("MiKTeX could not find Perl"));
        assert_eq!(env.missing(false), vec!["latexmk"]);
    }

    #[test]
    fn relative_directories_and_non_executables_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("latexmk"), "not executable").unwrap();
        let search = std::env::join_paths([PathBuf::from("relative/bin"), dir.path().to_path_buf()]).unwrap();
        assert_eq!(find_program("latexmk", Some(&search)), None);
    }
}
