//! Check for updates, through `craft_update`, on PSG Academic PDF's own update channel.
//!
//! The two PSG products publish into the same GitHub releases. craft-update only picks assets
//! named `<id>-<version>-<os>-<arch>.<ext>`, so with the id `psg-academic-pdf` this app is only
//! ever offered `psg-academic-pdf-…` installers, never PSG PDF's (packaging/products.toml,
//! `asset_stem`). There is no check at start: the user asks for one.

use craft_update::app::App;
use craft_update::updater::{Action, Status, Updater};
use craft_update::{CheckResultDialog, CheckResultResponse};

use crate::app::{APP_NAME, PUBLISHER};

/// PSG Academic PDF as its releases and installers know it.
pub const UPDATE_APP: App = App {
    icon_png: Some(include_bytes!("../../../assets/app-icon-academic/hicolor/128x128/apps/com.panoramascholarly.psgacademicpdf.png")),
    // packaging/products.toml `accent`.
    accent: Some(crate::theme::PRODUCT.brand()),
    developer: PUBLISHER,
    ..App::new("psg-academic-pdf", APP_NAME, crate::theme::PRODUCT.tagline(), "WENSHAO521/psg-pdf")
};

pub struct Updates {
    updater: Updater,
    /// The result dialog is showing.
    open: bool,
    check_requested: bool,
}

impl Default for Updates {
    fn default() -> Self {
        Self { updater: Updater::new(&UPDATE_APP, env!("CARGO_PKG_VERSION")), open: false, check_requested: false }
    }
}

impl Updates {
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
            let response = CheckResultDialog::new(&UPDATE_APP, env!("CARGO_PKG_VERSION")).show(ctx, self.updater.status());
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
    fn the_update_channel_is_this_products_own() {
        // The stem the updater matches release assets by is this product's, not PSG PDF's.
        assert_eq!(UPDATE_APP.stems().collect::<Vec<_>>(), ["psg-academic-pdf"]);
        assert_eq!(UPDATE_APP.repo, "WENSHAO521/psg-pdf");
        assert_eq!(UPDATE_APP.name, "PSG Academic PDF");
    }
}
