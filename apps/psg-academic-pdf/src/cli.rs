//! The headless commands: the same project, template and compilation code as the window, for
//! scripts, agents and tests.
//!
//! ```text
//! psg-academic-pdf check-tex
//! psg-academic-pdf install-tex | update-tex
//! psg-academic-pdf templates
//! psg-academic-pdf new <dir> [--template <id>]
//! psg-academic-pdf check <dir>
//! psg-academic-pdf compile <dir> [--out <file.pdf>] [--timeout <seconds>] [--manuscript <file.tex|file.docx>]
//! ```

use std::path::PathBuf;
use std::time::Duration;

use psg_academic::managed::ManagedTex;
use psg_academic::pandoc::{ManuscriptImporter, PandocImporter};
use psg_academic::{Outcome, Project, PublicationMetadata, TexEnvironment, template};

use crate::tex_setup::{self, Job, Progress};

/// PSG's own TeX for this run (the same folder the window uses).
fn managed() -> Option<ManagedTex> {
    let data = crate::data_dir::DataDir::for_this_run(crate::app::APP_NAME);
    tex_setup::managed_for(&data.root, data.portable)
}

/// The TeX programs, PSG's own first.
fn detect() -> (TexEnvironment, Option<ManagedTex>) {
    let tex = managed();
    (TexEnvironment::detect_with(tex_setup::bin_dir(tex.as_ref()).as_deref()), tex)
}

fn tex_job(job: Job) -> Result<(), String> {
    let tex = managed().ok_or("there is no folder to install TeX in")?;
    say(&format!("PSG TeX folder: {}", tex.root.display()));
    if job == Job::Update && !tex.installed() {
        return Err("PSG's TeX is not installed; run install-tex first".into());
    }
    let mut last = None;
    let mut progress = |p: Progress| {
        let line = match p {
            Progress::Looking => "Looking for the newest TinyTeX release…".to_owned(),
            Progress::Downloading { total, .. } => format!("Downloading {} MB…", total / 1_048_576),
            Progress::Unpacking => "Unpacking…".to_owned(),
            Progress::Packages => "Adding the packages the templates use…".to_owned(),
            Progress::Updating => "Updating with tlmgr…".to_owned(),
        };
        if last.as_ref() != Some(&line) {
            say(&line);
            last = Some(line);
        }
    };
    match job {
        Job::Install => say(&format!("Installed TinyTeX {}.", tex_setup::install(&tex, &mut progress)?)),
        Job::Update => match tex_setup::update(&tex, &mut progress)? {
            tex_setup::Done::Updated => say("TeX is up to date."),
            tex_setup::Done::Installed(release) => say(&format!("Reinstalled from TinyTeX {release} (a new TeX Live year).")),
        },
    }
    Ok(())
}

pub const USAGE: &str = "\
Usage: psg-academic-pdf [command]

Without a command, opens the PSG Academic PDF window.

Commands:
  check-tex                      show which TeX programs were found
  install-tex                    download and install PSG's own TeX (TinyTeX) now,
                                 replacing an earlier one
  update-tex                     update PSG's own TeX (the window does this weekly)
  templates                      list the journal templates
  new <dir> [--template <id>]    start a project in <dir> (default template: psg-article)
  check <dir>                    list the problems found before compiling (scripts the
                                 templates can't set, the bibliography); exit 1 on an error
  compile <dir> [--manuscript <file.tex|file.docx>] [--timeout <seconds>] [--out <file.pdf>]
                                 typeset the project in <dir> and optionally export the PDF
  -h, --help                     print this and exit
  --version                      print the version and exit";

/// The headless command named by `args`, if any; `None` opens the window.
pub fn run(args: &[String]) -> Option<i32> {
    let (cmd, rest) = args.split_first()?;
    Some(match cmd.as_str() {
        "-h" | "--help" => {
            say(USAGE);
            0
        }
        "--version" => {
            say(&format!("psg-academic-pdf {}", env!("CARGO_PKG_VERSION")));
            0
        }
        "check-tex" => check_tex(),
        "install-tex" => report(tex_job(Job::Install)),
        "update-tex" => report(tex_job(Job::Update)),
        "templates" => templates(),
        "new" => report(new(rest)),
        "check" => check(rest),
        "compile" => report(compile(rest)),
        _ => return None,
    })
}

fn say(text: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stdout().lock(), "{text}");
}

fn complain(text: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stderr().lock(), "psg-academic-pdf: {text}");
}

fn report(r: Result<(), String>) -> i32 {
    match r {
        Ok(()) => 0,
        Err(e) => {
            complain(&e);
            1
        }
    }
}

