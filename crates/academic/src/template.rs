//! Journal templates: a directory per template with a `template.toml` manifest.
//!
//! The PSG Research Article template is compiled into the program, so the app always has one
//! template that works. More templates are read from template directories (`templates/academic`
//! beside the program, or `PSG_TEMPLATES_DIR`); see `templates/academic/README.md`.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::Error;
use crate::paths::safe_relative_name;

/// The manifest's file name in a template directory.
pub const MANIFEST: &str = "template.toml";
/// The largest template file read (bytes).
const MAX_TEMPLATE_FILE: u64 = 8 * 1024 * 1024;
/// The most files a template may list.
const MAX_TEMPLATE_FILES: usize = 256;

/// What a template's manifest says.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub licence: String,
    /// Only `pdflatex` in this phase.
    #[serde(default = "default_engine")]
    pub engine: String,
    /// `biber` or `none`.
    #[serde(default = "default_bibliography")]
    pub bibliography: String,
    pub main: String,
    pub body: String,
    pub metadata: String,
    pub files: Vec<String>,
}

fn default_engine() -> String {
    "pdflatex".into()
}

fn default_bibliography() -> String {
    "biber".into()
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Self, Error> {
        let m: Self = toml::from_str(text).map_err(|e| Error::Template(format!("{MANIFEST}: {e}")))?;
        m.check()?;
        Ok(m)
    }

    fn check(&self) -> Result<(), Error> {
        let bad = |why: String| Err(Error::Template(format!("template \"{}\": {why}", self.id)));
        if self.id.is_empty() || !self.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
            return bad("the id may use only letters, digits, '-' and '_'".into());
        }
        if self.engine != "pdflatex" {
            return bad(format!("engine \"{}\" is not supported; only pdflatex is", self.engine));
        }
        if !matches!(self.bibliography.as_str(), "biber" | "none") {
            return bad(format!("bibliography \"{}\" is not supported; use \"biber\" or \"none\"", self.bibliography));
        }
        if self.files.len() > MAX_TEMPLATE_FILES {
            return bad(format!("more than {MAX_TEMPLATE_FILES} files"));
        }
        for f in &self.files {
            if safe_relative_name(f).is_none() {
                return bad(format!("\"{f}\" is not a plain file name"));
            }
        }
        for (what, f) in [("main", &self.main), ("body", &self.body), ("metadata", &self.metadata)] {
            if !self.files.contains(f) {
                return bad(format!("its {what} file \"{f}\" is not in `files`"));
            }
            if !f.ends_with(".tex") {
                return bad(format!("its {what} file \"{f}\" must be a .tex file"));
            }
        }
        Ok(())
    }

    pub fn uses_biber(&self) -> bool {
        self.bibliography == "biber"
    }
}

