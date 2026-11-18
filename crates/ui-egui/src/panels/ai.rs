//! The AI assistant: Preferences ▸ AI assistant, where people add the providers they use with
//! their own API keys, and the All tools ▸ AI assistant panel that summarizes, answers questions
//! about and translates the open document with the provider in use.
//!
//! Nothing is sent anywhere until a provider is added and an action is chosen. Keys go to the OS
//! credential store (`psg_ai::secret`), never into the settings file. Requests run on a worker
//! thread; the panel keeps drawing and can drop a request it no longer wants.

use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use egui::{Color32, CornerRadius, RichText};
use psg_ai::secret::{self, Saved, Store};
use psg_ai::{DocText, Message, Profile, Settings, Task, Wire};

use crate::dialogs::preferences::PrefSearchContext;
use crate::theme::{self, Tokens};
use crate::{PdfCraftApp, widgets};
use pdfcraft_engine::DocId;

/// How long a reply the assistant asks for when the profile leaves it open: Anthropic needs a
/// number, and a translation of a few pages fits.
const PANEL_MAX_TOKENS: u32 = 8192;

/// One line of a conversation as the panel shows it.
#[derive(Clone)]
pub(crate) struct Turn {
    /// What the model saw (kept for follow-up questions).
    pub message: Message,
    /// What the panel shows ("Summarize", the question itself, the answer).
    pub shown: String,
    pub truncated: bool,
}

/// A request on its way.
struct Job {
    doc: DocId,
    receiver: mpsc::Receiver<Result<(psg_ai::Reply, DocText), String>>,
    cancel: Arc<AtomicBool>,
    /// The person's turn, added to the conversation when the answer arrives.
    turn: Turn,
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

/// A test or model-list request from Preferences.
enum Check {
    Test(mpsc::Receiver<Result<String, String>>),
    Models(mpsc::Receiver<Result<Vec<String>, String>>),
}

/// Preferences ▸ AI assistant: the provider being added or edited.
pub(crate) struct Editor {
    pub profile: Profile,
    /// A new key typed in; empty keeps the saved one.
    pub key: String,
    pub show_key: bool,
    pub is_new: bool,
    /// The provider's own model names, once fetched.
    pub models: Vec<String>,
    check: Option<Check>,
    /// The last test or fetch: Ok(note) or Err(problem).
    pub status: Option<Result<String, String>>,
}

/// Everything the assistant keeps.
pub(crate) struct AiState {
    pub settings: Settings,
    pub secrets: Arc<dyn Store>,
    pub editor: Option<Editor>,
    /// Each open document's conversation.
    pub chats: HashMap<DocId, Vec<Turn>>,
    job: Option<Job>,
    /// The last request's failure, per document.
    pub errors: HashMap<DocId, String>,
    pub question: String,
    /// The language Translate writes in (a `psg_i18n::LANGUAGES` code); empty is the interface's.
    pub translate_to: String,
    /// Current page only, rather than the whole document.
    pub current_page_only: bool,
    /// A notice about the last saved key ("kept for this session only").
    pub key_note: Option<String>,
    /// An action chosen from the command palette or a menu, run once the panel draws.
    pub pending: Option<Task>,
}

impl Default for AiState {
    fn default() -> Self {
        // `PSG_AI_KEYCHAIN=0` keeps keys in memory (tests, and machines without a keychain).
        let memory = std::env::var_os("PSG_AI_KEYCHAIN").is_some_and(|v| v == "0");
        let secrets: Arc<dyn Store> = if memory { Arc::new(secret::Memory::default()) } else { Arc::new(super::ai_net::Keychain::default()) };
        Self {
            settings: Settings::default(),
            secrets,
            editor: None,
            chats: HashMap::new(),
            job: None,
            errors: HashMap::new(),
            question: String::new(),
            translate_to: String::new(),
            current_page_only: false,
            key_note: None,
            pending: None,
        }
    }
}

impl AiState {
    /// Keep keys in memory only (tests).
    #[cfg(test)]
    pub fn use_memory_keys(&mut self) {
        self.secrets = Arc::new(secret::Memory::default());
    }

    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// The panel's conversation for `doc`.
    pub fn chat(&self, doc: DocId) -> &[Turn] {
        self.chats.get(&doc).map_or(&[], Vec::as_slice)
    }

    /// Take a finished answer, if one arrived.
    fn poll(&mut self) {
        let Some(job) = &self.job else { return };
        let outcome = match job.receiver.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err(tl!("The request stopped unexpectedly.").to_string()),
        };
        let Some(job) = self.job.take() else { return };
        match outcome {
            Ok((reply, doc_text)) => {
                let chat = self.chats.entry(job.doc).or_default();
                chat.push(job.turn.clone());
                let mut shown = reply.text.trim().to_string();
                if doc_text.truncated {
                    shown.push_str("\n\n");
                    shown.push_str(&crate::i18n::fmt(
                        tl!("(Only the first {n} of {total} pages fit in the request.)"),
                        &[("n", &doc_text.pages_included.to_string()), ("total", &doc_text.pages_requested.to_string())],
                    ));
                }
                chat.push(Turn { message: Message::assistant(reply.text), shown, truncated: reply.truncated });
                self.errors.remove(&job.doc);
            }
            Err(e) => {
                self.errors.insert(job.doc, e);
            }
        }
    }

