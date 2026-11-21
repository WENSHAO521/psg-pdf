//! The parts of a SciFlow template that decorate the page: its Handlebars headers and footers,
//! its heading typography and the CSS rules that can be carried over to LaTeX.
//!
//! SciFlow's headers and footers are an HTML template with six regions, each a `div` with one
//! of the classes `first-page-header`, `first-page-footer`, `recto-page-header`,
//! `recto-page-footer`, `verso-page-header` and `verso-page-footer`. A region's children are
//! laid out left to right (`justify-between`). Each child becomes the left, centre or right part
//! of a fancyhdr header or footer, with the Handlebars expressions SciFlow fills from the article
//! metadata mapped to the PSG class's metadata macros, `{{#if}}` blocks to tests on them, the
//! page counter to `\thepage` and the running section strings to `\leftmark`/`\rightmark`.
//!
//! Everything comes from outside. Text is escaped; only macros from a fixed list are written;
//! nesting and sizes are bounded; an expression that can't be mapped is dropped and reported.

use crate::metadata::escape;

/// The largest Handlebars or CSS file read (bytes).
pub const MAX_FILE: usize = 256 * 1024;
/// The deepest `{{#if}}` nesting followed.
const MAX_BLOCK_DEPTH: usize = 16;
/// The most children taken from one region.
const MAX_CHILDREN: usize = 8;

/// One header or footer line: its left, centre and right parts, as LaTeX.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Line {
    pub left: String,
    pub centre: String,
    pub right: String,
}

impl Line {
    fn is_empty(&self) -> bool {
        self.left.is_empty() && self.centre.is_empty() && self.right.is_empty()
    }
}

/// The six regions SciFlow's headers and footers may define; `None` when a region is absent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Headers {
    pub first_header: Option<Line>,
    pub first_footer: Option<Line>,
    pub recto_header: Option<Line>,
    pub recto_footer: Option<Line>,
    pub verso_header: Option<Line>,
    pub verso_footer: Option<Line>,
    /// Whether a region shows the logo.
    pub uses_logo: bool,
}

const REGIONS: [&str; 6] =
    ["first-page-header", "first-page-footer", "recto-page-header", "recto-page-footer", "verso-page-header", "verso-page-footer"];

/// Read a SciFlow headers-and-footers template. Returns `None` when it has none of the regions.
pub fn headers(hbs: &str, notes: &mut Vec<String>) -> Option<Headers> {
    let text = hbs.get(..MAX_FILE.min(hbs.len())).unwrap_or_default();
    let text = strip_comments(text);
    let (html, styles) = split_styles(&text);
    let mut dropped = Vec::new();
    let mut lines: Vec<Option<Line>> = Vec::new();
    for region in REGIONS {
        let line = region_html(&html, region).map(|inner| {
            let italic = styles
                .iter()
                .any(|s| s.contains(region) && s.contains("font-style: italic") || s.contains(region) && s.contains("font-style:italic"));
            let align = if styles.iter().any(|s| s.contains(region) && s.replace(' ', "").contains("text-align:right")) {
                2
            } else if styles.iter().any(|s| s.contains(region) && s.replace(' ', "").contains("text-align:center")) {
                1
            } else {
                0
            };
            let mut parts: Vec<String> = children(inner).into_iter().map(|c| render_child(&c, &mut dropped)).take(MAX_CHILDREN).collect();
            if italic {
                for p in parts.iter_mut().filter(|p| !p.is_empty()) {
                    *p = format!("\\itshape {p}");
                }
            }
            let mut line = Line::default();
            match parts.len() {
                0 => {}
                1 => {
                    let only = parts.remove(0);
                    match align {
                        2 => line.right = only,
                        1 => line.centre = only,
                        _ => line.left = only,
                    }
                }
                _ => {
                    line.right = parts.pop().unwrap_or_default();
                    line.left = parts.remove(0);
                    line.centre = parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join("\\quad ");
                }
            }
            line
        });
        lines.push(line);
    }
    let mut it = lines.into_iter();
    let mut next = || it.next().flatten();
    let mut h = Headers {
        first_header: next(),
        first_footer: next(),
        recto_header: next(),
        recto_footer: next(),
        verso_header: next(),
        verso_footer: next(),
        uses_logo: false,
    };
    let all = [&h.first_header, &h.first_footer, &h.recto_header, &h.recto_footer, &h.verso_header, &h.verso_footer];
    if all.iter().all(|l| l.is_none()) {
        return None;
    }
    h.uses_logo = all.iter().filter_map(|l| l.as_ref()).any(|l| [&l.left, &l.centre, &l.right].iter().any(|p| p.contains(LOGO)));
    dropped.sort();
    dropped.dedup();
    if !dropped.is_empty() {
        notes.push(format!("Headers and footers: left out {}, which PSG Academic PDF has no value for.", dropped.join(", ")));
    }
    Some(h)
}

