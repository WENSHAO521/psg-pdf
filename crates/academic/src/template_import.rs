//! Adding a journal's own LaTeX template: a `.zip` or a folder as the journal hands it out.
//!
//! PSG Academic PDF finds the main file (the `.tex` with `\documentclass` and
//! `\begin{document}`), the engine it needs and its bibliography tool, and turns it into a
//! template: the text between the title block and the bibliography becomes the body file a
//! manuscript replaces, and the title, authors, abstract and keywords the main file typed are
//! replaced by the publication metadata. Everything found, and everything that could not be, is
//! reported, so a person can check the template before using it.
//!
//! The archive or folder comes from outside: every name is checked, sizes and counts are
//! bounded, links are not followed, and nothing is run.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::Error;
use crate::metadata::PublicationMetadata;
use crate::paths::{safe_relative_path, write_atomic};
use crate::template::{MANIFEST, Manifest, Template};

/// The most files taken from a template.
const MAX_FILES: usize = 256;
/// The largest file taken (bytes).
const MAX_FILE: u64 = 8 * 1024 * 1024;
/// The most bytes taken in all.
const MAX_TOTAL: u64 = 64 * 1024 * 1024;
/// The most entries looked at in an archive or folder.
const MAX_ENTRIES: usize = 4096;
/// The deepest folder walked.
const MAX_DEPTH: usize = 6;
/// The longest command argument looked at (bytes).
const MAX_GROUP: usize = 200_000;

/// The metadata file an imported template reads.
pub const METADATA_FILE: &str = "psg-metadata.tex";

/// Files a LaTeX run leaves behind; they are not part of a template.
const GENERATED: &[&str] =
    &["aux", "log", "out", "toc", "lof", "lot", "bbl", "blg", "bcf", "fls", "fdb_latexmk", "synctex", "gz", "xdv", "nav", "snm", "run.xml"];

/// A template that was added, and what was found in it.
#[derive(Debug, Clone)]
pub struct Imported {
    pub template: Template,
    /// The folder it was stored in.
    pub dir: PathBuf,
    /// What was found, one sentence each.
    pub found: Vec<String>,
    /// What could not be found or was left out, one sentence each.
    pub warnings: Vec<String>,
}

/// Add the template in `source` (a `.zip` file or a folder) to `store` (a folder of template
/// folders), named `name` or after the source. A template with the same id is replaced.
pub fn import(source: &Path, store: &Path, name: Option<&str>) -> Result<Imported, Error> {
    let mut warnings = Vec::new();
    let files = if source.is_dir() { read_folder(source, &mut warnings)? } else { read_zip(source, &mut warnings)? };
    let files = strip_common_folder(files);
    let main = find_main(&files).ok_or_else(|| {
        Error::Template(format!(
            "{}: no .tex file with \\documentclass and \\begin{{document}} was found; a template needs its main file",
            source.display()
        ))
    })?;
    // The main file's folder is the template's folder; files outside it are left out.
    let (main_dir, main_name) = match main.rsplit_once('/') {
        Some((dir, name)) => (format!("{dir}/"), name.to_owned()),
        None => (String::new(), main.clone()),
    };
    let mut kept: Vec<(String, Vec<u8>)> = Vec::new();
    for (path, bytes) in files {
        match path.strip_prefix(&main_dir) {
            Some(rest) if !rest.is_empty() => kept.push((rest.to_owned(), bytes)),
            _ => warnings.push(format!("{path}: outside the main file's folder; left out.")),
        }
    }
    let text = kept
        .iter()
        .find(|(p, _)| *p == main_name)
        .and_then(|(_, b)| String::from_utf8(b.clone()).ok())
        .ok_or_else(|| Error::Template(format!("{main}: the main file is not UTF-8 text; save it as UTF-8 and try again")))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned();

    let display_name = name.map(str::trim).filter(|n| !n.is_empty()).map(str::to_owned).unwrap_or_else(|| default_name(source));
    let id = template_id(&display_name);
    let names: Vec<&str> = kept.iter().map(|(p, _)| p.as_str()).collect();
    let body_name = ["body.tex", "psg-body.tex", "psg-article-body.tex"].into_iter().find(|n| !names.contains(n)).unwrap_or("psg-body.tex");
    let analysis = analyse(&text, body_name.trim_end_matches(".tex"))?;
    let mut found = analysis.found.clone();
    warnings.extend(analysis.warnings.iter().cloned());
    found.insert(0, format!("Main file: {main_name}."));

    // Write it to a fresh folder first, so a failure leaves the store as it was.
    std::fs::create_dir_all(store).map_err(|e| Error::io(store, e))?;
    let staging = store.join(format!(".adding-{id}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    let result = (|| {
        let mut listed = Vec::new();
        for (path, bytes) in &kept {
            let target = staging.join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
            }
            let bytes = if *path == main_name { analysis.main.as_bytes() } else { bytes.as_slice() };
            write_atomic(&target, bytes)?;
            listed.push(path.clone());
        }
        std::fs::create_dir_all(&staging).map_err(|e| Error::io(&staging, e))?;
        write_atomic(&staging.join(body_name), analysis.body.as_bytes())?;
        write_atomic(&staging.join(METADATA_FILE), PublicationMetadata::sample().to_latex().as_bytes())?;
        for extra in [body_name, METADATA_FILE] {
            if !listed.iter().any(|l| l == extra) {
                listed.push(extra.to_owned());
            }
        }
        listed.retain(|l| l != MANIFEST);
        let manifest = Manifest {
            id: id.clone(),
            name: display_name.clone(),
            description: format!("Added from {}.", source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()),
            version: String::new(),
            licence: String::new(),
            engine: analysis.engine.name().to_owned(),
            bibliography: analysis.bibliography.to_owned(),
            main: main_name.clone(),
            body: body_name.to_owned(),
            metadata: METADATA_FILE.to_owned(),
            files: listed,
        };
        let text = toml::to_string_pretty(&manifest).map_err(|e| Error::Template(format!("could not write {MANIFEST}: {e}")))?;
        write_atomic(&staging.join(MANIFEST), format!("# Added by PSG Academic PDF; edit it to change what the template uses.\n{text}").as_bytes())?;
        Template::from_dir(&staging)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    let dir = store.join(&id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
    }
    std::fs::rename(&staging, &dir).map_err(|e| Error::io(&dir, e))?;
    let template = Template::from_dir(&dir)?;
    Ok(Imported { template, dir, found, warnings })
}

/// A template name from the file or folder name: `jfs-template.zip` → `jfs-template`.
fn default_name(source: &Path) -> String {
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    if stem.trim().is_empty() { "Imported template".to_owned() } else { stem }
}

/// An id from `name`: ASCII letters and digits with `-`; a name without any (Chinese) gets one
/// from its characters' code points, so the same name always gets the same id.
pub fn template_id(name: &str) -> String {
    let mut id = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            id.push(c.to_ascii_lowercase());
        } else if !id.ends_with('-') && !id.is_empty() {
            id.push('-');
        }
        if id.len() >= 48 {
            break;
        }
    }
    let id = id.trim_matches('-').to_owned();
    if id.len() >= 3 {
        return id;
    }
    // FNV-1a of the name: stable and short.
    let mut hash: u32 = 0x811c_9dc5;
    for b in name.bytes() {
        hash ^= u32::from(b);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    format!("template-{hash:08x}")
}

