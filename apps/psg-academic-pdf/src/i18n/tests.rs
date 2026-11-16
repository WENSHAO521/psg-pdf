#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use psg_i18n::check::{Glossary, Key, report, source_keys};

use super::*;

/// Every string the app and the shared widgets look up, and the sentences the typesetting core
/// marks for the interface (`psg_academic::mark`), from their source.
fn keys() -> Vec<Key> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut keys = Vec::new();
    for dir in [root.join("src"), root.join("../../crates/desktop-ui/src"), root.join("../../crates/academic/src")] {
        let mut stack = vec![dir];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("tests.rs") && !path.ends_with("i18n.rs") {
                    keys.extend(source_keys(&std::fs::read_to_string(&path).unwrap()));
                }
            }
        }
    }
    keys.extend(source_keys(include_str!("../../../../crates/i18n/src/updater.rs")));
    keys.sort();
    keys.dedup();
    keys
}

fn glossary() -> Glossary {
    Glossary::parse(include_str!("../../../../i18n/glossary.tsv")).unwrap()
}

/// Each of the nine translated languages has a catalog, and every row of it is usable.
#[test]
fn every_language_has_a_well_formed_catalog() {
    let codes: Vec<&str> = CATALOGS.iter().map(|(c, _)| *c).collect();
    let expected: Vec<&str> = psg_i18n::translated().map(|l| l.code).collect();
    assert_eq!(codes, expected);
    let (_, problems) = Bundle::new(&CATALOGS);
    let shown: Vec<String> = problems.iter().map(ToString::to_string).collect();
    assert!(shown.is_empty(), "{}", shown.join("\n"));
}

/// No language is shipped with gaps: every string the interface uses is translated (with each
/// plural form the language needs), no row is left over, and the academic glossary and the
/// protected names (LaTeX, BibTeX, DOI, the product names) are followed.
#[test]
fn every_language_is_complete_and_uses_the_glossary() {
    let keys = keys();
    assert!(keys.len() > 300, "only {} keys found: is the source scan broken?", keys.len());
    let glossary = glossary();
    let (bundle, _) = Bundle::new(&CATALOGS);
    let mut failures = String::new();
    for lang in psg_i18n::translated() {
        let catalog = bundle.catalog(lang.code).unwrap();
        let r = report(catalog, &keys, &glossary);
        if !r.is_clean() {
            failures.push_str(&r.to_string());
        }
    }
    assert!(failures.is_empty(), "{failures}");
}

/// The glossary has a column for every translated language.
#[test]
fn glossary_covers_every_language() {
    let g = glossary();
    let expected: Vec<&str> = psg_i18n::translated().map(|l| l.code).collect();
    assert_eq!(g.codes(), expected.as_slice());
    assert!(g.terms().count() >= 30);
}

/// A sentence is never left in English. One word may read the same (German "Editor", French
/// "Volume", Spanish "Error"), but a phrase copied over is an untranslated row, and in Chinese,
/// Japanese and Korean any English word is, except the names listed here.
#[test]
fn translations_are_not_left_in_english() {
    const SAME_EVERYWHERE: &[&str] = &["Ctrl", "Shift", "ORCID iD", "Word (.docx)…", "Engine: pdfLaTeX", "pdfLaTeX", "DOI", "ISSN", "PDF", "OK"];
    let (bundle, _) = Bundle::new(&CATALOGS);
    let mut same = Vec::new();
    for lang in psg_i18n::translated() {
        let cjk = matches!(lang.code, "zh-hans" | "zh-hant" | "ja" | "ko");
        for row in bundle.catalog(lang.code).unwrap().rows() {
            // Words, not placeholders or punctuation: "{n} page" is one word.
            let english: String = row.english.split('{').map(|part| part.split_once('}').map_or(part, |(_, rest)| rest)).collect();
            let phrase = english.split_whitespace().filter(|w| w.chars().any(char::is_alphabetic)).count() > 1;
            let words = row.english.chars().filter(|c| c.is_alphabetic()).count() > 2;
            if row.translation == row.english && (phrase || cjk) && words && !SAME_EVERYWHERE.contains(&row.english) {
                same.push(format!("{}:{} {}", lang.code, row.line, row.english));
            }
        }
    }
    assert!(same.is_empty(), "rows copied from English:\n{}", same.join("\n"));
}

/// Switching the language changes the interface at once, plural forms follow each language's
/// rule, and placeholders are filled without reading inserted text again.
#[test]
fn switching_language_and_plurals() {
    set_language(psg_i18n::language("zh-hans").unwrap());
    assert_eq!(t("Cancel"), "取消");
    assert_eq!(tf("Opened {path}.", &[("path", "/x/{y}")]), "已打开 /x/{y}。");
    assert_eq!(psg_i18n::ui_text("Dismiss"), "关闭");
    set_language(psg_i18n::language("fr").unwrap());
    assert_eq!(t("Cancel"), "Annuler");
    assert_eq!(tn(0, "{n} error", "{n} errors", &[]), "0 erreur");
    assert_eq!(tn(2, "{n} error", "{n} errors", &[]), "2 erreurs");
    set_language(psg_i18n::ENGLISH);
    assert_eq!(t("Cancel"), "Cancel");
    assert_eq!(tn(1, "{n} error", "{n} errors", &[]), "1 error");
    assert_eq!(tn(5, "{n} error", "{n} errors", &[]), "5 errors");
    apply(psg_i18n::Preference::parse("not-a-language"));
    assert!(psg_i18n::language(current()).is_some());
}
