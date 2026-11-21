//! `cargo xtask products`: the PSG products' identities (packaging/products.toml), and a check
//! that every installer source, metadata file and icon agrees with them.
//!
//! Each PSG product ships independently: its own installers, ids, install folders, file
//! associations and update channels. A copy-paste slip between them (an upgrade
//! code reused, a desktop file naming the other app's icon) would make one installer upgrade or
//! uninstall the other, so `check` runs in `cargo xtask ci`.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, bail};
use serde::Deserialize;

pub const MANIFEST: &str = "packaging/products.toml";

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub legacy_suite: LegacySuite,
    #[serde(rename = "product")]
    pub products: Vec<Product>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct LegacySuite {
    pub name: String,
    pub split_version: String,
    pub msi_upgrade_code: String,
    pub linux_package: String,
    pub dmg_stem: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct Product {
    pub id: String,
    pub name: String,
    pub tagline: String,
    pub cargo_packages: Vec<String>,
    pub binaries: Vec<String>,
    pub app_id: String,
    pub asset_stem: String,
    pub icon_dir: String,
    pub icon_name: String,
    pub accent: String,
    pub msi_upgrade_code: String,
    pub msi_source: String,
    pub install_policy: String,
    pub windows_folder: String,
    pub windows_registry_key: String,
    pub windows_progid: String,
    pub bundle_name: String,
    pub info_plist: String,
    pub dmg_volume: String,
    pub dmg_layout: String,
    pub dmg_background: String,
    pub linux_package: String,
    pub nfpm: String,
    pub desktop_file: String,
    pub file_extensions: Vec<String>,
    pub mime_types: Vec<String>,
    pub update_channels: Vec<String>,
}

/// The release workflow's platforms: (name, runner).
pub const PLATFORMS: &[(&str, &str)] = &[("linux-x86_64", "ubuntu-22.04"), ("macos-universal", "macos-latest"), ("windows-x64", "windows-latest")];

pub fn load(root: &Path) -> anyhow::Result<Manifest> {
    let path = root.join(MANIFEST);
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let root = crate::gates::root();
    let manifest = load(&root)?;
    match args.first().map(String::as_str) {
        Some("check") | None => {
            let problems = check(&root, &manifest);
            if problems.is_empty() {
                println!("products: {} products agree with {MANIFEST}", manifest.products.len());
                Ok(())
            } else {
                for p in &problems {
                    eprintln!("  {p}");
                }
                bail!("{} problem(s) with the product identities in {MANIFEST}", problems.len())
            }
        }
        Some("matrix") => {
            println!("{}", matrix(&manifest));
            Ok(())
        }
        Some("ids") => {
            println!("{}", ids(&manifest));
            Ok(())
        }
        Some("get") => {
            let (Some(id), Some(key)) = (args.get(1), args.get(2)) else {
                bail!("usage: cargo xtask products get <product id | legacy_suite> <key>")
            };
            println!("{}", get(&manifest, id, key)?);
            Ok(())
        }
        Some(other) => bail!("unknown `products` subcommand `{other}`; use check, matrix, ids or get"),
    }
}

/// `{"include":[{"platform":…,"os":…,"products":"psg-pdf"}, …]}` for
/// `strategy.matrix`: one build job per platform, each building every product in turn on one
/// shared Cargo target directory, so the engine they share is compiled once per platform. Each
/// product is still packaged, checked and uploaded on its own (`.github/workflows/build.yaml`).
pub fn matrix(m: &Manifest) -> String {
    let products = ids(m);
    let include: Vec<serde_json::Value> =
        PLATFORMS.iter().map(|(platform, os)| serde_json::json!({ "platform": platform, "os": os, "products": products })).collect();
    serde_json::json!({ "include": include }).to_string()
}

/// Every product id, in manifest order, separated by spaces.
pub fn ids(m: &Manifest) -> String {
    m.products.iter().map(|p| p.id.as_str()).collect::<Vec<_>>().join(" ")
}

/// One value of one product, or of `legacy_suite` (the old combined installer, which the release
/// workflow's migration test installs); lists are joined with spaces.
pub fn get(m: &Manifest, id: &str, key: &str) -> anyhow::Result<String> {
    let v = if id == "legacy_suite" {
        serde_json::to_value(&m.legacy_suite)?
    } else {
        let p = m.products.iter().find(|p| p.id == id).with_context(|| format!("no product `{id}` in {MANIFEST}"))?;
        serde_json::to_value(p)?
    };
    match v.get(key) {
        Some(serde_json::Value::String(s)) => Ok(s.clone()),
        Some(serde_json::Value::Array(a)) => Ok(a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" ")),
        _ => bail!("product `{id}` has no key `{key}`"),
    }
}

