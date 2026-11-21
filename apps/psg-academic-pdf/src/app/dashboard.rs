//! Dashboard: where a session starts. Without a project it offers the ways in (a new project
//! from a template, an existing project, a manuscript to import) and the recent projects; with
//! one it summarises the project, its last run and its problems. The TeX installation is always
//! shown, since nothing typesets without it.

use std::fmt::Write as _;

use psg_academic::tex::TexEnvironment;
use psg_desktop_ui::{Tone, widgets};

use super::{AcademicApp, Page};
use crate::i18n::{t, tf, tn};
use crate::tex_setup::{Job, Progress};

impl AcademicApp {
    pub(super) fn dashboard_page(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(980.0));
            widgets::page_header(ui, t("Dashboard"), Some(t("Typeset accepted articles into publication-ready PDFs with your journal's template.")));
            if self.project.is_some() {
                self.project_summary(ui);
            } else {
                self.start_card(ui);
            }
            ui.add_space(psg_brand::space::L);
            // Side by side when there is room, stacked on a narrow window.
            if ui.available_width() >= 900.0 {
                ui.columns(2, |cols| {
                    if let [left, right] = cols {
                        self.recent_card(left);
                        self.tex_card(right);
                    }
                });
            } else {
                self.recent_card(ui);
                ui.add_space(psg_brand::space::L);
                self.tex_card(ui);
            }
        });
    }

    /// The ways to start: from a template, an existing project, or a manuscript.
    fn start_card(&mut self, ui: &mut egui::Ui) {
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::section_header(ui, t("Start a project"));
            let template = self.templates.get(self.selected_template).map(|tpl| tpl.manifest.name.clone()).unwrap_or_else(|| t("(none)").into());
            widgets::muted(ui, tf("New projects use the {template} template. Choose another in the Template Library.", &[("template", &template)]));
            ui.horizontal_wrapped(|ui| {
                ui.label(tf("Workspace: {path}", &[("path", &self.workspace.display().to_string())]));
                if widgets::ghost(ui, t("Choose workspace…")).clicked() {
                    self.choose_workspace();
                }
            });
            ui.add_space(psg_brand::space::M);
            ui.horizontal_wrapped(|ui| {
                let ready = self.env.is_some() && !self.templates.is_empty();
                if widgets::button_enabled(ui, ready, widgets::ButtonKind::Primary, t("Import manuscript…"), t("Checking the TeX installation…"))
                    .on_hover_text(t("A .tex or .docx file becomes the body of a new project"))
                    .clicked()
                {
                    self.import_new();
                }
                if widgets::button_enabled(
                    ui,
                    !self.templates.is_empty(),
                    widgets::ButtonKind::Secondary,
                    t("New project"),
                    t("No template is available."),
                )
                .on_hover_text(tf("Create the project in {path}", &[("path", &self.workspace.display().to_string())]))
                .clicked()
                {
                    self.new_project();
                }
                if widgets::secondary(ui, t("New in folder…")).clicked() {
                    self.new_project_in_folder();
                }
                if widgets::secondary(ui, t("Open project…")).clicked() {
                    self.open_project();
                }
                if widgets::ghost(ui, t("Template Library")).clicked() {
                    self.page = Page::Templates;
                }
            });
        });
    }

    /// The open project at a glance.
    fn project_summary(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else { return };
        let m = &project.file.metadata;
        let title = if m.title.trim().is_empty() { t("Untitled article").to_owned() } else { m.title.clone() };
        let authors = m.authors.iter().map(|a| a.name.trim()).filter(|n| !n.is_empty()).collect::<Vec<_>>().join(", ");
        let journal = [m.journal.trim(), m.volume.trim(), m.issue.trim(), m.year.trim()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" · ");
        let template = self
            .templates
            .iter()
            .find(|tpl| tpl.manifest.id == project.file.template)
            .map_or(project.file.template.clone(), |tpl| tpl.manifest.name.clone());
        let dir = project.dir.display().to_string();
        let (errors, warnings) = self.problem_counts();
        let last = self.history.last().map(|r| (r.outcome, r.summary.clone()));
        let mut go = None;
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::overline(ui, t("Current project"));
            ui.label(egui::RichText::new(title).font(psg_desktop_ui::fonts::semibold(psg_brand::text::TITLE)));
            if !authors.is_empty() {
                ui.label(authors);
            }
            if !journal.is_empty() {
                widgets::muted(ui, journal);
            }
            ui.add_space(psg_brand::space::S);
            ui.horizontal_wrapped(|ui| {
                widgets::chip(ui, Tone::Neutral, tf("Template: {template}", &[("template", &template)]));
                widgets::chip(ui, Tone::Neutral, t("pdfLaTeX"));
                match last {
                    Some((psg_academic::Outcome::Succeeded, s)) => widgets::chip(ui, Tone::Success, s),
                    Some((_, s)) => widgets::chip(ui, Tone::Danger, s),
                    None => widgets::chip(ui, Tone::Neutral, t("Not typeset in this session")),
                };
                if errors > 0 {
                    widgets::chip(ui, Tone::Danger, tn(errors, "{n} error", "{n} errors", &[]));
                }
                if warnings > 0 {
                    widgets::chip(ui, Tone::Warning, tn(warnings, "{n} warning", "{n} warnings", &[]));
                }
            });
            ui.add(egui::Label::new(egui::RichText::new(dir).small().weak()).truncate())
                .on_hover_text(t("The project folder: compilation happens here and only here"));
            ui.add_space(psg_brand::space::M);
            ui.horizontal_wrapped(|ui| {
                if widgets::secondary(ui, t("Edit manuscript")).clicked() {
                    self.manuscript_tab = super::ManuscriptTab::Source;
                    go = Some(Page::Manuscript);
                }
                if widgets::secondary(ui, t("Article details")).clicked() {
                    self.manuscript_tab = super::ManuscriptTab::Article;
                    go = Some(Page::Manuscript);
                }
                if errors + warnings > 0 && widgets::secondary(ui, t("Review problems")).clicked() {
                    go = Some(Page::Problems);
                }
                if widgets::ghost(ui, t("Open another project…")).clicked() {
                    self.open_project();
                }
            });
        });
        if let Some(page) = go {
            self.page = page;
        }
    }

    fn recent_card(&mut self, ui: &mut egui::Ui) {
        let mut open = None;
        let mut forget = None;
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::section_header(ui, t("Recent projects"));
            if self.recent.items().is_empty() {
                widgets::muted(ui, t("Projects you open or create appear here."));
            }
            let current = self.project.as_ref().map(|p| p.dir.clone());
            for (i, path) in self.recent.items().iter().enumerate() {
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
                let here = current.as_deref() == Some(path.as_path());
                let missing = !path.is_dir();
                ui.horizontal(|ui| {
                    let label = if here { tf("{name} (open)", &[("name", &name)]) } else { name };
                    let resp = ui.add_enabled(!here && !missing, egui::Button::new(label).frame(false)).on_hover_text(path.display().to_string());
                    if missing {
                        widgets::chip(ui, Tone::Warning, t("Missing"));
                    }
                    if resp.clicked() {
                        open = Some(path.clone());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("×").on_hover_text(t("Remove from the list (the folder is kept)")).clicked() {
                            forget = Some(i);
                        }
                    });
                });
            }
        });
        if let Some(i) = forget {
            self.recent.remove(i);
        }
        if let Some(path) = open {
            self.pending.push(path);
        }
    }

    fn tex_card(&mut self, ui: &mut egui::Ui) {
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::section_header(ui, t("TeX installation"));
            self.tex_status(ui);
        });
    }

    /// Where TeX comes from, the tools found and their versions, the install or update under
    /// way, and Check again / Update / Install.
    pub(super) fn tex_status(&mut self, ui: &mut egui::Ui) {
        self.tex_source(ui);
        match &self.env {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t("Checking…"));
                });
            }
            Some(env) => {
                widgets::form_grid(ui, "tex-grid", |ui| {
                    for tool in [&env.pdflatex, &env.latexmk, &env.biber, &env.pandoc] {
                        ui.horizontal(|ui| {
                            let tone = if tool.found() {
                                Tone::Success
                            } else if tool.probe_error.is_some() {
                                Tone::Danger
                            } else if tool.name == "pandoc" {
                                Tone::Neutral
                            } else if self.tex_job.is_some() {
                                Tone::Info
                            } else {
                                Tone::Danger
                            };
                            widgets::status_dot(ui, tone);
                            ui.label(tool.name);
                        });
                        match (&tool.path, &tool.probe_error) {
                            (Some(path), Some(error)) => {
                                let details = format!("{}\n{error}", path.display());
                                let response = ui
                                    .add(
                                        egui::Label::new(
                                            egui::RichText::new(error.lines().next().unwrap_or(error)).color(ui.visuals().error_fg_color),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(&details);
                                response.context_menu(|ui| {
                                    if ui.button(t("Copy")).clicked() {
                                        ui.copy_text(details);
                                        ui.close();
                                    }
                                });
                                response
                            }
                            // Version lines can be long; they end in "…" rather than widening the card.
                            (Some(p), None) => ui
                                .add(egui::Label::new(tool.version.clone().unwrap_or_else(|| t("found").into())).truncate())
                                .on_hover_text(p.display().to_string()),
                            (None, _) if tool.name == "pandoc" => widgets::muted(ui, t("not found (optional, for DOCX import)")),
                            (None, _) if self.tex_job.is_some() => widgets::muted(ui, t("being installed…")),
                            (None, _) => ui.colored_label(ui.visuals().error_fg_color, t("not found")),
                        };
                        ui.end_row();
                    }
                });
            }
        }
        if let Some(job) = &self.tex_job {
            ui.add_space(psg_brand::space::S);
            let (text, fraction) = match job.progress {
                Progress::Looking => (t("Looking for the newest TeX…").to_owned(), None),
                Progress::Downloading { done, total } => {
                    let mb = |b: u64| format!("{:.0}", b as f64 / 1_048_576.0);
                    let fraction = (total > 0).then(|| (done as f64 / total as f64).clamp(0.0, 1.0) as f32);
                    (tf("Downloading TeX: {done} of {total} MB", &[("done", &mb(done)), ("total", &mb(total))]), fraction)
                }
                Progress::Unpacking => (t("Unpacking TeX…").to_owned(), None),
                Progress::Packages => {
                    (if job.job == Job::RepairBiber { t("Repairing Biber…") } else { t("Adding the packages the templates use…") }.to_owned(), None)
                }
                Progress::Updating => (t("Updating TeX…").to_owned(), None),
            };
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(text);
            });
            if let Some(fraction) = fraction {
                ui.add(egui::ProgressBar::new(fraction).show_percentage());
            }
        } else if let Some(error) = &self.tex_error {
            ui.add_space(psg_brand::space::S);
            ui.colored_label(ui.visuals().error_fg_color, error.as_str());
        }
        ui.add_space(psg_brand::space::S);
        ui.horizontal_wrapped(|ui| {
            let idle = self.env_rx.is_none() && self.tex_job.is_none();
            if widgets::button_enabled(ui, idle, widgets::ButtonKind::Secondary, t("Check again"), "").clicked() {
                let ctx = ui.ctx().clone();
                self.env = None;
                self.detect_tex(&ctx);
            }
            if tex_diagnostics_button(ui, self.env.as_ref(), self.tex_error.as_deref()).clicked() {
                self.notices.push(Tone::Success, t("TeX diagnostics copied."), ui.input(|i| i.time));
            }
            let Some(tex) = &self.managed else { return };
            let installed = tex.installed();
            let free = idle && self.running.is_none();
            if installed {
                let hint = t("Wait for the current task to finish.");
                if widgets::button_enabled(ui, free, widgets::ButtonKind::Secondary, t("Update TeX now"), hint).clicked() {
                    self.start_tex_job(ui.ctx(), Job::Update);
                }
                if self.biber_repair_needed(self.env.as_ref())
                    && widgets::button_enabled(ui, free, widgets::ButtonKind::Secondary, t("Repair Biber"), hint).clicked()
                {
                    self.start_tex_job(ui.ctx(), Job::RepairBiber);
                }
            } else if widgets::button_enabled(ui, free, widgets::ButtonKind::Primary, t("Install TeX"), t("Wait for the current task to finish."))
                .clicked()
            {
                let ctx = ui.ctx().clone();
                self.start_tex_job(&ctx, Job::Install);
            }
        });
    }

    /// One line on where TeX comes from: PSG's own (which release, when updated), or the one
    /// found on this computer while PSG's is not installed yet.
    fn tex_source(&self, ui: &mut egui::Ui) {
        let Some(tex) = &self.managed else { return };
        let text = if self.env.as_ref().is_some_and(|e| e.managed) {
            let state = tex.state();
            let zone = jiff::tz::TimeZone::system();
            let date = |s: &str| s.parse::<jiff::Timestamp>().ok().map(|t| t.to_zoned_datetime(zone.clone()).date().to_string());
            let updated = date(&state.updated_at).or_else(|| date(&state.installed_at)).unwrap_or_else(|| "?".into());
            tf(
                "Installed by PSG Academic PDF (TinyTeX {release}), updated {date}. Updates itself weekly.",
                &[("release", &state.release), ("date", &updated)],
            )
        } else if self.env.as_ref().is_some_and(|e| e.pdflatex.found()) {
            t("Using the TeX found on this computer until PSG Academic PDF has installed its own.").to_owned()
        } else {
            t("PSG Academic PDF installs TeX by itself; nothing needs to be installed by hand.").to_owned()
        };
        widgets::muted(ui, text);
        ui.add_space(psg_brand::space::XS);
    }
}

