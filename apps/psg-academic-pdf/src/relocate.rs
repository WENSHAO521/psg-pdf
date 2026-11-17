//! Moving the data folder somewhere the person chooses (Settings ▸ Data folder).
//!
//! The move is a copy first: every file is copied and its size checked before anything else
//! changes, so a full disk or a pulled USB stick halfway through leaves the old folder exactly as
//! it was (and whatever was copied is removed again). Only then is the new place remembered (the
//! pointer in [`crate::data_dir::LOCATION_FILE`]) and the copied originals removed from the old
//! folder. Folders are only removed when empty, so anything the app didn't copy stays where it
//! was.
//!
//! What already sits in the new folder is never overwritten, with one exception: the app's own
//! one-line settings at the top of the folder (language, theme, update channel, recent projects,
//! window layout), which are exactly what is being carried over. A project folder that is already
//! there stops the move before anything is copied.

use std::path::{Path, PathBuf};

use crate::data_dir::LOCATION_FILE;

/// The settings files at the top of the data folder that a move may replace in the new folder.
pub const SETTINGS_FILES: [&str; 5] = ["app.ron", "language.txt", "theme.txt", "update-channel.txt", RECENT_FILE];
/// The recent projects list, whose paths are moved along with the projects.
pub const RECENT_FILE: &str = "recent-projects.txt";
/// The log folder stays behind: this run is still writing to it, and the next run starts a new
/// log in the new folder.
const LOGS: &str = "logs";
/// How deep a data folder is walked. Projects are a few levels deep; this only stops a cycle.
const MAX_DEPTH: usize = 64;
/// The most files a move will copy, so a wrongly chosen folder can't run for hours.
const MAX_FILES: usize = 200_000;

/// Why a move could not be done. Nothing has changed when one of these is returned.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// The chosen place is not a full path.
    NotAbsolute(PathBuf),
    /// The chosen place is the folder already in use.
    Same,
    /// The chosen place is inside the folder in use: it would copy itself.
    Inside(PathBuf),
    /// A file or project is already there and would be overwritten.
    Exists(PathBuf),
    /// Too many files or folders nested too deeply to be a data folder.
    TooLarge,
    /// Reading, copying or remembering failed.
    Io { path: PathBuf, message: String },
}

impl Error {
    fn io(path: &Path, e: &dyn std::fmt::Display) -> Self {
        Error::Io { path: path.to_path_buf(), message: e.to_string() }
    }
}

/// What a finished move did.
#[derive(Debug, PartialEq, Eq)]
pub struct Moved {
    pub to: PathBuf,
    pub files: usize,
    pub bytes: u64,
    /// Old files or folders that could not be removed after the copy (the copy is complete;
    /// these are just left behind in the old folder).
    pub left_behind: usize,
}

/// Where the data goes when `chosen` is picked: `chosen` itself when it is missing or empty,
/// otherwise a folder named `name` inside it, so a full folder such as Documents isn't strewn
/// with settings files.
pub fn destination(chosen: &Path, name: &str) -> PathBuf {
    let empty = match std::fs::read_dir(chosen) {
        Ok(mut entries) => entries.next().is_none(),
        Err(_) => !chosen.exists(),
    };
    if empty { chosen.to_path_buf() } else { chosen.join(name) }
}

