//! The PSG Academic PDF window: project and template on the left, metadata and settings in the
//! side panel, and the PDF preview, problems and logs in the middle.
//!
//! The workspace's disallowed egui checkbox and combo box (clippy.toml) are about PSG PDF's own
//! theme, which draws a checkbox as a circle, and about combo boxes whose contents change while
//! open. This window uses egui's default visuals, where a checkbox is square, and its template
//! list doesn't change while the menu is open, so egui's own widgets are used here.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use psg_academic::pandoc::{ManuscriptImporter, PandocImporter};
use psg_academic::{Author, CompileReport, Outcome, Project, PublicationMetadata, Severity, Template, TexEnvironment, compile, template};

use crate::preview::{MAX_PREVIEW_PAGES, PageSlot, Preview};

/// The program's display name.
pub const APP_NAME: &str = "PSG Academic PDF";
/// The organisation that publishes it.
pub const PUBLISHER: &str = "Panorama Scholarly Group";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Preview,
    Problems,
    Output,
    TexLog,
    Activity,
}

struct Running {
    rx: Receiver<Result<CompileReport, String>>,
    cancel: Arc<AtomicBool>,
    started: Instant,
}

/// Text-field buffers for the list fields, applied to the metadata before saving.
#[derive(Default)]
struct Buffers {
    keywords: String,
    author_affiliations: Vec<String>,
}

pub struct AcademicApp {
    env: Option<TexEnvironment>,
    env_rx: Option<Receiver<TexEnvironment>>,
    templates: Vec<Template>,
    template_problems: Vec<String>,
    selected_template: usize,
    workspace: PathBuf,
    project: Option<Project>,
    buffers: Buffers,
    running: Option<Running>,
    report: Option<CompileReport>,
    preview: Preview,
    zoom: f32,
    tab: Tab,
    status: String,
    activity: Vec<String>,
}

impl AcademicApp {
    pub fn new(ctx: &egui::Context) -> Self {
        let (templates, template_problems) = template::discover(&template::default_template_dirs());
        let workspace = eframe::storage_dir(APP_NAME)
            .map(|d| d.join("projects"))
            .unwrap_or_else(|| std::env::temp_dir().join("psg-academic-pdf").join("projects"));
        let mut app = Self {
            env: None,
            env_rx: None,
            templates,
            template_problems,
            selected_template: 0,
            workspace,
            project: None,
            buffers: Buffers::default(),
            running: None,
            report: None,
            preview: Preview::default(),
            zoom: 1.0,
            tab: Tab::Preview,
            status: "Checking the TeX installation…".into(),
            activity: Vec::new(),
        };
        for p in app.template_problems.clone() {
            app.note(format!("Template skipped: {p}"));
        }
        app.detect_tex(ctx);
        app
    }

    fn note(&mut self, text: impl Into<String>) {
        let text = text.into();
        log::info!("{text}");
        let stamp = chrono::Local::now().format("%H:%M:%S");
        self.activity.push(format!("[{stamp}] {text}"));
        if self.activity.len() > 2000 {
            self.activity.drain(..500);
        }
    }

    fn fail(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.status = text.clone();
        self.note(text);
    }

