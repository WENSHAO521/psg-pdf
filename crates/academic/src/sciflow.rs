//! Journal templates from SciFlow template configurations (`configuration.sfo.yml`).
//!
//! SciFlow (the MIT-licensed SciFlow Open Source publishing platform) describes a journal's PDF
//! declaratively: a YAML file of components, each a document with a `kind` (`Configuration`,
//! `Page`, `TableOfContents`, `JournalMeta`, …). SciFlow renders it with HTML and CSS; PSG
//! Academic PDF reads the parts that decide how the typeset page looks and writes a LaTeX template
//! on the PSG class from them, so a SciFlow template can be added like a journal's LaTeX
//! template (`template_import`) and three of SciFlow's generic templates ship built in.
//!
//! What is used: the page size and margins, single- or double-sided pages, the base font size,
//! a sans-serif or serif text face, widows and orphans, the citation style (a Zotero/CSL style id,
//! mapped to the BibLaTeX style closest to it), the table of contents and its depth and label,
//! the journal name and article type, and the theme colour. HTML, EPUB and reader components,
//! Handlebars headers and footers, custom CSS and logo images are left out, and each one left out
//! is reported.
//!
//! The file comes from outside: its size is bounded, nested brackets are limited before it is
//! parsed, every number is range-checked, and every text that reaches LaTeX is escaped.

use yaml_rust2::{Yaml, YamlLoader};

use crate::Error;
use crate::metadata::{PublicationMetadata, escape};
use crate::sciflow_page::{Headers, Style};

/// The configuration's file name in a SciFlow template folder.
pub const CONFIGURATION: &str = "configuration.sfo.yml";
/// The optional schema of the template's own data (its theme colour, say).
pub const DATA_SCHEMA: &str = "metaData.schema.yml";
/// The largest configuration read (bytes).
const MAX_TEXT: usize = 1024 * 1024;
/// The deepest bracket nesting accepted before parsing.
const MAX_NESTING: usize = 64;
/// The most components read.
const MAX_COMPONENTS: usize = 512;

/// A citation style, as BibLaTeX loads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationStyle {
    /// The SciFlow (CSL) id it came from.
    pub csl: String,
    /// The options BibLaTeX is loaded with, after `backend=biber`.
    pub biblatex: String,
    /// Whether the CSL style had a BibLaTeX style of its own; otherwise the closest generic one.
    pub exact: bool,
}

/// How the typeset page looks, as read from a SciFlow configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// The template's title (`Configuration.spec.title`).
    pub title: String,
    pub description: String,
    /// `journal`, `thesis`, `book`, …
    pub kind: String,
    /// Paper size in millimetres, width × height.
    pub paper_mm: (f64, f64),
    /// The paper size's LaTeX name when it has one (`a4paper`).
    pub paper_name: Option<&'static str>,
    pub inner_mm: f64,
    pub outer_mm: f64,
    pub top_mm: f64,
    pub bottom_mm: f64,
    pub two_sided: bool,
    /// 10, 11 or 12.
    pub font_pt: u8,
    pub sans: bool,
    /// Whether widows and orphans are prevented (`typesetting.widows`/`orphans` above 1).
    pub no_widows: bool,
    pub citation: CitationStyle,
    /// `Some(depth)` when there is a table of contents.
    pub toc_depth: Option<u8>,
    pub toc_label: String,
    /// Whether the body starts on a new page after the front matter (monographs).
    pub body_new_page: bool,
    pub journal: String,
    pub article_type: String,
    /// `RRGGBB`, from the template data's `themeColor`.
    pub accent: Option<String>,
    /// The running headers and footers, from the template's Handlebars file.
    pub headers: Option<Headers>,
    /// Heading and link styles, from the Typography component and the CSS.
    pub style: Style,
    /// The logo's file name in the template (`logo.png`), when it has one.
    pub logo_file: Option<String>,
}

/// A SciFlow configuration turned into PSG template files.
#[derive(Debug, Clone)]
pub struct Converted {
    pub layout: Layout,
    /// What was used, one sentence each.
    pub found: Vec<String>,
    /// What was left out or could not be used, one sentence each.
    pub warnings: Vec<String>,
    /// The logo to store beside the template as `layout.logo_file`.
    pub logo: Option<Vec<u8>>,
}

/// The largest logo taken (bytes).
const MAX_LOGO: usize = 8 * 1024 * 1024;

/// A template file named in the configuration (`./assets/x.hbs`, `tpl/<slug>/assets/logo.jpg`),
/// relative to the configuration's folder.
fn asset_path(path: &str) -> String {
    let p = path.trim().trim_start_matches("./");
    match p.strip_prefix("tpl/") {
        Some(rest) => rest.split_once('/').map_or(rest, |(_, r)| r).to_owned(),
        None => p.to_owned(),
    }
}

