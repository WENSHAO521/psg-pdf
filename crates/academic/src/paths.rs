//! File names and writes inside a project directory.

use std::io::Write;
use std::path::Path;

use crate::Error;

/// The longest file name accepted.
const MAX_NAME: usize = 128;

/// `name` if it is a plain file name that is safe to hand to latexmk and to create inside a
/// project directory: ASCII letters, digits, `-`, `_` and `.`, not starting with `.` or `-`, no
/// directory parts. TeX and latexmk treat spaces and many other characters specially.
pub fn safe_relative_name(name: &str) -> Option<&str> {
    let ok = !name.is_empty()
        && name.len() <= MAX_NAME
        && !name.starts_with('.')
        && !name.starts_with('-')
        && !name.contains("..")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    ok.then_some(name)
}

/// A safe file name made from `original` (an imported file's name): unsafe characters become `_`.
/// Returns `None` when nothing usable is left.
pub fn sanitize_name(original: &str) -> Option<String> {
    let mut out: String = original.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') { c } else { '_' }).collect();
    while out.contains("..") {
        out = out.replace("..", ".");
    }
    let out = out.trim_start_matches(['.', '-', '_']).to_owned();
    let out: String = if out.len() > MAX_NAME {
        // Keep the extension.
        let ext = out.rsplit_once('.').map(|(_, e)| e.to_owned()).unwrap_or_default();
        let stem: String = out.chars().take(MAX_NAME.saturating_sub(ext.len() + 1)).collect();
        if ext.is_empty() { stem } else { format!("{stem}.{ext}") }
    } else {
        out
    };
    safe_relative_name(&out).map(str::to_owned)
}

/// Write `bytes` to `path` through a temporary file in the same directory and a rename, so a
/// crash or a full disk never leaves a half-written file behind.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let dir = path.parent().ok_or_else(|| Error::Project(format!("{} has no parent directory", path.display())))?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::io(path, e));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_names() {
        for ok in ["main.tex", "body.tex", "fig-1.png", "a_b.bib"] {
            assert_eq!(safe_relative_name(ok), Some(ok));
        }
        for bad in ["", "../x.tex", "a/b.tex", "a\\b.tex", ".latexmkrc", "-pdf", "a b.tex", "x;rm.tex", "x..tex", "é.tex"] {
            assert_eq!(safe_relative_name(bad), None, "{bad}");
        }
    }

    #[test]
    fn sanitizing_imported_names() {
        assert_eq!(sanitize_name("My Paper (final).tex").as_deref(), Some("My_Paper__final_.tex"));
        assert_eq!(sanitize_name(".latexmkrc").as_deref(), Some("latexmkrc"));
        assert_eq!(sanitize_name("../../etc/passwd").as_deref(), Some("etc_passwd"));
        assert_eq!(sanitize_name("...").as_deref(), None);
        let long = format!("{}.png", "a".repeat(400));
        let s = sanitize_name(&long).unwrap();
        assert!(s.len() <= MAX_NAME && s.ends_with(".png"));
    }

    #[test]
    fn atomic_write_replaces_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.tex");
        write_atomic(&p, b"one").unwrap();
        write_atomic(&p, b"two").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"two");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1, "no temporary file left behind");
    }
}
