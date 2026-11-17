use serde_json::{Value, json};

use crate::Error;
use crate::provider::{self, CUSTOM, Profile, Settings, Wire, preset_by_id};
use crate::task::{self, DocText, Message, Task};
use crate::wire::{self, Request};

fn profile(preset: &str) -> Profile {
    let p = preset_by_id(preset).unwrap();
    let mut profile = Profile::from_preset(p, "p1".into());
    if profile.base_url.is_empty() {
        profile.base_url = "https://example.openai.azure.com/".into();
        profile.model = "my-deployment".into();
    }
    profile
}

fn request() -> Request {
    Request { system: "be brief".into(), messages: vec![Message::user("hi"), Message::assistant("hello"), Message::user("again")], max_tokens: 0 }
}

fn body(call: &wire::HttpCall) -> Value {
    serde_json::from_str(call.body.as_deref().unwrap()).unwrap()
}

fn header<'a>(call: &'a wire::HttpCall, name: &str) -> Option<&'a str> {
    call.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

#[test]
fn every_preset_makes_a_usable_profile() {
    let mut ids = std::collections::HashSet::new();
    for p in provider::PRESETS {
        assert!(ids.insert(p.id), "duplicate preset {}", p.id);
        if p.base_url.is_empty() {
            assert!(p.id == "azure" || p.id == CUSTOM, "{} has no address", p.id);
            continue;
        }
        assert!(p.base_url.starts_with("https://") || p.local, "{} is not https", p.id);
        let profile = Profile::from_preset(p, "x".into());
        if !p.models.is_empty() {
            profile.check().unwrap();
            let call = wire::chat_call(&profile, Some("k"), &request()).unwrap();
            assert!(call.url.starts_with(p.base_url), "{}", call.url);
        }
    }
}

#[test]
fn openai_chat_call_carries_the_system_message_and_bearer_key() {
    let p = profile("deepseek");
    let call = wire::chat_call(&p, Some(" sk-test "), &Request { max_tokens: 500, ..request() }).unwrap();
    assert_eq!(call.method, "POST");
    assert_eq!(call.url, "https://api.deepseek.com/v1/chat/completions");
    assert_eq!(header(&call, "authorization"), Some("Bearer sk-test"));
    let b = body(&call);
    assert_eq!(b["model"], "deepseek-chat");
    assert_eq!(b["messages"][0], json!({ "role": "system", "content": "be brief" }));
    assert_eq!(b["messages"][2], json!({ "role": "assistant", "content": "hello" }));
    assert_eq!(b["max_tokens"], 500);
    // OpenAI's own API takes max_completion_tokens.
    let call = wire::chat_call(&profile("openai"), Some("k"), &Request { max_tokens: 500, ..request() }).unwrap();
    assert_eq!(body(&call)["max_completion_tokens"], 500);
    assert!(body(&call).get("max_tokens").is_none());
}

#[test]
fn anthropic_chat_call_puts_the_system_apart_and_always_asks_for_a_length() {
    let call = wire::chat_call(&profile("anthropic"), Some("ak"), &request()).unwrap();
    assert_eq!(call.url, "https://api.anthropic.com/v1/messages");
    assert_eq!(header(&call, "x-api-key"), Some("ak"));
    assert_eq!(header(&call, "anthropic-version"), Some(wire::ANTHROPIC_VERSION));
    let b = body(&call);
    assert_eq!(b["system"], "be brief");
    assert_eq!(b["max_tokens"], wire::ANTHROPIC_DEFAULT_MAX_TOKENS);
    assert_eq!(b["messages"].as_array().unwrap().len(), 3);
    assert_eq!(b["messages"][0]["role"], "user");
}

#[test]
fn gemini_chat_call_names_the_model_in_the_path_and_the_assistant_model() {
    let mut p = profile("gemini");
    p.model = "models/gemini-2.5-pro".into();
    let call = wire::chat_call(&p, Some("gk"), &Request { max_tokens: 64, ..request() }).unwrap();
    assert_eq!(call.url, "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-pro:generateContent");
    assert_eq!(header(&call, "x-goog-api-key"), Some("gk"));
    assert!(!call.url.contains("gk"), "the key stays out of the address");
    let b = body(&call);
    assert_eq!(b["systemInstruction"]["parts"][0]["text"], "be brief");
    assert_eq!(b["contents"][1]["role"], "model");
    assert_eq!(b["generationConfig"]["maxOutputTokens"], 64);
}

