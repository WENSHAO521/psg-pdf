//! The few Lucide icons the shared components draw (ISC, `assets/icons/LICENSE-lucide.txt`).
//!
//! They come from the same `assets/icons/` files PSG PDF embeds, recoloured to white and tinted per
//! use, so an icon follows the theme like text does. They need egui's image loaders, which
//! [`crate::style::install`] installs (PSG PDF installs them itself). Never use Unicode glyphs as
//! icons: the interface fonts don't carry them all.

use std::sync::OnceLock;

use craft_ui::icons::{SvgIconSet, SvgStyle};
use egui::{Color32, Rect, Vec2};

/// The icons, by Lucide name. `square` is the fallback for an unknown name.
static ICONS: &[(&str, &[u8])] = &[
    ("square", include_bytes!("../../../assets/icons/square.svg")),
    ("house", include_bytes!("../../../assets/icons/house.svg")),
    ("file-text", include_bytes!("../../../assets/icons/file-text.svg")),
    ("swatch-book", include_bytes!("../../../assets/icons/swatch-book.svg")),
    ("type", include_bytes!("../../../assets/icons/type.svg")),
    ("triangle-alert", include_bytes!("../../../assets/icons/triangle-alert.svg")),
    ("file-output", include_bytes!("../../../assets/icons/file-output.svg")),
    ("settings", include_bytes!("../../../assets/icons/settings.svg")),
];

fn set() -> &'static SvgIconSet {
    static SET: OnceLock<SvgIconSet> = OnceLock::new();
    SET.get_or_init(|| SvgIconSet::new(ICONS, SvgStyle { current_color: "#ffffff", stroke_width: Some("1.75") }, "bytes://psg-icons/", "square"))
}

/// Whether `name` is one of the shared icons.
pub fn exists(name: &str) -> bool {
    set().contains(name)
}

/// `name` at `size` points, tinted `tint`. An unknown name draws the fallback square.
pub fn image(name: &str, size: f32, tint: Color32) -> egui::Image<'static> {
    egui::Image::new(set().source(set().canonical_name(name).unwrap_or("square"))).fit_to_exact_size(Vec2::splat(size)).tint(tint)
}

/// Paints `name` centred in `rect`.
pub fn paint(ui: &egui::Ui, rect: Rect, name: &str, size: f32, tint: Color32) {
    image(name, size, tint).paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(size)));
}
