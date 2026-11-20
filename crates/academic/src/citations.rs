//! Word citations as LaTeX citations. A citation that a reference manager (Zotero, Mendeley,
//! EndNote) put into the Word file carries its bibliography entry with it; Pandoc's `citations`
//! extension reads these into `Cite` elements and the document's `references`. [`convert`]
//! gives every entry a citation key, writes each citation as `\parencite` / `\textcite`
//! (BibLaTeX) or `\cite` (BibTeX) and drops the reference list Word printed, since the template
//! prints the bibliography itself. Citations typed as plain text stay as they are.

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use crate::docx::inline_text;
use crate::metadata::escape;

/// How the template cites.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    /// BibLaTeX (`\addbibresource`): `\parencite`, `\textcite`.
    Biblatex,
    /// BibTeX (`\bibliography`): `\cite`.
    Bibtex,
}

/// What [`convert`] did.
#[derive(Debug, Clone, Default)]
pub struct Converted {
    /// A Pandoc document holding only the entries, under their new keys: what Pandoc writes as
    /// BibLaTeX or BibTeX.
    pub references: Value,
    /// The keys given, in the order of the entries.
    pub keys: Vec<String>,
    /// The citations written as LaTeX.
    pub citations: usize,
    /// Whether the reference list Word printed was taken out of the body.
    pub removed_list: bool,
}

/// The most bibliography entries taken from one manuscript.
const MAX_ENTRIES: usize = 5000;
/// The most citations rewritten.
const MAX_CITATIONS: usize = 50_000;
/// How deep the walk for citations goes; serde_json already refuses deeper trees than 128.
const MAX_DEPTH: usize = 128;
/// The longest key.
const MAX_KEY: usize = 40;

/// Headings of the reference list a reference manager prints at the end of a Word manuscript.
const REFERENCE_HEADINGS: &[&str] = &[
    "references",
    "reference",
    "bibliography",
    "works cited",
    "literature cited",
    "literature",
    "參考文獻",
    "参考文献",
    "參考資料",
    "参考资料",
    "引用文獻",
    "引用文献",
    "文獻",
    "文献",
];

