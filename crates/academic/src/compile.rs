//! Compiling a project with latexmk and pdfLaTeX, inside the project directory and sandboxed.
//!
//! Hardening, all on by default and not configurable from a document:
//! - `latexmk -norc`: no `latexmkrc` is read, so a project can't run Perl through one.
//! - `pdflatex -no-shell-escape`, and `shell_escape=f` in the environment: `\write18` is refused.
//! - `openin_any=p` and `openout_any=p` (kpathsea "paranoid"): TeX may not read or write files by
//!   absolute path or in parent directories, nor dot files, so a document can't read
//!   `~/.ssh/id_rsa` or write outside the project. (TeX Live honours these; MiKTeX has its own
//!   security settings, see docs/academic.md.)
//! - `TEXINPUTS` and similar search-path variables from the user's environment are removed, so
//!   only the project directory and the TeX installation are searched.
//! - A time limit, after which latexmk and everything it started is killed.
//! - The main file must be a plain file name inside the project directory.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use crate::Error;
use crate::diagnostics::{self, Diagnostic, Severity};
use crate::paths::safe_relative_name;
use crate::process;
use crate::tex::TexEnvironment;

/// The default time limit for one compilation (all latexmk passes together).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(180);
/// The shortest and longest time limits accepted.
pub const MIN_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_TIMEOUT: Duration = Duration::from_secs(3600);
/// The largest log read back for diagnostics (bytes).
const MAX_LOG: u64 = 16 * 1024 * 1024;

/// Environment variables removed before running TeX: search paths and configuration that could
/// make TeX read files from outside the project or change its security settings.
const REMOVED_VARIABLES: &[&str] = &[
    "TEXINPUTS",
    "BIBINPUTS",
    "BSTINPUTS",
    "TEXFONTS",
    "TFMFONTS",
    "TEXMFHOME",
    "TEXMFCNF",
    "TEXMFDOTDIR",
    "TEXMFOUTPUT",
    "TEXMFVAR",
    "TEXMFCONFIG",
    "TEXMFLOCAL",
    "TEXMFAUXTREES",
    "LUAINPUTS",
    "shell_escape_commands",
    "LATEXMKRCSYS",
    "PERL5OPT",
    "PERL5LIB",
    "PERLLIB",
];

/// How to compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileOptions {
    /// The main `.tex` file, a plain file name in the project directory.
    pub main: String,
    pub timeout: Duration,
    /// Rebuild everything (`latexmk -g`) instead of only what changed.
    pub force: bool,
    /// The template uses Biber; checked before running so the message is clear.
    pub needs_biber: bool,
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self { main: "main.tex".into(), timeout: DEFAULT_TIMEOUT, force: false, needs_biber: true }
    }
}

/// How a compilation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    /// latexmk reported an error; see the diagnostics.
    Failed,
    TimedOut,
    Cancelled,
}

/// What a compilation produced.
#[derive(Debug, Clone)]
pub struct CompileReport {
    pub outcome: Outcome,
    /// The PDF, when the compilation succeeded.
    pub pdf: Option<PathBuf>,
    /// The command line that ran, for the log view.
    pub command: String,
    /// latexmk's standard output and error.
    pub output: String,
    /// The main file's `.log`, when there is one.
    pub tex_log: Option<String>,
    /// Errors and warnings from the TeX log and Biber's log.
    pub diagnostics: Vec<Diagnostic>,
    pub elapsed: Duration,
}

impl CompileReport {
    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| d.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| d.severity == Severity::Warning)
    }

    /// One line for a status bar.
    pub fn summary(&self) -> String {
        let secs = self.elapsed.as_secs_f32();
        match self.outcome {
            Outcome::Succeeded => format!("Compiled in {secs:.1} s with {} warning(s).", self.warnings().count()),
            Outcome::Failed => format!("Compilation failed after {secs:.1} s with {} error(s).", self.errors().count().max(1)),
            Outcome::TimedOut => format!("Compilation stopped: it took longer than {secs:.0} s."),
            Outcome::Cancelled => "Compilation cancelled.".to_owned(),
        }
    }
}

