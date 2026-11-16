#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::check::{Glossary, Key, contains_word, report, source_keys};
use crate::locale::first_match;

#[test]
fn ten_languages_english_first() {
    assert_eq!(LANGUAGES.len(), 10);
    assert_eq!(ENGLISH.code, "en");
    let codes: Vec<&str> = LANGUAGES.iter().map(|l| l.code).collect();
    assert_eq!(codes, ["en", "zh-hans", "zh-hant", "ko", "ja", "de", "fr", "es", "pt-br", "vi"]);
    assert_eq!(translated().count(), 9);
    assert_eq!(language("ZH-Hant").map(|l| l.native_name), Some("繁體中文"));
    assert!(language("ru").is_none());
}

#[test]
fn system_locales_pick_a_language() {
    let code = |t: &str| for_locale(t).map(|l| l.code);
    assert_eq!(code("zh-CN"), Some("zh-hans"));
    assert_eq!(code("zh_SG.UTF-8"), Some("zh-hans"));
    assert_eq!(code("zh-Hans-HK"), Some("zh-hans"));
    assert_eq!(code("zh"), Some("zh-hans"));
    assert_eq!(code("zh_TW.UTF-8"), Some("zh-hant"));
    assert_eq!(code("zh-HK"), Some("zh-hant"));
    assert_eq!(code("zh-MO"), Some("zh-hant"));
    assert_eq!(code("zh-Hant"), Some("zh-hant"));
    assert_eq!(code("ko_KR.UTF-8"), Some("ko"));
    assert_eq!(code("ja-JP"), Some("ja"));
    assert_eq!(code("de_AT.UTF-8@euro"), Some("de"));
    assert_eq!(code("de-CH"), Some("de"));
    assert_eq!(code("fr_CA"), Some("fr"));
    assert_eq!(code("es-419"), Some("es"));
    assert_eq!(code("pt_PT"), Some("pt-br"));
    assert_eq!(code("pt-BR"), Some("pt-br"));
    assert_eq!(code("vi_VN"), Some("vi"));
    assert_eq!(code("en-GB"), Some("en"));
    assert_eq!(code("C"), None);
    assert_eq!(code("POSIX"), None);
    assert_eq!(code("C.UTF-8"), None);
    assert_eq!(code("ru-RU"), None);
    assert_eq!(code(""), None);
    // The first preferred language we have wins; none means English.
    assert_eq!(first_match(["ru-RU".to_string(), "fr-FR".to_string(), "de".to_string()]).code, "fr");
    assert_eq!(first_match(["C".to_string()]).code, "en");
    assert_eq!(first_match(Vec::<String>::new()).code, "en");
}

#[test]
fn preference_overrides_the_system() {
    let ko = || language("ko").unwrap();
    assert_eq!(Preference::parse("auto"), Preference::Auto);
    assert_eq!(Preference::parse("").resolve_with(ko).code, "ko");
    assert_eq!(Preference::parse("klingon"), Preference::Auto);
    assert_eq!(Preference::parse(" VI ").resolve_with(ko).code, "vi");
    assert_eq!(Preference::parse("pt-BR").as_str(), "pt-br");
    assert_eq!(Preference::Auto.as_str(), "auto");
    for l in &LANGUAGES {
        assert_eq!(Preference::parse(Preference::Fixed(l).as_str()), Preference::Fixed(l));
    }
}

#[test]
fn plural_rules() {
    for (code, zero, one, two) in [
        ("en", Plural::Other, Plural::One, Plural::Other),
        ("de", Plural::Other, Plural::One, Plural::Other),
        ("es", Plural::Other, Plural::One, Plural::Other),
        ("fr", Plural::One, Plural::One, Plural::Other),
        ("pt-br", Plural::One, Plural::One, Plural::Other),
        ("zh-hans", Plural::Other, Plural::Other, Plural::Other),
        ("zh-hant", Plural::Other, Plural::Other, Plural::Other),
        ("ja", Plural::Other, Plural::Other, Plural::Other),
        ("ko", Plural::Other, Plural::Other, Plural::Other),
        ("vi", Plural::Other, Plural::Other, Plural::Other),
    ] {
        assert_eq!((plural(code, 0), plural(code, 1), plural(code, 2)), (zero, one, two), "{code}");
        assert_eq!(plural(code, 1_000_001), Plural::Other, "{code}");
    }
}