fn tex_diagnostics_button(ui: &mut egui::Ui, env: Option<&TexEnvironment>, setup_error: Option<&str>) -> egui::Response {
    let button = widgets::button_enabled(ui, env.is_some() || setup_error.is_some(), widgets::ButtonKind::Secondary, t("Copy TeX diagnostics"), "");
    if button.clicked() {
        let mut report = String::new();
        if let Some(env) = env {
            for tool in [&env.pdflatex, &env.latexmk, &env.biber, &env.pandoc] {
                let status = tool
                    .probe_error
                    .as_deref()
                    .or(tool.version.as_deref())
                    .unwrap_or_else(|| if tool.path.is_some() { t("found") } else { t("not found") });
                let _ = writeln!(report, "{}: {status}", tool.name);
                if let Some(path) = &tool.path {
                    let _ = writeln!(report, "  {}", path.display());
                }
            }
        }
        if let Some(error) = setup_error {
            let _ = writeln!(report, "{error}");
        }
        ui.copy_text(report);
    }
    button
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_button_sends_the_full_tex_diagnostic_to_the_clipboard() {
        let ctx = egui::Context::default();
        let mut env = TexEnvironment::detect_in(None, false);
        env.biber.path = Some("biber.exe".into());
        env.biber.probe_error = Some("Version check timed out\nBiber did not finish starting".into());
        let mut pos = egui::Pos2::ZERO;
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            pos = tex_diagnostics_button(ui, Some(&env), Some("Update failed")).rect.center();
        });
        output.drop_without_applying_deltas();
        for pressed in [true, false] {
            let event = egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            let input = egui::RawInput { events: vec![egui::Event::PointerMoved(pos), event], ..Default::default() };
            let output = ctx.run_ui(input, |ui| {
                tex_diagnostics_button(ui, Some(&env), Some("Update failed"));
            });
            if !pressed {
                assert!(output.platform_output.commands.iter().any(|command| {
                    matches!(command, egui::OutputCommand::CopyText(text)
                        if text.contains("biber: Version check timed out\nBiber did not finish starting\n  biber.exe\n")
                            && text.contains("Update failed"))
                }));
            }
            output.drop_without_applying_deltas();
        }
    }
}
