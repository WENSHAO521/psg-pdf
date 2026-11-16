use super::*;

/// WCAG relative luminance of an sRGB colour.
fn luminance(c: Rgb) -> f64 {
    let channel = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * channel(c[0]) + 0.7152 * channel(c[1]) + 0.0722 * channel(c[2])
}

/// WCAG contrast ratio of two colours.
fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// The value of `key` in the `[[product]]` table whose id is `id`, read from the manifest the
/// installers use, so the apps' colours and names can't drift from the packaging.
fn manifest_value(id: &str, key: &str) -> Option<String> {
    let manifest = include_str!("../../../packaging/products.toml");
    let section = manifest.split("[[product]]").find(|s| s.lines().any(|l| l.trim() == format!("id = \"{id}\"")))?;
    let line = section.lines().find(|l| l.trim_start().starts_with(&format!("{key} =")))?;
    Some(line.split_once('=')?.1.trim().trim_matches('"').to_owned())
}

#[test]
fn names_taglines_and_brand_colours_match_the_product_manifest() {
    for product in Product::ALL {
        assert_eq!(manifest_value(product.id(), "name").as_deref(), Some(product.name()), "{product:?}");
        assert_eq!(manifest_value(product.id(), "tagline").as_deref(), Some(product.tagline()), "{product:?}");
        assert_eq!(manifest_value(product.id(), "accent").as_deref(), Some(hex(product.brand()).as_str()), "{product:?}");
    }
}

#[test]
fn the_light_accent_is_the_brand_colour() {
    for product in Product::ALL {
        assert_eq!(product.accent(false).accent, product.brand(), "{product:?}");
    }
}

/// Accent text and links stay readable (WCAG AA, 4.5:1) on both themes' panels and canvases,
/// and text on an accent fill does too.
#[test]
fn accent_colours_are_readable_in_both_themes() {
    for product in Product::ALL {
        for (dark, surfaces) in [(false, [neutral::LIGHT_PANEL, neutral::LIGHT_CANVAS]), (true, [neutral::DARK_PANEL, neutral::DARK_CANVAS])] {
            let a = product.accent(dark);
            for surface in surfaces {
                let ratio = contrast(a.accent_text, surface);
                assert!(ratio >= 4.5, "{product:?} dark={dark}: accent text {} on {} is {ratio:.2}", hex(a.accent_text), hex(surface));
                assert!(contrast(a.accent_text, a.accent_soft) >= 4.5, "{product:?} dark={dark}: accent text on its soft wash");
                // Non-text accents (focus rings, the active tab marker) need 3:1.
                assert!(contrast(a.accent, surface) >= 3.0, "{product:?} dark={dark}: accent on {}", hex(surface));
            }
            assert!(contrast(a.on_accent, a.accent) >= 4.5, "{product:?} dark={dark}: text on the accent fill");
        }
    }
}

/// The two products stay distinguishable: their accents are not near each other.
#[test]
fn the_products_accents_differ() {
    for dark in [false, true] {
        let (a, b) = (Product::Pdf.accent(dark).accent, Product::Academic.accent(dark).accent);
        let distance: i32 = a.iter().zip(b).map(|(&x, y)| (i32::from(x) - i32::from(y)).abs()).sum();
        assert!(distance > 90, "dark={dark}: {} vs {}", hex(a), hex(b));
    }
}

#[test]
fn release_kinds_come_from_the_version() {
    assert_eq!(Release::of("0.19.0"), Release::Stable);
    assert_eq!(Release::of("0.19.0-beta.3"), Release::Beta(Some(3)));
    assert_eq!(Release::of("0.19.0-beta"), Release::Beta(None));
    assert_eq!(Release::of("0.19.0-beta.3+titlebar"), Release::Beta(Some(3)));
    assert_eq!(Release::of("0.19.0-alpha.1"), Release::Development);
    assert_eq!(Release::of("0.19.0-betamax"), Release::Development);
    assert_eq!(Release::of("0.19.0+titlebar"), Release::Development);
    assert_eq!(Release::of(""), Release::Stable);
}

#[test]
fn versions_and_titles_read_as_people_expect() {
    assert_eq!(display_version("0.19.0"), "0.19.0");
    assert_eq!(display_version("0.19.0-beta.12"), "0.19.0 Beta 12");
    assert_eq!(display_version("0.19.0-rc.1"), "0.19.0-rc.1");
    assert_eq!(Product::Pdf.window_title("0.19.0"), "PSG PDF");
    assert_eq!(Product::Pdf.window_title("0.19.0-beta.4"), "PSG PDF Beta");
    assert_eq!(Product::Academic.window_title("0.19.0-beta.4"), "PSG Academic PDF Beta");
    assert_eq!(Product::Academic.window_title("1.0.0"), "PSG Academic PDF");
}

/// Body and secondary text stay readable (4.5:1) on every surface they are drawn on, in both
/// themes, so the two products' shared chrome passes WCAG AA.
#[test]
fn neutral_text_is_readable_on_every_surface() {
    for dark in [false, true] {
        let s = surfaces(dark);
        for (name, bg) in [("panel", s.panel), ("canvas", s.canvas), ("card", s.card), ("field", s.field)] {
            for (which, fg) in [("text", s.text), ("muted", s.text_muted), ("faint", s.text_faint)] {
                let ratio = contrast(fg, bg);
                assert!(ratio >= 4.5, "dark={dark}: {which} {} on {name} {} is {ratio:.2}", hex(fg), hex(bg));
            }
        }
        // Hovered rows keep body and secondary text readable.
        assert!(contrast(s.text_muted, s.hover) >= 4.5, "dark={dark}: muted on hover");
    }
}

/// Every tone's text is readable on panels and on its own wash, and its solid mark shows on a
/// panel, in both products and both themes.
#[test]
fn tones_are_readable_in_both_themes() {
    for product in Product::ALL {
        for dark in [false, true] {
            let panel = surfaces(dark).panel;
            for tone in Tone::ALL {
                let c = tone.colours(product, dark);
                assert!(contrast(c.text, panel) >= 4.5, "{product:?} {tone:?} dark={dark}: text on panel");
                assert!(contrast(c.text, c.soft) >= 4.5, "{product:?} {tone:?} dark={dark}: text on its wash");
                assert!(contrast(c.solid, panel) >= 3.0, "{product:?} {tone:?} dark={dark}: solid mark on panel");
            }
        }
    }
}

#[test]
fn surfaces_agree_with_the_neutral_constants() {
    assert_eq!(surfaces(false).panel, neutral::LIGHT_PANEL);
    assert_eq!(surfaces(false).canvas, neutral::LIGHT_CANVAS);
    assert_eq!(surfaces(true).panel, neutral::DARK_PANEL);
    assert_eq!(surfaces(true).canvas, neutral::DARK_CANVAS);
}
