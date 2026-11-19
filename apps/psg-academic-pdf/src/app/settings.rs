//! Settings: language, appearance, the TeX installation, the data folder, updates and About.

use psg_desktop_ui::style::ThemeChoice;
use psg_desktop_ui::widgets;

use super::{APP_NAME, AcademicApp, PUBLISHER};
use crate::i18n::t;

impl AcademicApp {
    pub(super) fn settings_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(820.0));
            widgets::page_header(ui, t("Settings"), None);
            widgets::card(ui, |ui| {
                ui.set_width(ui.available_width());
                widgets::section_header(ui, t("Language and appearance"));
                widgets::form_grid(ui, "appearance", |ui| {
                    widgets::form_label(ui, t("Language"));
                    self.language_picker(ui);
                    ui.end_row();
                    widgets::form_label(ui, t("Theme"));
                    let before = self.theme;
                    widgets::segmented(
                        ui,
                        &mut self.theme,
                        &[(ThemeChoice::System, t("Follow system")), (ThemeChoice::Light, t("Light")), (ThemeChoice::Dark, t("Dark"))],
                    );
                    if before != self.theme {
                        psg_desktop_ui::style::set_theme(ui.ctx(), self.theme);
                        if let Err(e) = write_setting(&self.theme_file, self.theme.key()) {
                            self.note(format!("{}: {e}", self.theme_file.display()));
                        }
                    }
                    ui.end_row();
                    widgets::form_label(ui, t("Interface size"));
                    ui.horizontal(|ui| {
                        let zoom = ui.ctx().zoom_factor();
                        if ui.small_button("−").clicked() {
                            ui.ctx().set_zoom_factor((zoom - 0.1).max(0.7));
                        }
                        ui.label(format!("{:.0}%", zoom * 100.0));
                        if ui.small_button("+").clicked() {
                            ui.ctx().set_zoom_factor((zoom + 0.1).min(2.0));
                        }
                        if ui.small_button(t("Reset")).clicked() {
                            ui.ctx().set_zoom_factor(1.0);
                        }
                        for key in ["+", "-", "0"] {
                            widgets::kbd(ui, &psg_desktop_ui::keys::command(key, false));
                        }
                    });
                    ui.end_row();
                });
            });
            ui.add_space(psg_brand::space::M);
            widgets::card(ui, |ui| {
                ui.set_width(ui.available_width());
                widgets::section_header(ui, t("TeX installation"));
                self.tex_status(ui);
            });
            ui.add_space(psg_brand::space::M);
            widgets::card(ui, |ui| {
                ui.set_width(ui.available_width());
                self.data_folder_card(ui);
            });
            ui.add_space(psg_brand::space::M);
            widgets::card(ui, |ui| {
                ui.set_width(ui.available_width());
                widgets::section_header(ui, t("Updates"));
                if widgets::secondary(ui, t("Check for updates…")).clicked() {
                    self.updates.request_check();
                }
                if let Some(error) = self.updates.channel_ui(ui) {
                    self.note(error);
                }
            });
            ui.add_space(psg_brand::space::M);
            widgets::card(ui, |ui| {
                ui.set_width(ui.available_width());
                widgets::section_header(ui, t("About"));
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(APP_NAME).strong());
                    ui.label(crate::theme::display_version());
                    widgets::release_badge(ui, env!("CARGO_PKG_VERSION"));
                });
                widgets::muted(ui, PUBLISHER);
                widgets::muted(ui, t("PDF engine: PdfCraft (MIT OR Apache-2.0)"));
                ui.add_space(psg_brand::space::S);
                ui.horizontal_wrapped(|ui| {
                    widgets::muted(ui, t("Shortcuts:"));
                    for (keys, what) in [
                        (psg_desktop_ui::keys::command("S", false), t("Save")),
                        (psg_desktop_ui::keys::command("R", false), t("Auto Typeset")),
                        (psg_desktop_ui::keys::command("E", false), t("Export")),
                        (psg_desktop_ui::keys::command("O", false), t("Open project…")),
                        (psg_desktop_ui::keys::command("1…7", false), t("Switch area")),
                    ] {
                        widgets::kbd(ui, &keys);
                        ui.label(what);
                    }
                });
            });
        });
    }

    /// The interface language: the system's (the default), or one picked here. The choice is
    /// saved in the data folder and shows on the next frame.
    fn language_picker(&mut self, ui: &mut egui::Ui) {
        let saved = self.language;
        let current = crate::i18n::language();
        let shown = match saved {
            psg_i18n::Preference::Auto => crate::i18n::tf("Follow system ({language})", &[("language", current.native_name)]),
            psg_i18n::Preference::Fixed(l) => l.native_name.to_owned(),
        };
        let mut chosen = None;
        egui::ComboBox::from_id_salt("language").selected_text(shown).show_ui(ui, |ui| {
            if ui.selectable_label(saved == psg_i18n::Preference::Auto, t("Follow system")).clicked() {
                chosen = Some(psg_i18n::Preference::Auto);
            }
            for lang in &psg_i18n::LANGUAGES {
                // Each language in its own name, so a reader can find theirs whatever is showing.
                let item = ui.selectable_label(saved == psg_i18n::Preference::Fixed(lang), lang.native_name).on_hover_text(lang.english_name);
                if item.clicked() {
                    chosen = Some(psg_i18n::Preference::Fixed(lang));
                }
            }
        });
        if let Some(pref) = chosen {
            self.language = pref;
            crate::i18n::apply(pref);
            crate::fonts::install(ui.ctx(), crate::i18n::current());
            if let Err(e) = write_setting(&self.language_file, pref.as_str()) {
                self.note(format!("{}: {e}", self.language_file.display()));
            }
        }
    }
}

/// Save a one-line setting, creating the data folder on a first run.
fn write_setting(path: &std::path::Path, value: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, value)
}