    fn poll_editor(&mut self) {
        let Some(editor) = &mut self.editor else { return };
        let Some(check) = &editor.check else { return };
        match check {
            Check::Test(rx) => match rx.try_recv() {
                Ok(r) => {
                    editor.status = Some(r);
                    editor.check = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => editor.check = None,
                Err(mpsc::TryRecvError::Empty) => {}
            },
            Check::Models(rx) => match rx.try_recv() {
                Ok(Ok(models)) => {
                    editor.status = Some(Ok(crate::i18n::fmt(tl!("Found {n} models."), &[("n", &models.len().to_string())])));
                    if editor.profile.model.trim().is_empty()
                        && let Some(first) = models.first()
                    {
                        editor.profile.model = first.clone();
                    }
                    editor.models = models;
                    editor.check = None;
                }
                Ok(Err(e)) => {
                    editor.status = Some(Err(e));
                    editor.check = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => editor.check = None,
                Err(mpsc::TryRecvError::Empty) => {}
            },
        }
    }
}

/// An error from `psg_ai`, in the interface language, with the provider's own words after it.
pub(crate) fn error_text(e: &psg_ai::Error) -> String {
    use psg_ai::Error as E;
    let with = |template: &str, detail: &str| crate::i18n::fmt(template, &[("detail", detail)]);
    match e {
        E::MissingKey => tl!("No API key is saved for this provider. Add one in Preferences > AI assistant.").to_string(),
        E::Provider { status: 401 | 403, message } => {
            with(tl!("The provider refused the API key: {detail}"), &format!("{message} ({})", status_of(e)))
        }
        E::Provider { status: 404, message } => with(tl!("The provider doesn't know this model or address: {detail}"), message),
        E::Provider { status: 429, message } => with(tl!("The provider's rate limit or quota was reached: {detail}"), message),
        E::Provider { status, message } => with(tl!("The provider answered with an error: {detail}"), &format!("{message} (HTTP {status})")),
        E::Network(m) => with(tl!("Couldn't reach the provider: {detail}"), m),
        E::Reply(m) => with(tl!("The provider's answer couldn't be read: {detail}"), m),
        E::Config(m) => with(tl!("This provider's settings need attention: {detail}"), m),
        E::Keychain(m) => with(tl!("The system keychain refused: {detail}"), m),
        E::Unsupported => tl!("AI providers aren't available in this version.").to_string(),
    }
}

fn status_of(e: &psg_ai::Error) -> String {
    match e {
        psg_ai::Error::Provider { status, .. } => format!("HTTP {status}"),
        _ => String::new(),
    }
}

/// The key for `profile`: a new one being typed, the saved one, or `PSG_AI_API_KEY`.
fn key_for(secrets: &dyn Store, profile: &Profile, typed: Option<&str>) -> Result<Option<String>, psg_ai::Error> {
    if let Some(k) = typed.map(str::trim).filter(|k| !k.is_empty()) {
        return Ok(Some(k.to_string()));
    }
    if profile.has_key
        && let Some(k) = secrets.load(&profile.id)?
    {
        return Ok(Some(k));
    }
    Ok(std::env::var(secret::KEY_ENV).ok().filter(|k| !k.trim().is_empty()))
}

/// The name of an API format in the interface language (the others are product names).
fn wire_label(wire: Wire) -> &'static str {
    match wire {
        Wire::OpenAi => tl!("OpenAI-compatible"),
        other => other.label(),
    }
}

/// The English name of a language code, as the request words it.
fn language_name(code: &str) -> &'static str {
    psg_i18n::LANGUAGES.iter().find(|l| l.code == code).map_or("English", |l| l.english_name)
}

fn interface_code() -> &'static str {
    crate::i18n::current().code()
}

/// Run `work` on a worker thread (inline in headless test runs), repainting when it is done.
fn spawn(ctx: Option<egui::Context>, inline: bool, name: &str, work: impl FnOnce() + Send + 'static) {
    let finish = move || {
        work();
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    };
    #[cfg(not(target_arch = "wasm32"))]
    if !inline {
        if let Err(e) = std::thread::Builder::new().name(name.into()).spawn(finish) {
            log::warn!("couldn't start the AI request: {e}");
        }
        return;
    }
    let _ = (inline, name);
    finish();
}