/// The macro the logo is drawn with (defined in the main file).
pub const LOGO: &str = "\\psgsciflowlogo{}";

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let long = rest.find("{{!--");
        let short = rest.find("{{!");
        let Some(start) = long.or(short) else {
            out.push_str(rest);
            return out;
        };
        out.push_str(rest.get(..start).unwrap_or_default());
        let after = rest.get(start..).unwrap_or_default();
        let end = if after.starts_with("{{!--") { after.find("--}}").map(|e| e + 4) } else { after.find("}}").map(|e| e + 2) };
        match end {
            Some(e) => rest = after.get(e..).unwrap_or_default(),
            None => return out,
        }
    }
}

/// The HTML without its `<style>` blocks, and the blocks' CSS.
fn split_styles(text: &str) -> (String, Vec<String>) {
    let mut html = String::with_capacity(text.len());
    let mut styles = Vec::new();
    let mut rest = text;
    while let Some(start) = find_ci(rest, "<style") {
        html.push_str(rest.get(..start).unwrap_or_default());
        let after = rest.get(start..).unwrap_or_default();
        let Some(end) = find_ci(after, "</style>") else {
            return (html, styles);
        };
        let css = after.get(..end).unwrap_or_default();
        styles.extend(css_rules(css.split_once('>').map_or("", |(_, c)| c)).into_iter().map(|(sel, body)| format!("{sel}{{{body}}}")));
        rest = after.get(end + "</style>".len()..).unwrap_or_default();
    }
    html.push_str(rest);
    (html, styles)
}

/// Tags are matched as written in lower case, as SciFlow's templates write them.
fn find_ci(haystack: &str, needle: &str) -> Option<usize> {
    haystack.find(needle)
}

/// The inner HTML of the first element whose class list names `class`.
fn region_html<'a>(html: &'a str, class: &str) -> Option<&'a str> {
    let mut search = 0usize;
    for _ in 0..64 {
        let at = search + html.get(search..)?.find(class)?;
        search = at + class.len();
        // The `class` must be inside an opening tag's class attribute.
        let tag_start = html.get(..at)?.rfind('<')?;
        let tag = html.get(tag_start..)?;
        let tag_end = tag.find('>')?;
        let opening = tag.get(..tag_end)?;
        if !opening.contains("class=") || opening.starts_with("</") || html.get(..at)?.rfind('>').is_some_and(|gt| gt > tag_start) {
            continue;
        }
        let name: String = opening.get(1..)?.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        let inner_start = tag_start + tag_end + 1;
        let inner = html.get(inner_start..)?;
        let end = matching_close(inner, &name)?;
        return inner.get(..end);
    }
    None
}

/// Where the element opened just before `inner` closes: the index of its `</name>`.
fn matching_close(inner: &str, name: &str) -> Option<usize> {
    let lower = inner;
    let open = format!("<{name}");
    let close = format!("</{name}");
    let mut depth = 1usize;
    let mut i = 0usize;
    while i < lower.len() {
        let rest = lower.get(i..)?;
        let next_open = rest.find(&open);
        let next_close = rest.find(&close)?;
        match next_open {
            Some(o) if o < next_close => {
                depth = depth.saturating_add(1);
                if depth > 64 {
                    return None;
                }
                i += o + open.len();
            }
            _ => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + next_close);
                }
                i += next_close + close.len();
            }
        }
    }
    None
}