/// The logo's file extension from its first bytes: only PNG, JPEG and PDF, which XeLaTeX reads.
fn image_kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"%PDF-") {
        Some("pdf")
    } else {
        None
    }
}

fn bad(why: impl Into<String>) -> Error {
    Error::Template(format!("{CONFIGURATION}: {}", why.into()))
}

/// Refuse text a YAML parser might take too long or too deep over.
fn check_text(text: &str) -> Result<(), Error> {
    if text.len() > MAX_TEXT {
        return Err(bad(format!("larger than {} KB", MAX_TEXT / 1024)));
    }
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '[' | '{' => {
                depth = depth.saturating_add(1);
                if depth > MAX_NESTING {
                    return Err(bad(format!("brackets nested more than {MAX_NESTING} deep")));
                }
            }
            ']' | '}' => depth = depth.saturating_sub(1),
            '\n' => depth = 0,
            _ => {}
        }
    }
    Ok(())
}

fn load(text: &str) -> Result<Vec<Yaml>, Error> {
    check_text(text)?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let docs = YamlLoader::load_from_str(text).map_err(|e| bad(format!("not valid YAML ({e})")))?;
    if docs.len() > MAX_COMPONENTS {
        return Err(bad(format!("more than {MAX_COMPONENTS} components")));
    }
    Ok(docs)
}

fn text_of(y: &Yaml) -> Option<String> {
    match y {
        Yaml::String(s) | Yaml::Real(s) => Some(s.trim().to_owned()),
        Yaml::Integer(i) => Some(i.to_string()),
        _ => None,
    }
    .filter(|s| !s.is_empty())
}

fn number_of(y: &Yaml) -> Option<f64> {
    match y {
        Yaml::Integer(i) => Some(*i as f64),
        Yaml::Real(s) | Yaml::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|n| n.is_finite())
}

/// A CSS length in millimetres: `25mm`, `2.5cm`, `1in`, `72pt`.
pub fn length_mm(text: &str) -> Option<f64> {
    let t = text.trim().to_ascii_lowercase();
    let split = t.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')).unwrap_or(t.len());
    let (number, unit) = (t.get(..split)?, t.get(split..)?.trim());
    let n: f64 = number.parse().ok().filter(|n: &f64| n.is_finite())?;
    let mm = match unit {
        "mm" => n,
        "cm" => n * 10.0,
        "in" => n * 25.4,
        "pt" => n * 25.4 / 72.0,
        "pc" => n * 25.4 / 6.0,
        "px" => n * 25.4 / 96.0,
        "" if n == 0.0 => 0.0,
        _ => return None,
    };
    Some(mm)
}

/// A CSS `size`: a name (`A4`, `Letter`, with `landscape`) or two lengths.
fn paper(text: &str) -> Option<((f64, f64), Option<&'static str>)> {
    const NAMED: &[(&str, f64, f64, &str)] = &[
        ("a3", 297.0, 420.0, "a3paper"),
        ("a4", 210.0, 297.0, "a4paper"),
        ("a5", 148.0, 210.0, "a5paper"),
        ("b4", 250.0, 353.0, "b4paper"),
        ("b5", 176.0, 250.0, "b5paper"),
        ("letter", 215.9, 279.4, "letterpaper"),
        ("legal", 215.9, 355.6, "legalpaper"),
    ];
    let lower = text.trim().to_ascii_lowercase();
    let mut words: Vec<&str> = lower.split_whitespace().collect();
    let landscape = words.contains(&"landscape");
    words.retain(|w| *w != "landscape" && *w != "portrait");
    let (size, name) = match words.as_slice() {
        [one] => {
            let (_, w, h, name) = NAMED.iter().find(|(n, ..)| n == one)?;
            ((*w, *h), Some(*name))
        }
        [w, h] => ((length_mm(w)?, length_mm(h)?), None),
        _ => return None,
    };
    let in_range = |v: f64| (50.0..=1200.0).contains(&v);
    if !in_range(size.0) || !in_range(size.1) {
        return None;
    }
    Some(if landscape { ((size.1, size.0), None) } else { (size, name) })
}

