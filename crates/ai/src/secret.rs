//! Where API keys are kept. The desktop app's [`Store`] is the operating system's credential store
//! (the macOS Keychain, the Windows Credential Manager, the Secret Service on Linux); [`Memory`]
//! keeps them for a session (tests, and machines whose credential store refuses). Keys never go
//! into the settings file, a log or a request other than the provider's own.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::Error;

/// The service name the keys are filed under in the credential store.
pub const SERVICE: &str = "PSG PDF AI";

/// The environment variable that gives a key to headless runs (`pdfcraft-cli`, MCP), which have
/// no saved profile.
pub const KEY_ENV: &str = "PSG_AI_API_KEY";

/// Where a key was saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Saved {
    /// In the credential store: it stays after the app quits.
    Keychain,
    /// In memory only, because the credential store refused: it is gone when the app quits.
    SessionOnly,
}

/// Where keys are kept.
pub trait Store: Send + Sync {
    /// Save `key` for profile `id`, replacing any earlier one.
    fn save(&self, id: &str, key: &str) -> Result<Saved, Error>;
    /// The key saved for profile `id`.
    fn load(&self, id: &str) -> Result<Option<String>, Error>;
    /// Forget the key for profile `id` (nothing saved is not an error).
    fn delete(&self, id: &str) -> Result<(), Error>;
}

/// Keys in memory, for tests and as the session-only fallback.
#[derive(Default)]
pub struct Memory {
    keys: Mutex<HashMap<String, String>>,
}

impl Store for Memory {
    fn save(&self, id: &str, key: &str) -> Result<Saved, Error> {
        let mut keys = self.keys.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        keys.insert(id.to_string(), key.to_string());
        Ok(Saved::SessionOnly)
    }

    fn load(&self, id: &str) -> Result<Option<String>, Error> {
        let keys = self.keys.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(keys.get(id).cloned())
    }

    fn delete(&self, id: &str) -> Result<(), Error> {
        let mut keys = self.keys.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        keys.remove(id);
        Ok(())
    }
}
