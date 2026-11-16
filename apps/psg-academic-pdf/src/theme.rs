//! PSG Academic PDF's look: the PSG Desktop Design System (`psg-desktop-ui`) with the academic
//! purple accent. Everything else — neutrals, type, shapes, components — is shared with PSG PDF.

/// This product in the PSG design system.
pub const PRODUCT: psg_brand::Product = psg_brand::Product::Academic;

/// Install the design system's light and dark styles with this product's accent.
pub fn apply(ctx: &egui::Context) {
    psg_desktop_ui::style::install(ctx, PRODUCT);
}

/// The window title: "PSG Academic PDF", or "PSG Academic PDF Beta" in a beta build.
pub fn window_title() -> String {
    PRODUCT.window_title(env!("CARGO_PKG_VERSION"))
}

/// The version as people read it ("0.19.0 Beta 3").
pub fn display_version() -> String {
    psg_brand::display_version(env!("CARGO_PKG_VERSION"))
}
