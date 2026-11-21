//! Settings ▸ Data folder: where projects and settings are kept, and moving them somewhere else.
//!
//! The copying is done by [`crate::relocate`] on a worker thread. While it runs the project is
//! closed (and saved first), so nothing is written to the old folder halfway through; when it
//! finishes the app switches to the new folder at once and opens the project again from there.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use psg_desktop_ui::{Tone, widgets};

use super::{APP_NAME, AcademicApp};
use crate::i18n::{t, tf};
use crate::recent::Recent;
use crate::relocate::{self, Error, Moved};

/// Where a change of data folder is.
#[derive(Default)]
pub(super) enum Relocation {
    #[default]
    Idle,
    /// The folder was picked; waiting for the person to confirm.
    Confirm(PathBuf),
    /// Copying. `reopen` is the project that was open, in the old folder.
    Moving { rx: Receiver<Result<Moved, Error>>, reopen: Option<PathBuf> },
}

#[derive(Clone, Copy)]
pub(super) enum FolderUse {
    Workspace,
    NewProject,
}

pub(super) struct FolderChoice {
    use_for: FolderUse,
    path: String,
}

impl AcademicApp {
    pub(super) fn workspace_card(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, t("Workspace folder"));
        widgets::muted(ui, t("New projects are saved here."));
        ui.label(egui::RichText::new(self.workspace.display().to_string()).monospace().small());
        let chosen = self.data.chosen_workspace();
        if let Some(path) = &chosen
            && !path.is_dir()
        {
            widgets::banner(ui, Tone::Warning, &tf("The chosen workspace is unavailable: {path}", &[("path", &path.display().to_string())]), None);
        }
        ui.horizontal_wrapped(|ui| {
            let busy = self.moving_data();
            let why = t("Moving the data folder…");
            if widgets::button_enabled(ui, !busy, widgets::ButtonKind::Secondary, t("Choose workspace…"), why).clicked() {
                self.choose_workspace();
            }
            if chosen.is_some() && widgets::button_enabled(ui, !busy, widgets::ButtonKind::Ghost, t("Use the default workspace"), why).clicked() {
                self.set_workspace(None);
            }
        });
    }

    pub(super) fn choose_workspace(&mut self) {
        self.choose_folder(FolderUse::Workspace);
    }

    pub(super) fn choose_folder(&mut self, use_for: FolderUse) {
        if self.moving_data() {
            return;
        }
        self.folder_choice = Some(FolderChoice { use_for, path: self.workspace.display().to_string() });
    }

    pub(super) fn folder_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut choice) = self.folder_choice.take() else { return };
        let title = match choice.use_for {
            FolderUse::Workspace => t("Choose workspace…"),
            FolderUse::NewProject => t("Choose a folder for the new project"),
        };
        let modal = egui::Modal::new(egui::Id::unique("psg-academic-folder-choice")).show(ctx, |ui| {
            ui.set_max_width(540.0);
            widgets::section_header(ui, title);
            ui.label(t("Folder path"));
            ui.horizontal_wrapped(|ui| {
                crate::clipboard::text_edit(ui, egui::TextEdit::singleline(&mut choice.path).desired_width(365.0));
                if widgets::secondary(ui, t("Browse…")).clicked() {
                    let directory = Path::new(&choice.path);
                    let dialog = rfd::FileDialog::new().set_title(title);
                    let dialog = if directory.is_dir() { dialog.set_directory(directory) } else { dialog };
                    if let Some(folder) = dialog.pick_folder() {
                        choice.path = folder.display().to_string();
                    }
                }
            });
            ui.add_space(psg_brand::space::S);
            ui.horizontal(|ui| {
                let folder = Path::new(&choice.path);
                let valid = folder.is_absolute() && folder.is_dir();
                let apply =
                    widgets::button_enabled(ui, valid, widgets::ButtonKind::Primary, t("Use this folder"), t("Enter an existing folder path."))
                        .clicked();
                let cancel = widgets::ghost(ui, t("Cancel")).clicked();
                (apply, cancel)
            })
            .inner
        });
        let (apply, cancel) = modal.inner;
        if apply {
            let folder = Path::new(&choice.path);
            let accepted = match choice.use_for {
                FolderUse::Workspace => self.set_workspace(Some(folder)),
                FolderUse::NewProject => self.create_project_in_folder(folder),
            };
            if accepted {
                return;
            }
        } else if cancel || modal.should_close() {
            return;
        }
        self.folder_choice = Some(choice);
    }

    fn set_workspace(&mut self, folder: Option<&Path>) -> bool {
        match self.data.remember_workspace(folder) {
            Ok(()) => {
                self.workspace = self.data.projects();
                self.status = tf("Workspace folder set to {path}.", &[("path", &self.workspace.display().to_string())]);
                self.queued.push((Tone::Success, self.status.clone()));
                true
            }
            Err(e) => {
                self.fail(tf("Could not change the workspace: {error}", &[("error", &e.to_string())]));
                false
            }
        }
    }

    pub(super) fn data_folder_card(&mut self, ui: &mut egui::Ui) {
        widgets::section_header(ui, t("Data folder"));
        widgets::muted(ui, t("New projects and the app's settings are kept in this folder."));
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(self.data.root.display().to_string()).monospace().small());
            if self.data.is_custom() {
                widgets::chip(ui, Tone::Info, t("Chosen"));
            } else {
                widgets::chip(ui, Tone::Neutral, t("Default"));
            }
        });
        ui.add_space(psg_brand::space::S);
        match &self.relocation {
            Relocation::Moving { .. } => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(t("Moving the data folder…"));
                });
            }
            Relocation::Confirm(to) => {
                let to = to.clone();
                ui.label(tf("Move the projects and settings to {path}?", &[("path", &to.display().to_string())]));
                widgets::muted(ui, t("Everything is copied and checked first; the old copies are removed only after that."));
                ui.horizontal(|ui| {
                    if widgets::primary(ui, t("Move data")).clicked() {
                        self.relocation = Relocation::Idle;
                        self.start_move(ui.ctx(), to);
                    }
                    if widgets::ghost(ui, t("Cancel")).clicked() {
                        self.relocation = Relocation::Idle;
                    }
                });
            }
            Relocation::Idle => {
                let busy = self.running.is_some();
                let why = if busy { t("Typesetting is already running.") } else { "" };
                ui.horizontal_wrapped(|ui| {
                    if widgets::button_enabled(ui, !busy, widgets::ButtonKind::Secondary, t("Change…"), why).clicked() {
                        self.choose_data_folder();
                    }
                    if self.data.is_custom()
                        && widgets::button_enabled(ui, !busy, widgets::ButtonKind::Ghost, t("Use the default folder"), why)
                            .on_hover_text(self.data.home.display().to_string())
                            .clicked()
                    {
                        self.relocation = Relocation::Confirm(self.data.home.clone());
                    }
                    if widgets::ghost(ui, t("Open folder")).clicked()
                        && let Err(e) = crate::psg_pdf::open_folder(&self.data.root)
                    {
                        self.fail(e);
                    }
                });
            }
        }
    }

    /// Pick the new place and ask to confirm, or say at once why it can't be used.
    fn choose_data_folder(&mut self) {
        let Some(chosen) = rfd::FileDialog::new().set_title(t("Choose where to keep projects and settings")).pick_folder() else { return };
        let to = relocate::destination(&chosen, APP_NAME);
        match relocate::check(&self.data.root, &to) {
            Ok(()) => self.relocation = Relocation::Confirm(to),
            Err(e) => self.fail(describe(&e)),
        }
    }

    fn start_move(&mut self, ctx: &egui::Context, to: PathBuf) {
        // The project keeps what was typed, and is closed so nothing writes to the old folder.
        if !self.save_all() {
            return;
        }
        let reopen = self.project.take().map(|p| p.dir);
        self.preview.clear();
        self.report = None;
        self.source = crate::source::SourceEditor::default();
        let (tx, rx) = channel();
        let (from, home, repaint) = (self.data.root.clone(), self.data.home.clone(), ctx.clone());
        let spawned = std::thread::Builder::new().name("psg-data-move".into()).spawn(move || {
            let _ = tx.send(relocate::move_data(&from, &to, &home));
            repaint.request_repaint();
        });
        match spawned {
            Ok(_) => {
                self.note(tf("Moving the data folder from {path}.", &[("path", &self.data.root.display().to_string())]));
                self.relocation = Relocation::Moving { rx, reopen };
            }
            Err(e) => {
                self.fail(tf("Could not move the data folder: {error}", &[("error", &e.to_string())]));
                self.reopen(reopen);
            }
        }
    }

    /// Called every frame: finishes a move that is done.
    pub(super) fn poll_relocation(&mut self, ctx: &egui::Context) {
        let Relocation::Moving { rx, .. } = &self.relocation else { return };
        let result = match rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err(Error::Io { path: self.data.root.clone(), message: "the copying stopped unexpectedly".into() })
            }
        };
        let Relocation::Moving { reopen, .. } = std::mem::take(&mut self.relocation) else { return };
        match result {
            Ok(moved) => {
                let from = std::mem::replace(&mut self.data.root, moved.to.clone());
                self.use_data_root();
                let reopen = reopen.map(|p| relocate::rebase(&p, &from, &moved.to).unwrap_or(p));
                self.notify(ctx, Tone::Success, tf("Projects and settings are now kept in {path}.", &[("path", &moved.to.display().to_string())]));
                if moved.left_behind > 0 {
                    self.notify(
                        ctx,
                        Tone::Warning,
                        tf(
                            "Everything was copied, but some old files could not be removed from {path}. You can delete them yourself.",
                            &[("path", &from.display().to_string())],
                        ),
                    );
                }
                self.reopen(reopen);
            }
            Err(e) => {
                self.fail(describe(&e));
                self.reopen(reopen);
            }
        }
    }

    /// Read and write everything in `self.data.root` from now on.
    fn use_data_root(&mut self) {
        let root = self.data.root.clone();
        self.workspace = self.data.projects();
        self.language_file = root.join("language.txt");
        self.theme_file = root.join("theme.txt");
        self.updates.set_channel_file(root.join("update-channel.txt"));
        self.recent = Recent::load(root.join(relocate::RECENT_FILE));
    }

    fn reopen(&mut self, dir: Option<PathBuf>) {
        let Some(dir) = dir else { return };
        match psg_academic::Project::open(&dir) {
            Ok(p) => self.set_project(p),
            Err(e) => self.fail(tf("Could not open the project: {error}", &[("error", &e.to_string())])),
        }
    }
}

/// What to tell the person about a move that didn't happen. Nothing was changed.
fn describe(e: &Error) -> String {
    let path = |p: &Path| p.display().to_string();
    match e {
        Error::NotAbsolute(p) => tf("{path} is not a full path. Choose a folder.", &[("path", &path(p))]),
        Error::Same => t("That is the folder already in use.").to_owned(),
        Error::Inside(p) => tf("{path} is inside the data folder. Choose a folder outside it.", &[("path", &path(p))]),
        Error::Exists(p) => tf("{path} already exists there. Nothing was moved; choose an empty folder.", &[("path", &path(p))]),
        Error::TooLarge => t("That folder holds too many files to be a data folder. Nothing was moved.").to_owned(),
        Error::Io { path: p, message } => {
            tf("Could not move the data folder ({path}: {error}). Nothing was changed.", &[("path", &path(p)), ("error", message)])
        }
    }
}
