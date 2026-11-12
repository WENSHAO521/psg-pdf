//! `cargo xtask changelog`: the PSG PDF Suite changelog, built from Conventional Commits.
//!
//! `CHANGELOG.md` is the one record. Each release adds a `## vX.Y.Z (YYYY-MM-DD)` section, newest
//! first, under the `<!-- changelog-base: <sha> -->` marker. The marker names the commit the suite
//! forked from: history before it belongs to PdfCraft and is never collected. A release collects
//! the commits since the newest stable tag (`vX.Y.Z`) reachable from `HEAD`, or since the marker.
//!
//! - `release [--version X.Y.Z]`: set the workspace version and write its section.
//! - `notes --version X.Y.Z`: print that version's section (the stable release notes).
//! - `notes --unreleased`: print what the next release would say (the beta release notes).
//! - `verify`: check `CHANGELOG.md` is well formed (CI).
//! - `lint [--strict] [<rev-range>]`: name commits whose subject is not a Conventional Commit.
//!
//! What a commit contributes:
//! - `feat`, `fix`, `perf` and `revert` subjects are collected; other types are left out unless
//!   they carry a `Changelog:` trailer. `Changelog: <text>` replaces the subject's wording, and
//!   `Changelog: skip` leaves the commit out.
//! - A `!` after the type, or a `BREAKING CHANGE:` footer, adds a breaking entry: the footer's
//!   text, else the subject's. The normal entry then appears only with a `Changelog:` trailer.
//! - Merge commits and `chore(release)` commits are never collected.

use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

use crate::{gates, version};

const FILE: &str = "CHANGELOG.md";
const BASE_MARKER: &str = "<!-- changelog-base: ";

/// Section kinds, in the order a section lists them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Breaking,
    Feature,
    Fix,
    Perf,
    Revert,
    Other,
}

impl Kind {
    fn title(self) -> &'static str {
        match self {
            Kind::Breaking => "Breaking Changes",
            Kind::Feature => "Features",
            Kind::Fix => "Bug Fixes",
            Kind::Perf => "Performance",
            Kind::Revert => "Reverts",
            Kind::Other => "Other Changes",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    kind: Kind,
    scope: Option<String>,
    text: String,
    sha: String,
}

/// The parts of a Conventional Commit subject: `type(scope)!: description`.
#[derive(Debug, PartialEq, Eq)]
struct Header<'a> {
    kind: &'a str,
    scope: Option<&'a str>,
    bang: bool,
    description: &'a str,
}

const TYPES: &[&str] = &["feat", "fix", "perf", "revert", "docs", "style", "refactor", "test", "build", "ci", "chore"];

fn header(subject: &str) -> Option<Header<'_>> {
    let (head, description) = subject.split_once(": ")?;
    let description = description.trim();
    let (head, bang) = match head.strip_suffix('!') {
        Some(h) => (h, true),
        None => (head, false),
    };
    let (kind, scope) = match head.split_once('(') {
        Some((k, rest)) => (k, Some(rest.strip_suffix(')')?)),
        None => (head, None),
    };
    let scope_ok = scope.is_none_or(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_/.".contains(c)));
    if !TYPES.contains(&kind) || !scope_ok || description.is_empty() {
        return None;
    }
    Some(Header { kind, scope, bang, description })
}

/// The value of the first `Name: value` trailer line in `body`.
fn trailer<'a>(body: &'a str, names: &[&str]) -> Option<&'a str> {
    body.lines().find_map(|l| names.iter().find_map(|n| l.strip_prefix(n).and_then(|r| r.strip_prefix(':')).map(str::trim)))
}

/// The entries one commit adds.
fn entries(subject: &str, body: &str, sha: &str) -> Vec<Entry> {
    let Some(h) = header(subject) else { return Vec::new() };
    if h.kind == "chore" && h.scope == Some("release") {
        return Vec::new();
    }
    let note = trailer(body, &["Changelog"]);
    if note.is_some_and(|n| n.eq_ignore_ascii_case("skip")) {
        return Vec::new();
    }
    let entry = |kind, text: &str| Entry { kind, scope: h.scope.map(str::to_string), text: text.to_string(), sha: sha.to_string() };
    let breaking = trailer(body, &["BREAKING CHANGE", "BREAKING-CHANGE"]).filter(|t| !t.is_empty());
    let mut out = Vec::new();
    if h.bang || breaking.is_some() {
        out.push(entry(Kind::Breaking, breaking.unwrap_or(h.description)));
        if let Some(n) = note.filter(|n| !n.is_empty()) {
            out.push(entry(kind_of(h.kind), n));
        }
        return out;
    }
    let kind = kind_of(h.kind);
    match note.filter(|n| !n.is_empty()) {
        Some(n) => out.push(entry(kind, n)),
        None if kind != Kind::Other => out.push(entry(kind, h.description)),
        None => {}
    }
    out
}

