//! The PSG Academic PDF window: a typesetting workspace built from the PSG Desktop Design System
//! (`psg-desktop-ui`).
//!
//! A toolbar carries the four main actions (Import, Auto Typeset, Preview, Export); a navigation
//! rail on the left switches between the workspace's areas (Dashboard, Manuscript, Template
//! Library, Typesetting, Problems, Export, Settings); a status bar shows the TeX installation, the
//! engine (always pdfLaTeX) and the last run; notifications stack in the corner. Each area is
//! drawn by a module under `app/`.
//!
//! The workspace's disallowed egui checkbox and combo box (clippy.toml) are about PSG PDF's own
//! theme, which draws a checkbox as a circle, and about combo boxes whose contents change while
//! open. This window's style keeps egui's square checkbox, and its lists don't change while a menu
//! is open, so egui's own widgets are used here.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use psg_academic::bib::BibEntry;
use psg_academic::managed::ManagedTex;
use psg_academic::pandoc::{ManuscriptImporter, PandocImporter};
use psg_academic::{CompileReport, Diagnostic, Outcome, Project, PublicationMetadata, Severity, Template, TexEnvironment, template};
use psg_desktop_ui::notice::Notices;
use psg_desktop_ui::style::ThemeChoice;
use psg_desktop_ui::{Tone, layout, widgets};

use crate::data_dir::DataDir;
use crate::i18n::{t, tf, tn};
use crate::launch::{self, ManuscriptKind, Request};
use crate::preview::Preview;
use crate::recent::Recent;
use crate::source::SourceEditor;
use crate::tex_setup::{self, Job, Progress};
use crate::updates::Updates;

mod dashboard;
mod data_folder;
mod export;
mod manuscript;
mod problems;
mod settings;
mod templates;
mod typesetting;

/// The program's display name (PSG design system, `psg-brand`).
pub const APP_NAME: &str = crate::theme::PRODUCT.name();
/// The organisation that publishes it.
pub const PUBLISHER: &str = psg_brand::PUBLISHER;

pub(crate) fn local_now() -> jiff::ZonedDateTime {
    psg_academic::managed::now_utc().to_zoned_datetime(jiff::tz::TimeZone::system())
}

/// The areas of the workspace, in the order the navigation rail lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Dashboard,
    Manuscript,
    Templates,
    Typesetting,
    Problems,
    Export,
    Settings,
}

impl Page {
    pub const ALL: [Page; 7] = [Page::Dashboard, Page::Manuscript, Page::Templates, Page::Typesetting, Page::Problems, Page::Export, Page::Settings];

    pub fn label(self) -> &'static str {
        match self {
            Page::Dashboard => t("Dashboard"),
            Page::Manuscript => t("Manuscript"),
            Page::Templates => t("Template Library"),
            Page::Typesetting => t("Typesetting"),
            Page::Problems => t("Problems"),
            Page::Export => t("Export"),
            Page::Settings => t("Settings"),
        }
    }

    /// The area's Lucide icon on the navigation rail (`psg_desktop_ui::icons`).
    pub fn icon(self) -> &'static str {
        match self {
            Page::Dashboard => "house",
            Page::Manuscript => "file-text",
            Page::Templates => "swatch-book",
            Page::Typesetting => "type",
            Page::Problems => "triangle-alert",
            Page::Export => "file-output",
            Page::Settings => "settings",
        }
    }
}

/// What the Manuscript area shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManuscriptTab {
    /// The source editor, alone, beside the PDF, or the PDF alone ([`SourceView`]).
    Source,
    /// Title, short title, type, keywords and abstract.
    Article,
    /// Authors and affiliations.
    Authors,
    /// Journal, issue, dates and licence.
    Publication,
    /// The bibliography entries.
    References,
}

/// How the source editor and the PDF preview share the Manuscript area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceView {
    Editor,
    Split,
    Preview,
}

/// Which log the Typesetting area shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogView {
    Output,
    TexLog,
    Activity,
}

/// Which problems the Problems area lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProblemFilter {
    All,
    Errors,
    Warnings,
}

/// One finished typesetting run, for the Typesetting area's history.
pub struct RunRecord {
    pub at: String,
    pub outcome: Outcome,
    pub summary: String,
}

/// Installing or updating PSG's own TeX, in the background.
pub(crate) struct TexJob {
    rx: Receiver<tex_setup::Event>,
    pub(crate) job: Job,
    pub(crate) progress: Progress,
}

/// A compilation's report and the TeX packages installed for it.
type CompileResult = Result<(CompileReport, Vec<String>), String>;

struct Running {
    rx: Receiver<CompileResult>,
    cancel: Arc<AtomicBool>,
    started: Instant,
}

/// Text-field buffers for the list fields, applied to the metadata before saving.
#[derive(Default)]
struct Buffers {
    keywords: String,
    author_affiliations: Vec<String>,
}

/// Where the layout of the workspace is remembered between runs (egui's persisted memory).
fn layout_id() -> egui::Id {
    egui::Id::unique("psg-academic-layout")
}

