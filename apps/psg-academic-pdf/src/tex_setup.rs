//! Installing and updating PSG's own TeX (`psg_academic::managed`) in the background.
//!
//! The download goes through craft-update's HTTPS client, the same one in-app updates use: the
//! system's certificate store, resuming after a dropped connection, and a SHA-256 check. The
//! checksum is the one GitHub publishes for the release file (its asset `digest`); a file without
//! one is refused, never installed unchecked.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

use craft_update::http::{Fetched, Transfer};
use craft_update::{Http, Stop};
use psg_academic::managed::{self, ManagedTex, Step};

/// What a job is doing now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    Looking,
    Downloading { done: u64, total: u64 },
    Unpacking,
    Packages,
    Updating,
}

/// What a job did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Done {
    /// Installed (or reinstalled) from this TinyTeX release.
    Installed(String),
    Updated,
}

pub enum Event {
    Progress(Progress),
    Finished(Result<Done, String>),
}

/// Which job to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Job {
    Install,
    Update,
}

/// Run `job` on its own thread; events arrive on the receiver, and `wake` is called after each.
pub fn start(tex: ManagedTex, job: Job, wake: impl Fn() + Send + 'static) -> std::io::Result<Receiver<Event>> {
    let (tx, rx) = channel();
    std::thread::Builder::new().name("psg-tex-setup".into()).spawn(move || {
        let send = |tx: &Sender<Event>, e: Event| {
            let _ = tx.send(e);
            wake();
        };
        let mut progress = |p: Progress| send(&tx, Event::Progress(p));
        let result = match job {
            Job::Install => install(&tex, &mut progress).map(Done::Installed),
            Job::Update => update(&tex, &mut progress),
        };
        send(&tx, Event::Finished(result));
    })?;
    Ok(rx)
}

fn http() -> Result<Http, String> {
    Http::new(&format!("PSG Academic PDF/{}", env!("CARGO_PKG_VERSION")))
}

/// Download the newest TinyTeX release for this computer and install it into `tex`. Returns the
/// release's tag.
pub fn install(tex: &ManagedTex, progress: &mut dyn FnMut(Progress)) -> Result<String, String> {
    let name =
        managed::asset_name().ok_or("PSG's TeX isn't available for this kind of computer; install TeX Live with latexmk and Biber instead.")?;
    if !managed::usable_root(&tex.root) {
        return Err(format!(
            "PSG's TeX can't be installed in {} (spaces or non-ASCII characters); set {} to another folder.",
            tex.root.display(),
            managed::DIR_VARIABLE
        ));
    }
    progress(Progress::Looking);
    let http = http()?;
    let url = format!("https://api.github.com/repos/{}/releases/latest", managed::RELEASES_REPO);
    let text = match http.get_api(&url, None)? {
        Fetched::Body { text, .. } => text,
        Fetched::RateLimited { .. } => return Err("GitHub is limiting requests right now; try again in an hour.".into()),
        Fetched::NotFound | Fetched::NotModified => return Err("TinyTeX's release page could not be read.".into()),
    };
    let release = craft_update::parse_github_release(&text, managed::RELEASES_REPO)?;
    let asset = release.assets.iter().find(|a| a.name == name).ok_or_else(|| format!("TinyTeX {} has no {name}", release.tag))?;
    let sha256 = asset.sha256.clone().ok_or_else(|| format!("GitHub publishes no checksum for {name}, so it wasn't downloaded"))?;
    let dest = tex.downloads().join(&name);
    let stop = Stop::default();
    http.download(&asset.url, &dest, asset.size, &sha256, &stop, &mut |t| {
        let (Transfer::Bytes { done, total } | Transfer::Reconnecting { done, total }) = t;
        progress(Progress::Downloading { done, total });
    })?;
    let result = tex.install_from(&dest, &release.tag, &mut |step| {
        progress(match step {
            Step::Unpacking => Progress::Unpacking,
            Step::Packages => Progress::Packages,
        })
    });
    let _ = std::fs::remove_file(&dest);
    result.map_err(|e| e.to_string())?;
    Ok(release.tag)
}

/// Update `tex` with tlmgr, or reinstall it when TeX Live has started a new yearly release.
pub fn update(tex: &ManagedTex, progress: &mut dyn FnMut(Progress)) -> Result<Done, String> {
    progress(Progress::Updating);
    match tex.update().map_err(|e| e.to_string())? {
        true => Ok(Done::Updated),
        false => {
            log::info!("TeX Live has a new yearly release; reinstalling PSG's TeX");
            install(tex, progress).map(Done::Installed)
        }
    }
}

/// The folder for PSG's TeX in this run: inside the portable data folder in portable mode.
pub fn managed_for(data_root: &Path, portable: bool) -> Option<ManagedTex> {
    ManagedTex::default_for(portable.then_some(data_root))
}

/// The managed programs folder, if installed.
pub fn bin_dir(tex: Option<&ManagedTex>) -> Option<PathBuf> {
    tex.and_then(ManagedTex::bin_dir)
}
