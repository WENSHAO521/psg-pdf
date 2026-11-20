//! The source editor: the manuscript body and the bibliography files, side by side with the PDF.
//!
//! Every LaTeX and bibliography file of the project is offered (the body first, then the main
//! file, the class, the `.bib` files and any chapter), and new ones can be made; only the
//! metadata file, which the metadata form writes, is left out. LaTeX is coloured as it is typed.
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
    /// The name typed for a new file.
    pub new_file: String,
    /// The coloured layout of the text last drawn, kept while the text doesn't change.
    highlighted: Option<(u64, Arc<LayoutJob>)>,
}

/// What the editor asks of the app, besides editing.
pub enum Request {
    /// Show this file.
    Open(String),
    /// Make this new file and show it.
    Create(String),
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
    /// Returns what was asked: another file from the list, or a new file.
    pub fn show(&mut self, ui: &mut egui::Ui, files: &[String], marked: &[u32]) -> Option<Request> {
        let mut picked = None;
        ui.horizontal_wrapped(|ui| {
            for f in files {
                let label = if *f == self.file && self.dirty() { format!("{f} •") } else { f.clone() };
                if ui.selectable_label(*f == self.file, label).clicked() && *f != self.file {
                    picked = Some(Request::Open(f.clone()));
                }
            }
            ui.separator();
            let field = ui.add(egui::TextEdit::singleline(&mut self.new_file).desired_width(140.0).hint_text("chapter2.tex"));
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button(crate::i18n::t("New file")).clicked() || entered) && !self.new_file.trim().is_empty() {
                picked = Some(Request::Create(std::mem::take(&mut self.new_file).trim().to_owned()));
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
                let colors = Colors::for_visuals(ui.visuals(), text_color);
                let mut cache = self.highlighted.take();
                let mut layouter = |ui: &egui::Ui, buf: &dyn TextBuffer, _wrap: f32| -> Arc<egui::Galley> {
                    let text = buf.as_str();
                    let key = text_key(text);
                    let job = match &cache {
                        Some((k, job)) if *k == key => Arc::clone(job),
                        _ => {
                            let job = Arc::new(highlight(text, &font, &colors));
                            cache = Some((key, Arc::clone(&job)));
                            job
                        }
                    };
                    ui.fonts_mut(|f| f.layout_job((*job).clone()))
                };
                let response = crate::clipboard::text_edit(
                    ui,
                    egui::TextEdit::multiline(&mut self.text)
                        .id(id)
                        .code_editor()
                        .desired_width(f32::INFINITY)
                        .desired_rows(30)
                        .lock_focus(true)
                        .layouter(&mut layouter),
                );
                self.highlighted = cache;
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

/// The colours of LaTeX source.
struct Colors {
    text: egui::Color32,
    command: egui::Color32,
    comment: egui::Color32,
    math: egui::Color32,
    brace: egui::Color32,
}

impl Colors {
    fn for_visuals(v: &egui::Visuals, text: egui::Color32) -> Colors {
        let (command, math) = if v.dark_mode {
            (egui::Color32::from_rgb(0x8c, 0xb4, 0xff), egui::Color32::from_rgb(0x7f, 0xd1, 0x9a))
        } else {
            (egui::Color32::from_rgb(0x1f, 0x4f, 0xb8), egui::Color32::from_rgb(0x1d, 0x7a, 0x3e))
        };
        Colors { text, command, comment: v.weak_text_color(), math, brace: v.hyperlink_color }
    }
}

/// Text longer than this is shown without colours, so typing stays quick.
const MAX_HIGHLIGHT: usize = 2 * 1024 * 1024;

/// A cheap fingerprint of the text, to tell when it changed.
fn text_key(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

/// What a stretch of LaTeX is, for its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    Command,
    Comment,
    Math,
    Brace,
}

/// The LaTeX in `text` cut into coloured stretches: commands (`\section`, `@article` in a
/// `.bib`), comments, inline and display math, and braces. Never wraps, so lines stay level
/// with the gutter's numbers.
fn highlight(text: &str, font: &FontId, colors: &Colors) -> LayoutJob {
    let mut job = LayoutJob { wrap: egui::text::TextWrapping { max_width: f32::INFINITY, ..Default::default() }, ..Default::default() };
    let color = |k: Kind| match k {
        Kind::Text => colors.text,
        Kind::Command => colors.command,
        Kind::Comment => colors.comment,
        Kind::Math => colors.math,
        Kind::Brace => colors.brace,
    };
    if text.len() > MAX_HIGHLIGHT {
        job.append(text, 0.0, egui::TextFormat::simple(font.clone(), colors.text));
        return job;
    }
    for (start, end, kind) in spans(text) {
        if let Some(piece) = text.get(start..end) {
            job.append(piece, 0.0, egui::TextFormat::simple(font.clone(), color(kind)));
        }
    }
    job
}

/// The stretches of [`highlight`], as byte ranges that cover the whole text.
fn spans(text: &str) -> Vec<(usize, usize, Kind)> {
    let bytes = text.as_bytes();
    let mut out: Vec<(usize, usize, Kind)> = Vec::new();
    let mut push = |start: usize, end: usize, kind: Kind| {
        if end <= start {
            return;
        }
        match out.last_mut() {
            Some(last) if last.2 == kind && last.1 == start => last.1 = end,
            _ => out.push((start, end, kind)),
        }
    };
    let mut i = 0;
    let mut math: Option<&[u8]> = None;
    while i < bytes.len() {
        let Some(&b) = bytes.get(i) else { break };
        let rest = text.get(i..).unwrap_or_default();
        let in_math = math.is_some();
        let base = if in_math { Kind::Math } else { Kind::Text };
        match b {
            b'%' => {
                let end = rest.find('\n').map_or(bytes.len(), |n| i + n);
                push(i, end, Kind::Comment);
                i = end;
            }
            b'\\' => {
                let mut end = i + 1;
                while bytes.get(end).is_some_and(u8::is_ascii_alphabetic) {
                    end += 1;
                }
                if end == i + 1 {
                    // A control symbol: \\, \%, \$, \(, \[ …
                    end = (i + 1 + rest.get(1..).and_then(|r| r.chars().next()).map_or(0, char::len_utf8)).min(bytes.len());
                    let sym = bytes.get(i + 1..end).unwrap_or_default();
                    match (math, sym) {
                        (None, b"(") => math = Some(b"\\)"),
                        (None, b"[") => math = Some(b"\\]"),
                        (Some(close), _) if close.get(1..) == Some(sym) && close.first() == Some(&b'\\') => math = None,
                        _ => {}
                    }
                    push(i, end, if math.is_some() || in_math { Kind::Math } else { Kind::Command });
                } else {
                    push(i, end, if in_math { Kind::Math } else { Kind::Command });
                }
                i = end;
            }
            b'@' if !in_math && (i == 0 || bytes.get(i - 1) == Some(&b'\n')) => {
                let mut end = i + 1;
                while bytes.get(end).is_some_and(u8::is_ascii_alphabetic) {
                    end += 1;
                }
                push(i, end, Kind::Command);
                i = end;
            }
            b'$' => {
                let double = bytes.get(i + 1) == Some(&b'$');
                let len = if double { 2 } else { 1 };
                let close: &[u8] = if double { b"$$" } else { b"$" };
                match math {
                    Some(c) if c == close => math = None,
                    None => math = Some(close),
                    _ => {}
                }
                push(i, i + len, Kind::Math);
                i += len;
            }
            b'{' | b'}' if !in_math => {
                push(i, i + 1, Kind::Brace);
                i += 1;
            }
            _ => {
                let len = rest.chars().next().map_or(1, char::len_utf8);
                push(i, i + len, base);
                i += len;
            }
        }
    }
    out
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
    fn latex_is_coloured_by_kind() {
        let text = "\\section{Intro} 50\\% $x^2$ % note\n@article{a,\n中文 \\(y\\)";
        let kinds: Vec<(&str, Kind)> = spans(text).into_iter().map(|(a, b, k)| (&text[a..b], k)).collect();
        assert!(kinds.contains(&("\\section", Kind::Command)), "{kinds:?}");
        assert!(kinds.contains(&("{", Kind::Brace)));
        assert!(kinds.contains(&("\\%", Kind::Command)), "an escaped percent sign is not a comment");
        assert!(kinds.contains(&("$x^2$", Kind::Math)), "{kinds:?}");
        assert!(kinds.contains(&("% note", Kind::Comment)));
        assert!(kinds.contains(&("@article", Kind::Command)));
        assert!(kinds.contains(&("\\(y\\)", Kind::Math)), "{kinds:?}");
        // The stretches cover the text exactly, and cut only at character boundaries.
        assert_eq!(kinds.iter().map(|(t, _)| *t).collect::<String>(), text);
        // An unfinished command at the end, or a lone backslash, is safe.
        for t in ["\\", "x\\", "$", "%", "\\中"] {
            assert_eq!(spans(t).iter().map(|(a, b, _)| &t[*a..*b]).collect::<String>(), t);
        }
    }

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