/// Turn the Word citations in `doc` into LaTeX citations in `style`, with keys not in `taken`.
/// `None` when the manuscript has no citations from a reference manager.
pub fn convert(doc: &mut Value, style: Style, taken: &HashSet<String>) -> Option<Converted> {
    let entries: Vec<Value> = match doc.get_mut("meta").and_then(Value::as_object_mut).and_then(|m| m.remove("references")) {
        Some(Value::Object(mut list)) if list.get("t").and_then(Value::as_str) == Some("MetaList") => match list.remove("c") {
            Some(Value::Array(items)) => items.into_iter().take(MAX_ENTRIES).collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    };
    if entries.is_empty() {
        return None;
    }
    let mut used: HashSet<String> = taken.iter().map(|k| k.to_lowercase()).collect();
    let mut renamed: HashMap<String, String> = HashMap::new();
    let mut keys = Vec::new();
    let mut kept = Vec::new();
    for mut entry in entries {
        let Some(map) = entry.get_mut("c").and_then(Value::as_object_mut) else { continue };
        let old = map.get("id").map(meta_text).unwrap_or_default();
        let key = unique_key(&base_key(map), &mut used);
        map.insert("id".into(), json!({"t": "MetaString", "c": key}));
        if !old.is_empty() {
            renamed.entry(old).or_insert_with(|| key.clone());
        }
        keys.push(key);
        kept.push(entry);
    }
    let mut citations = 0;
    if let Some(blocks) = doc.get_mut("blocks") {
        rewrite(blocks, style, &renamed, &mut citations, 0);
    }
    let removed_list = citations > 0 && doc.get_mut("blocks").and_then(Value::as_array_mut).is_some_and(remove_reference_list);
    let references = json!({
        "pandoc-api-version": doc.get("pandoc-api-version").cloned().unwrap_or(Value::Null),
        "meta": {"references": {"t": "MetaList", "c": kept}},
        "blocks": [],
    });
    Some(Converted { references, keys, citations, removed_list })
}

/// The text of a metadata value: a string, inlines, or the first item of a list.
fn meta_text(v: &Value) -> String {
    let c = v.get("c");
    match v.get("t").and_then(Value::as_str) {
        Some("MetaString") => c.and_then(Value::as_str).unwrap_or_default().to_owned(),
        Some("MetaInlines") => c.and_then(Value::as_array).map(|i| inline_text(i)).unwrap_or_default(),
        Some("MetaList") => c.and_then(Value::as_array).and_then(|l| l.first()).map(meta_text).unwrap_or_default(),
        Some("MetaMap") => c.and_then(|m| m.get("literal").or_else(|| m.get("family"))).map(meta_text).unwrap_or_default(),
        _ => String::new(),
    }
}

/// `smith2020` from an entry's first author (or editor, or title) and year.
fn base_key(entry: &serde_json::Map<String, Value>) -> String {
    let who = ["author", "editor"]
        .iter()
        .filter_map(|f| entry.get(*f))
        .map(meta_text)
        .find(|t| !t.trim().is_empty())
        .or_else(|| entry.get("title").map(meta_text))
        .unwrap_or_default();
    let mut key: String = who
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .take(MAX_KEY - 4)
        .collect();
    if !key.starts_with(|c: char| c.is_ascii_alphabetic()) {
        // A name in another script, or none: the key still has to be ASCII for BibTeX.
        key = format!("ref{key}");
    }
    let issued = entry.get("issued").map(meta_text).unwrap_or_default();
    let year: String =
        issued.as_bytes().windows(4).find(|w| w.iter().all(u8::is_ascii_digit)).map(|w| String::from_utf8_lossy(w).into_owned()).unwrap_or_default();
    key.push_str(&year);
    key
}

/// `base`, or `base` with a letter or number after it, not yet in `used` (compared without case,
/// as BibTeX does).
fn unique_key(base: &str, used: &mut HashSet<String>) -> String {
    let mut key = base.to_owned();
    let mut n = 0usize;
    while used.contains(&key.to_lowercase()) {
        let suffix = match u8::try_from(n).ok().filter(|n| *n < 26) {
            Some(i) => char::from(b'a' + i).to_string(),
            None => n.to_string(),
        };
        key = format!("{base}{suffix}");
        n += 1;
    }
    used.insert(key.to_lowercase());
    key
}

/// Replace every `Cite` whose entries all have keys with the LaTeX for it.
fn rewrite(v: &mut Value, style: Style, renamed: &HashMap<String, String>, count: &mut usize, depth: usize) {
    if depth > MAX_DEPTH || *count >= MAX_CITATIONS {
        return;
    }
    match v {
        Value::Array(items) => {
            for item in items {
                rewrite(item, style, renamed, count, depth + 1);
            }
        }
        Value::Object(map) => {
            if map.get("t").and_then(Value::as_str) == Some("Cite")
                && let Some(latex) = map.get("c").and_then(|c| cite_latex(c, style, renamed))
            {
                *v = json!({"t": "RawInline", "c": ["latex", latex]});
                *count += 1;
                return;
            }
            for (_, child) in map.iter_mut() {
                rewrite(child, style, renamed, count, depth + 1);
            }
        }
        _ => {}
    }
}

/// The LaTeX for one `Cite` (`[citations, text]`); `None` when one of its entries has no key.
fn cite_latex(c: &Value, style: Style, renamed: &HashMap<String, String>) -> Option<String> {
    let items = c.get(0)?.as_array()?;
    let mut keys = Vec::new();
    for item in items {
        let id = item.get("citationId")?.as_str()?;
        keys.push(renamed.get(id)?.as_str());
    }
    if keys.is_empty() {
        return None;
    }
    let text = |field: &str, item: Option<&Value>| {
        let inlines = item.and_then(|i| i.get(field)).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        escape(inline_text(inlines).trim().trim_start_matches([',', ';']).trim())
    };
    // In braces, so a `]` in the text can't end the optional argument.
    let braced = |t: String| if t.is_empty() { t } else { format!("{{{t}}}") };
    let prefix = braced(text("citationPrefix", items.first()));
    let suffix = braced(text("citationSuffix", items.last()));
    let mode = items.first().and_then(|i| i.get("citationMode")).and_then(|m| m.get("t")).and_then(Value::as_str).unwrap_or_default();
    let keys = keys.join(",");
    Some(match style {
        Style::Biblatex => {
            let command = match mode {
                "AuthorInText" => "\\textcite",
                "SuppressAuthor" => "\\parencite*",
                _ => "\\parencite",
            };
            match (prefix.is_empty(), suffix.is_empty()) {
                (true, true) => format!("{command}{{{keys}}}"),
                (true, false) => format!("{command}[{suffix}]{{{keys}}}"),
                (false, _) => format!("{command}[{prefix}][{suffix}]{{{keys}}}"),
            }
        }
        Style::Bibtex => {
            let lead = if prefix.is_empty() { String::new() } else { format!("{prefix} ") };
            if suffix.is_empty() { format!("{lead}\\cite{{{keys}}}") } else { format!("{lead}\\cite[{suffix}]{{{keys}}}") }
        }
    })
}

/// Take the last reference list heading and what follows it, up to the next heading of its level
/// or higher, out of `blocks`. Whether one was found.
fn remove_reference_list(blocks: &mut Vec<Value>) -> bool {
    let heading = |b: &Value| -> Option<(u64, String)> {
        if b.get("t").and_then(Value::as_str) != Some("Header") {
            return None;
        }
        let c = b.get("c")?;
        let level = c.get(0)?.as_u64()?;
        let text = inline_text(c.get(2)?.as_array()?);
        Some((level, text))
    };
    let Some((start, level)) = blocks.iter().enumerate().rev().find_map(|(i, b)| {
        let (level, text) = heading(b)?;
        let name = text.trim().trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, '.' | ' ' | ')')).trim_end_matches([':', '：']).trim();
        let name = name.to_lowercase();
        REFERENCE_HEADINGS.contains(&name.as_str()).then_some((i, level))
    }) else {
        return false;
    };
    let end = blocks.iter().enumerate().skip(start + 1).find(|(_, b)| heading(b).is_some_and(|(l, _)| l <= level)).map_or(blocks.len(), |(i, _)| i);
    blocks.drain(start..end);
    true
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn s(t: &str) -> Value {
        json!({"t": "Str", "c": t})
    }

    fn entry(id: &str, family: &str, year: &str) -> Value {
        json!({"t": "MetaMap", "c": {
            "id": {"t": "MetaString", "c": id},
            "type": {"t": "MetaString", "c": "article-journal"},
            "author": {"t": "MetaList", "c": [{"t": "MetaMap", "c": {"family": {"t": "MetaString", "c": family}}}]},
            "issued": {"t": "MetaString", "c": year},
        }})
    }

    fn cite(ids: &[&str], mode: &str, suffix: &str) -> Value {
        let items: Vec<Value> = ids
            .iter()
            .map(|id| {
                json!({"citationId": id, "citationPrefix": [], "citationSuffix": if suffix.is_empty() { json!([]) } else { json!([s(suffix)]) },
                "citationMode": {"t": mode}, "citationNoteNum": 0, "citationHash": 0})
            })
            .collect();
        json!({"t": "Cite", "c": [items, [s("(Smith, 2020)")]]})
    }

    fn header(text: &str) -> Value {
        json!({"t": "Header", "c": [1, ["", [], []], [s(text)]]})
    }

    fn doc() -> Value {
        json!({
            "pandoc-api-version": [1, 23, 1],
            "meta": {"references": {"t": "MetaList", "c": [entry("1", "Smith", "2020"), entry("2", "Smith", "2020"), entry("3", "王", "2019")]}},
            "blocks": [
                header("Introduction"),
                {"t": "Para", "c": [s("As"), cite(&["1"], "NormalCitation", ", p. 3"), s("and"), cite(&["2", "3"], "AuthorInText", ""), s("say.")]},
                {"t": "Para", "c": [s("Unknown"), cite(&["9"], "NormalCitation", "")]},
                header("References"),
                {"t": "Para", "c": [s("Smith, J. (2020). A.")]},
            ]
        })
    }

    #[test]
    fn citations_become_biblatex_with_keys_and_the_word_list_goes() {
        let mut d = doc();
        let taken = HashSet::from(["smith2020".to_owned()]);
        let c = convert(&mut d, Style::Biblatex, &taken).unwrap();
        assert_eq!(c.keys, ["smith2020a", "smith2020b", "ref2019"]);
        assert_eq!(c.citations, 2);
        assert!(c.removed_list);
        let para = &d["blocks"][1]["c"];
        assert_eq!(para[1]["c"][1], "\\parencite[{p. 3}]{smith2020a}");
        assert_eq!(para[3]["c"][1], "\\textcite{smith2020b,ref2019}");
        // A citation of an entry the manuscript doesn't have keeps Word's text.
        assert_eq!(d["blocks"][2]["c"][1]["t"], "Cite");
        assert_eq!(d["blocks"].as_array().unwrap().len(), 3, "the reference list is gone");
        assert_eq!(c.references["meta"]["references"]["c"][0]["c"]["id"]["c"], "smith2020a");
        assert!(d["meta"].get("references").is_none());
    }

    #[test]
    fn bibtex_citations_use_cite() {
        let mut d = doc();
        let c = convert(&mut d, Style::Bibtex, &HashSet::new()).unwrap();
        assert_eq!(c.keys[0], "smith2020");
        assert_eq!(d["blocks"][1]["c"][1]["c"][1], "\\cite[{p. 3}]{smith2020}");
    }

    #[test]
    fn a_manuscript_without_a_reference_manager_is_left_alone() {
        let mut d = json!({"meta": {}, "blocks": [header("References"), {"t": "Para", "c": [s("[1] A.")]}]});
        assert!(convert(&mut d, Style::Biblatex, &HashSet::new()).is_none());
        assert_eq!(d["blocks"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn hostile_suffixes_are_escaped() {
        let mut d = doc();
        d["blocks"][1]["c"][1] = cite(&["1"], "NormalCitation", "}\\input{/etc/passwd}");
        convert(&mut d, Style::Biblatex, &HashSet::new()).unwrap();
        let latex = d["blocks"][1]["c"][1]["c"][1].as_str().unwrap();
        assert!(!latex.contains("\\input"), "{latex}");
    }
}