// ---------------------------------------------------------------------------------------------
// Reading the files.

fn generated(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    name.starts_with('.')
        || lower.starts_with("__macosx/")
        || lower.contains("/__macosx/")
        || GENERATED.iter().any(|ext| name.ends_with(&format!(".{ext}")))
}

/// Keep `path` and `bytes` if the name is safe and the limits allow; say why not otherwise.
fn take(path: String, bytes: Vec<u8>, out: &mut Vec<(String, Vec<u8>)>, total: &mut u64, warnings: &mut Vec<String>) -> Result<(), Error> {
    if generated(&path) {
        return Ok(());
    }
    if safe_relative_path(&path).is_none() {
        warnings.push(format!(
            "{path}: the name has spaces or characters other than ASCII letters, digits, '-', '_' and '.', which TeX can't be relied on to read; left out. Rename it (and where the template uses it) and add the template again."
        ));
        return Ok(());
    }
    if out.len() >= MAX_FILES {
        return Err(Error::Template(format!("the template has more than {MAX_FILES} files")));
    }
    *total = total.saturating_add(bytes.len() as u64);
    if *total > MAX_TOTAL {
        return Err(Error::Template(format!("the template is larger than {} MB", MAX_TOTAL / 1024 / 1024)));
    }
    out.push((path, bytes));
    Ok(())
}

fn read_zip(source: &Path, warnings: &mut Vec<String>) -> Result<Vec<(String, Vec<u8>)>, Error> {
    let file = std::fs::File::open(source).map_err(|e| Error::io(source, e))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| Error::Template(format!("{}: not a .zip file or a folder ({e})", source.display())))?;
    if archive.len() > MAX_ENTRIES {
        return Err(Error::Template(format!("{}: more than {MAX_ENTRIES} entries", source.display())));
    }
    let mut out = Vec::new();
    let mut total = 0u64;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|e| Error::Template(format!("{}: {e}", source.display())))?;
        if entry.is_dir() {
            continue;
        }
        let is_link = entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000);
        let Some(path) = entry.enclosed_name().filter(|_| !is_link) else {
            warnings.push(format!("{}: an unsafe path or link in the archive; left out.", entry.name().unwrap_or_default()));
            continue;
        };
        let path = path.to_string_lossy().replace('\\', "/");
        if generated(&path) {
            continue;
        }
        if entry.size() > MAX_FILE {
            warnings.push(format!("{path}: larger than {} MB; left out.", MAX_FILE / 1024 / 1024));
            continue;
        }
        let mut bytes = Vec::new();
        (&mut entry).take(MAX_FILE.saturating_add(1)).read_to_end(&mut bytes).map_err(|e| Error::Template(format!("{path}: {e}")))?;
        if bytes.len() as u64 > MAX_FILE {
            warnings.push(format!("{path}: larger than {} MB; left out.", MAX_FILE / 1024 / 1024));
            continue;
        }
        take(path, bytes, &mut out, &mut total, warnings)?;
    }
    Ok(out)
}

fn read_folder(source: &Path, warnings: &mut Vec<String>) -> Result<Vec<(String, Vec<u8>)>, Error> {
    let mut out = Vec::new();
    let mut total = 0u64;
    let mut seen = 0usize;
    let mut stack = vec![(source.to_path_buf(), String::new(), 0usize)];
    while let Some((dir, prefix, depth)) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir).map_err(|e| Error::io(&dir, e))?.flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            seen = seen.saturating_add(1);
            if seen > MAX_ENTRIES {
                return Err(Error::Template(format!("{}: more than {MAX_ENTRIES} files and folders", source.display())));
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = format!("{prefix}{name}");
            // Links are not followed: a template is what is in its folder.
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_symlink() {
                warnings.push(format!("{path}: a link; left out."));
            } else if kind.is_dir() {
                if depth < MAX_DEPTH && !generated(&format!("{path}/x")) {
                    stack.push((entry.path(), format!("{path}/"), depth + 1));
                }
            } else if kind.is_file() {
                if generated(&path) {
                    continue;
                }
                let len = entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX);
                if len > MAX_FILE {
                    warnings.push(format!("{path}: larger than {} MB; left out.", MAX_FILE / 1024 / 1024));
                    continue;
                }
                let bytes = std::fs::read(entry.path()).map_err(|e| Error::io(&entry.path(), e))?;
                take(path, bytes, &mut out, &mut total, warnings)?;
            }
        }
    }
    Ok(out)
}

/// Archives often hold one folder with everything in it; take its contents.
fn strip_common_folder(mut files: Vec<(String, Vec<u8>)>) -> Vec<(String, Vec<u8>)> {
    loop {
        let Some(first) = files.first().and_then(|(p, _)| p.split_once('/')).map(|(d, _)| format!("{d}/")) else { return files };
        if !files.iter().all(|(p, _)| p.starts_with(&first)) {
            return files;
        }
        for (p, _) in &mut files {
            *p = p.get(first.len()..).unwrap_or_default().to_owned();
        }
    }
}