/// The BibLaTeX style closest to a SciFlow (Zotero/CSL) citation style id.
pub fn citation_style(csl: &str) -> CitationStyle {
    // CSL ids are lower-case letters, digits and '-'; anything else is dropped, so the id can be
    // written into the main file's comments.
    let id: String = csl.trim().to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(120).collect();
    let exact = |biblatex: &str| CitationStyle { csl: id.clone(), biblatex: biblatex.to_owned(), exact: true };
    let generic = |biblatex: &str| CitationStyle { csl: id.clone(), biblatex: biblatex.to_owned(), exact: false };
    match id.as_str() {
        "apa" | "apa-7th-edition" | "apa-6th-edition" => exact("style=apa"),
        "ieee" | "ieee-with-url" => exact("style=ieee"),
        "nature" => exact("style=nature"),
        "american-chemical-society" | "american-chemical-society-with-titles" => exact("style=chem-acs"),
        "royal-society-of-chemistry" => exact("style=chem-rsc"),
        "american-physics-society" | "american-physical-society" => exact("style=phys"),
        "modern-language-association" | "modern-language-association-8th-edition" | "modern-language-association-7th-edition" => exact("style=mla"),
        "vancouver" | "vancouver-brackets" | "vancouver-superscript" => exact("style=vancouver"),
        "new-england-journal-of-medicine" => exact("style=nejm"),
        _ if id.starts_with("chicago-note") || id.starts_with("chicago-fullnote") => exact("style=chicago-notes"),
        _ if id.starts_with("chicago-author-date") => exact("style=chicago-authordate"),
        _ if id.contains("harvard") || id.contains("author-date") || id.contains("authoryear") => {
            generic("style=authoryear-comp,maxcitenames=2,maxbibnames=10,uniquelist=false,dashed=false")
        }
        _ => generic("style=numeric-comp,sorting=none,maxbibnames=10"),
    }
}

fn find<'a>(docs: &'a [Yaml], kind: &str) -> Option<&'a Yaml> {
    docs.iter().find(|d| d["kind"].as_str() == Some(kind))
}

/// The PDF variant of a component when there are several (`runners` naming `princexml`).
fn find_pdf<'a>(docs: &'a [Yaml], kind: &str) -> Option<&'a Yaml> {
    let pdf = |d: &&Yaml| d["runners"].as_vec().is_none_or(|r| r.iter().any(|x| matches!(x.as_str(), Some("princexml" | "pagedjs"))));
    docs.iter().filter(|d| d["kind"].as_str() == Some(kind)).find(pdf).or_else(|| find(docs, kind))
}

/// Components that only matter to SciFlow's HTML, EPUB and reader output.
const NOT_PDF: &[&str] =
    &["HtmlPage", "EpubPage", "Reader", "Static", "GoogleFont", "Environment", "ParagraphNumbering", "Variant", "Settings", "PageOffset"];
/// Components this converter reads.
const USED: &[&str] = &["Configuration", "Page", "TableOfContents", "JournalMeta", "ArticleMeta", "Typography", "JournalTitle", "MonographTitle"];

