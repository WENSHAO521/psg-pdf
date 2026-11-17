//! The AI assistant's access to the outside: its HTTP transport and the OS credential store that
//! keeps API keys. `psg_ai` itself opens no sockets and touches no keychain.

use psg_ai::secret::{Memory, Saved, Store};
use psg_ai::{Error, HttpCall};

/// How long one request may take, start to finish. A long document and a slow model can take
/// minutes to answer.
#[cfg(not(target_arch = "wasm32"))]
const REQUEST_TIMEOUT_SECS: u64 = 300;

/// The User-Agent of the assistant's requests: the product and version, nothing about the person.
fn user_agent() -> String {
    format!("{}/{}", crate::brand::APP_NAME.replace(' ', "-"), env!("CARGO_PKG_VERSION"))
}

/// Send `call` with ureq (the same TLS set-up as the updater's requests); the status and body of
/// whatever came back.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn send(call: &HttpCall) -> Result<(u16, String), Error> {
    use std::time::Duration;
    let local = psg_ai::provider::is_loopback(&call.url);
    let agent = ureq::Agent::config_builder()
        // The OS checks certificate chains, as the updater's requests do.
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(ureq::tls::RootCerts::PlatformVerifier).build())
        // A server on this computer is reached directly, whatever proxy is set.
        .proxy(if local { None } else { ureq::Proxy::try_from_env() })
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_global(Some(Duration::from_secs(REQUEST_TIMEOUT_SECS)))
        .http_status_as_error(false)
        // A redirect would carry the key to another address: refused.
        .max_redirects(0)
        .max_redirects_will_error(false)
        .user_agent(user_agent())
        .build()
        .new_agent();
    let result = match (call.method, &call.body) {
        ("POST", Some(body)) => {
            let mut req = agent.post(&call.url);
            for (k, v) in &call.headers {
                req = req.header(k, v);
            }
            req.send(body.as_bytes())
        }
        _ => {
            let mut req = agent.get(&call.url);
            for (k, v) in &call.headers {
                req = req.header(k, v);
            }
            req.call()
        }
    };
    let mut response = result.map_err(|e| Error::Network(network_message(&e)))?;
    let status = response.status().as_u16();
    let body = response.body_mut().with_config().limit(psg_ai::MAX_REPLY_BYTES).read_to_string().map_err(|e| Error::Network(network_message(&e)))?;
    Ok((status, body))
}

/// ureq's error, without anything that could hold the key (it never prints headers, and no API
/// format puts the key in the address).
#[cfg(not(target_arch = "wasm32"))]
fn network_message(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Timeout(_) => "the provider took too long to answer".into(),
        ureq::Error::HostNotFound => "the API address's host name was not found".into(),
        ureq::Error::ConnectionFailed => "couldn't connect to the API address".into(),
        ureq::Error::BodyExceedsLimit(_) => "the provider's answer is too long".into(),
        other => other.to_string(),
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn send(_call: &HttpCall) -> Result<(u16, String), Error> {
    let _ = user_agent;
    Err(Error::Unsupported)
}

/// The OS credential store, falling back to memory for keys it refused.
#[derive(Default)]
pub(crate) struct Keychain {
    fallback: Memory,
}

impl Store for Keychain {
    fn save(&self, id: &str, key: &str) -> Result<Saved, Error> {
        match os::save(id, key) {
            Ok(()) => {
                // A stale session copy would hide the saved key.
                self.fallback.delete(id)?;
                Ok(Saved::Keychain)
            }
            Err(e) => {
                log::warn!("AI key kept for this session only: the credential store refused ({e})");
                self.fallback.save(id, key)
            }
        }
    }

    fn load(&self, id: &str) -> Result<Option<String>, Error> {
        if let Some(key) = self.fallback.load(id)? {
            return Ok(Some(key));
        }
        os::load(id)
    }

    fn delete(&self, id: &str) -> Result<(), Error> {
        self.fallback.delete(id)?;
        os::delete(id)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod os {
    use psg_ai::Error;
    use psg_ai::secret::SERVICE;

    fn entry(id: &str) -> Result<keyring::Entry, Error> {
        keyring::Entry::new(SERVICE, id).map_err(|e| Error::Keychain(e.to_string()))
    }

    pub(super) fn save(id: &str, key: &str) -> Result<(), Error> {
        entry(id)?.set_password(key).map_err(|e| Error::Keychain(e.to_string()))
    }

    pub(super) fn load(id: &str) -> Result<Option<String>, Error> {
        match entry(id)?.get_password() {
            Ok(k) => Ok(Some(k)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Error::Keychain(e.to_string())),
        }
    }

    pub(super) fn delete(id: &str) -> Result<(), Error> {
        match entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(Error::Keychain(e.to_string())),
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod os {
    use psg_ai::Error;

    pub(super) fn save(_id: &str, _key: &str) -> Result<(), Error> {
        Err(Error::Unsupported)
    }

    pub(super) fn load(_id: &str) -> Result<Option<String>, Error> {
        Ok(None)
    }

    pub(super) fn delete(_id: &str) -> Result<(), Error> {
        Ok(())
    }
}
