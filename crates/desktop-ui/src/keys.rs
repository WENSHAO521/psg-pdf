//! Keyboard shortcuts written the way each platform writes them: `⌘S` on macOS, `Ctrl+S`
//! elsewhere.

/// `key` with the command modifier (and Shift when `shift`), as this platform writes it.
pub fn command(key: &str, shift: bool) -> String {
    command_for(cfg!(target_os = "macos"), key, shift)
}

/// [`command`] for macOS (`mac == true`) or other platforms, so both spellings can be tested.
pub fn command_for(mac: bool, key: &str, shift: bool) -> String {
    match (mac, shift) {
        (true, false) => format!("⌘{key}"),
        (true, true) => format!("⇧⌘{key}"),
        // Key names follow the interface language (German writes `Strg`); macOS keeps its symbols.
        (false, false) => format!("{}+{key}", psg_i18n::ui_text("Ctrl")),
        (false, true) => format!("{}+{}+{key}", psg_i18n::ui_text("Ctrl"), psg_i18n::ui_text("Shift")),
    }
}

/// True when the command chord `key` (with Shift when `shift`) was pressed this frame and no
/// text field has focus, and consumes it so nothing else acts on it too.
pub fn consume_command(ctx: &egui::Context, key: egui::Key, shift: bool) -> bool {
    let mods = if shift { egui::Modifiers::COMMAND | egui::Modifiers::SHIFT } else { egui::Modifiers::COMMAND };
    ctx.input_mut(|i| i.consume_key(mods, key))
}
