//! Reading a Word manuscript's structure from Pandoc's document tree (its JSON AST): the article's
//! title, authors, affiliations, abstract and keywords, and the figures and tables with their
//! captions, so a converted manuscript typesets without hand editing.
//!
//! Word writes captions as ordinary paragraphs ("Figure 1: …", "表 1：…") beside the picture or
//! table, which Pandoc keeps as paragraphs. Here a captioned picture becomes a LaTeX `figure` with
//! `\caption`, a table takes the caption paragraph next to it as its own caption, and the number
//! Word typed is dropped, since LaTeX numbers them. Front matter is taken from Pandoc's metadata
//! (Word's Title, Subtitle, Author and Abstract styles) or, when the manuscript uses plain
//! paragraphs, from the paragraphs before the first heading and from "Abstract" and "Keywords"
//! paragraphs, which are then removed from the body: the template sets them in its title block.
//!
//! The tree comes from a file the person chose, so every walk is bounded: depth, block counts and
//! text lengths.

use serde_json::{Value, json};

/// The deepest inline nesting walked.
const MAX_DEPTH: usize = 64;
/// The longest metadata value kept (characters).
const MAX_TEXT: usize = 20_000;
/// How many leading paragraphs may hold the title, authors and affiliations.
const MAX_FRONT: usize = 12;
/// The most keywords and authors kept.
const MAX_LIST: usize = 50;

/// One author as the manuscript lists them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordAuthor {
    pub name: String,
    /// 1-based affiliation numbers from superscripts after the name.
    pub affiliations: Vec<usize>,
    /// Marked with `*` (or `†`): the corresponding author.
    pub corresponding: bool,
}

/// The front matter found in a Word manuscript.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WordFrontMatter {
    pub title: String,
    pub subtitle: String,
    pub authors: Vec<WordAuthor>,
    pub affiliations: Vec<String>,
    pub abstract_text: String,
    pub keywords: Vec<String>,
    /// `zh-hant`, `zh-hans`, `ja` or `en`, from the body's characters.
    pub language: String,
}

/// What [`restructure`] changed, for the import log.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    pub figures: usize,
    pub tables: usize,
    pub front_matter_paragraphs: usize,
    /// Headings that carry the number Word's author typed ("一、", "1.2"), kept unnumbered by
    /// LaTeX so they are not numbered twice.
    pub typed_heading_numbers: usize,
}

/// Take the front matter out of `doc` (a Pandoc JSON document) and turn captioned pictures and
/// tables into numbered figures and tables.
pub fn restructure(doc: &mut Value) -> (WordFrontMatter, Changes) {
    let mut changes = Changes::default();
    let mut front = front_from_meta(doc.get("meta").unwrap_or(&Value::Null));
    let blocks = match doc.get_mut("blocks").and_then(Value::as_array_mut) {
        Some(b) => b,
        None => return (front, changes),
    };
    changes.front_matter_paragraphs = front_from_body(blocks, &mut front);
    let (figures, tables) = captions(blocks);
    changes.typed_heading_numbers = keep_typed_heading_numbers(blocks);
    changes.figures = figures;
    changes.tables = tables;
    front.language = language(blocks);
    // The template prints the front matter; Pandoc must not print it again.
    if let Some(meta) = doc.get_mut("meta").and_then(Value::as_object_mut) {
        for key in ["title", "subtitle", "author", "abstract", "keywords", "date"] {
            meta.remove(key);
        }
    }
    (front, changes)
}

// ---------------------------------------------------------------------------------------------
// Text of inlines and blocks.

/// The plain text of a list of inlines: words, spaces and the text of formatting around them.
/// Notes, images and citations are left out.
pub fn inline_text(inlines: &[Value]) -> String {
    let mut out = String::new();
    push_inlines(inlines, &mut out, 0);
    out
}

fn push_inlines(inlines: &[Value], out: &mut String, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for i in inlines {
        if out.len() > MAX_TEXT * 4 {
            return;
        }
        let c = i.get("c");
        match tag(i) {
            "Str" => out.push_str(c.and_then(Value::as_str).unwrap_or_default()),
            "Space" | "SoftBreak" => out.push(' '),
            "LineBreak" => out.push('\n'),
            "Emph" | "Strong" | "Underline" | "Strikeout" | "Superscript" | "Subscript" | "SmallCaps" => {
                push_inlines(c.and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default(), out, depth + 1)
            }
            "Quoted" | "Span" | "Link" => {
                let inner = c.and_then(|c| c.get(1)).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
                let (open, close) = match (tag(i), c.and_then(|c| c.get(0)).map(tag)) {
                    ("Quoted", Some("DoubleQuote")) => ("“", "”"),
                    ("Quoted", _) => ("‘", "’"),
                    _ => ("", ""),
                };
                out.push_str(open);
                push_inlines(inner, out, depth + 1);
                out.push_str(close);
            }
            "Code" => out.push_str(c.and_then(|c| c.get(1)).and_then(Value::as_str).unwrap_or_default()),
            // Kept as TeX math, which the metadata writes as math (`metadata::escape_math`).
            "Math" => {
                out.push_str("\\(");
                out.push_str(c.and_then(|c| c.get(1)).and_then(Value::as_str).unwrap_or_default().trim());
                out.push_str("\\)");
            }
            _ => {}
        }
    }
}

fn tag(v: &Value) -> &str {
    v.get("t").and_then(Value::as_str).unwrap_or_default()
}