/// Read a SciFlow configuration. `read` gives the template's other files by their path relative
/// to the configuration's folder (the data schema, the Handlebars headers and footers, the CSS,
/// the logo), or `None`.
pub fn convert(configuration: &str, read: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Converted, Error> {
    let docs = load(configuration)?;
    let read_text = |path: &str| read(path).filter(|b| b.len() <= crate::sciflow_page::MAX_FILE).and_then(|b| String::from_utf8(b).ok());
    let data_schema = read_text(DATA_SCHEMA);
    let data_schema = data_schema.as_deref();
    let mut found = Vec::new();
    let mut warnings = Vec::new();
    let config = find(&docs, "Configuration")
        .map(|c| &c["spec"])
        .ok_or_else(|| bad("no `kind: Configuration` component; a SciFlow template configuration starts with one"))?;
    let title = text_of(&config["title"]).unwrap_or_else(|| "SciFlow template".to_owned());
    let description = text_of(&config["description"]).unwrap_or_default();
    let kind = text_of(&config["type"]).unwrap_or_else(|| "journal".to_owned());
    found.push(format!("SciFlow template \"{title}\" ({kind})."));

    // The page.
    let page = find_pdf(&docs, "Page").map(|p| &p["spec"]);
    let page_spec = page.map(|p| &p["page"]);
    let mut paper_mm = (210.0, 297.0);
    let mut paper_name = Some("a4paper");
    if let Some(size) = page_spec.and_then(|p| text_of(&p["size"])) {
        match paper(&size) {
            Some((mm, name)) => {
                paper_mm = mm;
                paper_name = name;
                found.push(format!("Page size {size} ({:.0} × {:.0} mm).", mm.0, mm.1));
            }
            None => warnings.push(format!("Page size \"{size}\" is not one PSG Academic PDF knows; A4 is used.")),
        }
    }
    let margin = |name: &str, default: f64, warnings: &mut Vec<String>| -> f64 {
        let Some(text) = page_spec.and_then(|p| text_of(&p["margins"][name])) else { return default };
        match length_mm(&text).filter(|mm| (0.0..=200.0).contains(mm)) {
            Some(mm) => mm,
            None => {
                warnings.push(format!("The {name} margin \"{text}\" can't be used; {default} mm is used."));
                default
            }
        }
    };
    let inner_mm = margin("inner", 22.0, &mut warnings);
    let outer_mm = margin("outer", 22.0, &mut warnings);
    let top_mm = margin("top", 25.0, &mut warnings);
    let bottom_mm = margin("bottom", 25.0, &mut warnings);
    if inner_mm + outer_mm >= paper_mm.0 - 20.0 || top_mm + bottom_mm >= paper_mm.1 - 20.0 {
        return Err(bad("the margins leave no room for text on the page"));
    }
    found.push(format!("Margins: inner {inner_mm} mm, outer {outer_mm} mm, top {top_mm} mm, bottom {bottom_mm} mm."));
    let handling = page_spec.and_then(|p| text_of(&p["handling"])).unwrap_or_default();
    let two_sided = handling == "double-sided";
    found.push(if two_sided { "Double-sided pages.".to_owned() } else { "Single-sided pages.".to_owned() });

    let mut font_pt = 10u8;
    if let Some(size) = page_spec.and_then(|p| text_of(&p["font"]["size"])) {
        match length_mm(&size).map(|mm| mm * 72.0 / 25.4) {
            Some(pt) if (6.0..=20.0).contains(&pt) => {
                font_pt = if pt < 10.5 {
                    10
                } else if pt < 11.5 {
                    11
                } else {
                    12
                };
                if (f64::from(font_pt) - pt).abs() > 0.25 {
                    warnings.push(format!("Font size {size}: LaTeX's article sizes are 10, 11 and 12 pt; {font_pt} pt is used."));
                } else {
                    found.push(format!("Text size {font_pt} pt."));
                }
            }
            _ => warnings.push(format!("Font size \"{size}\" can't be used; 10 pt is used.")),
        }
    }
    let family = page_spec.and_then(|p| text_of(&p["font"]["family"])).unwrap_or_default();
    let sans = {
        let lower = family.to_ascii_lowercase();
        lower.split(',').map(str::trim).any(|f| f == "sans-serif" || (f.contains("sans") && !f.contains("math")))
            && !lower.split(',').next().is_some_and(|f| f.contains("serif") && !f.contains("sans"))
    };
    if !family.is_empty() {
        found.push(format!("Text face: {} (from \"{family}\").", if sans { "sans-serif" } else { "serif" }));
    }
    let typesetting = page.map(|p| &p["typesetting"]);
    let no_widows =
        typesetting.is_some_and(|t| number_of(&t["widows"]).is_some_and(|n| n > 1.0) || number_of(&t["orphans"]).is_some_and(|n| n > 1.0));

    // Citations.
    let csl = text_of(&config["citationStyle"]["id"]).unwrap_or_else(|| "apa".to_owned());
    let citation = citation_style(&csl);
    if citation.exact {
        found.push(format!("Citation style {csl}: BibLaTeX {}.", citation.biblatex.split(',').next().unwrap_or_default()));
    } else {
        warnings.push(format!(
            "Citation style {csl} has no BibLaTeX style of its own; {} is used. Change \\psgbiblatexoptions in main.tex for another.",
            citation.biblatex.split(',').next().unwrap_or_default()
        ));
    }
    let alternatives: Vec<String> =
        config["citationStyleAlternative"].as_vec().map(|v| v.iter().filter_map(|a| text_of(&a["id"])).collect()).unwrap_or_default();
    if !alternatives.is_empty() {
        found.push(format!("Other citation styles the template offers (set in main.tex): {}.", alternatives.join(", ")));
    }

    // Table of contents.
    let toc = find_pdf(&docs, "TableOfContents");
    let toc_depth = toc.map(|t| number_of(&t["spec"]["depth"]).map_or(3, |d| d.clamp(1.0, 5.0) as u8));
    let toc_label = toc
        .and_then(|t| text_of(&t["translations"]["en-US"]["label"]).or_else(|| text_of(&t["spec"]["title"])))
        .unwrap_or_else(|| "Contents".to_owned());
    if let Some(depth) = toc_depth {
        found.push(format!("Table of contents \"{toc_label}\", {depth} levels."));
    }
    let body_new_page = toc.is_some() && (kind != "journal" || page.is_some_and(|p| p["page"]["pageBreaks"]["beforeChapter"].as_str().is_some()));

    // Journal details for the sample metadata.
    let journal = find(&docs, "JournalMeta").and_then(|j| text_of(&j["spec"]["name"])).unwrap_or_default();
    let article_type = find(&docs, "ArticleMeta").and_then(|a| text_of(&a["spec"]["articleType"])).unwrap_or_default();
    if !journal.is_empty() {
        found.push(format!("Journal: {journal}."));
    }

    // The theme colour, from the template's own data.
    let mut accent = None;
    if let Some(schema) = data_schema {
        match load(schema) {
            Ok(docs) => {
                let colour = docs.first().and_then(|d| text_of(&d["properties"]["themeColor"]["default"]));
                if let Some(colour) = colour {
                    let hex = colour.trim_start_matches('#');
                    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
                        found.push(format!("Theme colour #{}.", hex.to_ascii_uppercase()));
                        accent = Some(hex.to_ascii_uppercase());
                    } else {
                        warnings.push(format!("Theme colour \"{colour}\" is not a #RRGGBB value; the PSG accent colour is used."));
                    }
                }
            }
            Err(e) => warnings.push(format!("{DATA_SCHEMA} could not be read ({e}); the PSG accent colour is used.")),
        }
    }

    let (logo, logo_file, headers, style) = decorations(&docs, config, page, font_pt, read, &mut found, &mut warnings);

    let mut other: Vec<String> = docs
        .iter()
        .filter_map(|d| d["kind"].as_str())
        .filter(|k| !USED.contains(k) && !NOT_PDF.contains(k) && *k != "HeadersAndFooters")
        .map(str::to_owned)
        .collect();
    other.sort();
    other.dedup();
    if !other.is_empty() {
        warnings.push(format!("Not used: {} (components PSG Academic PDF does not convert).", other.join(", ")));
    }
    if let Some(language) = text_of(&config["language"]).filter(|l| !l.to_ascii_lowercase().starts_with("en")) {
        warnings.push(format!("The template's language is {language}; set the article language in the publication details."));
    }

    let layout = Layout {
        title,
        description,
        kind,
        paper_mm,
        paper_name,
        inner_mm,
        outer_mm,
        top_mm,
        bottom_mm,
        two_sided,
        font_pt,
        sans,
        no_widows,
        citation,
        toc_depth,
        toc_label,
        body_new_page,
        journal,
        article_type,
        accent,
        headers,
        style,
        logo_file,
    };
    Ok(Converted { layout, found, warnings, logo })
}

