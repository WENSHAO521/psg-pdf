//! Each API's HTTP call and reply, as plain data: building a request touches no network, and
//! reading a reply is a function of its status and body.

use serde_json::{Value, json};

use crate::Error;
use crate::provider::{DEFAULT_AZURE_API_VERSION, Profile, Wire};
use crate::task::{Message, Role};

/// Anthropic's API version header; its Messages API has kept this value since it launched.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
/// The reply length Anthropic is asked for when the profile leaves it open (its API requires one).
pub const ANTHROPIC_DEFAULT_MAX_TOKENS: u32 = 4096;
/// The longest error message kept from a provider's answer.
const MAX_ERROR_CHARS: usize = 400;
/// The most model names kept from a provider's list.
pub const MAX_MODELS: usize = 2000;

/// One request: a system instruction and the conversation so far, the last message the person's.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Request {
    pub system: String,
    pub messages: Vec<Message>,
    /// The longest reply to ask for, in tokens; 0 leaves it to the provider.
    pub max_tokens: u32,
}

/// An HTTP call, ready to send.
#[derive(Clone, PartialEq, Eq)]
pub struct HttpCall {
    /// `GET` or `POST`.
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// A JSON body, for `POST`.
    pub body: Option<String>,
}

impl std::fmt::Debug for HttpCall {
    // Never print the key: the headers that carry it are shown redacted.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let headers: Vec<(&str, &str)> =
            self.headers.iter().map(|(k, v)| (k.as_str(), if is_secret_header(k) { "<redacted>" } else { v.as_str() })).collect();
        f.debug_struct("HttpCall")
            .field("method", &self.method)
            .field("url", &redact_url(&self.url))
            .field("headers", &headers)
            .finish_non_exhaustive()
    }
}

fn is_secret_header(name: &str) -> bool {
    ["authorization", "x-api-key", "api-key", "x-goog-api-key"].iter().any(|h| name.eq_ignore_ascii_case(h))
}

fn redact_url(url: &str) -> String {
    match url.split_once("key=") {
        Some((head, _)) => format!("{head}key=<redacted>"),
        None => url.to_string(),
    }
}

/// The provider's answer.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Reply {
    pub text: String,
    /// The reply stopped at the length limit.
    pub truncated: bool,
}

fn base(profile: &Profile) -> &str {
    profile.base_url.trim().trim_end_matches('/')
}

/// The key as a header value: trimmed, and refused when it holds a character no key has (a line
/// break would let it add headers of its own).
fn clean_key(key: Option<&str>) -> Result<Option<String>, Error> {
    let Some(key) = key.map(str::trim).filter(|k| !k.is_empty()) else { return Ok(None) };
    if key.chars().any(|c| !(c.is_ascii_graphic() || c == ' ')) || key.len() > 4096 {
        return Err(Error::Config("the API key has characters an API key can't have".into()));
    }
    Ok(Some(key.to_string()))
}

/// Percent-encode a path segment or query value (everything but RFC 3986 unreserved characters).
pub(crate) fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn gemini_model(model: &str) -> &str {
    let m = model.trim();
    m.strip_prefix("models/").unwrap_or(m)
}

fn api_version(profile: &Profile) -> &str {
    let v = profile.api_version.trim();
    if v.is_empty() { DEFAULT_AZURE_API_VERSION } else { v }
}

fn auth_headers(profile: &Profile, key: Option<&str>) -> Result<Vec<(String, String)>, Error> {
    let key = clean_key(key)?;
    if key.is_none() && (profile.needs_key() || matches!(profile.wire, Wire::Anthropic | Wire::Gemini | Wire::Azure)) {
        return Err(Error::MissingKey);
    }
    let mut h = Vec::new();
    match (profile.wire, key) {
        (Wire::OpenAi, Some(k)) => h.push(("Authorization".into(), format!("Bearer {k}"))),
        (Wire::Anthropic, Some(k)) => h.push(("x-api-key".into(), k)),
        (Wire::Gemini, Some(k)) => h.push(("x-goog-api-key".into(), k)),
        (Wire::Azure, Some(k)) => h.push(("api-key".into(), k)),
        (_, None) => {}
    }
    if profile.wire == Wire::Anthropic {
        h.push(("anthropic-version".into(), ANTHROPIC_VERSION.into()));
    }
    Ok(h)
}