fn kind_of(t: &str) -> Kind {
    match t {
        "feat" => Kind::Feature,
        "fix" => Kind::Fix,
        "perf" => Kind::Perf,
        "revert" => Kind::Revert,
        _ => Kind::Other,
    }
}

/// The body of a section: its entries grouped by kind, in commit order within a kind.
fn render(entries: &[Entry]) -> String {
    if entries.is_empty() {
        return "- No user-facing changes.\n".into();
    }
    let mut kinds: Vec<Kind> = entries.iter().map(|e| e.kind).collect();
    kinds.sort();
    kinds.dedup();
    let mut o = String::new();
    for (i, kind) in kinds.into_iter().enumerate() {
        if i > 0 {
            o.push('\n');
        }
        let _ = writeln!(o, "**{}**\n", kind.title());
        for e in entries.iter().filter(|e| e.kind == kind) {
            let scope = e.scope.as_deref().map(|s| format!("**{s}** ")).unwrap_or_default();
            let _ = writeln!(o, "- {scope}{} ({})", e.text, e.sha);
        }
    }
    o
}

/// The commit `CHANGELOG.md`'s base marker names.
fn base(changelog: &str) -> Result<String, String> {
    let line = changelog.lines().find_map(|l| l.trim().strip_prefix(BASE_MARKER)).ok_or(format!("{FILE} has no `{BASE_MARKER}<sha> -->` line"))?;
    let sha = line.strip_suffix("-->").map(str::trim).unwrap_or_default();
    if sha.len() != 40 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("{FILE}: the changelog base must be a full commit hash, got `{sha}`"));
    }
    Ok(sha.to_string())
}

/// `X.Y.Z` when `v` is a stable version (no pre-release suffix).
fn is_stable(v: &str) -> bool {
    let parts: Vec<&str> = v.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Each `## vX.Y.Z (date)` heading: (line index, version, date).
fn sections(changelog: &str) -> Vec<(usize, &str, &str)> {
    changelog
        .lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let rest = l.strip_prefix("## v")?;
            let (v, date) = rest.split_once(' ').unwrap_or((rest, ""));
            Some((i, v, date))
        })
        .collect()
}

/// The text of `version`'s section, without its heading.
fn section(changelog: &str, version: &str) -> Option<String> {
    let lines: Vec<&str> = changelog.lines().collect();
    let start = sections(changelog).into_iter().find(|(_, v, _)| *v == version)?.0 + 1;
    let end = lines.iter().enumerate().skip(start).find(|(_, l)| l.starts_with("## ")).map_or(lines.len(), |(i, _)| i);
    let body = lines.get(start..end)?.join("\n");
    Some(format!("{}\n", body.trim()))
}

/// `changelog` with a new section for `version` above the newest one.
fn insert(changelog: &str, version: &str, date: &str, body: &str) -> Result<String, String> {
    if sections(changelog).iter().any(|(_, v, _)| *v == version) {
        return Err(format!("{FILE} already has a section for v{version}"));
    }
    let marker = changelog.find(BASE_MARKER).ok_or(format!("{FILE} has no changelog base marker"))?;
    let after = changelog.get(marker..).and_then(|s| s.find('\n')).map_or(changelog.len(), |i| marker + i + 1);
    let (head, tail) = (changelog.get(..after).unwrap_or(changelog), changelog.get(after..).unwrap_or(""));
    Ok(format!("{head}\n## v{version} ({date})\n\n{body}{}{}", if tail.trim().is_empty() { "" } else { "\n" }, tail.trim_start_matches('\n')))
}

