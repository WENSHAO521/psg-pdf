//! Provider presets and the saved provider settings.

use serde::{Deserialize, Serialize};

use crate::Error;

/// The API a provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Wire {
    /// `POST {base}/chat/completions` with a bearer key: OpenAI and the many services that copy
    /// its API (DeepSeek, Mistral, Groq, OpenRouter, xAI, Ollama, LM Studio, vLLM, …).
    #[default]
    OpenAi,
    /// Anthropic's Messages API: `POST {base}/v1/messages` with `x-api-key`.
    Anthropic,
    /// Google's Gemini API: `POST {base}/v1beta/models/{model}:generateContent`.
    Gemini,
    /// Azure OpenAI: `POST {base}/openai/deployments/{deployment}/chat/completions?api-version=…`
    /// with an `api-key` header. The model is the deployment's name.
    Azure,
}

impl Wire {
    pub const ALL: [Wire; 4] = [Wire::OpenAi, Wire::Anthropic, Wire::Gemini, Wire::Azure];

    /// The name the interface shows for this API format.
    pub fn label(self) -> &'static str {
        match self {
            Wire::OpenAi => "OpenAI-compatible",
            Wire::Anthropic => "Anthropic Messages",
            Wire::Gemini => "Google Gemini",
            Wire::Azure => "Azure OpenAI",
        }
    }
}

/// A provider people can pick without knowing its address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Preset {
    /// Stable id, stored in profiles.
    pub id: &'static str,
    /// The provider's own name.
    pub name: &'static str,
    pub wire: Wire,
    /// The API's base address; empty where each account has its own (Azure, a custom service).
    pub base_url: &'static str,
    /// Models to offer before the provider's own list is fetched; the first is the default.
    pub models: &'static [&'static str],
    /// Does the provider refuse requests without a key? Local servers don't.
    pub needs_key: bool,
    /// Runs on this computer (or the local network): nothing leaves it.
    pub local: bool,
}