#[test]
fn fill_is_one_pass() {
    assert_eq!(fill("Opened {path}.", &[("path", "/x/{path}")]), "Opened /x/{path}.");
    assert_eq!(fill("{b} then {a}", &[("a", "1"), ("b", "2")]), "2 then 1");
    assert_eq!(fill("Use \\textbf{x} and {missing}", &[]), "Use \\textbf{x} and {missing}");
    assert_eq!(fill("unclosed {brace", &[("brace", "x")]), "unclosed {brace");
    assert_eq!(placeholders("{n} of {total} in \\emph{Title}"), ["n", "total"]);
}

const FR: &str = "# comment\n\tCompile\tCompiler\n\tOpened {path}.\tOuvert : {path}.\nmenu\tFile\tFichier\n@one\t{n} error\t{n} erreur\n@other\t{n} error\t{n} erreurs\n";
const ZH: &str = "\tCompile\t编译\n@other\t{n} error\t{n} 个错误\n";

#[test]
fn bundle_lookup_fallback_and_switching() {
    let (bundle, problems) = Bundle::new(&[("fr", FR), ("zh-hans", ZH)]);
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(bundle.t("Compile"), "Compile");
    bundle.set_language(language("fr").unwrap());
    assert_eq!(bundle.language().code, "fr");
    assert_eq!(bundle.t("Compile"), "Compiler");
    assert_eq!(bundle.tf("Opened {path}.", &[("path", "a.tex")]), "Ouvert : a.tex.");
    assert_eq!(bundle.tc("menu", "File"), "Fichier");
    // A context without its own row uses the plain row; a missing row is English.
    assert_eq!(bundle.tc("button", "Compile"), "Compiler");
    assert_eq!(bundle.t("Not translated"), "Not translated");
    assert_eq!(bundle.tn(0, "{n} error", "{n} errors", &[]), "0 erreur");
    assert_eq!(bundle.tn(1, "{n} error", "{n} errors", &[]), "1 erreur");
    assert_eq!(bundle.tn(2, "{n} error", "{n} errors", &[]), "2 erreurs");
    bundle.set_language(language("zh-hans").unwrap());
    assert_eq!(bundle.tn(1, "{n} error", "{n} errors", &[]), "1 个错误");
    // Traditional Chinese has no catalog here: English, never Simplified.
    bundle.set_language(language("zh-hant").unwrap());
    assert_eq!(bundle.t("Compile"), "Compile");
    assert_eq!(bundle.tn(1, "{n} error", "{n} errors", &[]), "1 error");
    assert_eq!(bundle.tn(3, "{n} error", "{n} errors", &[]), "3 errors");
}

#[test]
fn bad_rows_are_reported_and_skipped() {
    let text = "\tA\n\tB\tb\textra\n\tC\t\n\t{x} D\td\n\tE\te\n\tE\te2\n@few\tF\tf\n\tG\tg\n";
    let (catalog, problems) = Catalog::parse("de", text);
    let lines: Vec<usize> = problems.iter().map(|p| p.line).collect();
    assert_eq!(lines, [1, 2, 3, 4, 6, 7], "{problems:?}");
    assert_eq!(catalog.rows().len(), 2);
    assert_eq!(catalog.get("", "E"), Some("e"));
    assert!(problems[0].to_string().starts_with("de.tsv:1:"));
    let (_, problems) = Bundle::new(&[("ru", "")]);
    assert_eq!(problems.len(), 1);
}

#[test]
fn source_keys_finds_every_lookup() {
    let src = r#"
        ui.label(t("Compile"));
        let s = tf("Opened {path}.", &[("path", &p)]);
        tc("menu", "File");
        i18n::tn(self.items.len() as u64, "{n} error", "{n} errors", &[]);
        tn(f(a, b), "{n} file",
            "{n} files", &[]);
        my_t("not a key");
        t(VARIABLE);
        t("Line \"quoted\" and \\ backslash");
        t("long \
           line");
        t("\u{2026}");
        psg_i18n::ui_text("Dismiss");
    "#;
    let keys = source_keys(src);
    let shown: Vec<(String, String, Option<String>)> = keys.into_iter().map(|k| (k.context, k.english, k.other)).collect();
    let k = |c: &str, e: &str| (c.to_owned(), e.to_owned(), None);
    let mut expected = vec![
        k("", "Compile"),
        k("", "Opened {path}."),
        k("menu", "File"),
        (String::new(), "{n} error".into(), Some("{n} errors".into())),
        (String::new(), "{n} file".into(), Some("{n} files".into())),
        k("", "Line \"quoted\" and \\ backslash"),
        k("", "long line"),
        k("", "…"),
        k("", "Dismiss"),
    ];
    expected.sort();
    assert_eq!(shown, expected);
}

