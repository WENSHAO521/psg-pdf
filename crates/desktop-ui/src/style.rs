//! The egui style of both themes, from the tokens.
//!
//! [`install`] gives egui's light and dark styles the product's accent and the shared neutrals,
//! radii, spacing, text sizes and focus ring, and remembers the product for [`crate::Palette::of`].
//! The window follows the system's light or dark setting live (egui's `ThemePreference::System`);
//! an app that offers a manual choice calls [`set_theme`].

use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle, Visuals};
use psg_brand::{Product, radius, size, space, text};

use crate::palette::Palette;

fn product_id() -> egui::Id {
    egui::Id::unique("psg-desktop-ui-product")
}

/// The product whose style is installed in `ctx` (PSG PDF before [`install`]).
pub fn product(ctx: &egui::Context) -> Product {
    ctx.data(|d| d.get_temp::<Product>(product_id())).unwrap_or(Product::Pdf)
}

/// The theme a person chose: follow the system, or always light or dark.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    /// All choices, in the order a menu offers them.
    pub const ALL: [ThemeChoice; 3] = [ThemeChoice::System, ThemeChoice::Light, ThemeChoice::Dark];

    /// The stable name saved in settings files.
    pub const fn key(self) -> &'static str {
        match self {
            ThemeChoice::System => "system",
            ThemeChoice::Light => "light",
            ThemeChoice::Dark => "dark",
        }
    }

    /// The choice saved as `key`; anything unknown follows the system.
    pub fn from_key(key: &str) -> Self {
        match key.trim() {
            "light" => ThemeChoice::Light,
            "dark" => ThemeChoice::Dark,
            _ => ThemeChoice::System,
        }
    }
}

/// Follow the system, or pin light or dark.
pub fn set_theme(ctx: &egui::Context, choice: ThemeChoice) {
    ctx.set_theme(match choice {
        ThemeChoice::System => egui::ThemePreference::System,
        ThemeChoice::Light => egui::ThemePreference::Light,
        ThemeChoice::Dark => egui::ThemePreference::Dark,
    });
}

/// Remember `product` for [`crate::Palette::of`] without touching the style, for an app that
/// writes its own egui style from the same tokens (PSG PDF).
pub fn remember_product(ctx: &egui::Context, product: Product) {
    ctx.data_mut(|d| d.insert_temp(product_id(), product));
}

/// Install `product`'s style for both themes. A desktop that reports no theme gets light.
///
/// The components draw with the [`crate::fonts::MEDIUM`] and [`crate::fonts::SEMIBOLD`] families,
/// so the app installs [`crate::fonts::definitions`] (or a superset with those two families)
/// before the first frame.
pub fn install(ctx: &egui::Context, product: Product) {
    ctx.data_mut(|d| d.insert_temp(product_id(), product));
    ctx.options_mut(|o| o.fallback_theme = egui::Theme::Light);
    for (theme, dark) in [(egui::Theme::Light, false), (egui::Theme::Dark, true)] {
        ctx.style_mut_of(theme, |style| apply(style, &Palette::new(product, dark)));
    }
}

/// The type ramp: the sizes of egui's text styles, in points.
pub fn text_styles() -> [(TextStyle, FontId); 5] {
    [
        (TextStyle::Small, FontId::new(text::SMALL, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(text::BODY, FontFamily::Proportional)),
        (TextStyle::Button, FontId::new(text::BODY, FontFamily::Proportional)),
        (TextStyle::Heading, FontId::new(text::TITLE, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(text::SMALL, FontFamily::Monospace)),
    ]
}

/// Write palette `p` and the shared shape, spacing and type tokens into `style`.
pub fn apply(style: &mut egui::Style, p: &Palette) {
    let mut v = if p.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = p.panel;
    v.window_fill = p.card;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.extreme_bg_color = p.field;
    v.faint_bg_color = p.hover;
    v.code_bg_color = p.hover;
    v.override_text_color = None;
    v.hyperlink_color = p.accent_text;
    v.selection.bg_fill = p.selected;
    v.selection.stroke = Stroke::new(1.0, p.accent_text);
    v.warn_fg_color = p.tone(psg_brand::Tone::Warning).text;
    v.error_fg_color = p.tone(psg_brand::Tone::Danger).text;
    v.window_corner_radius = CornerRadius::same(radius::DIALOG);
    v.menu_corner_radius = CornerRadius::same(radius::CARD);
    v.window_shadow = egui::Shadow { offset: [0, 6], blur: 24, spread: 0, color: p.shadow };
    v.popup_shadow = egui::Shadow { offset: [0, 3], blur: 12, spread: 0, color: p.shadow };
    let control = CornerRadius::same(radius::CONTROL);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.divider);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.inactive.bg_fill = p.field;
    w.inactive.weak_bg_fill = p.hover;
    w.inactive.bg_stroke = Stroke::new(1.0, p.border);
    w.inactive.fg_stroke = Stroke::new(1.0, p.text);
    w.hovered.bg_fill = p.hover;
    w.hovered.weak_bg_fill = p.hover;
    w.hovered.bg_stroke = Stroke::new(1.0, p.accent);
    w.hovered.fg_stroke = Stroke::new(1.5, p.text);
    w.active.bg_fill = p.pressed;
    w.active.weak_bg_fill = p.pressed;
    w.active.bg_stroke = Stroke::new(size::FOCUS_RING, p.accent);
    w.active.fg_stroke = Stroke::new(1.5, p.text);
    w.open.bg_fill = p.accent_soft;
    w.open.weak_bg_fill = p.accent_soft;
    w.open.bg_stroke = Stroke::new(1.0, p.accent);
    w.open.fg_stroke = Stroke::new(1.0, p.text);
    for state in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
        state.corner_radius = control;
        state.expansion = 0.0;
    }
    style.visuals = v;
    style.text_styles = text_styles().into_iter().collect();
    let s = &mut style.spacing;
    s.item_spacing = egui::vec2(space::S, space::S - 2.0);
    s.button_padding = egui::vec2(space::M - 2.0, space::XS);
    s.interact_size = egui::vec2(40.0, size::CONTROL - 6.0);
    s.window_margin = egui::Margin::same(space::L as i8);
    s.menu_margin = egui::Margin::same(space::XS as i8);
    s.indent = space::L + 2.0;
    s.combo_width = 160.0;
    s.text_edit_width = 240.0;
    s.scroll.bar_width = 8.0;
    // Keyboard focus is visible: egui draws the focused widget with the `active` stroke.
    style.interaction.selectable_labels = true;
}

/// The focus ring colour of `p`: the accent, which meets 3:1 on every panel.
pub fn focus_ring(p: &Palette) -> Stroke {
    Stroke::new(size::FOCUS_RING, p.accent)
}

/// The colour of disabled text in `p`.
pub fn disabled(p: &Palette) -> Color32 {
    p.text_faint
}