impl PdfCraftApp {
    /// Start `task` on the active document with the provider in use.
    pub(crate) fn ai_run(&mut self, task: Task) {
        self.ai.poll();
        let Some((view, id)) = self.active_ids() else {
            self.notify(tl!("Open a document first."));
            return;
        };
        let Some(profile) = self.ai.settings.active_profile().cloned() else {
            self.ai.errors.insert(id, tl!("Add an AI provider in Preferences > AI assistant first.").to_string());
            return;
        };
        let Some(doc) = self.session.get(id) else { return };
        if let Err(e) = doc.check_copying() {
            self.ai.errors.insert(id, e.to_string());
            return;
        }
        let source = doc.export_source();
        let title = doc.display_name();
        let pages: Vec<usize> = if self.ai.current_page_only { vec![self.views[view].current] } else { (0..source.pages).collect() };
        let history: Vec<Message> = self.ai.chat(id).iter().map(|t| t.message.clone()).collect();
        let shown = match &task {
            Task::Summarize => tl!("Summarize").to_string(),
            Task::Ask(q) => q.trim().to_string(),
            Task::Translate(lang) => {
                let native = psg_i18n::LANGUAGES.iter().find(|l| l.english_name == lang).map_or(lang.as_str(), |l| l.native_name);
                crate::i18n::fmt(tl!("Translate into {language}"), &[("language", native)])
            }
        };
        let turn = Turn { message: Message::user(psg_ai::task::prompt(&task)), shown, truncated: false };
        let language = language_name(interface_code()).to_string();
        let secrets = self.ai.secrets.clone();
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.ai.errors.remove(&id);
        self.ai.job = Some(Job { doc: id, receiver: rx, cancel: cancel.clone(), turn });
        let max_tokens = if profile.max_tokens > 0 { profile.max_tokens } else { PANEL_MAX_TOKENS };
        let work = move || {
            let outcome = (|| {
                let key = key_for(secrets.as_ref(), &profile, None).map_err(|e| error_text(&e))?;
                let requested = pages.len();
                let mut exporter = pdfcraft_engine::export::Exporter::from_source(source);
                let texts = pages.into_iter().map_while(|p| {
                    if cancel.load(Ordering::Relaxed) {
                        return None;
                    }
                    // A page that can't be read counts as blank rather than failing the request.
                    let text = pdfcraft_engine::guard(|| exporter.text(p)).ok().and_then(Result::ok).unwrap_or_default();
                    Some((p, text))
                });
                let doc_text = DocText::from_pages(texts, requested, psg_ai::task::DEFAULT_DOC_CHARS);
                if cancel.load(Ordering::Relaxed) {
                    return Err(String::new());
                }
                if doc_text.is_blank() {
                    return Err(tl!("This document has no text to send. Scanned pages need Scan & OCR first.").to_string());
                }
                let request = psg_ai::task::build_request(&task, &doc_text, &history, &language, &title, max_tokens);
                let reply = psg_ai::net::chat(&profile, key.as_deref(), &request, &super::ai_net::send).map_err(|e| error_text(&e))?;
                Ok((reply, doc_text))
            })();
            if !cancel.load(Ordering::Relaxed) {
                let _ = tx.send(outcome);
            }
        };
        spawn(self.ctx.clone(), self.run_inline, "ai-request", work);
        self.ai.poll();
    }

    /// The palette's and menus' AI commands: open the panel and run the action there (Ask opens
    /// the panel for the question).
    pub(crate) fn ai_command(&mut self, command: &str) -> bool {
        let task = match command {
            "ai.summary" => Some(Task::Summarize),
            "ai.translate" => Some(Task::Translate(language_name(self.ai_translate_code()).to_string())),
            "ai.ask" => None,
            _ => return false,
        };
        self.select_tool("ai");
        if self.ai.settings.active_profile().is_none() {
            self.dialog = Some(crate::Dialog::Preferences);
        } else if let Some(task) = task {
            self.ai.pending = Some(task);
        }
        true
    }

    fn ai_translate_code(&self) -> &'static str {
        psg_i18n::LANGUAGES.iter().find(|l| l.code == self.ai.translate_to).map_or_else(interface_code, |l| l.code)
    }

    /// Save the provider being edited: its key to the credential store, the rest to settings.
    pub(crate) fn ai_save_editor(&mut self) -> Result<(), String> {
        let Some(editor) = &mut self.ai.editor else { return Ok(()) };
        let mut profile = editor.profile.clone();
        profile.name = profile.name.trim().to_string();
        profile.base_url = profile.base_url.trim().to_string();
        profile.model = profile.model.trim().to_string();
        if let Err(e) = profile.check() {
            return Err(error_text(&e));
        }
        let key = editor.key.trim().to_string();
        self.ai.key_note = None;
        if !key.is_empty() {
            match self.ai.secrets.save(&profile.id, &key) {
                Ok(Saved::Keychain) => {}
                Ok(Saved::SessionOnly) => {
                    self.ai.key_note = Some(tl!("The system keychain isn't available, so the key is kept only until PSG PDF quits.").to_string());
                }
                Err(e) => return Err(error_text(&e)),
            }
            profile.has_key = true;
        }
        self.ai.settings.upsert(profile).map_err(|e| error_text(&e))?;
        self.ai.editor = None;
        Ok(())
    }

    /// Remove a provider and its saved key.
    pub(crate) fn ai_remove(&mut self, id: &str) {
        if let Err(e) = self.ai.secrets.delete(id) {
            log::warn!("couldn't remove the AI key from the keychain: {e}");
        }
        self.ai.settings.remove(id);
        if self.ai.editor.as_ref().is_some_and(|e| e.profile.id == id) {
            self.ai.editor = None;
        }
    }

    /// Start a new provider from `preset`.
    pub(crate) fn ai_add(&mut self, preset: &psg_ai::Preset) {
        let seed = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(1, |d| d.as_millis() as u64);
        let id = self.ai.settings.new_id(seed);
        let profile = Profile::from_preset(preset, id);
        self.ai.editor = Some(Editor { profile, key: String::new(), show_key: false, is_new: true, models: Vec::new(), check: None, status: None });
    }

    fn ai_editor_check(&mut self, models: bool) {
        let Some(editor) = &mut self.ai.editor else { return };
        let profile = editor.profile.clone();
        let typed = editor.key.clone();
        let secrets = self.ai.secrets.clone();
        editor.status = None;
        let ctx = self.ctx.clone();
        if models {
            let (tx, rx) = mpsc::channel();
            editor.check = Some(Check::Models(rx));
            spawn(ctx, self.run_inline, "ai-models", move || {
                let r = key_for(secrets.as_ref(), &profile, Some(&typed))
                    .and_then(|key| psg_ai::net::models(&profile, key.as_deref(), &super::ai_net::send))
                    .map_err(|e| error_text(&e));
                let _ = tx.send(r);
            });
        } else {
            let (tx, rx) = mpsc::channel();
            editor.check = Some(Check::Test(rx));
            spawn(ctx, self.run_inline, "ai-test", move || {
                let request =
                    psg_ai::Request { system: String::new(), messages: vec![Message::user("Reply with the single word OK.")], max_tokens: 16 };
                let r = key_for(secrets.as_ref(), &profile, Some(&typed))
                    .and_then(|key| psg_ai::net::chat(&profile, key.as_deref(), &request, &super::ai_net::send))
                    .map(|_| tl!("Connected: the provider answered.").to_string())
                    .map_err(|e| error_text(&e));
                let _ = tx.send(r);
            });
        }
        self.ai.poll_editor();
    }
}

