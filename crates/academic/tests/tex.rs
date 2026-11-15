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

#[test]
fn every_built_in_template_compiles() {
    let Some(env) = tex() else { return };
    let root = tempfile::tempdir().unwrap();
    for template in Template::builtin() {
        let id = template.manifest.id.clone();
        let project = Project::create(&root.path().join(&id), &template, PublicationMetadata::sample()).unwrap();
        let report = project.compile(&env, None).unwrap();
        assert_eq!(report.outcome, Outcome::Succeeded, "{id}: {}\n{:?}", report.output, report.diagnostics);
        assert!(!report.warnings().any(|w| w.message.contains("undefined")), "{id}: {:?}", report.diagnostics);
    }
}

/// A Word manuscript, made here with Pandoc from Markdown, goes through the DOCX import and
/// compiles: the class defines what Pandoc's LaTeX uses (lists, tables, strikeout, figures).
#[test]
fn an_imported_word_manuscript_compiles() {
    use psg_academic::pandoc::{ManuscriptImporter, PandocImporter};
    let Some(env) = tex() else { return };
    let Some(pandoc) = env.pandoc.path.clone() else {
        eprintln!("skipped: pandoc not installed");
        return;
    };
    let root = tempfile::tempdir().unwrap();
    let src = root.path().join("src");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("fig.png"), tiny_png()).unwrap();
    std::fs::write(
        src.join("in.md"),
        "# Introduction\n\nSome *emphasis*, ~~struck~~, a [link](https://example.org), caf\u{e9} \u{2014} and $E=mc^2$.[^1]\n\n\
         - one\n- two\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n![A figure](fig.png)\n\n[^1]: A footnote.\n",
    )
    .unwrap();
    let made = std::process::Command::new(&pandoc).current_dir(&src).args(["in.md", "-o", "manuscript.docx"]).status().unwrap();
    assert!(made.success());
    for template in Template::builtin() {
        let id = template.manifest.id.clone();
        let project = Project::create(&root.path().join(&id), &template, PublicationMetadata::sample()).unwrap();
        let note = PandocImporter { pandoc: pandoc.clone() }.import(&src.join("manuscript.docx"), &project, None).unwrap();
        assert!(!note.contains("To fix before compiling"), "{id}: {note}");
        let report = project.compile(&env, None).unwrap();
        assert_eq!(report.outcome, Outcome::Succeeded, "{id}: {}\n{:?}", report.output, report.diagnostics);
    }
}

/// A 64 x 16 RGB PNG.
fn tiny_png() -> Vec<u8> {
    fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
        let mut crc_input = kind.to_vec();
        crc_input.extend_from_slice(data);
        out.extend_from_slice(&crc_input);
        out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    }
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFFu32;
        for b in bytes {
            crc ^= u32::from(*b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
            }
        }
        !crc
    }
    let (w, h) = (64u32, 16u32);
    // Each row: filter type 0 (none), then the pixels.
    let row: Vec<u8> = std::iter::once(0).chain([0x5b, 0x2a, 0x78].repeat(w as usize)).collect();
    let raw = row.repeat(h as usize);
    // Stored (uncompressed) deflate blocks inside a zlib stream.
    let mut z = vec![0x78, 0x01];
    let mut chunks = raw.chunks(65535).peekable();
    while let Some(block) = chunks.next() {
        z.push(u8::from(chunks.peek().is_none()));
        let len = u16::try_from(block.len()).unwrap();
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(block);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for byte in &raw {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &z);
    chunk(&mut png, b"IEND", &[]);
    png
}
