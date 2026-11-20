//! PSG's own TeX: a TinyTeX that PSG Academic PDF installs, updates and completes by itself.
//!
//! The person never installs or finds TeX. On first launch the app downloads the TinyTeX-1
//! bundle (a small TeX Live with about a hundred common LaTeX packages) from its publisher's
//! GitHub releases into a per-user folder, adds what the PSG templates need (latexmk, Biber,
//! BibLaTeX, …) with TeX Live's own package manager `tlmgr`, and from then on:
//!
//! - updates it with `tlmgr update --self --all` about once a week and repairs it when a
//!   required tool is missing;
//! - reinstalls it from the newest release when TeX Live's yearly release makes `tlmgr` refuse a
//!   cross-release update;
//! - installs the package that holds a missing `.sty` or `.cls` when a compilation stops for one,
//!   and compiles again.
//!
//! Nothing of TeX is shipped in PSG's installers: the files come from TinyTeX's and CTAN's own
//! servers (docs/academic-tex.md). This module holds the platform layout, the commands and the
//! parsing; the download itself is the app's (it owns the HTTPS client), so this crate stays free
//! of networking.
//!
//! The folder never has spaces or non-ASCII characters on Windows, where TeX Live breaks on them
//! (a user name like `華陽` puts both in `%APPDATA%`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::Error;
use crate::process;

/// Where TinyTeX is published (GitHub `owner/name`).
pub const RELEASES_REPO: &str = "rstudio/tinytex-releases";
/// The bundle installed: TeX Live's infrastructure plus about a hundred common packages.
pub const BUNDLE: &str = "TinyTeX-1";
/// The folder inside [`ManagedTex::root`] holding the TeX tree.
const TREE: &str = "TinyTeX";
/// The state file inside [`ManagedTex::root`].
const STATE: &str = "psg-tex.toml";
/// Set to a folder to keep PSG's TeX there instead (tests, CI, a person who wants it elsewhere).
pub const DIR_VARIABLE: &str = "PSG_ACADEMIC_TEX_DIR";
/// How often the installed TeX is updated.
pub const UPDATE_EVERY: jiff::SignedDuration = jiff::SignedDuration::from_hours(7 * 24);

/// TeX Live packages every PSG template needs beyond TinyTeX-1, installed right after it. Names
/// are TeX Live package names (`tlmgr info <name>`); most are already in the bundle, and `tlmgr
/// install` passes over those.
pub const REQUIRED_PACKAGES: &[&str] = &[
    "latexmk",
    "biber",
    "biblatex",
    "csquotes",
    "soul",
    "microtype",
    "lm",
    "ec",
    "geometry",
    "booktabs",
    "caption",
    "fancyhdr",
    "xcolor",
    "colortbl",
    "hyperref",
    "amsmath",
    "amsfonts",
    "tools",
    "graphics",
    "etoolbox",
    "logreq",
    "url",
    // XeLaTeX with xeCJK, for Chinese and Japanese.
    "xetex",
    "fontspec",
    "xecjk",
    "iftex",
];

/// A Chinese and Japanese font XeLaTeX templates use, downloaded into [`ManagedTex::fonts_dir`]
/// from the Noto CJK project's own repository at a release tag, and checked against its SHA-256.
/// Never bundled or committed (AGENTS.md §1.4); Noto CJK is OFL-1.1 (AGENTS.md §1.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CjkFont {
    /// The file name the templates load it by.
    pub file: &'static str,
    /// Its path in the repository, starting with the release tag.
    pub path: &'static str,
    pub sha256: &'static str,
    pub size: u64,
}

impl CjkFont {
    pub fn url(&self) -> String {
        format!("https://raw.githubusercontent.com/{CJK_FONTS_REPO}/{}", self.path)
    }
}

/// The Noto CJK repository.
pub const CJK_FONTS_REPO: &str = "notofonts/noto-cjk";

