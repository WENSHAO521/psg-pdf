//! The recent projects list on the Dashboard: project folders, newest first, one per line in a
//! text file in the data folder.

use std::path::{Path, PathBuf};

/// The most projects remembered.
pub const MAX: usize = 8;

pub struct Recent {
    file: PathBuf,
    items: Vec<PathBuf>,
}

impl Recent {
    /// Read the list from `file`; a missing or unreadable file is an empty list.
    pub fn load(file: PathBuf) -> Self {
        let items = std::fs::read_to_string(&file).map(|text| parse(&text)).unwrap_or_default();
        Self { file, items }
    }

    /// The folders, newest first.
    pub fn items(&self) -> &[PathBuf] {
        &self.items
    }

    /// Put `dir` first.
    pub fn add(&mut self, dir: &Path) {
        self.items.retain(|p| p != dir);
        self.items.insert(0, dir.to_path_buf());
        self.items.truncate(MAX);
        self.save();
    }

    /// Forget the entry at `index` (the folder itself is kept).
    pub fn remove(&mut self, index: usize) {
        if index < self.items.len() {
            self.items.remove(index);
            self.save();
        }
    }

    fn save(&self) {
        let text: String = self.items.iter().map(|p| format!("{}\n", p.display())).collect();
        // The data folder may not exist yet on a first run.
        if let Some(dir) = self.file.parent()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            log::warn!("{}: {e}", dir.display());
        }
        if let Err(e) = psg_academic::paths::write_atomic(&self.file, text.as_bytes()) {
            log::warn!("{}: {e}", self.file.display());
        }
    }
}

/// The folders in `text`, one per line, without blanks or repeats, at most [`MAX`].
fn parse(text: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let p = PathBuf::from(line);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out.truncate(MAX);
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_without_repeats_and_bounded() {
        let dir = tempfile::tempdir().unwrap();
        // A data folder that doesn't exist yet is created.
        let file = dir.path().join("data").join("recent.txt");
        let mut r = Recent::load(file.clone());
        assert!(r.items().is_empty());
        for i in 0..12 {
            r.add(&dir.path().join(format!("p{i}")));
        }
        r.add(&dir.path().join("p5"));
        assert_eq!(r.items().len(), MAX);
        assert_eq!(r.items().first(), Some(&dir.path().join("p5")));
        assert_eq!(r.items().iter().filter(|p| p.ends_with("p5")).count(), 1);
        let again = Recent::load(file);
        assert_eq!(again.items(), r.items());
    }

    #[test]
    fn blank_and_repeated_lines_are_ignored() {
        assert_eq!(parse("\n/a\n  \n/a\n/b\n"), vec![PathBuf::from("/a"), PathBuf::from("/b")]);
    }
}