fn role_name(role: Role, wire: Wire) -> &'static str {
    match (role, wire) {
        (Role::User, _) => "user",
        (Role::Assistant, Wire::Gemini) => "model",
        (Role::Assistant, _) => "assistant",
    }
}

/// The chat call for `request`.
pub fn chat_call(profile: &Profile, key: Option<&str>, request: &Request) -> Result<HttpCall, Error> {
    profile.check()?;
    let mut headers = auth_headers(profile, key)?;
    headers.push(("Content-Type".into(), "application/json".into()));
    let base = base(profile);
    let model = profile.model.trim();
    let (url, body) = match profile.wire {
        Wire::OpenAi | Wire::Azure => {
            let mut messages = Vec::with_capacity(request.messages.len().saturating_add(1));
            if !request.system.is_empty() {
                messages.push(json!({ "role": "system", "content": request.system }));
            }
            messages.extend(request.messages.iter().map(|m| json!({ "role": role_name(m.role, profile.wire), "content": m.content })));
            let mut body = json!({ "messages": messages });
            let url = if profile.wire == Wire::Azure {
                format!("{base}/openai/deployments/{}/chat/completions?api-version={}", encode(model), encode(api_version(profile)))
            } else {
                body["model"] = json!(model);
                format!("{base}/chat/completions")
            };
            if request.max_tokens > 0 {
                // OpenAI's own newer models take only `max_completion_tokens`; the services that
                // copy its API mostly know `max_tokens`.
                let field = if profile.preset == "openai" || profile.wire == Wire::Azure { "max_completion_tokens" } else { "max_tokens" };
                body[field] = json!(request.max_tokens);
            }
            (url, body)
        }
        Wire::Anthropic => {
            let messages: Vec<Value> =
                request.messages.iter().map(|m| json!({ "role": role_name(m.role, Wire::Anthropic), "content": m.content })).collect();
            let max = if request.max_tokens > 0 { request.max_tokens } else { ANTHROPIC_DEFAULT_MAX_TOKENS };
            let mut body = json!({ "model": model, "max_tokens": max, "messages": messages });
            if !request.system.is_empty() {
                body["system"] = json!(request.system);
            }
            (format!("{base}/v1/messages"), body)
        }
        Wire::Gemini => {
            let contents: Vec<Value> =
                request.messages.iter().map(|m| json!({ "role": role_name(m.role, Wire::Gemini), "parts": [{ "text": m.content }] })).collect();
            let mut body = json!({ "contents": contents });
            if !request.system.is_empty() {
                body["systemInstruction"] = json!({ "parts": [{ "text": request.system }] });
            }
            if request.max_tokens > 0 {
                body["generationConfig"] = json!({ "maxOutputTokens": request.max_tokens });
            }
            (format!("{base}/v1beta/models/{}:generateContent", encode(gemini_model(model))), body)
        }
    };
    Ok(HttpCall { method: "POST", url, headers, body: Some(body.to_string()) })
}

/// The call that lists the provider's models. Azure has no such list for a key: its deployments
/// are named in the Azure portal.
pub fn models_call(profile: &Profile, key: Option<&str>) -> Result<HttpCall, Error> {
    let mut check = profile.clone();
    if check.model.trim().is_empty() {
        // Listing models needs no model.
        check.model = "-".into();
    }
    check.check()?;
    let headers = auth_headers(profile, key)?;
    let base = base(profile);
    let url = match profile.wire {
        Wire::OpenAi => format!("{base}/models"),
        Wire::Anthropic => format!("{base}/v1/models?limit=1000"),
        Wire::Gemini => format!("{base}/v1beta/models?pageSize=1000"),
        Wire::Azure => return Err(Error::Config("Azure OpenAI has no model list; enter the deployment name".into())),
    };
    Ok(HttpCall { method: "GET", url, headers, body: None })
}