#[test]
fn azure_chat_call_uses_the_deployment_and_api_version() {
    let mut p = profile("azure");
    p.model = "gpt 4o/x".into();
    let call = wire::chat_call(&p, Some("az"), &request()).unwrap();
    assert_eq!(call.url, "https://example.openai.azure.com/openai/deployments/gpt%204o%2Fx/chat/completions?api-version=2024-10-21");
    assert_eq!(header(&call, "api-key"), Some("az"));
    assert!(body(&call).get("model").is_none());
}

#[test]
fn keys_are_required_where_the_api_needs_one_and_refused_when_malformed() {
    assert_eq!(wire::chat_call(&profile("anthropic"), None, &request()), Err(Error::MissingKey));
    assert_eq!(wire::chat_call(&profile("openai"), Some("  "), &request()), Err(Error::MissingKey));
    // A local server needs none, and gets no Authorization header.
    let call = wire::chat_call(&profile("ollama"), None, &request()).unwrap();
    assert!(header(&call, "authorization").is_none());
    // A line break would add a header of its own.
    assert!(matches!(wire::chat_call(&profile("openai"), Some("sk\r\nX-Evil: 1"), &request()), Err(Error::Config(_))));
}

#[test]
fn debug_output_never_shows_the_key() {
    let call = wire::chat_call(&profile("openai"), Some("sk-secret-123"), &request()).unwrap();
    let shown = format!("{call:?}");
    assert!(!shown.contains("sk-secret-123"), "{shown}");
    assert!(shown.contains("<redacted>"));
}

#[test]
fn profiles_are_checked_before_anything_is_sent() {
    let mut p = profile("openai");
    p.base_url = "ftp://x".into();
    assert!(matches!(p.check(), Err(Error::Config(_))));
    p.base_url = "https://".into();
    assert!(matches!(p.check(), Err(Error::Config(_))));
    p.base_url = "https://api.example.com/v1".into();
    p.model = " ".into();
    assert!(matches!(p.check(), Err(Error::Config(_))));
    p.model = "m".into();
    p.check().unwrap();
    assert!(!p.unencrypted_remote());
    p.base_url = "http://192.168.1.5:11434/v1".into();
    assert!(p.unencrypted_remote());
    p.base_url = "http://localhost:11434/v1".into();
    assert!(!p.unencrypted_remote());
    assert!(provider::is_loopback("http://127.0.0.1:8080/v1"));
    assert!(provider::is_loopback("http://[::1]:8080/v1"));
    assert!(!provider::is_loopback("http://localhost.evil.com/v1"));
}

