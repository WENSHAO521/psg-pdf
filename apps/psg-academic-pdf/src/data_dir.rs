//! Where PSG Academic PDF keeps its window settings and its projects folder.
//!
//! Normally that is the per-user folder (`%APPDATA%\PSG Academic PDF`, `~/Library/Application
//! Support/PSG Academic PDF`, `~/.local/share/PSG Academic PDF`). In the Windows portable zip a
//! `portable.txt` sits beside the exe, and everything goes to `PSGAcademicPDFData` beside it
//! instead, so the folder can be carried around; nothing is written to the user's profile.
//!
//! Either default folder (the "home" folder) can hand everything on to a folder the person chose
//! in Settings: [`LOCATION_FILE`] in the home folder names it. The pointer lives in the home
//! folder, which installers and updates never touch, so the choice survives updates and
//! reinstalls. When the chosen folder can't be used (a disconnected drive), the home folder is
//! used for that run and the Activity log says why; the choice itself is kept.

use std::path::{Component, Path, PathBuf};

/// The file whose presence beside the exe switches portable mode on.
pub const PORTABLE_MARKER: &str = "portable.txt";
/// The data folder beside the exe in portable mode.
pub const PORTABLE_DIR: &str = "PSGAcademicPDFData";
/// In the home folder: the full path of the data folder the person chose, on one line.
pub const LOCATION_FILE: &str = "data-location.txt";
/// The workspace choice, stored with the rest of the settings.
pub const WORKSPACE_FILE: &str = "workspace-location.txt";

/// The data folder and how it was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDir {
    /// The folder in use.
    pub root: PathBuf,
    /// The default folder, which holds [`LOCATION_FILE`]; `root` unless one was chosen.
    pub home: PathBuf,
    pub portable: bool,
    /// Said in the Activity log: why portable mode could not be used.
    pub note: Option<String>,
}

impl DataDir {
    /// Where projects are created.
    pub fn projects(&self) -> PathBuf {
        self.chosen_workspace().filter(|path| path.is_dir()).unwrap_or_else(|| self.root.join("projects"))
    }

    pub fn chosen_workspace(&self) -> Option<PathBuf> {
        let text = std::fs::read_to_string(self.root.join(WORKSPACE_FILE)).ok()?;
        let path = PathBuf::from(text.trim_end_matches(['\r', '\n']));
        if path.is_absolute() {
            Some(path)
        } else if !path.as_os_str().is_empty() && path.components().all(|part| matches!(part, Component::CurDir | Component::Normal(_))) {
            Some(self.root.join(path))
        } else {
            None
        }
    }

    pub fn remember_workspace(&self, folder: Option<&Path>) -> std::io::Result<()> {
        let location = self.root.join(WORKSPACE_FILE);
        let Some(folder) = folder else {
            return match std::fs::remove_file(&location) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                result => result,
            };
        };
        if !folder.is_absolute() || !folder.is_dir() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "the workspace must be an existing folder"));
        }
        // A workspace inside the data folder follows it when the data folder is moved.
        let path = folder
            .strip_prefix(&self.root)
            .ok()
            .filter(|path| path.components().all(|part| matches!(part, Component::CurDir | Component::Normal(_))))
            .unwrap_or(folder);
        let path = if path.as_os_str().is_empty() { Path::new(".") } else { path };
        let text = path.to_str().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "the folder path is not Unicode"))?;
        if text.contains(['\r', '\n']) {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "the folder path contains a line break"));
        }
        // The selected folder must allow creating project directories. Check before changing
        // the saved choice so a read-only drive leaves the working location selected.
        let probe = folder.join(format!(".psg-workspace-check-{}", std::process::id()));
        std::fs::create_dir(&probe)?;
        std::fs::remove_dir(&probe)?;
        std::fs::create_dir_all(&self.root)?;
        psg_academic::paths::write_atomic(&location, format!("{text}\n").as_bytes()).map_err(|e| std::io::Error::other(e.to_string()))
    }

    /// Whether the person chose this folder (rather than it being the default).
    pub fn is_custom(&self) -> bool {
        self.root != self.home
    }

    /// The data folder for this run: the home folder is portable when `exe_dir` holds the marker
    /// and the folder beside it can be created, otherwise `user` (eframe's per-user folder),
    /// otherwise a folder in the system temp directory. Then the folder the home folder points
    /// to, if one was chosen and can be used.
    pub fn resolve(exe_dir: Option<&Path>, user: Option<PathBuf>) -> Self {
        Self::home(exe_dir, user).follow()
    }

    fn home(exe_dir: Option<&Path>, user: Option<PathBuf>) -> Self {
        let fallback = |note: Option<String>| {
            let root = user.clone().unwrap_or_else(|| std::env::temp_dir().join("psg-academic-pdf"));
            DataDir { home: root.clone(), root, portable: false, note }
        };
        let Some(exe_dir) = exe_dir else { return fallback(None) };
        if !exe_dir.join(PORTABLE_MARKER).is_file() {
            return fallback(None);
        }
        let root = exe_dir.join(PORTABLE_DIR);
        match std::fs::create_dir_all(&root) {
            Ok(()) => DataDir { home: root.clone(), root, portable: true, note: None },
            Err(e) => {
                fallback(Some(format!("Portable mode is on, but {} can't be written ({e}); using the per-user folder instead.", root.display())))
            }
        }
    }

    /// Use the folder [`LOCATION_FILE`] names, when it is a full path that exists or can be made.
    fn follow(mut self) -> Self {
        let Some(chosen) = chosen(&self.home) else { return self };
        let problem = if !chosen.is_absolute() {
            format!("{} is not a full path", chosen.display())
        } else {
            match std::fs::create_dir_all(&chosen) {
                Ok(()) => {
                    self.root = chosen;
                    return self;
                }
                Err(e) => format!("{} can't be used ({e})", chosen.display()),
            }
        };
        let note = format!("The chosen data folder {problem}; using {} for now.", self.home.display());
        log::warn!("{note}");
        self.note = Some(match self.note.take() {
            Some(before) => format!("{before} {note}"),
            None => note,
        });
        self
    }

    /// For the running program.
    pub fn for_this_run(app_name: &str) -> Self {
        let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        Self::resolve(exe_dir.as_deref(), eframe::storage_dir(app_name))
    }
}