    fn detect_tex(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("psg-tex-detect".into()).spawn(move || {
            let _ = tx.send(TexEnvironment::detect());
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => self.env_rx = Some(rx),
            Err(e) => self.fail(format!("Could not check the TeX installation: {e}")),
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.env_rx
            && let Ok(env) = rx.try_recv()
        {
            self.env_rx = None;
            let missing = env.missing(true);
            if missing.is_empty() {
                self.status = "TeX is ready.".into();
                self.note(format!(
                    "TeX found: pdflatex {}, latexmk {}, biber {}{}",
                    env.pdflatex.version.as_deref().unwrap_or("?"),
                    env.latexmk.version.as_deref().unwrap_or("?"),
                    env.biber.version.as_deref().unwrap_or("?"),
                    if env.pandoc.found() { ", pandoc found" } else { "" }
                ));
            } else {
                self.fail(format!(
                    "TeX is not ready: {} not found. Install TeX Live with latexmk and biber, then press Check again.",
                    missing.join(", ")
                ));
            }
            self.env = Some(env);
        }
        if let Some(running) = &self.running
            && let Ok(result) = running.rx.try_recv()
        {
            self.running = None;
            match result {
                Ok(report) => self.finished(report, ctx),
                Err(e) => self.fail(e),
            }
        }
        self.preview.poll(ctx);
    }

    fn finished(&mut self, report: CompileReport, ctx: &egui::Context) {
        self.status = report.summary();
        self.note(report.summary());
        match report.outcome {
            Outcome::Succeeded => {
                self.tab = Tab::Preview;
                self.load_preview(ctx);
            }
            Outcome::Failed => self.tab = Tab::Problems,
            Outcome::TimedOut | Outcome::Cancelled => self.tab = Tab::Output,
        }
        self.report = Some(report);
    }

    fn load_preview(&mut self, ctx: &egui::Context) {
        let Some(project) = &self.project else { return };
        let pdf = project.pdf_path();
        match std::fs::read(&pdf) {
            Ok(bytes) => self.preview.load(bytes, self.zoom * ctx.pixels_per_point(), ctx),
            Err(e) => self.fail(format!("Could not read {}: {e}", pdf.display())),
        }
    }

    fn set_project(&mut self, project: Project) {
        self.buffers.keywords = project.file.metadata.keywords.join("; ");
        self.buffers.author_affiliations = project.file.metadata.authors.iter().map(|a| join_numbers(&a.affiliations)).collect();
        self.note(format!("Project: {} (template {})", project.dir.display(), project.file.template));
        self.project = Some(project);
        self.report = None;
        self.preview.clear();
    }

    /// Copy the text buffers into the metadata.
    fn apply_buffers(&mut self) {
        let Some(project) = &mut self.project else { return };
        let m = &mut project.file.metadata;
        m.keywords = self.buffers.keywords.split([';', ',']).map(str::trim).filter(|k| !k.is_empty()).map(str::to_owned).collect();
        for (author, text) in m.authors.iter_mut().zip(&self.buffers.author_affiliations) {
            author.affiliations = text.split([',', ' ']).filter_map(|n| n.trim().parse().ok()).collect();
        }
    }

    fn new_project(&mut self) {
        let Some(t) = self.templates.get(self.selected_template).cloned() else {
            self.fail("No template is available.");
            return;
        };
        let name = format!("{}-{}", t.manifest.id, chrono::Local::now().format("%Y%m%d-%H%M%S"));
        let dir = self.workspace.join(name);
        match Project::create(&dir, &t, PublicationMetadata::sample()) {
            Ok(p) => {
                self.status = format!("New project from {} in {}.", t.manifest.name, p.dir.display());
                self.set_project(p);
            }
            Err(e) => self.fail(format!("Could not create the project: {e}")),
        }
    }

    fn new_project_in_folder(&mut self) {
        let Some(t) = self.templates.get(self.selected_template).cloned() else { return };
        let Some(dir) = rfd::FileDialog::new().set_title("Choose an empty folder for the project").pick_folder() else { return };
        match Project::create(&dir, &t, PublicationMetadata::sample()) {
            Ok(p) => self.set_project(p),
            Err(e) => self.fail(format!("Could not create the project: {e}")),
        }
    }

    fn open_project(&mut self) {
        let Some(dir) = rfd::FileDialog::new().set_title("Open a PSG Academic PDF project folder").pick_folder() else { return };
        match Project::open(&dir) {
            Ok(p) => {
                self.status = format!("Opened {}.", p.dir.display());
                self.set_project(p);
            }
            Err(e) => self.fail(format!("Could not open the project: {e}")),
        }
    }

    fn import_manuscript(&mut self) {
        let Some(project) = &self.project else { return };
        let Some(path) = rfd::FileDialog::new().set_title("Import the manuscript body (LaTeX)").add_filter("LaTeX", &["tex"]).pick_file() else {
            return;
        };
        let result = project.import_manuscript(&path);
        self.after_import(result);
    }

    fn import_docx(&mut self) {
        let (Some(project), Some(pandoc)) = (&self.project, self.env.as_ref().and_then(|e| e.pandoc.path.clone())) else { return };
        let Some(path) =
            rfd::FileDialog::new().set_title("Import a Word manuscript (experimental, via Pandoc)").add_filter("Word", &["docx"]).pick_file()
        else {
            return;
        };
        let result = PandocImporter { pandoc }.import(&path, project, None);
        self.after_import(result);
    }

    fn after_import(&mut self, result: Result<String, psg_academic::Error>) {
        match result {
            Ok(note) => {
                self.status = note.clone();
                self.note(note);
            }
            Err(e) => self.fail(format!("Import failed: {e}")),
        }
    }

    fn add_files(&mut self) {
        let Some(project) = &self.project else { return };
        let Some(paths) = rfd::FileDialog::new()
            .set_title("Add bibliography, figures or other files")
            .add_filter("Supported files", psg_academic::project::ASSET_EXTENSIONS)
            .pick_files()
        else {
            return;
        };
        let results: Vec<String> = paths
            .iter()
            .map(|p| match project.import_asset(p) {
                Ok(name) => format!("Added {} as {name}", p.display()),
                Err(e) => format!("Not added: {e}"),
            })
            .collect();
        for r in results {
            self.note(r);
        }
        self.status = "Files added; see Activity for their names in the project.".into();
    }

    fn start_compile(&mut self, ctx: &egui::Context) {
        if self.running.is_some() {
            return;
        }
        self.apply_buffers();
        let (Some(project), Some(env)) = (self.project.clone(), self.env.clone()) else {
            self.fail("Open or create a project, and wait for the TeX check, before compiling.");
            return;
        };
        if let Err(e) = project.save() {
            self.fail(format!("Could not save the project: {e}"));
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let flag = cancel.clone();
        let repaint = ctx.clone();
        let spawned = std::thread::Builder::new().name("psg-compile".into()).spawn(move || {
            let result = project.compile(&env, Some(&flag)).map_err(|e| e.to_string());
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        match spawned {
            Ok(_) => {
                self.running = Some(Running { rx, cancel, started: Instant::now() });
                self.status = "Compiling with latexmk and pdfLaTeX…".into();
                self.note("Compilation started.");
            }
            Err(e) => self.fail(format!("Could not start the compilation: {e}")),
        }
    }

    fn export(&mut self) {
        let Some(project) = &self.project else { return };
        let suggested = suggested_file_name(&project.file.metadata);
        let Some(dest) = rfd::FileDialog::new().set_title("Export the PDF").add_filter("PDF", &["pdf"]).set_file_name(suggested).save_file() else {
            return;
        };
        let result = project.export_pdf(&dest);
        match result {
            Ok(()) => {
                self.status = format!("Exported {}.", dest.display());
                self.note(self.status.clone());
            }
            Err(e) => self.fail(format!("Export failed: {e}")),
        }
    }

    fn open_in_psg_pdf(&mut self) {
        let Some(project) = &self.project else { return };
        let pdf = project.pdf_path();
        let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(if cfg!(windows) { "psg-pdf.exe" } else { "psg-pdf" })));
        match exe.filter(|e| e.is_file()) {
            Some(exe) => match std::process::Command::new(&exe).arg("--new-window").arg(&pdf).spawn() {
                Ok(_) => self.note(format!("Opened {} in PSG PDF.", pdf.display())),
                Err(e) => self.fail(format!("Could not start PSG PDF: {e}")),
            },
            None => self.fail("PSG PDF was not found next to PSG Academic PDF. Build it with `cargo build -p psg-pdf`."),
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::Panel::top("psg-top").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(APP_NAME);
                ui.separator();
                let busy = self.running.is_some();
                let can_compile = self.project.is_some()
                    && self.env.as_ref().is_some_and(|e| e.missing(self.project.as_ref().is_none_or(|p| p.file.uses_biber)).is_empty());
                if ui.add_enabled(can_compile && !busy, egui::Button::new("▶ Compile")).on_hover_text("Typeset with latexmk and pdfLaTeX").clicked()
                {
                    self.start_compile(&ctx);
                }
                if busy
                    && ui.button("■ Cancel").clicked()
                    && let Some(r) = &self.running
                {
                    r.cancel.store(true, Ordering::Relaxed);
                    self.note("Cancelling…");
                }
                let has_pdf = self.project.as_ref().is_some_and(|p| p.pdf_path().is_file()) && !busy;
                if ui.add_enabled(has_pdf, egui::Button::new("Export PDF…")).clicked() {
                    self.export();
                }
                if ui
                    .add_enabled(has_pdf, egui::Button::new("Open in PSG PDF"))
                    .on_hover_text("Open the compiled PDF in PSG PDF for review, annotation and further editing")
                    .clicked()
                {
                    self.open_in_psg_pdf();
                }
                ui.separator();
                if let Some(r) = &self.running {
                    ui.spinner();
                    ui.label(format!("{:.0} s", r.started.elapsed().as_secs_f32()));
                    ctx.request_repaint_after(std::time::Duration::from_millis(250));
                }
                ui.label(&self.status);
            });
        });
    }

