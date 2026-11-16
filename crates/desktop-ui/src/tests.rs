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
    assert_eq!(defs.families.get(&egui::FontFamily::Proportional).and_then(|f| f.first()).map(String::as_str), Some("Inter"));
    assert_eq!(defs.families.get(&egui::FontFamily::Monospace).and_then(|f| f.first()).map(String::as_str), Some("JetBrainsMono"));
    assert!(defs.families.contains_key(&egui::FontFamily::Name(crate::fonts::SEMIBOLD.into())));
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