/// The logo, the headers and footers and the heading styles: what the logo, Handlebars and CSS
/// assets and the Typography component give.
#[allow(clippy::type_complexity)]
fn decorations(
    docs: &[Yaml],
    config: &Yaml,
    page: Option<&Yaml>,
    font_pt: u8,
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
    found: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> (Option<Vec<u8>>, Option<String>, Option<Headers>, Style) {
    let read_text = |path: &str| read(path).filter(|b| b.len() <= crate::sciflow_page::MAX_FILE).and_then(|b| String::from_utf8(b).ok());
    // The assets: Handlebars files by id, the logo, the CSS.
    let assets: Vec<&Yaml> = config["assets"].as_vec().map(|v| v.iter().collect()).unwrap_or_default();
    let asset_by_id = |id: &str| assets.iter().find(|a| a["id"].as_str() == Some(id)).and_then(|a| a["path"].as_str()).map(asset_path);

    let mut logo = None;
    let mut logo_file = None;
    if let Some(path) = assets.iter().find(|a| a["type"].as_str() == Some("logo")).and_then(|a| a["path"].as_str()).map(asset_path) {
        match read(&path).filter(|b| b.len() <= MAX_LOGO) {
            Some(bytes) => match image_kind(&bytes) {
                Some(ext) => {
                    found.push(format!("Logo: {path}."));
                    logo_file = Some(format!("logo.{ext}"));
                    logo = Some(bytes);
                }
                None => warnings.push(format!("The logo {path} is not a PNG, JPEG or PDF image; it is left out.")),
            },
            None => warnings.push(format!("The logo {path} is not in the template; it is left out.")),
        }
    }

    let mut headers = None;
    let hf = find_pdf(docs, "HeadersAndFooters");
    if let Some(hf) = hf {
        let file = text_of(&hf["spec"]["template"]["fileId"]).and_then(|id| asset_by_id(&id));
        match file.as_deref().and_then(read_text) {
            Some(hbs) => {
                let mut notes = Vec::new();
                headers = crate::sciflow_page::headers(&hbs, &mut notes);
                match &headers {
                    Some(_) => found.push(format!("Headers and footers: {}.", file.unwrap_or_default())),
                    None => {
                        warnings.push(format!("{}: no header or footer regions found; the PSG running headers are used.", file.unwrap_or_default()))
                    }
                }
                warnings.extend(notes);
            }
            None => warnings.push("The headers and footers file is not in the template; the PSG running headers are used.".to_owned()),
        }
    }

    // Heading styles: the Typography component, then the CSS files and custom styles.
    let base_pt = f64::from(font_pt);
    let mut typography = Vec::new();
    if let Some(t) = find(docs, "Typography") {
        for level in 1..=3usize {
            let h = &t["spec"][format!("h{level}").as_str()];
            let mut props = Vec::new();
            for key in ["fontSize", "fontWeight", "fontStyle", "color"] {
                if let Some(v) = text_of(&h[key]) {
                    props.push((key.to_owned(), v));
                }
            }
            if !props.is_empty() {
                typography.push((level, props));
            }
        }
    }
    let mut css = Vec::new();
    for a in &assets {
        let path = a["path"].as_str().map(asset_path).unwrap_or_default();
        if (path.ends_with(".css") || path.ends_with(".scss"))
            && let Some(text) = read_text(&path)
        {
            css.push(text);
        }
    }
    if let Some(custom) = page.and_then(|p| p["customStyles"].as_vec()) {
        css.extend(custom.iter().filter_map(|c| text_of(&c["css"])));
    }
    let mut notes = Vec::new();
    let style = crate::sciflow_page::style(&typography, &css, base_pt, &mut notes);
    for note in notes {
        if note.contains("left out") { warnings.push(note) } else { found.push(note) }
    }
    if !typography.is_empty() {
        found.push("Heading sizes and weights from the Typography component.".to_owned());
    }

    (logo, logo_file, headers, style)
}

fn mm(v: f64) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    format!("{rounded}mm")
}