/// The inlines of a `Para` or `Plain` block.
fn para_inlines(block: &Value) -> Option<&Vec<Value>> {
    matches!(tag(block), "Para" | "Plain").then(|| block.get("c").and_then(Value::as_array)).flatten()
}

fn block_text(block: &Value) -> String {
    match tag(block) {
        "Para" | "Plain" => para_inlines(block).map(|i| inline_text(i)).unwrap_or_default(),
        "Header" => block.get("c").and_then(|c| c.get(2)).and_then(Value::as_array).map(|i| inline_text(i)).unwrap_or_default(),
        _ => String::new(),
    }
}

fn clip(text: &str) -> String {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ");
    t.chars().take(MAX_TEXT).collect()
}

// ---------------------------------------------------------------------------------------------
// Front matter.

fn meta_text(v: &Value) -> String {
    match tag(v) {
        "MetaString" => v.get("c").and_then(Value::as_str).unwrap_or_default().to_owned(),
        "MetaInlines" => v.get("c").and_then(Value::as_array).map(|i| inline_text(i)).unwrap_or_default(),
        "MetaBlocks" => v
            .get("c")
            .and_then(Value::as_array)
            .map(|blocks| blocks.iter().map(block_text).filter(|t| !t.trim().is_empty()).collect::<Vec<_>>().join("\n\n"))
            .unwrap_or_default(),
        "MetaList" => v.get("c").and_then(Value::as_array).map(|l| l.iter().map(meta_text).collect::<Vec<_>>().join(", ")).unwrap_or_default(),
        _ => String::new(),
    }
}

fn meta_inlines(v: &Value) -> Vec<Value> {
    match tag(v) {
        "MetaInlines" => v.get("c").and_then(Value::as_array).cloned().unwrap_or_default(),
        "MetaString" => vec![json!({"t": "Str", "c": v.get("c").and_then(Value::as_str).unwrap_or_default()})],
        "MetaBlocks" => v.get("c").and_then(Value::as_array).and_then(|b| b.first()).and_then(para_inlines).cloned().unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn front_from_meta(meta: &Value) -> WordFrontMatter {
    let mut front = WordFrontMatter::default();
    let get = |k: &str| meta.get(k).map(meta_text).map(|t| clip(&t)).unwrap_or_default();
    front.title = get("title");
    front.subtitle = get("subtitle");
    front.abstract_text = meta.get("abstract").map(meta_text).map(|t| t.chars().take(MAX_TEXT).collect()).unwrap_or_default();
    if let Some(k) = meta.get("keywords") {
        front.keywords = split_keywords(&meta_text(k));
    }
    if let Some(a) = meta.get("author") {
        let items: Vec<Vec<Value>> = match tag(a) {
            "MetaList" => a.get("c").and_then(Value::as_array).map(|l| l.iter().map(meta_inlines).collect()).unwrap_or_default(),
            _ => vec![meta_inlines(a)],
        };
        for item in items.iter().take(MAX_LIST) {
            front.authors.extend(parse_authors(item));
        }
        front.authors.truncate(MAX_LIST);
    }
    front
}

/// The heading words that start an abstract.
const ABSTRACT_WORDS: &[&str] = &["abstract", "摘要", "摘 要", "提要", "内容提要", "內容提要", "summary"];
/// The words that start a keywords line.
const KEYWORD_WORDS: &[&str] = &["keywords", "key words", "keyword", "關鍵詞", "关键词", "關鍵字", "关键字", "關鍵詞語", "关键词语", "キーワード"];

/// `text` without a leading `word` and the colon after it, when it starts with one of `words`.
fn after_label<'a>(text: &'a str, words: &[&str]) -> Option<&'a str> {
    let t = text.trim_start();
    let lower = t.to_lowercase();
    for w in words {
        if lower.starts_with(w) {
            // Lowercasing keeps byte lengths for these words (ASCII and CJK).
            let rest = t.get(w.len()..)?;
            let rest_trim = rest.trim_start_matches([' ', '\u{3000}']);
            if rest_trim.is_empty() {
                return Some("");
            }
            if let Some(r) = rest_trim.strip_prefix([':', '：', '.', '．', '—', '-']) {
                return Some(r.trim_start_matches([' ', '\u{3000}']));
            }
            // "Abstract" alone on the line, or followed by a space and text.
            if rest.starts_with([' ', '\u{3000}']) && !w.chars().all(|c| c.is_ascii_alphabetic()) {
                return Some(rest_trim);
            }
        }
    }
    None
}

fn split_keywords(text: &str) -> Vec<String> {
    text.split([',', ';', '，', '；', '、', '\n'])
        .map(|k| k.trim().trim_end_matches(['.', '。']).trim())
        .filter(|k| !k.is_empty())
        .take(MAX_LIST)
        .map(|k| k.chars().take(200).collect())
        .collect()
}