/// `path` with symbolic links and `..` resolved as far as it exists, the rest appended, so two
/// spellings of one folder compare equal.
fn resolved(path: &Path) -> PathBuf {
    let mut rest = Vec::new();
    let mut cur = path.to_path_buf();
    loop {
        if let Ok(real) = std::fs::canonicalize(&cur) {
            return rest.iter().rev().fold(real, |acc: PathBuf, part: &std::ffi::OsString| acc.join(part));
        }
        match (cur.file_name().map(ToOwned::to_owned), cur.parent().map(Path::to_path_buf)) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                cur = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// Whether `from` can be moved to `to`, without touching anything.
pub fn check(from: &Path, to: &Path) -> Result<(), Error> {
    if !to.is_absolute() {
        return Err(Error::NotAbsolute(to.to_path_buf()));
    }
    let (from_r, to_r) = (resolved(from), resolved(to));
    if from_r == to_r {
        return Err(Error::Same);
    }
    if to_r.starts_with(&from_r) {
        return Err(Error::Inside(to.to_path_buf()));
    }
    Ok(())
}

/// One file to copy, relative to the data folder.
struct Item {
    rel: PathBuf,
    len: u64,
}

/// The files and folders under `from` that move, the logs and the location pointer left out.
fn plan(from: &Path) -> Result<(Vec<PathBuf>, Vec<Item>), Error> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    let mut stack = vec![(PathBuf::new(), 0usize)];
    while let Some((rel, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            return Err(Error::TooLarge);
        }
        let dir = from.join(&rel);
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            // A data folder that was never created has nothing to move.
            Err(e) if rel.as_os_str().is_empty() && e.kind() == std::io::ErrorKind::NotFound => return Ok((dirs, files)),
            Err(e) => return Err(Error::io(&dir, &e)),
        };
        for entry in entries {
            let entry = entry.map_err(|e| Error::io(&dir, &e))?;
            let name = entry.file_name();
            if depth == 0 && (name == LOGS || name == LOCATION_FILE) {
                continue;
            }
            let child = rel.join(&name);
            // Not followed: a link is not the person's data, and following one could loop.
            let kind = entry.file_type().map_err(|e| Error::io(&entry.path(), &e))?;
            if kind.is_dir() {
                dirs.push(child.clone());
                stack.push((child, depth.saturating_add(1)));
            } else if kind.is_file() {
                let len = entry.metadata().map_err(|e| Error::io(&entry.path(), &e))?.len();
                files.push(Item { rel: child, len });
            } else {
                log::warn!("data folder move: {} is not a file or folder; left where it is", entry.path().display());
            }
            if dirs.len().saturating_add(files.len()) > MAX_FILES {
                return Err(Error::TooLarge);
            }
        }
    }
    dirs.sort();
    Ok((dirs, files))
}

/// Whether the file at `rel` may replace one already in the new folder.
fn replaceable(rel: &Path) -> bool {
    rel.parent().is_some_and(|p| p.as_os_str().is_empty()) && SETTINGS_FILES.iter().any(|f| rel == Path::new(f))
}

/// The recent projects list with every project that lived in `from` pointing into `to`.
pub fn rebase_recent(text: &str, from: &Path, to: &Path) -> String {
    text.lines()
        .map(|line| {
            let p = Path::new(line.trim());
            match p.strip_prefix(from) {
                Ok(rest) if !line.trim().is_empty() => format!("{}\n", to.join(rest).display()),
                _ => format!("{line}\n"),
            }
        })
        .collect()
}

/// `path` moved from `from` to `to` when it was inside `from`.
pub fn rebase(path: &Path, from: &Path, to: &Path) -> Option<PathBuf> {
    path.strip_prefix(from).ok().map(|rest| to.join(rest))
}

