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

use psg_academic::bib::BibEntry;
use psg_academic::pandoc::{ManuscriptImporter, PandocImporter};
use psg_academic::{Author, CompileReport, Diagnostic, Outcome, Project, PublicationMetadata, Severity, Template, TexEnvironment, compile, template};

use crate::data_dir::DataDir;
use crate::i18n::{t, tf};
use crate::launch::{self, ManuscriptKind, Request};
use crate::preview::{MAX_PREVIEW_PAGES, PageSlot, Preview};
use crate::source::SourceEditor;
use crate::updates::Updates;

/// The program's display name.
pub const APP_NAME: &str = "PSG Academic PDF";
/// The organisation that publishes it.
pub const PUBLISHER: &str = "Panorama Scholarly Group";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Preview,
    /// The source editor and the PDF side by side.
    Source,
    References,
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
    updates: Updates,
    /// Paths to open (command line, Open with, Finder, drops), handled once the TeX check is
    /// done: a Word manuscript needs to know where Pandoc is.
    pending: Vec<PathBuf>,
    source: SourceEditor,
    /// Problems found before compiling (scripts the templates can't set, the bibliography).
    preflight: Vec<Diagnostic>,
    bib: Vec<(String, BibEntry)>,
    bib_filter: String,
    /// Where the chosen interface language is saved ("" or missing: follow the system).
    language_file: PathBuf,
    #[cfg(target_os = "macos")]
    os_events: Option<crate::apple_events::OsEventsFn>,
}