/// Noto Serif and Sans CJK, Traditional Chinese first (Simplified covers the characters only it
/// uses), Regular and Bold, and their licence. About 68 MB.
pub const CJK_FONTS: &[CjkFont] = &[
    CjkFont {
        file: "NotoSerifTC-Regular.otf",
        path: "Serif2.003/Serif/SubsetOTF/TC/NotoSerifTC-Regular.otf",
        sha256: "63515eb0622d589a2f4062e8757c598b3da3bffd053f69abd181c210158ef10c",
        size: 7936104,
    },
    CjkFont {
        file: "NotoSerifTC-Bold.otf",
        path: "Serif2.003/Serif/SubsetOTF/TC/NotoSerifTC-Bold.otf",
        sha256: "3ca2b3294ec84b795d0a45695e78e3612a44dce50f9fc776cd40206340a5768d",
        size: 8253304,
    },
    CjkFont {
        file: "NotoSansTC-Regular.otf",
        path: "Sans2.004/Sans/SubsetOTF/TC/NotoSansTC-Regular.otf",
        sha256: "5bab0cb3c1cf89dde07c4a95a4054b195afbcfe784d69d75c340780712237537",
        size: 5683368,
    },
    CjkFont {
        file: "NotoSansTC-Bold.otf",
        path: "Sans2.004/Sans/SubsetOTF/TC/NotoSansTC-Bold.otf",
        sha256: "55420b259eb119bf5f2a0aadba10cf9d736c12d64ab93e78546d69ef5f43558b",
        size: 5839972,
    },
    CjkFont {
        file: "NotoSerifSC-Regular.otf",
        path: "Serif2.003/Serif/SubsetOTF/SC/NotoSerifSC-Regular.otf",
        sha256: "e8f396decc1f0963a016a989c3d8852e863d1350996f573860a80767c83a1cd3",
        size: 11625800,
    },
    CjkFont {
        file: "NotoSerifSC-Bold.otf",
        path: "Serif2.003/Serif/SubsetOTF/SC/NotoSerifSC-Bold.otf",
        sha256: "24693d48bdb9152f0a06b02af625638a1097abd6de4010ebba027f6e82710527",
        size: 12094336,
    },
    CjkFont {
        file: "NotoSansSC-Regular.otf",
        path: "Sans2.004/Sans/SubsetOTF/SC/NotoSansSC-Regular.otf",
        sha256: "faa6c9df652116dde789d351359f3d7e5d2285a2b2a1f04a2d7244df706d5ea9",
        size: 8331336,
    },
    CjkFont {
        file: "NotoSansSC-Bold.otf",
        path: "Sans2.004/Sans/SubsetOTF/SC/NotoSansSC-Bold.otf",
        sha256: "c6cb5a93abaa9edc8ee7463b7ebb7f42d618d40e6ed2f7a5371c97b0b64767c0",
        size: 8543168,
    },
    CjkFont { file: "OFL.txt", path: "Sans2.004/LICENSE", sha256: "6a73f9541c2de74158c0e7cf6b0a58ef774f5a780bf191f2d7ec9cc53efe2bf2", size: 4301 },
];

/// The longest a single `tlmgr` run may take (a full update of an old tree can be slow).
const TLMGR_TIMEOUT: Duration = Duration::from_secs(60 * 60);
/// The longest unpacking the bundle may take.
const EXTRACT_TIMEOUT: Duration = Duration::from_secs(20 * 60);
/// The most packages installed for one compilation, so a log full of missing files can't start
/// an unbounded download.
pub const MAX_AUTO_PACKAGES: usize = 12;

/// The bundle's release asset for this computer, or `None` where TinyTeX publishes none.
pub fn asset_name() -> Option<String> {
    asset_name_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// [`asset_name`] for `os` and `arch` (`std::env::consts` values).
pub fn asset_name_for(os: &str, arch: &str) -> Option<String> {
    let suffix = match (os, arch) {
        // TeX Live's Windows binaries are x86-64; Windows on Arm runs them emulated.
        ("windows", "x86_64" | "aarch64") => "windows.exe",
        ("macos", "x86_64" | "aarch64") => "darwin.tar.xz",
        ("linux", "x86_64") => "linux-x86_64.tar.xz",
        ("linux", "aarch64") => "linux-arm64.tar.xz",
        _ => return None,
    };
    Some(format!("{BUNDLE}-{suffix}"))
}

/// The platform asset name used by TinyTeX releases that include the tag in each filename.
pub fn tagged_asset_name(name: &str, tag: &str) -> Option<String> {
    if tag.is_empty() || tag.len() > 64 || !tag.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_')) {
        return None;
    }
    let (stem, extension) = name.split_once('.')?;
    Some(format!("{stem}-{tag}.{extension}"))
}

/// `path` can hold a TeX Live tree on this platform: on Windows only ASCII without spaces.
pub fn usable_root(path: &Path) -> bool {
    let text = path.to_string_lossy();
    !cfg!(windows) || text.chars().all(|c| c.is_ascii() && c != ' ')
}

/// The per-user folder for PSG's TeX, or `None` when no home folder can be found. `portable` is
/// the portable data folder, when the app runs portable: TeX then travels with it.
pub fn default_root(portable: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(DIR_VARIABLE).filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    if let Some(portable) = portable {
        let root = portable.join("tex");
        if usable_root(&root) {
            return Some(root);
        }
    }
    platform_root()
}

#[cfg(windows)]
fn platform_root() -> Option<PathBuf> {
    let per_user = std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("PSGAcademicTeX"));
    if let Some(root) = per_user.filter(|r| usable_root(r)) {
        return Some(root);
    }
    // What TinyTeX's own installer falls back to for such user names.
    std::env::var_os("ProgramData").map(|d| PathBuf::from(d).join("PSGAcademicTeX")).filter(|r| usable_root(r))
}

#[cfg(target_os = "macos")]
fn platform_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("PSGAcademicTeX"))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn platform_root() -> Option<PathBuf> {
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|d| d.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")))?;
    Some(data.join("psg-academic-tex"))
}

/// What PSG's TeX remembers about itself, in `psg-tex.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct State {
    /// The TinyTeX release installed (`v2026.11`).
    #[serde(default)]
    pub release: String,
    /// When it was installed (RFC 3339).
    #[serde(default)]
    pub installed_at: String,
    /// When `tlmgr update` last succeeded (RFC 3339).
    #[serde(default)]
    pub updated_at: String,
    /// Packages added for documents (beyond [`REQUIRED_PACKAGES`]), put back after a reinstall.
    #[serde(default)]
    pub added_packages: Vec<String>,
}

/// PSG's own TeX installation in `root`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedTex {
    pub root: PathBuf,
}

