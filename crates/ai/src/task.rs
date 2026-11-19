//! What the assistant asks the model: the document's text and one of its tasks.

use crate::wire::Request;

/// Who wrote a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into() }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into() }
    }
}

/// One of the assistant's actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Task {
    /// An overview and the key points, with pages.
    Summarize,
    /// A question about the document.
    Ask(String),
    /// The text in another language (its English name: "Japanese").
    Translate(String),
}

/// The default share of a document a request carries, in characters: about 30 000 tokens of
/// English, which every current model takes, with room for the reply.
pub const DEFAULT_DOC_CHARS: usize = 120_000;

/// The document's text as a request carries it, each page under a `[Page N]` line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocText {
    pub text: String,
    /// Pages whose text is (at least partly) in [`DocText::text`].
    pub pages_included: usize,
    /// The pages that were asked for.
    pub pages_requested: usize,
    /// Some text was left out to stay within the budget.
    pub truncated: bool,
}

impl DocText {
    /// Gather `pages` (0-based number, text) of the `requested` asked for, within `budget`
    /// characters. A page that doesn't fit whole ends the text, and no later page is read (the
    /// iterator can extract lazily); the first page is cut to fit rather than left out.
    pub fn from_pages(pages: impl IntoIterator<Item = (usize, String)>, requested: usize, budget: usize) -> Self {
        let mut out = DocText { pages_requested: requested, ..DocText::default() };
        let mut used = 0usize;
        for (page, text) in pages {
            let text = text.trim();
            let block = format!("[Page {}]\n{}\n\n", page.saturating_add(1), if text.is_empty() { "(no text on this page)" } else { text });
            let chars = block.chars().count();
            if used.saturating_add(chars) <= budget {
                out.text.push_str(&block);
                used = used.saturating_add(chars);
                out.pages_included = out.pages_included.saturating_add(1);
            } else {
                if out.pages_included == 0 {
                    out.text.extend(block.chars().take(budget));
                    out.pages_included = 1;
                }
                out.truncated = true;
                break;
            }
        }
        out.truncated |= out.pages_included < requested;
        out
    }

    /// No page had any text: a scan that needs OCR first.
    pub fn is_blank(&self) -> bool {
        self.text.lines().all(|l| l.trim().is_empty() || l.starts_with("[Page ") || l == "(no text on this page)")
    }
}

/// The request for `task` on `doc`, after the conversation so far (`history`, oldest first).
/// `language` is the English name of the language to answer in; `title` names the document.
pub fn build_request(task: &Task, doc: &DocText, history: &[Message], language: &str, title: &str, max_tokens: u32) -> Request {
    let mut system = format!(
        "You are the AI assistant of a PDF application. Work only from the document text below, \
         extracted from the PDF \"{title}\"; each page starts with a [Page N] line. When you state \
         something the document says, cite the page as (p. N). If the document doesn't say, say \
         so instead of guessing. Answer in {language} unless asked for another language. Write \
         plain text: short paragraphs, or simple lists starting with \"- \"."
    );
    if doc.truncated {
        system.push_str(&format!(
            " Only the first {} of the {} pages asked for fit in this request; say so when the \
             answer may depend on the rest.",
            doc.pages_included, doc.pages_requested
        ));
    }
    system.push_str("\n\n<document>\n");
    system.push_str(&doc.text);
    system.push_str("</document>");
    let mut messages = history.to_vec();
    messages.push(Message::user(prompt(task)));
    Request { system, messages, max_tokens }
}

/// The person's message for `task`, as the conversation keeps it.
pub fn prompt(task: &Task) -> String {
    match task {
        Task::Summarize => "Summarize this document: one short overview paragraph, then its key points as a list, each with its page.".to_string(),
        Task::Ask(question) => question.trim().to_string(),
        Task::Translate(target) => {
            format!("Translate the document text into {target}. Keep the [Page N] lines and the paragraphs; translate everything and add nothing.")
        }
    }
}