/// The main file: a `.tex` with `\documentclass` and `\begin{document}`, preferring the usual
/// names and the shallowest folder.
fn find_main(files: &[(String, Vec<u8>)]) -> Option<String> {
    let mut best: Option<(i32, &str)> = None;
    for (path, bytes) in files {
        if !path.to_ascii_lowercase().ends_with(".tex") {
            continue;
        }
        let Ok(text) = std::str::from_utf8(bytes) else { continue };
        let masked = mask_comments(text);
        if find_command(&masked, "documentclass", 0).is_none() || masked.find("\\begin{document}").is_none() {
            continue;
        }
        let stem = path.rsplit('/').next().unwrap_or(path).trim_end_matches(".tex").to_ascii_lowercase();
        let mut score = 0i32;
        if ["main", "template", "sample", "example", "paper", "article", "manuscript", "demo"].iter().any(|n| stem.contains(n)) {
            score += 10;
        }
        score -= 5 * i32::try_from(path.matches('/').count()).unwrap_or(10);
        if best.is_none_or(|(s, p)| score > s || (score == s && path.len() < p.len())) {
            best = Some((score, path));
        }
    }
    best.map(|(_, p)| p.to_owned())
}

// ---------------------------------------------------------------------------------------------
// Reading the main file.

/// `text` with every comment blanked out byte for byte, so positions stay the same.
pub(crate) fn mask_comments(text: &str) -> String {
    let mut out = Vec::with_capacity(text.len());
    let mut in_comment = false;
    let mut escaped = false;
    for &b in text.as_bytes() {
        if b == b'\n' {
            in_comment = false;
            escaped = false;
            out.push(b);
            continue;
        }
        if in_comment {
            out.push(b' ');
            continue;
        }
        if b == b'%' && !escaped {
            in_comment = true;
            out.push(b' ');
            continue;
        }
        escaped = b == b'\\' && !escaped;
        out.push(b);
    }
    // Only ASCII bytes were replaced by ASCII spaces, and every byte of a comment is replaced.
    String::from_utf8(out).unwrap_or_default()
}

/// The position of `\name` (not followed by a letter) at or after `from`.
fn find_command(masked: &str, name: &str, from: usize) -> Option<usize> {
    let needle = format!("\\{name}");
    let mut at = from;
    while let Some(found) = masked.get(at..)?.find(&needle) {
        let start = at + found;
        let end = start + needle.len();
        if !masked.as_bytes().get(end).is_some_and(u8::is_ascii_alphabetic) {
            return Some(start);
        }
        at = end;
    }
    None
}