/// The top-level children of a region: its elements, and any text between them.
fn children(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = inner;
    while !rest.is_empty() && out.len() < MAX_CHILDREN * 2 {
        let Some(lt) = rest.find('<') else {
            push_text(&mut out, rest);
            break;
        };
        push_text(&mut out, rest.get(..lt).unwrap_or_default());
        let tag = rest.get(lt..).unwrap_or_default();
        let name: String = tag.get(1..).unwrap_or_default().chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        let Some(gt) = tag.find('>') else { break };
        if name.is_empty() || tag.get(..gt).is_some_and(|t| t.ends_with('/')) || matches!(name.to_ascii_lowercase().as_str(), "br" | "img" | "hr") {
            rest = tag.get(gt + 1..).unwrap_or_default();
            continue;
        }
        let body = tag.get(gt + 1..).unwrap_or_default();
        let Some(end) = matching_close(body, &name) else {
            out.push(tag.to_owned());
            break;
        };
        out.push(tag.get(..gt + 1 + end).unwrap_or_default().to_owned());
        let after = body.get(end..).unwrap_or_default();
        rest = after.find('>').and_then(|g| after.get(g + 1..)).unwrap_or_default();
    }
    // A Handlebars block (`{{#if}} <div>…</div> {{/if}}`) around elements is one child.
    let mut grouped: Vec<String> = Vec::new();
    let mut open = 0i64;
    for child in out {
        let opens = i64::try_from(child.matches("{{#").count()).unwrap_or(0);
        let closes = i64::try_from(child.matches("{{/").count()).unwrap_or(0);
        match grouped.last_mut() {
            Some(last) if open > 0 => last.push_str(&child),
            _ => grouped.push(child),
        }
        open = (open + opens - closes).max(0);
    }
    grouped
}

fn push_text(out: &mut Vec<String>, text: &str) {
    if !text.trim().is_empty() {
        out.push(text.to_owned());
    }
}

/// One child of a region as LaTeX.
fn render_child(html: &str, dropped: &mut Vec<String>) -> String {
    // Spans whose content SciFlow sets with CSS: the page counter and running heading strings.
    let mut text = String::new();
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        text.push_str(rest.get(..lt).unwrap_or_default());
        let tag = rest.get(lt..).unwrap_or_default();
        let Some(gt) = tag.find('>') else { break };
        let opening = tag.get(..gt).unwrap_or_default();
        if opening.contains("counter(page") {
            text.push_str("{{psg-page}}");
        } else if opening.contains("string(main-heading-text-title") {
            text.push_str("{{psg-main-heading}}");
        } else if opening.contains("string(sub-heading-text-title") {
            text.push_str("{{psg-sub-heading}}");
        } else if opening.starts_with("<br") || opening.starts_with("</div") || opening.starts_with("</p") {
            text.push(' ');
        }
        rest = tag.get(gt + 1..).unwrap_or_default();
    }
    if !rest.contains('>') {
        text.push_str(rest);
    }
    let tokens = tokenize(&text);
    let mut pos = 0usize;
    let latex = render_tokens(&tokens, &mut pos, 0, dropped);
    latex.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Debug)]
enum Token {
    Text(String),
    Expr(String),
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push(Token::Text(rest.get(..start).unwrap_or_default().to_owned()));
        let after = rest.get(start..).unwrap_or_default();
        let (open, close) = if after.starts_with("{{{") { (3, "}}}") } else { (2, "}}") };
        let Some(end) = after.find(close) else {
            rest = "";
            break;
        };
        let inner = after.get(open..end).unwrap_or_default();
        out.push(Token::Expr(inner.split_whitespace().collect::<Vec<_>>().join(" ").trim_matches('~').trim().to_owned()));
        rest = after.get(end + close.len()..).unwrap_or_default();
    }
    out.push(Token::Text(rest.to_owned()));
    out
}