    fn tex_status(&mut self, ui: &mut egui::Ui) {
        ui.heading("TeX installation");
        match &self.env {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("Checking…");
                });
            }
            Some(env) => {
                egui::Grid::new("tex-grid").num_columns(2).show(ui, |ui| {
                    for tool in [&env.pdflatex, &env.latexmk, &env.biber, &env.pandoc] {
                        ui.label(tool.name);
                        match &tool.path {
                            Some(p) => ui.label(tool.version.clone().unwrap_or_else(|| "found".into())).on_hover_text(p.display().to_string()),
                            None if tool.name == "pandoc" => ui.weak("not found (optional, for DOCX import)"),
                            None => ui.colored_label(ui.visuals().error_fg_color, "not found"),
                        };
                        ui.end_row();
                    }
                });
            }
        }
        if ui.add_enabled(self.env_rx.is_none(), egui::Button::new("Check again")).clicked() {
            let ctx = ui.ctx().clone();
            self.env = None;
            self.detect_tex(&ctx);
        }
    }

    fn project_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Project");
        ui.label("Journal template");
        let current = self.templates.get(self.selected_template).map(|t| t.manifest.name.clone()).unwrap_or_else(|| "(none)".into());
        egui::ComboBox::from_id_salt("template").selected_text(current).width(ui.available_width()).show_ui(ui, |ui| {
            for (i, t) in self.templates.iter().enumerate() {
                ui.selectable_value(&mut self.selected_template, i, &t.manifest.name).on_hover_text(&t.manifest.description);
            }
        });
        if let Some(t) = self.templates.get(self.selected_template) {
            ui.weak(format!("{} · {} · bibliography: {}", t.manifest.id, t.manifest.engine, t.manifest.bibliography));
        }
        ui.horizontal_wrapped(|ui| {
            if ui.button("New project").on_hover_text(format!("Create the project in {}", self.workspace.display())).clicked() {
                self.new_project();
            }
            if ui.button("New in folder…").clicked() {
                self.new_project_in_folder();
            }
            if ui.button("Open…").clicked() {
                self.open_project();
            }
        });
        let Some(project) = &self.project else {
            ui.weak("No project yet. Choose a template and press New project.");
            return;
        };
        ui.label(egui::RichText::new(project.dir.display().to_string()).small())
            .on_hover_text("The project folder: compilation happens here and only here");
        ui.add_space(4.0);
        ui.label("Import");
        let pandoc = self.env.as_ref().is_some_and(|e| e.pandoc.found());
        ui.horizontal_wrapped(|ui| {
            if ui.button("Manuscript (.tex)…").on_hover_text("Replaces the article body; a full document keeps only its body").clicked() {
                self.import_manuscript();
            }
            if ui
                .add_enabled(pandoc, egui::Button::new("Word (.docx)…"))
                .on_hover_text("Experimental: converts with the local Pandoc")
                .on_disabled_hover_text("Install Pandoc to import .docx (experimental)")
                .clicked()
            {
                self.import_docx();
            }
            if ui.button("Bibliography / figures…").clicked() {
                self.add_files();
            }
        });
    }

    fn settings_section(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &mut self.project else { return };
        ui.heading("Typesetting");
        let s = &mut project.file.settings;
        ui.horizontal(|ui| {
            ui.label("Engine");
            ui.weak("pdfLaTeX via latexmk (shell escape off)");
        });
        ui.horizontal(|ui| {
            ui.label("Time limit");
            let (min, max) = (compile::MIN_TIMEOUT.as_secs(), compile::MAX_TIMEOUT.as_secs());
            ui.add(egui::DragValue::new(&mut s.timeout_secs).range(min..=max).suffix(" s"));
        });
        ui.checkbox(&mut s.force_rebuild, "Rebuild everything each time");
        ui.horizontal(|ui| {
            ui.label("Preview zoom");
            ui.add(egui::Slider::new(&mut self.zoom, 0.5..=3.0).step_by(0.25));
        });
        if ui.add_enabled(!self.preview.rendering && project.pdf_path().is_file(), egui::Button::new("Redraw preview")).clicked() {
            let ctx = ui.ctx().clone();
            self.load_preview(&ctx);
        }
    }

    fn metadata_section(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &mut self.project else { return };
        let m = &mut project.file.metadata;
        ui.heading("Publication metadata");
        egui::CollapsingHeader::new("Journal and issue").default_open(true).show(ui, |ui| {
            egui::Grid::new("journal-grid").num_columns(2).show(ui, |ui| {
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
                    ui.label(label);
                    ui.text_edit_singleline(value);
                    ui.end_row();
                }
            });
        });
        egui::CollapsingHeader::new("Article").default_open(true).show(ui, |ui| {
            egui::Grid::new("article-grid").num_columns(2).show(ui, |ui| {
                for (label, value) in [("Type", &mut m.article_type), ("Title", &mut m.title), ("Short title", &mut m.short_title)] {
                    ui.label(label);
                    ui.text_edit_singleline(value);
                    ui.end_row();
                }
                ui.label("Keywords");
                ui.text_edit_singleline(&mut self.buffers.keywords).on_hover_text("Separate keywords with ; or ,");
                ui.end_row();
            });
            ui.label("Abstract");
            ui.add(egui::TextEdit::multiline(&mut m.abstract_text).desired_rows(5).desired_width(f32::INFINITY));
        });
        egui::CollapsingHeader::new("Authors and affiliations").default_open(false).show(ui, |ui| {
            let mut remove = None;
            self.buffers.author_affiliations.resize(m.authors.len(), String::new());
            for (i, (author, aff)) in m.authors.iter_mut().zip(self.buffers.author_affiliations.iter_mut()).enumerate() {
                ui.push_id(("author", i), |ui| {
                    egui::Grid::new("author").num_columns(2).show(ui, |ui| {
                        ui.label(format!("Author {}", i + 1));
                        ui.text_edit_singleline(&mut author.name);
                        ui.end_row();
                        ui.label("Affiliations");
                        ui.text_edit_singleline(aff).on_hover_text("Affiliation numbers, e.g. 1, 2");
                        ui.end_row();
                        ui.label("ORCID iD");
                        ui.text_edit_singleline(&mut author.orcid);
                        ui.end_row();
                        ui.label("Email");
                        ui.text_edit_singleline(&mut author.email);
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut author.corresponding, "Corresponding author");
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                    ui.separator();
                });
            }
            if let Some(i) = remove {
                m.authors.remove(i);
                self.buffers.author_affiliations.remove(i);
            }
            if ui.button("Add author").clicked() {
                m.authors.push(Author::default());
                self.buffers.author_affiliations.push(String::new());
            }
            ui.add_space(6.0);
            let mut remove = None;
            for (i, a) in m.affiliations.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(format!("{}", i + 1));
                    ui.text_edit_singleline(a);
                    if ui.small_button("✕").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                m.affiliations.remove(i);
            }
            if ui.button("Add affiliation").clicked() {
                m.affiliations.push(String::new());
            }
        });
        egui::CollapsingHeader::new("Dates and licence").default_open(false).show(ui, |ui| {
            egui::Grid::new("dates-grid").num_columns(2).show(ui, |ui| {
                for (label, value) in [
                    ("Received", &mut m.received),
                    ("Revised", &mut m.revised),
                    ("Accepted", &mut m.accepted),
                    ("Published", &mut m.published),
                    ("Copyright", &mut m.copyright),
                ] {
                    ui.label(label);
                    ui.add(egui::TextEdit::singleline(value).hint_text(if label == "Copyright" { "" } else { "YYYY-MM-DD" }));
                    ui.end_row();
                }
            });
            ui.label("Licence statement");
            ui.add(egui::TextEdit::multiline(&mut m.licence).desired_rows(2).desired_width(f32::INFINITY));
        });
        let issues = m.validate();
        if !issues.is_empty() {
            ui.add_space(4.0);
            for issue in issues {
                let color = if issue.blocking { ui.visuals().error_fg_color } else { ui.visuals().warn_fg_color };
                ui.colored_label(color, format!("{}: {}", issue.field, issue.message));
            }
        }
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            let problems = self.report.as_ref().map_or(0, |r| r.diagnostics.len());
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Preview, "PDF preview");
                ui.selectable_value(&mut self.tab, Tab::Problems, format!("Problems ({problems})"));
                ui.selectable_value(&mut self.tab, Tab::Output, "Compile output");
                ui.selectable_value(&mut self.tab, Tab::TexLog, "TeX log");
                ui.selectable_value(&mut self.tab, Tab::Activity, "Activity");
            });
            ui.separator();
            match self.tab {
                Tab::Preview => self.preview_tab(ui),
                Tab::Problems => self.problems_tab(ui),
                Tab::Output => {
                    let text = self
                        .report
                        .as_ref()
                        .map(|r| format!("$ {}\n\n{}", r.command, r.output))
                        .unwrap_or_else(|| "Compile to see latexmk's output here.".into());
                    log_view(ui, &text);
                }
                Tab::TexLog => {
                    let text = self.report.as_ref().and_then(|r| r.tex_log.clone()).unwrap_or_else(|| "No TeX log yet.".into());
                    log_view(ui, &text);
                }
                Tab::Activity => log_view(ui, &self.activity.join("\n")),
            }
        });
    }

    fn preview_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(e) = &self.preview.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        if self.preview.pages.is_empty() {
            if self.preview.rendering {
                ui.spinner();
            } else {
                ui.weak("Compile the project to see the PDF here.");
            }
            return;
        }
        if self.preview.total_pages > MAX_PREVIEW_PAGES {
            ui.weak(format!("Showing the first {MAX_PREVIEW_PAGES} of {} pages.", self.preview.total_pages));
        }
        let ppp = ui.ctx().pixels_per_point();
        egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                for (i, page) in self.preview.pages.iter().enumerate() {
                    match page {
                        PageSlot::Ready(texture, size) => {
                            ui.add(egui::Image::new((texture.id(), *size / ppp)));
                        }
                        PageSlot::Pending => {
                            ui.add_sized([200.0, 280.0], egui::Spinner::new());
                        }
                        PageSlot::Failed(e) => {
                            ui.colored_label(ui.visuals().error_fg_color, format!("Page {}: {e}", i + 1));
                        }
                    }
                    ui.add_space(12.0);
                }
            });
        });
    }

    fn problems_tab(&mut self, ui: &mut egui::Ui) {
        let Some(report) = &self.report else {
            ui.weak("Compile to see errors and warnings here.");
            return;
        };
        ui.label(report.summary());
        if report.diagnostics.is_empty() {
            ui.weak("No errors or warnings.");
            return;
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for d in &report.diagnostics {
                let color = match d.severity {
                    Severity::Error => ui.visuals().error_fg_color,
                    Severity::Warning => ui.visuals().warn_fg_color,
                };
                ui.colored_label(color, d.to_string());
            }
        });
    }
}