/// The end (exclusive) of the group that opens at `open` (`{` or `[`).
fn group_end(masked: &str, open: usize) -> Option<usize> {
    let bytes = masked.as_bytes();
    let (left, right) = match bytes.get(open)? {
        b'{' => (b'{', b'}'),
        b'[' => (b'[', b']'),
        _ => return None,
    };
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() && i - open < MAX_GROUP {
        match bytes.get(i)? {
            b'\\' => i += 1,
            b if *b == left => depth += 1,
            b if *b == right => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn skip_space(masked: &str, mut at: usize) -> usize {
    while masked.as_bytes().get(at).is_some_and(|b| b.is_ascii_whitespace()) {
        at += 1;
    }
    at
}

/// A command found with its arguments: where it starts and ends, its optional arguments and its
/// mandatory ones (each the range inside the braces).
#[derive(Debug, Clone)]
pub(crate) struct Call {
    start: usize,
    pub(crate) end: usize,
    optional: Vec<(usize, usize)>,
    mandatory: Vec<(usize, usize)>,
}

/// `\name[..]{..}` at or after `from`, with up to `args` mandatory arguments.
pub(crate) fn find_call(masked: &str, name: &str, from: usize, args: usize) -> Option<Call> {
    let start = find_command(masked, name, from)?;
    let mut at = start + 1 + name.len();
    let mut optional = Vec::new();
    let mut mandatory = Vec::new();
    loop {
        let next = skip_space(masked, at);
        match masked.as_bytes().get(next) {
            Some(b'[') if mandatory.is_empty() => {
                let end = group_end(masked, next)?;
                optional.push((next + 1, end - 1));
                at = end;
            }
            Some(b'{') if mandatory.len() < args => {
                let end = group_end(masked, next)?;
                mandatory.push((next + 1, end - 1));
                at = end;
            }
            _ => break,
        }
    }
    Some(Call { start, end: at, optional, mandatory })
}

/// Every call of `\name` in `from..to`.
fn calls(masked: &str, name: &str, from: usize, to: usize, args: usize) -> Vec<Call> {
    let mut out = Vec::new();
    let mut at = from;
    while let Some(call) = find_call(masked, name, at, args) {
        if call.start >= to {
            break;
        }
        at = call.end.max(call.start + 1);
        out.push(call);
        if out.len() > 1000 {
            break;
        }
    }
    out
}

/// `\begin{env}` .. `\end{env}` in `from..to`: (start of `\begin`, end of the `\begin{env}`,
/// start of `\end`, end of `\end{env}`).
fn environment(masked: &str, env: &str, from: usize, to: usize) -> Option<(usize, usize, usize, usize)> {
    let open = format!("\\begin{{{env}}}");
    let close = format!("\\end{{{env}}}");
    let start = from + masked.get(from..to)?.find(&open)?;
    let inner = start + open.len();
    let end = inner + masked.get(inner..)?.find(&close)?;
    Some((start, inner, end, end + close.len()))
}

/// What the main file says.
struct Analysis {
    /// The rewritten main file.
    main: String,
    /// The template's own body (its sample text), now the body file.
    body: String,
    engine: crate::Engine,
    bibliography: &'static str,
    found: Vec<String>,
    warnings: Vec<String>,
}

/// One replacement in the main file.
struct Edit {
    from: usize,
    to: usize,
    text: String,
}

/// The macros the metadata file sets, defined for a template that isn't PSG's own.
const SETTERS: &[&str] = &[
    "Journal",
    "JournalAbbrev",
    "ISSN",
    "Volume",
    "Issue",
    "Year",
    "Pages",
    "ArticleNumber",
    "DOI",
    "ArticleType",
    "Title",
    "ShortTitle",
    "Authors",
    "Affiliations",
    "Correspondence",
    "Abstract",
    "Keywords",
    "Received",
    "Revised",
    "Accepted",
    "Published",
    "Licence",
    "Copyright",
    "PdfTitle",
    "PdfAuthor",
    "PdfKeywords",
    "Language",
];

/// What a manuscript converted from Word uses (Pandoc's LaTeX), for templates that don't load
/// it themselves. Loading a package the template already loaded does nothing.
const PANDOC_SUPPORT: &str = "\\RequirePackage{graphicx,longtable,booktabs,array,calc,amsmath,amssymb}\n\\providecommand{\\tightlist}{\\setlength{\\itemsep}{0pt}\\setlength{\\parskip}{0pt}}\n\\providecommand{\\passthrough}[1]{#1}\n\\providecommand{\\pandocbounded}[1]{\\resizebox{\\ifdim\\width>\\linewidth\\linewidth\\else\\width\\fi}{!}{#1}}\n\\AtBeginDocument{\\providecommand{\\hypertarget}[2]{#2}\\providecommand{\\href}[2]{#2}\\providecommand{\\url}[1]{\\texttt{#1}}}\n";

/// `\psgLanguage{zh-hant}` (and zh-hans, ja) for a template that isn't PSG's own.
const LANGUAGE_NAMES: &str = "\\def\\psgmeta@names#1#2#3#4{\\AtBeginDocument{\\def\\abstractname{#1}\\def\\figurename{#2}\\def\\tablename{#3}\\def\\refname{#4}\\def\\bibname{#4}}\\def\\Authand{、}\\def\\Authands{、}}\n\\def\\psgmeta@zhhant{zh-hant}\\def\\psgmeta@zhhans{zh-hans}\\def\\psgmeta@ja{ja}\n\\def\\psgLanguage#1{\\def\\psgmetaLanguage{#1}\\edef\\psgmeta@lang{#1}%\n  \\ifx\\psgmeta@lang\\psgmeta@zhhant\\psgmeta@names{摘要}{圖}{表}{參考文獻}\\fi\n  \\ifx\\psgmeta@lang\\psgmeta@zhhans\\psgmeta@names{摘要}{图}{表}{参考文献}\\fi\n  \\ifx\\psgmeta@lang\\psgmeta@ja\\psgmeta@names{要旨}{図}{表}{参考文献}\\fi}\n";

/// The Noto CJK faces PSG Academic PDF installs, for a xeCJK or ctex template that names no
/// Chinese font of its own: the same faces on every computer, with Traditional and Simplified
/// Chinese and Japanese.
const NOTO_CJK: &str = concat!("\\@ifpackageloaded{xeCJK}{", noto_cjk_fonts!(), "}{}\n");

/// The xeCJK font setup with the Noto CJK faces, when they are installed.
macro_rules! noto_cjk_fonts {
    () => {
        "\\IfFontExistsTF{NotoSerifTC-Regular.otf}{\\setCJKmainfont{NotoSerifTC-Regular.otf}[BoldFont=NotoSerifTC-Bold.otf]\\setCJKsansfont{NotoSansTC-Regular.otf}[BoldFont=NotoSansTC-Bold.otf]\\setCJKmonofont{NotoSansTC-Regular.otf}\\IfFontExistsTF{NotoSerifSC-Regular.otf}{\\setCJKfallbackfamilyfont{\\CJKrmdefault}[BoldFont=NotoSerifSC-Bold.otf]{NotoSerifSC-Regular.otf}}{}}{}"
    };
}
use noto_cjk_fonts;

/// What a pdfLaTeX template gets when its manuscript has Chinese or Japanese text, right after
/// `\documentclass`.
pub(crate) const XECJK_FOR_PDFLATEX: &str = concat!(
    "\n%% Added by PSG Academic PDF: the manuscript has Chinese or Japanese text, which pdfLaTeX can't set,\n%% so this template is typeset with XeLaTeX and xeCJK.\n\\usepackage{xeCJK}\n",
    noto_cjk_fonts!(),
    "\n"
);

/// What goes before `\begin{document}`: the setters `psg-metadata.tex` calls, kept as
/// `\psgmeta…` macros, and the metadata itself.
fn metadata_block(authblk: bool, cjk_fonts: bool) -> String {
    let mut out = String::from(
        "%% Added by PSG Academic PDF: the publication metadata (psg-metadata.tex), which replaces the\n%% title, authors, abstract and keywords typed above.\n\\makeatletter\n\\providecommand\\psgAffMark[1]{\\textsuperscript{#1}}\n",
    );
    out.push_str(PANDOC_SUPPORT);
    if cjk_fonts {
        out.push_str(NOTO_CJK);
    }
    for s in SETTERS {
        out.push_str(&format!("\\long\\def\\psgmeta{s}{{}}\\long\\def\\psg{s}#1{{\\long\\def\\psgmeta{s}{{#1}}}}\n"));
    }
    out.push_str(
        "\\newcommand\\psgmeta@ifempty[1]{\\if\\relax\\detokenize\\expandafter{#1}\\relax\\expandafter\\@firstoftwo\\else\\expandafter\\@secondoftwo\\fi}\n",
    );
    // The article's language names the abstract, figures, tables and references, and joins
    // authblk's authors with 、 in Chinese and Japanese.
    out.push_str(LANGUAGE_NAMES);
    // The authors and affiliations as lists (psgAuthorList, psgAffiliationList).
    out.push_str(
        "\\providecommand\\psgAuthorList[1]{}\\providecommand\\psgAffiliationList[1]{}\n\\renewcommand\\psgAuthorList[1]{\\def\\psgmeta@authors{#1}}\n\\renewcommand\\psgAffiliationList[1]{\\def\\psgmeta@affils{#1}}\n\\def\\psgmeta@authors{}\\def\\psgmeta@affils{}\n",
    );
    if authblk {
        // authblk: one \author and \affil per entry.
        out.push_str(
            "\\newcommand\\psgmetaAuthorCommands{\\def\\psgAuthorItem##1##2{\\author[##2]{##1}}\\def\\psgAffiliationItem##1##2{\\affil[##1]{##2}}\\psgmeta@authors\\psgmeta@affils}\n",
        );
    } else {
        // \author's argument is set as table rows: each affiliation is a row of its own, and
        // nothing groups across a row's end.
        out.push_str(
            "\\def\\psgAffiliationItem#1#2{\\\\\\footnotesize\\psgAffMark{#1}#2}\n\\newcommand\\psgmetaAuthorBlock{\\psgmetaAuthors\\psgmeta@affils}\n",
        );
    }
    out.push_str("\\input{psg-metadata}\n\\psgmeta@ifempty\\psgmetaShortTitle{\\let\\psgmetaShortTitle\\psgmetaTitle}{}\n");
    if authblk {
        // authblk's \author and \affil are called here, once the metadata is read.
        out.push_str("\\psgmetaAuthorCommands\n");
    }
    out.push_str("\\makeatother\n");
    out
}

/// The main file being read: its text, the same with comments blanked out, where the document
/// starts and ends, and what has been found and is to be changed so far.
struct Scan<'a> {
    text: &'a str,
    masked: String,
    /// `\begin{document}`, and the end of it.
    begin: usize,
    begin_end: usize,
    /// `\end{document}` (or the end of the text).
    end_doc: usize,
    edits: Vec<Edit>,
    found: Vec<String>,
    warnings: Vec<String>,
}

/// The class, packages, engine and bibliography tool.
struct Setup {
    class: String,
    packages: Vec<String>,
    engine: crate::Engine,
    bibliography: &'static str,
}

impl Setup {
    fn uses(&self, package: &str) -> bool {
        self.packages.iter().any(|p| p == package)
    }

    fn loads_xecjk(&self) -> bool {
        self.uses("xeCJK") || self.uses("ctex") || self.class.starts_with("ctex")
    }
}

fn analyse(text: &str, body_input: &str) -> Result<Analysis, Error> {
    let masked = mask_comments(text);
    let begin = masked.find("\\begin{document}").ok_or_else(|| Error::Template("the main file has no \\begin{document}".into()))?;
    let begin_end = begin + "\\begin{document}".len();
    let end_doc = masked.get(begin_end..).and_then(|m| m.find("\\end{document}")).map_or(masked.len(), |e| begin_end + e);
    let mut scan = Scan { text, masked, begin, begin_end, end_doc, edits: Vec::new(), found: Vec::new(), warnings: Vec::new() };
    let setup = scan.setup();
    let front_end = scan.front_matter();
    let body = scan.body(front_end, body_input);
    scan.title_and_authors(front_end, setup.uses("authblk"));
    if setup.uses("biblatex") {
        scan.biber_backend();
    }
    // A xeCJK or ctex template that names no Chinese font gets the Noto faces.
    let names_cjk_font =
        find_command(&scan.masked, "setCJKmainfont", 0).is_some_and(|at| at < begin) || text.to_ascii_lowercase().contains("fontset=");
    let cjk_fonts = setup.engine == crate::Engine::Xelatex && setup.loads_xecjk() && !names_cjk_font;
    if cjk_fonts {
        scan.found.push("Chinese and Japanese text: Noto Serif CJK, which PSG Academic PDF installs (the template names no Chinese font).".into());
    }
    // The metadata goes just before \begin{document}.
    scan.edits.push(Edit { from: begin, to: begin, text: metadata_block(setup.uses("authblk"), cjk_fonts) });
    let Scan { edits, found, warnings, .. } = scan;
    Ok(Analysis { main: apply(text, edits), body, engine: setup.engine, bibliography: setup.bibliography, found, warnings })
}

/// `text` with `edits` made, from the end; an edit overlapping one already made is skipped.
fn apply(text: &str, mut edits: Vec<Edit>) -> String {
    edits.sort_by(|a, b| b.from.cmp(&a.from).then(b.to.cmp(&a.to)));
    let mut main = text.to_owned();
    let mut floor = usize::MAX;
    for edit in edits {
        if edit.to > floor || edit.from > edit.to || !main.is_char_boundary(edit.from) || !main.is_char_boundary(edit.to) {
            continue;
        }
        main.replace_range(edit.from..edit.to, &edit.text);
        floor = edit.from;
    }
    main
}

impl Scan<'_> {
    /// The class and packages, the engine they need and the bibliography tool.
    fn setup(&mut self) -> Setup {
        let (text, masked, begin) = (self.text, &self.masked, self.begin);
        let lower = text.to_ascii_lowercase();
        let class =
            find_call(masked, "documentclass", 0, 1).and_then(|c| c.mandatory.first().and_then(|(a, b)| text.get(*a..*b))).unwrap_or("").to_owned();
        let packages: Vec<String> = calls(masked, "usepackage", 0, begin, 1)
            .iter()
            .chain(calls(masked, "RequirePackage", 0, begin, 1).iter())
            .filter_map(|c| c.mandatory.first().and_then(|(a, b)| text.get(*a..*b)))
            .flat_map(|list| list.split(',').map(|p| p.trim().to_owned()).collect::<Vec<_>>())
            .collect();
        let mut setup = Setup { class, packages, engine: crate::Engine::Pdflatex, bibliography: "none" };
        let cjk_text = text.chars().any(|c| matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF));
        let wants_lua =
            lower.contains("program = lualatex") || lower.contains("program=lualatex") || setup.uses("luatexja") || setup.class.starts_with("ltj");
        let wants_xe = lower.contains("program = xelatex") || lower.contains("program=xelatex");
        if setup.uses("fontspec") || setup.loads_xecjk() || wants_xe || cjk_text || wants_lua {
            setup.engine = crate::Engine::Xelatex;
        }
        if wants_lua {
            self.warnings.push(
                "The template asks for LuaLaTeX, which PSG Academic PDF does not run; it is typeset with XeLaTeX, which may need changes to the template."
                    .into(),
            );
        }
        let class_name = if setup.class.is_empty() { "(none named)" } else { setup.class.as_str() };
        self.found.push(format!("Class {class_name}; typeset with {}.", setup.engine.title()));
        if cjk_text && !setup.loads_xecjk() {
            self.warnings.push(
                "The template has Chinese or Japanese text but loads neither ctex nor xeCJK; add one of them, or the characters will be missing."
                    .into(),
            );
        }
        let bibtex_call = find_command(masked, "bibliography", self.begin_end).is_some_and(|at| at < self.end_doc);
        setup.bibliography = if setup.uses("biblatex") {
            "biber"
        } else if bibtex_call {
            "bibtex"
        } else {
            "none"
        };
        self.found.push(match setup.bibliography {
            "biber" => "Bibliography: BibLaTeX with Biber.".to_owned(),
            "bibtex" => "Bibliography: BibTeX (\\bibliography).".to_owned(),
            _ => "No bibliography file.".to_owned(),
        });
        setup
    }

    /// Fill the abstract and keywords from the metadata, and find where the title block ends:
    /// after `\maketitle`, the abstract and keywords, or, without them, right after
    /// `\begin{document}`.
    fn front_matter(&mut self) -> usize {
        let (begin_end, end_doc) = (self.begin_end, self.end_doc);
        let mut front_end = begin_end;
        let mut note_end = |at: usize| {
            if at > front_end && at <= end_doc {
                front_end = at;
            }
        };
        for name in ["maketitle", "IEEEpeerreviewmaketitle"] {
            for call in calls(&self.masked, name, begin_end, end_doc, 0) {
                note_end(call.end);
            }
        }
        let limit = first_section(&self.masked, begin_end, end_doc).unwrap_or(end_doc);
        let abstract_end = self.fill_abstract(limit);
        let keywords_end = self.fill_keywords(limit);
        for at in [abstract_end, keywords_end].into_iter().flatten() {
            note_end(at);
        }
        // Elsevier's and others' title block environment ends after the abstract and keywords.
        if let Some((_, _, _, after)) = environment(&self.masked, "frontmatter", begin_end, end_doc) {
            note_end(after);
        }
        if abstract_end.is_some() {
            self.found.push("Abstract: filled from the metadata.".into());
        } else {
            self.warnings.push("No abstract environment or \\abstract was found; the abstract is not printed.".into());
        }
        if keywords_end.is_some() {
            self.found.push("Keywords: filled from the metadata.".into());
        } else {
            self.warnings.push("No keywords command or environment was found; the keywords are not printed.".into());
        }
        // A title block typed into the document without \maketitle: keep it, and the body starts
        // at the first section.
        if front_end == begin_end
            && abstract_end.is_none()
            && let Some(section) = first_section(&self.masked, begin_end, end_doc)
            && self.masked.get(begin_end..section).is_some_and(|t| !t.trim().is_empty())
        {
            self.warnings.push(
                "No \\maketitle: the text before the first section is kept as the template's title block, with the template's own words.".into(),
            );
            front_end = section;
        }
        front_end
    }

    /// The abstract before `limit` (an environment or `\abstract{…}`) filled from the metadata.
    /// Returns where it ends, when there is one in the document.
    fn fill_abstract(&mut self, limit: usize) -> Option<usize> {
        if let Some((_, inner, close, after)) = environment(&self.masked, "abstract", 0, limit) {
            self.edits.push(Edit { from: inner, to: close, text: "\n\\psgmetaAbstract\n".into() });
            return Some(after);
        }
        let call = find_call(&self.masked, "abstract", 0, 1).filter(|c| c.start < limit)?;
        let &(a, b) = call.mandatory.first()?;
        self.edits.push(Edit { from: a, to: b, text: "\\psgmetaAbstract".into() });
        Some(call.end)
    }

    /// The keywords before `limit`: an environment, a command, or a labelled line
    /// ("\textbf{Keywords:} a, b", "關鍵詞：甲；乙"), filled from the metadata. Returns where
    /// they end (`Some(0)` for a command in the preamble).
    fn fill_keywords(&mut self, limit: usize) -> Option<usize> {
        for env in ["keywords", "keyword", "IEEEkeywords", "kwd"] {
            if let Some((_, inner, close, after)) = environment(&self.masked, env, 0, limit) {
                self.edits.push(Edit { from: inner, to: close, text: "\n\\psgmetaKeywords\n".into() });
                return Some(after);
            }
        }
        for name in ["keywords", "keyword", "kwd"] {
            if let Some(call) = find_call(&self.masked, name, 0, 1).filter(|c| c.start < limit)
                && let Some(&(a, b)) = call.mandatory.first()
            {
                self.edits.push(Edit { from: a, to: b, text: "\\psgmetaKeywords".into() });
                return Some(if call.start > self.begin { call.end } else { 0 });
            }
        }
        let (value, line_end) = keyword_line(&self.masked, self.begin_end, limit)?;
        self.edits.push(Edit { from: value, to: line_end, text: "\\psgmetaKeywords".into() });
        Some(line_end)
    }

    /// The article's text, from `front_end` to the bibliography, becomes the body file; the
    /// main file inputs it there. Returns the body file's text.
    fn body(&mut self, front_end: usize, body_input: &str) -> String {
        let (text, masked) = (self.text, &self.masked);
        let mut body_end = self.end_doc;
        for name in ["bibliographystyle", "bibliography", "printbibliography"] {
            if let Some(at) = find_command(masked, name, front_end).filter(|&at| at < body_end) {
                body_end = at;
            }
        }
        let mut sample_references = None;
        if let Some((start, _, _, after)) = environment(masked, "thebibliography", front_end, self.end_doc)
            && start < body_end
        {
            // A typed reference list is the template's sample; it goes with the sample body.
            body_end = start;
            sample_references = Some((start, after));
        }
        let sample = text.get(front_end..body_end).unwrap_or_default().trim();
        let mut body = format!("% The article's text: PSG Academic PDF replaces this file with the imported manuscript.\n{sample}\n");
        if let Some((start, after)) = sample_references {
            body.push_str(text.get(start..after).unwrap_or_default());
            body.push('\n');
            self.edits.push(Edit { from: start, to: after, text: String::new() });
        }
        self.edits.push(Edit { from: front_end, to: body_end, text: format!("\n\\input{{{body_input}}}\n") });
        self.found.push(format!(
            "The article's text: lines {} to {} of the main file, now {body_input}.tex.",
            line_of(text, front_end),
            line_of(text, body_end)
        ));
        body
    }

    /// `\title` and `\author` (with the affiliation commands beside them) before `front_end`
    /// take the metadata's.
    fn title_and_authors(&mut self, front_end: usize, authblk: bool) {
        match find_call(&self.masked, "title", 0, 1).filter(|c| c.start < front_end && !c.mandatory.is_empty()) {
            Some(call) => {
                for &(a, b) in &call.optional {
                    self.edits.push(Edit { from: a, to: b, text: "\\psgmetaShortTitle".into() });
                }
                if let Some(&(a, b)) = call.mandatory.first() {
                    self.edits.push(Edit { from: a, to: b, text: "\\psgmetaTitle".into() });
                }
                self.found.push("Title: \\title, filled from the metadata.".into());
            }
            None => self.warnings.push("No \\title was found; the template's title stays as it is.".into()),
        }
        let mut author_calls = calls(&self.masked, "author", 0, front_end, 1);
        author_calls.retain(|c| !c.mandatory.is_empty());
        let Some(first) = author_calls.first() else {
            self.warnings.push("No \\author was found; the template's authors stay as they are.".into());
            return;
        };
        // With authblk the authors are set where the metadata is read; see `metadata_block`.
        let replacement = if authblk { String::new() } else { "\\author{\\psgmetaAuthorBlock}".to_owned() };
        self.edits.push(Edit { from: first.start, to: first.end, text: replacement });
        for extra in author_calls.iter().skip(1) {
            self.edits.push(Edit { from: extra.start, to: extra.end, text: String::new() });
        }
        let mut removed = 0;
        for name in ["affil", "affiliation", "address", "institute", "email", "orcid", "corres", "corresp"] {
            for call in calls(&self.masked, name, 0, front_end, 1).into_iter().filter(|c| !c.mandatory.is_empty()) {
                self.edits.push(Edit { from: call.start, to: call.end, text: String::new() });
                removed += 1;
            }
        }
        let also = if removed > 0 { format!(" and {removed} affiliation or address command(s)") } else { String::new() };
        self.found.push(format!("Authors: {} \\author command(s){also} replaced by the metadata's authors and affiliations.", author_calls.len()));
    }

    /// BibLaTeX's bibtex backend becomes Biber's, which PSG Academic PDF runs.
    fn biber_backend(&mut self) {
        for call in calls(&self.masked, "usepackage", 0, self.begin, 1) {
            for &(a, b) in &call.optional {
                if let Some(opts) = self.text.get(a..b).filter(|o| o.contains("backend=bibtex")) {
                    let text = opts.replace("backend=bibtex8", "backend=biber").replace("backend=bibtex", "backend=biber");
                    self.edits.push(Edit { from: a, to: b, text });
                    self.warnings.push("BibLaTeX was set to the bibtex backend; it now uses Biber, which PSG Academic PDF runs.".into());
                }
            }
        }
    }
}

