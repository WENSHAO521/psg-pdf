//! Release checking, downloading, installing and self-updating for the Craft App Suite and ArtCraft Launcher.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod app;
pub mod desktop_entry;
pub mod detect;
pub mod distro;
mod folders;
pub mod http;
pub mod install;
pub mod manifest;
pub mod msi_context;
mod msi_copy;
mod msi_migrate;
pub mod packages;
pub mod paths;
pub mod pins;
pub mod platform;
#[cfg(windows)]
pub mod portable;
pub mod process;
pub mod protocol;
pub mod proxy;
pub mod queue;
pub mod release;
pub mod self_update;
// PSG patch: releases signed with the developer's own key.
pub mod signature;
pub mod source;
pub mod status;
pub mod suite;
pub mod time;
pub mod translate;
pub mod version;

#[cfg(feature = "egui")]
pub mod dialog;
#[cfg(feature = "egui")]
pub mod updater;

pub use app::App;
pub use http::{Http, Stop};
pub use paths::Paths;
pub use platform::{Arch, Os, PackageKind, Platform, pick_asset};
pub use protocol::{InstallLocations, JobPhase, LatestRelease, Source};
pub use release::{Asset, Release, parse_github_release, parse_github_release_list};
pub use status::{InstallSource, Installed};
pub use suite::SUITE;
pub use time::parse_rfc3339;
pub use version::{Version, is_newer, parse as parse_version};

#[cfg(feature = "egui")]
pub use dialog::{CheckResultDialog, CheckResultResponse, StartupPrompt, StartupPromptResponse};

/// Checks for an update against GitHub releases for the given app.
pub fn check_for_update(app: &App, http: &Http, current_version: &str) -> Result<Option<Release>, String> {
    check_for_update_on(app, http, current_version, false)
}

/// PSG patch: [`check_for_update`] on a channel. With `prereleases`, the newest release of all,
/// betas included, is offered (the beta channel); without, only stable releases.
pub fn check_for_update_on(app: &App, http: &Http, current_version: &str, prereleases: bool) -> Result<Option<Release>, String> {
    let version = self_update::current_version(app, current_version).map(|v| v.to_string());
    let current_version = version.as_deref().unwrap_or(current_version);
    let url = format!("https://api.github.com/repos/{}/releases?per_page=30", app.repo);
    match http.get_api(&url, None)? {
        crate::http::Fetched::Body { text, .. } => offered_release(&text, app.repo, current_version, prereleases),
        crate::http::Fetched::RateLimited { .. } => Err("GitHub's request limit was reached".into()),
        crate::http::Fetched::NotModified | crate::http::Fetched::NotFound => Ok(None),
    }
}

/// Unix seconds now.
pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

/// The release in GitHub's release list `json` to offer a copy running `current_version`, if any:
/// the latest published release, when [`version::offers_update`] says so.
fn offered_release(json: &str, repo: &str, current_version: &str, prereleases: bool) -> Result<Option<Release>, String> {
    let latest = crate::release::parse_github_release_list(json, repo, prereleases)?;
    Ok(latest.filter(|release| {
        let running = crate::version::parse(current_version);
        release.version.as_ref().zip(running.as_ref()).is_some_and(|(rel_v, cur_v)| crate::version::offers_update(rel_v, cur_v))
    }))
}

#[cfg(test)]
pub(crate) const TEST_AGENT: &str = "ArtCraft-Launcher-Test/0.0";

#[cfg(test)]
mod offer_tests {
    use super::offered_release;

    const REPO: &str = "storytold/photocraft";
    const LIST: &str = r#"[{"tag_name":"v0.17.0","prerelease":false,"draft":false,"assets":[]},
        {"tag_name":"v0.16.4","prerelease":false,"draft":false,"assets":[]}]"#;

    fn offered(running: &str) -> Option<String> {
        offered_release(LIST, REPO, running, false).unwrap().map(|release| release.tag)
    }

    #[test]
    fn a_build_from_main_is_not_offered_the_release_it_already_contains() {
        assert_eq!(offered("0.17.0-integration.20261029.f5b3f123"), None);
        assert_eq!(offered("0.16.5-dev"), Some("v0.17.0".into()));
    }

    /// PSG patch: the beta channel is offered betas; the stable channel never is.
    #[test]
    fn only_the_beta_channel_is_offered_betas() {
        const WITH_BETA: &str = r#"[{"tag_name":"v0.20.0-beta.2","prerelease":true,"draft":false,"assets":[]},
            {"tag_name":"v0.19.0","prerelease":false,"draft":false,"assets":[]}]"#;
        let offer = |running: &str, beta: bool| offered_release(WITH_BETA, REPO, running, beta).unwrap().map(|r| r.tag);
        assert_eq!(offer("0.18.0", false), Some("v0.19.0".into()));
        assert_eq!(offer("0.18.0", true), Some("v0.20.0-beta.2".into()));
        assert_eq!(offer("0.19.0", false), None);
        assert_eq!(offer("0.19.0", true), Some("v0.20.0-beta.2".into()));
        // Left the beta channel while on a beta: no older stable release is offered.
        assert_eq!(offer("0.20.0-beta.1", false), None);
        assert_eq!(offer("0.20.0-beta.1", true), Some("v0.20.0-beta.2".into()));
    }

    #[test]
    fn releases_and_release_candidates_are_offered_as_before() {
        assert_eq!(offered("0.17.0-rc.2"), Some("v0.17.0".into()));
        assert_eq!(offered("0.16.4"), Some("v0.17.0".into()));
        assert_eq!(offered("0.17.0"), None);
        assert_eq!(offered("not a version"), None);
    }
}