/// Render tokens from `pos` until an `{{else}}`/`{{^}}`/`{{/if}}` at this level.
fn render_tokens(tokens: &[Token], pos: &mut usize, depth: usize, dropped: &mut Vec<String>) -> String {
    let mut out = String::new();
    while let Some(token) = tokens.get(*pos) {
        match token {
            Token::Text(t) => {
                // `escape` trims; the spaces around the text separate it from the fields.
                let text = decode_entities(t);
                let starts_with_punctuation = text.trim_start().starts_with([',', '.', ';', ':', ')', '!', '?']);
                if text.starts_with(char::is_whitespace) && !text.trim().is_empty() && !starts_with_punctuation {
                    out.push(' ');
                }
                out.push_str(&escape(&text));
                if text.ends_with(char::is_whitespace) {
                    out.push(' ');
                }
                *pos += 1;
            }
            Token::Expr(e) if e == "^" || e == "else" || e.starts_with("/") => return out,
            Token::Expr(e) if e.starts_with("#if ") || e.starts_with("#unless ") => {
                *pos += 1;
                let unless = e.starts_with("#unless");
                let cond = e.split_once(' ').map(|(_, c)| c).unwrap_or_default().trim_matches(|c| c == '(' || c == ')');
                if depth >= MAX_BLOCK_DEPTH {
                    return out;
                }
                let then = render_tokens(tokens, pos, depth + 1, dropped);
                let mut otherwise = String::new();
                if matches!(tokens.get(*pos), Some(Token::Expr(x)) if x == "^" || x == "else") {
                    *pos += 1;
                    otherwise = render_tokens(tokens, pos, depth + 1, dropped);
                    // SciFlow's templates put hints for its editor there ("you can set meta data in
                    // the article meta settings"); they don't belong in a typeset article.
                    if otherwise.to_ascii_lowercase().contains("settings") {
                        otherwise.clear();
                    }
                }
                if matches!(tokens.get(*pos), Some(Token::Expr(x)) if x.starts_with('/')) {
                    *pos += 1;
                }
                let (then, otherwise) = if unless { (otherwise, then) } else { (then, otherwise) };
                match field(cond) {
                    Some(Field::Macro(m)) => out.push_str(&format!("\\psg@ifempty{m}{{{otherwise}}}{{{then}}}")),
                    Some(Field::Literal(l)) if !l.is_empty() => out.push_str(&then),
                    _ => {
                        dropped.push(name_of(cond));
                        out.push_str(&otherwise);
                    }
                }
            }
            Token::Expr(e) => {
                *pos += 1;
                if e.starts_with('#') {
                    // Another block helper (each, with): its content is not used.
                    dropped.push(e.split_whitespace().next().unwrap_or_default().to_owned());
                    let _ = render_tokens(tokens, pos, depth + 1, &mut Vec::new());
                    if matches!(tokens.get(*pos), Some(Token::Expr(x)) if x.starts_with('/')) {
                        *pos += 1;
                    }
                    continue;
                }
                match field(e) {
                    Some(Field::Macro(m)) => out.push_str(&format!("{{{m}}}")),
                    Some(Field::Literal(l)) => out.push_str(&l),
                    None => dropped.push(name_of(e)),
                }
            }
        }
    }
    out
}

enum Field {
    Macro(String),
    Literal(String),
}

/// What a Handlebars expression shows, as a PSG macro.
fn field(expr: &str) -> Option<Field> {
    let e = expr.trim();
    match e {
        "psg-page" => return Some(Field::Literal("\\thepage".into())),
        "psg-main-heading" => return Some(Field::Literal("\\nouppercase{\\leftmark}".into())),
        "psg-sub-heading" => return Some(Field::Literal("\\nouppercase{\\rightmark}".into())),
        _ => {}
    }
    let mut words = e.split_whitespace();
    let helper = words.next().unwrap_or_default();
    let quoted: Option<String> = e.split(['\'', '"']).nth(1).map(str::to_owned);
    match helper {
        "renderAuthorNames" => Some(Field::Macro("\\psg@pdfauthor".into())),
        "image" => (quoted.as_deref() == Some("logo")).then(|| Field::Literal(LOGO.into())),
        "getYear" => Some(Field::Macro("\\psg@year".into())),
        "get" | "getMetaData" | "exists" => metadata_macro(&quoted?).map(|m| Field::Macro(m.into())),
        _ => None,
    }
}

/// The PSG metadata macro for a SciFlow metadata path (`ArticleMeta:volume`, `journal-meta.title`).
fn metadata_macro(path: &str) -> Option<&'static str> {
    let p: String = path.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == ':').collect();
    let last = p.rsplit(['.', ':']).next().unwrap_or_default();
    let journal = p.starts_with("journalmeta") || p.starts_with("journal-meta") || p.starts_with("journalmeta");
    Some(match last {
        "name" | "title" if journal => "\\psg@journal",
        "shortname" | "abbrevtitle" if journal => "\\psg@journalabbrev",
        "volume" => "\\psg@volume",
        "issue" => "\\psg@issue",
        "issn" | "eissn" | "pissn" => "\\psg@issn",
        "doi" => "\\psg@doi",
        "articletype" => "\\psg@articletype",
        "alttitle" | "shorttitle" | "runningtitle" => "\\psg@shorttitle",
        "title" => "\\psg@title",
        "epubdate" | "year" | "publicationyear" => "\\psg@year",
        "fpage" | "pages" | "pagerange" => "\\psg@pages",
        "copyrightstatement" | "copyright" => "\\psg@copyright",
        "licensep" | "license" | "licence" => "\\psg@licence",
        _ => return None,
    })
}