/// A line in `from..to` that starts with a keywords label: where its words start and where the
/// line ends.
fn keyword_line(masked: &str, from: usize, to: usize) -> Option<(usize, usize)> {
    const LABELS: &[&str] = &["keywords", "key words", "index terms", "關鍵詞", "关键词", "關鍵字", "关键字", "キーワード"];
    let lower = masked.get(from..to)?.to_ascii_lowercase();
    let (at, label) = LABELS.iter().filter_map(|l| lower.find(l).map(|at| (at, *l))).min_by_key(|(at, _)| *at)?;
    let mut value = from + at + label.len();
    let line_end = masked.get(value..).and_then(|rest| rest.find('\n')).map_or(to, |n| value + n);
    // Past the label's punctuation and the brace that closes \textbf{…}.
    while let Some(c) = masked.get(value..line_end).and_then(|rest| rest.chars().next()) {
        if matches!(c, ':' | '：' | '}' | ' ' | '\t' | '*' | '.') {
            value += c.len_utf8();
        } else {
            break;
        }
    }
    Some((value, line_end))
}

/// The first `\section` (or `\chapter`) in `from..to`.
fn first_section(masked: &str, from: usize, to: usize) -> Option<usize> {
    ["section", "chapter"].iter().filter_map(|n| find_command(masked, n, from)).filter(|&at| at < to).min()
}

