//! End-to-end compilation with the local TeX installation. Each test returns early (and says so)
//! when pdflatex, latexmk or biber is not installed, so the suite passes on machines without TeX.

use std::time::Duration;

use psg_academic::{Outcome, Project, PublicationMetadata, Severity, Template, TexEnvironment};

fn tex() -> Option<TexEnvironment> {
    let env = TexEnvironment::detect();
    let missing = env.missing(true);
    if missing.is_empty() {
        Some(env)
    } else {
        eprintln!("skipped: {} not installed", missing.join(", "));
        None
    }
}

fn sample_project(root: &std::path::Path) -> Project {
    Project::create(&root.join("p"), &Template::builtin().remove(0), PublicationMetadata::sample()).unwrap()
}

#[test]
fn the_sample_template_compiles_to_a_pdf() {
    let Some(env) = tex() else { return };
    let root = tempfile::tempdir().unwrap();
    let project = sample_project(root.path());
    let report = project.compile(&env, None).unwrap();
    assert_eq!(report.outcome, Outcome::Succeeded, "{}\n{:?}", report.output, report.diagnostics);
    let pdf = std::fs::read(report.pdf.as_ref().unwrap()).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
    // Biber ran: the bibliography's entries are resolved, so no citation is left undefined.
    assert!(!report.warnings().any(|w| w.message.contains("undefined")), "{:?}", report.diagnostics);
    let out = root.path().join("exported.pdf");
    project.export_pdf(&out).unwrap();
    assert_eq!(std::fs::read(out).unwrap(), pdf);
}

#[test]
fn an_error_is_reported_with_its_file_and_line() {
    let Some(env) = tex() else { return };
    let root = tempfile::tempdir().unwrap();
    let project = sample_project(root.path());
    std::fs::write(project.dir.join("body.tex"), "First line.\n\n\\undefinedcommand\n").unwrap();
    let report = project.compile(&env, None).unwrap();
    assert_eq!(report.outcome, Outcome::Failed);
    assert!(report.pdf.is_none());
    let error = report.diagnostics.iter().find(|d| d.severity == Severity::Error).unwrap();
    assert_eq!(error.file.as_deref(), Some("./body.tex"), "{:?}", report.diagnostics);
    assert_eq!(error.line, Some(3));
    assert!(error.message.contains("Undefined control sequence"));
}

#[test]
fn a_document_that_never_ends_is_stopped_at_the_time_limit() {
    let Some(env) = tex() else { return };
    let root = tempfile::tempdir().unwrap();
    let mut project = sample_project(root.path());
    std::fs::write(project.dir.join("body.tex"), "\\def\\x{\\x}\\x\n").unwrap();
    project.file.settings.timeout_secs = 5;
    let start = std::time::Instant::now();
    let report = project.compile(&env, None).unwrap();
    assert_eq!(report.outcome, Outcome::TimedOut);
    assert!(start.elapsed() < Duration::from_secs(30));
}

#[test]
fn shell_escape_and_reading_outside_the_project_are_refused() {
    let Some(env) = tex() else { return };
    let root = tempfile::tempdir().unwrap();
    let secret = root.path().join("secret.tex");
    std::fs::write(&secret, "TOPSECRET").unwrap();
    let project = sample_project(root.path());
    let body = format!("\\immediate\\write18{{touch pwned}}\n\\input{{{}}}\n", secret.display());
    std::fs::write(project.dir.join("body.tex"), body).unwrap();
    let report = project.compile(&env, None).unwrap();
    assert!(!project.dir.join("pwned").exists(), "shell escape ran");
    assert_eq!(report.outcome, Outcome::Failed, "the absolute \\input should have been refused");
    let log = report.tex_log.unwrap_or_default();
    assert!(log.contains("runsystem(touch pwned)...disabled"), "shell escape not reported as disabled");
    assert!(!log.contains("TOPSECRET"));
}