impl Layout {
    /// The main file: the PSG class set up as the SciFlow configuration says.
    pub fn main_tex(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("% Main file of the \"{}\" template, converted by PSG Academic PDF from a SciFlow\n", one_line(&self.title)));
        out.push_str("% template configuration (configuration.sfo.yml). PSG Academic PDF regenerates psg-metadata.tex\n");
        out.push_str("% from the publication metadata and replaces body.tex with the imported manuscript.\n");
        out.push_str(&format!("% Citation style: {} (SciFlow/CSL id \"{}\").\n", self.citation.biblatex, one_line(&self.citation.csl)));
        out.push_str(&format!("\\def\\psgbiblatexoptions{{{}}}\n", self.citation.biblatex));
        let mut options = vec![format!("{}pt", self.font_pt)];
        if !self.two_sided {
            options.push("oneside".to_owned());
        }
        if self.citation.biblatex.contains("authoryear") {
            options.push("authoryear".to_owned());
        }
        out.push_str(&format!("\\documentclass[{}]{{psg-article}}\n", options.join(",")));
        let paper = match self.paper_name {
            Some(name) => name.to_owned(),
            None => format!("paperwidth={},paperheight={}", mm(self.paper_mm.0), mm(self.paper_mm.1)),
        };
        out.push_str(&format!(
            "\\geometry{{{paper},inner={},outer={},top={},bottom={}{}}}\n",
            mm(self.inner_mm),
            mm(self.outer_mm),
            mm(self.top_mm),
            mm(self.bottom_mm),
            if self.two_sided { "" } else { ",twoside=false" }
        ));
        if self.headers.as_ref().is_some_and(|h| h.uses_logo) && self.logo_file.is_some() {
            out.push_str("% Room for the logo in the header.\n\\geometry{headheight=10mm}\n");
        }
        out.push_str("% The running headers are as wide as the new text block.\n\\setlength\\headwidth{\\textwidth}\n");
        if self.sans {
            out.push_str("% A sans-serif text face; Chinese and Japanese follow with Noto Sans CJK.\n\\renewcommand\\familydefault{\\sfdefault}\n");
        }
        if self.no_widows {
            out.push_str("\\widowpenalty=10000\n\\clubpenalty=10000\n");
        }
        if let Some(accent) = &self.accent {
            out.push_str(&format!("\\definecolor{{psgaccent}}{{HTML}}{{{accent}}}\n"));
        }
        if let Some(depth) = self.toc_depth {
            out.push_str(&format!("\\setcounter{{tocdepth}}{{{depth}}}\n"));
            out.push_str("% The contents heading; a Chinese or Japanese article's language replaces it.\n");
            out.push_str(&format!("\\psgContentsName{{{}}}\n", escape(&self.toc_label)));
        }
        let logo = self.logo_file.as_deref().map(|f| format!("\\includegraphics[height=8mm]{{{f}}}")).unwrap_or_default();
        if let Some(f) = &self.logo_file
            && !self.headers.as_ref().is_some_and(|h| h.uses_logo)
        {
            out.push_str(&format!("% The journal's logo, above the title.\n\\psgLogo{{{f}}}\n"));
        }
        let style = self.style.latex();
        if self.headers.is_some() || !style.is_empty() {
            out.push_str("\\makeatletter\n");
            if let Some(h) = &self.headers {
                out.push_str("% Headers and footers from the SciFlow template's Handlebars file.\n");
                out.push_str(&format!("\\newcommand\\psgsciflowlogo{{{logo}}}\n"));
                out.push_str(&h.latex(self.two_sided));
            }
            if !style.is_empty() {
                out.push_str("% Headings and links from the SciFlow template's typography and CSS.\n");
                out.push_str(&style);
            }
            out.push_str("\\makeatother\n");
        }
        out.push_str("\\input{psg-metadata}\n\\addbibresource{references.bib}\n\n\\begin{document}\n\\psgmaketitle\n");
        if self.toc_depth.is_some() {
            out.push_str("\\tableofcontents\n");
            out.push_str(if self.body_new_page { "\\clearpage\n" } else { "\\bigskip\n" });
        }
        out.push_str("\\input{body}\n\n\\printbibliography\n\\end{document}\n");
        out
    }

    /// The sample metadata a new project starts with, with the template's journal name.
    pub fn sample_metadata(&self) -> PublicationMetadata {
        let mut m = PublicationMetadata::sample();
        if !self.journal.is_empty() {
            m.journal = self.journal.clone();
            m.journal_abbrev = String::new();
        }
        if !self.article_type.is_empty() {
            m.article_type = self.article_type.clone();
        }
        m.title = "A Sample Article Typeset with a Template Converted from SciFlow".to_owned();
        m
    }

    /// The template's files beside the PSG class and sample body and bibliography, which the
    /// caller adds: `main.tex` and `psg-metadata.tex`.
    pub fn files(&self) -> Vec<(&'static str, String)> {
        vec![("main.tex", self.main_tex()), (crate::template_import::METADATA_FILE, self.sample_metadata().to_latex())]
    }
}

