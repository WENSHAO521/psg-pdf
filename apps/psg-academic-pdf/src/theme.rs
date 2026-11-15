//! PSG Academic PDF's look, from the PSG design system (`psg-brand`, docs/brand.md): egui's own
//! widgets with the academic purple accent, the shared corner radii and spacing, and the release
//! badge ("Beta") drawn the same way as in PSG PDF.

use egui::{Color32, CornerRadius, Stroke};

/// This product in the PSG design system.
pub const PRODUCT: psg_brand::Product = psg_brand::Product::Academic;

fn colour([r, g, b]: psg_brand::Rgb) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Give both of egui's themes (light and dark) the PSG Academic PDF accent, radii and spacing.
/// The window follows the system's light or dark setting and, like PSG PDF, is light when the
/// system says nothing.
pub fn apply(ctx: &egui::Context) {
    ctx.options_mut(|o| o.fallback_theme = egui::Theme::Light);
    for (theme, dark) in [(egui::Theme::Light, false), (egui::Theme::Dark, true)] {
        let accent = PRODUCT.accent(dark);
        ctx.style_mut_of(theme, |style| {
            let v = &mut style.visuals;
            v.selection.bg_fill = colour(accent.accent);
            v.selection.stroke = Stroke::new(1.0, colour(accent.on_accent));
            v.hyperlink_color = colour(accent.accent_text);
            let control = CornerRadius::same(psg_brand::radius::CONTROL);
            for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
                w.corner_radius = control;
            }
            v.widgets.hovered.bg_stroke = Stroke::new(1.0, colour(accent.accent));
            v.widgets.active.bg_stroke = Stroke::new(1.0, colour(accent.accent));
            v.window_corner_radius = CornerRadius::same(psg_brand::radius::DIALOG);
            v.menu_corner_radius = CornerRadius::same(psg_brand::radius::CARD);
            style.spacing.item_spacing = egui::vec2(psg_brand::space::S, psg_brand::space::S - 2.0);
            style.spacing.button_padding = egui::vec2(psg_brand::space::S, psg_brand::space::XS);
        });
    }
}

/// The main action of a screen (Compile): white text on the accent fill. The interface fonts
/// have no ▶ glyph, so the label is text only.
pub fn primary_button(ui: &egui::Ui, label: &str) -> egui::Button<'static> {
    let accent = PRODUCT.accent(ui.visuals().dark_mode);
    egui::Button::new(egui::RichText::new(label.to_owned()).strong().color(colour(accent.on_accent))).fill(colour(accent.accent))
}

/// The product name in its brand colour, then the release badge in a beta build.
pub fn product_heading(ui: &mut egui::Ui) {
    let dark = ui.visuals().dark_mode;
    ui.label(egui::RichText::new(PRODUCT.name()).strong().color(colour(PRODUCT.accent(dark).accent_text)));
    badge(ui);
}

/// The release badge ("Beta") in the shared PSG gold, or nothing in a stable release.
pub fn badge(ui: &mut egui::Ui) {
    if let Some(text) = psg_brand::Release::of(env!("CARGO_PKG_VERSION")).badge() {
        let gold = colour(psg_brand::GOLD);
        egui::Frame::NONE
            .fill(gold.gamma_multiply(0.18))
            .stroke(Stroke::new(1.0, gold))
            .corner_radius(CornerRadius::same(psg_brand::radius::SMALL))
            .inner_margin(egui::Margin::symmetric(6, 1))
            .show(ui, |ui| {
                ui.label(egui::RichText::new(text).small().strong().color(ui.visuals().text_color()));
            });
    }
}

/// The window title: "PSG Academic PDF", or "PSG Academic PDF Beta" in a beta build.
pub fn window_title() -> String {
    PRODUCT.window_title(env!("CARGO_PKG_VERSION"))
}

/// The version as people read it ("0.19.0 Beta 3").
pub fn display_version() -> String {
    psg_brand::display_version(env!("CARGO_PKG_VERSION"))
}
