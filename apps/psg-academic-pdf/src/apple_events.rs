//! macOS: Finder double-clicks, Open With and drops on the Dock icon arrive as Apple events, not
//! on the command line. The audited `fmv-macos-events` crate (PSG PDF uses it too) registers the
//! handler behind a safe main-thread API; this module turns its events into paths to open.

use std::path::PathBuf;

use fmv_macos_events::{Event, Inbox, Registration};

/// Keeps the handlers registered; hold it until the event loop returns.
pub struct AppleEvents {
    _registration: Registration,
    inbox: Inbox,
}

/// What arrived since the last frame.
pub enum OsEvent {
    Open(Vec<PathBuf>),
    Quit,
}

pub type OsEventsFn = Box<dyn FnMut() -> Vec<OsEvent>>;

impl AppleEvents {
    /// Register the handlers, on the main thread, before the event loop starts.
    pub fn install() -> Self {
        let (registration, inbox) = Registration::install();
        Self { _registration: registration, inbox }
    }

    /// The queue the window drains each frame; later events wake `ctx`.
    pub fn connect(&self, ctx: &egui::Context) -> OsEventsFn {
        let ctx = ctx.clone();
        self.inbox.set_wake(move || ctx.request_repaint());
        let inbox = self.inbox.clone();
        Box::new(move || {
            inbox
                .drain()
                .into_iter()
                .map(|e| match e {
                    Event::Open(paths) => OsEvent::Open(paths),
                    Event::Quit => OsEvent::Quit,
                })
                .collect()
        })
    }
}