/// The error a non-success status carries: the API's own message when it gives one.
fn status_error(status: u16, body: &str) -> Error {
    let parsed: Option<Value> = serde_json::from_str(body).ok();
    let message = parsed
        .as_ref()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.get("message"))
                .or_else(|| v.get("error").filter(|e| e.is_string()))
                .or_else(|| v.get("detail"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.trim().to_string());
    let mut message: String = message.chars().filter(|c| !c.is_control() || *c == '\n').take(MAX_ERROR_CHARS).collect();
    if message.is_empty() {
        message = format!("HTTP {status}");
    }
    Error::Provider { status, message }
}

fn parse_json(body: &str) -> Result<Value, Error> {
    serde_json::from_str(body).map_err(|e| Error::Reply(format!("the provider's answer is not JSON ({e})")))
}

/// Text from an OpenAI `content`, a string or a list of parts.
fn openai_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts.iter().filter_map(|p| p.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join(""),
        _ => String::new(),
    }
}

/// Read a chat reply.
pub fn parse_chat(wire: Wire, status: u16, body: &str) -> Result<Reply, Error> {
    if !(200..300).contains(&status) {
        return Err(status_error(status, body));
    }
    let v = parse_json(body)?;
    let reply = match wire {
        Wire::OpenAi | Wire::Azure => {
            let choice = v.pointer("/choices/0").ok_or_else(|| Error::Reply("the provider's answer has no choices".into()))?;
            let message = choice.get("message");
            let mut text = openai_text(message.and_then(|m| m.get("content")));
            if text.is_empty()
                && let Some(refusal) = message.and_then(|m| m.get("refusal")).and_then(Value::as_str)
            {
                text = refusal.to_string();
            }
            Reply { text, truncated: choice.get("finish_reason").and_then(Value::as_str) == Some("length") }
        }
        Wire::Anthropic => {
            let parts = v.get("content").and_then(Value::as_array).ok_or_else(|| Error::Reply("the provider's answer has no content".into()))?;
            let text = parts
                .iter()
                .filter(|p| p.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("");
            Reply { text, truncated: v.get("stop_reason").and_then(Value::as_str) == Some("max_tokens") }
        }
        Wire::Gemini => {
            let Some(candidate) = v.pointer("/candidates/0") else {
                let why = v.pointer("/promptFeedback/blockReason").and_then(Value::as_str).unwrap_or("no candidates");
                return Err(Error::Reply(format!("the provider returned no answer ({why})")));
            };
            let text = candidate
                .pointer("/content/parts")
                .and_then(Value::as_array)
                .map(|parts| {
                    parts
                        .iter()
                        .filter(|p| p.get("thought").and_then(Value::as_bool) != Some(true))
                        .filter_map(|p| p.get("text").and_then(Value::as_str))
                        .collect::<Vec<_>>()
                        .join("")
                })
                .unwrap_or_default();
            Reply { text, truncated: candidate.get("finishReason").and_then(Value::as_str) == Some("MAX_TOKENS") }
        }
    };
    if reply.text.trim().is_empty() {
        return Err(Error::Reply("the provider returned an empty answer".into()));
    }
    Ok(reply)
}

/// Read a list of models, sorted, without duplicates.
pub fn parse_models(wire: Wire, status: u16, body: &str) -> Result<Vec<String>, Error> {
    if !(200..300).contains(&status) {
        return Err(status_error(status, body));
    }
    let v = parse_json(body)?;
    let mut names: Vec<String> = match wire {
        Wire::OpenAi | Wire::Anthropic | Wire::Azure => v
            .get("data")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|m| m.get("id").and_then(Value::as_str)).map(str::to_string).collect())
            .unwrap_or_default(),
        Wire::Gemini => v
            .get("models")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter(|m| {
                        m.get("supportedGenerationMethods")
                            .and_then(Value::as_array)
                            .is_none_or(|methods| methods.iter().any(|x| x.as_str() == Some("generateContent")))
                    })
                    .filter_map(|m| m.get("name").and_then(Value::as_str))
                    .map(|n| gemini_model(n).to_string())
                    .collect()
            })
            .unwrap_or_default(),
    };
    names.retain(|n| !n.is_empty() && n.chars().count() <= 256 && !n.chars().any(char::is_control));
    names.sort();
    names.dedup();
    names.truncate(MAX_MODELS);
    if names.is_empty() {
        return Err(Error::Reply("the provider listed no models".into()));
    }
    Ok(names)
}