fn line_of(text: &str, at: usize) -> usize {
    text.get(..at).map_or(0, |t| t.matches('\n').count()) + 1
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const ARTICLE: &str = r"\documentclass[11pt]{article}
\usepackage{amsmath}
% \title{A commented title}
\title[Short]{Journal of Things: Sample Article}
\author{Ann Author\thanks{Uni A}}
\author{Bob Writer}
\date{}
\begin{document}
\maketitle
\begin{abstract}
This is the sample abstract.
\end{abstract}
\keywords{one, two}

\section{Introduction}
Sample text 50\% of the time.

\bibliographystyle{plain}
\bibliography{refs}
\end{document}
";

    #[test]
    fn a_plain_article_template_is_understood() {
        let a = analyse(ARTICLE, "body").unwrap();
        assert_eq!(a.engine, crate::Engine::Pdflatex);
        assert_eq!(a.bibliography, "bibtex");
        assert!(a.body.contains("\\section{Introduction}\nSample text 50\\% of the time."), "{}", a.body);
        assert!(!a.body.contains("bibliography"));
        assert!(a.main.contains("\\title[\\psgmetaShortTitle]{\\psgmetaTitle}"), "{}", a.main);
        assert!(a.main.contains("% \\title{A commented title}"), "comments stay as they were");
        assert!(a.main.contains("\\author{\\psgmetaAuthorBlock}") && !a.main.contains("Bob Writer"), "{}", a.main);
        assert!(a.main.contains("\\begin{abstract}\n\\psgmetaAbstract\n\\end{abstract}"));
        assert!(a.main.contains("\\keywords{\\psgmetaKeywords}"));
        assert!(a.main.contains("\\input{body}\n") && a.main.contains("\\bibliographystyle{plain}\n\\bibliography{refs}"), "{}", a.main);
        assert!(a.main.find("\\input{psg-metadata}").unwrap() < a.main.find("\\begin{document}").unwrap());
        assert!(a.warnings.is_empty(), "{:?}", a.warnings);
    }

    #[test]
    fn a_chinese_template_with_biblatex_and_authblk() {
        let tex = "\\documentclass{ctexart}\n\\usepackage{authblk}\n\\usepackage[backend=bibtex,style=gb7714-2015]{biblatex}\n\\addbibresource{ref.bib}\n\\title{論文題目}\n\\author[1]{張三}\n\\affil[1]{某大學}\n\\begin{document}\n\\maketitle\n\\begin{abstract}摘要內容\\end{abstract}\n\\noindent\\textbf{關鍵詞：}甲；乙\n\\section{引言}正文\n\\printbibliography\n\\end{document}\n";
        let a = analyse(tex, "body").unwrap();
        assert_eq!(a.engine, crate::Engine::Xelatex);
        assert_eq!(a.bibliography, "biber");
        assert!(a.main.contains("[backend=biber,style=gb7714-2015]"), "{}", a.main);
        assert!(a.main.contains("\\noindent\\textbf{關鍵詞：}\\psgmetaKeywords\n"), "{}", a.main);
        assert!(a.main.contains("\\input{psg-metadata}") && a.main.contains("\\psgmetaAuthorCommands\n\\makeatother"), "{}", a.main);
        assert!(!a.main.contains("某大學") && !a.main.contains("張三"), "{}", a.main);
        assert!(a.body.contains("\\section{引言}正文"));
        assert!(a.main.contains("\\input{body}\n\\printbibliography"), "{}", a.main);
    }

    #[test]
    fn an_elsevier_style_front_matter_stays_whole() {
        let tex = "\\documentclass{elsarticle}\n\\begin{document}\n\\begin{frontmatter}\n\\title{T}\n\\author{A}\n\\address{Somewhere}\n\\begin{abstract}\nAbs.\n\\end{abstract}\n\\begin{keyword}\nk1 \\sep k2\n\\end{keyword}\n\\end{frontmatter}\n\\section{Intro}\nText.\n\\bibliography{refs}\n\\end{document}\n";
        let a = analyse(tex, "body").unwrap();
        assert!(a.main.contains("\\end{keyword}\n\\end{frontmatter}\n\\input{body}"), "{}", a.main);
        assert!(a.body.contains("\\section{Intro}") && !a.body.contains("frontmatter"), "{}", a.body);
        assert!(!a.main.contains("Somewhere") && a.main.contains("\\author{\\psgmetaAuthorBlock}"));
    }

    #[test]
    fn a_template_without_a_title_block_keeps_its_front_text() {
        let tex = "\\documentclass{article}\n\\begin{document}\n{\\Large My Journal}\n\n\\section{One}\nText.\n\\end{document}\n";
        let a = analyse(tex, "body").unwrap();
        assert!(a.main.contains("{\\Large My Journal}"));
        assert!(a.body.contains("\\section{One}\nText."));
        assert!(a.warnings.iter().any(|w| w.contains("No \\maketitle")));
        assert!(a.warnings.iter().any(|w| w.contains("No \\title")));
    }

    #[test]
    fn typed_references_go_with_the_sample_body() {
        let tex = "\\documentclass{article}\n\\title{T}\\author{A}\n\\begin{document}\\maketitle\nText \\cite{a}.\n\\begin{thebibliography}{9}\\bibitem{a} A.\\end{thebibliography}\n\\end{document}\n";
        let a = analyse(tex, "body").unwrap();
        assert!(a.body.contains("\\bibitem{a}"));
        assert!(!a.main.contains("\\bibitem"), "{}", a.main);
    }

    #[test]
    fn hostile_input_is_bounded() {
        assert_eq!(group_end("{{{{", 0), None);
        assert_eq!(group_end("{a\\}b}", 0), Some(6));
        let deep = format!("\\documentclass{{article}}\\title{{{}}}\\begin{{document}}x\\end{{document}}", "{".repeat(300_000));
        assert!(analyse(&deep, "body").is_ok());
        // Every byte of a comment becomes a space, so positions stay put ("é" is two bytes).
        assert_eq!(mask_comments("a % b é\nc \\% d"), format!("a{}\nc \\% d", " ".repeat(7)));
    }

    fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
        use std::io::Write;
        let mut out = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut out);
        for (name, text) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        out.into_inner()
    }

    #[test]
    fn a_zip_becomes_a_template_in_the_store() {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("Journal of Things.zip");
        std::fs::write(
            &src,
            zip_of(&[
                ("jot/main.tex", ARTICLE),
                ("jot/refs.bib", "@book{a, title={A}}"),
                ("jot/figures/logo.pdf", "%PDF"),
                ("jot/main.aux", "generated"),
                ("jot/說明.txt", "notes"),
                ("__MACOSX/jot/._main.tex", "junk"),
            ]),
        )
        .unwrap();
        let store = root.path().join("templates");
        let imported = import(&src, &store, None).unwrap();
        let m = &imported.template.manifest;
        assert_eq!(m.id, "journal-of-things");
        assert_eq!(m.name, "Journal of Things");
        assert_eq!(m.main, "main.tex");
        assert_eq!(m.body, "body.tex");
        assert_eq!(m.bibliography, "bibtex");
        let mut files = m.files.clone();
        files.sort();
        assert_eq!(files, ["body.tex", "figures/logo.pdf", "main.tex", "psg-metadata.tex", "refs.bib"]);
        assert!(imported.warnings.iter().any(|w| w.contains("說明.txt")), "{:?}", imported.warnings);
        assert!(std::fs::read_to_string(imported.dir.join("main.tex")).unwrap().contains("\\input{body}"));
        // It is found with the other templates, and a project made from it gets every file.
        let (all, problems) = crate::template::discover(std::slice::from_ref(&store));
        assert!(problems.is_empty(), "{problems:?}");
        let t = all.into_iter().find(|t| t.manifest.id == "journal-of-things").unwrap();
        let p = crate::Project::create(&root.path().join("p"), &t, PublicationMetadata::sample()).unwrap();
        assert!(p.dir.join("figures/logo.pdf").is_file());
        // Adding it again replaces it.
        assert!(import(&src, &store, None).is_ok());
        assert_eq!(std::fs::read_dir(&store).unwrap().count(), 1);
    }

    #[test]
    fn a_folder_without_a_main_file_is_refused() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("x.tex"), "no class").unwrap();
        let err = import(root.path(), &root.path().join("store"), Some("X")).unwrap_err();
        assert!(err.to_string().contains("no .tex file with \\documentclass"), "{err}");
    }

    #[test]
    fn ids_are_ascii_and_stable() {
        assert_eq!(template_id("Journal of Things!"), "journal-of-things");
        let chinese = template_id("中文期刊模板");
        assert!(chinese.starts_with("template-") && chinese == template_id("中文期刊模板"));
        assert!(crate::paths::safe_relative_name(&chinese).is_some());
    }
}