/// Where a template's files come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Compiled into the program.
    Builtin(&'static [(&'static str, &'static str)]),
    /// A directory on disk.
    Directory(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Template {
    pub manifest: Manifest,
    pub source: Source,
}

/// The PSG Research Article template's files, compiled in.
const PSG_ARTICLE: &[(&str, &str)] = &[
    ("template.toml", include_str!("../../../templates/academic/psg-article/template.toml")),
    ("main.tex", include_str!("../../../templates/academic/psg-article/main.tex")),
    ("psg-article.cls", include_str!("../../../templates/academic/psg-article/psg-article.cls")),
    ("body.tex", include_str!("../../../templates/academic/psg-article/body.tex")),
    ("references.bib", include_str!("../../../templates/academic/psg-article/references.bib")),
    ("psg-metadata.tex", include_str!("../../../templates/academic/psg-article/psg-metadata.tex")),
];

impl Template {
    /// The templates compiled into the program.
    pub fn builtin() -> Vec<Template> {
        let mut out = Vec::new();
        // One entry per built-in template; more are added here.
        let builtins: &[&'static [(&'static str, &'static str)]] = &[PSG_ARTICLE];
        for &files in builtins {
            let manifest = files.iter().find(|(n, _)| *n == MANIFEST).map(|(_, t)| Manifest::parse(t));
            match manifest {
                Some(Ok(manifest)) => out.push(Template { manifest, source: Source::Builtin(files) }),
                Some(Err(e)) => log::error!("built-in template: {e}"),
                None => log::error!("built-in template without {MANIFEST}"),
            }
        }
        out
    }

    /// Read the template in `dir`.
    pub fn from_dir(dir: &Path) -> Result<Template, Error> {
        let text = read_limited(&dir.join(MANIFEST))?;
        let manifest = Manifest::parse(&text)?;
        for f in &manifest.files {
            let path = dir.join(f);
            if !path.is_file() {
                return Err(Error::Template(format!("template \"{}\" lists {f}, which is not in {}", manifest.id, dir.display())));
            }
        }
        Ok(Template { manifest, source: Source::Directory(dir.to_path_buf()) })
    }

    /// Copy the template's files into `dest`, which must exist.
    pub fn materialize(&self, dest: &Path) -> Result<(), Error> {
        for name in &self.manifest.files {
            let target = dest.join(name);
            match &self.source {
                Source::Builtin(files) => {
                    let text = files
                        .iter()
                        .find(|(n, _)| n == name)
                        .map(|(_, t)| *t)
                        .ok_or_else(|| Error::Template(format!("built-in template has no {name}")))?;
                    crate::paths::write_atomic(&target, text.as_bytes())?;
                }
                Source::Directory(dir) => {
                    let bytes = read_limited_bytes(&dir.join(name))?;
                    crate::paths::write_atomic(&target, &bytes)?;
                }
            }
        }
        Ok(())
    }
}

fn read_limited_bytes(path: &Path) -> Result<Vec<u8>, Error> {
    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    if meta.len() > MAX_TEMPLATE_FILE {
        return Err(Error::Template(format!("{} is larger than {} MB", path.display(), MAX_TEMPLATE_FILE / 1024 / 1024)));
    }
    std::fs::read(path).map_err(|e| Error::io(path, e))
}

fn read_limited(path: &Path) -> Result<String, Error> {
    String::from_utf8(read_limited_bytes(path)?).map_err(|_| Error::Template(format!("{} is not UTF-8 text", path.display())))
}

/// The built-in templates, then every valid template found in `dirs` (each a directory of
/// template directories). A template whose id is already listed is skipped; an invalid one is
/// reported in the second list and skipped.
pub fn discover(dirs: &[PathBuf]) -> (Vec<Template>, Vec<String>) {
    let mut templates = Template::builtin();
    let mut problems = Vec::new();
    for root in dirs {
        let Ok(entries) = std::fs::read_dir(root) else { continue };
        let mut subdirs: Vec<PathBuf> = entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.join(MANIFEST).is_file()).collect();
        subdirs.sort();
        for dir in subdirs.into_iter().take(MAX_TEMPLATE_FILES) {
            match Template::from_dir(&dir) {
                Ok(t) if templates.iter().any(|have| have.manifest.id == t.manifest.id) => {
                    log::debug!("template {} in {} is already listed", t.manifest.id, dir.display());
                }
                Ok(t) => templates.push(t),
                Err(e) => problems.push(e.to_string()),
            }
        }
    }
    (templates, problems)
}

/// Where template directories are looked for: `PSG_TEMPLATES_DIR`, then `templates/academic`
/// beside the program (Windows, development builds), `../Resources/templates/academic` (the macOS
/// app bundle, where `Contents/MacOS` may hold only signed code) and
/// `../share/psg-academic-pdf/templates` (Linux packages).
pub fn default_template_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os("PSG_TEMPLATES_DIR").filter(|d| !d.is_empty()) {
        dirs.push(PathBuf::from(dir));
    }
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)) {
        dirs.push(exe_dir.join("templates").join("academic"));
        dirs.push(exe_dir.join("..").join("Resources").join("templates").join("academic"));
        dirs.push(exe_dir.join("..").join("share").join("psg-academic-pdf").join("templates"));
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_dirs_include_the_macos_bundle_resources() {
        let resources = Path::new("..").join("Resources").join("templates").join("academic");
        assert!(default_template_dirs().iter().any(|d| d.ends_with(&resources)));
    }

    #[test]
    fn the_builtin_template_is_valid() {
        let t = Template::builtin();
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].manifest.id, "psg-article");
        assert!(t[0].manifest.uses_biber());
    }

    #[test]
    fn the_builtin_template_matches_its_directory() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../templates/academic/psg-article");
        let from_dir = Template::from_dir(&dir).unwrap();
        assert_eq!(from_dir.manifest, Template::builtin()[0].manifest);
    }

    #[test]
    fn manifests_with_unsafe_names_are_refused() {
        let base = "id = \"x\"\nname = \"X\"\nmain = \"main.tex\"\nbody = \"body.tex\"\nmetadata = \"m.tex\"\n";
        for files in ["[\"../main.tex\", \"body.tex\", \"m.tex\"]", "[\"/etc/main.tex\", \"body.tex\", \"m.tex\"]", "[\"body.tex\", \"m.tex\"]"] {
            let text = format!("{base}files = {files}\n");
            assert!(Manifest::parse(&text).is_err(), "{files}");
        }
        assert!(Manifest::parse(&format!("{base}files = [\"main.tex\", \"body.tex\", \"m.tex\"]\nengine = \"lualatex\"\n")).is_err());
        assert!(Manifest::parse(&format!("{base}files = [\"main.tex\", \"body.tex\", \"m.tex\"]\n")).is_ok());
    }

    #[test]
    fn discover_adds_directory_templates_and_reports_broken_ones() {
        let root = tempfile::tempdir().unwrap();
        let good = root.path().join("good");
        std::fs::create_dir(&good).unwrap();
        std::fs::write(good.join(MANIFEST), "id = \"good\"\nname = \"Good\"\nbibliography = \"none\"\nmain = \"main.tex\"\nbody = \"body.tex\"\nmetadata = \"m.tex\"\nfiles = [\"main.tex\", \"body.tex\", \"m.tex\"]\n").unwrap();
        for f in ["main.tex", "body.tex", "m.tex"] {
            std::fs::write(good.join(f), "").unwrap();
        }
        let bad = root.path().join("bad");
        std::fs::create_dir(&bad).unwrap();
        std::fs::write(bad.join(MANIFEST), "id = \"bad\"\n").unwrap();
        let (templates, problems) = discover(&[root.path().to_path_buf()]);
        let ids: Vec<&str> = templates.iter().map(|t| t.manifest.id.as_str()).collect();
        assert_eq!(ids, vec!["psg-article", "good"]);
        assert_eq!(problems.len(), 1);
    }
}