fn name_of(expr: &str) -> String {
    let e = expr.trim_matches(|c| c == '(' || c == ')');
    let quoted = e.split(['\'', '"']).nth(1);
    let name = quoted.unwrap_or_else(|| e.split_whitespace().next().unwrap_or_default());
    name.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '_')).take(60).collect()
}

fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&amp;", "&")
}

/// `selector { body }` pairs of flat CSS; nested (SCSS) blocks are skipped.
pub fn css_rules(css: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = css.get(..MAX_FILE.min(css.len())).unwrap_or_default();
    // Drop comments.
    let mut clean = String::new();
    while let Some(start) = rest.find("/*") {
        clean.push_str(rest.get(..start).unwrap_or_default());
        rest = rest.get(start..).and_then(|r| r.find("*/").and_then(|e| r.get(e + 2..))).unwrap_or_default();
    }
    clean.push_str(rest);
    let clean: String = clean.lines().map(|l| l.split("//").next().unwrap_or_default()).collect::<Vec<_>>().join("\n");
    let mut depth = 0usize;
    let mut selector_start = 0usize;
    let mut body_start = 0usize;
    let mut nested = false;
    for (i, c) in clean.char_indices() {
        match c {
            '{' => {
                if depth == 0 {
                    body_start = i + 1;
                    nested = false;
                } else {
                    nested = true;
                }
                depth = depth.saturating_add(1);
            }
            '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    if !nested {
                        let selector = clean.get(selector_start..body_start.saturating_sub(1)).unwrap_or_default().trim().to_owned();
                        let body = clean.get(body_start..i).unwrap_or_default().trim().to_owned();
                        out.push((selector, body));
                    } else {
                        out.push((
                            clean.get(selector_start..body_start.saturating_sub(1)).unwrap_or_default().trim().to_owned(),
                            String::from("\u{0}nested"),
                        ));
                    }
                    selector_start = i + 1;
                }
            }
            _ => {}
        }
        if out.len() > 512 {
            break;
        }
    }
    out
}

/// How section headings look.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Heading {
    /// Size in points.
    pub size_pt: Option<f64>,
    pub bold: Option<bool>,
    pub italic: bool,
    /// `RRGGBB`.
    pub colour: Option<String>,
}

impl Heading {
    fn is_default(&self) -> bool {
        *self == Heading::default()
    }
}

/// Headings (levels 1–3) and the link colour, from the Typography component and CSS rules.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Style {
    pub headings: [Heading; 3],
    pub link_colour: Option<String>,
}

/// A CSS size in points relative to `base_pt` (`1.5rem`, `18pt`, `120%`).
pub fn size_pt(text: &str, base_pt: f64) -> Option<f64> {
    let t = text.trim().to_ascii_lowercase();
    let pt = if let Some(n) = t.strip_suffix("rem").or_else(|| t.strip_suffix("em")) {
        n.trim().parse::<f64>().ok()? * base_pt
    } else if let Some(n) = t.strip_suffix('%') {
        n.trim().parse::<f64>().ok()? * base_pt / 100.0
    } else {
        crate::sciflow::length_mm(&t)? * 72.0 / 25.4
    };
    (pt.is_finite() && (5.0..=72.0).contains(&pt)).then_some(pt)
}

/// A CSS colour as `RRGGBB`: `#abc`, `#aabbcc`, `rgb(1, 2, 3)` or a few names.
pub fn colour(text: &str) -> Option<String> {
    let t = text.trim().trim_end_matches("!important").trim().to_ascii_lowercase();
    if let Some(hex) = t.strip_prefix('#') {
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        return match hex.len() {
            6 => Some(hex.to_ascii_uppercase()),
            3 => Some(hex.chars().flat_map(|c| [c, c]).collect::<String>().to_ascii_uppercase()),
            _ => None,
        };
    }
    if let Some(args) = t.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let v: Vec<u8> = args.split(',').filter_map(|p| p.trim().parse::<u8>().ok()).collect();
        return (v.len() == 3).then(|| v.iter().map(|b| format!("{b:02X}")).collect());
    }
    Some(
        match t.as_str() {
            "black" => "000000",
            "white" => "FFFFFF",
            "red" => "FF0000",
            "blue" => "0000FF",
            "navy" => "000080",
            "gray" | "grey" => "808080",
            "green" => "008000",
            "maroon" => "800000",
            "teal" => "008080",
            _ => return None,
        }
        .to_owned(),
    )
}