/// Copy everything from `from` to `to`, remember `to` in `home` (or forget the pointer when `to`
/// is `home`), then remove the copied originals. See the module documentation for the order.
pub fn move_data(from: &Path, to: &Path, home: &Path) -> Result<Moved, Error> {
    check(from, to)?;
    let (dirs, files) = plan(from)?;
    // Everything that would be overwritten is found before anything is written.
    for item in &files {
        let dest = to.join(&item.rel);
        if dest.exists() && !replaceable(&item.rel) {
            return Err(Error::Exists(dest));
        }
    }
    let created_root = !to.exists();
    let mut made_dirs: Vec<PathBuf> = Vec::new();
    let mut made_files: Vec<PathBuf> = Vec::new();
    let copied = (|| -> Result<u64, Error> {
        std::fs::create_dir_all(to).map_err(|e| Error::io(to, &e))?;
        for rel in &dirs {
            let dest = to.join(rel);
            if !dest.is_dir() {
                std::fs::create_dir(&dest).map_err(|e| Error::io(&dest, &e))?;
                made_dirs.push(dest);
            }
        }
        let mut bytes = 0u64;
        for item in &files {
            let (src, dest) = (from.join(&item.rel), to.join(&item.rel));
            let existed = dest.exists();
            let written = if item.rel == Path::new(RECENT_FILE) {
                let text = std::fs::read_to_string(&src).map_err(|e| Error::io(&src, &e))?;
                let text = rebase_recent(&text, from, to);
                psg_academic::paths::write_atomic(&dest, text.as_bytes()).map_err(|e| Error::io(&dest, &e))?;
                if !existed {
                    made_files.push(dest.clone());
                }
                item.len
            } else {
                // Copied beside the target and renamed into place, so a replaced settings file is
                // never left half written.
                let tmp = dest.with_file_name(format!(".{}.moving", item.rel.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()));
                let n = std::fs::copy(&src, &tmp).map_err(|e| Error::io(&src, &e))?;
                if n != item.len {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(Error::io(&src, &format!("copied {n} of {} bytes", item.len)));
                }
                std::fs::rename(&tmp, &dest).map_err(|e| {
                    let _ = std::fs::remove_file(&tmp);
                    Error::io(&dest, &e)
                })?;
                if !existed {
                    made_files.push(dest);
                }
                n
            };
            bytes = bytes.saturating_add(written);
        }
        crate::data_dir::remember(home, to).map_err(|e| Error::io(&home.join(LOCATION_FILE), &e))?;
        Ok(bytes)
    })();
    let bytes = match copied {
        Ok(b) => b,
        Err(e) => {
            // Undo: only what this move created. A settings file it replaced keeps the copied
            // values, which the old folder (still the one in use) holds too.
            for f in made_files.iter().rev() {
                let _ = std::fs::remove_file(f);
            }
            for d in made_dirs.iter().rev() {
                let _ = std::fs::remove_dir(d);
            }
            if created_root {
                let _ = std::fs::remove_dir(to);
            }
            return Err(e);
        }
    };
    // The new folder is complete and remembered; now the old copies go.
    let mut left_behind = 0usize;
    for item in &files {
        let src = from.join(&item.rel);
        if let Err(e) = std::fs::remove_file(&src) {
            log::warn!("data folder move: {} was copied but not removed: {e}", src.display());
            left_behind = left_behind.saturating_add(1);
        }
    }
    for rel in dirs.iter().rev() {
        let dir = from.join(rel);
        // Only an empty folder is removed: anything the move left alone stays.
        if std::fs::remove_dir(&dir).is_err() && dir.exists() {
            left_behind = left_behind.saturating_add(1);
        }
    }
    Ok(Moved { to: to.to_path_buf(), files: files.len(), bytes, left_behind })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// A data folder with settings, logs and one project.
    fn sample(root: &Path) {
        std::fs::create_dir_all(root.join("projects/p1/figures")).unwrap();
        std::fs::create_dir_all(root.join("logs")).unwrap();
        std::fs::write(root.join("language.txt"), "zh-hant").unwrap();
        std::fs::write(root.join("logs/psg-academic-pdf.log"), "log").unwrap();
        std::fs::write(root.join("projects/p1/body.tex"), "Hello").unwrap();
        std::fs::write(root.join("projects/p1/figures/a.png"), [1u8, 2, 3]).unwrap();
        let recent = format!("{}\n/elsewhere/p9\n", root.join("projects/p1").display());
        std::fs::write(root.join(RECENT_FILE), recent).unwrap();
    }

    #[test]
    fn moves_projects_and_settings_and_remembers_the_place() {
        let t = tempfile::tempdir().unwrap();
        let (home, to) = (t.path().join("home"), t.path().join("new"));
        sample(&home);
        let moved = move_data(&home, &to, &home).unwrap();
        assert_eq!(moved.files, 4);
        assert_eq!(moved.left_behind, 0);
        assert_eq!(std::fs::read_to_string(to.join("projects/p1/body.tex")).unwrap(), "Hello");
        assert_eq!(std::fs::read(to.join("projects/p1/figures/a.png")).unwrap(), [1, 2, 3]);
        assert_eq!(std::fs::read_to_string(to.join("language.txt")).unwrap(), "zh-hant");
        // The recent list follows the projects; a project kept elsewhere is untouched.
        let recent = std::fs::read_to_string(to.join(RECENT_FILE)).unwrap();
        assert_eq!(recent, format!("{}\n/elsewhere/p9\n", to.join("projects/p1").display()));
        // The originals are gone, the log stays behind, and the pointer names the new place.
        assert!(!home.join("projects").exists());
        assert!(!home.join("language.txt").exists());
        assert!(home.join("logs/psg-academic-pdf.log").is_file());
        assert!(!to.join("logs").exists());
        let d = crate::data_dir::DataDir::resolve(None, Some(home.clone()));
        assert_eq!(d.root, to);
        assert_eq!(d.home, home);
    }

    #[test]
    fn moving_back_to_the_default_folder_forgets_the_pointer() {
        let t = tempfile::tempdir().unwrap();
        let (home, to) = (t.path().join("home"), t.path().join("new"));
        sample(&home);
        move_data(&home, &to, &home).unwrap();
        // This run's window layout is written to the old place when it closes.
        std::fs::write(home.join("app.ron"), "old").unwrap();
        std::fs::write(to.join("app.ron"), "new").unwrap();
        move_data(&to, &home, &home).unwrap();
        assert!(!home.join(LOCATION_FILE).exists());
        assert_eq!(std::fs::read_to_string(home.join("app.ron")).unwrap(), "new", "the settings in use replace the stale ones");
        assert!(home.join("projects/p1/body.tex").is_file());
        assert_eq!(crate::data_dir::DataDir::resolve(None, Some(home.clone())).root, home);
    }

    #[test]
    fn an_existing_project_stops_the_move_before_anything_changes() {
        let t = tempfile::tempdir().unwrap();
        let (home, to) = (t.path().join("home"), t.path().join("new"));
        sample(&home);
        std::fs::create_dir_all(to.join("projects/p1")).unwrap();
        std::fs::write(to.join("projects/p1/body.tex"), "Theirs").unwrap();
        let err = move_data(&home, &to, &home).unwrap_err();
        assert_eq!(err, Error::Exists(to.join("projects/p1/body.tex")));
        assert_eq!(std::fs::read_to_string(to.join("projects/p1/body.tex")).unwrap(), "Theirs");
        assert!(!to.join("language.txt").exists());
        assert!(home.join("projects/p1/body.tex").is_file());
        assert!(!home.join(LOCATION_FILE).exists());
    }

    #[test]
    fn a_failed_copy_removes_what_it_made_and_keeps_the_old_folder() {
        let t = tempfile::tempdir().unwrap();
        let (home, to) = (t.path().join("home"), t.path().join("new"));
        sample(&home);
        // The pointer can't be written (a folder is in its way), the last step before removal.
        std::fs::create_dir_all(home.join(LOCATION_FILE)).unwrap();
        assert!(matches!(move_data(&home, &to, &home), Err(Error::Io { .. })));
        assert!(!to.exists(), "the new folder is removed again");
        assert!(home.join("projects/p1/body.tex").is_file());
        assert!(home.join("language.txt").is_file());
    }

    #[test]
    fn a_folder_inside_the_current_one_or_the_same_one_is_refused() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().join("home");
        sample(&home);
        assert_eq!(check(&home, &home.join("projects/x")), Err(Error::Inside(home.join("projects/x"))));
        assert_eq!(check(&home, &home.join("projects/..")), Err(Error::Same));
        assert_eq!(check(&home, Path::new("relative")), Err(Error::NotAbsolute(PathBuf::from("relative"))));
        assert_eq!(check(&home, &t.path().join("other")), Ok(()));
    }

    #[test]
    fn a_full_folder_gets_a_named_subfolder() {
        let t = tempfile::tempdir().unwrap();
        let empty = t.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(destination(&empty, "PSG Academic PDF"), empty);
        assert_eq!(destination(&t.path().join("missing"), "PSG Academic PDF"), t.path().join("missing"));
        std::fs::write(empty.join("notes.txt"), "x").unwrap();
        assert_eq!(destination(&empty, "PSG Academic PDF"), empty.join("PSG Academic PDF"));
    }
}