fn read(root: &Path, rel: &str, problems: &mut Vec<String>) -> Option<String> {
    match std::fs::read_to_string(root.join(rel)) {
        Ok(t) => Some(t),
        Err(e) => {
            problems.push(format!("{rel}: {e}"));
            None
        }
    }
}

fn need(problems: &mut Vec<String>, file: &str, text: &str, wanted: &str, why: &str) {
    if !text.contains(wanted) {
        problems.push(format!("{file}: expected `{wanted}` ({why})"));
    }
}

fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.split(['-', '+']).next()?;
    let mut it = core.split('.').map(|n| n.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

/// Every disagreement between the manifest and the files it names.
pub fn check(root: &Path, m: &Manifest) -> Vec<String> {
    let mut problems = Vec::new();
    check_unique(m, &mut problems);
    let legacy = &m.legacy_suite;

    // The workspace version is the products' version; the split happened at split_version.
    match (
        std::fs::read_to_string(root.join("Cargo.toml")).ok().and_then(|t| crate::version::read(&t).ok()).as_deref().and_then(parse_version),
        parse_version(&legacy.split_version),
    ) {
        (Some(v), Some(split)) if v < split => problems.push(format!(
            "Cargo.toml version is below legacy_suite.split_version {}: the independent installers must sort above every suite release",
            legacy.split_version
        )),
        (Some(_), Some(_)) => {}
        _ => problems.push("cannot read the workspace version or legacy_suite.split_version".into()),
    }

    let continuing: Vec<&Product> = m.products.iter().filter(|p| p.msi_upgrade_code == legacy.msi_upgrade_code).collect();
    if continuing.len() != 1 {
        problems.push(format!(
            "exactly one product must continue the suite's MSI UpgradeCode {}, found {}",
            legacy.msi_upgrade_code,
            continuing.len()
        ));
    }
    if !m.products.iter().any(|p| p.linux_package == legacy.linux_package) {
        problems.push(format!("one product must continue the suite's Linux package `{}`", legacy.linux_package));
    }

    for p in &m.products {
        check_product(root, m, p, &mut problems);
    }
    problems
}

fn check_unique(m: &Manifest, problems: &mut Vec<String>) {
    type Field = fn(&Product) -> &str;
    let fields: &[(&str, Field)] = &[
        ("id", |p| &p.id),
        ("name", |p| &p.name),
        ("app_id", |p| &p.app_id),
        ("asset_stem", |p| &p.asset_stem),
        ("msi_upgrade_code", |p| &p.msi_upgrade_code),
        ("windows_folder", |p| &p.windows_folder),
        ("windows_registry_key", |p| &p.windows_registry_key),
        ("windows_progid", |p| &p.windows_progid),
        ("bundle_name", |p| &p.bundle_name),
        ("dmg_volume", |p| &p.dmg_volume),
        ("linux_package", |p| &p.linux_package),
        ("msi_source", |p| &p.msi_source),
        ("nfpm", |p| &p.nfpm),
        ("desktop_file", |p| &p.desktop_file),
        ("icon_dir", |p| &p.icon_dir),
    ];
    for (name, f) in fields {
        let mut seen = HashSet::new();
        for p in &m.products {
            if !seen.insert(f(p).to_ascii_lowercase()) {
                problems.push(format!("two products share {name} `{}`; each product needs its own", f(p)));
            }
        }
    }
    let mut bins = HashSet::new();
    for p in &m.products {
        for b in &p.binaries {
            if !bins.insert(b.clone()) {
                problems.push(format!("binary `{b}` is in two products; each file must belong to one installer"));
            }
        }
    }
    // An asset stem must not be a prefix of another's followed by `-`, or the updater of one
    // could take the other's files (`psg-pdf` vs `psg-pdf-suite`).
    for a in &m.products {
        for b in &m.products {
            if a.id != b.id
                && b.asset_stem.starts_with(&format!("{}-", a.asset_stem))
                && parse_version(&b.asset_stem[a.asset_stem.len() + 1..]).is_some()
            {
                problems.push(format!("asset stems `{}` and `{}` overlap", a.asset_stem, b.asset_stem));
            }
        }
    }
}

fn check_product(root: &Path, m: &Manifest, p: &Product, problems: &mut Vec<String>) {
    for pkg in &p.cargo_packages {
        let dir = root.join("apps").join(pkg);
        if !dir.join("Cargo.toml").is_file() {
            problems.push(format!("{}: cargo package `{pkg}` has no apps/{pkg}/Cargo.toml", p.id));
        }
    }
    if p.binaries.first() != Some(&p.id) {
        problems.push(format!("{}: the first binary must be the app itself (`{}`)", p.id, p.id));
    }

    check_windows(root, m, p, problems);
    check_macos(root, p, problems);
    check_linux(root, m, p, problems);

    // Icons, generated from the brand master (packaging/psg-icons.py).
    for f in [
        format!("{}/{}.svg", p.icon_dir, p.icon_name),
        format!("{}/{}.ico", p.icon_dir, p.icon_name),
        format!("{}/{}.icns", p.icon_dir, p.icon_name),
        format!("{}/{}-256.png", p.icon_dir, p.icon_name),
        format!("{}/{}-1024.png", p.icon_dir, p.icon_name),
        format!("{}/hicolor/128x128/apps/{}.png", p.icon_dir, p.app_id),
        format!("{}/hicolor/scalable/apps/{}.svg", p.icon_dir, p.app_id),
    ] {
        if !root.join(&f).is_file() {
            problems.push(format!("{}: icon {f} is missing (python3 packaging/psg-icons.py)", p.id));
        }
    }
    if let Some(master) = read(root, "assets/brand/psg-logo-master.svg", problems) {
        need(problems, "assets/brand/psg-logo-master.svg", &master, &format!("<g id=\"{}\"", p.id), "the product's icon in the master");
    }

    // The in-app updater only takes this product's assets from the shared releases.
    let updater = format!("App::new(\"{}\", ", p.asset_stem);
    let sources = [format!("apps/{}/src", p.id), "crates/ui-egui/src".to_owned()];
    let found = sources.iter().any(|dir| rs_contains(&root.join(dir), &updater));
    if !found {
        problems.push(format!("{}: no craft-update App with id `{}` (the product's update channel)", p.id, p.asset_stem));
    }
    for ch in &p.update_channels {
        if ch != "stable" && ch != "beta" {
            problems.push(format!("{}: unknown update channel `{ch}` (stable, beta)", p.id));
        }
    }
}

/// The MSI source and its install policy.
fn check_windows(root: &Path, m: &Manifest, p: &Product, problems: &mut Vec<String>) {
    let legacy = &m.legacy_suite;
    let continues_suite = p.msi_upgrade_code == legacy.msi_upgrade_code;
    // Windows installer source.
    if let Some(wxs) = read(root, &p.msi_source, problems) {
        need(problems, &p.msi_source, &wxs, &format!("UpgradeCode=\"{}\"", p.msi_upgrade_code), "the product's own upgrade identity");
        need(problems, &p.msi_source, &wxs, &format!("<Package Name=\"{}\"", p.name), "Add/Remove Programs name");
        need(problems, &p.msi_source, &wxs, &format!("<?define ProductFolder=\"{}\" ?>", p.windows_folder), "install folder");
        need(problems, &p.msi_source, &wxs, &format!("<?define ProductRegKey=\"{}\" ?>", p.windows_registry_key), "remembered install folder");
        need(problems, &p.msi_source, &wxs, &format!("<?define ProductExe=\"{}.exe\" ?>", p.id), "App Paths and legacy folder search");
        need(problems, &p.msi_source, &wxs, &format!("Software\\Classes\\{}", p.windows_progid), "file association ProgID");
        for ext in &p.file_extensions {
            need(problems, &p.msi_source, &wxs, &format!("Software\\Classes\\.{ext}\\OpenWithProgids"), "Open with registration");
        }
        for other in m.products.iter().filter(|o| o.id != p.id) {
            for b in &other.binaries {
                if wxs.contains(&format!("{b}.exe")) {
                    problems.push(format!("{}: installs `{b}.exe`, which belongs to {}", p.msi_source, other.name));
                }
            }
            if wxs.contains(&format!("UpgradeCode=\"{}\"", other.msi_upgrade_code)) {
                problems.push(format!("{}: uses {}'s UpgradeCode", p.msi_source, other.name));
            }
        }
        if !continues_suite {
            // Detect, never remove, the old suite: it carries the other product too.
            need(problems, &p.msi_source, &wxs, &format!("<Upgrade Id=\"{}\">", legacy.msi_upgrade_code), "suite detection");
            need(problems, &p.msi_source, &wxs, "OnlyDetect=\"yes\" Property=\"PSGSUITEFOUND\"", "the suite is only detected");
            need(problems, &p.msi_source, &wxs, &format!("Maximum=\"{}\"", legacy.split_version), "only suite releases are detected");
        }
    }
    if let Some(policy) = read(root, &p.install_policy, problems) {
        match serde_json::from_str::<serde_json::Value>(&policy) {
            Ok(v) => {
                if v["name"] != p.windows_folder.as_str() {
                    problems.push(format!("{}: name must be `{}` (the install folder)", p.install_policy, p.windows_folder));
                }
                if v["upgradeCode"] != p.msi_upgrade_code.as_str() {
                    problems.push(format!("{}: upgradeCode must be {}", p.install_policy, p.msi_upgrade_code));
                }
                if v["icon"] != format!("{}/{}.ico", p.icon_dir, p.icon_name).as_str() {
                    problems.push(format!("{}: icon must be {}/{}.ico", p.install_policy, p.icon_dir, p.icon_name));
                }
            }
            Err(e) => problems.push(format!("{}: {e}", p.install_policy)),
        }
    }
}

/// The bundle's Info.plist and the DMG window.
fn check_macos(root: &Path, p: &Product, problems: &mut Vec<String>) {
    // macOS bundle.
    if let Some(plist) = read(root, &p.info_plist, problems) {
        need(problems, &p.info_plist, &plist, &format!("<string>{}</string>", p.app_id), "bundle id");
        need(problems, &p.info_plist, &plist, &format!("<string>{}</string>", p.bundle_name), "bundle name");
        for ext in &p.file_extensions {
            need(problems, &p.info_plist, &plist, &format!("<string>{ext}</string>"), "document type");
        }
    }
    for f in [&p.dmg_layout, &p.dmg_background] {
        if !root.join(f).is_file() {
            problems.push(format!("{}: {f} is missing (python3 packaging/macos/dmg/generate.py)", p.id));
        }
    }
}

/// The deb/rpm configuration and the desktop entry.
fn check_linux(root: &Path, m: &Manifest, p: &Product, problems: &mut Vec<String>) {
    let legacy = &m.legacy_suite;
    // Linux packages.
    if let Some(nfpm) = read(root, &p.nfpm, problems) {
        need(problems, &p.nfpm, &nfpm, &format!("\nname: {}\n", p.linux_package), "package name");
        for b in &p.binaries {
            need(problems, &p.nfpm, &nfpm, &format!("dst: /usr/bin/{b}\n"), "every binary of the product");
        }
        for other in m.products.iter().filter(|o| o.id != p.id) {
            for b in &other.binaries {
                if nfpm.contains(&format!("dst: /usr/bin/{b}\n")) {
                    problems.push(format!("{}: ships /usr/bin/{b}, which belongs to {}", p.nfpm, other.name));
                }
            }
        }
        if p.linux_package != legacy.linux_package {
            let replaces = format!("{} (<< {}~)", legacy.linux_package, legacy.split_version);
            need(problems, &p.nfpm, &nfpm, &replaces, "takes over the suite's copy of its files (deb)");
            let conflicts = format!("{} < {}~", legacy.linux_package, legacy.split_version);
            need(problems, &p.nfpm, &nfpm, &conflicts, "never installs beside a suite (rpm)");
        }
    }
    if let Some(desktop) = read(root, &p.desktop_file, problems) {
        need(problems, &p.desktop_file, &desktop, &format!("\nName={}\n", p.name), "menu name");
        need(problems, &p.desktop_file, &desktop, &format!("\nIcon={}\n", p.app_id), "icon");
        need(problems, &p.desktop_file, &desktop, &format!("\nStartupWMClass={}\n", p.app_id), "window grouping");
        need(problems, &p.desktop_file, &desktop, &format!("\nExec={} ", p.id), "launch command");
        for mime in &p.mime_types {
            need(problems, &p.desktop_file, &desktop, &format!("{mime};"), "file association");
        }
    }
}

fn rs_contains(dir: &Path, needle: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    entries.flatten().any(|e| {
        let path = e.path();
        if path.is_dir() {
            rs_contains(&path, needle)
        } else {
            path.extension().is_some_and(|x| x == "rs") && std::fs::read_to_string(&path).is_ok_and(|t| t.contains(needle))
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    #[test]
    fn the_repository_agrees_with_the_manifest() {
        let root = repo();
        let m = load(&root).unwrap();
        let problems = check(&root, &m);
        assert!(problems.is_empty(), "{problems:#?}");
    }

    #[test]
    fn each_desktop_app_enables_the_system_clipboard_for_standalone_builds() {
        let root = repo();
        for product in load(&root).unwrap().products {
            let manifest = root.join("apps").join(&product.id).join("Cargo.toml");
            let manifest: toml::Value = toml::from_str(&std::fs::read_to_string(manifest).unwrap()).unwrap();
            let features = manifest["dependencies"]["eframe"]["features"].as_array().unwrap();
            // A workspace build can get this feature from the other app through feature unification.
            assert!(features.iter().any(|feature| feature.as_str() == Some("clipboard")), "{} must enable eframe's system clipboard", product.id);
        }
    }

    /// The manifest with a second product: a copy of the first under its own id, sharing the
    /// first one's identities until a test changes them.
    fn with_a_second_product(root: &Path) -> Manifest {
        let mut m = load(root).unwrap();
        let mut second = m.products[0].clone();
        second.id = "psg-second".into();
        m.products.push(second);
        m
    }

    #[test]
    fn a_shared_upgrade_code_is_caught() {
        let root = repo();
        let m = with_a_second_product(&root);
        let problems = check(&root, &m);
        assert!(problems.iter().any(|p| p.contains("share msi_upgrade_code")), "{problems:#?}");
    }

    #[test]
    fn a_shared_package_name_is_caught() {
        let root = repo();
        let m = with_a_second_product(&root);
        assert!(check(&root, &m).iter().any(|p| p.contains("share linux_package")));
    }

    #[test]
    fn the_matrix_builds_every_product_on_every_platform() {
        let m = load(&repo()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&matrix(&m)).unwrap();
        let include = v["include"].as_array().unwrap();
        assert_eq!(include.len(), PLATFORMS.len());
        for leg in include {
            assert_eq!(leg["products"].as_str().unwrap(), "psg-pdf");
        }
        assert_eq!(ids(&m), "psg-pdf");
        assert_eq!(get(&m, "psg-pdf", "binaries").unwrap(), "psg-pdf pdfcraft-cli");
        // PSG Academic PDF is retired: no longer built or released.
        assert!(get(&m, "psg-academic-pdf", "binaries").is_err());
        assert!(get(&m, "psg-pdf", "nope").is_err());
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(parse_version("0.19.0").unwrap() > parse_version("0.18.12-beta.3").unwrap());
        assert!(parse_version("x").is_none());
    }
}