/// The checks the window shows under Problems ▸ Before compiling, one per line. Exit status 1
/// when any is an error, so a script can stop before compiling.
fn check(args: &[String]) -> i32 {
    let Some(dir) = args.first() else {
        complain("check needs a project folder: psg-academic-pdf check <dir>");
        return 2;
    };
    let project = match Project::open(&PathBuf::from(dir)) {
        Ok(p) => p,
        Err(e) => {
            complain(&e.to_string());
            return 1;
        }
    };
    let problems = project.preflight();
    for d in &problems {
        say(&d.to_string());
    }
    if problems.is_empty() {
        say("No problems found before compiling.");
    }
    i32::from(problems.iter().any(|d| d.severity == psg_academic::Severity::Error))
}

fn check_tex() -> i32 {
    let (env, tex) = detect();
    match &tex {
        Some(tex) if env.managed => say(&format!("PSG TeX   {} (TinyTeX {})", tex.root.display(), tex.state().release)),
        Some(tex) => say(&format!("PSG TeX   not installed in {} (install-tex installs it)", tex.root.display())),
        None => {}
    }
    for tool in [&env.pdflatex, &env.latexmk, &env.biber, &env.pandoc] {
        match (&tool.path, &tool.version) {
            (Some(p), v) => say(&format!("{:<9} {}  ({})", tool.name, p.display(), v.as_deref().unwrap_or("version unknown"))),
            (None, _) => say(&format!("{:<9} not found", tool.name)),
        }
    }
    let missing = env.missing(true);
    if missing.is_empty() {
        say("TeX is ready.");
        0
    } else {
        complain(&format!("missing: {}", missing.join(", ")));
        1
    }
}

fn templates() -> i32 {
    let (templates, problems) = template::discover(&template::default_template_dirs());
    for t in &templates {
        let m = &t.manifest;
        say(&format!("{:<20} {} {} — {}", m.id, m.name, m.version, m.description));
    }
    for p in problems {
        complain(&p);
    }
    0
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn new(args: &[String]) -> Result<(), String> {
    let dir = args.first().filter(|a| !a.starts_with("--")).ok_or("new: name the project folder")?;
    let id = flag(args, "--template").unwrap_or("psg-article");
    let (templates, _) = template::discover(&template::default_template_dirs());
    let t = templates.into_iter().find(|t| t.manifest.id == id).ok_or_else(|| format!("no template \"{id}\" (see `psg-academic-pdf templates`)"))?;
    let p = Project::create(&PathBuf::from(dir), &t, PublicationMetadata::sample()).map_err(|e| e.to_string())?;
    say(&format!("Created {} from {}.", p.dir.display(), t.manifest.name));
    Ok(())
}

fn compile(args: &[String]) -> Result<(), String> {
    let dir = args.first().filter(|a| !a.starts_with("--")).ok_or("compile: name the project folder")?;
    let mut project = Project::open(&PathBuf::from(dir)).map_err(|e| e.to_string())?;
    let (env, tex) = detect();
    if let Some(manuscript) = flag(args, "--manuscript") {
        let path = PathBuf::from(manuscript);
        let note = if manuscript.to_ascii_lowercase().ends_with(".docx") {
            let pandoc = env.pandoc.path.clone().ok_or("pandoc is not installed, so .docx can't be imported")?;
            PandocImporter { pandoc }.import(&path, &project, None)
        } else {
            project.import_manuscript(&path)
        };
        say(&note.map_err(|e| e.to_string())?);
    }
    if let Some(t) = flag(args, "--timeout") {
        let secs: u64 = t.parse().map_err(|_| format!("--timeout: \"{t}\" is not a number of seconds"))?;
        project.file.settings.timeout_secs = secs;
        project.save().map_err(|e| e.to_string())?;
    }
    let (report, installed) = psg_academic::managed::compile_completing(&project, &env, tex.as_ref(), None).map_err(|e| e.to_string())?;
    if !installed.is_empty() {
        say(&format!("Installed the TeX packages this document needs: {}", installed.join(", ")));
    }
    for d in &report.diagnostics {
        complain(&d.to_string());
        if let Some(hint) = d.hint() {
            complain(&format!("  hint: {hint}"));
        }
    }
    say(&report.summary());
    if report.outcome != Outcome::Succeeded {
        return Err(match report.outcome {
            Outcome::TimedOut => format!("stopped at the time limit ({:?})", Duration::from_secs(project.file.settings.timeout_secs)),
            _ => "compilation failed".into(),
        });
    }
    if let Some(out) = flag(args, "--out") {
        project.export_pdf(&PathBuf::from(out)).map_err(|e| e.to_string())?;
        say(&format!("Exported {out}."));
    } else {
        say(&format!("PDF: {}", project.pdf_path().display()));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    #[test]
    fn check_fails_on_chinese_text_and_passes_on_the_sample() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("p");
        let dir_arg = dir.display().to_string();
        assert_eq!(super::run(&["new".into(), dir_arg.clone()]), Some(0));
        assert_eq!(super::run(&["check".into(), dir_arg.clone()]), Some(0), "the sample has warnings at most");
        std::fs::write(dir.join("body.tex"), "张三\n").unwrap();
        assert_eq!(super::run(&["check".into(), dir_arg]), Some(1));
        assert_eq!(super::run(&["check".into()]), Some(2));
    }
}
