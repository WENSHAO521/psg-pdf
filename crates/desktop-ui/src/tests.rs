use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use psg_brand::{Product, Tone};

use crate::notice::Notices;
use crate::palette::Palette;
use crate::{keys, layout, style, widgets};

#[test]
fn each_product_installs_its_own_accent_for_both_themes() {
    for product in Product::ALL {
        let ctx = egui::Context::default();
        style::install(&ctx, product);
        assert_eq!(style::product(&ctx), product);
        for (theme, dark) in [(egui::Theme::Light, false), (egui::Theme::Dark, true)] {
            let s = ctx.style_of(theme);
            let p = Palette::new(product, dark);
            assert_eq!(s.visuals.hyperlink_color, p.accent_text, "{product:?} dark={dark}");
            assert_eq!(s.visuals.widgets.active.bg_stroke.color, p.accent, "{product:?} dark={dark}: focus ring");
            assert_eq!(s.visuals.panel_fill, p.panel);
            assert_eq!(s.visuals.dark_mode, dark);
        }
    }
}

#[test]
fn the_two_products_share_every_neutral() {
    for dark in [false, true] {
        let (a, b) = (Palette::new(Product::Pdf, dark), Palette::new(Product::Academic, dark));
        assert_eq!((a.panel, a.canvas, a.text, a.border, a.field), (b.panel, b.canvas, b.text, b.border, b.field));
        assert_ne!(a.accent, b.accent);
    }
}

#[test]
fn errors_stay_and_information_fades() {
    let mut n = Notices::default();
    n.push(Tone::Success, "Saved.", 0.0);
    n.push(Tone::Danger, "Export failed.", 0.0);
    n.expire(f64::from(psg_brand::motion::NOTICE_SECS) + 1.0);
    assert_eq!(n.items().len(), 1);
    assert_eq!(n.items().first().map(|x| x.tone), Some(Tone::Danger));
}

#[test]
fn notices_do_not_repeat_or_pile_up() {
    let mut n = Notices::default();
    for i in 0..10 {
        n.push(Tone::Info, format!("message {i}"), 0.0);
        n.push(Tone::Info, "same", 0.0);
    }
    assert_eq!(n.items().len(), crate::notice::MAX_SHOWN);
    assert_eq!(n.items().iter().filter(|x| x.text == "same").count(), 1);
}

#[test]
fn shortcuts_follow_the_platform() {
    assert_eq!(keys::command_for(true, "S", false), "⌘S");
    assert_eq!(keys::command_for(true, "E", true), "⇧⌘E");
    assert_eq!(keys::command_for(false, "S", false), "Ctrl+S");
    assert_eq!(keys::command_for(false, "E", true), "Ctrl+Shift+E");
}

#[test]
fn split_ratios_stay_usable() {
    assert_eq!(layout::clamp_ratio(0.0, 0.2), 0.2);
    assert_eq!(layout::clamp_ratio(1.0, 0.2), 0.8);
    assert_eq!(layout::clamp_ratio(f32::NAN, 0.2), 0.5);
    // A nonsensical minimum is kept sane rather than inverting the range.
    assert_eq!(layout::clamp_ratio(0.9, 0.9), 0.55);
}

#[test]
fn theme_choices_round_trip_and_unknown_follows_the_system() {
    for c in style::ThemeChoice::ALL {
        assert_eq!(style::ThemeChoice::from_key(c.key()), c);
    }
    assert_eq!(style::ThemeChoice::from_key("sepia"), style::ThemeChoice::System);
}

/// The components are reachable by assistive technology under their labels, and the navigation
/// rail reports which entry is selected.
#[test]
fn components_are_accessible() {
    let mut selected = 0usize;
    let mut harness = Harness::new_ui(|ui| {
        ui.ctx().set_fonts(crate::fonts::definitions([]));
        egui_extras::install_image_loaders(ui.ctx());
        style::install(ui.ctx(), Product::Academic);
        for (i, label) in ["Dashboard", "Manuscript"].into_iter().enumerate() {
            if widgets::nav_item(ui, "house", label, selected == i, Some((i * 3, Tone::Danger)), false).clicked() {
                selected = i;
            }
        }
        widgets::primary(ui, "Auto Typeset");
        widgets::chip(ui, Tone::Success, "Ready");
    });
    harness.run();
    harness.get_by_label("Auto Typeset");
    harness.get_by_label("Ready");
    harness.get_by_label("Manuscript").click();
    harness.run();
    drop(harness);
    assert_eq!(selected, 1);
}

#[test]
fn shared_fonts_put_inter_and_jetbrains_mono_first() {
    let defs = crate::fonts::definitions([]);
    // Behind PSG's interface face when the build has it (the next test).
    let lead = if pdfcraft_fonts::psg_ui_font("Regular").is_some() { 1 } else { 0 };
    assert_eq!(defs.families.get(&egui::FontFamily::Proportional).and_then(|f| f.get(lead)).map(String::as_str), Some("Inter"));
    assert_eq!(defs.families.get(&egui::FontFamily::Monospace).and_then(|f| f.first()).map(String::as_str), Some("JetBrainsMono"));
    assert!(defs.families.contains_key(&egui::FontFamily::Name(crate::fonts::SEMIBOLD.into())));
}

/// Every badge the brand can show is one the translation checks know about.
#[test]
fn release_badges_are_marked_for_translation() {
    for version in ["0.20.0-beta", "0.20.0-beta.3", "0.20.0-dev", "0.20.0-rc1"] {
        if let Some(badge) = psg_brand::Release::of(version).badge() {
            assert!(crate::widgets::BADGES.contains(&badge), "{badge}");
        }
    }
}

