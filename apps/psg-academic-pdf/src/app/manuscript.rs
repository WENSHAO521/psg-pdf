//! Manuscript: the article itself. The source editor and the PDF side by side (the split is
//! dragged to any ratio and remembered), the article's outline, and the article details —
//! title, abstract and keywords, authors and affiliations, journal and dates — and the
//! bibliography.

use psg_academic::Author;
use psg_desktop_ui::{Tone, layout, widgets};

use super::{AcademicApp, ManuscriptTab, SourceView};
use crate::i18n::{t, tf};
use crate::preview::{MAX_PREVIEW_PAGES, PageSlot};

impl AcademicApp {
    pub(super) fn manuscript_page(&mut self, ui: &mut egui::Ui) {
        if self.project.is_none() {
            widgets::empty_state(ui, t("No manuscript open"), t("Import a manuscript or create a project from a template to start."), |ui| {
                if widgets::button_enabled(
                    ui,
                    self.env.is_some(),
                    widgets::ButtonKind::Primary,
                    t("Import manuscript…"),
                    t("Checking the TeX installation…"),
                )
                .clicked()
                {
                    self.import_new();
                }
                if widgets::secondary(ui, t("Open project…")).clicked() {
                    self.open_project();
                }
            });
            return;
        }
        ui.horizontal(|ui| {
            let refs = tf("References ({n})", &[("n", &self.bib.len().to_string())]);
            widgets::segmented(
                ui,
                &mut self.manuscript_tab,
                &[
                    (ManuscriptTab::Source, t("Source")),
                    (ManuscriptTab::Article, t("Title & abstract")),
                    (ManuscriptTab::Authors, t("Authors")),
                    (ManuscriptTab::Publication, t("Publication")),
                    (ManuscriptTab::References, &refs),
                ],
            );
        });
        ui.add_space(psg_brand::space::S);
        match self.manuscript_tab {
            ManuscriptTab::Source => self.source_area(ui),
            ManuscriptTab::Article => self.form_page(ui, Self::article_form),
            ManuscriptTab::Authors => self.form_page(ui, Self::authors_form),
            ManuscriptTab::Publication => self.form_page(ui, Self::publication_form),
            ManuscriptTab::References => self.references(ui),
        }
    }

