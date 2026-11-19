//! Records which saved inputs produced the current PDF, including across app restarts.

use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::project::{ASSET_EXTENSIONS, ProjectFile};
use crate::{Error, Project};

const STAMP: &str = ".psg-typeset.toml";
const MAX_ENTRIES: usize = 4096;
const MAX_STAMP_BYTES: u64 = 2 * 1024 * 1024;
const TEX_INPUTS: &[&str] = &["cls", "sty", "clo", "def", "cfg", "ldf", "bbx", "cbx", "lbx", "bst", "ltx", "txt", "dat", "tsv", "svg"];

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct FileState {
    size: u64,
    modified: SystemTime,
}

impl FileState {
    fn read(path: &Path) -> std::io::Result<Self> {
        let meta = fs::metadata(path)?;
        if !meta.is_file() {
            return Err(std::io::Error::other("expected a regular file"));
        }
        Ok(Self { size: meta.len(), modified: meta.modified()? })
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Input {
    path: PathBuf,
    state: FileState,
}

#[derive(Serialize, Deserialize)]
struct Revision {
    project: ProjectFile,
    inputs: Vec<Input>,
    pdf: FileState,
    generated_metadata: FileState,
}

fn definition(project: &Project) -> ProjectFile {
    let mut definition = project.file.clone();
    // Time limit and force-rebuild are run options, not manuscript inputs.
    definition.settings = Default::default();
    definition
}

pub(super) fn forget(project: &Project) -> Result<(), Error> {
    let path = project.dir.join(STAMP);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(Error::io(&path, e)),
    }
}

/// Check the supported manuscript, template, reference and figure files in the project. TeX's
/// generated logs and auxiliary files do not invalidate a result.
pub(super) fn sources(project: &Project) -> Result<Vec<Input>, Error> {
    let mut dirs = vec![project.dir.clone()];
    let mut inputs = Vec::new();
    let output = project.pdf_path();
    let generated_metadata = project.dir.join(&project.file.metadata_file);
    let mut entries = 0;
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).map_err(|e| Error::io(&dir, e))? {
            entries += 1;
            if entries > MAX_ENTRIES {
                return Err(Error::Project("too many files to verify the typeset project".into()));
            }
            let entry = entry.map_err(|e| Error::io(&dir, e))?;
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            let file_type = entry.file_type().map_err(|e| Error::io(&path, e))?;
            if file_type.is_dir() {
                dirs.push(path);
                continue;
            }
            let Some(extension) = path.extension().and_then(|ext| ext.to_str()).map(str::to_ascii_lowercase) else { continue };
            if path == output
                || path == generated_metadata
                || !(ASSET_EXTENSIONS.contains(&extension.as_str()) || TEX_INPUTS.contains(&extension.as_str()))
            {
                continue;
            }
            if file_type.is_symlink() && !path.is_file() {
                continue;
            }
            let relative = path.strip_prefix(&project.dir).map_err(|e| Error::Project(e.to_string()))?;
            inputs.push(Input { path: relative.to_path_buf(), state: FileState::read(&path).map_err(|e| Error::io(&path, e))? });
        }
    }
    inputs.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    Ok(inputs)
}

pub(super) fn record(project: &Project, inputs: Vec<Input>) -> Result<(), Error> {
    if sources(project)? != inputs {
        return Err(Error::Project("the inputs changed while typesetting".into()));
    }
    let pdf = project.pdf_path();
    let metadata = project.dir.join(&project.file.metadata_file);
    let revision = Revision {
        project: definition(project),
        inputs,
        pdf: FileState::read(&pdf).map_err(|e| Error::io(&pdf, e))?,
        generated_metadata: FileState::read(&metadata).map_err(|e| Error::io(&metadata, e))?,
    };
    let text = toml::to_string(&revision).map_err(|e| Error::Project(e.to_string()))?;
    if text.len() as u64 > MAX_STAMP_BYTES {
        return Err(Error::Project("the typeset project's input list is too large".into()));
    }
    crate::paths::write_atomic(&project.dir.join(STAMP), text.as_bytes())
}

pub(super) fn is_current(project: &Project) -> bool {
    let path = project.dir.join(STAMP);
    let mut text = String::new();
    if fs::metadata(&path).is_ok_and(|m| m.is_file() && m.len() <= MAX_STAMP_BYTES)
        && let Ok(file) = fs::File::open(&path)
        && file.take(MAX_STAMP_BYTES + 1).read_to_string(&mut text).is_ok()
        && text.len() as u64 <= MAX_STAMP_BYTES
        && let Ok(stamp) = toml::from_str::<Revision>(&text)
    {
        stamp.project == definition(project)
            && FileState::read(&project.pdf_path()).is_ok_and(|state| state == stamp.pdf)
            && FileState::read(&project.dir.join(&project.file.metadata_file)).is_ok_and(|state| state == stamp.generated_metadata)
            && stamp.inputs.iter().all(|input| {
                input.path.components().all(|c| matches!(c, Component::Normal(_)))
                    && FileState::read(&project.dir.join(&input.path)).is_ok_and(|state| state == input.state)
            })
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PublicationMetadata, Template};

    #[test]
    fn only_the_successfully_compiled_revision_is_exportable_after_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let project = Project::create(dir.path(), &Template::builtin().remove(0), PublicationMetadata::sample()).unwrap();
        fs::write(project.pdf_path(), b"%PDF-1.7").unwrap();
        assert!(!project.pdf_is_current(), "a PDF alone does not prove which draft it contains");
        let inputs = sources(&project).unwrap();
        record(&project, inputs).unwrap();
        let mut reopened = Project::open(dir.path()).unwrap();
        assert!(reopened.pdf_is_current());
        fs::write(dir.path().join("exported.pdf"), b"%PDF-1.7").unwrap();
        assert!(reopened.pdf_is_current(), "an extra output must not invalidate the current PDF");
        reopened.file.metadata.title.push_str(" revised");
        assert!(!reopened.pdf_is_current());
        assert!(project.pdf_is_current());
        project.write_body("a revised manuscript").unwrap();
        assert!(!project.pdf_is_current());
        let inputs = sources(&project).unwrap();
        record(&project, inputs).unwrap();
        forget(&project).unwrap();
        assert!(!project.pdf_is_current(), "a failed new compilation must not certify the old on-disk PDF");
    }
}