/// Problems with `changelog`'s structure.
fn check(changelog: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if let Err(e) = base(changelog) {
        problems.push(e);
    }
    let mut previous: Option<Vec<u64>> = None;
    for (i, v, date) in sections(changelog) {
        let line = i + 1;
        if !is_stable(v) {
            problems.push(format!("line {line}: `v{v}` is not a stable version (vX.Y.Z)"));
            continue;
        }
        let ok_date =
            date.strip_prefix('(').and_then(|d| d.strip_suffix(')')).is_some_and(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok());
        if !ok_date {
            problems.push(format!("line {line}: v{v} needs a date, as `## v{v} (YYYY-MM-DD)`"));
        }
        let parts: Vec<u64> = v.split('.').filter_map(|p| p.parse().ok()).collect();
        if previous.as_ref().is_some_and(|p| *p <= parts) {
            problems.push(format!("line {line}: v{v} is not older than the section above it; newest comes first"));
        }
        previous = Some(parts);
    }
    problems
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git").current_dir(root).args(args).output().map_err(|e| format!("git: {e}"))?;
    if !out.status.success() {
        return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Where collection starts: the newest stable tag reachable from HEAD, else the base marker.
fn since(root: &Path, changelog: &str) -> Result<String, String> {
    let tags = git(root, &["tag", "--merged", "HEAD", "--list", "v*", "--sort=-v:refname"])?;
    match tags.lines().map(str::trim).find(|t| t.strip_prefix('v').is_some_and(is_stable)) {
        Some(t) => Ok(t.to_string()),
        None => base(changelog),
    }
}

/// (short sha, subject, body) of every non-merge commit in `range`, oldest first.
fn commits(root: &Path, range: &str) -> Result<Vec<(String, String, String)>, String> {
    let log = git(root, &["log", "--no-merges", "--reverse", "--format=%h%x1f%s%x1f%b%x1e", range])?;
    Ok(log
        .split('\x1e')
        .filter_map(|c| {
            let mut f = c.trim_start_matches('\n').splitn(3, '\x1f');
            Some((f.next()?.to_string(), f.next()?.to_string(), f.next().unwrap_or("").to_string()))
        })
        .collect())
}

fn unreleased(root: &Path, changelog: &str) -> Result<Vec<Entry>, String> {
    let range = format!("{}..HEAD", since(root, changelog)?);
    Ok(commits(root, &range)?.iter().flat_map(|(sha, s, b)| entries(s, b, sha)).collect())
}

fn read(root: &Path) -> Result<String, String> {
    std::fs::read_to_string(root.join(FILE)).map_err(|e| format!("{FILE}: {e}"))
}

/// The value after `flag` in `args`.
fn flag<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).map(String::as_str)
}

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let root = gates::root();
    let rest = args.get(1..).unwrap_or_default();
    let r = match args.first().map(String::as_str) {
        Some("release") => release(&root, flag(rest, "--version")),
        Some("notes") if rest.iter().any(|a| a == "--unreleased") => read(&root).and_then(|c| unreleased(&root, &c)).map(|e| print!("{}", render(&e))),
        Some("notes") => notes(&root, flag(rest, "--version")),
        Some("verify") => verify(&root),
        Some("lint") => lint(&root, rest),
        _ => Err("usage: cargo xtask changelog release [--version X.Y.Z] | notes (--version X.Y.Z | --unreleased) | verify | lint [--strict] [<rev-range>]".into()),
    };
    r.map_err(anyhow::Error::msg)
}

fn release(root: &Path, version: Option<&str>) -> Result<(), String> {
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).map_err(|e| format!("Cargo.toml: {e}"))?;
    let current = version::read(&manifest)?;
    let version = version.map_or(current.clone(), str::to_string);
    if !is_stable(&version) {
        return Err(format!("a release needs a stable version X.Y.Z, got {version}"));
    }
    if !git(root, &["tag", "--list", &format!("v{version}")])?.trim().is_empty() {
        return Err(format!("v{version} is already tagged; pass --version with the next one"));
    }
    if version != current {
        version::run(root, &["set", &version])?;
    }
    let changelog = read(root)?;
    let body = render(&unreleased(root, &changelog)?);
    let date = chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string();
    std::fs::write(root.join(FILE), insert(&changelog, &version, &date, &body)?).map_err(|e| format!("{FILE}: {e}"))?;
    println!("{FILE}: wrote v{version} ({date}). Review it, then commit:\n  git commit -am \"chore(release): v{version}\"");
    Ok(())
}

fn notes(root: &Path, version: Option<&str>) -> Result<(), String> {
    let version = match version {
        Some(v) => v.trim_start_matches('v').to_string(),
        None => version::read(&std::fs::read_to_string(root.join("Cargo.toml")).map_err(|e| format!("Cargo.toml: {e}"))?)?,
    };
    let text = section(&read(root)?, &version).ok_or(format!("{FILE} has no section for v{version}"))?;
    print!("{text}");
    Ok(())
}

fn verify(root: &Path) -> Result<(), String> {
    let changelog = read(root)?;
    let problems = check(&changelog);
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    println!("{FILE}: ok ({} releases)", sections(&changelog).len());
    Ok(())
}

