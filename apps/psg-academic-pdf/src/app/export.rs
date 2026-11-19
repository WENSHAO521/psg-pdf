//! Export: the last checks before a PDF leaves the project — typeset, no errors, the required
//! metadata — then the export itself, or opening the PDF in PSG PDF for review.

use psg_desktop_ui::{Tone, widgets};

use super::AcademicApp;
use crate::i18n::{t, tn};

impl AcademicApp {
    fn current_pdf(&self) -> bool {
        !self.source.dirty() && self.project.as_ref().is_some_and(|project| project.pdf_is_current())
    }

    fn export_blocker(&self, current: bool) -> Option<&'static str> {
        if self.running.is_some() {
            Some(t("Typesetting is already running."))
        } else if !current {
            Some(t("Typeset the current manuscript before exporting."))
        } else if self.problem_counts().0 > 0 || self.project.as_ref().is_some_and(|p| p.file.metadata.validate().iter().any(|i| i.blocking)) {
            Some(t("Resolve the errors before exporting."))
        } else {
            None
        }
    }

    pub(super) fn check_export(&mut self) -> bool {
        self.apply_buffers();
        if let Some(reason) = self.export_blocker(self.current_pdf()) {
            self.fail(reason);
            return false;
        }
        true
    }

    pub(super) fn export_page(&mut self, ui: &mut egui::Ui) {
        self.apply_buffers();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(820.0));
            widgets::page_header(ui, t("Export"), Some(t("Check the finished article and save the PDF where it is needed.")));
            let Some(project) = &self.project else {
                widgets::empty_state(ui, t("No project open"), t("Open or create a project, then typeset it to export a PDF."), |_| {});
                return;
            };
            let pdf = project.pdf_path();
            let file = std::fs::metadata(&pdf).ok().filter(std::fs::Metadata::is_file);
            let (errors, warnings) = self.problem_counts();
            let issues = project.file.metadata.validate();
            let blocking = issues.iter().filter(|i| i.blocking).count();
            let current = self.current_pdf();
            let blocker = self.export_blocker(current);
            let suggested = super::suggested_file_name(&project.file.metadata);
            widgets::card(ui, |ui| {
                ui.set_width(ui.available_width());
                widgets::section_header(ui, t("Before you export"));
                check(ui, current, t("The current manuscript has been typeset"), Some(t("Typeset the current manuscript before exporting.")));
                check(ui, errors == 0, t("No errors"), Some(&tn(errors, "{n} error: see Problems.", "{n} errors: see Problems.", &[])));
                check(
                    ui,
                    blocking == 0,
                    t("Required publication details are filled in"),
                    Some(t("See Manuscript ▸ Title & abstract, Authors and Publication.")),
                );
                if warnings > 0 {
                    ui.horizontal(|ui| {
                        widgets::chip(ui, Tone::Warning, tn(warnings, "{n} warning", "{n} warnings", &[]));
                        widgets::muted(ui, t("Worth a look, but they don't stop the export."));
                    });
                }
            });
            ui.add_space(psg_brand::space::M);
            let mut export = false;
            let mut open = false;
            widgets::card(ui, |ui| {
                ui.set_width(ui.available_width());
                widgets::section_header(ui, t("PDF"));
                match &file {
                    Some(meta) => {
                        widgets::form_grid(ui, "pdf-info", |ui| {
                            widgets::form_label(ui, t("File"));
                            ui.add(egui::Label::new(egui::RichText::new(pdf.display().to_string()).monospace().small()).truncate());
                            ui.end_row();
                            widgets::form_label(ui, t("Size"));
                            ui.label(format!("{:.1} KB", meta.len() as f64 / 1024.0));
                            ui.end_row();
                            if self.preview.total_pages > 0 {
                                widgets::form_label(ui, t("Pages"));
                                ui.label(self.preview.total_pages.to_string());
                                ui.end_row();
                            }
                            if let Some(date) = meta.modified().ok().and_then(|t| jiff::ZonedDateTime::try_from(t).ok()) {
                                widgets::form_label(ui, t("Typeset"));
                                ui.label(date.strftime("%Y-%m-%d %H:%M").to_string());
                                ui.end_row();
                            }
                            widgets::form_label(ui, t("Suggested name"));
                            ui.label(egui::RichText::new(&suggested).monospace());
                            ui.end_row();
                        });
                    }
                    None => {
                        widgets::muted(ui, t("No PDF yet."));
                    }
                }
                ui.add_space(psg_brand::space::M);
                ui.horizontal(|ui| {
                    let ready = blocker.is_none();
                    let why = blocker.unwrap_or_default();
                    export = widgets::button_enabled(ui, ready, widgets::ButtonKind::Primary, t("Export PDF…"), why).clicked();
                    open = widgets::button_enabled(ui, ready, widgets::ButtonKind::Secondary, t("Open in PSG PDF"), why).clicked();
                });
            });
            if export {
                self.export();
            }
            if open {
                self.open_in_psg_pdf();
            }
        });
    }
}

/// One line of the checklist: a mark, the check, and what to do when it fails.
fn check(ui: &mut egui::Ui, ok: bool, label: &str, fix: Option<&str>) {
    ui.horizontal(|ui| {
        widgets::status_dot(ui, if ok { Tone::Success } else { Tone::Danger });
        ui.label(label);
        if !ok && let Some(fix) = fix {
            widgets::muted(ui, fix);
        }
    });
}