/// Names separated by commas, `and` or `、`, each followed by superscript affiliation marks
/// (`1,2`, `*`).
fn parse_authors(inlines: &[Value]) -> Vec<WordAuthor> {
    let mut authors = Vec::new();
    let mut current = WordAuthor::default();
    let finish = |a: &mut WordAuthor, out: &mut Vec<WordAuthor>| {
        let name = clip(a.name.trim().trim_matches([',', '，', '、', ';', '；']));
        if !name.is_empty() && out.len() < MAX_LIST {
            out.push(WordAuthor { name, affiliations: std::mem::take(&mut a.affiliations), corresponding: a.corresponding });
        }
        *a = WordAuthor::default();
    };
    for i in inlines {
        match tag(i) {
            "Superscript" => {
                let marks = inline_text(i.get("c").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default());
                read_marks(&marks, &mut current);
            }
            "Str" => {
                let s = i.get("c").and_then(Value::as_str).unwrap_or_default();
                let mut word = String::new();
                for ch in s.chars() {
                    match ch {
                        ',' | '，' | '、' | ';' | '；' => {
                            current.name.push_str(&word);
                            word.clear();
                            finish(&mut current, &mut authors);
                        }
                        '*' | '†' => current.corresponding = true,
                        _ => word.push(ch),
                    }
                }
                if matches!(word.as_str(), "and" | "&" | "與" | "与" | "和" | "及") {
                    finish(&mut current, &mut authors);
                } else {
                    current.name.push_str(&word);
                }
            }
            "Space" | "SoftBreak" => {
                if !current.name.is_empty() {
                    current.name.push(' ');
                }
            }
            "LineBreak" => finish(&mut current, &mut authors),
            "Emph" | "Strong" | "Span" | "SmallCaps" => {
                let inner = match tag(i) {
                    "Span" => i.get("c").and_then(|c| c.get(1)),
                    _ => i.get("c"),
                };
                current.name.push_str(&inline_text(inner.and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default()));
            }
            _ => {}
        }
    }
    finish(&mut current, &mut authors);
    authors
}

fn read_marks(marks: &str, author: &mut WordAuthor) {
    for part in marks.split([',', '，', ' ', ';']) {
        let part = part.trim();
        if part.contains(['*', '†']) {
            author.corresponding = true;
        }
        let digits: String = part.chars().filter(char::is_ascii_digit).collect();
        if let Ok(n) = digits.parse::<usize>()
            && (1..=MAX_LIST).contains(&n)
            && !author.affiliations.contains(&n)
        {
            author.affiliations.push(n);
        }
    }
}

/// An affiliation line: a leading number (plain or superscript) and an institution.
fn affiliation_line(inlines: &[Value]) -> Option<String> {
    let first = inlines.first()?;
    let text = clip(&inline_text(inlines));
    let numbered = tag(first) == "Superscript" || text.starts_with(|c: char| c.is_ascii_digit());
    let institution = [
        "universit",
        "institute",
        "college",
        "school",
        "department",
        "academy",
        "laboratory",
        "center",
        "centre",
        "大學",
        "大学",
        "學院",
        "学院",
        "研究所",
        "研究院",
        "系",
        "中心",
        "學系",
        "学系",
    ]
    .iter()
    .any(|w| text.to_lowercase().contains(w));
    (numbered || institution).then(|| text.trim_start_matches(|c: char| c.is_ascii_digit() || matches!(c, ' ' | '.' | ')')).trim().to_owned())
}

/// Take the title block, abstract and keywords out of the body. Returns how many blocks went.
fn front_from_body(blocks: &mut Vec<Value>, front: &mut WordFrontMatter) -> usize {
    let mut remove = Vec::new();
    // Title, authors and affiliations: the paragraphs before the first heading or abstract.
    let first_heading = blocks.iter().position(|b| tag(b) == "Header" && after_label(&block_text(b), ABSTRACT_WORDS).is_none());
    let labelled =
        blocks.iter().position(|b| after_label(&block_text(b), ABSTRACT_WORDS).is_some() || after_label(&block_text(b), KEYWORD_WORDS).is_some());
    let lead_end = labelled.into_iter().chain(first_heading).min().unwrap_or(0).min(MAX_FRONT);
    let lead: Vec<usize> = (0..lead_end).filter(|&i| blocks.get(i).is_some_and(|b| !block_text(b).trim().is_empty())).collect();
    // Only a short lead looks like a title block; a manuscript starting with prose keeps it.
    let short = !lead.is_empty() && lead.iter().all(|&i| blocks.get(i).is_some_and(|b| block_text(b).chars().count() <= 300));
    // Short paragraphs alone are not enough: an introduction can be short too. The lead is a title
    // block only when something says so: Word's Title style gave the title, an abstract or
    // keywords label follows it, or one of its lines is an affiliation.
    let evidence = !front.title.is_empty()
        || labelled.is_some_and(|l| l == lead_end)
        || lead.iter().any(|&i| blocks.get(i).and_then(para_inlines).is_some_and(|p| affiliation_line(p).is_some()));
    let looks_like_title_block = short && evidence;
    if looks_like_title_block {
        let mut iter = lead.into_iter();
        if front.title.is_empty()
            && let Some(i) = iter.next()
        {
            front.title = clip(&blocks.get(i).map(block_text).unwrap_or_default());
            remove.push(i);
        } else if !front.title.is_empty() {
            // The metadata had the title: the lead may still repeat it.
            iter = iter.collect::<Vec<_>>().into_iter();
        }
        for i in iter {
            let Some(inlines) = blocks.get(i).and_then(para_inlines).cloned() else { continue };
            if let Some(aff) = affiliation_line(&inlines) {
                if front.affiliations.len() < MAX_LIST {
                    front.affiliations.push(aff);
                }
                remove.push(i);
            } else if front.authors.is_empty() {
                front.authors = parse_authors(&inlines);
                remove.push(i);
            } else if clip(&inline_text(&inlines)) == front.title || clip(&inline_text(&inlines)) == front.subtitle {
                remove.push(i);
            }
        }
    }
    // Abstract: a heading or paragraph labelled Abstract, with the paragraphs after it up to the
    // next heading or keywords line.
    if let Some(start) = blocks.iter().position(|b| after_label(&block_text(b), ABSTRACT_WORDS).is_some()) {
        let mut parts = Vec::new();
        let first_text = blocks.get(start).map(block_text).unwrap_or_default();
        let rest = after_label(&first_text, ABSTRACT_WORDS).unwrap_or_default().trim().to_owned();
        if !rest.is_empty() {
            parts.push(rest);
        }
        remove.push(start);
        let mut i = start + 1;
        while let Some(b) = blocks.get(i) {
            if tag(b) == "Header" || after_label(&block_text(b), KEYWORD_WORDS).is_some() || !matches!(tag(b), "Para" | "Plain") {
                break;
            }
            // An unlabelled abstract runs one paragraph unless the label stood alone.
            if !parts.is_empty() && tag(blocks.get(start).unwrap_or(&Value::Null)) != "Header" && !first_text.trim().ends_with(['：', ':']) {
                break;
            }
            parts.push(block_text(b));
            remove.push(i);
            i += 1;
        }
        if front.abstract_text.is_empty() {
            front.abstract_text = parts.join("\n\n").chars().take(MAX_TEXT).collect();
        }
    }
    if let Some(k) = blocks.iter().position(|b| after_label(&block_text(b), KEYWORD_WORDS).is_some()) {
        let text = blocks.get(k).map(block_text).unwrap_or_default();
        if front.keywords.is_empty() {
            front.keywords = split_keywords(after_label(&text, KEYWORD_WORDS).unwrap_or_default());
        }
        remove.push(k);
    }
    remove.sort_unstable();
    remove.dedup();
    for &i in remove.iter().rev() {
        if i < blocks.len() {
            blocks.remove(i);
        }
    }
    remove.len()
}

