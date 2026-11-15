//! The source editor: the manuscript body and the bibliography files, side by side with the PDF.
//!
//! Only the files a person is meant to change are offered (the template's body file and the
//! project's `.bib` files); the template's class and main file stay as the journal set them.
//! Lines with a problem are marked in the gutter, and a problem's "Go to line" puts the cursor on
//! its line and scrolls to it.

use std::sync::Arc;

use egui::text::{CCursor, CCursorRange, LayoutJob};
use egui::{FontId, TextBuffer};

use crate::i18n::tf;

/// The most text the editor holds (a body or a .bib file); larger files are refused.
const MAX_EDIT: usize = 16 * 1024 * 1024;

#[derive(Default)]
pub struct SourceEditor {
    /// The file shown, by name in the project folder.
    pub file: String,
    pub text: String,
    /// The text as last loaded or saved, to tell unsaved changes.
    saved: String,
    /// A line (1-based) to put the cursor on at the next frame.
    jump: Option<u32>,
    pub error: Option<String>,
}

impl SourceEditor {
    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }

    /// Load `file` from `dir` (the caller saves or discards unsaved changes first).
    pub fn load(&mut self, dir: &std::path::Path, file: &str) {
        let path = dir.join(file);
        match std::fs::metadata(&path) {
            Ok(m) if m.len() > MAX_EDIT as u64 => {
                self.error = Some(tf("{file} is too large to edit here.", &[("file", file)]));
                return;
            }
            _ => {}
        }
        match std::fs::read(&path).map(|b| String::from_utf8(b).map_err(|_| ())) {
            Ok(Ok(text)) => {
                let text = text.strip_prefix('\u{feff}').map(str::to_owned).unwrap_or(text);
                self.file = file.to_owned();
                self.saved.clone_from(&text);
                self.text = text;
                self.error = None;
            }
            Ok(Err(())) => self.error = Some(tf("{file} is not UTF-8 text; save it as UTF-8 and try again.", &[("file", file)])),
            Err(e) => self.error = Some(format!("{}: {e}", path.display())),
        }
    }

    /// Mark the current text as saved (after the caller wrote it).
    pub fn mark_saved(&mut self) {
        self.saved.clone_from(&self.text);
    }

    /// Put the cursor on `line` (1-based) at the next frame.
    pub fn go_to(&mut self, line: u32) {
        self.jump = Some(line.max(1));
    }

    /// Draw the editor. `marked` are the lines with a problem, `files` the ones that can be shown.
    /// Returns the file chosen in the file list, if another one was picked.
    pub fn show(&mut self, ui: &mut egui::Ui, files: &[String], marked: &[u32]) -> Option<String> {
        let mut picked = None;
        ui.horizontal(|ui| {
            for f in files {
                let label = if *f == self.file && self.dirty() { format!("{f} •") } else { f.clone() };
                if ui.selectable_label(*f == self.file, label).clicked() && *f != self.file {
                    picked = Some(f.clone());
                }
            }
        });
        if let Some(e) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        let font = FontId::monospace(13.0);
        let row = ui.fonts_mut(|f| f.row_height(&font));
        let id = ui.scope_id().with(("source-editor", &self.file));
        let text_color = ui.visuals().text_color();
        let mut scroll = egui::ScrollArea::both().id_salt(("source-scroll", &self.file)).auto_shrink([false, false]);
        if let Some(line) = self.jump {
            // A few lines of context above the one asked for.
            scroll = scroll.vertical_scroll_offset(((line.saturating_sub(4)) as f32 * row).max(0.0));
        }
        scroll.show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let lines = self.text.as_str().lines().count().max(1) + usize::from(self.text.ends_with('\n'));
                let mut gutter = LayoutJob::default();
                for n in 1..=lines {
                    let bad = u32::try_from(n).is_ok_and(|n| marked.contains(&n));
                    let color = if bad { ui.visuals().error_fg_color } else { ui.visuals().weak_text_color() };
                    gutter.append(&format!("{}{n:>5}\n", if bad { "●" } else { " " }), 0.0, egui::TextFormat::simple(font.clone(), color));
                }
                ui.label(gutter);
                let mut layouter = |ui: &egui::Ui, buf: &dyn TextBuffer, _wrap: f32| -> Arc<egui::Galley> {
                    // No wrapping, so editor lines stay level with the gutter's numbers.
                    let job = LayoutJob::simple(buf.as_str().to_owned(), font.clone(), text_color, f32::INFINITY);
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                let response = ui.add(
                    egui::TextEdit::multiline(&mut self.text)
                        .id(id)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(30)
                        .lock_focus(true)
                        .layouter(&mut layouter),
                );
                if let Some(line) = self.jump.take() {
                    let index = char_index_of_line(&self.text, line);
                    if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(index))));
                        state.store(ui.ctx(), id);
                    }
                    response.request_focus();
                }
            });
        });
        picked
    }
}

/// The character index where `line` (1-based) starts; the end of the text past the last line.
pub fn char_index_of_line(text: &str, line: u32) -> usize {
    let mut chars = 0usize;
    for (n, l) in text.split_inclusive('\n').enumerate() {
        if u32::try_from(n + 1).is_ok_and(|n| n >= line) {
            return chars;
        }
        chars += l.chars().count();
    }
    chars
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn line_starts_count_characters_not_bytes() {
        let text = "第一行\nsecond\n第三 line\n";
        assert_eq!(char_index_of_line(text, 1), 0);
        assert_eq!(char_index_of_line(text, 2), 4);
        assert_eq!(char_index_of_line(text, 3), 11);
        assert_eq!(char_index_of_line(text, 99), text.chars().count());
        assert_eq!(char_index_of_line("", 1), 0);
    }

    #[test]
    fn loading_and_saving_tracks_changes() {
        let dir = tempfile::tempdir().unwrap();
        let _ = std::fs::write(dir.path().join("body.tex"), "\u{feff}Hello 张三\n");
        let mut e = SourceEditor::default();
        e.load(dir.path(), "body.tex");
        assert_eq!(e.text, "Hello 张三\n");
        assert!(!e.dirty());
        e.text.push('x');
        assert!(e.dirty());
        e.mark_saved();
        assert!(!e.dirty());
        let _ = std::fs::write(dir.path().join("bad.bib"), [0xff, 0xfe, 0x00]);
        e.load(dir.path(), "bad.bib");
        assert!(e.error.is_some() && e.file == "body.tex");
    }
}