fn log_view(ui: &mut egui::Ui, text: &str) {
    egui::ScrollArea::both().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
        ui.add(egui::Label::new(egui::RichText::new(text).monospace().size(12.0)).wrap_mode(egui::TextWrapMode::Extend).selectable(true));
    });
}

fn join_numbers(n: &[usize]) -> String {
    n.iter().map(usize::to_string).collect::<Vec<_>>().join(", ")
}

/// A file name for the exported PDF: the DOI's suffix, else the short title, else "article".
fn suggested_file_name(m: &PublicationMetadata) -> String {
    let base = m
        .doi
        .split_once('/')
        .map(|(_, s)| s.to_owned())
        .filter(|s| !s.trim().is_empty())
        .or_else(|| Some(m.short_title.clone()).filter(|s| !s.trim().is_empty()))
        .unwrap_or_else(|| "article".into());
    let name = psg_academic::paths::sanitize_name(&base).unwrap_or_else(|| "article".into());
    format!("{name}.pdf")
}

impl eframe::App for AcademicApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        if self.running.is_some() || self.preview.rendering || self.env_rx.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        self.top_bar(ui);
        egui::Panel::left("psg-side").resizable(true).default_size(380.0).min_size(300.0).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                self.project_section(ui);
                ui.separator();
                self.settings_section(ui);
                ui.separator();
                self.metadata_section(ui);
                ui.separator();
                self.tex_status(ui);
                ui.add_space(8.0);
                ui.weak(format!("{APP_NAME} {} · {PUBLISHER}", env!("CARGO_PKG_VERSION")));
                ui.weak("PDF engine: PdfCraft (MIT OR Apache-2.0)");
            });
        });
        self.central(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_names_are_safe() {
        let mut m = PublicationMetadata::sample();
        assert_eq!(suggested_file_name(&m), "psg.example.0001.pdf");
        m.doi.clear();
        assert_eq!(suggested_file_name(&m), "A_Sample_PSG_Article.pdf");
        m.short_title = "../../x".into();
        assert_eq!(suggested_file_name(&m), "x.pdf");
        m.short_title.clear();
        assert_eq!(suggested_file_name(&m), "article.pdf");
    }
}