impl Layout {
    /// One sentence on what the template looks like: "A4, single-sided, 11 pt sans-serif, apa citations."
    pub fn summary(&self) -> String {
        let paper = match self.paper_name {
            Some(name) => name.trim_end_matches("paper").to_ascii_uppercase(),
            None => format!("{} × {} mm", self.paper_mm.0.round(), self.paper_mm.1.round()),
        };
        let mut parts = vec![
            paper,
            (if self.two_sided { "double-sided" } else { "single-sided" }).to_owned(),
            format!("{} pt {}", self.font_pt, if self.sans { "sans-serif" } else { "serif" }),
            format!("{} citations", one_line(&self.citation.csl)),
        ];
        if self.toc_depth.is_some() {
            parts.push("table of contents".to_owned());
        }
        parts.join(", ")
    }
}

/// The manifest of a built-in template converted from one of SciFlow's generic templates.
pub fn builtin_manifest(id: &str, name: &str, source: &str, layout: &Layout) -> String {
    format!(
        "# PSG Academic PDF template manifest, written by the SciFlow converter (crate::sciflow) from\n\
         # templates/academic/sciflow-source/{source}. Don't edit it: change the converter and write it again.\n\
         id = \"{id}\"\n\
         name = \"{name}\"\n\
         description = \"Converted from SciFlow's {source} template: {summary}. XeLaTeX with Chinese and Japanese, BibLaTeX with Biber.\"\n\
         version = \"1.0.0\"\n\
         licence = \"MIT OR Apache-2.0; SciFlow configuration MIT (SciFlow GmbH)\"\n\
         engine = \"xelatex\"\n\
         bibliography = \"biber\"\n\
         main = \"main.tex\"\n\
         body = \"body.tex\"\n\
         metadata = \"psg-metadata.tex\"\n\
         files = [\"main.tex\", \"psg-article.cls\", \"body.tex\", \"references.bib\", \"psg-metadata.tex\"]\n",
        summary = layout.summary()
    )
}

