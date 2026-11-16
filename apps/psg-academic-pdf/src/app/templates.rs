//! Template Library: the journal templates as cards. One is chosen for new projects; the open
//! project's template is marked. Importing a template from a ZIP file and checking a template's
//! compatibility are planned and shown as such, never as working buttons.

use psg_desktop_ui::{Tone, widgets};

use super::{AcademicApp, Page};
use crate::i18n::{t, tf};

impl AcademicApp {
    pub(super) fn templates_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(1100.0));
            widgets::page_header(ui, t("Template Library"), Some(t("Journal templates set the layout, fonts and citation style. New projects use the selected template.")));
            ui.horizontal_wrapped(|ui| {
                let soon = t("Planned for a later version; not available yet.");
                widgets::button_enabled(ui, false, widgets::ButtonKind::Secondary, t("Import template (.zip)…"), soon);
                widgets::button_enabled(ui, false, widgets::ButtonKind::Secondary, t("Check compatibility"), soon);
                widgets::chip(ui, Tone::Neutral, t("Coming later"));
            });
            ui.add_space(psg_brand::space::M);
            if self.templates.is_empty() {
                widgets::banner(ui, Tone::Danger, t("No template is available."), None);
            }
            let in_use = self.project.as_ref().map(|p| p.file.template.clone());
            let width = 320.0;
            let mut chosen = None;
            let mut start = None;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(psg_brand::space::M, psg_brand::space::M);
                for (i, tpl) in self.templates.iter().enumerate() {
                    let m = &tpl.manifest;
                    let selected = i == self.selected_template;
                    let resp = widgets::selectable_card(ui, selected, width, |ui| {
                        ui.set_min_height(170.0);
                        template_thumbnail(ui, &m.id);
                        ui.add_space(psg_brand::space::S);
                        ui.add(egui::Label::new(egui::RichText::new(&m.name).font(psg_desktop_ui::fonts::semibold(psg_brand::text::HEADING))).wrap());
                        ui.add(egui::Label::new(egui::RichText::new(&m.description).small()).wrap());
                        ui.add_space(psg_brand::space::XS);
                        ui.horizontal_wrapped(|ui| {
                            widgets::chip(ui, Tone::Neutral, m.engine.clone());
                            let bib = if m.bibliography == "none" { t("No bibliography").to_owned() } else { tf("Bibliography: {tool}", &[("tool", &m.bibliography)]) };
                            widgets::chip(ui, Tone::Neutral, bib);
                            if !m.version.is_empty() {
                                widgets::chip(ui, Tone::Neutral, format!("v{}", m.version));
                            }
                            if matches!(tpl.source, psg_academic::template::Source::Builtin(_)) {
                                widgets::chip(ui, Tone::Info, t("Built in"));
                            }
                            if in_use.as_deref() == Some(m.id.as_str()) {
                                widgets::chip(ui, Tone::Success, t("Used by this project"));
                            }
                            if selected {
                                widgets::chip(ui, Tone::Info, t("Selected for new projects"));
                            }
                        });
                    });
                    let resp = resp.on_hover_text(tf("{id} · licence {licence}", &[("id", &m.id), ("licence", &m.licence)]));
                    if resp.clicked() {
                        chosen = Some(i);
                    }
                    if resp.double_clicked() {
                        start = Some(i);
                    }
                }
            });
            if let Some(i) = chosen {
                self.selected_template = i;
            }
            ui.add_space(psg_brand::space::L);
            ui.horizontal(|ui| {
                if widgets::button_enabled(ui, !self.templates.is_empty(), widgets::ButtonKind::Primary, t("New project from this template"), "").clicked() {
                    start = Some(self.selected_template);
                }
                if widgets::button_enabled(ui, self.env.is_some() && !self.templates.is_empty(), widgets::ButtonKind::Secondary, t("Import manuscript…"), "").clicked() {
                    self.import_new();
                }
            });
            if self.project.is_some() {
                ui.add_space(psg_brand::space::S);
                widgets::muted(ui, t("An open project keeps the template it was created with; the selection applies to new projects."));
            }
            ui.add_space(psg_brand::space::L);
            widgets::section_header(ui, t("Where templates come from"));
            widgets::muted(ui, t("Besides the built-in templates, PSG Academic PDF reads templates from these folders (each template is a folder with a template.toml):"));
            for dir in psg_academic::template::default_template_dirs() {
                ui.label(egui::RichText::new(dir.display().to_string()).monospace().small());
            }
            if !self.template_problems.is_empty() {
                ui.add_space(psg_brand::space::S);
                for p in &self.template_problems {
                    widgets::banner(ui, Tone::Warning, &tf("Template skipped: {problem}", &[("problem", p)]), None);
                }
            }
            if let Some(i) = start {
                self.selected_template = i;
                self.new_project();
                if self.project.is_some() {
                    self.page = Page::Manuscript;
                }
            }
        });
    }
}

/// A small drawn page that suggests the template's layout (one column with a title block), in
/// the product colours. It is a sketch, not a rendering of the template.
fn template_thumbnail(ui: &mut egui::Ui, id: &str) {
    let p = psg_desktop_ui::Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 64.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, egui::CornerRadius::same(psg_brand::radius::CONTROL), p.canvas);
    let page = egui::Rect::from_center_size(rect.center(), egui::vec2(44.0, 56.0));
    painter.rect(page, 2, egui::Color32::WHITE, egui::Stroke::new(1.0, p.border), egui::StrokeKind::Inside);
    let line = |y: f32, w: f32, c: egui::Color32| {
        painter.hline(page.left() + 5.0..=page.left() + 5.0 + w, page.top() + y, egui::Stroke::new(2.0, c));
    };
    line(7.0, 34.0, p.accent);
    line(12.0, 22.0, p.gold);
    let grey = egui::Color32::from_gray(190);
    // Humanities templates carry footnotes: a short rule near the foot of the page.
    let rows = if id.contains("humanities") { 6 } else { 7 };
    for r in 0..rows {
        line(20.0 + r as f32 * 4.5, if r % 3 == 2 { 26.0 } else { 34.0 }, grey);
    }
    if id.contains("humanities") {
        line(50.0, 14.0, grey);
    }
}