/// Every character the Latin-script translations use (German, French, Spanish, Portuguese and
/// Vietnamese, with its stacked diacritics) is in all three Inter weights, so a translated label
/// is never drawn partly in a fallback face or as a box. CJK and Hangul come from craft-fonts.
/// With `CRAFT_I18N_DIR` set (a craft-i18n checkout), PSG PDF's craft-i18n catalogs are checked
/// too.
#[test]
fn inter_draws_every_latin_translation() {
    use skrifa::MetadataProvider;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for code in ["de", "fr", "es", "pt-br", "vi"] {
        files.push(root.join(format!("i18n/psg/{code}.tsv")));
        files.push(root.join(format!("i18n/psg-academic/{code}.tsv")));
        if let Some(dir) = std::env::var_os("CRAFT_I18N_DIR") {
            for name in ["common.tsv", "pdfcraft.tsv"] {
                files.push(std::path::Path::new(&dir).join("locales").join(code).join(name));
            }
        }
    }
    let faces =
        [("Inter", crate::fonts::INTER_REGULAR), ("Inter-Medium", crate::fonts::INTER_MEDIUM), ("Inter-SemiBold", crate::fonts::INTER_SEMIBOLD)];
    let mut gaps = std::collections::BTreeSet::new();
    for file in files {
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        // A symbol the English itself uses (the ▸ of a menu path) is the source's choice, drawn
        // by egui's fallback face in every language alike; only what a translation adds counts.
        let used: std::collections::BTreeSet<char> = text
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| {
                let mut cols = l.split('\t').skip(1);
                Some((cols.next()?, cols.next()?))
            })
            .flat_map(|(english, translated)| translated.chars().filter(move |c| !english.contains(*c)))
            .filter(|c| !c.is_control())
            .collect();
        for (name, bytes) in faces {
            let font = skrifa::FontRef::new(bytes).expect("Inter parses");
            let charmap = font.charmap();
            for &c in &used {
                if charmap.map(c).is_none() {
                    gaps.insert(format!("{name} lacks {c:?} (U+{:04X}) used in {}", u32::from(c), file.display()));
                }
            }
        }
    }
    assert!(gaps.is_empty(), "{gaps:#?}");
}

/// Chips in a wrapping row on a narrow card move to the next line whole, and the row never grows
/// wider than the room it was given (the template cards widened past the window before).
#[test]
fn chips_wrap_inside_a_narrow_row() {
    let used = std::cell::Cell::new(0.0f32);
    let mut harness = Harness::builder().with_size(egui::vec2(600.0, 400.0)).build_ui(|ui| {
        ui.ctx().set_fonts(crate::fonts::definitions([]));
        style::install(ui.ctx(), Product::Academic);
        ui.allocate_ui(egui::vec2(160.0, 300.0), |ui| {
            ui.set_max_width(160.0);
            let row = ui.horizontal_wrapped(|ui| {
                for label in ["pdflatex", "Bibliography: biber", "v1.0.0", "Built in", "Used by this project"] {
                    widgets::chip(ui, Tone::Neutral, label);
                }
            });
            used.set(row.response.rect.width());
        });
    });
    harness.run();
    drop(harness);
    assert!(used.get() <= 160.5, "the chip row grew to {} points", used.get());
}

/// With PSG's interface face in the build, Noto Serif CJK TC leads every interface family, in
/// every language, with Inter behind it; without it the definitions are Inter's as before.
#[test]
fn the_interface_face_leads_every_family_when_the_build_has_it() {
    let defs = crate::fonts::definitions([]);
    let first = |family: egui::FontFamily| defs.families.get(&family).and_then(|s| s.first().cloned()).unwrap_or_default();
    let medium = egui::FontFamily::Name(crate::fonts::MEDIUM.into());
    let semibold = egui::FontFamily::Name(crate::fonts::SEMIBOLD.into());
    if pdfcraft_fonts::psg_ui_font("Regular").is_none() {
        eprintln!("built without the interface face (CRAFT_FONTS_DIR with packaging/fetch-psg-fonts.sh): Inter leads");
        assert_eq!(first(egui::FontFamily::Proportional), "Inter");
        assert_eq!(first(semibold), "Inter-SemiBold");
        return;
    }
    let regular = crate::fonts::interface_face_name("Regular");
    assert_eq!(first(egui::FontFamily::Proportional), regular);
    assert_eq!(first(medium), regular);
    assert_eq!(first(semibold), crate::fonts::interface_face_name("SemiBold"));
    let mono = &defs.families[&egui::FontFamily::Monospace];
    assert_eq!(mono.first().map(String::as_str), Some("JetBrainsMono"), "code stays monospaced");
    assert_eq!(mono.get(1), Some(&regular));
    assert!(defs.families[&egui::FontFamily::Proportional].contains(&"Inter".to_owned()), "Inter stays as a fallback");
    // Every interface language's own letters are in the face.
    let face = pdfcraft_fonts::psg_ui_font("Regular").map(|f| f.bytes).unwrap_or_default();
    for text in ["設定", "设置", "設定する", "설정", "Einstellungen ß", "Paramètres", "Configuración", "Configurações", "Cài đặt"]
    {
        assert!(text.chars().filter(|c| !c.is_whitespace()).all(|c| pdfcraft_fonts::face_has(face, c)), "{text}");
    }
}