// ---------------------------------------------------------------------------------------------
// Figures and tables.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptionKind {
    Figure,
    Table,
}

/// Whether `text` is a caption ("Figure 2: …", "Fig. 2.1 …", "圖2　…", "表 1：…") and the number of
/// characters of its label, number and separator.
fn caption_label(text: &str) -> Option<(CaptionKind, usize)> {
    let t = text.trim_start();
    let skipped = text.chars().count() - t.chars().count();
    let lower = t.to_lowercase();
    let labels: [(&str, CaptionKind); 9] = [
        ("figure", CaptionKind::Figure),
        ("fig.", CaptionKind::Figure),
        ("fig", CaptionKind::Figure),
        ("圖", CaptionKind::Figure),
        ("图", CaptionKind::Figure),
        ("table", CaptionKind::Table),
        ("tab.", CaptionKind::Table),
        ("表", CaptionKind::Table),
        ("abbildung", CaptionKind::Figure),
    ];
    let (word, kind) = labels.iter().find(|(w, _)| lower.starts_with(w))?;
    let rest = lower.get(word.len()..)?;
    let mut chars = rest.chars().peekable();
    let mut used = word.chars().count();
    while chars.next_if(|c| matches!(c, ' ' | '\u{3000}')).is_some() {
        used += 1;
    }
    let mut digits = 0;
    while chars.next_if(|c| c.is_ascii_digit() || "一二三四五六七八九十〇零".contains(*c) || (digits > 0 && matches!(c, '.' | '-' | '–'))).is_some()
    {
        digits += 1;
        used += 1;
    }
    if digits == 0 {
        return None;
    }
    let mut separated = false;
    while chars.next_if(|c| matches!(c, ':' | '：' | '.' | '．' | '、' | ' ' | '\u{3000}' | '—' | '-')).is_some() {
        separated = true;
        used += 1;
    }
    (separated || chars.peek().is_none()).then_some((*kind, skipped + used))
}

/// `inlines` without their first `n` characters (as [`inline_text`] counts them).
fn drop_chars(inlines: &[Value], mut n: usize) -> Vec<Value> {
    let mut out = Vec::new();
    for i in inlines {
        if n == 0 {
            out.push(i.clone());
            continue;
        }
        match tag(i) {
            "Str" => {
                let s = i.get("c").and_then(Value::as_str).unwrap_or_default();
                let len = s.chars().count();
                if len <= n {
                    n -= len;
                } else {
                    let kept: String = s.chars().skip(n).collect();
                    n = 0;
                    out.push(json!({"t": "Str", "c": kept}));
                }
            }
            "Space" | "SoftBreak" | "LineBreak" => n -= 1,
            "Emph" | "Strong" | "Underline" | "SmallCaps" => {
                let inner = i.get("c").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
                let len = inline_text(inner).chars().count();
                if len <= n {
                    n -= len;
                } else {
                    let kept = drop_chars(inner, n);
                    n = 0;
                    out.push(json!({"t": tag(i), "c": kept}));
                }
            }
            _ => {
                // Something without countable text (an image, a note): keep it, stop dropping.
                n = 0;
                out.push(i.clone());
            }
        }
    }
    // Leading spaces left after the label.
    while out.first().is_some_and(|i| matches!(tag(i), "Space" | "SoftBreak")) {
        out.remove(0);
    }
    out
}

/// The caption inlines of a caption paragraph, without Word's label and number.
fn caption_body(inlines: &[Value], kind: CaptionKind) -> Option<Vec<Value>> {
    let (found, label) = caption_label(&inline_text(inlines))?;
    (found == kind).then(|| drop_chars(inlines, label))
}

/// The one image a paragraph holds, if that is all it holds.
fn lone_image(block: &Value) -> Option<&Value> {
    let inlines = para_inlines(block)?;
    let mut images = inlines.iter().filter(|i| !matches!(tag(i), "Space" | "SoftBreak" | "LineBreak"));
    let first = images.next()?;
    (tag(first) == "Image" && images.next().is_none()).then_some(first)
}