#[test]
fn chat_replies_are_read_in_each_format() {
    let openai = r#"{"choices":[{"message":{"role":"assistant","content":"Hi (p. 1)"},"finish_reason":"length"}]}"#;
    let r = wire::parse_chat(Wire::OpenAi, 200, openai).unwrap();
    assert_eq!((r.text.as_str(), r.truncated), ("Hi (p. 1)", true));
    let parts = r#"{"choices":[{"message":{"content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]},"finish_reason":"stop"}]}"#;
    assert_eq!(wire::parse_chat(Wire::OpenAi, 200, parts).unwrap().text, "ab");
    let anthropic = r#"{"content":[{"type":"thinking","thinking":"x"},{"type":"text","text":"Sum"}],"stop_reason":"end_turn"}"#;
    assert_eq!(wire::parse_chat(Wire::Anthropic, 200, anthropic).unwrap(), wire::Reply { text: "Sum".into(), truncated: false });
    let gemini = r#"{"candidates":[{"content":{"parts":[{"text":"thinking","thought":true},{"text":"G"}]},"finishReason":"MAX_TOKENS"}]}"#;
    assert_eq!(wire::parse_chat(Wire::Gemini, 200, gemini).unwrap(), wire::Reply { text: "G".into(), truncated: true });
    let blocked = r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#;
    assert!(matches!(wire::parse_chat(Wire::Gemini, 200, blocked), Err(Error::Reply(m)) if m.contains("SAFETY")));
    assert!(matches!(wire::parse_chat(Wire::OpenAi, 200, "<html>"), Err(Error::Reply(_))));
    assert!(matches!(wire::parse_chat(Wire::OpenAi, 200, r#"{"choices":[{"message":{"content":""}}]}"#), Err(Error::Reply(_))));
}

#[test]
fn error_statuses_carry_the_provider_s_message() {
    let e = wire::parse_chat(Wire::OpenAi, 401, r#"{"error":{"message":"Incorrect API key provided"}}"#).unwrap_err();
    assert_eq!(e, Error::Provider { status: 401, message: "Incorrect API key provided".into() });
    let e = wire::parse_chat(Wire::Anthropic, 404, r#"{"type":"error","error":{"type":"not_found_error","message":"model: x"}}"#).unwrap_err();
    assert_eq!(e, Error::Provider { status: 404, message: "model: x".into() });
    let e = wire::parse_chat(Wire::OpenAi, 502, "").unwrap_err();
    assert_eq!(e, Error::Provider { status: 502, message: "HTTP 502".into() });
    let long = "x".repeat(5000);
    let Error::Provider { message, .. } = wire::parse_chat(Wire::OpenAi, 500, &long).unwrap_err() else { panic!() };
    assert_eq!(message.chars().count(), 400);
}

#[test]
fn model_lists_are_read_sorted_and_without_duplicates() {
    let openai = r#"{"data":[{"id":"b"},{"id":"a"},{"id":"b"}]}"#;
    assert_eq!(wire::parse_models(Wire::OpenAi, 200, openai).unwrap(), ["a", "b"]);
    let gemini = r#"{"models":[{"name":"models/gemini-x","supportedGenerationMethods":["generateContent"]},{"name":"models/embed","supportedGenerationMethods":["embedContent"]}]}"#;
    assert_eq!(wire::parse_models(Wire::Gemini, 200, gemini).unwrap(), ["gemini-x"]);
    assert!(wire::parse_models(Wire::OpenAi, 200, r#"{"data":[]}"#).is_err());
    let call = wire::models_call(&profile("anthropic"), Some("k")).unwrap();
    assert_eq!((call.method, call.url.as_str()), ("GET", "https://api.anthropic.com/v1/models?limit=1000"));
    assert!(wire::models_call(&profile("azure"), Some("k")).is_err());
}

#[test]
fn restored_settings_are_bounded_and_never_hold_a_key() {
    let mut s = Settings::default();
    let mut p = profile("openai");
    p.has_key = true;
    s.upsert(p.clone()).unwrap();
    assert_eq!(s.active.as_deref(), Some("p1"), "the first provider added is the one in use");
    let saved = s.to_json();
    assert!(!saved.to_string().contains("sk-"), "{saved}");
    assert_eq!(Settings::from_json(&saved), s);

    let hostile = json!({
        "profiles": (0..100).map(|i| json!({ "id": if i < 2 { "same".to_string() } else { format!("id{i}") }, "name": "n".repeat(1000), "max_tokens": u32::MAX })).collect::<Vec<_>>(),
        "active": "missing"
    });
    let s = Settings::from_json(&hostile);
    assert!(s.profiles.len() < provider::MAX_PROFILES);
    assert_eq!(s.profiles.iter().filter(|p| p.id == "same").count(), 1);
    assert!(s.profiles.iter().all(|p| p.name.chars().count() == 80 && p.max_tokens == provider::MAX_OUTPUT_TOKENS));
    assert_eq!(s.active, None);
    assert_eq!(Settings::from_json(&json!("nonsense")), Settings::default());
    assert_eq!(Settings::from_json(&json!({ "profiles": [{ "wire": "telepathy" }] })), Settings::default());
}

#[test]
fn removing_the_provider_in_use_hands_over_to_the_next() {
    let mut s = Settings::default();
    s.upsert(profile("openai")).unwrap();
    let mut second = profile("gemini");
    second.id = s.new_id(1);
    s.upsert(second.clone()).unwrap();
    assert_ne!(second.id, "p1");
    s.remove("p1");
    assert_eq!(s.active.as_deref(), Some(second.id.as_str()));
    s.remove(&second.id);
    assert_eq!(s.active, None);
}

#[test]
fn document_text_keeps_pages_within_the_budget() {
    let pages = vec![(0, "alpha".to_string()), (1, String::new()), (2, "gamma".repeat(100))];
    let all = DocText::from_pages(pages.clone(), 3, 10_000);
    assert_eq!((all.pages_included, all.pages_requested, all.truncated), (3, 3, false));
    assert!(all.text.starts_with("[Page 1]\nalpha\n\n[Page 2]\n(no text on this page)"));
    let cut = DocText::from_pages(pages.clone(), 3, 60);
    assert_eq!((cut.pages_included, cut.pages_requested, cut.truncated), (2, 3, true));
    // No page past the budget is read.
    let mut read = 0;
    let lazy = DocText::from_pages(pages.iter().cloned().inspect(|_| read += 1), 3, 60);
    assert_eq!((lazy.pages_included, read), (2, 3));
    // The first page is cut to fit, not left out.
    let tiny = DocText::from_pages(vec![(0, "é".repeat(500))], 1, 20);
    assert_eq!((tiny.pages_included, tiny.text.chars().count()), (1, 20));
    assert!(DocText::from_pages(vec![(0, " ".into()), (1, String::new())], 2, 1000).is_blank());
    assert!(!all.is_blank());
}

#[test]
fn requests_carry_the_document_the_history_and_the_task() {
    let doc = DocText::from_pages(vec![(0, "Revenue rose 5%.".to_string())], 1, 1000);
    let history = vec![Message::user("Summarize"), Message::assistant("It rose (p. 1).")];
    let r = task::build_request(&Task::Ask("By how much?".into()), &doc, &history, "Japanese", "report.pdf", 0);
    assert!(r.system.contains("Revenue rose 5%.") && r.system.contains("report.pdf") && r.system.contains("Answer in Japanese"));
    assert_eq!(r.messages.len(), 3);
    assert_eq!(r.messages[2], Message::user("By how much?"));
    let t = task::build_request(&Task::Translate("German".into()), &doc, &[], "English", "a.pdf", 0);
    assert!(t.messages[0].content.contains("into German"));
    let cut = DocText { truncated: true, pages_included: 3, pages_requested: 9, ..doc };
    assert!(task::build_request(&Task::Summarize, &cut, &[], "English", "a.pdf", 0).system.contains("first 3 of the 9 pages"));
}

#[test]
fn chats_and_model_lists_go_through_the_transport_given() {
    use std::sync::Mutex;
    let seen = Mutex::new(Vec::new());
    let send = |call: &wire::HttpCall| {
        seen.lock().unwrap().push(call.clone());
        Ok((200, r#"{"choices":[{"message":{"content":"Local answer"}}],"data":[{"id":"m"}]}"#.to_string()))
    };
    let p = profile("ollama");
    assert_eq!(crate::net::chat(&p, None, &request(), &send).unwrap().text, "Local answer");
    assert_eq!(crate::net::models(&p, None, &send).unwrap(), ["m"]);
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen[0].url, "http://localhost:11434/v1/chat/completions");
    assert_eq!(seen[1].url, "http://localhost:11434/v1/models");
    let fail = |_: &wire::HttpCall| Ok((429, r#"{"error":{"message":"Rate limit reached"}}"#.to_string()));
    assert_eq!(crate::net::chat(&p, None, &request(), &fail).unwrap_err(), Error::Provider { status: 429, message: "Rate limit reached".into() });
    // A redirect the transport didn't follow is an error, not an answer.
    let moved = |_: &wire::HttpCall| Ok((302, String::new()));
    assert!(matches!(crate::net::models(&p, None, &moved), Err(Error::Provider { status: 302, .. })));
}

#[test]
fn memory_store_keeps_and_forgets_keys() {
    use crate::secret::{Memory, Saved, Store};
    let m = Memory::default();
    assert_eq!(m.load("a").unwrap(), None);
    assert_eq!(m.save("a", "k").unwrap(), Saved::SessionOnly);
    assert_eq!(m.load("a").unwrap().as_deref(), Some("k"));
    m.delete("a").unwrap();
    m.delete("a").unwrap();
    assert_eq!(m.load("a").unwrap(), None);
}