/// The folder `home`'s [`LOCATION_FILE`] names, if any.
pub fn chosen(home: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(home.join(LOCATION_FILE)).ok()?;
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    Some(PathBuf::from(line))
}

/// Remember `root` as the data folder in `home`: the pointer is written (atomically), or removed
/// when `root` is `home` itself.
pub fn remember(home: &Path, root: &Path) -> std::io::Result<()> {
    let file = home.join(LOCATION_FILE);
    if root == home {
        return match std::fs::remove_file(&file) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    }
    std::fs::create_dir_all(home)?;
    psg_academic::paths::write_atomic(&file, format!("{}\n", root.display()).as_bytes()).map_err(|e| std::io::Error::other(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_existing_workspace_is_remembered_and_can_be_reset() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("Manuscripts");
        std::fs::create_dir(&workspace).unwrap();
        std::fs::write(workspace.join("notes.txt"), "keep this").unwrap();
        let root = dir.path().join("data");
        let data = DataDir::resolve(None, Some(root.clone()));
        data.remember_workspace(Some(&workspace)).unwrap();
        assert_eq!(DataDir::resolve(None, Some(root.clone())).projects(), workspace);
        assert!(data.remember_workspace(Some(&workspace.join("missing"))).is_err());
        assert_eq!(data.projects(), workspace);
        data.remember_workspace(None).unwrap();
        assert_eq!(data.projects(), root.join("projects"));
        assert_eq!(std::fs::read_to_string(workspace.join("notes.txt")).unwrap(), "keep this");
    }

    #[test]
    fn without_the_marker_the_user_folder_is_used() {
        let exe = tempfile::tempdir().unwrap();
        let user = exe.path().join("user");
        let d = DataDir::resolve(Some(exe.path()), Some(user.clone()));
        assert_eq!(d, DataDir { root: user.clone(), home: user.clone(), portable: false, note: None });
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
    fn a_chosen_folder_is_followed() {
        let t = tempfile::tempdir().unwrap();
        let (user, chosen) = (t.path().join("user"), t.path().join("chosen"));
        remember(&user, &chosen).unwrap();
        let d = DataDir::resolve(None, Some(user.clone()));
        assert_eq!((d.root.clone(), d.home.clone()), (chosen.clone(), user.clone()));
        assert!(d.is_custom() && d.note.is_none());
        assert_eq!(d.projects(), chosen.join("projects"));
        // Choosing the default again forgets the pointer.
        remember(&user, &user).unwrap();
        assert!(!user.join(LOCATION_FILE).exists());
        assert!(!DataDir::resolve(None, Some(user)).is_custom());
    }

    #[test]
    fn an_unusable_chosen_folder_falls_back_and_keeps_the_choice() {
        let t = tempfile::tempdir().unwrap();
        let user = t.path().join("user");
        // A file where the folder should be: it can't be created on any platform.
        std::fs::write(t.path().join("blocked"), "in the way").unwrap();
        remember(&user, &t.path().join("blocked")).unwrap();
        let d = DataDir::resolve(None, Some(user.clone()));
        assert_eq!(d.root, user);
        assert!(d.note.unwrap().contains("can't be used"));
        assert!(user.join(LOCATION_FILE).is_file(), "the choice is kept for when the drive is back");
        std::fs::write(user.join(LOCATION_FILE), "relative/path\n").unwrap();
        assert!(DataDir::resolve(None, Some(user.clone())).note.unwrap().contains("not a full path"));
    }

    #[test]
    fn the_portable_folder_can_point_elsewhere_too() {
        let exe = tempfile::tempdir().unwrap();
        std::fs::write(exe.path().join(PORTABLE_MARKER), "portable").unwrap();
        let chosen = exe.path().join("stick");
        remember(&exe.path().join(PORTABLE_DIR), &chosen).unwrap();
        let d = DataDir::resolve(Some(exe.path()), Some(exe.path().join("user")));
        assert!(d.portable);
        assert_eq!((d.root, d.home), (chosen, exe.path().join(PORTABLE_DIR)));
    }

    #[test]
    fn no_folder_at_all_uses_the_temp_directory() {
        let d = DataDir::resolve(None, None);
        assert!(d.root.starts_with(std::env::temp_dir()));
    }
}