fn apply_declarations(h: &mut Heading, body: &str, base_pt: f64) -> bool {
    let mut used = false;
    for decl in body.split(';') {
        let Some((prop, value)) = decl.split_once(':') else { continue };
        let value = value.trim();
        match prop.trim().to_ascii_lowercase().as_str() {
            "font-size" | "fontsize" => {
                if let Some(pt) = size_pt(value, base_pt) {
                    h.size_pt = Some(pt);
                    used = true;
                }
            }
            "font-weight" | "fontweight" => {
                let v = value.to_ascii_lowercase();
                let bold = v == "bold" || v == "bolder" || v.parse::<u32>().is_ok_and(|n| n >= 600);
                h.bold = Some(bold);
                used = true;
            }
            "font-style" | "fontstyle" => {
                h.italic = value.eq_ignore_ascii_case("italic");
                used = true;
            }
            "color" => {
                if let Some(c) = colour(value) {
                    h.colour = Some(c);
                    used = true;
                }
            }
            _ => {}
        }
    }
    used
}

/// Read the Typography component's `h1`–`h3` (a YAML map rendered as `key: value` pairs by the
/// caller) and the CSS files' rules.
pub fn style(typography: &[(usize, Vec<(String, String)>)], css: &[String], base_pt: f64, notes: &mut Vec<String>) -> Style {
    let mut s = Style::default();
    for (level, props) in typography {
        if let Some(h) = s.headings.get_mut(level.saturating_sub(1)) {
            let body: String = props.iter().map(|(k, v)| format!("{k}:{v};")).collect();
            apply_declarations(h, &body, base_pt);
        }
    }
    let mut unused = 0usize;
    let mut used = 0usize;
    for file in css {
        for (selector, body) in css_rules(file) {
            let mut any = false;
            if body != "\u{0}nested" {
                for sel in selector.split(',').map(str::trim) {
                    let level = match sel {
                        "h1" | "section h1" | ".h1" => Some(0),
                        "h2" | "section h2" | ".h2" => Some(1),
                        "h3" | "section h3" | ".h3" => Some(2),
                        _ => None,
                    };
                    if let Some(h) = level.and_then(|l| s.headings.get_mut(l)) {
                        any |= apply_declarations(h, &body, base_pt);
                    } else if (sel == "a" || sel == "a:link")
                        && let Some(c) =
                            body.split(';').filter_map(|d| d.split_once(':')).find(|(p, _)| p.trim() == "color").and_then(|(_, v)| colour(v))
                    {
                        s.link_colour = Some(c);
                        any = true;
                    }
                }
            }
            if any {
                used += 1;
            } else {
                unused += 1;
            }
        }
    }
    if used > 0 {
        notes.push(format!("CSS: {used} rule(s) for headings and links used."));
    }
    if unused > 0 {
        notes.push(format!(
            "CSS: {unused} rule(s) left out; only the colour, size, weight and style of h1–h3 and the colour of links can be carried over to LaTeX."
        ));
    }
    s
}