impl ManagedTex {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The installation in the default folder ([`default_root`]).
    pub fn default_for(portable: Option<&Path>) -> Option<Self> {
        default_root(portable).map(Self::new)
    }

    pub fn tree(&self) -> PathBuf {
        self.root.join(TREE)
    }

    /// Where downloads are kept until they are unpacked.
    pub fn downloads(&self) -> PathBuf {
        self.root.join("downloads")
    }

    /// Where the CJK fonts ([`CJK_FONTS`]) are kept. XeLaTeX finds them there by file name.
    pub fn fonts_dir(&self) -> PathBuf {
        self.root.join("fonts")
    }

    /// The CJK fonts not yet downloaded (or of the wrong size: a cut-off download).
    pub fn missing_fonts(&self) -> Vec<CjkFont> {
        let dir = self.fonts_dir();
        CJK_FONTS.iter().filter(|f| std::fs::metadata(dir.join(f.file)).map_or(true, |m| m.len() != f.size)).copied().collect()
    }

    /// The folder of TeX's programs (`TinyTeX/bin/<platform>`), when the tree is installed.
    pub fn bin_dir(&self) -> Option<PathBuf> {
        bin_dir_in(&self.tree())
    }

    pub fn installed(&self) -> bool {
        self.bin_dir().is_some()
    }

    pub fn state(&self) -> State {
        std::fs::read_to_string(self.root.join(STATE)).ok().and_then(|text| toml::from_str(&text).ok()).unwrap_or_default()
    }

    pub fn save_state(&self, state: &State) -> Result<(), Error> {
        let text = toml::to_string(state).map_err(|e| Error::Project(format!("could not write {STATE}: {e}")))?;
        std::fs::create_dir_all(&self.root).map_err(|e| Error::io(&self.root, e))?;
        crate::paths::write_atomic(&self.root.join(STATE), text.as_bytes())
    }

    /// Maintain an installed tree when a required tool is missing or the weekly update is due.
    pub fn update_due(&self, now: jiff::Timestamp) -> bool {
        let Some(bin) = self.bin_dir() else { return false };
        if missing_core_tools(&bin).next().is_some() {
            return true;
        }
        let state = self.state();
        let last = if state.updated_at.is_empty() { &state.installed_at } else { &state.updated_at };
        match last.parse::<jiff::Timestamp>() {
            Ok(at) => now.duration_since(at) >= UPDATE_EVERY,
            Err(_) => true,
        }
    }

    /// A `tlmgr` command for this tree, with the user's TeX search variables removed so only
    /// this tree is changed.
    pub fn tlmgr(&self) -> Result<Command, Error> {
        let bin = self.bin_dir().ok_or_else(|| Error::TexMissing("PSG TeX".into()))?;
        let program = if cfg!(windows) { bin.join("tlmgr.bat") } else { bin.join("tlmgr") };
        let mut cmd = Command::new(program);
        for var in ["TEXMFHOME", "TEXMFCNF", "TEXMFVAR", "TEXMFCONFIG", "TEXMFLOCAL", "TEXMFAUXTREES", "PERL5OPT", "PERL5LIB", "PERLLIB"] {
            cmd.env_remove(var);
        }
        // tlmgr runs its helpers (mktexlsr, fmtutil) by name: this tree's first.
        let mut dirs = vec![bin];
        if let Some(path) = std::env::var_os("PATH") {
            dirs.extend(std::env::split_paths(&path).filter(|d| d.is_absolute()));
        }
        if let Ok(joined) = std::env::join_paths(dirs) {
            cmd.env("PATH", joined);
        }
        Ok(cmd)
    }

    /// Run `tlmgr` with `args`; the output, or an error with what it printed last.
    pub fn run_tlmgr(&self, args: &[&str]) -> Result<String, Error> {
        let mut cmd = self.tlmgr()?;
        cmd.args(args);
        log::info!("tlmgr {}", args.join(" "));
        let out = process::run(cmd, TLMGR_TIMEOUT, None).map_err(|e| Error::Compile(format!("could not run tlmgr: {e}")))?;
        let text = format!("{}{}", out.stdout, out.stderr);
        if out.success() {
            Ok(text)
        } else if out.timed_out {
            Err(Error::Compile("tlmgr took too long and was stopped".into()))
        } else {
            Err(Error::Compile(format!("tlmgr {} failed: {}", args.first().unwrap_or(&""), last_lines(&text, 6))))
        }
    }

    /// Install TeX Live `packages` (names checked first).
    pub fn install_packages(&self, packages: &[String]) -> Result<String, Error> {
        let names: Vec<&str> = packages.iter().map(String::as_str).filter(|p| is_package_name(p)).collect();
        if names.is_empty() {
            return Ok(String::new());
        }
        let mut args = vec!["install"];
        args.extend(names);
        self.run_tlmgr(&args)
    }