pub struct AcademicApp {
    env: Option<TexEnvironment>,
    env_rx: Option<Receiver<TexEnvironment>>,
    /// PSG's own TeX, which the app installs and keeps up to date (`None`: no folder for it).
    managed: Option<ManagedTex>,
    tex_job: Option<TexJob>,
    /// The automatic install or update was started in this run (it isn't retried by itself).
    tex_auto_started: bool,
    /// Why the last install or update failed, shown in the TeX card.
    tex_error: Option<String>,
    templates: Vec<Template>,
    template_problems: Vec<String>,
    /// What the last template added was found to have: its name, what was found, what to check.
    template_added: Option<(String, Vec<String>, Vec<String>)>,
    selected_template: usize,
    workspace: PathBuf,
    folder_choice: Option<data_folder::FolderChoice>,
    project: Option<Project>,
    saved_project: Option<psg_academic::project::ProjectFile>,
    buffers: Buffers,
    running: Option<Running>,
    report: Option<CompileReport>,
    preview: Preview,
    zoom: f32,
    /// Draw preview pages no wider than their pane.
    fit_width: bool,
    page: Page,
    manuscript_tab: ManuscriptTab,
    source_view: SourceView,
    /// The source editor's share of the Manuscript area in the split view.
    split_ratio: f32,
    show_outline: bool,
    log_view: LogView,
    problem_filter: ProblemFilter,
    history: Vec<RunRecord>,
    /// The last thing that happened, in the status bar.
    status: String,
    notices: Notices,
    /// Notifications raised where no egui context is at hand, shown at the next frame.
    queued: Vec<(Tone, String)>,
    activity: Vec<String>,
    updates: Updates,
    recent: Recent,
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
    /// The saved interface language: the system's, or one picked in Settings.
    language: psg_i18n::Preference,
    /// Where the chosen theme is saved ("system", "light" or "dark").
    theme_file: PathBuf,
    theme: ThemeChoice,
    /// The data folder in use and the default one.
    data: DataDir,
    relocation: data_folder::Relocation,
    #[cfg(target_os = "macos")]
    os_events: Option<crate::apple_events::OsEventsFn>,
}