fn raw(tex: &str) -> Value {
    json!({"t": "RawInline", "c": ["latex", tex]})
}

/// The `\includegraphics` options for a picture `width_in` inches wide in Word: as wide as the
/// text column when it was wider than half of it, otherwise its own width, never taller than half
/// the page.
fn graphics_options(attrs: &[Value]) -> String {
    let width = attrs.iter().find(|kv| kv.get(0).and_then(Value::as_str) == Some("width")).and_then(|kv| kv.get(1)).and_then(Value::as_str);
    let inches = width.and_then(|w| w.strip_suffix("in")).and_then(|w| w.parse::<f64>().ok()).filter(|w| w.is_finite() && *w > 0.0);
    match inches {
        Some(w) if w < 3.0 => format!("width={w:.2}in,height=0.5\\textheight,keepaspectratio"),
        _ => "width=\\linewidth,height=0.5\\textheight,keepaspectratio".to_owned(),
    }
}

/// Escape a path for `\includegraphics` (Pandoc's media names are plain, but a folder may not be).
fn graphics_path(path: &str) -> Option<String> {
    (!path.is_empty() && !path.contains(['\\', '{', '}', '%', '#', '$', '&', '^', '~', '\n']) && !path.starts_with('/') && !path.contains(".."))
        .then(|| path.to_owned())
}

fn figure(image: &Value, caption: Vec<Value>, n: usize) -> Option<Value> {
    let c = image.get("c")?;
    let attrs = c.get(0).and_then(|a| a.get(2)).and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
    let path = graphics_path(c.get(2).and_then(|t| t.get(0)).and_then(Value::as_str)?)?;
    let mut inlines =
        vec![raw(&format!("\\begin{{figure}}[!htbp]\n\\centering\n\\includegraphics[{}]{{{path}}}\n\\caption{{", graphics_options(attrs)))];
    inlines.extend(caption);
    inlines.push(raw(&format!("}}\\label{{fig:word-{n}}}\n\\end{{figure}}")));
    Some(json!({"t": "Plain", "c": inlines}))
}

/// A table's caption inlines (Pandoc ≥ 2.10's `Caption (Maybe ShortCaption) [Block]`).
fn table_caption_mut(table: &mut Value) -> Option<&mut Vec<Value>> {
    table.get_mut("c")?.get_mut(1)?.get_mut(1)?.as_array_mut()
}