    /// Update the package manager and installed packages, then restore missing core tools.
    /// `Ok(false)` when TeX Live has moved to a new yearly release and needs a new bundle.
    pub fn update(&self) -> Result<bool, Error> {
        let result = self.run_tlmgr(&["update", "--self", "--all"]).and_then(|_| self.repair_core_tools());
        match result {
            Ok(_) => {
                let mut state = self.state();
                state.updated_at = now_utc().to_string();
                self.save_state(&state)?;
                Ok(true)
            }
            Err(Error::Compile(text)) if needs_reinstall(&text) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Restore the bibliography tools from TeX Live in this managed installation.
    pub fn reinstall_biber(&self) -> Result<(), Error> {
        let bin = self.bin_dir().ok_or_else(|| Error::TexMissing("PSG TeX".into()))?;
        self.run_tlmgr(&["install", "--reinstall", "biber", "biblatex"])?;
        if missing_core_tools(&bin).any(|tool| tool == "biber") {
            return Err(Error::TexMissing("biber".into()));
        }
        Ok(())
    }

    fn repair_core_tools(&self) -> Result<(), Error> {
        let bin = self.bin_dir().ok_or_else(|| Error::TexMissing("PSG TeX".into()))?;
        let missing: Vec<_> = missing_core_tools(&bin).collect();
        if missing.is_empty() {
            return Ok(());
        }
        // The local package registry may still list a package whose launcher was deleted.
        let mut args = vec!["install", "--reinstall"];
        args.extend(missing.iter().copied());
        if missing.contains(&"biber") {
            args.push("biblatex");
        }
        self.run_tlmgr(&args)?;
        if let Some(tool) = missing_core_tools(&bin).next() {
            return Err(Error::TexMissing(tool.into()));
        }
        Ok(())
    }

    /// The TeX Live packages holding `files` (`soul.sty`), found with `tlmgr search --global`.
    pub fn packages_for(&self, files: &[String]) -> Vec<String> {
        let mut found = Vec::new();
        for file in files.iter().take(MAX_AUTO_PACKAGES) {
            let pattern = format!("/{file}");
            if let Ok(out) = self.run_tlmgr(&["search", "--global", "--file", &pattern]) {
                for name in parse_search(&out, file) {
                    if !found.contains(&name) {
                        found.push(name);
                    }
                }
            }
        }
        found.truncate(MAX_AUTO_PACKAGES);
        found
    }

    /// Install what a compilation log says is missing. Returns the packages installed (empty
    /// when nothing missing was found, or nothing provides it).
    pub fn install_missing(&self, tex_log: &str) -> Result<Vec<String>, Error> {
        let files = missing_files(tex_log);
        if files.is_empty() {
            return Ok(Vec::new());
        }
        let packages = self.packages_for(&files);
        if packages.is_empty() {
            return Ok(Vec::new());
        }
        self.install_packages(&packages)?;
        let mut state = self.state();
        for p in &packages {
            if !state.added_packages.contains(p) && !REQUIRED_PACKAGES.contains(&p.as_str()) {
                state.added_packages.push(p.clone());
            }
        }
        self.save_state(&state)?;
        Ok(packages)
    }

    /// The command that unpacks the downloaded bundle `archive` into the empty folder `dest`.
    pub fn extract_command(archive: &Path, dest: &Path) -> Command {
        if archive.extension().is_some_and(|e| e.eq_ignore_ascii_case("exe")) {
            // A 7-Zip self-extracting archive: -y answers every prompt, -o names the folder.
            let mut cmd = Command::new(archive);
            cmd.arg("-y").arg(format!("-o{}", dest.display()));
            cmd
        } else {
            let mut cmd = Command::new("tar");
            cmd.arg("-xJf").arg(archive).arg("-C").arg(dest);
            cmd
        }
    }

    /// Unpack `archive` and make it the installed tree, replacing any older one only once the
    /// new one is complete. Then add [`REQUIRED_PACKAGES`] and the packages added before, and
    /// remember `release`.
    pub fn install_from(&self, archive: &Path, release: &str, progress: &mut dyn FnMut(Step)) -> Result<(), Error> {
        if !usable_root(&self.root) {
            return Err(Error::Project(format!("{} has spaces or non-ASCII characters, which TeX can't be installed in", self.root.display())));
        }
        progress(Step::Unpacking);
        let staging = self.root.join(format!("staging-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).map_err(|e| Error::io(&staging, e))?;
        let result = (|| {
            let out = process::run(Self::extract_command(archive, &staging), EXTRACT_TIMEOUT, None)
                .map_err(|e| Error::Compile(format!("could not unpack {}: {e}", archive.display())))?;
            if !out.success() {
                return Err(Error::Compile(format!(
                    "unpacking {} failed: {}",
                    archive.display(),
                    last_lines(&format!("{}{}", out.stdout, out.stderr), 4)
                )));
            }
            let unpacked = unpacked_tree(&staging).ok_or_else(|| Error::Compile(format!("{} holds no TeX installation", archive.display())))?;
            self.replace_tree(&unpacked)
        })();
        let _ = std::fs::remove_dir_all(&staging);
        result?;
        let mut state = self.state();
        state.release = release.to_owned();
        state.installed_at = now_utc().to_string();
        state.updated_at.clear();
        self.save_state(&state)?;
        progress(Step::Packages);
        // The package list TinyTeX ships points at a fixed CTAN mirror; let tlmgr pick a near one.
        let _ = self.run_tlmgr(&["option", "repository", "ctan"]);
        let mut packages: Vec<String> = REQUIRED_PACKAGES.iter().map(|p| (*p).to_owned()).collect();
        packages.extend(state.added_packages.iter().cloned());
        // One name the repository has dropped fails the whole command, though the rest are
        // installed: what matters is that the programs a compilation runs are there.
        let install = self.install_packages(&packages);
        let bin = self.bin_dir().ok_or_else(|| Error::TexMissing("PSG TeX".into()))?;
        if missing_core_tools(&bin).next().is_some() {
            install?;
            self.repair_core_tools()?;
        } else if let Err(e) = install {
            log::warn!("installing the template packages: {e}");
        }
        let mut state = self.state();
        state.updated_at = now_utc().to_string();
        self.save_state(&state)?;
        Ok(())
    }

    /// Move `unpacked` into place as the tree: the old tree is moved aside first and removed only
    /// after the new one is in place, so a failure leaves the old one working.
    fn replace_tree(&self, unpacked: &Path) -> Result<(), Error> {
        let tree = self.tree();
        let old = self.root.join(format!("{TREE}.old"));
        let _ = std::fs::remove_dir_all(&old);
        let had_old = tree.exists();
        if had_old {
            std::fs::rename(&tree, &old).map_err(|e| Error::io(&tree, e))?;
        }
        if let Err(e) = std::fs::rename(unpacked, &tree) {
            if had_old {
                let _ = std::fs::rename(&old, &tree);
            }
            return Err(Error::io(&tree, e));
        }
        let _ = std::fs::remove_dir_all(&old);
        Ok(())
    }

    /// Remove PSG's TeX entirely (Settings ▸ Remove).
    pub fn remove(&self) -> Result<(), Error> {
        if self.root.exists() {
            std::fs::remove_dir_all(&self.root).map_err(|e| Error::io(&self.root, e))?;
        }
        Ok(())
    }
}

/// Compile `project`; when PSG's TeX compiled it and the log names missing `.sty`/`.cls` files,
/// install the packages holding them and compile once more. Returns the report and the packages
/// installed (empty when none were needed).
pub fn compile_completing(
    project: &crate::Project,
    env: &crate::TexEnvironment,
    tex: Option<&ManagedTex>,
    cancel: Option<&std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<(crate::CompileReport, Vec<String>), Error> {
    let report = project.compile(env, cancel)?;
    let (Some(tex), true, crate::Outcome::Failed) = (tex, env.managed, report.outcome) else { return Ok((report, Vec::new())) };
    let Some(log) = report.tex_log.as_deref() else { return Ok((report, Vec::new())) };
    let installed = match tex.install_missing(log) {
        Ok(installed) => installed,
        Err(e) => {
            log::warn!("installing missing packages: {e}");
            Vec::new()
        }
    };
    if installed.is_empty() {
        return Ok((report, installed));
    }
    let again = project.compile(env, cancel)?;
    Ok((again, installed))
}

/// What an installation is doing, for a progress line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Unpacking,
    Packages,
}

/// The `bin/<platform>` folder of a TeX Live tree that holds pdflatex.
fn bin_dir_in(tree: &Path) -> Option<PathBuf> {
    let pdflatex = if cfg!(windows) { "pdflatex.exe" } else { "pdflatex" };
    let mut dirs: Vec<PathBuf> =
        std::fs::read_dir(tree.join("bin")).ok()?.flatten().map(|e| e.path()).filter(|p| p.join(pdflatex).is_file()).collect();
    dirs.sort();
    dirs.into_iter().next()
}

fn missing_core_tools(bin: &Path) -> impl Iterator<Item = &'static str> + '_ {
    ["latexmk", "biber"].into_iter().filter(|name| {
        let file = if cfg!(windows) { bin.join(format!("{name}.exe")) } else { bin.join(name) };
        !file.is_file()
    })
}

/// The TeX tree an unpacked bundle holds: the folder (one level down, `TinyTeX` or `.TinyTeX`)
/// with a `bin` folder in it.
fn unpacked_tree(staging: &Path) -> Option<PathBuf> {
    if staging.join("bin").is_dir() && staging.join("texmf-dist").is_dir() {
        return Some(staging.to_path_buf());
    }
    let mut found: Vec<PathBuf> =
        std::fs::read_dir(staging).ok()?.flatten().map(|e| e.path()).filter(|p| p.join("bin").is_dir() && p.join("texmf-dist").is_dir()).collect();
    found.sort();
    found.into_iter().next()
}

/// `tlmgr` refused because TeX Live has a newer yearly release than the installed tree.
pub fn needs_reinstall(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    lower.contains("older than remote repository") || lower.contains("cross release updates are only supported")
}

/// A TeX Live package name: lowercase ASCII letters, digits, `-`, `_` and `.`; never an option.
pub fn is_package_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && !name.starts_with(['-', '.'])
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}

/// A file name a log says is missing and a package could provide: `name.ext` with a TeX
/// extension, no folders.
fn is_missing_file_name(name: &str) -> bool {
    const EXTENSIONS: &[&str] = &["sty", "cls", "def", "cfg", "fd", "clo", "bbx", "cbx", "lbx", "tex", "bst", "ldf"];
    let Some((stem, ext)) = name.rsplit_once('.') else { return false };
    !stem.is_empty()
        && name.len() <= 100
        && EXTENSIONS.contains(&ext)
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The files a TeX log says it couldn't find (`! LaTeX Error: File `soul.sty' not found.`),
/// deduplicated, in order.
pub fn missing_files(log: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in log.lines() {
        let Some(start) = line.find("File `") else { continue };
        if !line.contains("not found") {
            continue;
        }
        let rest = line.get(start + "File `".len()..).unwrap_or_default();
        let Some(end) = rest.find('\'') else { continue };
        let name = rest.get(..end).unwrap_or_default();
        if is_missing_file_name(name) && !files.iter().any(|f| f == name) {
            files.push(name.to_owned());
        }
    }
    files
}

/// The packages `tlmgr search --global --file /<file>` lists as holding exactly `file`.
///
/// ```text
/// tlmgr: package repository https://… (verified)
/// soul:
///         texmf-dist/tex/generic/soul/soul.sty
/// ```
pub fn parse_search(output: &str, file: &str) -> Vec<String> {
    let suffix = format!("/{file}");
    let mut packages = Vec::new();
    let mut current: Option<&str> = None;
    for line in output.lines() {
        if line.starts_with(char::is_whitespace) {
            if let Some(name) = current
                && line.trim().ends_with(&suffix)
                && !packages.iter().any(|p| p == name)
            {
                packages.push(name.to_owned());
            }
        } else {
            current = line.strip_suffix(':').filter(|n| is_package_name(n) && !n.contains('.'));
        }
    }
    packages
}

/// The current UTC timestamp. An out-of-range system clock falls back to the Unix epoch.
pub fn now_utc() -> jiff::Timestamp {
    jiff::Timestamp::try_from(std::time::SystemTime::now()).unwrap_or(jiff::Timestamp::UNIX_EPOCH)
}

/// The last `n` non-empty lines of `text`, joined, for an error message.
fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    lines.get(lines.len().saturating_sub(n)..).unwrap_or_default().join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_fonts_are_pinned_and_are_the_ones_the_templates_load() {
        let mut names = std::collections::HashSet::new();
        for f in CJK_FONTS {
            assert!(names.insert(f.file), "{} twice", f.file);
            assert!(f.sha256.len() == 64 && f.sha256.bytes().all(|b| b.is_ascii_hexdigit()), "{}", f.file);
            assert!(f.path.starts_with("Serif2.") || f.path.starts_with("Sans2."), "pinned to a release tag: {}", f.path);
            assert!(f.url().starts_with("https://raw.githubusercontent.com/notofonts/noto-cjk/"));
            assert!(crate::paths::safe_relative_name(f.file).is_some());
        }
        let class = include_str!("../../../templates/academic/psg-article/psg-article.cls");
        for f in CJK_FONTS.iter().filter(|f| f.file.ends_with(".otf")) {
            assert!(class.contains(f.file), "the class doesn't load {}", f.file);
        }
    }

    #[test]
    fn fonts_of_the_wrong_size_are_missing() {
        let dir = tempfile::tempdir().unwrap();
        let tex = ManagedTex::new(dir.path().to_path_buf());
        assert_eq!(tex.missing_fonts().len(), CJK_FONTS.len());
        std::fs::create_dir_all(tex.fonts_dir()).unwrap();
        std::fs::write(tex.fonts_dir().join("OFL.txt"), vec![b'x'; 4301]).unwrap();
        std::fs::write(tex.fonts_dir().join("NotoSerifTC-Regular.otf"), b"cut off").unwrap();
        let missing = tex.missing_fonts();
        assert_eq!(missing.len(), CJK_FONTS.len() - 1);
        assert!(missing.iter().any(|f| f.file == "NotoSerifTC-Regular.otf"));
    }

    #[test]
    fn every_supported_platform_has_its_bundle() {
        assert_eq!(asset_name_for("windows", "x86_64").as_deref(), Some("TinyTeX-1-windows.exe"));
        assert_eq!(asset_name_for("macos", "aarch64").as_deref(), Some("TinyTeX-1-darwin.tar.xz"));
        assert_eq!(asset_name_for("linux", "x86_64").as_deref(), Some("TinyTeX-1-linux-x86_64.tar.xz"));
        assert_eq!(asset_name_for("linux", "aarch64").as_deref(), Some("TinyTeX-1-linux-arm64.tar.xz"));
        assert_eq!(asset_name_for("freebsd", "x86_64"), None);
    }

    #[test]
    fn missing_files_come_from_the_log() {
        let log = "(./main.tex\n! LaTeX Error: File `soul.sty' not found.\n\nType X to quit\n\
                   ! LaTeX Error: File `figure1.png' not found.\n\
                   ./main.tex:3: LaTeX Error: File `soul.sty' not found.\n\
                   ! LaTeX Error: File `../../etc/x.sty' not found.\n\
                   ! LaTeX Error: File `acmart.cls' not found.\n";
        assert_eq!(missing_files(log), vec!["soul.sty".to_owned(), "acmart.cls".to_owned()]);
        assert!(missing_files("no problems here").is_empty());
    }

    #[test]
    fn search_output_names_the_packages_holding_the_file() {
        let out = "tlmgr: package repository https://mirror.example/tlnet (verified)\n\
                   soul:\n\ttexmf-dist/tex/generic/soul/soul.sty\n\
                   mysoul:\n\ttexmf-dist/tex/latex/mysoul/mysoul.sty\n\
                   soul.x86_64-linux:\n\tbin/x86_64-linux/soul.sty\n\
                   --evil:\n\ttexmf-dist/tex/latex/evil/soul.sty\n";
        assert_eq!(parse_search(out, "soul.sty"), vec!["soul".to_owned()]);
        assert!(parse_search("tlmgr: no matches\n", "soul.sty").is_empty());
    }

    #[test]
    fn package_names_are_checked() {
        for ok in ["soul", "biblatex", "cm-super", "l3kernel"] {
            assert!(is_package_name(ok), "{ok}");
        }
        for bad in ["", "-all", "--self", "a b", "Soul", "x;rm", ".hidden"] {
            assert!(!is_package_name(bad), "{bad}");
        }
    }

    #[test]
    fn cross_release_refusals_ask_for_a_reinstall() {
        assert!(needs_reinstall("tlmgr: Local TeX Live (2025) is older than remote repository (2026)."));
        assert!(needs_reinstall("Cross release updates are only supported with\n  update-tlmgr-latest(.sh/.exe) --update"));
        assert!(!needs_reinstall("tlmgr: package repository unreachable"));
    }

    #[test]
    fn a_tree_is_found_by_its_pdflatex() {
        let dir = tempfile::tempdir().unwrap();
        let tex = ManagedTex::new(dir.path().to_path_buf());
        assert!(!tex.installed());
        let bin = tex.tree().join("bin").join("x86_64-linux");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join(if cfg!(windows) { "pdflatex.exe" } else { "pdflatex" }), "").unwrap();
        assert_eq!(tex.bin_dir(), Some(bin));
        assert!(tex.installed());
    }