impl AcademicApp {
    pub fn new(ctx: &egui::Context, data: DataDir, paths: Vec<PathBuf>) -> Self {
        let (templates, template_problems) = template::discover(&crate::cli::template_dirs(&data.root));
        let workspace = data.projects();
        let language_file = data.root.join("language.txt");
        let chosen = std::fs::read_to_string(&language_file).unwrap_or_default();
        let language = psg_i18n::Preference::parse(&chosen);
        crate::i18n::apply(language);
        crate::fonts::install(ctx, crate::i18n::current());
        crate::theme::apply(ctx);
        let theme_file = data.root.join("theme.txt");
        let theme = ThemeChoice::from_key(&std::fs::read_to_string(&theme_file).unwrap_or_default());
        psg_desktop_ui::style::set_theme(ctx, theme);
        let (split_ratio, show_outline) = ctx.data_mut(|d| d.get_persisted::<(f32, bool)>(layout_id())).unwrap_or((0.5, true));
        let mut app = Self {
            env: None,
            env_rx: None,
            managed: tex_setup::managed_for(&data.root, data.portable),
            tex_job: None,
            tex_auto_started: false,
            tex_error: None,
            templates,
            template_problems,
            template_added: None,
            selected_template: 0,
            workspace,
            folder_choice: None,
            project: None,
            saved_project: None,
            buffers: Buffers::default(),
            running: None,
            report: None,
            preview: Preview::default(),
            zoom: 1.0,
            fit_width: true,
            page: Page::Dashboard,
            manuscript_tab: ManuscriptTab::Source,
            source_view: SourceView::Split,
            split_ratio,
            show_outline,
            log_view: LogView::Output,
            problem_filter: ProblemFilter::All,
            history: Vec::new(),
            status: t("Checking the TeX installation…").into(),
            notices: Notices::default(),
            queued: Vec::new(),
            activity: Vec::new(),
            updates: Updates::with_channel_file(data.root.join("update-channel.txt")),
            recent: Recent::load(data.root.join("recent-projects.txt")),
            pending: paths,
            source: SourceEditor::default(),
            preflight: Vec::new(),
            bib: Vec::new(),
            bib_filter: String::new(),
            language_file,
            language,
            theme_file,
            theme,
            data: data.clone(),
            relocation: data_folder::Relocation::Idle,
            #[cfg(target_os = "macos")]
            os_events: None,
        };
        if data.portable {
            app.note(tf("Portable mode: settings and projects are kept in {path}.", &[("path", &data.root.display().to_string())]));
        }
        if data.is_custom() {
            app.note(format!("Data folder (chosen): {}", data.root.display()));
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

    /// Show a notification and record it in the activity log.
    fn notify(&mut self, ctx: &egui::Context, tone: Tone, text: impl Into<String>) {
        let text = text.into();
        self.notices.push(tone, text.clone(), ctx.input(|i| i.time));
        self.status = text.clone();
        self.note(text);
    }

    /// The number of errors and warnings, before compiling and from the last compilation.
    fn problem_counts(&self) -> (usize, usize) {
        let all = self.preflight.iter().chain(self.report.iter().flat_map(|r| r.diagnostics.iter()));
        all.fold((0, 0), |(e, w), d| match d.severity {
            Severity::Error => (e + 1, w),
            Severity::Warning => (e, w + 1),
        })
    }

    /// Whether everything a compilation needs is there: a project and the TeX tools it uses.
    fn can_compile(&self) -> bool {
        self.project.is_some()
            && self.running.is_none()
            && self.env.as_ref().is_some_and(|e| e.missing(self.project.as_ref().is_none_or(|p| p.file.uses_biber)).is_empty())
            && !self.tex_busy()
    }

    /// PSG's TeX, which the compilation would use, is being installed or updated.
    fn tex_busy(&self) -> bool {
        self.tex_job.is_some() && self.env.as_ref().is_none_or(|e| e.managed || !e.missing(true).is_empty())
    }

    /// Why the Auto Typeset action is unavailable, for its tooltip.
    fn why_not_compile(&self) -> &'static str {
        if self.project.is_none() {
            t("Open or create a project first.")
        } else if self.running.is_some() {
            t("Typesetting is already running.")
        } else if self.env.is_none() {
            t("Checking the TeX installation…")
        } else if self.tex_busy() {
            t("TeX is being installed or updated; typesetting starts once it is done.")
        } else {
            t("TeX is not ready: see Settings ▸ TeX installation.")
        }
    }

    fn has_pdf(&self) -> bool {
        self.running.is_none() && self.project.as_ref().is_some_and(|p| p.pdf_path().is_file())
    }

    /// Show the PDF beside the manuscript, drawing it first if it isn't drawn yet.
    fn show_preview(&mut self, ctx: &egui::Context) {
        self.page = Page::Manuscript;
        self.manuscript_tab = ManuscriptTab::Source;
        if self.source_view == SourceView::Editor {
            self.source_view = SourceView::Split;
        }
        if self.preview.pages.is_empty() && !self.preview.rendering && self.has_pdf() {
            self.load_preview(ctx);
        }
    }

    /// Save the manuscript file shown and the project's metadata and settings. Quietly does
    /// nothing without a project. False when something couldn't be saved (already reported).
    fn save_all(&mut self) -> bool {
        self.apply_buffers();
        if !self.save_source() {
            return false;
        }
        let Some(project) = &self.project else { return true };
        if self.saved_project.as_ref() == Some(&project.file) {
            return true;
        }
        match project.save() {
            Ok(()) => {
                self.saved_project = Some(project.file.clone());
                true
            }
            Err(e) => {
                self.fail(tf("Could not save the project: {error}", &[("error", &e.to_string())]));
                false
            }
        }
    }

    /// Choose a manuscript (.tex or, with Pandoc, .docx) and start a project around it from the
    /// selected template.
    fn import_new(&mut self) {
        let pandoc = self.env.as_ref().is_some_and(|e| e.pandoc.found());
        let mut dialog = rfd::FileDialog::new().set_title(t("Import a manuscript")).add_filter("LaTeX", &["tex"]);
        if pandoc {
            dialog = dialog.add_filter("Word", &["docx"]);
        }
        let Some(path) = dialog.pick_file() else { return };
        self.pending.push(path);
    }

    fn remember_layout(&self, ctx: &egui::Context) {
        ctx.data_mut(|d| d.insert_persisted(layout_id(), (self.split_ratio, self.show_outline)));
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
        if self.pending.is_empty() || self.env.is_none() || self.running.is_some() || self.moving_data() {
            return;
        }
        let mut paths = std::mem::take(&mut self.pending);
        let Some(last) = paths.pop() else { return };
        for p in paths {
            self.note(tf("Not opened (one project at a time): {path}", &[("path", &p.display().to_string())]));
        }
        if !self.prepare_project_switch() {
            return;
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
        let name = launch::project_name(manuscript, &local_now().strftime("%Y%m%d-%H%M%S").to_string());
        let mut project = match Project::create(&self.workspace.join(name), &tpl, PublicationMetadata::sample()) {
            Ok(p) => p,
            Err(e) => return self.fail(tf("Could not create the project: {error}", &[("error", &e.to_string())])),
        };
        let result = match (kind, pandoc) {
            (ManuscriptKind::Docx, Some(pandoc)) => PandocImporter { pandoc }.import(manuscript, &mut project, None),
            _ => project.import_manuscript(manuscript),
        };
        self.note(tf("New project from {template} around {path}.", &[("template", &tpl.manifest.name), ("path", &manuscript.display().to_string())]));
        self.set_project(project);
        self.after_import(result);
    }

    fn note(&mut self, text: impl Into<String>) {
        let text = text.into();
        log::info!("{text}");
        let stamp = local_now().strftime("%H:%M:%S");
        self.activity.push(format!("[{stamp}] {text}"));
        if self.activity.len() > 2000 {
            self.activity.drain(..500);
        }
    }

    /// Report a failure: in the status bar, the activity log and a notification that stays
    /// until dismissed.
    fn fail(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.status = text.clone();
        self.queued.push((Tone::Danger, text.clone()));
        self.note(text);
    }

    fn detect_tex(&mut self, ctx: &egui::Context) {
        let managed = self.managed.clone();
        let (tx, rx) = channel();
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("psg-tex-detect".into()).spawn(move || {
            let _ = tx.send(tex_setup::detect(managed.as_ref()));
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
            let installing = self.managed.as_ref().is_some_and(|m| !m.installed());
            if missing.is_empty() {
                self.status = t("TeX is ready.").into();
                self.note(format!(
                    "TeX found: pdflatex {}, latexmk {}, biber {}{}",
                    env.pdflatex.version.as_deref().unwrap_or("?"),
                    env.latexmk.version.as_deref().unwrap_or("?"),
                    env.biber.version.as_deref().unwrap_or("?"),
                    if env.pandoc.found() { ", pandoc found" } else { "" }
                ));
            } else if let Some(tool) = [&env.pdflatex, &env.latexmk, &env.biber].into_iter().find(|tool| tool.probe_error.is_some()) {
                let detail = format!("{}: {}", tool.name, tool.probe_error.as_deref().unwrap_or_default());
                let message = tf("Could not check the TeX installation: {error}", &[("error", &detail)]);
                if tool.name == "biber" && !self.tex_auto_started && self.biber_repair_needed(Some(&env)) {
                    self.status = t("Repairing Biber…").into();
                    self.note(message);
                } else {
                    self.fail(message);
                }
            } else if installing {
                self.status = t("Installing TeX for PSG Academic PDF…").into();
                self.note(tf("TeX not found yet ({tools}); PSG Academic PDF installs its own.", &[("tools", &missing.join(", "))]));
            } else {
                self.fail(tf(
                    "TeX is not ready: {tools} not found. Press Install TeX in Settings ▸ TeX installation.",
                    &[("tools", &missing.join(", "))],
                ));
            }
            self.env = Some(env);
            self.start_tex_maintenance(ctx);
        }
        self.poll_tex_job(ctx);
        if let Some(running) = &self.running
            && let Ok(result) = running.rx.try_recv()
        {
            self.running = None;
            match result {
                Ok((report, installed)) => {
                    if !installed.is_empty() {
                        self.note(tf("Installed the TeX packages this document needs: {packages}", &[("packages", &installed.join(", "))]));
                    }
                    self.finished(report, ctx)
                }
                Err(e) => self.fail(e),
            }
        }
        self.preview.poll(ctx);
    }

    /// Install, repair or update PSG's TeX automatically, once per app run.
    fn start_tex_maintenance(&mut self, ctx: &egui::Context) {
        if self.tex_auto_started || self.tex_job.is_some() {
            return;
        }
        let Some(tex) = &self.managed else { return };
        let job = if !tex.installed() {
            Job::Install
        } else if self.biber_repair_needed(self.env.as_ref()) {
            Job::RepairBiber
        } else if tex.update_due(psg_academic::managed::now_utc()) {
            Job::Update
        } else if !tex.missing_fonts().is_empty() {
            Job::Fonts
        } else {
            return;
        };
        self.tex_auto_started = true;
        self.start_tex_job(ctx, job);
    }

    /// Only repair Biber from PSG's own TeX tree.
    fn biber_repair_needed(&self, env: Option<&TexEnvironment>) -> bool {
        let Some(env) = env.filter(|env| env.managed && env.biber.probe_error.is_some()) else { return false };
        let Some(bin) = tex_setup::bin_dir(self.managed.as_ref()) else { return false };
        env.biber.path.as_deref().and_then(Path::parent) == Some(bin.as_path())
    }

    /// Start installing, repairing or updating PSG's TeX.
    pub(crate) fn start_tex_job(&mut self, ctx: &egui::Context, job: Job) {
        if self.tex_job.is_some() || self.running.is_some() {
            return;
        }
        let Some(tex) = self.managed.clone() else {
            self.fail(t("There is no folder PSG Academic PDF can install TeX in."));
            return;
        };
        let repaint = ctx.clone();
        match tex_setup::start(tex, job, move || repaint.request_repaint()) {
            Ok(rx) => {
                self.tex_error = None;
                let progress = match job {
                    Job::Install => Progress::Looking,
                    Job::Update => Progress::Updating,
                    Job::RepairBiber => Progress::Packages,
                    Job::Fonts => Progress::Fonts { done: 0, total: 0 },
                };
                self.tex_job = Some(TexJob { rx, job, progress });
                self.note(match job {
                    Job::Install => t("Installing TeX for PSG Academic PDF…"),
                    Job::Update => t("Updating TeX…"),
                    Job::RepairBiber => t("Repairing Biber…"),
                    Job::Fonts => t("Downloading the Chinese and Japanese fonts…"),
                });
            }
            Err(e) => self.fail(tf("Could not install TeX: {error}", &[("error", &e.to_string())])),
        }
    }

    fn poll_tex_job(&mut self, ctx: &egui::Context) {
        let Some(job) = &mut self.tex_job else { return };
        let mut finished = None;
        while let Ok(event) = job.rx.try_recv() {
            match event {
                tex_setup::Event::Progress(p) => job.progress = p,
                tex_setup::Event::Finished(result) => finished = Some(result),
            }
        }
        let Some(result) = finished else { return };
        let kind = job.job;
        self.tex_job = None;
        match result {
            Ok(tex_setup::Done::Installed(release)) => {
                self.notify(
                    ctx,
                    Tone::Success,
                    tf("TeX is installed (TinyTeX {release}) and will keep itself up to date.", &[("release", &release)]),
                );
                self.detect_tex(ctx);
            }
            Ok(tex_setup::Done::Updated) => {
                self.note(match kind {
                    Job::RepairBiber => t("Biber was reinstalled."),
                    Job::Fonts => t("The Chinese and Japanese fonts are installed."),
                    _ => t("TeX is up to date."),
                });
                self.detect_tex(ctx);
            }
            Err(e) => {
                let text = match kind {
                    Job::Install => tf("Could not install TeX: {error}", &[("error", &e)]),
                    Job::Update => tf("Could not update TeX: {error}", &[("error", &e)]),
                    Job::RepairBiber => tf("Could not repair Biber: {error}", &[("error", &e)]),
                    Job::Fonts => tf("Could not download the Chinese and Japanese fonts: {error}", &[("error", &e)]),
                };
                self.tex_error = Some(text.clone());
                if kind == Job::Install {
                    self.fail(text);
                } else {
                    // A failed weekly update leaves a working TeX: say so quietly.
                    self.note(text);
                }
            }
        }
    }

    fn finished(&mut self, report: CompileReport, ctx: &egui::Context) {
        let summary = summary(&report);
        self.history.push(RunRecord { at: local_now().strftime("%H:%M:%S").to_string(), outcome: report.outcome, summary: summary.clone() });
        if self.history.len() > 50 {
            self.history.remove(0);
        }
        match report.outcome {
            Outcome::Succeeded => {
                self.notify(ctx, Tone::Success, summary);
                // The preview updates where it is shown; from elsewhere, show the result.
                if self.page != Page::Manuscript {
                    self.page = Page::Typesetting;
                }
                self.load_preview(ctx);
            }
            Outcome::Failed => {
                self.notify(ctx, Tone::Danger, summary);
                self.page = Page::Problems;
            }
            Outcome::TimedOut | Outcome::Cancelled => {
                self.notify(ctx, Tone::Warning, summary);
                self.page = Page::Typesetting;
                self.log_view = LogView::Output;
            }
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

    /// Save before reading the destination or creating a new project folder. On error, keep
    /// the active project and its editor buffers available for another attempt.
    fn prepare_project_switch(&mut self) -> bool {
        if self.running.is_some() {
            self.fail(t("Typesetting is already running."));
            return false;
        }
        if self.moving_data() {
            self.fail(t("Moving the data folder…"));
            return false;
        }
        self.save_all()
    }

    fn moving_data(&self) -> bool {
        matches!(self.relocation, data_folder::Relocation::Moving { .. })
    }

    fn set_project(&mut self, project: Project) {
        self.recent.add(&project.dir);
        self.buffers.keywords = project.file.metadata.keywords.join("; ");
        self.buffers.author_affiliations = project.file.metadata.authors.iter().map(|a| join_numbers(&a.affiliations)).collect();
        self.note(tf("Project: {path} (template {template})", &[("path", &project.dir.display().to_string()), ("template", &project.file.template)]));
        self.source = SourceEditor::default();
        self.source.load(&project.dir, &project.file.body);
        self.saved_project = Some(project.file.clone());
        self.project = Some(project);
        self.report = None;
        self.history.clear();
        self.preview.clear();
        self.refresh_checks();
        if self.page == Page::Dashboard || self.page == Page::Templates {
            self.page = Page::Manuscript;
            self.manuscript_tab = ManuscriptTab::Source;
        }
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
            project.write_body(&self.source.text)
        } else {
            project
                .invalidate_pdf()
                .and_then(|()| psg_academic::paths::write_atomic(&project.dir.join(&self.source.file), self.source.text.as_bytes()))
        };
        match result {
            Ok(()) => {
                self.source.mark_saved();
                self.note(tf("Saved {file}.", &[("file", &self.source.file)]));
                self.refresh_checks();
                true
            }
            Err(e) => {
                self.fail(tf("Could not save {file}: {error}", &[("file", &self.source.file), ("error", &e.to_string())]));
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
        self.page = Page::Manuscript;
        self.manuscript_tab = ManuscriptTab::Source;
        if self.source_view == SourceView::Preview {
            self.source_view = SourceView::Split;
        }
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
        if !self.prepare_project_switch() {
            return;
        }
        let name = format!("{}-{}", tpl.manifest.id, local_now().strftime("%Y%m%d-%H%M%S"));
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
        self.choose_folder(data_folder::FolderUse::NewProject);
    }

    fn create_project_in_folder(&mut self, folder: &std::path::Path) -> bool {
        let Some(tpl) = self.templates.get(self.selected_template).cloned() else { return false };
        if !self.prepare_project_switch() {
            return false;
        }
        let name = format!("{}-{}", tpl.manifest.id, local_now().strftime("%Y%m%d-%H%M%S"));
        let result = new_project_location(folder, &name)
            .map_err(|e| e.to_string())
            .and_then(|dir| Project::create(&dir, &tpl, PublicationMetadata::sample()).map_err(|e| e.to_string()));
        match result {
            Ok(p) => {
                self.set_project(p);
                true
            }
            Err(e) => {
                self.fail(tf("Could not create the project: {error}", &[("error", &e)]));
                false
            }
        }
    }

    fn open_project(&mut self) {
        let Some(dir) = rfd::FileDialog::new().set_title(t("Open a PSG Academic PDF project folder")).pick_folder() else { return };
        if !self.prepare_project_switch() {
            return;
        }
        match Project::open(&dir) {
            Ok(p) => {
                self.status = tf("Opened {path}.", &[("path", &p.dir.display().to_string())]);
                self.set_project(p);
            }
            Err(e) => self.fail(tf("Could not open the project: {error}", &[("error", &e.to_string())])),
        }
    }

    fn import_manuscript(&mut self) {
        if self.project.is_none() {
            return;
        }
        let Some(path) = rfd::FileDialog::new().set_title(t("Import the manuscript body (LaTeX)")).add_filter("LaTeX", &["tex"]).pick_file() else {
            return;
        };
        // What was typed is saved first, so the import starts from it.
        if !self.save_all() {
            return;
        }
        let Some(project) = &self.project else { return };
        let result = project.import_manuscript(&path);
        self.reload_after_import();
        self.after_import(result);
    }

    /// Show what an import into the open project changed: the body in the editor, and the
    /// metadata it may have filled in (title, authors, abstract, keywords).
    fn reload_after_import(&mut self) {
        let Some(project) = &self.project else { return };
        self.buffers.keywords = project.file.metadata.keywords.join("; ");
        self.buffers.author_affiliations = project.file.metadata.authors.iter().map(|a| join_numbers(&a.affiliations)).collect();
        self.saved_project = Some(project.file.clone());
        if self.source.file == project.file.body {
            let dir = project.dir.clone();
            let body = project.file.body.clone();
            self.source.load(&dir, &body);
        }
        self.refresh_checks();
    }

    fn import_docx(&mut self) {
        let Some(pandoc) = self.env.as_ref().and_then(|e| e.pandoc.path.clone()) else { return };
        if self.project.is_none() {
            return;
        }
        let Some(path) =
            rfd::FileDialog::new().set_title(t("Import a Word manuscript (experimental, via Pandoc)")).add_filter("Word", &["docx"]).pick_file()
        else {
            return;
        };
        if !self.save_all() {
            return;
        }
        let Some(project) = self.project.as_mut() else { return };
        let result = PandocImporter { pandoc }.import(&path, project, None);
        self.reload_after_import();
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
                    self.page = Page::Typesetting;
                    self.log_view = LogView::Activity;
                    self.queued.push((Tone::Danger, self.status.clone()));
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
        if !self.save_all() {
            return;
        }
        self.refresh_checks();
        let blocking = self.preflight.iter().filter(|d| d.severity == Severity::Error).count();
        if blocking > 0 {
            self.note(tn(blocking, "{n} problem found before compiling; see Problems.", "{n} problems found before compiling; see Problems.", &[]));
        }
        let (Some(project), Some(env)) = (self.project.clone(), self.env.clone()) else {
            self.fail(t("Open or create a project, and wait for the TeX check, before compiling."));
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let flag = cancel.clone();
        let repaint = ctx.clone();
        let managed = self.managed.clone();
        let spawned = std::thread::Builder::new().name("psg-compile".into()).spawn(move || {
            // PSG's TeX installs a package a document needs but doesn't have, then compiles again.
            let result = psg_academic::managed::compile_completing(&project, &env, managed.as_ref(), Some(&flag)).map_err(|e| e.to_string());
            let _ = tx.send(result);
            repaint.request_repaint();
        });
        match spawned {
            Ok(_) => {
                self.running = Some(Running { rx, cancel, started: Instant::now() });
                self.status = t("Typesetting with latexmk and pdfLaTeX…").into();
                self.log_view = LogView::Output;
                self.note(t("Compilation started."));
            }
            Err(e) => self.fail(tf("Could not start the compilation: {error}", &[("error", &e.to_string())])),
        }
    }

    fn export(&mut self) {
        if !self.check_export() {
            return;
        }
        let Some(project) = &self.project else { return };
        let suggested = suggested_file_name(&project.file.metadata);
        let Some(dest) = rfd::FileDialog::new().set_title(t("Export the PDF")).add_filter("PDF", &["pdf"]).set_file_name(suggested).save_file()
        else {
            return;
        };
        // The native dialog may have been open while another program changed the source files.
        if !self.check_export() {
            return;
        }
        let Some(project) = &self.project else { return };
        let result = project.export_pdf(&dest);
        match result {
            Ok(()) => {
                self.status = tf("Exported {path}.", &[("path", &dest.display().to_string())]);
                self.queued.push((Tone::Success, self.status.clone()));
                self.note(self.status.clone());
            }
            Err(e) => self.fail(tf("Export failed: {error}", &[("error", &e.to_string())])),
        }
    }

    fn open_in_psg_pdf(&mut self) {
        if !self.check_export() {
            return;
        }
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

    /// The toolbar: the product, the open project, and the four main actions.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        layout::toolbar(ui, "psg-toolbar", |ui| {
            widgets::product_name(ui, env!("CARGO_PKG_VERSION"));
            ui.add_space(psg_brand::space::S);
            let p = psg_desktop_ui::Palette::of(ui);
            let project = self.project.as_ref().map(project_title).unwrap_or_else(|| t("No project open").to_owned());
            ui.add(egui::Label::new(egui::RichText::new(project).color(p.text_muted)).truncate())
                .on_hover_text(self.project.as_ref().map(|p| p.dir.display().to_string()).unwrap_or_default());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let has_pdf = self.has_pdf();
                if widgets::button_enabled(ui, has_pdf, widgets::ButtonKind::Secondary, t("Export"), t("Typeset the project first."))
                    .on_hover_text(t("Check and export the finished PDF"))
                    .clicked()
                {
                    self.page = Page::Export;
                }
                if widgets::button_enabled(ui, has_pdf, widgets::ButtonKind::Secondary, t("Preview"), t("Typeset the project first."))
                    .on_hover_text(t("Show the PDF beside the manuscript"))
                    .clicked()
                {
                    self.show_preview(&ctx);
                }
                if let Some((cancel, started)) = self.running.as_ref().map(|r| (r.cancel.clone(), r.started)) {
                    if widgets::button(ui, widgets::ButtonKind::Danger, t("Cancel")).clicked() {
                        cancel.store(true, Ordering::Relaxed);
                        self.note(t("Cancelling…"));
                    }
                    ui.label(format!("{:.0} s", started.elapsed().as_secs_f32()));
                    ui.spinner();
                } else {
                    let why = self.why_not_compile();
                    let keys = psg_desktop_ui::keys::command("R", false);
                    if widgets::button_enabled(ui, self.can_compile(), widgets::ButtonKind::Primary, t("Auto Typeset"), why)
                        .on_hover_text(tf("Save, check and typeset with latexmk and pdfLaTeX ({keys})", &[("keys", &keys)]))
                        .clicked()
                    {
                        self.start_compile(&ctx);
                    }
                }
                self.import_menu(ui);
            });
        });
    }

    /// Import: a manuscript (new project when none is open), Word via Pandoc, or extra files.
    fn import_menu(&mut self, ui: &mut egui::Ui) {
        let label = widgets::button_widget(ui, widgets::ButtonKind::Secondary, t("Import"), psg_brand::size::CONTROL);
        let busy = self.running.is_some() || self.env.is_none();
        let pandoc_version = self.env.as_ref().and_then(|e| e.pandoc.version.clone());
        let pandoc_too_old = pandoc_version.as_deref().and_then(psg_academic::pandoc::sandbox_supported) == Some(false);
        let pandoc = self.env.as_ref().is_some_and(|e| e.pandoc.found()) && !pandoc_too_old;
        ui.add_enabled_ui(!busy, |ui| {
            egui::containers::menu::MenuButton::from_button(label).ui(ui, |ui| {
                ui.set_min_width(260.0);
                if self.project.is_none() {
                    if ui
                        .button(t("Manuscript as a new project…"))
                        .on_hover_text(t("A .tex or .docx file becomes the body of a new project"))
                        .clicked()
                    {
                        self.import_new();
                    }
                    ui.weak(t("Open or create a project to import into it."));
                    return;
                }
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
                ui.separator();
                if ui.button(t("Bibliography / figures…")).clicked() {
                    self.add_files();
                }
            });
        });
    }

    /// The navigation rail: one entry per area, with the number of problems on Problems.
    fn nav(&mut self, ui: &mut egui::Ui) {
        let compact = layout::is_compact(ui.ctx());
        layout::nav_rail(ui, "psg-nav", compact, |ui| {
            let (errors, warnings) = self.problem_counts();
            for page in Page::ALL {
                if page == Page::Settings {
                    // Settings sits at the bottom of the rail, apart from the workflow.
                    ui.add_space((ui.available_height() - 40.0).max(psg_brand::space::S));
                }
                let count = match page {
                    Page::Problems if errors > 0 => Some((errors, Tone::Danger)),
                    Page::Problems => Some((warnings, Tone::Warning)),
                    _ => None,
                };
                if widgets::nav_item(ui, page.icon(), page.label(), self.page == page, count, compact).clicked() {
                    self.page = page;
                }
            }
        });
    }

    /// The status bar: TeX, the engine, what happened last, and the language.
    fn status_bar(&mut self, ui: &mut egui::Ui) {
        layout::status_bar(ui, "psg-status", |ui| {
            let (tone, tex) = match &self.env {
                _ if self.tex_job.as_ref().is_some_and(|j| j.job == Job::Install) => (Tone::Info, t("Installing TeX…")),
                _ if self.tex_job.is_some() => (Tone::Info, t("Updating TeX…")),
                None => (Tone::Info, t("Checking TeX…")),
                Some(env) if env.missing(true).is_empty() => (Tone::Success, t("TeX ready")),
                Some(env) if env.missing(false).is_empty() => (Tone::Warning, t("TeX ready, no Biber")),
                Some(_) => (Tone::Danger, t("TeX not found")),
            };
            widgets::status_dot(ui, tone);
            if ui.add(egui::Label::new(tex).sense(egui::Sense::click())).on_hover_text(t("Settings ▸ TeX installation")).clicked() {
                self.page = Page::Settings;
            }
            ui.separator();
            ui.label(t("Engine: pdfLaTeX"));
            ui.separator();
            // The language and the unsaved mark first, from the right, so the last message
            // (long in German or Vietnamese) is shortened to the space between, never under them.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(crate::i18n::language().native_name);
                if self.source.dirty() {
                    ui.separator();
                    ui.label(t("Unsaved changes"));
                }
                ui.separator();
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(self.status.as_str()).truncate()).on_hover_text(self.status.as_str());
                });
            });
        });
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        use psg_desktop_ui::keys::consume_command;
        if consume_command(ctx, egui::Key::S, false) && self.save_all() && self.project.is_some() {
            self.notify(ctx, Tone::Success, t("Saved."));
        }
        if consume_command(ctx, egui::Key::R, false) && self.can_compile() {
            self.start_compile(ctx);
        }
        if consume_command(ctx, egui::Key::E, false) && self.has_pdf() {
            self.page = Page::Export;
        }
        if consume_command(ctx, egui::Key::O, false) && self.running.is_none() {
            self.open_project();
        }
        // Ctrl+1 … Ctrl+7 switch areas.
        let keys = [egui::Key::Num1, egui::Key::Num2, egui::Key::Num3, egui::Key::Num4, egui::Key::Num5, egui::Key::Num6, egui::Key::Num7];
        for (key, page) in keys.into_iter().zip(Page::ALL) {
            if consume_command(ctx, key, false) {
                self.page = page;
            }
        }
    }

    fn central(&mut self, ui: &mut egui::Ui) {
        let p = psg_desktop_ui::Palette::of(ui);
        let fill = if matches!(self.page, Page::Manuscript | Page::Typesetting) { p.panel } else { p.canvas };
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(fill).inner_margin(egui::Margin::same(psg_brand::space::L as i8))).show(
            ui,
            |ui| match self.page {
                Page::Dashboard => self.dashboard_page(ui),
                Page::Manuscript => self.manuscript_page(ui),
                Page::Templates => self.templates_page(ui),
                Page::Typesetting => self.typesetting_page(ui),
                Page::Problems => self.problems_page(ui),
                Page::Export => self.export_page(ui),
                Page::Settings => self.settings_page(ui),
            },
        );
    }
}

