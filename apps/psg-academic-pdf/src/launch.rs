//! What a path given to PSG Academic PDF asks for: on the command line, from "Open with" on
//! Windows and Linux, or from Finder on macOS (an Apple event).
//!
//! - a project folder, or its `psg-project.toml`: open that project;
//! - a LaTeX body (`.tex`) or a Word manuscript (`.docx`): start a new project from the selected
//!   journal template, with the file imported as its manuscript. The original is never changed.

use std::path::{Path, PathBuf};

use psg_academic::project::PROJECT_FILE;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// Open the project in this folder.
    Project(PathBuf),
    /// Start a new project around this manuscript.
    Manuscript { path: PathBuf, kind: ManuscriptKind },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManuscriptKind {
    Tex,
    Docx,
}

/// What `path` asks for, or why it can't be opened.
pub fn classify(path: &Path) -> Result<Request, String> {
    if path.is_dir() {
        return if path.join(PROJECT_FILE).is_file() {
            Ok(Request::Project(path.to_path_buf()))
        } else {
            Err(format!("{} is a folder without a {PROJECT_FILE}, so it isn't a PSG Academic PDF project.", path.display()))
        };
    }
    if !path.is_file() {
        return Err(format!("{} was not found.", path.display()));
    }
    if path.file_name().is_some_and(|n| n == PROJECT_FILE) {
        let dir = path.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        return Ok(Request::Project(dir));
    }
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("tex") => Ok(Request::Manuscript { path: path.to_path_buf(), kind: ManuscriptKind::Tex }),
        Some("docx") => Ok(Request::Manuscript { path: path.to_path_buf(), kind: ManuscriptKind::Docx }),
        _ => Err(format!(
            "{} isn't something PSG Academic PDF opens: choose a project folder, a .tex manuscript or a .docx manuscript.",
            path.display()
        )),
    }
}

/// The project folder name for a new project around `manuscript`: its file stem, made safe,
/// and a time stamp so it never collides with an earlier one.
pub fn project_name(manuscript: &Path, stamp: &str) -> String {
    let stem = manuscript.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let safe = psg_academic::paths::sanitize_name(&stem).filter(|s| !s.is_empty()).unwrap_or_else(|| "manuscript".to_owned());
    format!("{safe}-{stamp}")
}

/// The paths on the command line that are not a headless command (`cli::run` took those).
pub fn paths_from_args(args: &[String]) -> Vec<PathBuf> {
    args.iter().filter(|a| !a.starts_with('-')).map(PathBuf::from).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_and_manuscripts_are_told_apart() {
        let dir = tempfile::tempdir().unwrap();
        let project = dir.path().join("p");
        std::fs::create_dir(&project).unwrap();
        std::fs::write(project.join(PROJECT_FILE), "").unwrap();
        assert_eq!(classify(&project), Ok(Request::Project(project.clone())));
        assert_eq!(classify(&project.join(PROJECT_FILE)), Ok(Request::Project(project.clone())));

        let tex = dir.path().join("Body.TEX");
        std::fs::write(&tex, "x").unwrap();
        assert_eq!(classify(&tex), Ok(Request::Manuscript { path: tex.clone(), kind: ManuscriptKind::Tex }));
        let docx = dir.path().join("论文 稿.docx");
        std::fs::write(&docx, "PK").unwrap();
        assert_eq!(classify(&docx), Ok(Request::Manuscript { path: docx.clone(), kind: ManuscriptKind::Docx }));

        assert!(classify(dir.path()).unwrap_err().contains("isn't a PSG Academic PDF project"));
        assert!(classify(&dir.path().join("missing.tex")).unwrap_err().contains("not found"));
        let pdf = dir.path().join("x.pdf");
        std::fs::write(&pdf, "%PDF").unwrap();
        assert!(classify(&pdf).unwrap_err().contains(".docx"));
    }

    #[test]
    fn project_names_are_safe_for_any_file_name() {
        assert_eq!(project_name(Path::new("/a/My Paper (v2).docx"), "20261115-0610"), "My_Paper__v2_-20261115-0610");
        // A name with no safe characters left still gives a usable folder name.
        assert!(project_name(Path::new("/a/论文.docx"), "s").ends_with("-s"));
        assert!(!project_name(Path::new("/a/论文.docx"), "s").starts_with('-'));
        assert_eq!(project_name(Path::new("/a/....docx"), "s"), "manuscript-s");
    }

    #[test]
    fn options_are_not_paths() {
        assert_eq!(paths_from_args(&["--new-window".into(), "a.tex".into()]), vec![PathBuf::from("a.tex")]);
    }
}