fn one_line(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOURNAL: &str = "---\nkind: Configuration\nspec:\n  title: Journal Article\n  type: journal\n  citationStyle:\n    id: apa\n  citationStyleAlternative:\n    - id: ieee\n  assets:\n    - id: logo\n      type: logo\n      path: x.jpg\n---\nkind: Page\nrunners:\n  - princexml\nspec:\n  page:\n    font:\n      family: Noto Sans, sans-serif\n      size: 11pt\n    size: A4\n    margins:\n      inner: 25mm\n      outer: 18mm\n    handling: single-sided\n  typesetting:\n    widows: 2\n---\nkind: HtmlPage\nspec: {}\n---\nkind: JournalMeta\nspec:\n  name: Demo journal\n";

    #[test]
    fn a_journal_configuration_becomes_a_layout() {
        let c = convert(JOURNAL, &|p: &str| (p == DATA_SCHEMA).then(|| b"properties:\n  themeColor:\n    default: \"#1a2b3c\"\n".to_vec())).unwrap();
        let l = &c.layout;
        assert_eq!(l.title, "Journal Article");
        assert_eq!(l.paper_name, Some("a4paper"));
        assert_eq!((l.inner_mm, l.outer_mm, l.top_mm), (25.0, 18.0, 25.0));
        assert!(!l.two_sided && l.sans && l.no_widows);
        assert_eq!(l.font_pt, 11);
        assert_eq!(l.citation.biblatex, "style=apa");
        assert_eq!(l.journal, "Demo journal");
        assert_eq!(l.accent.as_deref(), Some("1A2B3C"));
        assert!(c.warnings.iter().any(|w| w.contains("logo")));
        let main = l.main_tex();
        assert!(main.contains("\\def\\psgbiblatexoptions{style=apa}"));
        assert!(main.contains("\\documentclass[11pt,oneside]{psg-article}"));
        assert!(main.contains("\\geometry{a4paper,inner=25mm,outer=18mm,top=25mm,bottom=25mm,twoside=false}"));
        assert!(main.contains("\\definecolor{psgaccent}{HTML}{1A2B3C}"));
        assert!(!main.contains("tableofcontents"));
    }

    #[test]
    fn custom_sizes_contents_and_double_sided_pages() {
        let text = "kind: Configuration\nspec:\n  title: Book\n  type: thesis\n  citationStyle:\n    id: elsevier-harvard\n---\nkind: Page\nspec:\n  page:\n    size: 17cm 24cm\n    pageBreaks:\n      beforeChapter: recto\n    margins: {inner: 25mm, outer: 20mm, top: 17mm, bottom: 20mm}\n    handling: double-sided\n---\nkind: TableOfContents\nspec:\n  depth: 9\ntranslations:\n  en-US:\n    label: Table of Contents\n";
        let c = convert(text, &|_: &str| None).unwrap();
        let l = &c.layout;
        assert_eq!(l.paper_mm, (170.0, 240.0));
        assert!(l.two_sided && l.body_new_page);
        assert_eq!(l.toc_depth, Some(5));
        assert!(!l.citation.exact && l.citation.biblatex.starts_with("style=authoryear-comp"));
        let main = l.main_tex();
        assert!(main.contains("paperwidth=170mm,paperheight=240mm"));
        assert!(main.contains("\\documentclass[10pt,authoryear]{psg-article}"));
        assert!(main.contains("\\psgContentsName{Table of Contents}"));
        assert!(main.contains("\\tableofcontents\n\\clearpage"));
    }

    #[test]
    fn hostile_values_are_refused_or_escaped() {
        // Not a configuration at all.
        assert!(convert("kind: Page\n", &|_: &str| None).is_err());
        assert!(convert(&"[".repeat(10_000), &|_: &str| None).is_err());
        assert!(convert(&"x".repeat(MAX_TEXT + 1), &|_: &str| None).is_err());
        // Margins that leave no room.
        let squeezed = "kind: Configuration\nspec: {}\n---\nkind: Page\nspec:\n  page:\n    margins: {inner: 150mm, outer: 150mm}\n";
        assert!(convert(squeezed, &|_: &str| None).is_err());
        // Nonsense values fall back, and a label can't run TeX commands.
        let odd = "kind: Configuration\nspec:\n  citationStyle: {id: '\\input{/etc/passwd}'}\n---\nkind: Page\nspec:\n  page:\n    size: huge\n    font: {size: 900pt}\n    margins: {inner: -5mm}\n---\nkind: TableOfContents\nspec: {title: '\\write18{rm}'}\n";
        let c = convert(odd, &|_: &str| None).unwrap();
        assert_eq!(c.layout.paper_name, Some("a4paper"));
        assert_eq!(c.layout.font_pt, 10);
        assert_eq!(c.layout.inner_mm, 22.0);
        assert_eq!(c.layout.citation.biblatex, "style=numeric-comp,sorting=none,maxbibnames=10");
        let main = c.layout.main_tex();
        assert!(!main.contains("\\write18"), "{main}");
        assert!(!main.contains("\\input{/etc"), "{main}");
        assert!(c.warnings.len() >= 3);
    }

    #[test]
    fn lengths_and_styles() {
        assert_eq!(length_mm("2.5cm"), Some(25.0));
        assert_eq!(length_mm("1in"), Some(25.4));
        assert_eq!(length_mm("0"), Some(0.0));
        assert_eq!(length_mm("12em"), None);
        assert_eq!(length_mm("nanmm"), None);
        assert_eq!(citation_style("ieee").biblatex, "style=ieee");
        assert_eq!(citation_style("chicago-note-bibliography-16th-edition").biblatex, "style=chicago-notes");
        assert!(!citation_style("american-medical-association").exact);
    }
}
