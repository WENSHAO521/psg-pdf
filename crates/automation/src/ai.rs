//! `ai_ask`: summarize, ask about or translate a document with an AI provider the caller names.
//!
//! Headless runs have no saved providers: the call names the API format, address and model, and
//! the key comes from the `PSG_AI_API_KEY` environment variable (never an argument, so it stays out
//! of agent transcripts). The request goes through the transport the host gave
//! ([`Automation::with_ai_transport`]); this crate opens no sockets, so without one the tool says
//! so and sends nothing.

use serde_json::{Value, json};

use psg_ai::provider::{CUSTOM, MAX_OUTPUT_TOKENS, Profile, Wire};
use psg_ai::{DocText, Task};

use crate::error::failed;
use crate::json::Args;
use crate::{Automation, Result, ToolError};

/// The longest question taken, in characters.
const MAX_QUESTION_CHARS: usize = 8000;

fn invalid(what: impl Into<String>) -> ToolError {
    ToolError::InvalidArgs(what.into())
}

/// The provider object: format, base_url, model, and optional api_version and max_tokens.
fn provider(v: Option<&Value>) -> Result<Profile> {
    let v = v.ok_or_else(|| Args::missing("provider"))?;
    let obj = v.as_object().ok_or_else(|| Args::wrong("provider", "an object with format, base_url and model"))?;
    for key in obj.keys() {
        if !matches!(key.as_str(), "format" | "base_url" | "model" | "api_version" | "max_tokens") {
            return Err(invalid(format!("provider has no field {key:?}; it takes format, base_url, model, api_version and max_tokens")));
        }
    }
    let text = |key: &str| -> Result<String> {
        match obj.get(key) {
            None | Some(Value::Null) => Ok(String::new()),
            Some(Value::String(s)) => Ok(s.clone()),
            Some(_) => Err(Args::wrong(&format!("provider.{key}"), "a string")),
        }
    };
    let wire = match text("format")?.as_str() {
        "openai" | "" => Wire::OpenAi,
        "anthropic" => Wire::Anthropic,
        "gemini" => Wire::Gemini,
        "azure" => Wire::Azure,
        other => return Err(invalid(format!("provider.format must be \"openai\", \"anthropic\", \"gemini\" or \"azure\", not {other:?}"))),
    };
    let max_tokens = match obj.get("max_tokens") {
        None | Some(Value::Null) => 0,
        Some(n) => n
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .filter(|n| *n <= MAX_OUTPUT_TOKENS)
            .ok_or_else(|| Args::wrong("provider.max_tokens", &format!("an integer from 0 to {MAX_OUTPUT_TOKENS}")))?,
    };
    let profile = Profile {
        id: "headless".into(),
        name: String::new(),
        preset: CUSTOM.into(),
        wire,
        base_url: text("base_url")?,
        model: text("model")?,
        api_version: text("api_version")?,
        max_tokens,
        has_key: false,
    };
    profile.check().map_err(|e| invalid(format!("provider: {e}")))?;
    Ok(profile)
}

impl Automation {
    pub(crate) fn ai_ask(&mut self, a: &Args) -> Result<Value> {
        let task = match a.str("task")? {
            "summarize" => Task::Summarize,
            "ask" => {
                let q = a.str("question")?.trim();
                if q.is_empty() || q.chars().count() > MAX_QUESTION_CHARS {
                    return Err(invalid(format!("question must have 1 to {MAX_QUESTION_CHARS} characters")));
                }
                Task::Ask(q.to_string())
            }
            "translate" => Task::Translate(a.opt_str("language")?.unwrap_or("English").trim().to_string()),
            other => return Err(invalid(format!("task must be \"summarize\", \"ask\" or \"translate\", not {other:?}"))),
        };
        let answer_language = a.opt_str("answer_language")?.unwrap_or("English").trim().to_string();
        if [answer_language.as_str(), if let Task::Translate(l) = &task { l } else { "x" }].iter().any(|l| l.is_empty() || l.chars().count() > 64) {
            return Err(invalid("language and answer_language must be language names of 1 to 64 characters"));
        }
        let profile = provider(a.get("provider"))?;
        let Some(transport) = self.ai_transport.clone() else {
            return Err(failed(
                "this host has no network access for AI providers: the automation tools and pdfcraft-cli open no sockets. Use the AI assistant in PSG PDF, or a host that provides a transport",
            ));
        };
        let doc = self.doc_to_copy(a)?;
        let (id, title, count) = (doc.id, doc.display_name(), doc.info.pages.len());
        let pages = match a.opt_ints("pages")? {
            Some(_) => self.pages(a, "pages")?,
            None => (0..count).collect(),
        };
        let texts = self.page_texts(id, &pages)?;
        let doc_text =
            DocText::from_pages(pages.iter().copied().zip(texts.iter().map(|t| t.plain_text())), pages.len(), psg_ai::task::DEFAULT_DOC_CHARS);
        if doc_text.is_blank() {
            return Err(failed("the pages have no text to send; run ocr first on scanned pages"));
        }
        let key = std::env::var(psg_ai::secret::KEY_ENV).ok().filter(|k| !k.trim().is_empty());
        let request = psg_ai::task::build_request(&task, &doc_text, &[], &answer_language, &title, profile.max_tokens);
        let reply = psg_ai::net::chat(&profile, key.as_deref(), &request, transport.as_ref()).map_err(|e| match e {
            psg_ai::Error::MissingKey => failed(format!("this provider needs an API key: set the {} environment variable", psg_ai::secret::KEY_ENV)),
            other => failed(other),
        })?;
        Ok(json!({
            "answer": reply.text,
            "answer_truncated": reply.truncated,
            "pages_sent": doc_text.pages_included,
            "pages_requested": doc_text.pages_requested,
            "text_truncated": doc_text.truncated,
        }))
    }
}