fn lint(root: &Path, args: &[String]) -> Result<(), String> {
    let strict = args.iter().any(|a| a == "--strict");
    let range = match args.iter().find(|a| !a.starts_with("--")) {
        Some(r) => r.clone(),
        None => format!("{}..HEAD", since(root, &read(root)?)?),
    };
    let bad: Vec<String> = commits(root, &range)?
        .into_iter()
        .filter(|(_, s, _)| header(s).is_none() && !s.starts_with("fixup! ") && !s.starts_with("squash! ") && !s.starts_with("Revert \""))
        .map(|(sha, s, _)| format!("{sha} {s}"))
        .collect();
    let ci = std::env::var_os("GITHUB_ACTIONS").is_some();
    for b in &bad {
        if ci {
            println!("::warning title=Not a Conventional Commit::{b}");
        } else {
            println!("not a Conventional Commit: {b}");
        }
    }
    if strict && !bad.is_empty() {
        return Err(format!("{} commit(s) in {range} need a `type(scope): description` subject (docs/releasing.md)", bad.len()));
    }
    println!("changelog lint: {} commit(s) to fix in {range}", bad.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "# Changelog\n\nIntro.\n\n<!-- changelog-base: d2916decb3d948b7e404ee839d7542692bf18d92 -->\n";

    #[test]
    fn parses_conventional_subjects() {
        assert_eq!(
            header("feat(academic): BibLaTeX support"),
            Some(Header { kind: "feat", scope: Some("academic"), bang: false, description: "BibLaTeX support" })
        );
        assert_eq!(header("fix!: drop macOS 10").map(|h| h.bang), Some(true));
        assert_eq!(header("Package both apps"), None);
        assert_eq!(header("feature: nope"), None);
        assert_eq!(header("fix(): empty scope"), None);
        assert_eq!(header("fix(a b): space"), None);
    }

    #[test]
    fn collects_by_type_and_trailer() {
        assert_eq!(entries("feat(pdf): Stamp pages", "", "abc")[0].kind, Kind::Feature);
        assert!(entries("refactor: tidy", "", "abc").is_empty());
        let e = entries("ci: build the suite", "Changelog: Installers for every platform\n", "abc");
        assert_eq!((e[0].kind, e[0].text.as_str()), (Kind::Other, "Installers for every platform"));
        assert!(entries("fix: typo", "Changelog: skip", "abc").is_empty());
        assert!(entries("chore(release): v1.0.0", "", "abc").is_empty());
    }

    #[test]
    fn breaking_changes_take_the_footer() {
        let e = entries("feat(macos)!: require macOS 13", "BREAKING CHANGE: macOS 12 is no longer supported.\nChangelog: New look\n", "abc");
        assert_eq!(e.len(), 2);
        assert_eq!((e[0].kind, e[0].text.as_str()), (Kind::Breaking, "macOS 12 is no longer supported."));
        assert_eq!((e[1].kind, e[1].text.as_str()), (Kind::Feature, "New look"));
        let e = entries("fix!: change the default", "", "abc");
        assert_eq!((e.len(), e[0].kind), (1, Kind::Breaking));
    }

    #[test]
    fn renders_groups_in_order() {
        let mut e = entries("fix(ui): Close button", "", "111");
        e.extend(entries("feat: Templates", "", "222"));
        assert_eq!(render(&e), "**Features**\n\n- Templates (222)\n\n**Bug Fixes**\n\n- **ui** Close button (111)\n");
        assert_eq!(render(&[]), "- No user-facing changes.\n");
    }

    #[test]
    fn inserts_newest_first_and_reads_sections_back() {
        let one = insert(BASE, "1.0.0", "2026-11-12", "- A (1)\n").unwrap();
        let two = insert(&one, "1.0.1", "2026-11-13", "- B (2)\n").unwrap();
        assert!(two.find("## v1.0.1").unwrap() < two.find("## v1.0.0").unwrap());
        assert_eq!(section(&two, "1.0.0").as_deref(), Some("- A (1)\n"));
        assert_eq!(section(&two, "1.0.1").as_deref(), Some("- B (2)\n"));
        assert!(check(&two).is_empty(), "{:?}", check(&two));
        assert!(insert(&two, "1.0.1", "2026-11-13", "").is_err());
    }

    #[test]
    fn verify_names_each_problem() {
        assert!(check("# Changelog\n").iter().any(|p| p.contains("changelog-base")));
        let bad = format!("{BASE}\n## v1.0.0 (2026-11-12)\n\n- A\n\n## v1.0.1 (soon)\n\n- B\n\n## v2.0.0-beta.1 (2026-11-12)\n");
        let p = check(&bad);
        assert!(p.iter().any(|p| p.contains("v1.0.1 needs a date")), "{p:?}");
        assert!(p.iter().any(|p| p.contains("not older")), "{p:?}");
        assert!(p.iter().any(|p| p.contains("not a stable version")), "{p:?}");
    }
}