fn notice(ui: &mut egui::Ui, t: &Tokens, fill: Color32, text: &str) {
    egui::Frame::NONE.fill(fill).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(RichText::new(text).color(t.text).font(theme::regular(12.0)));
    });
}

fn privacy_line(profile: &Profile) -> String {
    let local = profile.preset().is_some_and(|p| p.local) || psg_ai::provider::is_loopback(&profile.base_url);
    if local {
        tl!("This provider runs on your computer: the document stays here.").to_string()
    } else {
        crate::i18n::fmt(tl!("The text of this document is sent to {provider}."), &[("provider", profile.display_name())])
    }
}

/// All tools ▸ AI assistant.
pub(crate) fn panel(app: &mut PdfCraftApp, ui: &mut egui::Ui, t: &Tokens) {
    app.ai.poll();
    if app.ai.busy() {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
    if app.ai.settings.profiles.is_empty() {
        notice(
            ui,
            t,
            t.accent_soft,
            tl!(
                "Use the AI service you choose, with your own API key: OpenAI, Anthropic Claude, Google Gemini, Azure OpenAI, DeepSeek, Qwen, a model on your own computer through Ollama, or any OpenAI-compatible service."
            ),
        );
        ui.add_space(8.0);
        if widgets::pill_button(ui, tl!("Add a provider"), true).clicked() {
            app.dialog = Some(crate::Dialog::Preferences);
        }
        return;
    }
    // Which provider.
    let active = app.ai.settings.active.clone();
    let current = app.ai.settings.active_profile().map_or_else(|| tl!("None").to_string(), |p| p.display_name().to_string());
    ui.horizontal(|ui| {
        let label = ui.label(RichText::new(tl!("Provider")).color(t.text_muted).font(theme::regular(12.0)));
        let mut chosen = active.clone();
        craft_ui::ComboBox::from_id_salt("ai-provider")
            .selected_text(current)
            .width((ui.available_width() - 34.0).max(80.0))
            .show_ui(ui, |ui| {
                for p in &app.ai.settings.profiles {
                    ui.selectable_value(&mut chosen, Some(p.id.clone()), format!("{} · {}", p.display_name(), p.model));
                }
            })
            .response
            .labelled_by(label.id);
        if chosen != active {
            app.ai.settings.active = chosen;
        }
        if widgets::ghost_button(ui, "settings", "").on_hover_text(tl!("AI assistant settings")).clicked() {
            app.dialog = Some(crate::Dialog::Preferences);
        }
    });
    let Some(profile) = app.ai.settings.active_profile().cloned() else { return };
    let Some((_, id)) = app.active_ids() else {
        ui.add_space(6.0);
        ui.label(RichText::new(tl!("Open a document to use the assistant.")).color(t.text_muted).font(theme::regular(12.0)));
        return;
    };
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        widgets::radio_value(ui, &mut app.ai.current_page_only, false, tl!("Whole document"));
        widgets::radio_value(ui, &mut app.ai.current_page_only, true, tl!("Current page"));
    });
    ui.add_space(4.0);
    let busy = app.ai.busy();
    let mut run = app.ai.pending.take();
    ui.add_enabled_ui(!busy, |ui| {
        if widgets::icon_pill(ui, "sparkles", tl!("Summarize"), false).clicked() {
            run = Some(Task::Summarize);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let code = app.ai_translate_code();
            let label = ui.label(RichText::new(tl!("Translate into")).color(t.text_muted).font(theme::regular(12.0)));
            let mut to = code.to_string();
            let shown = psg_i18n::LANGUAGES.iter().find(|l| l.code == to).map_or("", |l| l.native_name);
            craft_ui::ComboBox::from_id_salt("ai-translate-to")
                .selected_text(shown)
                .width(130.0)
                .show_ui(ui, |ui| {
                    for l in &psg_i18n::LANGUAGES {
                        ui.selectable_value(&mut to, l.code.to_string(), l.native_name);
                    }
                })
                .response
                .labelled_by(label.id);
            if widgets::icon_pill(ui, "languages", tl!("Translate"), false).clicked() {
                run = Some(Task::Translate(language_name(&to).to_string()));
            }
            if to != code {
                app.ai.translate_to = to;
            }
        });
    });
    ui.add_space(6.0);
    // The conversation.
    let input_h = 150.0;
    let chat_h = (ui.available_height() - input_h).max(80.0);
    egui::ScrollArea::vertical().id_salt("ai-chat").auto_shrink([false, false]).max_height(chat_h).stick_to_bottom(true).show(ui, |ui| {
        for turn in app.ai.chat(id) {
            let mine = turn.message.role == psg_ai::Role::User;
            let who = if mine { tl!("You") } else { profile.display_name() };
            ui.label(RichText::new(who).color(if mine { t.text_muted } else { t.accent }).font(theme::semibold(11.0)));
            let fill = if mine { t.pressed } else { t.card };
            egui::Frame::NONE
                .fill(fill)
                .stroke(egui::Stroke::new(1.0, t.border))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(egui::Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.add(egui::Label::new(RichText::new(&turn.shown).color(t.text).font(theme::regular(12.5))).selectable(true).wrap());
                    if turn.truncated {
                        ui.label(RichText::new(tl!("The answer stopped at the length limit.")).color(t.warning).font(theme::regular(11.0)));
                    }
                    if !mine && widgets::ghost_button(ui, "copy", tl!("Copy")).clicked() {
                        ui.ctx().copy_text(turn.message.content.clone());
                    }
                });
            ui.add_space(6.0);
        }
        if busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(
                    RichText::new(crate::i18n::fmt(tl!("Waiting for {provider}…"), &[("provider", profile.display_name())])).color(t.text_muted),
                );
            });
        }
        if let Some(e) = app.ai.errors.get(&id) {
            ui.label(RichText::new(e).color(t.danger).font(theme::regular(12.0)));
        }
    });
    ui.separator();
    // Ask.
    let label = ui.label(RichText::new(tl!("Ask about this document")).color(t.text_muted).font(theme::regular(12.0)));
    let field = ui
        .add_enabled(!busy, widgets::block(&mut app.ai.question).desired_rows(2).desired_width(f32::INFINITY).hint_text(tl!("Type a question")))
        .labelled_by(label.id);
    let enter = field.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
    ui.horizontal(|ui| {
        if busy {
            if widgets::pill_button(ui, tl!("Cancel"), false).clicked() {
                app.ai.job = None;
            }
        } else {
            let can_ask = !app.ai.question.trim().is_empty();
            if (ui.add_enabled(can_ask, egui::Button::new(tl!("Ask"))).clicked() || (enter && can_ask)) && run.is_none() {
                run = Some(Task::Ask(std::mem::take(&mut app.ai.question).trim().to_string()));
            }
            if !app.ai.chat(id).is_empty() && widgets::ghost_button(ui, "trash-2", tl!("Clear")).clicked() {
                app.ai.chats.remove(&id);
                app.ai.errors.remove(&id);
            }
        }
    });
    ui.label(RichText::new(privacy_line(&profile)).color(t.text_faint).font(theme::regular(11.0)));
    if let Some(task) = run.filter(|_| !busy) {
        app.ai_run(task);
    }
}