/// The latexmk command for `opts`, run in `project`. Exposed for tests and the log view.
pub fn command(env: &TexEnvironment, project: &Path, opts: &CompileOptions) -> Result<Command, Error> {
    let main = safe_relative_name(&opts.main)
        .filter(|m| m.ends_with(".tex"))
        .ok_or_else(|| Error::Compile(format!("\"{}\" is not a plain .tex file name", opts.main)))?;
    if !project.join(main).is_file() {
        return Err(Error::Compile(format!("{} has no {main}", project.display())));
    }
    let missing = env.missing(opts.needs_biber);
    if !missing.is_empty() {
        return Err(Error::TexMissing(missing.join(", ")));
    }
    let latexmk = env.latexmk.path.clone().ok_or_else(|| Error::TexMissing("latexmk".into()))?;
    let mut cmd = Command::new(latexmk);
    cmd.current_dir(project);
    cmd.args(["-norc", "-pdf", "-interaction=nonstopmode", "-halt-on-error", "-file-line-error"]);
    // latexmk runs this through its own command template; %O and %S are its placeholders.
    cmd.arg("-pdflatex=pdflatex -no-shell-escape -interaction=nonstopmode -halt-on-error -file-line-error %O %S");
    if opts.force {
        cmd.arg("-g");
    }
    cmd.arg(main);
    for var in REMOVED_VARIABLES {
        cmd.env_remove(var);
    }
    cmd.env("openin_any", "p").env("openout_any", "p").env("shell_escape", "f");
    // Long log lines unwrapped, so file:line: messages stay on one line.
    cmd.env("max_print_line", "10000").env("error_line", "254").env("half_error_line", "238");
    // Put the TeX installation that was detected first on the search path, so latexmk runs the
    // same pdflatex and biber that were checked.
    if let Some(bin) = env.tex_bin_dir() {
        let mut dirs = vec![bin.to_path_buf()];
        if let Some(path) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&path).filter(|d| d.is_absolute()));
        }
        if let Ok(joined) = std::env::join_paths(dirs) {
            cmd.env("PATH", joined);
        }
    }
    Ok(cmd)
}

fn describe(cmd: &Command) -> String {
    let mut s = cmd.get_program().to_string_lossy().into_owned();
    for a in cmd.get_args() {
        let a = a.to_string_lossy();
        if a.contains(' ') {
            s.push_str(&format!(" \"{a}\""));
        } else {
            s.push(' ');
            s.push_str(&a);
        }
    }
    s
}

/// Compile `opts.main` in `project`. Errors are for things that stop the run from starting (no
/// TeX, no main file); a run that fails, times out or is cancelled returns a report saying so.
pub fn compile(env: &TexEnvironment, project: &Path, opts: &CompileOptions, cancel: Option<&Arc<AtomicBool>>) -> Result<CompileReport, Error> {
    let cmd = command(env, project, opts)?;
    let command = describe(&cmd);
    log::info!("compiling in {}: {command}", project.display());
    let run = process::run(cmd, opts.timeout.clamp(MIN_TIMEOUT, MAX_TIMEOUT), cancel)
        .map_err(|e| Error::Compile(format!("could not start latexmk: {e}")))?;
    let stem = opts.main.trim_end_matches(".tex");
    let tex_log = read_text(&project.join(format!("{stem}.log")));
    let mut diagnostics = tex_log.as_deref().map(diagnostics::parse_tex_log).unwrap_or_default();
    if let Some(blg) = read_text(&project.join(format!("{stem}.blg"))) {
        diagnostics.extend(diagnostics::parse_biber_log(&blg));
    }
    let pdf_path = project.join(format!("{stem}.pdf"));
    let outcome = if run.timed_out {
        Outcome::TimedOut
    } else if run.cancelled {
        Outcome::Cancelled
    } else if run.success() && pdf_path.is_file() {
        Outcome::Succeeded
    } else {
        Outcome::Failed
    };
    if outcome == Outcome::Failed && !diagnostics.iter().any(|d| d.severity == Severity::Error) {
        // latexmk failed without a TeX error: say what it printed last.
        let last = run.stdout.lines().chain(run.stderr.lines()).rfind(|l| !l.trim().is_empty()).unwrap_or("latexmk failed").to_owned();
        diagnostics.insert(0, Diagnostic { severity: Severity::Error, file: None, line: None, message: last });
    }
    let mut output = run.stdout;
    if !run.stderr.is_empty() {
        output.push_str("\n--- standard error ---\n");
        output.push_str(&run.stderr);
    }
    Ok(CompileReport {
        outcome,
        pdf: (outcome == Outcome::Succeeded).then_some(pdf_path),
        command,
        output,
        tex_log,
        diagnostics,
        elapsed: run.elapsed,
    })
}

