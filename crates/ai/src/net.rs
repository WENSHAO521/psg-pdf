//! Requests through a transport the app provides. This crate opens no sockets itself: the
//! desktop app passes its HTTP client, and headless hosts (the automation tools behind
//! `pdfcraft-cli`, whose batch path promises never to reach the network) have none unless one is
//! given to them.

use crate::Error;
use crate::provider::Profile;
use crate::wire::{self, HttpCall, Reply, Request};

/// Sends a call and returns the status and body of whatever came back, success or not. It must
/// not follow redirects (they would carry the key elsewhere) and should cap the body at
/// [`crate::MAX_REPLY_BYTES`].
pub type Transport = dyn Fn(&HttpCall) -> Sent + Send + Sync;

/// What a transport returns: the status and body, or why nothing came back.
pub type Sent = Result<(u16, String), Error>;

/// Send a chat request and read the reply.
pub fn chat(profile: &Profile, key: Option<&str>, request: &Request, send: &dyn Fn(&HttpCall) -> Sent) -> Result<Reply, Error> {
    let call = wire::chat_call(profile, key, request)?;
    let (status, body) = send(&call)?;
    wire::parse_chat(profile.wire, status, &body)
}

/// List the provider's models.
pub fn models(profile: &Profile, key: Option<&str>, send: &dyn Fn(&HttpCall) -> Sent) -> Result<Vec<String>, Error> {
    let call = wire::models_call(profile, key)?;
    let (status, body) = send(&call)?;
    wire::parse_models(profile.wire, status, &body)
}
