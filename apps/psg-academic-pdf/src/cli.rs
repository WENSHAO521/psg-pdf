//! The headless commands: the same project, template and compilation code as the window, for
//! scripts, agents and tests.
//!
//! ```text
//! psg-academic-pdf check-tex
//! psg-academic-pdf install-tex | update-tex | repair-biber
//! psg-academic-pdf templates | add-template <file.zip|folder> [--name <name>]
//! psg-academic-pdf typeset <manuscript.docx|.tex> --out <file.pdf> [--template <id>] [--metadata <file>] [--project <dir>] [--json]
//! psg-academic-pdf new <dir> [--template <id>]
//! psg-academic-pdf check <dir>
//! psg-academic-pdf compile <dir> [--out <file.pdf>] [--timeout <seconds>] [--manuscript <file.tex|file.docx>]
//! psg-academic-pdf source-at <dir> --page <n> --x <pt> --y <pt> [--json]
//! psg-academic-pdf page-of <dir> --file <file.tex> --line <n> [--json]
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

/// Where templates people add are kept (the same folder the window uses).
pub fn user_templates(data_root: &std::path::Path) -> PathBuf {
    data_root.join("templates")
}

/// The template folders: the ones people added, then the ones beside the program.
pub fn template_dirs(data_root: &std::path::Path) -> Vec<PathBuf> {
    let mut dirs = vec![user_templates(data_root)];
    dirs.extend(template::default_template_dirs());
    dirs
}

fn all_templates() -> (Vec<template::Template>, Vec<String>) {
    let data = crate::data_dir::DataDir::for_this_run(crate::app::APP_NAME);
    template::discover(&template_dirs(&data.root))
}

/// The TeX programs, PSG's own first.
fn detect() -> (TexEnvironment, Option<ManagedTex>) {
    let tex = managed();
    (tex_setup::detect(tex.as_ref()), tex)
}

