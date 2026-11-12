//! The headless commands: the same project, template and compilation code as the window, for
//! scripts, agents and tests.
//!
//! ```text
//! psg-academic-pdf check-tex
//! psg-academic-pdf templates
//! psg-academic-pdf new <dir> [--template <id>]
//! psg-academic-pdf compile <dir> [--out <file.pdf>] [--timeout <seconds>] [--manuscript <file.tex|file.docx>]
//! ```

use std::path::PathBuf;
use std::time::Duration;

use psg_academic::pandoc::{ManuscriptImporter, PandocImporter};
use psg_academic::{Outcome, Project, PublicationMetadata, TexEnvironment, template};

pub const USAGE: &str = "\
Usage: psg-academic-pdf [command]

Without a command, opens the PSG Academic PDF window.

Commands:
  check-tex                      show which TeX programs were found
  templates                      list the journal templates
  new <dir> [--template <id>]    start a project in <dir> (default template: psg-article)
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
        "templates" => templates(),
        "new" => report(new(rest)),
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

fn check_tex() -> i32 {
    let env = TexEnvironment::detect();
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
    let env = TexEnvironment::detect();
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
    let report = project.compile(&env, None).map_err(|e| e.to_string())?;
    for d in &report.diagnostics {
        complain(&d.to_string());
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