    #[test]
    fn the_unpacked_tree_may_be_hidden_or_not() {
        for name in ["TinyTeX", ".TinyTeX"] {
            let dir = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(dir.path().join(name).join("bin")).unwrap();
            std::fs::create_dir_all(dir.path().join(name).join("texmf-dist")).unwrap();
            assert_eq!(unpacked_tree(dir.path()), Some(dir.path().join(name)));
        }
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(unpacked_tree(empty.path()), None);
    }

    #[test]
    fn replacing_the_tree_keeps_only_the_new_one() {
        let dir = tempfile::tempdir().unwrap();
        let tex = ManagedTex::new(dir.path().to_path_buf());
        std::fs::create_dir_all(tex.tree()).unwrap();
        std::fs::write(tex.tree().join("old.txt"), "").unwrap();
        let new = dir.path().join("staging").join("TinyTeX");
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join("new.txt"), "").unwrap();
        tex.replace_tree(&new).unwrap();
        assert!(tex.tree().join("new.txt").is_file());
        assert!(!tex.tree().join("old.txt").exists());
        assert!(!dir.path().join("TinyTeX.old").exists());
    }

    #[test]
    fn updates_are_due_weekly() {
        let dir = tempfile::tempdir().unwrap();
        let tex = ManagedTex::new(dir.path().to_path_buf());
        let now = now_utc();
        assert!(!tex.update_due(now), "nothing installed, nothing to update");
        let bin = tex.tree().join("bin").join("x");
        std::fs::create_dir_all(&bin).unwrap();
        for name in ["pdflatex", "latexmk", "biber"] {
            let executable = if cfg!(windows) { bin.join(format!("{name}.exe")) } else { bin.join(name) };
            std::fs::write(executable, "").unwrap();
        }
        // Read the +00:00 offset written by older versions.
        let installed_at = (now - jiff::SignedDuration::from_hours(24)).strftime("%Y-%m-%dT%H:%M:%S%.9f+00:00").to_string();
        let state = State { installed_at, ..State::default() };
        tex.save_state(&state).unwrap();
        assert!(!tex.update_due(now));
        assert!(tex.update_due(now + UPDATE_EVERY));
        assert_eq!(tex.state(), state);
    }

    #[test]
    fn unix_bundles_are_unpacked_with_tar() {
        let cmd = ManagedTex::extract_command(Path::new("/d/TinyTeX-1-linux-x86_64.tar.xz"), Path::new("/d/staging"));
        assert_eq!(cmd.get_program(), "tar");
        let cmd = ManagedTex::extract_command(Path::new("/d/TinyTeX-1-windows.exe"), Path::new("/d/s"));
        let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
        assert_eq!(args, vec!["-y".to_owned(), "-o/d/s".to_owned()]);
    }

    /// A fake TinyTeX archive: a `.TinyTeX` tree whose tlmgr records its arguments.
    #[cfg(unix)]
    fn fake_bundle(dir: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let tree = dir.join("src").join(".TinyTeX");
        let bin = tree.join("bin").join("x86_64-linux");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(tree.join("texmf-dist")).unwrap();
        for (name, body) in [
            ("pdflatex", "#!/bin/sh\necho pdfTeX fake\n"),
            ("latexmk", "#!/bin/sh\necho latexmk fake\n"),
            ("biber", "#!/bin/sh\necho biber fake\n"),
            ("tlmgr", "#!/bin/sh\necho \"$@\" >> \"$(dirname \"$0\")/tlmgr.log\"\n"),
        ] {
            let p = bin.join(name);
            std::fs::write(&p, body).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let archive = dir.join("TinyTeX-1-linux-x86_64.tar.xz");
        let ok = Command::new("tar").arg("-cJf").arg(&archive).arg("-C").arg(dir.join("src")).arg(".TinyTeX").status().unwrap();
        assert!(ok.success(), "tar with xz is needed for this test");
        archive
    }

    #[cfg(unix)]
    #[test]
    fn a_bundle_is_unpacked_installed_and_completed() {
        let dir = tempfile::tempdir().unwrap();
        let archive = fake_bundle(dir.path());
        let tex = ManagedTex::new(dir.path().join("root"));
        let mut steps = Vec::new();
        tex.install_from(&archive, "v2026.11", &mut |s| steps.push(s)).unwrap();
        assert_eq!(steps, vec![Step::Unpacking, Step::Packages]);
        let bin = tex.bin_dir().unwrap();
        let log = std::fs::read_to_string(bin.join("tlmgr.log")).unwrap();
        assert!(log.contains("option repository ctan"), "{log}");
        assert!(log.lines().any(|l| l.starts_with("install latexmk biber biblatex")), "{log}");
        let state = tex.state();
        assert_eq!(state.release, "v2026.11");
        assert!(!tex.update_due(now_utc()));
        // Nothing is left behind but the tree and the state.
        let mut names: Vec<String> = std::fs::read_dir(&tex.root).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, vec![TREE.to_owned(), STATE.to_owned()]);
        // The environment finds PSG's TeX first.
        let env = crate::TexEnvironment::detect_with(Some(&bin));
        assert!(env.managed && env.missing(true).is_empty());
        // A reinstall replaces the tree and keeps the added packages.
        let mut state = tex.state();
        state.added_packages.push("soul".into());
        tex.save_state(&state).unwrap();
        tex.install_from(&archive, "v2026.12", &mut |_| {}).unwrap();
        let log = std::fs::read_to_string(tex.bin_dir().unwrap().join("tlmgr.log")).unwrap();
        assert!(log.lines().any(|l| l.starts_with("install ") && l.ends_with(" soul")), "{log}");
        assert_eq!(tex.state().release, "v2026.12");
    }

    #[cfg(unix)]
    #[test]
    fn maintenance_reinstalls_biber_even_when_tex_was_recently_updated() {
        let dir = tempfile::tempdir().unwrap();
        let archive = fake_bundle(dir.path());
        let tex = ManagedTex::new(dir.path().join("root"));
        tex.install_from(&archive, "v2026.11", &mut |_| {}).unwrap();
        let bin = tex.bin_dir().unwrap();
        let state = tex.state();
        std::fs::remove_file(bin.join("biber")).unwrap();
        assert!(tex.update_due(now_utc()));

        // A successful tlmgr exit alone cannot mark an incomplete installation as healthy.
        assert!(matches!(tex.update(), Err(Error::TexMissing(tool)) if tool == "biber"));
        assert_eq!(tex.state(), state);

        let tlmgr = r#"#!/bin/sh
bin="$(dirname "$0")"
echo "$@" >> "$bin/tlmgr.log"
if [ "$*" = "install --reinstall biber biblatex" ]; then
    printf '#!/bin/sh\necho Biber fake\n' > "$bin/biber"
    chmod +x "$bin/biber"
fi
"#;
        std::fs::write(bin.join("tlmgr"), tlmgr).unwrap();
        assert!(tex.update().unwrap());
        assert!(!tex.update_due(now_utc()));
        let env = crate::TexEnvironment::detect_with(Some(&bin));
        assert_eq!(env.biber.path.as_deref(), Some(bin.join("biber").as_path()));
        assert!(env.biber.found());

        // The executable can be present but fail to start. Targeted repair replaces it without
        // touching the weekly-update timestamp and the next probe verifies it can run.
        let state = tex.state();
        std::fs::write(bin.join("biber"), "#!/bin/sh\necho Biber startup failed >&2\nexit 1\n").unwrap();
        let env = crate::TexEnvironment::detect_with(Some(&bin));
        assert!(!env.biber.found());
        assert_eq!(env.biber.probe_error.as_deref(), Some("Biber startup failed"));
        tex.reinstall_biber().unwrap();
        assert!(crate::TexEnvironment::detect_with(Some(&bin)).biber.found());
        assert_eq!(tex.state(), state);
    }

    #[cfg(unix)]
    #[test]
    fn a_broken_archive_leaves_the_old_tree_working() {
        let dir = tempfile::tempdir().unwrap();
        let archive = fake_bundle(dir.path());
        let tex = ManagedTex::new(dir.path().join("root"));
        tex.install_from(&archive, "v1", &mut |_| {}).unwrap();
        let broken = dir.path().join("broken.tar.xz");
        std::fs::write(&broken, "not an archive").unwrap();
        assert!(tex.install_from(&broken, "v2", &mut |_| {}).is_err());
        assert!(tex.installed());
        assert_eq!(tex.state().release, "v1");
    }

    #[cfg(windows)]
    #[test]
    fn windows_roots_avoid_spaces_and_non_ascii() {
        assert!(usable_root(Path::new(r"C:\Users\hy\AppData\Local\PSGAcademicTeX")));
        assert!(!usable_root(Path::new(r"C:\Users\華陽\AppData\Local\PSGAcademicTeX")));
        assert!(!usable_root(Path::new(r"C:\Users\Hua Yang\AppData\Local\PSGAcademicTeX")));
    }
}
