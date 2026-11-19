//! "Open in PSG PDF": the two products are installed independently, so PSG PDF is looked for
//! where its own installers put it, not only beside this program. Without it the PDF opens in the
//! system's default viewer instead.
//!
//! - beside this program (a development build, or both portable zips unpacked into one folder);
//! - Windows: the App Paths entry PSG PDF's MSI writes (per-user, then per-machine);
//! - macOS: LaunchServices, by PSG PDF's bundle id;
//! - Linux: `psg-pdf` on `PATH` (the deb and rpm put it in /usr/bin).

use std::path::{Path, PathBuf};
use std::process::Command;

/// PSG PDF's program name (packaging/products.toml).
const PSG_PDF_EXE: &str = if cfg!(windows) { "psg-pdf.exe" } else { "psg-pdf" };
/// PSG PDF's macOS bundle id (packaging/products.toml `app_id`).
#[cfg(target_os = "macos")]
const PSG_PDF_BUNDLE_ID: &str = "com.panoramascholarly.psgpdf";

/// How the PDF was opened, for the Activity log.
#[derive(Debug, PartialEq, Eq)]
pub enum Opened {
    InPsgPdf,
    InDefaultViewer,
}

/// Opens `pdf` in PSG PDF, or in the default viewer when PSG PDF isn't installed.
pub fn open(pdf: &Path) -> Result<Opened, String> {
    if let Some(exe) = find() {
        return Command::new(&exe)
            .arg("--new-window")
            .arg(pdf)
            .spawn()
            .map(|_| Opened::InPsgPdf)
            .map_err(|e| format!("Could not start PSG PDF ({}): {e}", exe.display()));
    }
    #[cfg(target_os = "macos")]
    if Command::new("open").arg("-b").arg(PSG_PDF_BUNDLE_ID).arg(pdf).status().is_ok_and(|s| s.success()) {
        return Ok(Opened::InPsgPdf);
    }
    open_with_default_viewer(pdf).map(|()| Opened::InDefaultViewer)
}

/// PSG PDF's program, where one can be found.
fn find() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(PSG_PDF_EXE)));
    beside.into_iter().chain(installed()).find(|p| p.is_file())
}

#[cfg(windows)]
fn installed() -> Vec<PathBuf> {
    ["HKCU", "HKLM"]
        .iter()
        .filter_map(|root| {
            let key = format!(r"{root}\Software\Microsoft\Windows\CurrentVersion\App Paths\{PSG_PDF_EXE}");
            let out = hidden(Command::new("reg").args(["query", &key, "/ve"])).output().ok()?;
            out.status.success().then(|| reg_default_value(&String::from_utf8_lossy(&out.stdout))).flatten()
        })
        .collect()
}

#[cfg(all(unix, not(target_os = "macos")))]
fn installed() -> Vec<PathBuf> {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).map(|d| d.join(PSG_PDF_EXE)).collect()).unwrap_or_default()
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
fn installed() -> Vec<PathBuf> {
    Vec::new()
}

/// The data of `reg query <key> /ve`: the line `    (Default)    REG_SZ    C:\…\psg-pdf.exe`
/// (the "(Default)" label is translated, the type is not).
#[cfg_attr(not(windows), allow(dead_code))]
fn reg_default_value(text: &str) -> Option<PathBuf> {
    text.lines().find_map(|line| {
        let (_, rest) = line.split_once("REG_")?;
        let (_, value) = rest.split_once(char::is_whitespace)?;
        let value = value.trim().trim_matches('"');
        (!value.is_empty()).then(|| PathBuf::from(value))
    })
}

/// Show `dir` in the file manager (Explorer, Finder, the desktop's default).
pub fn open_folder(dir: &Path) -> Result<(), String> {
    let program = if cfg!(windows) {
        "explorer"
    } else if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let mut cmd = Command::new(program);
    cmd.arg(dir);
    hidden(&mut cmd).spawn().map(|_| ()).map_err(|e| format!("{}: {e}", dir.display()))
}

fn open_with_default_viewer(pdf: &Path) -> Result<(), String> {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("explorer");
        c.arg(pdf);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        c.arg(pdf);
        c
    } else {
        let mut c = Command::new("xdg-open");
        c.arg(pdf);
        c
    };
    hidden(&mut cmd).spawn().map(|_| ()).map_err(|e| format!("PSG PDF isn't installed, and the default PDF viewer could not be started: {e}"))
}

/// No console window flashes up on Windows.
fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_app_paths_entry_is_read_in_any_language() {
        let en = "\r\nHKEY_LOCAL_MACHINE\\Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\psg-pdf.exe\r\n    (Default)    REG_SZ    C:\\Program Files\\Panorama Scholarly Group\\PSG PDF\\psg-pdf.exe\r\n\r\n";
        assert_eq!(reg_default_value(en), Some(PathBuf::from(r"C:\Program Files\Panorama Scholarly Group\PSG PDF\psg-pdf.exe")));
        let zh = "    (默认)    REG_EXPAND_SZ    \"D:\\Apps\\PSG PDF\\psg-pdf.exe\"\n";
        assert_eq!(reg_default_value(zh), Some(PathBuf::from(r"D:\Apps\PSG PDF\psg-pdf.exe")));
        assert_eq!(reg_default_value("ERROR: The system was unable to find the specified registry key or value.\n"), None);
        assert_eq!(reg_default_value("    (Default)    REG_SZ    \n"), None);
    }
}
