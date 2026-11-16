//! Typesetting: the task centre. What runs and how long it has taken, the run's settings (the
//! engine is always pdfLaTeX through latexmk), this session's runs, and the logs — latexmk's
//! output, the TeX log and the activity log — beside the PDF.

use psg_academic::{Outcome, compile};
use psg_desktop_ui::{Tone, layout, widgets};

use super::{AcademicApp, LogView};
use crate::i18n::{t, tf};

impl AcademicApp {
    pub(super) fn typesetting_page(&mut self, ui: &mut egui::Ui) {
        let mut ratio = self.split_ratio;
        layout::split(ui, "typeset-split", &mut ratio, 0.25, self, |ui, app| app.task_centre(ui), |ui, app| app.preview_pane(ui));
        self.split_ratio = ratio;
    }

    fn task_centre(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::ScrollArea::vertical().id_salt("task-centre").auto_shrink([false, false]).show(ui, |ui| {
            widgets::page_header(
                ui,
                t("Typesetting"),
                Some(t("latexmk runs pdfLaTeX (and Biber for the bibliography) in the project folder, without shell escape.")),
            );
            widgets::card(ui, |ui| {
                ui.set_width(ui.available_width());
                match (&self.running, self.history.last()) {
                    (Some(r), _) => {
                        ui.horizontal(|ui| {
                            widgets::chip(ui, Tone::Info, t("Running"));
                            ui.label(tf("Typesetting for {secs} s…", &[("secs", &format!("{:.0}", r.started.elapsed().as_secs_f32()))]));
                        });
                        ui.add_space(psg_brand::space::S);
                        widgets::progress(ui, None);
                    }
                    (None, Some(last)) => {
                        ui.horizontal(|ui| {
                            let (tone, label) = match last.outcome {
                                Outcome::Succeeded => (Tone::Success, t("Finished")),
                                Outcome::Failed => (Tone::Danger, t("Failed")),
                                Outcome::TimedOut => (Tone::Warning, t("Timed out")),
                                Outcome::Cancelled => (Tone::Neutral, t("Cancelled")),
                            };
                            widgets::chip(ui, tone, label);
                            ui.label(&last.summary);
                        });
                    }
                    (None, None) => {
                        widgets::chip(ui, Tone::Neutral, t("Idle"));
                        widgets::muted(ui, t("Nothing typeset in this session yet."));
                    }
                }
                ui.add_space(psg_brand::space::M);
                ui.horizontal(|ui| {
                    if let Some(cancel) = self.running.as_ref().map(|r| r.cancel.clone()) {
                        if widgets::button(ui, widgets::ButtonKind::Danger, t("Cancel")).clicked() {
                            cancel.store(true, std::sync::atomic::Ordering::Relaxed);
                            self.note(t("Cancelling…"));
                        }
                    } else if widgets::button_enabled(ui, self.can_compile(), widgets::ButtonKind::Primary, t("Auto Typeset"), self.why_not_compile())
                        .clicked()
                    {
                        self.start_compile(&ctx);
                    }
                    let (errors, warnings) = self.problem_counts();
                    if errors + warnings > 0 && widgets::ghost(ui, tf("Problems ({n})", &[("n", &(errors + warnings).to_string())])).clicked() {
                        self.page = super::Page::Problems;
                    }
                });
            });
            ui.add_space(psg_brand::space::M);
            if let Some(project) = &mut self.project {
                widgets::card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    widgets::section_header(ui, t("Settings for this project"));
                    let s = &mut project.file.settings;
                    widgets::form_grid(ui, "typeset-settings", |ui| {
                        widgets::form_label(ui, t("Engine"));
                        ui.horizontal(|ui| {
                            widgets::chip(ui, Tone::Info, "pdfLaTeX");
                            widgets::muted(ui, t("via latexmk, shell escape off"));
                        });
                        ui.end_row();
                        widgets::form_label(ui, t("Time limit"));
                        let (min, max) = (compile::MIN_TIMEOUT.as_secs(), compile::MAX_TIMEOUT.as_secs());
                        ui.add(egui::DragValue::new(&mut s.timeout_secs).range(min..=max).suffix(" s"));
                        ui.end_row();
                        ui.label("");
                        ui.checkbox(&mut s.force_rebuild, t("Rebuild everything each time"));
                        ui.end_row();
                    });
                });
                ui.add_space(psg_brand::space::M);
            }
            if !self.history.is_empty() {
                widgets::card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    widgets::section_header(ui, t("Runs in this session"));
                    for r in self.history.iter().rev().take(8) {
                        ui.horizontal(|ui| {
                            widgets::status_dot(
                                ui,
                                match r.outcome {
                                    Outcome::Succeeded => Tone::Success,
                                    Outcome::Failed => Tone::Danger,
                                    Outcome::TimedOut => Tone::Warning,
                                    Outcome::Cancelled => Tone::Neutral,
                                },
                            );
                            ui.label(egui::RichText::new(&r.at).monospace().small());
                            ui.label(&r.summary);
                        });
                    }
                });
                ui.add_space(psg_brand::space::M);
            }
            widgets::segmented(
                ui,
                &mut self.log_view,
                &[(LogView::Output, t("Compile output")), (LogView::TexLog, t("TeX log")), (LogView::Activity, t("Activity"))],
            );
            ui.add_space(psg_brand::space::XS);
            let text = match self.log_view {
                LogView::Output => self
                    .report
                    .as_ref()
                    .map(|r| format!("$ {}\n\n{}", r.command, r.output))
                    .unwrap_or_else(|| t("Typeset to see latexmk's output here.").into()),
                LogView::TexLog => self.report.as_ref().and_then(|r| r.tex_log.clone()).unwrap_or_else(|| t("No TeX log yet.").into()),
                LogView::Activity => self.activity.join("\n"),
            };
            let p = psg_desktop_ui::Palette::of(ui);
            egui::Frame::NONE
                .fill(p.field)
                .stroke(egui::Stroke::new(1.0, p.border))
                .corner_radius(egui::CornerRadius::same(psg_brand::radius::CONTROL))
                .inner_margin(egui::Margin::same(psg_brand::space::S as i8))
                .show(ui, |ui| {
                    ui.set_min_height(240.0);
                    egui::ScrollArea::both()
                        .id_salt(("log", self.log_view as u8))
                        .max_height(420.0)
                        .auto_shrink([false, false])
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(egui::RichText::new(text).monospace().size(12.0))
                                    .wrap_mode(egui::TextWrapMode::Extend)
                                    .selectable(true),
                            );
                        });
                });
        });
    }
}