impl Style {
    /// LaTeX that restyles the section headings and links; empty when nothing changes.
    pub fn latex(&self) -> String {
        let mut out = String::new();
        let commands = [
            ("section", 1, "-3ex \\@plus -1ex \\@minus -.2ex", "1.5ex \\@plus .2ex", "\\large", true),
            ("subsection", 2, "-2.5ex \\@plus -1ex \\@minus -.2ex", "1ex \\@plus .2ex", "\\normalsize", true),
            ("subsubsection", 3, "-2ex \\@plus -1ex \\@minus -.2ex", "0.8ex \\@plus .2ex", "\\normalsize", false),
        ];
        for (h, (name, level, before, after, default_size, accent)) in self.headings.iter().zip(commands) {
            if h.is_default() {
                continue;
            }
            let size = h.size_pt.map_or_else(|| default_size.to_owned(), |pt| format!("\\fontsize{{{:.1}pt}}{{{:.1}pt}}\\selectfont", pt, pt * 1.2));
            let weight = if h.bold.unwrap_or(true) { "\\bfseries" } else { "\\mdseries" };
            let shape = if h.italic { "\\itshape" } else { "" };
            let colour = match &h.colour {
                Some(c) => format!("\\color[HTML]{{{c}}}"),
                None if accent => "\\color{psgaccent}".to_owned(),
                None => String::new(),
            };
            out.push_str(&format!(
                "\\renewcommand\\{name}{{\\@startsection{{{name}}}{{{level}}}{{\\z@}}{{{before}}}{{{after}}}{{\\normalfont{size}{weight}{shape}{colour}}}}}\n"
            ));
        }
        if let Some(c) = &self.link_colour {
            out.push_str(&format!(
                "\\AtBeginDocument{{\\hypersetup{{colorlinks=true,linkcolor=[HTML]{{{c}}},urlcolor=[HTML]{{{c}}},citecolor=[HTML]{{{c}}}}}}}\n"
            ));
        }
        out
    }
}