/// Make captioned pictures figures and give tables the caption Word set beside them. Returns the
/// figures and tables changed.
fn captions(blocks: &mut Vec<Value>) -> (usize, usize) {
    let (mut figures, mut tables) = (0, 0);
    let mut i = 0;
    while i < blocks.len() && i < 1_000_000 {
        let kind = tag(&blocks[i]).to_owned();
        if kind == "Table" {
            // The caption Pandoc read, and the paragraphs before and after it.
            let own = table_caption_mut(&mut blocks[i])
                .map(|b| b.iter().filter_map(para_inlines).flatten().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            let own_text = clip(&inline_text(&own));
            let mut caption = (!own.is_empty()).then(|| caption_body(&own, CaptionKind::Table).unwrap_or(own.clone()));
            let mut drop = None;
            for at in [i.checked_sub(1), Some(i + 1)].into_iter().flatten() {
                let Some(inl) = blocks.get(at).and_then(para_inlines) else { continue };
                let Some(body) = caption_body(inl, CaptionKind::Table) else { continue };
                let same = clip(&inline_text(inl)) == own_text;
                if caption.is_none() || same {
                    if caption.is_none() {
                        caption = Some(body);
                    }
                    drop = Some(at);
                    break;
                }
            }
            if let Some(cap) = caption {
                if let Some(slot) = table_caption_mut(&mut blocks[i]) {
                    *slot = vec![json!({"t": "Plain", "c": cap})];
                    tables += 1;
                }
                // The short caption (for a list of tables) still has Word's typed number.
                if let Some(short) = blocks[i].get_mut("c").and_then(|c| c.get_mut(1)).and_then(|c| c.get_mut(0)) {
                    *short = Value::Null;
                }
            }
            if let Some(at) = drop {
                blocks.remove(at);
                if at < i {
                    i -= 1;
                }
            }
        } else if let Some(image) = lone_image(&blocks[i]).cloned() {
            let after = blocks.get(i + 1).and_then(para_inlines).and_then(|inl| caption_body(inl, CaptionKind::Figure));
            let before = i.checked_sub(1).and_then(|b| blocks.get(b)).and_then(para_inlines).and_then(|inl| caption_body(inl, CaptionKind::Figure));
            // The picture's own description, when Word's caption paragraph is missing.
            let alt = image.get("c").and_then(|c| c.get(1)).and_then(Value::as_array).and_then(|a| caption_body(a, CaptionKind::Figure));
            let (caption, drop) = match (after, before, alt) {
                (Some(c), _, _) => (Some(c), Some(i + 1)),
                (None, Some(c), _) => (Some(c), i.checked_sub(1)),
                (None, None, Some(c)) => (Some(c), None),
                _ => (None, None),
            };
            if let Some(caption) = caption
                && let Some(fig) = figure(&image, caption, figures + 1)
            {
                blocks[i] = fig;
                figures += 1;
                if let Some(at) = drop {
                    blocks.remove(at);
                    if at < i {
                        i -= 1;
                    }
                }
            }
        }
        i += 1;
    }
    (figures, tables)
}

// ---------------------------------------------------------------------------------------------
// Headings.

/// Chinese numerals as headings number them.
const CHINESE_NUMERALS: &str = "一二三四五六七八九十百零〇壹貳參肆伍陸柒捌玖拾";

/// Whether a heading starts with a number its author typed: "一、", "（二）", "第三章", "1.",
/// "2.3 ", "IV.". A year or a number that is part of the title ("1990年代") is not one.
fn typed_heading_number(text: &str) -> bool {
    let text = text.trim_start();
    let numeral = |c: char| CHINESE_NUMERALS.contains(c);
    // （一） (一)
    if let Some(rest) = text.strip_prefix(['（', '(']) {
        let n = rest.chars().take_while(|c| numeral(*c) || c.is_ascii_digit()).count();
        return n > 0 && rest.chars().nth(n).is_some_and(|c| c == '）' || c == ')');
    }
    // 第三章, 第一節
    if let Some(rest) = text.strip_prefix('第') {
        let n = rest.chars().take_while(|c| numeral(*c) || c.is_ascii_digit()).count();
        return n > 0 && rest.chars().nth(n).is_some_and(|c| "章節节部篇".contains(c));
    }
    // 一、 十二．
    let n = text.chars().take_while(|c| numeral(*c)).count();
    if n > 0 {
        return text.chars().nth(n).is_some_and(|c| "、．.　 ".contains(c));
    }
    // 1. 1.2 1.2.3 1、 (at most two digits a part)
    let mut chars = text.chars().peekable();
    let mut parts = 0;
    loop {
        let digits = std::iter::from_fn(|| chars.next_if(char::is_ascii_digit)).count();
        if digits == 0 || digits > 2 {
            break;
        }
        parts += 1;
        match chars.peek() {
            Some('.' | '．') => {
                chars.next();
                if chars.peek().is_some_and(char::is_ascii_digit) {
                    continue;
                }
                return true;
            }
            Some('、' | ' ' | '　' | ')' | '）') => return true,
            _ => return false,
        }
    }
    if parts > 0 {
        return false;
    }
    // I. IV. (Roman numerals with a full stop)
    let roman = text.chars().take_while(|c| "IVX".contains(*c)).count();
    roman > 0 && text.chars().nth(roman) == Some('.')
}

/// Mark the headings that carry a typed number unnumbered. Returns how many there were.
fn keep_typed_heading_numbers(blocks: &mut [Value]) -> usize {
    let mut changed = 0;
    for block in blocks.iter_mut().filter(|b| tag(b) == "Header") {
        let text = block.get("c").and_then(|c| c.get(2)).and_then(Value::as_array).map(|i| inline_text(i)).unwrap_or_default();
        if !typed_heading_number(&text) {
            continue;
        }
        if let Some(classes) = block.get_mut("c").and_then(|c| c.get_mut(1)).and_then(|a| a.get_mut(1)).and_then(Value::as_array_mut) {
            if !classes.iter().any(|c| c == "unnumbered") {
                classes.push(json!("unnumbered"));
            }
            changed += 1;
        }
    }
    changed
}

// ---------------------------------------------------------------------------------------------
// Language.

/// Characters only Traditional and only Simplified Chinese write, and kana.
const TRADITIONAL: &str = "這個們來為與學國說時對會發經數據體關於點電開問題實際應過還現從後當將區劃們條續認識論藝術歷達選環產業務標準議縣傳統際";
const SIMPLIFIED: &str = "这个们来为与学国说时对会发经数据体关于点电开问题实际应过还现从后当将区划们条续认识论艺术历达选环产业务标准议县传统际";

fn language(blocks: &[Value]) -> String {
    let (mut han, mut kana, mut latin, mut trad, mut simp) = (0usize, 0usize, 0usize, 0usize, 0usize);
    for b in blocks.iter().take(5_000) {
        for c in block_text(b).chars() {
            match c {
                '\u{3040}'..='\u{30FF}' => kana += 1,
                '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' => {
                    han += 1;
                    // The two lists share characters that are the same in both scripts.
                    if TRADITIONAL.contains(c) && !SIMPLIFIED.contains(c) {
                        trad += 1;
                    } else if SIMPLIFIED.contains(c) && !TRADITIONAL.contains(c) {
                        simp += 1;
                    }
                }
                c if c.is_ascii_alphabetic() => latin += 1,
                _ => {}
            }
        }
    }
    // A CJK character carries about as much as a Latin word: compare against letters / 5.
    if kana > 0 && kana * 10 >= han {
        "ja".into()
    } else if han * 5 >= latin {
        if simp > trad { "zh-hans".into() } else { "zh-hant".into() }
    } else {
        "en".into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(t: &str) -> Value {
        json!({"t": "Str", "c": t})
    }
    fn sup(t: &str) -> Value {
        json!({"t": "Superscript", "c": [s(t)]})
    }
    fn sp() -> Value {
        json!({"t": "Space"})
    }
    fn para(inl: Vec<Value>) -> Value {
        json!({"t": "Para", "c": inl})
    }
    fn words(text: &str) -> Vec<Value> {
        let mut out = Vec::new();
        for (n, w) in text.split(' ').enumerate() {
            if n > 0 {
                out.push(sp());
            }
            out.push(s(w));
        }
        out
    }
    fn image(path: &str, width: &str) -> Value {
        json!({"t": "Image", "c": [["", [], [["width", width]]], [], [path, ""]]})
    }
    fn table(caption: Vec<Value>) -> Value {
        // Pandoc gives a table caption from Word a short form too, with the typed number.
        let (short, blocks) =
            if caption.is_empty() { (Value::Null, json!([])) } else { (json!(caption.clone()), json!([{"t": "Plain", "c": caption}])) };
        json!({"t": "Table", "c": [["", [], []], [short, blocks], [], [], [], []]})
    }

    #[test]
    fn caption_labels_in_english_and_chinese() {
        assert_eq!(caption_label("Figure 1: Workflow"), Some((CaptionKind::Figure, 10)));
        assert_eq!(caption_label("Fig. 2.1. Map"), Some((CaptionKind::Figure, 10)));
        assert_eq!(caption_label("圖 1：研究流程"), Some((CaptionKind::Figure, 4)));
        assert_eq!(caption_label("表1　樣本"), Some((CaptionKind::Table, 3)));
        assert_eq!(caption_label("Table 3"), Some((CaptionKind::Table, 7)));
        assert_eq!(caption_label("Tables show"), None);
        assert_eq!(caption_label("表示方法"), None);
        assert_eq!(caption_label("Figure shows"), None);
    }

    #[test]
    fn a_picture_and_the_caption_under_it_become_a_figure() {
        let mut doc = json!({"meta": {}, "blocks": [
            para(vec![image("media/rId21.png", "5.8in")]),
            para(words("Figure 1: Research workflow")),
            para(words("Text after.")),
        ]});
        let (_, changes) = restructure(&mut doc);
        assert_eq!(changes.figures, 1);
        let blocks = doc["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), 2, "the caption paragraph is gone");
        let fig = serde_json::to_string(&blocks[0]).unwrap();
        assert!(fig.contains("\\\\begin{figure}") && fig.contains("\\\\caption{"), "{fig}");
        assert!(fig.contains("{media/rId21.png}") && fig.contains("width=\\\\linewidth"), "{fig}");
        assert_eq!(inline_text(blocks[0]["c"].as_array().unwrap()), "Research workflow", "the number Word typed is dropped");
    }

    #[test]
    fn a_picture_without_a_caption_stays_as_it_is() {
        let mut doc = json!({"meta": {}, "blocks": [para(vec![image("media/a.png", "2in")]), para(words("Ordinary text."))]});
        let before = doc.clone();
        assert_eq!(restructure(&mut doc).1.figures, 0);
        assert_eq!(doc["blocks"], before["blocks"]);
    }

    #[test]
    fn a_table_takes_the_caption_beside_it_once() {
        let mut doc = json!({"meta": {}, "blocks": [
            para(words("表 1 列出樣本統計。")),
            para(vec![s("表"), sp(), s("1：樣本統計")]),
            table(vec![s("表"), sp(), s("1：樣本統計")]),
        ]});
        let (_, changes) = restructure(&mut doc);
        assert_eq!(changes.tables, 1);
        let blocks = doc["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), 2, "{blocks:?}");
        assert_eq!(block_text(&blocks[0]), "表 1 列出樣本統計。", "a sentence mentioning the table stays");
        let cap = blocks[1]["c"][1][1][0]["c"].as_array().unwrap();
        assert_eq!(inline_text(cap), "樣本統計");
        assert!(blocks[1]["c"][1][0].is_null(), "no short caption with Word's number");

        let mut doc = json!({"meta": {}, "blocks": [para(words("Table 2. Sample")), table(vec![])]});
        restructure(&mut doc);
        assert_eq!(doc["blocks"].as_array().unwrap().len(), 1);
        assert_eq!(inline_text(doc["blocks"][0]["c"][1][1][0]["c"].as_array().unwrap()), "Sample");
    }

    #[test]
    fn headings_keep_the_numbers_their_author_typed() {
        for typed in
            ["一、引言", "十二．結論", "（二）方法", "(3) Data", "第三章 討論", "1. Introduction", "2.3 Data", "1、引言", "IV. Results", "１０ 全形"]
        {
            let expect = !typed.starts_with('１');
            assert_eq!(typed_heading_number(typed), expect, "{typed}");
        }
        for plain in ["Introduction", "1990年代的變遷", "2020 elections", "一般方法", "Ideas", "123. Too long"] {
            assert!(!typed_heading_number(plain), "{plain}");
        }
        let mut doc = json!({"meta": {}, "blocks": [
            {"t": "Header", "c": [1, ["a", [], []], [s("一、引言")]]},
            {"t": "Header", "c": [1, ["b", [], []], words("Background")]},
        ]});
        let (_, changes) = restructure(&mut doc);
        assert_eq!(changes.typed_heading_numbers, 1);
        assert_eq!(doc["blocks"][0]["c"][1][1], json!(["unnumbered"]));
        assert_eq!(doc["blocks"][1]["c"][1][1], json!([]));
    }

    #[test]
    fn front_matter_comes_from_word_styles() {
        let mut doc = json!({"meta": {
            "title": {"t": "MetaInlines", "c": words("A Study of Things")},
            "author": {"t": "MetaList", "c": [{"t": "MetaInlines", "c": [s("Ana"), sp(), s("Lee"), {"t": "Superscript", "c": [s("1,*")]}]}, {"t": "MetaInlines", "c": [s("王小明"), {"t": "Superscript", "c": [s("2")]}]}]},
            "abstract": {"t": "MetaBlocks", "c": [para(words("We study things."))]}
        }, "blocks": [{"t": "Header", "c": [1, ["intro", [], []], words("Introduction")]}, para(words("Body."))]});
        let (front, _) = restructure(&mut doc);
        assert_eq!(front.title, "A Study of Things");
        assert_eq!(
            front.authors,
            vec![
                WordAuthor { name: "Ana Lee".into(), affiliations: vec![1], corresponding: true },
                WordAuthor { name: "王小明".into(), affiliations: vec![2], corresponding: false },
            ]
        );
        assert_eq!(front.abstract_text, "We study things.");
        assert!(doc["meta"].get("title").is_none(), "Pandoc must not print the title again");
        assert_eq!(doc["blocks"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn front_matter_typed_as_plain_paragraphs() {
        let mut doc = json!({"meta": {}, "blocks": [
            para(words("數位人文研究中的文本分析方法")),
            para(vec![s("王小明"), sup("1"), s("，李四"), sup("2,*")]),
            para(vec![sup("1"), s("國立臺灣大學歷史學系")]),
            para(vec![sup("2"), s("中央研究院")]),
            para(vec![s("摘要：本文探討文本分析。")]),
            para(vec![s("關鍵詞：數位人文；文本分析、方法")]),
            {"t": "Header", "c": [1, ["a", [], []], [s("引言")]]},
            para(vec![s("正文。")]),
        ]});
        let (front, changes) = restructure(&mut doc);
        assert_eq!(front.title, "數位人文研究中的文本分析方法");
        assert_eq!(front.authors.len(), 2, "{:?}", front.authors);
        assert_eq!(front.authors[0], WordAuthor { name: "王小明".into(), affiliations: vec![1], corresponding: false });
        assert_eq!(front.authors[1], WordAuthor { name: "李四".into(), affiliations: vec![2], corresponding: true });
        assert_eq!(front.affiliations, vec!["國立臺灣大學歷史學系", "中央研究院"]);
        assert_eq!(front.abstract_text, "本文探討文本分析。");
        assert_eq!(front.keywords, vec!["數位人文", "文本分析", "方法"]);
        assert_eq!(front.language, "zh-hant");
        assert_eq!(changes.front_matter_paragraphs, 6);
        assert_eq!(doc["blocks"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_manuscript_that_starts_with_prose_keeps_it() {
        let long = "word ".repeat(80);
        let mut doc = json!({"meta": {}, "blocks": [para(words(long.trim())), {"t": "Header", "c": [1, ["a", [], []], [s("Methods")]]}]});
        let (front, _) = restructure(&mut doc);
        assert!(front.title.is_empty());
        assert_eq!(doc["blocks"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn a_short_introduction_before_the_first_heading_stays_in_the_body() {
        let mut doc = json!({"meta": {}, "blocks": [
            para(words("This study asks why.")),
            para(words("It answers in three parts.")),
            {"t": "Header", "c": [1, ["m", [], []], [s("Methods")]]},
        ]});
        let (front, _) = restructure(&mut doc);
        assert!(front.title.is_empty() && front.authors.is_empty(), "{front:?}");
        assert_eq!(doc["blocks"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn an_equation_in_the_abstract_stays_math() {
        let math = json!({"t": "Math", "c": [{"t": "InlineMath"}, "\\alpha + x^2"]});
        let mut doc = json!({"meta": {"title": {"t": "MetaString", "c": "T"}}, "blocks": [
            para({ let mut p = words("Abstract: We fit"); p.extend([sp(), math, s(".")]); p }),
            {"t": "Header", "c": [1, ["i", [], []], [s("Introduction")]]},
        ]});
        let (front, _) = restructure(&mut doc);
        assert_eq!(front.abstract_text, "We fit \\(\\alpha + x^2\\).");
        let mut m = crate::PublicationMetadata::sample();
        m.abstract_text = front.abstract_text;
        assert!(m.to_latex().contains("We fit \\(\\alpha + x^2\\)."), "{}", m.to_latex());
    }

    #[test]
    fn an_abstract_section_with_a_heading() {
        let mut doc = json!({"meta": {"title": {"t": "MetaString", "c": "T"}}, "blocks": [
            {"t": "Header", "c": [1, ["abs", [], []], [s("Abstract")]]},
            para(words("First part.")),
            para(words("Second part.")),
            para(words("Keywords: alpha; beta")),
            {"t": "Header", "c": [1, ["i", [], []], [s("Introduction")]]},
        ]});
        let (front, _) = restructure(&mut doc);
        assert_eq!(front.abstract_text, "First part.\n\nSecond part.");
        assert_eq!(front.keywords, vec!["alpha", "beta"]);
        assert_eq!(doc["blocks"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn language_from_the_characters() {
        assert_eq!(language(&[para(words("這個研究關於數據"))]), "zh-hant");
        assert_eq!(language(&[para(words("这个研究关于数据"))]), "zh-hans");
        assert_eq!(language(&[para(words("This is English text with 一 character"))]), "en");
        assert_eq!(language(&[para(words("これは研究です"))]), "ja");
    }

    #[test]
    fn hostile_trees_are_bounded() {
        let mut deep = s("x");
        // serde_json refuses input nested deeper than 128 when parsing Pandoc's output, so this is the
        // deepest tree a file can produce; the walks stop well before it.
        for _ in 0..120 {
            deep = json!({"t": "Emph", "c": [deep]});
        }
        assert!(inline_text(&[deep.clone()]).len() <= 1);
        let mut doc = json!({"meta": {"title": {"t": "MetaInlines", "c": [deep]}}, "blocks": "not a list"});
        let _ = restructure(&mut doc);
        assert!(graphics_path("../../etc/passwd").is_none());
        assert!(graphics_path("media/a}b.png").is_none());
        assert_eq!(graphics_path("media/media/rId1.png").as_deref(), Some("media/media/rId1.png"));
    }
}
