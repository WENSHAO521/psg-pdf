//! Check for updates, through `craft_update`, on PSG Academic PDF's own update channel.
//!
//! The two PSG products publish into the same GitHub releases. craft-update only picks assets
//! named `<id>-<version>-<os>-<arch>.<ext>`, so with the id `psg-academic-pdf` this app is only
//! ever offered `psg-academic-pdf-…` installers, never PSG PDF's (packaging/products.toml,
//! `asset_stem`). There is no check at start: the user asks for one.

use craft_update::app::App;
use craft_update::updater::{Action, Status, Updater};
use craft_update::{CheckResultDialog, CheckResultResponse};

use std::path::PathBuf;

use crate::app::{APP_NAME, PUBLISHER};
use crate::i18n::t;

/// PSG Academic PDF as its releases and installers know it.
pub const UPDATE_APP: App = App {
    icon_png: Some(include_bytes!("../../../assets/app-icon-academic/hicolor/128x128/apps/com.panoramascholarly.psgacademicpdf.png")),
    // packaging/products.toml `accent`.
    accent: Some(crate::theme::PRODUCT.brand()),
    developer: PUBLISHER,
    // The MSI's Manufacturer (packaging/windows/*.wxs): the updater finds its own uninstall
    // record by it, so an MSI copy can update itself.
    publisher: PUBLISHER,
    // packaging/products.toml `app_id`.
    app_id: Some("com.panoramascholarly.psgacademicpdf"),
    // Releases are signed with PSG's own key (docs/update-signing.md).
    update_key: Some(include_str!("../../../packaging/update-signing-key.txt")),
    ..App::new("psg-academic-pdf", APP_NAME, crate::theme::PRODUCT.tagline(), "WENSHAO521/psg-pdf")
};

pub struct Updates {
    updater: Updater,
    /// The result dialog is showing.
    open: bool,
    check_requested: bool,
    /// The beta channel: updates offer the newest release, betas included.
    beta: bool,
    /// A channel change waiting for its confirmation (`Some(true)` joining the betas).
    pending: Option<bool>,
    /// Where the channel is kept ("beta", or anything else for stable). `None`: not kept (tests).
    channel_file: Option<PathBuf>,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            updater: Updater::new(&UPDATE_APP, env!("CARGO_PKG_VERSION")).with_translator(crate::i18n::updater_text),
            open: false,
            check_requested: false,
            beta: false,
            pending: None,
            channel_file: None,
        }
    }
}

/// Whether a copy running `version` follows the betas, from the channel file's text `kept`
/// ("beta" or "stable", written only when the user picks one). With none picked, a beta build
/// follows the betas and a stable build stable releases, so an installed beta is offered the
/// next beta instead of saying it is up to date.
fn channel_from(kept: Option<&str>, version: &str) -> bool {
    match kept.map(str::trim) {
        Some("beta") => true,
        Some("stable") => false,
        _ => psg_brand::Release::of(version).follows_betas_by_default(),
    }
}

impl Updates {
    /// Updates whose channel is kept in `channel_file`.
    pub fn with_channel_file(channel_file: PathBuf) -> Self {
        let beta = channel_from(std::fs::read_to_string(&channel_file).ok().as_deref(), env!("CARGO_PKG_VERSION"));
        let mut updates = Self { beta, channel_file: Some(channel_file), ..Self::default() };
        updates.updater.set_prereleases(beta);
        updates
    }

    #[cfg(test)]
    pub fn beta(&self) -> bool {
        self.beta
    }

    fn set_beta(&mut self, on: bool) -> Result<(), String> {
        self.beta = on;
        self.updater.set_prereleases(on);
        match &self.channel_file {
            Some(file) => std::fs::write(file, if on { "beta" } else { "stable" }).map_err(|e| format!("{}: {e}", file.display())),
            None => Ok(()),
        }
    }