    /// A scrolling column of forms, with the metadata's problems above them.
    fn form_page(&mut self, ui: &mut egui::Ui, form: fn(&mut Self, &mut egui::Ui)) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.set_max_width(ui.available_width().min(820.0));
            if let Some(project) = &self.project {
                for issue in project.file.metadata.validate() {
                    let tone = if issue.blocking { Tone::Danger } else { Tone::Warning };
                    widgets::banner(ui, tone, &format!("{}: {}", t(issue.field), issue.message), None);
                    ui.add_space(psg_brand::space::XS);
                }
            }
            form(self, ui);
            ui.add_space(psg_brand::space::S);
            widgets::muted(
                ui,
                tf("Saved with the project ({keys}) and before every typesetting run.", &[("keys", &psg_desktop_ui::keys::command("S", false))]),
            );
        });
    }

    fn article_form(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &mut self.project else { return };
        let m = &mut project.file.metadata;
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::section_header(ui, t("Article"));
            widgets::form_grid(ui, "article-grid", |ui| {
                for (label, value) in [("Type", &mut m.article_type), ("Title", &mut m.title), ("Short title", &mut m.short_title)] {
                    widgets::form_label(ui, t(label));
                    ui.add(widgets::text_field(value, ""));
                    ui.end_row();
                }
                widgets::form_label(ui, t("Keywords"));
                ui.add(widgets::text_field(&mut self.buffers.keywords, t("Separate keywords with ; or ,")));
                ui.end_row();
            });
            ui.add_space(psg_brand::space::S);
            widgets::form_label(ui, t("Abstract"));
            ui.add(egui::TextEdit::multiline(&mut m.abstract_text).desired_rows(10).desired_width(f32::INFINITY));
            let words = m.abstract_text.split_whitespace().count();
            widgets::muted(ui, tf("{n} words", &[("n", &words.to_string())]));
        });
    }

    fn authors_form(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &mut self.project else { return };
        let m = &mut project.file.metadata;
        let mut remove = None;
        self.buffers.author_affiliations.resize(m.authors.len(), String::new());
        for (i, (author, aff)) in m.authors.iter_mut().zip(self.buffers.author_affiliations.iter_mut()).enumerate() {
            ui.push_id(("author", i), |ui| {
                widgets::card(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        widgets::overline(ui, &tf("Author {n}", &[("n", &(i + 1).to_string())]));
                        if author.corresponding {
                            widgets::chip(ui, Tone::Info, t("Corresponding author"));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if widgets::button(ui, widgets::ButtonKind::Ghost, t("Remove")).clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                    widgets::form_grid(ui, "author", |ui| {
                        widgets::form_label(ui, t("Name"));
                        ui.add(widgets::text_field(&mut author.name, ""));
                        ui.end_row();
                        widgets::form_label(ui, t("Affiliations"));
                        ui.add(widgets::text_field(aff, t("Affiliation numbers, e.g. 1, 2")));
                        ui.end_row();
                        widgets::form_label(ui, t("ORCID iD"));
                        ui.add(widgets::text_field(&mut author.orcid, "0000-0000-0000-0000"));
                        ui.end_row();
                        widgets::form_label(ui, t("Email"));
                        ui.add(widgets::text_field(&mut author.email, ""));
                        ui.end_row();
                        ui.label("");
                        ui.checkbox(&mut author.corresponding, t("Corresponding author"));
                        ui.end_row();
                    });
                });
                ui.add_space(psg_brand::space::S);
            });
        }
        if let Some(i) = remove {
            m.authors.remove(i);
            self.buffers.author_affiliations.remove(i);
        }
        if widgets::secondary(ui, t("Add author")).clicked() {
            m.authors.push(Author::default());
            self.buffers.author_affiliations.push(String::new());
        }
        ui.add_space(psg_brand::space::L);
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::section_header(ui, t("Affiliations"));
            widgets::muted(ui, t("Authors refer to these by number."));
            let mut remove = None;
            for (i, a) in m.affiliations.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{}", i + 1)).strong());
                    ui.add(egui::TextEdit::singleline(a).desired_width(ui.available_width() - 40.0));
                    if ui.small_button("×").on_hover_text(t("Remove")).clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                m.affiliations.remove(i);
            }
            if widgets::secondary(ui, t("Add affiliation")).clicked() {
                m.affiliations.push(String::new());
            }
        });
    }

    fn publication_form(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &mut self.project else { return };
        let m = &mut project.file.metadata;
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::section_header(ui, t("Journal and issue"));
            widgets::form_grid(ui, "journal-grid", |ui| {
                for (label, value) in [
                    ("Journal", &mut m.journal),
                    ("Abbreviation", &mut m.journal_abbrev),
                    ("ISSN", &mut m.issn),
                    ("Volume", &mut m.volume),
                    ("Issue", &mut m.issue),
                    ("Year", &mut m.year),
                    ("Pages", &mut m.pages),
                    ("Article no.", &mut m.article_number),
                    ("DOI", &mut m.doi),
                ] {
                    widgets::form_label(ui, t(label));
                    ui.add(widgets::text_field(value, ""));
                    ui.end_row();
                }
            });
        });
        ui.add_space(psg_brand::space::L);
        widgets::card(ui, |ui| {
            ui.set_width(ui.available_width());
            widgets::section_header(ui, t("Dates and licence"));
            widgets::form_grid(ui, "dates-grid", |ui| {
                for (label, value) in
                    [("Received", &mut m.received), ("Revised", &mut m.revised), ("Accepted", &mut m.accepted), ("Published", &mut m.published)]
                {
                    widgets::form_label(ui, t(label));
                    ui.add(widgets::text_field(value, "YYYY-MM-DD"));
                    ui.end_row();
                }
                widgets::form_label(ui, t("Copyright"));
                ui.add(widgets::text_field(&mut m.copyright, ""));
                ui.end_row();
            });
            ui.add_space(psg_brand::space::S);
            widgets::form_label(ui, t("Licence statement"));
            ui.add(egui::TextEdit::multiline(&mut m.licence).desired_rows(3).desired_width(f32::INFINITY));
        });
    }

    /// The source editor, the outline and the PDF, as [`SourceView`] says.
    fn source_area(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let can_save = self.source.dirty();
            if widgets::button_enabled(ui, can_save, widgets::ButtonKind::Secondary, t("Save"), "")
                .on_hover_text(tf("Save the file ({keys})", &[("keys", &psg_desktop_ui::keys::command("S", false))]))
                .clicked()
            {
                self.save_source();
            }
            if widgets::ghost(ui, t("Reload from disk")).on_hover_text(t("Discard unsaved changes and read the file again")).clicked()
                && let Some(project) = &self.project
            {
                let (dir, file) = (project.dir.clone(), self.source.file.clone());
                self.source.load(&dir, &file);
            }
            ui.checkbox(&mut self.show_outline, t("Outline"));
            if self.source.dirty() {
                widgets::chip(ui, Tone::Warning, t("Unsaved"));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let before = self.source_view;
                widgets::segmented(
                    ui,
                    &mut self.source_view,
                    &[(SourceView::Preview, t("PDF")), (SourceView::Split, t("Split")), (SourceView::Editor, t("Editor"))],
                );
                if before != self.source_view && self.source_view != SourceView::Editor && self.preview.pages.is_empty() && self.has_pdf() {
                    let ctx = ui.ctx().clone();
                    self.load_preview(&ctx);
                }
            });
        });
        ui.add_space(psg_brand::space::XS);
        let files = self.editable_files();
        let file = self.source.file.clone();
        let marked: Vec<u32> = self
            .preflight
            .iter()
            .chain(self.report.iter().flat_map(|r| r.diagnostics.iter()))
            .filter(|d| d.file.as_deref().is_some_and(|f| f.trim_start_matches("./") == file))
            .filter_map(|d| d.line)
            .collect();
        let mut picked = None;
        let mut jump = None;
        let body_file = self.project.as_ref().map(|p| p.file.body.clone()).unwrap_or_default();
        if self.show_outline && self.source_view != SourceView::Preview && file == body_file {
            let headings = crate::outline::headings(&self.source.text);
            egui::Panel::left("outline").resizable(true).default_size(200.0).size_range(140.0..=360.0).show(ui, |ui| {
                layout::pane_header(ui, t("Outline"), |_| {});
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if headings.is_empty() {
                        widgets::muted(ui, t("No \\section headings yet."));
                    }
                    for h in &headings {
                        ui.horizontal(|ui| {
                            ui.add_space(f32::from(h.level.saturating_sub(1)) * psg_brand::space::M);
                            let text = if h.level == 1 { egui::RichText::new(&h.title).strong() } else { egui::RichText::new(&h.title) };
                            if ui
                                .add(egui::Button::new(text).frame(false).truncate())
                                .on_hover_text(tf("Line {n}", &[("n", &h.line.to_string())]))
                                .clicked()
                            {
                                jump = Some(h.line);
                            }
                        });
                    }
                });
            });
        }
        match self.source_view {
            SourceView::Editor => picked = self.source.show(ui, &files, &marked),
            SourceView::Preview => self.preview_pane(ui),
            SourceView::Split => {
                let mut ratio = self.split_ratio;
                let mut chosen = None;
                layout::split(
                    ui,
                    "source-split",
                    &mut ratio,
                    0.2,
                    self,
                    |ui, app| chosen = app.source.show(ui, &files, &marked),
                    |ui, app| app.preview_pane(ui),
                );
                self.split_ratio = ratio;
                picked = chosen;
            }
        }
        if let Some(name) = picked {
            self.go_to(&name, None);
        }
        if let Some(line) = jump {
            self.source.go_to(line);
        }
    }

    fn references(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else { return };
        if !project.file.uses_biber {
            widgets::banner(ui, Tone::Neutral, t("This template doesn't use a bibliography file."), None);
            ui.add_space(psg_brand::space::S);
        }
        ui.horizontal(|ui| {
            ui.add(widgets::text_field(&mut self.bib_filter, t("Filter by key, author or title")).desired_width(320.0));
            if widgets::secondary(ui, t("Check again")).clicked() {
                self.refresh_checks();
            }
            if widgets::ghost(ui, t("Add bibliography file…")).clicked() {
                self.add_files();
            }
        });
        ui.add_space(psg_brand::space::S);
        let filter = self.bib_filter.to_lowercase();
        let mut go = None;
        if self.bib.is_empty() {
            widgets::muted(ui, t("No bibliography entries yet."));
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("bib-grid").num_columns(3).striped(true).spacing(egui::vec2(psg_brand::space::L, psg_brand::space::S)).show(ui, |ui| {
                ui.strong(t("Key"));
                ui.strong(t("Type"));
                ui.strong(t("Entry"));
                ui.end_row();
                for (file, e) in &self.bib {
                    let summary = e.summary();
                    if !filter.is_empty() && !e.key.to_lowercase().contains(&filter) && !summary.to_lowercase().contains(&filter) {
                        continue;
                    }
                    if ui.link(&e.key).on_hover_text(format!("{file}:{}", e.line)).clicked() {
                        go = Some((file.clone(), e.line));
                    }
                    widgets::chip(ui, Tone::Neutral, e.kind.clone());
                    ui.label(summary);
                    ui.end_row();
                }
            });
        });
        if let Some((file, line)) = go {
            self.go_to(&file, Some(line));
        }
    }

    /// The typeset PDF on the canvas colour, page by page.
    pub(super) fn preview_pane(&mut self, ui: &mut egui::Ui) {
        let p = psg_desktop_ui::Palette::of(ui);
        egui::Frame::NONE
            .fill(p.canvas)
            .corner_radius(egui::CornerRadius::same(psg_brand::radius::CARD))
            .inner_margin(egui::Margin::same(psg_brand::space::S as i8))
            .show(ui, |ui| {
                ui.set_min_size(ui.available_size());
                // A narrow pane keeps the controls and drops the title, so nothing overlaps.
                let roomy = ui.available_width() >= 420.0;
                ui.horizontal(|ui| {
                    if roomy {
                        ui.label(egui::RichText::new(t("PDF preview")).strong());
                    }
                    if roomy && self.preview.total_pages > 0 {
                        widgets::muted(ui, tf("{n} pages", &[("n", &self.preview.total_pages.to_string())]));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let ctx = ui.ctx().clone();
                        if ui.add_enabled(!self.preview.rendering && self.has_pdf(), egui::Button::new(t("Redraw"))).clicked() {
                            self.load_preview(&ctx);
                        }
                        ui.checkbox(&mut self.fit_width, t("Fit width"));
                        let before = self.zoom;
                        ui.add(egui::Slider::new(&mut self.zoom, 0.5..=3.0).step_by(0.25).show_value(false));
                        ui.label(format!("{:.0}%", self.zoom * 100.0));
                        if (before - self.zoom).abs() > f32::EPSILON && self.has_pdf() {
                            self.load_preview(&ctx);
                        }
                    });
                });
                if let Some(e) = &self.preview.error {
                    widgets::banner(ui, Tone::Danger, e, None);
                }
                if self.preview.pages.is_empty() {
                    if self.preview.rendering {
                        ui.centered_and_justified(|ui| ui.spinner());
                    } else {
                        widgets::empty_state(ui, t("No PDF yet"), t("Press Auto Typeset to typeset the manuscript; the PDF appears here."), |_| {});
                    }
                    return;
                }
                if self.preview.total_pages > MAX_PREVIEW_PAGES {
                    widgets::muted(
                        ui,
                        tf(
                            "Showing the first {shown} of {total} pages.",
                            &[("shown", &MAX_PREVIEW_PAGES.to_string()), ("total", &self.preview.total_pages.to_string())],
                        ),
                    );
                }
                let ppp = ui.ctx().pixels_per_point();
                let fit = self.fit_width;
                egui::ScrollArea::both().id_salt("preview-scroll").auto_shrink([false, false]).show(ui, |ui| {
                    // Fit width: a page wider than the pane is drawn at the pane's width, so a
                    // narrow split shows whole lines instead of a page cut off on both sides.
                    let room = (ui.available_width() - 4.0).max(80.0);
                    ui.vertical_centered(|ui| {
                        for (i, page) in self.preview.pages.iter().enumerate() {
                            match page {
                                PageSlot::Ready(texture, size) => {
                                    let natural = *size / ppp;
                                    let shown = if fit && natural.x > room { natural * (room / natural.x) } else { natural };
                                    let resp = ui.add(egui::Image::new((texture.id(), shown)));
                                    ui.painter().rect_stroke(resp.rect, 0, egui::Stroke::new(1.0, p.border), egui::StrokeKind::Outside);
                                }
                                PageSlot::Pending => {
                                    ui.add_sized([200.0, 280.0], egui::Spinner::new());
                                }
                                PageSlot::Failed(e) => {
                                    ui.colored_label(
                                        ui.visuals().error_fg_color,
                                        tf("Page {n}: {error}", &[("n", &(i + 1).to_string()), ("error", e)]),
                                    );
                                }
                            }
                            ui.add_space(psg_brand::space::M);
                        }
                    });
                });
            });
    }
}