/// Preferences ▸ AI assistant.
pub(crate) fn preferences_card(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens, ctx: &mut PrefSearchContext) {
    app.ai.poll_editor();
    if app.ai.editor.as_ref().is_some_and(|e| e.check.is_some()) {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
    }
    ctx.label_secondary(
        ui,
        tl!(
            "Off until you add a provider. Use your own API key with any AI service; the key is kept in your system's keychain, never in PSG PDF's settings. Only the document you ask about is sent, and only when you ask."
        ),
        t,
    );
    ui.add_space(6.0);
    // Saved providers.
    let mut remove = None;
    let mut edit = None;
    let mut active = app.ai.settings.active.clone();
    for p in &app.ai.settings.profiles {
        ui.horizontal(|ui| {
            widgets::radio_value(ui, &mut active, Some(p.id.clone()), p.display_name()).on_hover_text(tl!("Use this provider"));
            ui.label(RichText::new(format!("{} · {}", p.model, wire_label(p.wire))).color(t.text_muted).font(theme::regular(11.5)));
            if !p.has_key && p.needs_key() {
                ui.label(RichText::new(tl!("no key")).color(t.warning).font(theme::regular(11.5)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if widgets::ghost_button(ui, "trash-2", "").on_hover_text(tl!("Remove")).clicked() {
                    remove = Some(p.id.clone());
                }
                if widgets::ghost_button(ui, "pencil", "").on_hover_text(tl!("Edit")).clicked() {
                    edit = Some(p.clone());
                }
            });
        });
    }
    if active != app.ai.settings.active {
        app.ai.settings.active = active;
    }
    if let Some(id) = remove {
        app.ai_remove(&id);
    }
    if let Some(profile) = edit {
        app.ai.editor = Some(Editor { profile, key: String::new(), show_key: false, is_new: false, models: Vec::new(), check: None, status: None });
    }
    if let Some(note) = &app.ai.key_note {
        ui.label(RichText::new(note).color(t.warning).font(theme::regular(11.5)));
    }
    ui.add_space(4.0);
    if app.ai.editor.is_none() {
        let mut picked = None;
        ui.horizontal(|ui| {
            let label = ui.label(tl!("Add a provider"));
            craft_ui::ComboBox::from_id_salt("ai-add-provider")
                .selected_text(tl!("Choose…"))
                .width(220.0)
                .height(420.0)
                .show_ui(ui, |ui| {
                    for preset in psg_ai::PRESETS {
                        let name = if preset.id == psg_ai::provider::CUSTOM { tl!("Custom (any address)") } else { preset.name };
                        if ui.selectable_label(false, name).clicked() {
                            picked = Some(preset);
                        }
                    }
                })
                .response
                .labelled_by(label.id);
        });
        if let Some(preset) = picked {
            app.ai_add(preset);
        }
        return;
    }
    editor_form(ui, app, t);
}

fn editor_form(ui: &mut egui::Ui, app: &mut PdfCraftApp, t: &Tokens) {
    let mut save = false;
    let mut cancel = false;
    let mut check = None;
    let Some(editor) = &mut app.ai.editor else { return };
    let busy = editor.check.is_some();
    let custom = editor.profile.preset == psg_ai::provider::CUSTOM;
    egui::Frame::NONE.stroke(egui::Stroke::new(1.0, t.border)).corner_radius(CornerRadius::same(6)).inner_margin(egui::Margin::same(10)).show(
        ui,
        |ui| {
            ui.set_width(ui.available_width());
            let title = if editor.is_new { tl!("New provider") } else { tl!("Edit provider") };
            ui.label(RichText::new(title).font(theme::semibold(12.5)).color(t.text));
            ui.add_space(4.0);
            egui::Grid::new("ai-editor").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                let label = ui.label(tl!("Name"));
                ui.add(widgets::line(&mut editor.profile.name).desired_width(260.0).char_limit(80)).labelled_by(label.id);
                ui.end_row();
                if custom {
                    let label = ui.label(tl!("API format"));
                    craft_ui::ComboBox::from_id_salt("ai-wire")
                        .selected_text(wire_label(editor.profile.wire))
                        .width(260.0)
                        .show_ui(ui, |ui| {
                            for w in Wire::ALL {
                                ui.selectable_value(&mut editor.profile.wire, w, wire_label(w));
                            }
                        })
                        .response
                        .labelled_by(label.id);
                    ui.end_row();
                }
                let label = ui.label(if editor.profile.wire == Wire::Azure { tl!("Endpoint") } else { tl!("API address") });
                let hint = match editor.profile.wire {
                    Wire::Azure => "https://<resource>.openai.azure.com",
                    _ => "https://…/v1",
                };
                ui.add(widgets::line(&mut editor.profile.base_url).desired_width(260.0).hint_text(hint)).labelled_by(label.id);
                ui.end_row();
                let label = ui.label(tl!("API key"));
                ui.horizontal(|ui| {
                    let hint = if editor.profile.has_key { tl!("Saved — type to replace") } else { tl!("Paste your key") };
                    ui.add(widgets::line(&mut editor.key).password(!editor.show_key).desired_width(220.0).hint_text(hint)).labelled_by(label.id);
                    let icon = if editor.show_key { "eye-off" } else { "eye" };
                    if widgets::ghost_button(ui, icon, "").on_hover_text(tl!("Show or hide the key")).clicked() {
                        editor.show_key = !editor.show_key;
                    }
                });
                ui.end_row();
                let label = ui.label(if editor.profile.wire == Wire::Azure { tl!("Deployment") } else { tl!("Model") });
                ui.horizontal(|ui| {
                    ui.add(widgets::line(&mut editor.profile.model).desired_width(180.0)).labelled_by(label.id);
                    let suggestions: Vec<String> = if editor.models.is_empty() {
                        editor.profile.preset().map(|p| p.models.iter().map(|m| (*m).to_string()).collect()).unwrap_or_default()
                    } else {
                        editor.models.clone()
                    };
                    if !suggestions.is_empty() {
                        craft_ui::ComboBox::from_id_salt("ai-model-list")
                            .selected_text("")
                            .width(36.0)
                            .height(320.0)
                            .show_ui(ui, |ui| {
                                for m in &suggestions {
                                    ui.selectable_value(&mut editor.profile.model, m.clone(), m);
                                }
                            })
                            .response
                            .labelled_by(label.id)
                            .on_hover_text(tl!("Models this provider offers"));
                    }
                });
                ui.end_row();
                if editor.profile.wire == Wire::Azure {
                    let label = ui.label(tl!("API version"));
                    ui.add(
                        widgets::line(&mut editor.profile.api_version).desired_width(140.0).hint_text(psg_ai::provider::DEFAULT_AZURE_API_VERSION),
                    )
                    .labelled_by(label.id);
                    ui.end_row();
                }
                let label = ui.label(tl!("Longest answer"));
                ui.horizontal(|ui| {
                    ui.add(egui::DragValue::new(&mut editor.profile.max_tokens).range(0..=psg_ai::provider::MAX_OUTPUT_TOKENS).speed(64))
                        .labelled_by(label.id);
                    ui.label(RichText::new(tl!("tokens (0: the provider's default)")).color(t.text_muted).font(theme::regular(11.5)));
                });
                ui.end_row();
            });
            if editor.profile.unencrypted_remote() {
                ui.label(
                    RichText::new(tl!("This address isn't encrypted (http://): the key and the document cross the network as plain text."))
                        .color(t.warning)
                        .font(theme::regular(11.5)),
                );
            }
            match &editor.status {
                Some(Ok(note)) => {
                    ui.label(RichText::new(note).color(t.success).font(theme::regular(11.5)));
                }
                Some(Err(problem)) => {
                    ui.label(RichText::new(problem).color(t.danger).font(theme::regular(11.5)));
                }
                None => {}
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    if widgets::pill_button(ui, tl!("Test connection"), false).clicked() {
                        check = Some(false);
                    }
                    if editor.profile.wire != Wire::Azure && widgets::pill_button(ui, tl!("Fetch models"), false).clicked() {
                        check = Some(true);
                    }
                });
                if busy {
                    ui.spinner();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::pill_button(ui, tl!("Save"), true).clicked() {
                        save = true;
                    }
                    if widgets::pill_button(ui, tl!("Cancel"), false).clicked() {
                        cancel = true;
                    }
                });
            });
        },
    );
    if let Some(models) = check {
        app.ai_editor_check(models);
    }
    if cancel {
        app.ai.editor = None;
    } else if save
        && let Err(e) = app.ai_save_editor()
        && let Some(editor) = &mut app.ai.editor
    {
        editor.status = Some(Err(e));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Dialog;
    use egui_kittest::{Harness, kittest::Queryable};
    use std::io::{Read, Write};

    /// A loopback HTTP server that answers `replies` requests in turn with (status, body), and
    /// hands back each request it read.
    fn serve(replies: Vec<(u16, &'static str)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (status, body) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut received = Vec::new();
                let mut buf = [0u8; 65536];
                loop {
                    let n = stream.read(&mut buf).unwrap();
                    received.extend_from_slice(&buf[..n]);
                    let text = String::from_utf8_lossy(&received).to_string();
                    if let Some(end) = text.find("\r\n\r\n") {
                        let len = text[..end]
                            .lines()
                            .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap()))
                            .unwrap_or(0);
                        if received.len() >= end + 4 + len {
                            break;
                        }
                    }
                    if n == 0 {
                        break;
                    }
                }
                let reply = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(reply.as_bytes()).unwrap();
                seen.push(String::from_utf8_lossy(&received).to_string());
            }
            seen
        });
        (format!("http://127.0.0.1:{port}/v1"), handle)
    }

    fn app_with_document() -> PdfCraftApp {
        let bytes = pdfcraft_engine::Session::new().create_from_text("Report", "Revenue rose five percent\u{c}Costs fell").unwrap();
        let mut app = PdfCraftApp::new();
        app.run_inline = true;
        app.ai.use_memory_keys();
        app.set_option("language", "en").unwrap();
        app.open_bytes("report.pdf", None, bytes.as_ref().clone()).unwrap();
        app
    }

    fn add_provider(app: &mut PdfCraftApp, url: &str, key: &str) {
        let custom = psg_ai::provider::preset_by_id(psg_ai::provider::CUSTOM).unwrap();
        app.ai_add(custom);
        let editor = app.ai.editor.as_mut().unwrap();
        editor.profile.name = "Mock".into();
        editor.profile.base_url = url.into();
        editor.profile.model = "mock-model".into();
        editor.key = key.into();
        app.ai_save_editor().unwrap();
    }

    const ANSWER: &str = r#"{"choices":[{"message":{"content":"Revenue rose (p. 1)."},"finish_reason":"stop"}]}"#;

    #[test]
    fn a_saved_provider_keeps_its_key_out_of_the_settings_file() {
        let mut app = app_with_document();
        add_provider(&mut app, "https://api.example.com/v1", "sk-never-in-settings");
        let profile = app.ai.settings.active_profile().unwrap().clone();
        assert!(profile.has_key);
        assert_eq!(app.ai.secrets.load(&profile.id).unwrap().as_deref(), Some("sk-never-in-settings"));
        assert_eq!(app.ai.key_note.as_deref().map(|n| n.contains("quits")), Some(true), "a memory store says the key lasts the session");
        let saved = app.persist();
        assert!(!saved.contains("sk-never-in-settings"), "{saved}");
        assert!(saved.contains("api.example.com"));
        let mut restored = PdfCraftApp::new();
        restored.restore(&saved);
        assert_eq!(restored.ai.settings, app.ai.settings);
        // Removing the provider forgets its key.
        app.ai_remove(&profile.id);
        assert!(app.ai.settings.profiles.is_empty());
        assert_eq!(app.ai.secrets.load(&profile.id).unwrap(), None);
    }

    #[test]
    fn a_provider_with_a_bad_address_is_not_saved() {
        let mut app = app_with_document();
        app.ai_add(psg_ai::provider::preset_by_id(psg_ai::provider::CUSTOM).unwrap());
        app.ai.editor.as_mut().unwrap().profile.base_url = "api.example.com".into();
        app.ai.editor.as_mut().unwrap().profile.model = "m".into();
        let err = app.ai_save_editor().unwrap_err();
        assert!(err.contains("https://"), "{err}");
        assert!(app.ai.settings.profiles.is_empty());
        assert!(app.ai.editor.is_some(), "the form stays open to be corrected");
    }

    #[test]
    fn without_a_provider_the_assistant_asks_for_one() {
        let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_eframe(|_| {
            let mut app = app_with_document();
            app.run_command("ai.summary");
            app
        });
        h.run_steps(2);
        assert_eq!(h.state().dialog, Some(Dialog::Preferences));
        assert_eq!(h.state().left, crate::LeftPanel::Tool("ai"));
        h.get_by_role_and_label(egui::accesskit::Role::Button, "Add a provider");
        h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Add a provider");
    }

    #[test]
    fn summarize_and_a_follow_up_question_go_to_the_provider_with_the_document() {
        let (url, server) = serve(vec![(200, ANSWER), (200, r#"{"choices":[{"message":{"content":"Costs fell (p. 2)."}}]}"#)]);
        let mut h = Harness::builder().with_size(egui::vec2(1200.0, 900.0)).build_eframe(|_| {
            let mut app = app_with_document();
            add_provider(&mut app, &url, "sk-mock");
            app.run_command("ai.summary");
            app
        });
        h.run_steps(3);
        let id = h.state().active_ids().unwrap().1;
        let chat: Vec<String> = h.state().ai.chat(id).iter().map(|t| t.shown.clone()).collect();
        assert_eq!(chat, ["Summarize", "Revenue rose (p. 1)."], "{:?}", h.state().ai.errors);
        h.get_by_label("Revenue rose (p. 1).");
        // A follow-up typed into the panel carries the conversation so far.
        h.get_by_role_and_label(egui::accesskit::Role::MultilineTextInput, "Ask about this document").click();
        h.run_steps(1);
        h.get_by_role_and_label(egui::accesskit::Role::MultilineTextInput, "Ask about this document").type_text("What about costs?");
        h.run_steps(1);
        h.get_by_label("Ask").click();
        h.run_steps(3);
        assert_eq!(h.state().ai.chat(id).len(), 4, "{:?}", h.state().ai.errors);
        assert_eq!(h.state().ai.chat(id)[3].shown, "Costs fell (p. 2).");
        assert!(h.state().ai.question.is_empty());
        let seen = server.join().unwrap();
        assert!(seen[0].starts_with("POST /v1/chat/completions"), "{}", seen[0]);
        assert!(seen[0].to_ascii_lowercase().contains("authorization: bearer sk-mock"));
        assert!(seen[0].contains("Revenue rose five percent") && seen[0].contains("[Page 2]") && seen[0].contains("report.pdf"), "{}", seen[0]);
        assert!(seen[0].contains("\"model\":\"mock-model\""));
        assert!(seen[1].contains("What about costs?") && seen[1].contains("Revenue rose (p. 1)."), "{}", seen[1]);
    }

    #[test]
    fn current_page_only_sends_that_page_and_errors_show_in_the_panel() {
        let (url, server) = serve(vec![(401, r#"{"error":{"message":"Incorrect API key provided"}}"#)]);
        let mut app = app_with_document();
        add_provider(&mut app, &url, "sk-wrong");
        app.ai.current_page_only = true;
        app.ai_run(Task::Translate("Japanese".into()));
        let id = app.active_ids().unwrap().1;
        let error = app.ai.errors.get(&id).cloned().unwrap_or_default();
        assert!(error.contains("refused the API key") && error.contains("Incorrect API key provided"), "{error}");
        assert!(app.ai.chat(id).is_empty());
        let seen = server.join().unwrap();
        assert!(seen[0].contains("into Japanese") && seen[0].contains("Revenue rose") && !seen[0].contains("Costs fell"), "{}", seen[0]);
    }

    #[test]
    fn test_connection_and_fetch_models_report_in_the_form() {
        let (url, server) = serve(vec![(200, ANSWER), (200, r#"{"data":[{"id":"zeta"},{"id":"alpha"}]}"#)]);
        let mut app = app_with_document();
        app.ai_add(psg_ai::provider::preset_by_id("ollama").unwrap());
        app.ai.editor.as_mut().unwrap().profile.base_url = url;
        app.ai.editor.as_mut().unwrap().profile.model.clear();
        app.ai.editor.as_mut().unwrap().profile.model.push_str("llama");
        app.ai_editor_check(false);
        assert_eq!(app.ai.editor.as_ref().unwrap().status, Some(Ok("Connected: the provider answered.".into())));
        app.ai.editor.as_mut().unwrap().profile.model.clear();
        app.ai_editor_check(true);
        let editor = app.ai.editor.as_ref().unwrap();
        assert_eq!(editor.models, ["alpha", "zeta"]);
        assert_eq!(editor.profile.model, "alpha", "an empty model takes the first one listed");
        server.join().unwrap();
    }

    #[test]
    fn the_preferences_card_adds_a_provider() {
        let mut h = Harness::builder().with_size(egui::vec2(1200.0, 1400.0)).build_eframe(|_| {
            let mut app = app_with_document();
            app.dialog = Some(Dialog::Preferences);
            app
        });
        // The card is the last one: searching scrolls it into view.
        h.ctx.data_mut(|d| d.insert_temp(egui::Id::unique("pdfcraft_preferences_search_query"), "API key".to_string()));
        h.run_steps(3);
        h.get_by_role_and_label(egui::accesskit::Role::ComboBox, "Add a provider").click();
        h.run_steps(2);
        h.get_by_label("DeepSeek").click();
        h.run_steps(2);
        assert_eq!(h.state().ai.editor.as_ref().map(|e| e.profile.base_url.clone()).as_deref(), Some("https://api.deepseek.com/v1"));
        h.get_by_role_and_label(egui::accesskit::Role::PasswordInput, "API key").scroll_to_me();
        h.run_steps(6);
        h.get_by_role_and_label(egui::accesskit::Role::PasswordInput, "API key").focus();
        h.run_steps(1);
        h.get_by_role_and_label(egui::accesskit::Role::PasswordInput, "API key").type_text("sk-typed");
        h.run_steps(1);
        h.get_by_label("Save").scroll_to_me();
        h.run_steps(6);
        h.get_by_label("Save").click();
        h.run_steps(2);
        let s = &h.state().ai.settings;
        assert_eq!(s.profiles.len(), 1);
        assert_eq!(s.active_profile().map(|p| (p.model.as_str(), p.has_key)), Some(("deepseek-chat", true)));
        assert!(h.state().ai.editor.is_none());
    }
}