fn read_text(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let bytes = if meta.len() > MAX_LOG {
        use std::io::{Read, Seek, SeekFrom};
        // Keep the end of an enormous log: that's where the error is.
        let mut f = std::fs::File::open(path).ok()?;
        f.seek(SeekFrom::End(-(MAX_LOG as i64))).ok()?;
        let mut v = Vec::new();
        f.take(MAX_LOG).read_to_end(&mut v).ok()?;
        v
    } else {
        std::fs::read(path).ok()?
    };
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tex::Tool;

    fn env_with(dir: &Path) -> TexEnvironment {
        let tool = |name: &'static str| Tool { name, path: Some(dir.join(name)), version: None };
        TexEnvironment {
            pdflatex: tool("pdflatex"),
            latexmk: tool("latexmk"),
            biber: tool("biber"),
            pandoc: Tool { name: "pandoc", path: None, version: None },
        }
    }

    #[test]
    fn the_command_is_sandboxed() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.tex"), "").unwrap();
        let cmd = command(&env_with(dir.path()), dir.path(), &CompileOptions::default()).unwrap();
        let args: Vec<String> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert!(args.contains(&"-norc".to_owned()));
        assert!(args.iter().any(|a| a.contains("-no-shell-escape")));
        assert!(!args.iter().any(|a| a.contains("-shell-escape") && !a.contains("-no-shell-escape")));
        let envs: Vec<(String, Option<String>)> =
            cmd.get_envs().map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned()))).collect();
        assert!(envs.contains(&("openin_any".into(), Some("p".into()))));
        assert!(envs.contains(&("openout_any".into(), Some("p".into()))));
        assert!(envs.contains(&("shell_escape".into(), Some("f".into()))));
        assert!(envs.contains(&("TEXINPUTS".into(), None)), "TEXINPUTS is removed");
        assert_eq!(cmd.get_current_dir(), Some(dir.path()));
    }

    #[test]
    fn unsafe_main_files_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_with(dir.path());
        for main in ["../main.tex", "/etc/main.tex", "main.pdf", "-main.tex", "a b.tex", "missing.tex"] {
            let opts = CompileOptions { main: main.into(), ..CompileOptions::default() };
            assert!(command(&env, dir.path(), &opts).is_err(), "{main}");
        }
    }

    #[test]
    fn missing_tex_is_reported_before_running() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.tex"), "").unwrap();
        let mut env = env_with(dir.path());
        env.biber.path = None;
        match command(&env, dir.path(), &CompileOptions::default()) {
            Err(Error::TexMissing(m)) => assert_eq!(m, "biber"),
            other => panic!("{other:?}"),
        }
        assert!(command(&env, dir.path(), &CompileOptions { needs_biber: false, ..CompileOptions::default() }).is_ok());
    }
}