fn tex_job(job: Job) -> Result<(), String> {
    let tex = managed().ok_or("there is no folder to install TeX in")?;
    say(&format!("PSG TeX folder: {}", tex.root.display()));
    if matches!(job, Job::Update | Job::RepairBiber) && !tex.installed() {
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
            Progress::Fonts { total, .. } => format!("Downloading the Chinese and Japanese fonts ({} MB)…", total / 1_048_576),
        };
        if last.as_ref() != Some(&line) {
            say(&line);
            last = Some(line);
        }
    };
    match job {
        Job::Install => say(&format!("Installed TinyTeX {}.", tex_setup::install(&tex, &mut progress)?)),
        Job::Fonts => {
            tex_setup::install_fonts(&tex, &mut progress)?;
            say(&format!("The Chinese and Japanese fonts are in {}.", tex.fonts_dir().display()));
        }
        Job::Update => match tex_setup::update(&tex, &mut progress)? {
            tex_setup::Done::Updated => say("TeX is up to date."),
            tex_setup::Done::Installed(release) => say(&format!("Reinstalled from TinyTeX {release} (a new TeX Live year).")),
        },
        Job::RepairBiber => {
            tex.reinstall_biber().map_err(|e| e.to_string())?;
            say("Reinstalled Biber and biblatex.");
        }
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
  repair-biber                   reinstall Biber and biblatex in PSG's own TeX
  install-fonts                  download the Chinese and Japanese fonts (Noto CJK) the
                                 XeLaTeX templates use (install-tex does this too)
  templates                      list the journal templates
  add-template <file.zip|folder> [--name <name>]
                                 add a journal's own LaTeX template: finds its main file,
                                 the article's text and where the title, authors, abstract
                                 and keywords go, and says what it found
  typeset <manuscript.docx|.tex> --out <file.pdf> [--template <id>] [--metadata <file>]
          [--project <dir>] [--timeout <seconds>] [--json]
                                 one step from a manuscript to a PDF: a new project from the
                                 template, the manuscript's title, authors, abstract,
                                 keywords, figures and tables read, the metadata file's
                                 fields (TOML or JSON: volume, doi, received, …) laid over
                                 them, typeset and exported; --json prints a report
  new <dir> [--template <id>]    start a project in <dir> (default template: psg-article)
  check <dir>                    list the problems found before compiling (scripts the
                                 templates can't set, the bibliography); exit 1 on an error
  compile <dir> [--manuscript <file.tex|file.docx>] [--timeout <seconds>] [--out <file.pdf>]
                                 typeset the project in <dir> and optionally export the PDF
  source-at <dir> --page <n> --x <pt> --y <pt> [--json]
                                 the source file and line that put what is at that point of
                                 the PDF (page from 1, points from the page's top-left corner)
  page-of <dir> --file <file.tex> --line <n> [--json]
                                 the page (from 1) and height in points where that line of the
                                 source is in the PDF; both need a typeset project
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
        "repair-biber" => report(tex_job(Job::RepairBiber)),
        "install-fonts" => report(tex_job(Job::Fonts)),
        "templates" => templates(),
        "add-template" => report(add_template(rest)),
        "typeset" => typeset(rest),
        "new" => report(new(rest)),
        "check" => check(rest),
        "compile" => report(compile(rest)),
        "source-at" => report(source_at(rest)),
        "page-of" => report(page_of(rest)),
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
    for tool in [&env.pdflatex, &env.xelatex, &env.latexmk, &env.biber, &env.pandoc] {
        match (&tool.path, &tool.probe_error) {
            (Some(p), Some(error)) => complain(&format!("{:<9} {}\n{error}", tool.name, p.display())),
            (Some(p), None) => say(&format!("{:<9} {}  ({})", tool.name, p.display(), tool.version.as_deref().unwrap_or("version unknown"))),
            (None, _) => say(&format!("{:<9} not found", tool.name)),
        }
    }
    let missing = env.missing(true);
    if missing.is_empty() {
        say("TeX is ready.");
        0
    } else {
        complain(&format!("missing or unusable: {}", missing.join(", ")));
        1
    }
}

fn templates() -> i32 {
    let (templates, problems) = all_templates();
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
    let (templates, _) = all_templates();
    let t = templates.into_iter().find(|t| t.manifest.id == id).ok_or_else(|| format!("no template \"{id}\" (see `psg-academic-pdf templates`)"))?;
    let p = Project::create(&PathBuf::from(dir), &t, PublicationMetadata::sample()).map_err(|e| e.to_string())?;
    say(&format!("Created {} from {}.", p.dir.display(), t.manifest.name));
    Ok(())
}

fn add_template(args: &[String]) -> Result<(), String> {
    let source = args.first().filter(|a| !a.starts_with("--")).ok_or("add-template: name a .zip file or a folder")?;
    let data = crate::data_dir::DataDir::for_this_run(crate::app::APP_NAME);
    let added = psg_academic::template_import::import(&PathBuf::from(source), &user_templates(&data.root), flag(args, "--name"))
        .map_err(|e| e.to_string())?;
    let m = &added.template.manifest;
    say(&format!("Added \"{}\" as template {} in {}.", m.name, m.id, added.dir.display()));
    for line in &added.found {
        say(&format!("  found: {line}"));
    }
    for line in &added.warnings {
        complain(&format!("check: {line}"));
    }
    say(&format!("Use it with: psg-academic-pdf typeset <manuscript.docx> --template {} --out <file.pdf>", m.id));
    Ok(())
}

/// `typeset`: exit 0 when the PDF was made, 1 when a step failed, 2 for a usage mistake.
fn typeset(args: &[String]) -> i32 {
    let json = args.iter().any(|a| a == "--json");
    let usage = |why: &str| {
        complain(&format!("typeset: {why}"));
        2
    };
    let Some(manuscript) = args.first().filter(|a| !a.starts_with("--")) else { return usage("name the manuscript (.docx or .tex)") };
    let Some(out) = flag(args, "--out") else { return usage("--out <file.pdf> says where the PDF goes") };
    let id = flag(args, "--template").unwrap_or("psg-article");
    let (templates, _) = all_templates();
    let Some(template) = templates.into_iter().find(|t| t.manifest.id == id) else {
        return usage(&format!("no template \"{id}\" (see `psg-academic-pdf templates`)"));
    };
    let timeout_secs = match flag(args, "--timeout").map(str::parse::<u64>) {
        Some(Ok(secs)) => Some(secs),
        Some(Err(_)) => return usage("--timeout takes a number of seconds"),
        None => None,
    };
    let project_dir = match flag(args, "--project") {
        Some(dir) => PathBuf::from(dir),
        None => {
            // A new project in the workspace, named after the manuscript, as the window does.
            let data = crate::data_dir::DataDir::for_this_run(crate::app::APP_NAME);
            let stamp = crate::app::local_now().strftime("%Y%m%d-%H%M%S").to_string();
            data.projects().join(crate::launch::project_name(std::path::Path::new(manuscript), &stamp))
        }
    };
    let (env, tex) = detect();
    let request = psg_academic::typeset::Request {
        manuscript: PathBuf::from(manuscript),
        template,
        project_dir,
        metadata: flag(args, "--metadata").map(PathBuf::from),
        out: Some(PathBuf::from(out)),
        timeout_secs,
    };
    let report = psg_academic::typeset::typeset(&request, &env, tex.as_ref(), env.pandoc.path.as_deref());
    if json {
        match serde_json::to_string_pretty(&report) {
            Ok(text) => say(&text),
            Err(e) => complain(&e.to_string()),
        }
    } else {
        for line in &report.import {
            say(line);
        }
        if !report.metadata_set.is_empty() {
            say(&format!("Metadata from the file: {}", report.metadata_set.join(", ")));
        }
        if !report.installed_packages.is_empty() {
            say(&format!("Installed the TeX packages this document needs: {}", report.installed_packages.join(", ")));
        }
        for p in &report.problems {
            complain(&p.diagnostic.to_string());
            if let Some(hint) = p.hint {
                complain(&format!("  hint: {hint}"));
            }
        }
        say(&format!("Project: {}", report.project));
        match (&report.pdf, &report.error) {
            (Some(pdf), _) => say(&format!("Typeset in {:.1} s: {pdf}", report.seconds)),
            (None, Some(e)) => complain(&format!("{} failed: {e}", report.failed_step.unwrap_or("typesetting"))),
            (None, None) => {}
        }
    }
    i32::from(!report.ok)
}

fn compile(args: &[String]) -> Result<(), String> {
    let dir = args.first().filter(|a| !a.starts_with("--")).ok_or("compile: name the project folder")?;
    let mut project = Project::open(&PathBuf::from(dir)).map_err(|e| e.to_string())?;
    let (env, tex) = detect();
    if let Some(manuscript) = flag(args, "--manuscript") {
        let path = PathBuf::from(manuscript);
        let note = if manuscript.to_ascii_lowercase().ends_with(".docx") {
            let pandoc = env.pandoc.path.clone().ok_or("pandoc is not installed, so .docx can't be imported")?;
            PandocImporter { pandoc }.import(&path, &mut project, None)
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

/// The SyncTeX data of the typeset project named first in `args`.
fn synctex_of(args: &[String], cmd: &str) -> Result<psg_academic::synctex::SyncTex, String> {
    let dir = args.first().filter(|a| !a.starts_with("--")).ok_or_else(|| format!("{cmd}: name the project folder"))?;
    let project = Project::open(&PathBuf::from(dir)).map_err(|e| e.to_string())?;
    psg_academic::synctex::SyncTex::read(&project.dir, &project.file.main)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("{cmd}: the project has no SyncTeX data yet; compile it first"))
}

/// A number flag, or a usage error that says which and why.
fn number<T: std::str::FromStr>(args: &[String], cmd: &str, name: &str) -> Result<T, String> {
    let v = flag(args, name).ok_or_else(|| format!("{cmd}: {name} is needed"))?;
    v.parse().map_err(|_| format!("{cmd}: {name} takes a number, not \"{v}\""))
}

fn source_at(args: &[String]) -> Result<(), String> {
    let page: usize = number(args, "source-at", "--page")?;
    let x: f32 = number(args, "source-at", "--x")?;
    let y: f32 = number(args, "source-at", "--y")?;
    if page == 0 || !x.is_finite() || !y.is_finite() {
        return Err("source-at: pages count from 1 and points must be finite".into());
    }
    let sync = synctex_of(args, "source-at")?;
    let (file, line) = sync.source_at(page - 1, x, y).ok_or_else(|| format!("source-at: nothing from the project's sources is on page {page}"))?;
    if args.iter().any(|a| a == "--json") {
        say(&serde_json::json!({ "file": file, "line": line }).to_string());
    } else {
        say(&format!("{file}:{line}"));
    }
    Ok(())
}

fn page_of(args: &[String]) -> Result<(), String> {
    let file = flag(args, "--file").ok_or("page-of: --file is needed")?;
    let line: u32 = number(args, "page-of", "--line")?;
    let sync = synctex_of(args, "page-of")?;
    let (page, y) = sync.page_of(file, line).ok_or_else(|| format!("page-of: nothing from {file} is in the PDF; compile the project first"))?;
    let page = page.saturating_add(1);
    if args.iter().any(|a| a == "--json") {
        say(&serde_json::json!({ "page": page, "y": y }).to_string());
    } else {
        say(&format!("page {page}, {y:.1} pt from the top"));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    #[test]
    fn check_fails_on_text_the_template_cant_set_and_passes_on_the_sample() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("p");
        let dir_arg = dir.display().to_string();
        assert_eq!(super::run(&["new".into(), dir_arg.clone()]), Some(0));
        assert_eq!(super::run(&["check".into(), dir_arg.clone()]), Some(0), "the sample has warnings at most");
        // The template sets Chinese and Japanese with XeLaTeX, but not Korean.
        std::fs::write(dir.join("body.tex"), "张三 김철수\n").unwrap();
        assert_eq!(super::run(&["check".into(), dir_arg]), Some(1));
        assert_eq!(super::run(&["check".into()]), Some(2));
    }

    #[test]
    fn the_pdf_and_source_lookups_run_headless() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("p");
        let d = dir.display().to_string();
        let run = |args: &[&str]| super::run(&args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>());
        assert_eq!(run(&["new", &d]), Some(0));
        // Before the first compilation there is nothing to look up.
        assert_eq!(run(&["page-of", &d, "--file", "body.tex", "--line", "1"]), Some(1));
        let project = psg_academic::Project::open(&dir).unwrap();
        let stem = project.file.main.trim_end_matches(".tex").to_owned();
        let text = format!("Input:1:{}/./body.tex\n{{1\nx1,3:6578176,6578176\n}}1\n", dir.display());
        std::fs::write(dir.join(format!("{stem}.synctex")), text).unwrap();
        assert_eq!(run(&["page-of", &d, "--file", "body.tex", "--line", "3", "--json"]), Some(0));
        assert_eq!(run(&["source-at", &d, "--page", "1", "--x", "100", "--y", "100"]), Some(0));
        assert_eq!(run(&["source-at", &d, "--page", "2", "--x", "100", "--y", "100"]), Some(1));
        assert_eq!(run(&["source-at", &d, "--page", "0", "--x", "1", "--y", "1"]), Some(1));
        assert_eq!(run(&["page-of", &d, "--file", "body.tex", "--line", "x"]), Some(1));
    }
}
