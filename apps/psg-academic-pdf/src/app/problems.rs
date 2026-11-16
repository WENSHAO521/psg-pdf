//! Problems: everything that needs fixing, from the checks before typesetting (scripts the
//! templates can't set, the bibliography) and from the last run's TeX and Biber logs, with a
//! hint and "Go to line" where the problem is in a file the editor shows.

use psg_academic::{Diagnostic, Severity};
use psg_desktop_ui::{Tone, widgets};

use super::{AcademicApp, ProblemFilter};
use crate::i18n::{t, tf};

impl AcademicApp {
    pub(super) fn problems_page(&mut self, ui: &mut egui::Ui) {
        let (errors, warnings) = self.problem_counts();
        widgets::page_header(ui, t("Problems"), Some(t("Errors stop typesetting; warnings are worth a look before export.")));
        ui.horizontal(|ui| {
            let all = tf("All ({n})", &[("n", &(errors + warnings).to_string())]);
            let e = tf("Errors ({n})", &[("n", &errors.to_string())]);
            let w = tf("Warnings ({n})", &[("n", &warnings.to_string())]);
            widgets::segmented(
                ui,
                &mut self.problem_filter,
                &[(ProblemFilter::All, &all), (ProblemFilter::Errors, &e), (ProblemFilter::Warnings, &w)],
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::button_enabled(ui, self.project.is_some(), widgets::ButtonKind::Secondary, t("Check again"), "").clicked() {
                    self.refresh_checks();
                }
            });
        });
        ui.add_space(psg_brand::space::S);
        if self.project.is_none() {
            widgets::empty_state(ui, t("No project open"), t("Open or create a project to check it."), |_| {});
            return;
        }
        let mut go = None;
        let editable = self.editable_files();
        let filter = self.problem_filter;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(980.0));
            widgets::section_header(ui, t("Before typesetting"));
            let pre: Vec<&Diagnostic> = self.preflight.iter().filter(|d| keep(filter, d)).collect();
            if pre.is_empty() {
                widgets::muted(ui, t("No problems in the manuscript's scripts or the bibliography."));
            }
            for (i, d) in pre.into_iter().enumerate() {
                ui.push_id(("pre", i), |ui| {
                    if let Some(target) = problem_card(ui, d, &editable) {
                        go = Some(target);
                    }
                });
            }
            ui.add_space(psg_brand::space::L);
            widgets::section_header(ui, t("From the last typesetting run"));
            let Some(report) = &self.report else {
                widgets::muted(ui, t("Typeset to see errors and warnings here."));
                return;
            };
            widgets::muted(ui, super::summary(report));
            let tex: Vec<&Diagnostic> = report.diagnostics.iter().filter(|d| keep(filter, d)).collect();
            if tex.is_empty() {
                widgets::muted(ui, t("No errors or warnings."));
            }
            for (i, d) in tex.into_iter().enumerate() {
                ui.push_id(("tex", i), |ui| {
                    if let Some(target) = problem_card(ui, d, &editable) {
                        go = Some(target);
                    }
                });
            }
        });
        if let Some((file, line)) = go {
            self.go_to(&file, line);
        }
    }
}

fn keep(filter: ProblemFilter, d: &Diagnostic) -> bool {
    match filter {
        ProblemFilter::All => true,
        ProblemFilter::Errors => d.severity == Severity::Error,
        ProblemFilter::Warnings => d.severity == Severity::Warning,
    }
}

/// One problem as a card: severity, where, the message, the hint, and Go to line.
fn problem_card(ui: &mut egui::Ui, d: &Diagnostic, editable: &[String]) -> Option<(String, Option<u32>)> {
    let mut go = None;
    widgets::card(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            match d.severity {
                Severity::Error => widgets::chip(ui, Tone::Danger, t("Error")),
                Severity::Warning => widgets::chip(ui, Tone::Warning, t("Warning")),
            };
            if let Some(file) = &d.file {
                let place = match d.line {
                    Some(line) => format!("{}:{line}", file.trim_start_matches("./")),
                    None => file.trim_start_matches("./").to_owned(),
                };
                ui.label(egui::RichText::new(place).monospace().small());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let file = d.file.as_deref().map(|f| f.trim_start_matches("./")).filter(|f| editable.iter().any(|e| e == f));
                if let Some(file) = file
                    && widgets::secondary(ui, if d.line.is_some() { t("Go to line") } else { t("Open file") }).clicked()
                {
                    go = Some((file.to_owned(), d.line));
                }
            });
        });
        let (message, hint) = super::problem_text(d);
        ui.add(egui::Label::new(message).wrap());
        if let Some(hint) = hint {
            widgets::muted(ui, t(hint));
        }
    });
    ui.add_space(psg_brand::space::S);
    go
}