impl AcademicApp {
    pub fn new(ctx: &egui::Context, data: DataDir, paths: Vec<PathBuf>) -> Self {
        let (templates, template_problems) = template::discover(&template::default_template_dirs());
        let workspace = data.projects();
        let language_file = data.root.join("language.txt");
        let chosen = std::fs::read_to_string(&language_file).unwrap_or_default();
        crate::i18n::set(if chosen.trim().is_empty() { crate::i18n::system() } else { chosen.trim() });
        crate::fonts::install(ctx, crate::i18n::current());
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
            status: t("Checking the TeX installation…").into(),
            activity: Vec::new(),
            updates: Updates::default(),
            pending: paths,
            source: SourceEditor::default(),
            preflight: Vec::new(),
            bib: Vec::new(),
            bib_filter: String::new(),
            language_file,
            #[cfg(target_os = "macos")]
            os_events: None,
        };
        if data.portable {
            app.note(tf("Portable mode: settings and projects are kept in {path}.", &[("path", &data.root.display().to_string())]));
        }
        if let Some(note) = data.note {
            app.note(note);
        }
        for p in app.template_problems.clone() {
            app.note(tf("Template skipped: {problem}", &[("problem", &p)]));
        }
        app.detect_tex(ctx);
        app
    }

    /// Where Finder's open-documents events come from (macOS).
    #[cfg(target_os = "macos")]
    pub fn set_os_events(&mut self, events: crate::apple_events::OsEventsFn) {
        self.os_events = Some(events);
    }

    /// Picks up paths from Finder and from files dropped on the window. True when macOS asks the
    /// app to quit.
    fn take_os_paths(&mut self, ctx: &egui::Context) -> bool {
        #[cfg(target_os = "macos")]
        let quit = {
            let mut quit = false;
            if let Some(events) = &mut self.os_events {
                for event in events() {
                    match event {
                        crate::apple_events::OsEvent::Open(paths) => self.pending.extend(paths),
                        crate::apple_events::OsEvent::Quit => quit = true,
                    }
                }
            }
            quit
        };
        #[cfg(not(target_os = "macos"))]
        let quit = false;
        let dropped: Vec<PathBuf> =
            ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect());
        self.pending.extend(dropped);
        quit
    }

    /// Opens what was asked for, once TeX has been checked. One project is open at a time, so
    /// of several paths the last one wins and the others are noted.
    fn open_pending(&mut self) {
        if self.pending.is_empty() || self.env.is_none() || self.running.is_some() {
            return;
        }
        let mut paths = std::mem::take(&mut self.pending);
        let Some(last) = paths.pop() else { return };
        for p in paths {
            self.note(tf("Not opened (one project at a time): {path}", &[("path", &p.display().to_string())]));
        }
        match launch::classify(&last) {
            Ok(Request::Project(dir)) => match Project::open(&dir) {
                Ok(p) => {
                    self.status = tf("Opened {path}.", &[("path", &p.dir.display().to_string())]);
                    self.set_project(p);
                }
                Err(e) => self.fail(tf("Could not open the project: {error}", &[("error", &e.to_string())])),
            },
            Ok(Request::Manuscript { path, kind }) => self.new_project_around(&path, kind),
            Err(e) => self.fail(e),
        }
    }

    /// A new project from the selected template, with `manuscript` imported as its body. The
    /// original file is only read.
    fn new_project_around(&mut self, manuscript: &std::path::Path, kind: ManuscriptKind) {
        let pandoc = self.env.as_ref().and_then(|e| e.pandoc.path.clone());
        if kind == ManuscriptKind::Docx && pandoc.is_none() {
            self.fail(tf("{path} is a Word manuscript; install Pandoc to import it.", &[("path", &manuscript.display().to_string())]));
            return;
        }
        let Some(tpl) = self.templates.get(self.selected_template).cloned() else {
            self.fail(t("No template is available."));
            return;
        };
        let name = launch::project_name(manuscript, &chrono::Local::now().format("%Y%m%d-%H%M%S").to_string());
        let project = match Project::create(&self.workspace.join(name), &tpl, PublicationMetadata::sample()) {
            Ok(p) => p,
            Err(e) => return self.fail(tf("Could not create the project: {error}", &[("error", &e.to_string())])),
        };
        let result = match (kind, pandoc) {
            (ManuscriptKind::Docx, Some(pandoc)) => PandocImporter { pandoc }.import(manuscript, &project, None),
            _ => project.import_manuscript(manuscript),
        };
        self.note(tf("New project from {template} around {path}.", &[("template", &tpl.manifest.name), ("path", &manuscript.display().to_string())]));
        self.set_project(project);
        self.after_import(result);
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
            Err(e) => self.fail(tf("Could not check the TeX installation: {error}", &[("error", &e.to_string())])),
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.env_rx
            && let Ok(env) = rx.try_recv()
        {
            self.env_rx = None;
            let missing = env.missing(true);
            if missing.is_empty() {
                self.status = t("TeX is ready.").into();
                self.note(format!(
                    "TeX found: pdflatex {}, latexmk {}, biber {}{}",
                    env.pdflatex.version.as_deref().unwrap_or("?"),
                    env.latexmk.version.as_deref().unwrap_or("?"),
                    env.biber.version.as_deref().unwrap_or("?"),
                    if env.pandoc.found() { ", pandoc found" } else { "" }
                ));
            } else {
                self.fail(tf(
                    "TeX is not ready: {tools} not found. Install TeX Live with latexmk and biber, then press Check again.",
                    &[("tools", &missing.join(", "))],
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
            Err(e) => self.fail(tf("Could not read {path}: {error}", &[("path", &pdf.display().to_string()), ("error", &e.to_string())])),
        }
    }

    fn set_project(&mut self, project: Project) {
        self.buffers.keywords = project.file.metadata.keywords.join("; ");
        self.buffers.author_affiliations = project.file.metadata.authors.iter().map(|a| join_numbers(&a.affiliations)).collect();
        self.note(tf("Project: {path} (template {template})", &[("path", &project.dir.display().to_string()), ("template", &project.file.template)]));
        self.source = SourceEditor::default();
        self.source.load(&project.dir, &project.file.body);
        self.project = Some(project);
        self.report = None;
        self.preview.clear();
        self.refresh_checks();
    }

    /// Run the checks that need no TeX: the manuscript's scripts and the bibliography.
    fn refresh_checks(&mut self) {
        let Some(project) = &self.project else { return };
        self.preflight = project.preflight();
        self.bib = project.bibliography().0;
    }

    /// Save the file open in the source editor, if it changed. False when saving failed.
    fn save_source(&mut self) -> bool {
        let Some(project) = &self.project else { return true };
        if !self.source.dirty() || self.source.file.is_empty() {
            return true;
        }
        let result = if self.source.file == project.file.body {
            project.write_body(&self.source.text).map_err(|e| e.to_string())
        } else {
            psg_academic::paths::write_atomic(&project.dir.join(&self.source.file), self.source.text.as_bytes()).map_err(|e| e.to_string())
        };
        match result {
            Ok(()) => {
                self.source.mark_saved();
                self.note(tf("Saved {file}.", &[("file", &self.source.file)]));
                self.refresh_checks();
                true
            }
            Err(e) => {
                self.fail(tf("Could not save {file}: {error}", &[("file", &self.source.file), ("error", &e)]));
                false
            }
        }
    }

    /// Show `file` at `line` in the source editor (saving the file shown first).
    fn go_to(&mut self, file: &str, line: Option<u32>) {
        let Some(dir) = self.project.as_ref().map(|p| p.dir.clone()) else { return };
        let name = file.trim_start_matches("./").to_owned();
        if !self.editable_files().contains(&name) {
            return;
        }
        if name != self.source.file {
            if !self.save_source() {
                return;
            }
            self.source.load(&dir, &name);
        }
        if let Some(line) = line {
            self.source.go_to(line);
        }
        self.tab = Tab::Source;
    }

    /// The files the source editor offers: the body, then the bibliography files.
    fn editable_files(&self) -> Vec<String> {
        let Some(project) = &self.project else { return Vec::new() };
        let mut files = vec![project.file.body.clone()];
        files.extend(project.bib_files());
        files
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
        let Some(tpl) = self.templates.get(self.selected_template).cloned() else {
            self.fail(t("No template is available."));
            return;
        };
        let name = format!("{}-{}", tpl.manifest.id, chrono::Local::now().format("%Y%m%d-%H%M%S"));
        let dir = self.workspace.join(name);
        match Project::create(&dir, &tpl, PublicationMetadata::sample()) {
            Ok(p) => {
                self.status =
                    tf("New project from {template} in {path}.", &[("template", &tpl.manifest.name), ("path", &p.dir.display().to_string())]);
                self.set_project(p);
            }
            Err(e) => self.fail(tf("Could not create the project: {error}", &[("error", &e.to_string())])),
        }
    }

    fn new_project_in_folder(&mut self) {
        let Some(tpl) = self.templates.get(self.selected_template).cloned() else { return };
        let Some(dir) = rfd::FileDialog::new().set_title(t("Choose an empty folder for the project")).pick_folder() else { return };
        match Project::create(&dir, &tpl, PublicationMetadata::sample()) {
            Ok(p) => self.set_project(p),
            Err(e) => self.fail(tf("Could not create the project: {error}", &[("error", &e.to_string())])),
        }
    }

    fn open_project(&mut self) {
        let Some(dir) = rfd::FileDialog::new().set_title(t("Open a PSG Academic PDF project folder")).pick_folder() else { return };
        match Project::open(&dir) {
            Ok(p) => {
                self.status = tf("Opened {path}.", &[("path", &p.dir.display().to_string())]);
                self.set_project(p);
            }
            Err(e) => self.fail(tf("Could not open the project: {error}", &[("error", &e.to_string())])),
        }
    }

    fn import_manuscript(&mut self) {
        let Some(project) = &self.project else { return };
        let Some(path) = rfd::FileDialog::new().set_title(t("Import the manuscript body (LaTeX)")).add_filter("LaTeX", &["tex"]).pick_file() else {
            return;
        };
        let result = project.import_manuscript(&path);
        self.after_import(result);
    }

    fn import_docx(&mut self) {
        let (Some(project), Some(pandoc)) = (&self.project, self.env.as_ref().and_then(|e| e.pandoc.path.clone())) else { return };
        let Some(path) =
            rfd::FileDialog::new().set_title(t("Import a Word manuscript (experimental, via Pandoc)")).add_filter("Word", &["docx"]).pick_file()
        else {
            return;
        };
        let result = PandocImporter { pandoc }.import(&path, project, None);
        self.after_import(result);
    }

    fn after_import(&mut self, result: Result<String, psg_academic::Error>) {
        match result {
            Ok(note) => {
                // The first line; what to fix before compiling follows in the Activity log.
                let mut lines = note.lines();
                let first = lines.next().unwrap_or_default().to_owned();
                self.status =
                    if lines.next().is_some() { tf("{first} Some things need fixing first: see Activity.", &[("first", &first)]) } else { first };
                for line in note.lines() {
                    self.note(line.to_owned());
                }
                if note.lines().count() > 1 {
                    self.tab = Tab::Activity;
                }
            }
            Err(e) => self.fail(tf("Import failed: {error}", &[("error", &e.to_string())])),
        }
    }

    fn add_files(&mut self) {
        let Some(project) = &self.project else { return };
        let Some(paths) = rfd::FileDialog::new()
            .set_title(t("Add bibliography, figures or other files"))
            .add_filter(t("Supported files"), psg_academic::project::ASSET_EXTENSIONS)
            .pick_files()
        else {
            return;
        };
        let results: Vec<String> = paths
            .iter()
            .map(|p| match project.import_asset(p) {
                Ok(name) => tf("Added {path} as {name}", &[("path", &p.display().to_string()), ("name", &name)]),
                Err(e) => tf("Not added: {error}", &[("error", &e.to_string())]),
            })
            .collect();
        for r in results {
            self.note(r);
        }
        self.status = t("Files added; see Activity for their names in the project.").into();
    }

    fn start_compile(&mut self, ctx: &egui::Context) {
        if self.running.is_some() {
            return;
        }
        self.apply_buffers();
        if !self.save_source() {
            return;
        }
        self.refresh_checks();
        let blocking = self.preflight.iter().filter(|d| d.severity == Severity::Error).count();
        if blocking > 0 {
            self.note(tf("{n} problems found before compiling; see Problems.", &[("n", &blocking.to_string())]));
        }
        let (Some(project), Some(env)) = (self.project.clone(), self.env.clone()) else {
            self.fail(t("Open or create a project, and wait for the TeX check, before compiling."));
            return;
        };
        if let Err(e) = project.save() {
            self.fail(tf("Could not save the project: {error}", &[("error", &e.to_string())]));
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
                self.status = t("Compiling with latexmk and pdfLaTeX…").into();
                self.note(t("Compilation started."));
            }
            Err(e) => self.fail(tf("Could not start the compilation: {error}", &[("error", &e.to_string())])),
        }
    }

    fn export(&mut self) {
        let Some(project) = &self.project else { return };
        let suggested = suggested_file_name(&project.file.metadata);
        let Some(dest) = rfd::FileDialog::new().set_title(t("Export the PDF")).add_filter("PDF", &["pdf"]).set_file_name(suggested).save_file()
        else {
            return;
        };
        let result = project.export_pdf(&dest);
        match result {
            Ok(()) => {
                self.status = tf("Exported {path}.", &[("path", &dest.display().to_string())]);
                self.note(self.status.clone());
            }
            Err(e) => self.fail(tf("Export failed: {error}", &[("error", &e.to_string())])),
        }
    }

    fn open_in_psg_pdf(&mut self) {
        let Some(project) = &self.project else { return };
        let pdf = project.pdf_path();
        match crate::psg_pdf::open(&pdf) {
            Ok(crate::psg_pdf::Opened::InPsgPdf) => self.note(tf("Opened {path} in PSG PDF.", &[("path", &pdf.display().to_string())])),
            Ok(crate::psg_pdf::Opened::InDefaultViewer) => {
                self.status = t("PSG PDF isn't installed, so the PDF opened in the default viewer. PSG PDF is a separate download.").into();
                self.note(tf("Opened {path} in the default PDF viewer.", &[("path", &pdf.display().to_string())]));
            }
            Err(e) => self.fail(e),
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
                if ui
                    .add_enabled(can_compile && !busy, egui::Button::new(format!("▶ {}", t("Compile"))))
                    .on_hover_text(t("Typeset with latexmk and pdfLaTeX"))
                    .clicked()
                {
                    self.start_compile(&ctx);
                }
                if busy
                    && ui.button(format!("■ {}", t("Cancel"))).clicked()
                    && let Some(r) = &self.running
                {
                    r.cancel.store(true, Ordering::Relaxed);
                    self.note(t("Cancelling…"));
                }
                let has_pdf = self.project.as_ref().is_some_and(|p| p.pdf_path().is_file()) && !busy;
                if ui.add_enabled(has_pdf, egui::Button::new(t("Export PDF…"))).clicked() {
                    self.export();
                }
                if ui
                    .add_enabled(has_pdf, egui::Button::new(t("Open in PSG PDF")))
                    .on_hover_text(t("Open the compiled PDF in PSG PDF for review, annotation and further editing"))
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

    /// The interface language: the system's, or one picked here (saved in the data folder).
    fn language_picker(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(t("Language"));
            let current = crate::i18n::current();
            let name = crate::i18n::LANGUAGES.iter().find(|(c, _)| *c == current).map_or("English", |(_, n)| *n);
            let mut chosen = None;
            egui::ComboBox::from_id_salt("language").selected_text(name).show_ui(ui, |ui| {
                for (code, name) in crate::i18n::LANGUAGES {
                    if ui.selectable_label(code == current, name).clicked() {
                        chosen = Some(code);
                    }
                }
            });
            if let Some(code) = chosen {
                crate::i18n::set(code);
                crate::fonts::install(ui.ctx(), code);
                if let Err(e) = std::fs::write(&self.language_file, code) {
                    self.note(format!("{}: {e}", self.language_file.display()));
                }
            }
        });
    }

    fn tex_status(&mut self, ui: &mut egui::Ui) {
        ui.heading(t("TeX installation"));
        match &self.env {
            None => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t("Checking…"));
                });
            }
            Some(env) => {
                egui::Grid::new("tex-grid").num_columns(2).show(ui, |ui| {
                    for tool in [&env.pdflatex, &env.latexmk, &env.biber, &env.pandoc] {
                        ui.label(tool.name);
                        match &tool.path {
                            Some(p) => ui.label(tool.version.clone().unwrap_or_else(|| t("found").into())).on_hover_text(p.display().to_string()),
                            None if tool.name == "pandoc" => ui.weak(t("not found (optional, for DOCX import)")),
                            None => ui.colored_label(ui.visuals().error_fg_color, t("not found")),
                        };
                        ui.end_row();
                    }
                });
            }
        }
        if ui.add_enabled(self.env_rx.is_none(), egui::Button::new(t("Check again"))).clicked() {
            let ctx = ui.ctx().clone();
            self.env = None;
            self.detect_tex(&ctx);
        }
    }

    fn project_section(&mut self, ui: &mut egui::Ui) {
        ui.heading(t("Project"));
        ui.label(t("Journal template"));
        let current = self.templates.get(self.selected_template).map(|tpl| tpl.manifest.name.clone()).unwrap_or_else(|| t("(none)").into());
        egui::ComboBox::from_id_salt("template").selected_text(current).width(ui.available_width()).show_ui(ui, |ui| {
            for (i, tpl) in self.templates.iter().enumerate() {
                ui.selectable_value(&mut self.selected_template, i, &tpl.manifest.name).on_hover_text(&tpl.manifest.description);
            }
        });
        if let Some(tpl) = self.templates.get(self.selected_template) {
            ui.weak(tf(
                "{id} · {engine} · bibliography: {bibliography}",
                &[("id", &tpl.manifest.id), ("engine", &tpl.manifest.engine), ("bibliography", &tpl.manifest.bibliography)],
            ));
        }
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(t("New project"))
                .on_hover_text(tf("Create the project in {path}", &[("path", &self.workspace.display().to_string())]))
                .clicked()
            {
                self.new_project();
            }
            if ui.button(t("New in folder…")).clicked() {
                self.new_project_in_folder();
            }
            if ui.button(t("Open…")).clicked() {
                self.open_project();
            }
        });
        let Some(project) = &self.project else {
            ui.weak(t("No project yet. Choose a template and press New project."));
            return;
        };
        ui.label(egui::RichText::new(project.dir.display().to_string()).small())
            .on_hover_text(t("The project folder: compilation happens here and only here"));
        ui.add_space(4.0);
        ui.label(t("Import"));
        let pandoc_version = self.env.as_ref().and_then(|e| e.pandoc.version.clone());
        let pandoc_too_old = pandoc_version.as_deref().and_then(psg_academic::pandoc::sandbox_supported) == Some(false);
        let pandoc = self.env.as_ref().is_some_and(|e| e.pandoc.found()) && !pandoc_too_old;
        ui.horizontal_wrapped(|ui| {
            if ui.button(t("Manuscript (.tex)…")).on_hover_text(t("Replaces the article body; a full document keeps only its body")).clicked() {
                self.import_manuscript();
            }
            if ui
                .add_enabled(pandoc, egui::Button::new(t("Word (.docx)…")))
                .on_hover_text(t("Experimental: converts with the local Pandoc"))
                .on_disabled_hover_text(if pandoc_too_old {
                    t("This Pandoc is too old: DOCX import needs Pandoc 2.15 or newer")
                } else {
                    t("Install Pandoc to import .docx (experimental)")
                })
                .clicked()
            {
                self.import_docx();
            }
            if ui.button(t("Bibliography / figures…")).clicked() {
                self.add_files();
            }
        });
    }

    fn settings_section(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &mut self.project else { return };
        ui.heading(t("Typesetting"));
        let s = &mut project.file.settings;
        ui.horizontal(|ui| {
            ui.label(t("Engine"));
            ui.weak(t("pdfLaTeX via latexmk (shell escape off)"));
        });
        ui.horizontal(|ui| {
            ui.label(t("Time limit"));
            let (min, max) = (compile::MIN_TIMEOUT.as_secs(), compile::MAX_TIMEOUT.as_secs());
            ui.add(egui::DragValue::new(&mut s.timeout_secs).range(min..=max).suffix(" s"));
        });
        ui.checkbox(&mut s.force_rebuild, t("Rebuild everything each time"));
        ui.horizontal(|ui| {
            ui.label(t("Preview zoom"));
            ui.add(egui::Slider::new(&mut self.zoom, 0.5..=3.0).step_by(0.25));
        });
        if ui.add_enabled(!self.preview.rendering && project.pdf_path().is_file(), egui::Button::new(t("Redraw preview"))).clicked() {
            let ctx = ui.ctx().clone();
            self.load_preview(&ctx);
        }
    }

    fn metadata_section(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &mut self.project else { return };
        let m = &mut project.file.metadata;
        ui.heading(t("Publication metadata"));
        egui::CollapsingHeader::new(t("Journal and issue")).default_open(true).show(ui, |ui| {
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
                    ui.label(t(label));
                    ui.text_edit_singleline(value);
                    ui.end_row();
                }
            });
        });
        egui::CollapsingHeader::new(t("Article")).default_open(true).show(ui, |ui| {
            egui::Grid::new("article-grid").num_columns(2).show(ui, |ui| {
                for (label, value) in [("Type", &mut m.article_type), ("Title", &mut m.title), ("Short title", &mut m.short_title)] {
                    ui.label(t(label));
                    ui.text_edit_singleline(value);
                    ui.end_row();
                }
                ui.label(t("Keywords"));
                ui.text_edit_singleline(&mut self.buffers.keywords).on_hover_text(t("Separate keywords with ; or ,"));
                ui.end_row();
            });
            ui.label(t("Abstract"));
            ui.add(egui::TextEdit::multiline(&mut m.abstract_text).desired_rows(5).desired_width(f32::INFINITY));
        });
        egui::CollapsingHeader::new(t("Authors and affiliations")).default_open(false).show(ui, |ui| {
            let mut remove = None;
            self.buffers.author_affiliations.resize(m.authors.len(), String::new());
            for (i, (author, aff)) in m.authors.iter_mut().zip(self.buffers.author_affiliations.iter_mut()).enumerate() {
                ui.push_id(("author", i), |ui| {
                    egui::Grid::new("author").num_columns(2).show(ui, |ui| {
                        ui.label(tf("Author {n}", &[("n", &(i + 1).to_string())]));
                        ui.text_edit_singleline(&mut author.name);
                        ui.end_row();
                        ui.label(t("Affiliations"));
                        ui.text_edit_singleline(aff).on_hover_text(t("Affiliation numbers, e.g. 1, 2"));
                        ui.end_row();
                        ui.label(t("ORCID iD"));
                        ui.text_edit_singleline(&mut author.orcid);
                        ui.end_row();
                        ui.label(t("Email"));
                        ui.text_edit_singleline(&mut author.email);
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut author.corresponding, t("Corresponding author"));
                        if ui.small_button(t("Remove")).clicked() {
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
            if ui.button(t("Add author")).clicked() {
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
            if ui.button(t("Add affiliation")).clicked() {
                m.affiliations.push(String::new());
            }
        });
        egui::CollapsingHeader::new(t("Dates and licence")).default_open(false).show(ui, |ui| {
            egui::Grid::new("dates-grid").num_columns(2).show(ui, |ui| {
                for (label, value) in [
                    ("Received", &mut m.received),
                    ("Revised", &mut m.revised),
                    ("Accepted", &mut m.accepted),
                    ("Published", &mut m.published),
                    ("Copyright", &mut m.copyright),
                ] {
                    ui.label(t(label));
                    ui.add(egui::TextEdit::singleline(value).hint_text(if label == "Copyright" { "" } else { "YYYY-MM-DD" }));
                    ui.end_row();
                }
            });
            ui.label(t("Licence statement"));
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
            let problems = self.report.as_ref().map_or(0, |r| r.diagnostics.len()) + self.preflight.len();
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Preview, t("PDF preview"));
                ui.selectable_value(&mut self.tab, Tab::Source, t("Source and PDF"));
                ui.selectable_value(&mut self.tab, Tab::References, tf("References ({n})", &[("n", &self.bib.len().to_string())]));
                ui.selectable_value(&mut self.tab, Tab::Problems, tf("Problems ({n})", &[("n", &problems.to_string())]));
                ui.selectable_value(&mut self.tab, Tab::Output, t("Compile output"));
                ui.selectable_value(&mut self.tab, Tab::TexLog, t("TeX log"));
                ui.selectable_value(&mut self.tab, Tab::Activity, t("Activity"));
            });
            ui.separator();
            match self.tab {
                Tab::Preview => self.preview_tab(ui),
                Tab::Source => self.source_tab(ui),
                Tab::References => self.references_tab(ui),
                Tab::Problems => self.problems_tab(ui),
                Tab::Output => {
                    let text = self
                        .report
                        .as_ref()
                        .map(|r| format!("$ {}\n\n{}", r.command, r.output))
                        .unwrap_or_else(|| t("Compile to see latexmk's output here.").into());
                    log_view(ui, &text);
                }
                Tab::TexLog => {
                    let text = self.report.as_ref().and_then(|r| r.tex_log.clone()).unwrap_or_else(|| t("No TeX log yet.").into());
                    log_view(ui, &text);
                }
                Tab::Activity => log_view(ui, &self.activity.join("\n")),
            }
        });
    }

    /// The source editor on the left and the PDF on the right.
    fn source_tab(&mut self, ui: &mut egui::Ui) {
        if self.project.is_none() {
            ui.weak(t("Open or create a project to edit its manuscript."));
            return;
        }
        let save = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
        ui.horizontal(|ui| {
            if ui.add_enabled(self.source.dirty(), egui::Button::new(t("Save"))).on_hover_text(t("Save the file (Ctrl+S / ⌘S)")).clicked() || save {
                self.save_source();
            }
            if ui.button(t("Reload from disk")).on_hover_text(t("Discard unsaved changes and read the file again")).clicked()
                && let Some(project) = &self.project
            {
                let (dir, file) = (project.dir.clone(), self.source.file.clone());
                self.source.load(&dir, &file);
            }
            if self.source.dirty() {
                ui.weak(t("Unsaved changes; they are saved before compiling."));
            }
        });
        let files = self.editable_files();
        let file = self.source.file.clone();
        let marked: Vec<u32> = self
            .preflight
            .iter()
            .chain(self.report.iter().flat_map(|r| r.diagnostics.iter()))
            .filter(|d| d.file.as_deref().is_some_and(|f| f.trim_start_matches("./") == file))
            .filter_map(|d| d.line)
            .collect();
        let width = ui.available_width();
        let mut picked = None;
        ui.columns(2, |cols| {
            if let [left, right] = cols {
                left.set_min_width(width * 0.3);
                picked = self.source.show(left, &files, &marked);
                self.preview_tab(right);
            }
        });
        if let Some(name) = picked {
            self.go_to(&name, None);
        }
    }

    fn references_tab(&mut self, ui: &mut egui::Ui) {
        let Some(project) = &self.project else {
            ui.weak(t("Open or create a project to see its bibliography."));
            return;
        };
        if !project.file.uses_biber {
            ui.weak(t("This template doesn't use a bibliography file."));
        }
        ui.horizontal(|ui| {
            ui.label(t("Filter"));
            ui.text_edit_singleline(&mut self.bib_filter);
            if ui.button(t("Check again")).clicked() {
                self.refresh_checks();
            }
        });
        let filter = self.bib_filter.to_lowercase();
        let mut go = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("bib-grid").num_columns(3).striped(true).show(ui, |ui| {
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
                    ui.label(&e.kind);
                    ui.label(summary);
                    ui.end_row();
                }
            });
        });
        if let Some((file, line)) = go {
            self.go_to(&file, Some(line));
        }
    }

    fn preview_tab(&mut self, ui: &mut egui::Ui) {
        if let Some(e) = &self.preview.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        if self.preview.pages.is_empty() {
            if self.preview.rendering {
                ui.spinner();
            } else {
                ui.weak(t("Compile the project to see the PDF here."));
            }
            return;
        }
        if self.preview.total_pages > MAX_PREVIEW_PAGES {
            ui.weak(tf(
                "Showing the first {shown} of {total} pages.",
                &[("shown", &MAX_PREVIEW_PAGES.to_string()), ("total", &self.preview.total_pages.to_string())],
            ));
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
                            ui.colored_label(ui.visuals().error_fg_color, tf("Page {n}: {error}", &[("n", &(i + 1).to_string()), ("error", e)]));
                        }
                    }
                    ui.add_space(12.0);
                }
            });
        });
    }

    fn problems_tab(&mut self, ui: &mut egui::Ui) {
        let mut go = None;
        let editable = self.editable_files();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.heading(t("Before compiling"));
            if self.project.is_none() {
                ui.weak(t("Open or create a project to check it."));
            } else if self.preflight.is_empty() {
                ui.weak(t("No problems in the manuscript's scripts or the bibliography."));
            }
            for (i, d) in self.preflight.iter().enumerate() {
                ui.push_id(("pre", i), |ui| {
                    if let Some(target) = problem_row(ui, d, &editable) {
                        go = Some(target);
                    }
                });
            }
            ui.add_space(8.0);
            ui.heading(t("From the last compilation"));
            let Some(report) = &self.report else {
                ui.weak(t("Compile to see errors and warnings here."));
                return;
            };
            ui.label(report.summary());
            if report.diagnostics.is_empty() {
                ui.weak(t("No errors or warnings."));
            }
            for (i, d) in report.diagnostics.iter().enumerate() {
                ui.push_id(("tex", i), |ui| {
                    if let Some(target) = problem_row(ui, d, &editable) {
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

/// One problem: its message in the severity's colour, a hint when there is one, and "Go to line"
/// for a problem in a file the editor shows. Returns the place to go to when that is pressed.
fn problem_row(ui: &mut egui::Ui, d: &Diagnostic, editable: &[String]) -> Option<(String, Option<u32>)> {
    let color = match d.severity {
        Severity::Error => ui.visuals().error_fg_color,
        Severity::Warning => ui.visuals().warn_fg_color,
    };
    // The advice for a script the templates can't set is a sentence of its own, in this language.
    let advice = [
        psg_academic::scripts::Script::Han,
        psg_academic::scripts::Script::Greek,
        psg_academic::scripts::Script::Cyrillic,
        psg_academic::scripts::Script::Arabic,
        psg_academic::scripts::Script::Thai,
    ]
    .into_iter()
    .map(psg_academic::scripts::Script::advice)
    .find(|a| d.message.ends_with(a));
    let text = d.to_string();
    let text = advice.map_or(text.as_str(), |a| text.trim_end_matches(a).trim_end());
    let mut go = None;
    ui.horizontal_wrapped(|ui| {
        ui.colored_label(color, text);
        let file = d.file.as_deref().map(|f| f.trim_start_matches("./")).filter(|f| editable.iter().any(|e| e == f));
        if let Some(file) = file
            && ui.small_button(if d.line.is_some() { t("Go to line") } else { t("Open file") }).clicked()
        {
            go = Some((file.to_owned(), d.line));
        }
    });
    if let Some(hint) = advice.or_else(|| d.hint()) {
        ui.indent("hint", |ui| ui.weak(t(hint)));
    }
    go
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
        let quit = self.take_os_paths(&ctx);
        self.poll(&ctx);
        self.open_pending();
        // A ready update waits while a compilation runs.
        if self.updates.frame(&ctx, self.running.is_none()) || quit {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
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
                ui.separator();
                self.language_picker(ui);
                ui.add_space(8.0);
                ui.weak(format!("{APP_NAME} {} · {PUBLISHER}", env!("CARGO_PKG_VERSION")));
                ui.weak(t("PDF engine: PdfCraft (MIT OR Apache-2.0)"));
                if ui.small_button(t("Check for updates…")).clicked() {
                    self.updates.request_check();
                }
            });
        });
        self.central(ui);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(r) = &self.running {
            r.cancel.store(true, Ordering::Relaxed);
        }
        self.updates.exiting();
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
