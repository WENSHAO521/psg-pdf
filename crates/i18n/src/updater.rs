//! The words of the shared updater (`vendor/craft-update`), which both apps translate.
//!
//! craft-update draws its prompt and dialogs in English and asks the app's translator for each
//! string (`with_translator`), with the English as the key and `{name}` values beside it. The
//! apps pass [`translate`]-shaped functions over their own catalogs, and their completeness tests
//! require a row for each string here. A test checks that each one still appears in the updater's
//! source, so the list follows the vendored crate.

/// Every string the update prompt and dialogs ask to translate. Buttons are looked up as written
/// here (title case) and drawn in capitals.
pub const TEXT: [&str; 34] = [
    mark("{app} {version} is available with the latest features and bug fixes."),
    mark("Downloading and installing update…"),
    mark("Downloading update…"),
    mark("Reconnecting…"),
    mark("Verifying update…"),
    mark("What's new"),
    mark("Dismiss"),
    mark("Restart to Update"),
    mark("Update and Restart"),
    mark("Download"),
    mark("Later"),
    mark("Update"),
    mark("Close"),
    mark("Checking for updates…"),
    mark("Installing update…"),
    mark("Update ready. Close {app} to install it."),
    mark("The update is installed. Close {app} to finish; it restarts by itself."),
    mark("{app} {version} follows the version chosen in the launcher, so it does not update itself."),
    mark("The update could not be installed: {reason}"),
    mark("Could not check for updates."),
    mark("All releases are listed at {url}"),
    mark("{app} {version} is the newest version available."),
    mark("Check Again"),
    mark("OK"),
    mark("{app} updated to {version}."),
    mark("Cannot auto-update this installation type. Please download manually."),
    mark("Close {app} to finish"),
    mark("Close {app}"),
    mark("Couldn't start the update ({error})"),
    mark("Save or close your open work first; the update restarts the app."),
    mark("The update failed unexpectedly."),
    mark("The update stopped unexpectedly."),
    mark("by {developer}"),
    mark("v{from} to {to}"),
];

/// Marks interface text that is translated where it is drawn rather than here (the English is
/// the catalog key). It returns `english` unchanged; [`crate::check::source_keys`] finds it.
pub const fn mark(english: &'static str) -> &'static str {
    english
}