const fn preset(id: &'static str, name: &'static str, wire: Wire, base_url: &'static str, models: &'static [&'static str]) -> Preset {
    Preset { id, name, wire, base_url, models, needs_key: true, local: false }
}

const fn local(id: &'static str, name: &'static str, base_url: &'static str, models: &'static [&'static str]) -> Preset {
    Preset { id, name, wire: Wire::OpenAi, base_url, models, needs_key: false, local: true }
}

/// The id of the free-form preset: any address, any of the four API formats.
pub const CUSTOM: &str = "custom";

/// Every preset, in the order the picker lists them. Model names are suggestions; "Fetch models"
/// reads the provider's own list, and any name can be typed.
pub static PRESETS: &[Preset] = &[
    preset("openai", "OpenAI", Wire::OpenAi, "https://api.openai.com/v1", &["gpt-5-mini", "gpt-5", "gpt-4.1", "gpt-4o-mini"]),
    preset(
        "anthropic",
        "Anthropic Claude",
        Wire::Anthropic,
        "https://api.anthropic.com",
        &["claude-sonnet-5-5", "claude-opus-5-5", "claude-haiku-5-5"],
    ),
    preset("gemini", "Google Gemini", Wire::Gemini, "https://generativelanguage.googleapis.com", &["gemini-2.5-flash", "gemini-2.5-pro"]),
    preset("azure", "Azure OpenAI", Wire::Azure, "", &[]),
    preset("deepseek", "DeepSeek", Wire::OpenAi, "https://api.deepseek.com/v1", &["deepseek-chat", "deepseek-reasoner"]),
    preset("mistral", "Mistral AI", Wire::OpenAi, "https://api.mistral.ai/v1", &["mistral-large-latest", "mistral-small-latest"]),
    preset("xai", "xAI Grok", Wire::OpenAi, "https://api.x.ai/v1", &["grok-4"]),
    preset("groq", "Groq", Wire::OpenAi, "https://api.groq.com/openai/v1", &["llama-3.3-70b-versatile"]),
    preset("openrouter", "OpenRouter", Wire::OpenAi, "https://openrouter.ai/api/v1", &["openrouter/auto"]),
    preset("together", "Together AI", Wire::OpenAi, "https://api.together.xyz/v1", &["meta-llama/Llama-3.3-70B-Instruct-Turbo"]),
    preset("perplexity", "Perplexity", Wire::OpenAi, "https://api.perplexity.ai", &["sonar", "sonar-pro"]),
    preset(
        "qwen",
        "Alibaba Qwen (DashScope)",
        Wire::OpenAi,
        "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        &["qwen-plus", "qwen-max", "qwen-turbo"],
    ),
    preset("moonshot", "Moonshot Kimi", Wire::OpenAi, "https://api.moonshot.ai/v1", &["moonshot-v1-32k"]),
    preset("zhipu", "Zhipu GLM", Wire::OpenAi, "https://open.bigmodel.cn/api/paas/v4", &["glm-4.5", "glm-4-plus"]),
    preset("doubao", "Volcengine Doubao", Wire::OpenAi, "https://ark.cn-beijing.volces.com/api/v3", &[]),
    preset("siliconflow", "SiliconFlow", Wire::OpenAi, "https://api.siliconflow.cn/v1", &["deepseek-ai/DeepSeek-V3", "Qwen/Qwen2.5-72B-Instruct"]),
    local("ollama", "Ollama (local)", "http://localhost:11434/v1", &["llama3.2", "qwen2.5"]),
    local("lmstudio", "LM Studio (local)", "http://localhost:1234/v1", &[]),
    Preset { id: CUSTOM, name: "Custom", wire: Wire::OpenAi, base_url: "", models: &[], needs_key: false, local: false },
];

/// The preset with this id, if there is one.
pub fn preset_by_id(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// Limits on restored settings, which are untrusted like any file.
pub const MAX_PROFILES: usize = 32;
const MAX_NAME_CHARS: usize = 80;
const MAX_URL_CHARS: usize = 2048;
const MAX_MODEL_CHARS: usize = 256;
const MAX_VERSION_CHARS: usize = 64;
/// The largest reply length a profile may ask for, in tokens.
pub const MAX_OUTPUT_TOKENS: u32 = 200_000;
/// Azure OpenAI's generally available API version when the profile names none.
pub const DEFAULT_AZURE_API_VERSION: &str = "2024-10-21";

/// One saved provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    /// Names the key in the credential store; never shown.
    pub id: String,
    /// What the person calls it ("Work OpenAI").
    pub name: String,
    /// The preset it started from ([`CUSTOM`] for a free-form one).
    pub preset: String,
    pub wire: Wire,
    pub base_url: String,
    /// The model, or for Azure the deployment.
    pub model: String,
    /// Azure only; empty uses [`DEFAULT_AZURE_API_VERSION`].
    pub api_version: String,
    /// The longest reply to ask for, in tokens; 0 leaves it to the provider (Anthropic needs one,
    /// and gets 4096).
    pub max_tokens: u32,
    /// Whether a key was saved for it (the key itself is in the credential store).
    pub has_key: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            preset: CUSTOM.into(),
            wire: Wire::OpenAi,
            base_url: String::new(),
            model: String::new(),
            api_version: String::new(),
            max_tokens: 0,
            has_key: false,
        }
    }
}

impl Profile {
    /// A new profile from `preset`, with the preset's address and first model filled in.
    pub fn from_preset(preset: &Preset, id: String) -> Self {
        Self {
            id,
            name: preset.name.into(),
            preset: preset.id.into(),
            wire: preset.wire,
            base_url: preset.base_url.into(),
            model: preset.models.first().map_or_else(String::new, |m| (*m).to_string()),
            ..Self::default()
        }
    }