const GLOSSARY: &str = "en\tfr\tzh-hans\tde\n# names\nLaTeX\t=\t=\t=\nbibliography\tbibliographie\t参考文献\tLiteraturverzeichnis|Bibliografie\ntemplate\tmodèle\t模板\t\n";

#[test]
fn glossary_terms_and_names() {
    let g = Glossary::parse(GLOSSARY).unwrap();
    assert_eq!(g.terms().collect::<Vec<_>>(), ["LaTeX", "bibliography", "template"]);
    assert!(g.violations("fr", "Add a bibliography", "Ajouter une bibliographie").is_empty());
    assert_eq!(g.violations("fr", "Add a bibliography", "Ajouter des références").len(), 1);
    assert_eq!(g.violations("fr", "Compile the LaTeX source", "Compiler la source Latex").len(), 1);
    // A name is kept as this sentence writes it (the `latex` command in lowercase).
    assert!(g.violations("fr", "Run latex twice", "Lancez latex deux fois").is_empty());
    assert!(g.violations("zh-hans", "Templates", "模板").is_empty());
    assert!(g.violations("de", "Bibliography file", "Literaturverzeichnisdatei").is_empty());
    assert!(g.violations("de", "Bibliography", "Bibliografie").is_empty());
    // An empty cell sets no rule; a language without a column has none either.
    assert!(g.violations("de", "Template", "Vorlage").is_empty());
    assert!(g.violations("ja", "Template", "x").is_empty());
    // A placeholder named like a term is a value, not the term.
    let g2 = Glossary::parse("en\tzh-hans\nerror\t错误\n").unwrap();
    assert!(g2.violations("zh-hans", "Could not save: {error}", "无法保存：{error}").is_empty());
    assert_eq!(g2.violations("zh-hans", "An error: {error}", "无法保存：{error}").len(), 1);
    assert!(Glossary::parse("fr\ten\n").is_err());
    assert!(Glossary::parse("en\tfr\nterm\n").is_err());
}

#[test]
fn whole_words() {
    assert!(contains_word("Open the template", "template"));
    assert!(contains_word("Templates", "template"));
    assert!(contains_word("Boxes", "box"));
    assert!(!contains_word("Templated", "template"));
    assert!(!contains_word("cite", "it"));
    assert!(contains_word("Check the DOI.", "DOI"));
    assert!(contains_word("cover letter", "cover letter"));
}

#[test]
fn reports_missing_stale_and_terminology() {
    let (catalog, _) =
        Catalog::parse("fr", "\tCompile\tCompiler\n\tOld\tVieux\n@one\t{n} error\t{n} erreur\n\tEdit the bibliography\tModifier les références\n");
    let keys = vec![
        Key { context: String::new(), english: "Compile".into(), other: None },
        Key { context: String::new(), english: "Missing".into(), other: None },
        Key { context: String::new(), english: "{n} error".into(), other: Some("{n} errors".into()) },
        Key { context: String::new(), english: "Edit the bibliography".into(), other: None },
    ];
    let g = Glossary::parse(GLOSSARY).unwrap();
    let r = report(&catalog, &keys, &g);
    assert_eq!(r.missing, [" / Missing", "@other / {n} error"]);
    assert_eq!(r.stale, ["line 2: Old"]);
    assert_eq!(r.terminology.len(), 1);
    assert!(!r.is_clean());
    // A one-form language needs only `@other`.
    let (zh, _) = Catalog::parse("zh-hans", "@other\t{n} error\t{n} 个错误\n");
    let r = report(&zh, &keys[2..3], &g);
    assert!(r.is_clean(), "{r}");
}

#[test]
fn ui_text_uses_the_installed_translator() {
    // Before any translator: unchanged. (Tests in this binary share the global, so this test
    // installs, checks and reinstalls in one place.)
    fn upper(s: &str) -> String {
        s.to_uppercase()
    }
    install_ui_translator(upper);
    assert_eq!(ui_text("Dismiss"), "DISMISS");
}

/// Each updater string is still one the vendored craft-update asks to translate.
#[test]
fn updater_strings_follow_the_vendored_crate() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vendor/craft-update/src");
    let mut source = String::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        source.push_str(&std::fs::read_to_string(entry.path()).unwrap());
    }
    for text in updater::TEXT {
        assert!(source.contains(&format!("{text:?}")), "{text:?} is no longer in craft-update");
    }
    let keys = source_keys(include_str!("updater.rs"));
    assert_eq!(keys.len(), updater::TEXT.len());
}