    /// The beta channel switch. Joining and leaving each wait for a second, explicit
    /// confirmation that says what the change means. Returns an error to show when the choice
    /// couldn't be kept.
    pub fn channel_ui(&mut self, ui: &mut egui::Ui) -> Option<String> {
        let mut shown = self.pending.unwrap_or(self.beta);
        // egui's square checkbox, as everywhere in this window (app.rs explains the allow).
        #[allow(clippy::disallowed_methods)]
        let changed = ui.checkbox(&mut shown, t("Get beta updates")).changed();
        if changed {
            self.pending = (shown != self.beta).then_some(shown);
        }
        let join = self.pending?;
        let mut error = None;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label(if join {
                t("Join the beta channel? Betas can have unfinished features and bugs. PSG Academic PDF then updates to every new beta, and to each stable release as soon as it is newer.")
            } else {
                t("Leave the beta channel? PSG Academic PDF keeps the version it has and updates again with the first stable release newer than it. It never goes back to an older version.")
            });
            ui.horizontal(|ui| {
                if ui.button(if join { t("Join beta") } else { t("Leave beta") }).clicked() {
                    self.pending = None;
                    error = self.set_beta(join).err();
                }
                if ui.button(t("Cancel")).clicked() {
                    self.pending = None;
                }
            });
        });
        error
    }

    /// The "Check for updates" button.
    pub fn request_check(&mut self) {
        self.open = true;
        self.check_requested = true;
    }

    /// Runs the check and draws its prompt and dialog. `can_start` is false while a compilation
    /// runs, which holds a ready update back. Returns true when the window should close so that
    /// the update installs.
    pub fn frame(&mut self, ctx: &egui::Context, can_start: bool) -> bool {
        if std::mem::take(&mut self.check_requested) {
            self.updater.check_now(ctx);
        }
        self.updater.tick(ctx);
        let mut close = false;
        match self.updater.show_if(ctx, can_start) {
            Action::None => {}
            Action::OpenUrl(url) => ctx.open_url(egui::OpenUrl::new_tab(url)),
            Action::Close => close = true,
        }
        // A check that found a release is answered by the prompt above, not by the dialog.
        if self.open && matches!(self.updater.status(), Status::Idle) {
            self.open = false;
        }
        if self.open {
            let response = CheckResultDialog::new(&UPDATE_APP, env!("CARGO_PKG_VERSION"))
                .with_translator(crate::i18n::updater_text)
                .show(ctx, self.updater.status());
            match response {
                CheckResultResponse::None => {}
                CheckResultResponse::Close => {
                    self.open = false;
                    self.updater.dismiss_status();
                }
                CheckResultResponse::CheckAgain => self.check_requested = true,
                CheckResultResponse::RestartToUpdate => self.updater.restart_to_update(),
            }
        }
        close
    }

    /// Starts the helper of an update that waits for the app to exit.
    pub fn exiting(&mut self) {
        self.updater.start_pending_update();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_beta_channel_is_kept_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("update-channel.txt");
        let own = channel_from(None, env!("CARGO_PKG_VERSION"));
        assert_eq!(Updates::with_channel_file(file.clone()).beta(), own, "the build's own channel unless chosen");
        let mut updates = Updates::with_channel_file(file.clone());
        updates.set_beta(true).unwrap();
        assert!(updates.updater.prereleases());
        let again = Updates::with_channel_file(file.clone());
        assert!(again.beta() && again.updater.prereleases());
        updates.set_beta(false).unwrap();
        assert!(!Updates::with_channel_file(file).beta());
    }

    /// The bug: an installed beta followed the stable channel unless its user had joined the
    /// betas, so it said "up to date" while newer betas were out.
    #[test]
    fn a_beta_follows_the_betas_until_its_user_picks_a_channel() {
        assert!(channel_from(None, "0.20.1-beta.65"));
        assert!(!channel_from(None, "0.20.0"));
        assert!(!channel_from(Some("stable"), "0.20.1-beta.65"), "a picked channel wins");
        assert!(channel_from(Some("beta\n"), "0.20.0"));
        assert!(channel_from(Some(""), "0.20.1-beta.65"));
    }

    #[test]
    fn the_update_channel_is_this_products_own() {
        // The stem the updater matches release assets by is this product's, not PSG PDF's.
        assert_eq!(UPDATE_APP.stems().collect::<Vec<_>>(), ["psg-academic-pdf"]);
        assert_eq!(UPDATE_APP.repo, "WENSHAO521/psg-pdf");
        assert_eq!(UPDATE_APP.name, "PSG Academic PDF");
    }

    #[test]
    fn the_updater_knows_the_msis_own_uninstall_record() {
        // An MSI copy updates only by finding exactly one uninstall record with this name and
        // publisher, so both must be what packaging/windows/psg-academic-pdf.wxs registers.
        let wxs = include_str!("../../../packaging/windows/psg-academic-pdf.wxs");
        assert!(
            wxs.contains(&format!(r#"<Package Name="{}" Manufacturer="{}""#, UPDATE_APP.name, UPDATE_APP.publisher)),
            "the MSI registers another name or publisher"
        );
        assert!(wxs.contains(&format!(r"App Paths\{}.exe", UPDATE_APP.id)));
    }
}