    /// The preset it started from.
    pub fn preset(&self) -> Option<&'static Preset> {
        preset_by_id(&self.preset)
    }

    /// Does this provider refuse requests without a key? A custom one may not need one (a local
    /// server), so only the presets that do are sure.
    pub fn needs_key(&self) -> bool {
        self.preset().is_some_and(|p| p.needs_key)
    }

    /// The name to show: the given name, else the model.
    pub fn display_name(&self) -> &str {
        if self.name.trim().is_empty() { &self.model } else { &self.name }
    }

    /// Why the profile can't send a request yet, if it can't.
    pub fn check(&self) -> Result<(), Error> {
        let url = self.base_url.trim();
        if url.is_empty() {
            return Err(Error::Config("enter the API address".into()));
        }
        let lower = url.to_ascii_lowercase();
        if !(lower.starts_with("https://") || lower.starts_with("http://")) {
            return Err(Error::Config("the API address must start with https:// or http://".into()));
        }
        if url.chars().any(|c| c.is_whitespace() || c.is_control()) || url.chars().count() > MAX_URL_CHARS {
            return Err(Error::Config("the API address is not a valid web address".into()));
        }
        if host_of(url).is_empty() {
            return Err(Error::Config("the API address has no host name".into()));
        }
        let model = self.model.trim();
        if model.is_empty() {
            return Err(Error::Config(if self.wire == Wire::Azure { "enter the deployment name".into() } else { "enter a model name".into() }));
        }
        if model.chars().any(char::is_control) || model.chars().count() > MAX_MODEL_CHARS {
            return Err(Error::Config("the model name is not valid".into()));
        }
        Ok(())
    }

    /// Plain HTTP to a host other than this computer: the key and the document text would cross
    /// the network unencrypted. The interface warns; it doesn't refuse (a server on the local
    /// network is a common set-up).
    pub fn unencrypted_remote(&self) -> bool {
        let url = self.base_url.trim();
        url.get(..7).is_some_and(|s| s.eq_ignore_ascii_case("http://")) && !is_loopback(url)
    }

    /// Clamp what a settings file restored to the limits above.
    fn sanitize(&mut self) {
        clamp_chars(&mut self.id, MAX_NAME_CHARS);
        clamp_chars(&mut self.name, MAX_NAME_CHARS);
        clamp_chars(&mut self.preset, MAX_NAME_CHARS);
        clamp_chars(&mut self.base_url, MAX_URL_CHARS);
        clamp_chars(&mut self.model, MAX_MODEL_CHARS);
        clamp_chars(&mut self.api_version, MAX_VERSION_CHARS);
        self.max_tokens = self.max_tokens.min(MAX_OUTPUT_TOKENS);
    }
}

fn clamp_chars(s: &mut String, max: usize) {
    if let Some((cut, _)) = s.char_indices().nth(max) {
        s.truncate(cut);
    }
}

/// The host of an `http(s)://host[:port]/…` address, lower-cased, without the port.
pub(crate) fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next().unwrap_or_default()
    } else {
        authority.split(':').next().unwrap_or_default()
    };
    host.to_ascii_lowercase()
}

/// Is the address on this computer? Those requests skip any proxy.
pub fn is_loopback(url: &str) -> bool {
    let host = host_of(url);
    host == "localhost" || host.ends_with(".localhost") || host == "::1" || host.starts_with("127.")
}

/// The saved providers and which one the assistant uses.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub profiles: Vec<Profile>,
    /// The id of the profile in use; `None` keeps the assistant off.
    pub active: Option<String>,
}

impl Settings {
    /// Settings restored from a settings file. Anything unreadable gives the defaults (no
    /// provider); too many profiles, overlong fields and duplicate ids are cut.
    pub fn from_json(value: &serde_json::Value) -> Self {
        let mut s: Settings = serde_json::from_value(value.clone()).unwrap_or_default();
        s.profiles.truncate(MAX_PROFILES);
        let mut seen = std::collections::HashSet::new();
        s.profiles.retain_mut(|p| {
            p.sanitize();
            !p.id.is_empty() && seen.insert(p.id.clone())
        });
        if s.active.as_ref().is_some_and(|a| !s.profiles.iter().any(|p| &p.id == a)) {
            s.active = None;
        }
        s
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_default()
    }

    /// The profile in use.
    pub fn active_profile(&self) -> Option<&Profile> {
        let id = self.active.as_ref()?;
        self.profiles.iter().find(|p| &p.id == id)
    }

    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// Add or replace (by id) a profile. The first one added becomes the one in use.
    pub fn upsert(&mut self, profile: Profile) -> Result<(), Error> {
        if let Some(slot) = self.profiles.iter_mut().find(|p| p.id == profile.id) {
            *slot = profile;
        } else {
            if self.profiles.len() >= MAX_PROFILES {
                return Err(Error::Config(format!("at most {MAX_PROFILES} providers can be saved")));
            }
            if self.active.is_none() {
                self.active = Some(profile.id.clone());
            }
            self.profiles.push(profile);
        }
        Ok(())
    }

    /// Remove a profile; when it was in use, the next one (if any) takes over.
    pub fn remove(&mut self, id: &str) {
        self.profiles.retain(|p| p.id != id);
        if self.active.as_deref() == Some(id) {
            self.active = self.profiles.first().map(|p| p.id.clone());
        }
    }

    /// A fresh profile id, unlike any saved one.
    pub fn new_id(&self, seed: u64) -> String {
        let mut n = seed;
        loop {
            let id = format!("p{n:x}");
            if !self.profiles.iter().any(|p| p.id == id) {
                return id;
            }
            n = n.wrapping_add(1);
        }
    }
}
