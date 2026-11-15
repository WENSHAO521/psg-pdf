//! Where PSG Academic PDF keeps its window settings and its projects folder.
//!
//! Normally that is the per-user folder (`%APPDATA%\PSG Academic PDF`, `~/Library/Application
//! Support/PSG Academic PDF`, `~/.local/share/PSG Academic PDF`). In the Windows portable zip a
//! `portable.txt` sits beside the exe, and everything goes to `PSGAcademicPDFData` beside it
//! instead, so the folder can be carried around; nothing is written to the user's profile.

use std::path::{Path, PathBuf};

/// The file whose presence beside the exe switches portable mode on.
pub const PORTABLE_MARKER: &str = "portable.txt";
/// The data folder beside the exe in portable mode.
pub const PORTABLE_DIR: &str = "PSGAcademicPDFData";

/// The data folder and how it was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDir {
    pub root: PathBuf,
    pub portable: bool,
    /// Said in the Activity log: why portable mode could not be used.
    pub note: Option<String>,
}

impl DataDir {
    /// Where projects are created.
    pub fn projects(&self) -> PathBuf {
        self.root.join("projects")
    }

    /// The data folder for this run: portable when `exe_dir` holds the marker and the folder
    /// beside it can be created, otherwise `user` (eframe's per-user folder), otherwise a folder
    /// in the system temp directory.
    pub fn resolve(exe_dir: Option<&Path>, user: Option<PathBuf>) -> Self {
        let fallback = |note: Option<String>| DataDir {
            root: user.clone().unwrap_or_else(|| std::env::temp_dir().join("psg-academic-pdf")),
            portable: false,
            note,
        };
        let Some(exe_dir) = exe_dir else { return fallback(None) };
        if !exe_dir.join(PORTABLE_MARKER).is_file() {
            return fallback(None);
        }
        let root = exe_dir.join(PORTABLE_DIR);
        match std::fs::create_dir_all(&root) {
            Ok(()) => DataDir { root, portable: true, note: None },
            Err(e) => {
                fallback(Some(format!("Portable mode is on, but {} can't be written ({e}); using the per-user folder instead.", root.display())))
            }
        }
    }

    /// For the running program.
    pub fn for_this_run(app_name: &str) -> Self {
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        Self::resolve(exe_dir.as_deref(), eframe::storage_dir(app_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_the_marker_the_user_folder_is_used() {
        let exe = tempfile::tempdir().unwrap();
        let user = exe.path().join("user");
        let d = DataDir::resolve(Some(exe.path()), Some(user.clone()));
        assert_eq!(d, DataDir { root: user.clone(), portable: false, note: None });
        assert_eq!(d.projects(), user.join("projects"));
        assert!(!exe.path().join(PORTABLE_DIR).exists(), "nothing is written beside the exe");
    }

    #[test]
    fn the_marker_keeps_everything_beside_the_exe() {
        let exe = tempfile::tempdir().unwrap();
        std::fs::write(exe.path().join(PORTABLE_MARKER), "portable").unwrap();
        let d = DataDir::resolve(Some(exe.path()), Some(exe.path().join("user")));
        assert!(d.portable);
        assert_eq!(d.root, exe.path().join(PORTABLE_DIR));
        assert!(d.root.is_dir());
    }

    #[test]
    fn an_unwritable_portable_folder_falls_back_and_says_why() {
        let exe = tempfile::tempdir().unwrap();
        std::fs::write(exe.path().join(PORTABLE_MARKER), "portable").unwrap();
        // A file where the folder should go: create_dir_all fails on every platform.
        std::fs::write(exe.path().join(PORTABLE_DIR), "in the way").unwrap();
        let d = DataDir::resolve(Some(exe.path()), Some(exe.path().join("user")));
        assert!(!d.portable);
        assert_eq!(d.root, exe.path().join("user"));
        assert!(d.note.unwrap().contains("Portable mode is on"));
    }

    #[test]
    fn no_folder_at_all_uses_the_temp_directory() {
        let d = DataDir::resolve(None, None);
        assert!(d.root.starts_with(std::env::temp_dir()));
    }
}