/// The project's title for the toolbar: the article's short title, else its title, else the folder.
fn project_title(project: &Project) -> String {
    let m = &project.file.metadata;
    [&m.short_title, &m.title]
        .into_iter()
        .map(|s| s.trim())
        .find(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| project.dir.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

/// The report's outcome in the interface language.
fn summary(report: &CompileReport) -> String {
    let secs = format!("{:.1}", report.elapsed.as_secs_f32());
    match report.outcome {
        Outcome::Succeeded => {
            tn(report.warnings().count(), "Typeset in {secs} s with {n} warning.", "Typeset in {secs} s with {n} warnings.", &[("secs", &secs)])
        }
        Outcome::Failed => tn(
            report.errors().count().max(1),
            "Typesetting failed after {secs} s with {n} error.",
            "Typesetting failed after {secs} s with {n} errors.",
            &[("secs", &secs)],
        ),
        Outcome::TimedOut => tf("Typesetting stopped: it took longer than {secs} s.", &[("secs", &secs)]),
        Outcome::Cancelled => t("Typesetting cancelled.").to_owned(),
    }
}

/// A problem's message (without its place) and its hint, if any. The advice for a script the templates can't set is
/// a sentence of its own, shown (translated) as the hint instead of inside the message.
fn problem_text(d: &Diagnostic) -> (String, Option<&'static str>) {
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
    // The file and line are shown beside the message, so only the message is returned.
    let text = advice.map_or(d.message.as_str(), |a| d.message.trim_end_matches(a).trim_end()).to_owned();
    (text, advice.or_else(|| d.hint()))
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
        self.poll_relocation(&ctx);
        self.open_pending();
        // A ready update waits while a compilation runs.
        let restart = self.updates.frame(&ctx, self.running.is_none() && !self.moving_data());
        if ctx.input(|i| i.viewport().close_requested()) && (self.moving_data() || !self.save_all()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if (restart || quit) && !self.moving_data() && self.save_all() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.running.is_some() || self.preview.rendering || self.env_rx.is_some() || self.tex_job.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        let now = ctx.input(|i| i.time);
        for (tone, text) in std::mem::take(&mut self.queued) {
            self.notices.push(tone, text, now);
        }
        self.keyboard(&ctx);
        self.toolbar(ui);
        self.status_bar(ui);
        self.nav(ui);
        self.central(ui);
        self.folder_dialog(&ctx);
        self.notices.show(&ctx);
        self.remember_layout(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(r) = &self.running {
            r.cancel.store(true, Ordering::Relaxed);
        }
        // What was typed into the metadata is kept, as the manuscript file is when it is saved.
        self.save_all();
        self.updates.exiting();
    }
}

/// An empty picked folder can be the project itself. In a workspace with existing files, create
/// a separate project folder to keep those files out of the manuscript and compilation.
fn new_project_location(folder: &std::path::Path, name: &str) -> std::io::Result<PathBuf> {
    if std::fs::read_dir(folder)?.next().transpose()?.is_some() { Ok(folder.join(name)) } else { Ok(folder.to_path_buf()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nonempty_selected_folder_can_hold_a_new_project() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(new_project_location(dir.path(), "article").unwrap(), dir.path());
        let existing = dir.path().join("existing.txt");
        std::fs::write(&existing, "notes").unwrap();
        let location = new_project_location(dir.path(), "article").unwrap();
        assert_eq!(location, dir.path().join("article"));
        let project = Project::create(&location, &Template::builtin().remove(0), PublicationMetadata::sample()).unwrap();
        assert_eq!(Project::open(&location).unwrap().dir, project.dir);
        assert_eq!(std::fs::read_to_string(existing).unwrap(), "notes");
    }

    #[test]
    fn a_failed_save_prevents_leaving_the_active_draft() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::resolve(None, Some(dir.path().join("data")));
        let mut app = AcademicApp::new(&egui::Context::default(), data, Vec::new());
        let article = dir.path().join("article");
        let project = Project::create(&article, &Template::builtin().remove(0), PublicationMetadata::sample()).unwrap();
        let body = article.join(&project.file.body);
        app.set_project(project);
        app.source.text = "unsaved edits".into();
        std::fs::remove_file(&body).unwrap();
        std::fs::create_dir(&body).unwrap();
        assert!(!app.prepare_project_switch());
        assert_eq!(app.project.as_ref().unwrap().dir, article);
        assert_eq!(app.source.text, "unsaved edits");
        assert!(app.source.dirty());
        std::fs::remove_dir(&body).unwrap();
        assert!(app.prepare_project_switch());
        assert_eq!(std::fs::read_to_string(body).unwrap(), "unsaved edits");
    }

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
