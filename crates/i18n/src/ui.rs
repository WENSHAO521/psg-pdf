//! The translator the shared design-system widgets use.
//!
//! `psg-desktop-ui` draws a few words of its own (a close button's "Dismiss", a splitter's
//! "Resize panes"). It doesn't know which app it is in, so each app installs its lookup once at
//! startup, and the widgets call [`ui_text`] each time they draw: a change of language shows on
//! the next frame. Without an installed translator the text stays English.

use std::sync::RwLock;

/// The English `text` (the key) in the app's current interface language.
pub type Translate = fn(text: &str) -> String;

static TRANSLATOR: RwLock<Option<Translate>> = RwLock::new(None);

/// Make `f` the translator for [`ui_text`]. Calling it again replaces the previous one.
pub fn install_ui_translator(f: Translate) {
    // A poisoned lock only means a panic elsewhere while it was held; the value is a plain `fn`.
    *TRANSLATOR.write().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(f);
}

/// `text` in the app's interface language, or unchanged when no translator is installed.
pub fn ui_text(text: &str) -> String {
    let f = *TRANSLATOR.read().unwrap_or_else(std::sync::PoisonError::into_inner);
    f.map_or_else(|| text.to_owned(), |f| f(text))
}