impl Headers {
    /// LaTeX (inside `\makeatletter`) that sets fancyhdr's headers and footers.
    pub fn latex(&self, two_sided: bool) -> String {
        let mut out = String::from("\\fancyhf{}\n\\renewcommand{\\headrulewidth}{0pt}\n");
        let set = |out: &mut String, cmd: &str, sides: &str, line: &Line| {
            for (pos, text) in [("L", &line.left), ("C", &line.centre), ("R", &line.right)] {
                if !text.is_empty() {
                    let spec: String = sides.split(',').map(|s| format!("{pos}{s}")).collect::<Vec<_>>().join(",");
                    let spec = if sides.is_empty() { pos.to_owned() } else { spec };
                    out.push_str(&format!("\\{cmd}[{spec}]{{\\small {text}}}\n"));
                }
            }
        };
        let recto_h = self.recto_header.as_ref().or(self.verso_header.as_ref());
        let recto_f = self.recto_footer.as_ref().or(self.verso_footer.as_ref());
        if two_sided {
            let verso_h = self.verso_header.as_ref().or(recto_h);
            let verso_f = self.verso_footer.as_ref().or(recto_f);
            for (cmd, line, sides) in
                [("fancyhead", recto_h, "O"), ("fancyfoot", recto_f, "O"), ("fancyhead", verso_h, "E"), ("fancyfoot", verso_f, "E")]
            {
                if let Some(line) = line {
                    set(&mut out, cmd, sides, line);
                }
            }
        } else {
            for (cmd, line) in [("fancyhead", recto_h), ("fancyfoot", recto_f)] {
                if let Some(line) = line {
                    set(&mut out, cmd, "", line);
                }
            }
        }
        if self.first_header.is_some() || self.first_footer.is_some() {
            let mut first = String::new();
            for (cmd, line) in [("fancyhead", &self.first_header), ("fancyfoot", &self.first_footer)] {
                if let Some(line) = line.as_ref().filter(|l| !l.is_empty()) {
                    set(&mut first, cmd, "", line);
                }
            }
            out.push_str(&format!("\\fancypagestyle{{psgfirst}}{{\\fancyhf{{}}\\renewcommand{{\\headrulewidth}}{{0pt}}%\n{first}}}\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HBS: &str = r#"<div class="HeadersAndFooters {{getUniqueKey}}">
<style>.{{getPage}} .recto-page-header { text-align: right; font-style: italic; }</style>
{{!-- The first page header --}}
<div class="first-page-header flex justify-between col-12 pt2">
  <div>{{{image 'logo' (get 'JournalMeta:name') 0.2}}}</div>
  {{#if (exists 'ArticleMeta:articleType') }}<div><span class="article-type">{{ get 'ArticleMeta:articleType' }}</span></div>{{/if}}
</div>
<div class="first-page-footer flex">
  <div>{{#if (exists 'ArticleMeta:volume')}}Volume {{get 'ArticleMeta:volume'}}{{^}} - set it in the settings{{/if}}{{#if (exists 'ArticleMeta:issue')}}{{^}}none{{/if}} &amp; more</div>
  <div>{{{image 'license-image' ''}}}</div>
</div>
<div class="recto-page-header"><div><span>{{get 'ArticleMeta:altTitle'}}</span></div></div>
<div class="recto-page-footer flex"><div>{{get 'JournalMeta:publisher.publisherName'}}</div><span class="page-number" style="content: counter(page, {{get 'pageCounter.style'}});"></span></div>
<div class="verso-page-header"><span style="content: string(main-heading-text-title, first-except)"></span></div>
</div>"#;

    #[test]
    fn handlebars_regions_become_fancyhdr_lines() {
        let mut notes = Vec::new();
        let h = headers(HBS, &mut notes).unwrap();
        assert!(h.uses_logo);
        let first = h.first_header.clone().unwrap();
        assert_eq!(first.left, LOGO);
        assert!(first.right.starts_with("\\psg@ifempty\\psg@articletype{}"), "{}", first.right);
        let footer = h.first_footer.clone().unwrap();
        assert_eq!(footer.left, "\\psg@ifempty\\psg@volume{}{Volume {\\psg@volume}}\\psg@ifempty\\psg@issue{none}{} \\& more");
        let recto = h.recto_header.clone().unwrap();
        assert_eq!(recto.right, "\\itshape {\\psg@shorttitle}");
        assert_eq!(h.recto_footer.clone().unwrap().right, "\\thepage");
        assert_eq!(h.verso_header.clone().unwrap().left, "\\nouppercase{\\leftmark}");
        assert!(notes.iter().any(|n| n.contains("publisherName")), "{notes:?}");
        let latex = h.latex(true);
        assert!(latex.contains("\\fancyhead[RO]{\\small \\itshape {\\psg@shorttitle}}"), "{latex}");
        assert!(latex.contains("\\fancyfoot[RO]{\\small \\thepage}"), "{latex}");
        assert!(latex.contains("\\fancypagestyle{psgfirst}"), "{latex}");
    }

    #[test]
    fn hostile_templates_stay_text() {
        let mut notes = Vec::new();
        let evil = "<div class=\"recto-page-header\"><div>\\input{/etc/passwd} {{get '\\write18{x}'}} {{#if a}}{{#if b}}{{#if c}}x{{/if}}{{/if}}</div></div>";
        let h = headers(evil, &mut notes).unwrap();
        let line = h.recto_header.unwrap();
        assert!(!line.left.contains("\\input{"), "{}", line.left);
        assert!(!line.left.contains("\\write18"), "{}", line.left);
        assert!(headers("<div class=\"other\">x</div>", &mut notes).is_none());
        // Deeply nested or unclosed regions are given up on, quickly.
        let started = std::time::Instant::now();
        let _ = headers(&"<div class=\"recto-page-header\">".repeat(5000), &mut notes);
        assert!(started.elapsed().as_secs() < 5);
        assert!(headers("{{!-- unterminated", &mut notes).is_none());
    }

    #[test]
    fn css_headings_and_links() {
        let mut notes = Vec::new();
        let css = vec!["/* c */ h1 { color: #336699; font-size: 2rem; }\n.Title { .meta-data { p { margin: 0 } } }\na { color: rgb(0, 0, 255) }\nh2, h3 { font-weight: normal; font-style: italic }".to_owned()];
        let s = style(&[(1, vec![("fontSize".into(), "1.5rem".into())])], &css, 10.0, &mut notes);
        assert_eq!(s.headings[0].size_pt, Some(20.0));
        assert_eq!(s.headings[0].colour.as_deref(), Some("336699"));
        assert_eq!(s.headings[1].bold, Some(false));
        assert!(s.headings[2].italic);
        assert_eq!(s.link_colour.as_deref(), Some("0000FF"));
        assert!(notes.iter().any(|n| n.contains("1 rule(s) left out")), "{notes:?}");
        let latex = s.latex();
        assert!(latex.contains("\\renewcommand\\section{\\@startsection{section}{1}"), "{latex}");
        assert!(latex.contains("\\fontsize{20.0pt}{24.0pt}\\selectfont\\bfseries\\color[HTML]{336699}"), "{latex}");
        assert!(latex.contains("linkcolor=[HTML]{0000FF}"), "{latex}");
        assert_eq!(colour("#abc").as_deref(), Some("AABBCC"));
        assert_eq!(colour("url(x)"), None);
        assert_eq!(size_pt("1e400rem", 10.0), None);
    }
}
