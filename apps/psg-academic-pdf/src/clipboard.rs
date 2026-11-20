use egui::text::CCursorRange;
use egui::{Response, TextEdit, Ui, ViewportCommand};

use crate::i18n::t;

/// A native text input with clipboard actions on its right-click menu.
pub fn text_edit(ui: &mut Ui, edit: TextEdit<'_>) -> Response {
    let egui::text_edit::TextEditOutput { response, mut state, .. } = edit.show(ui);
    let response = response.response;
    let id = response.id.with("clipboard-selection");
    let saved = ui.data(|data| data.get_temp::<CCursorRange>(id));
    let right_press = response.contains_pointer() && ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Secondary));
    let menu_open = response.context_menu_opened();
    // A secondary press can move the caret and the popup can briefly take focus. Keep the
    // editor's selection through both so Copy, Cut and Paste use the intended range.
    let preserved = if right_press {
        saved.filter(|range| !range.is_empty())
    } else if menu_open {
        saved
    } else {
        None
    };
    let range = preserved.or_else(|| state.cursor.char_range());
    let mut restore = right_press && preserved.is_some();
    response.context_menu(|ui| {
        let selected = range.is_some_and(|range| !range.is_empty());
        for (label, key, action, enabled) in [
            (t("Cut"), "X", ViewportCommand::RequestCut, selected),
            (t("Copy"), "C", ViewportCommand::RequestCopy, selected),
            (t("Paste"), "V", ViewportCommand::RequestPaste, true),
        ] {
            let button = egui::Button::new(label).shortcut_text(psg_desktop_ui::keys::command(key, false));
            if ui.add_enabled(enabled, button).clicked() {
                // The window supplies an ordinary clipboard event on its next pass.
                response.request_focus();
                restore = true;
                ui.close();
                ui.ctx().send_viewport_cmd(action);
            }
        }
    });
    if restore {
        state.cursor.set_char_range(range);
        state.store(ui.ctx(), response.id);
    }
    ui.data_mut(|data| {
        if let Some(range) = range.filter(|range| right_press || menu_open || !range.is_empty()) {
            data.insert_temp(id, range);
        } else {
            data.remove::<CCursorRange>(id);
        }
    });
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_click_opens_the_text_edit_menu_and_keeps_the_selection() {
        let ctx = egui::Context::default();
        let mut text = String::from("manuscript");
        let mut rect = egui::Rect::NOTHING;
        let mut id = egui::Id::NULL;
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let edit = text_edit(ui, TextEdit::singleline(&mut text));
            rect = edit.rect;
            id = edit.id;
            edit.request_focus();
        });
        output.drop_without_applying_deltas();
        let select_all = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
        let key = egui::Event::Key { key: egui::Key::A, physical_key: None, pressed: true, repeat: false, modifiers: select_all };
        let selected = || TextEdit::load_state(&ctx, id).and_then(|s| s.cursor.char_range()).is_some_and(|range| !range.is_empty());
        let input = egui::RawInput { events: vec![key], ..Default::default() };
        let output = ctx.run_ui(input, |ui| {
            text_edit(ui, TextEdit::singleline(&mut text));
        });
        output.drop_without_applying_deltas();
        assert!(selected());
        let pos = rect.center();
        let mut opened = false;
        for pressed in [true, false] {
            let click = egui::Event::PointerButton { pos, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE };
            let input = egui::RawInput { events: vec![egui::Event::PointerMoved(pos), click], ..Default::default() };
            let output = ctx.run_ui(input, |ui| {
                opened |= text_edit(ui, TextEdit::singleline(&mut text)).context_menu_opened();
            });
            output.drop_without_applying_deltas();
        }
        assert!(opened);
        assert!(selected());
        assert_eq!(text, "manuscript");
    }
}
