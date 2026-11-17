//! psg-ai — the AI provider people bring to PSG's apps themselves.
//!
//! Nothing here runs until someone adds a provider: PSG ships no key, no account and no default
//! endpoint that is called on its own. A provider is a [`Profile`]: which API it speaks
//! ([`Wire`]), where (`base_url`), which model, and whether a key is saved for it. The key itself
//! never goes into a profile or the settings file; the app keeps it in the operating system's
//! credential store through a [`secret::Store`].
//!
//! - [`provider`] holds the presets (OpenAI, Anthropic, Google Gemini, Azure OpenAI, DeepSeek,
//!   Mistral, Groq, OpenRouter, Ollama, LM Studio and other OpenAI-compatible services) and the
//!   saved [`Settings`].
//! - [`wire`] turns a request into the HTTP call each API expects and reads its reply. It is pure,
//!   so every format is tested without a network.
//! - [`net`] sends the call through a transport the app provides: this crate opens no sockets,
//!   so a host without one (the headless batch tools) can't reach the network through it.
//! - [`task`] words the assistant's requests: summarize, ask about, translate a document.
//!
//! The document text a request carries leaves this computer for the provider the person chose;
//! the apps say so beside every action that sends it.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod net;
pub mod provider;
pub mod secret;
pub mod task;
pub mod wire;

#[cfg(test)]
mod tests;

pub use provider::{PRESETS, Preset, Profile, Settings, Wire};
pub use task::{DocText, Message, Role, Task};
pub use wire::{HttpCall, Reply, Request};

/// What went wrong, in words an interface can show (it adds its own translation of the kind).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// The profile can't be used as it is: a missing model, a malformed address.
    #[error("{0}")]
    Config(String),
    /// The provider needs a key and none is saved.
    #[error("no API key is saved for this provider")]
    MissingKey,
    /// The provider answered with an error status.
    #[error("the provider answered {status}: {message}")]
    Provider { status: u16, message: String },
    /// The request didn't reach the provider, or its answer didn't arrive.
    #[error("{0}")]
    Network(String),
    /// The provider answered, but not in a form this API returns.
    #[error("{0}")]
    Reply(String),
    /// The credential store refused.
    #[error("{0}")]
    Keychain(String),
    /// Not in this build (the browser build sends nothing).
    #[error("AI providers are not available in this build")]
    Unsupported,
}

/// The longest reply body read (4 MiB). A chat reply is a few kilobytes; a list of models a few
/// hundred. Anything longer is refused rather than held in memory.
pub const MAX_REPLY_BYTES: u64 = 4 * 1024 * 1024;
