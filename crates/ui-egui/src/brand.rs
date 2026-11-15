//! PSG PDF's name and identity, in one place. PSG PDF is Panorama Scholarly Group's edition of
//! PdfCraft (MIT OR Apache-2.0); the PDF engine underneath is PdfCraft's, unchanged. The names,
//! colours and release label come from the PSG design system (`psg-brand`, `docs/brand.md`),
//! which PSG Academic PDF uses too.

/// This product in the PSG design system.
pub const PRODUCT: psg_brand::Product = psg_brand::Product::Pdf;
/// The product name, in window titles, menus and the About dialog.
pub const APP_NAME: &str = PRODUCT.name();
/// The organisation that publishes PSG PDF.
pub const PUBLISHER: &str = psg_brand::PUBLISHER;
/// The folder name for settings, recovery, logs and caches, so PSG PDF never shares them with an
/// installed PdfCraft. Never change it: installed copies keep their settings there.
pub const DATA_FOLDER: &str = "PSG PDF";
/// The upstream project PSG PDF is based on (plain-text attribution, as its licence allows).
pub const BASED_ON: &str = "Based on PdfCraft by the ArtCraft team (MIT OR Apache-2.0).";

/// What kind of release this build is (stable, beta or a development build).
pub fn release() -> psg_brand::Release {
    psg_brand::Release::of(pdfcraft_engine::build_info::VERSION)
}

/// The window title with no document open: "PSG PDF", or "PSG PDF Beta" in a beta build.
pub fn window_title() -> String {
    PRODUCT.window_title(pdfcraft_engine::build_info::VERSION)
}

/// The version as people read it ("0.19.0 Beta 3"), with the build label of a local build.
pub fn display_version() -> String {
    let version = psg_brand::display_version(pdfcraft_engine::build_info::VERSION);
    match pdfcraft_engine::build_info::LABEL.map(str::trim).filter(|l| !l.is_empty()) {
        Some(label) => format!("{version} ({label})"),
        None => version,
    }
}

/// The release badge ("Beta") beside the product name, drawn in the shared PSG gold, or nothing
/// in a stable release.
pub fn badge(ui: &mut egui::Ui) {
    if let Some(text) = release().badge() {
        let [r, g, b] = psg_brand::GOLD;
        let gold = egui::Color32::from_rgb(r, g, b);
        egui::Frame::NONE
            .fill(gold.gamma_multiply(0.18))
            .stroke(egui::Stroke::new(1.0, gold))
            .corner_radius(egui::CornerRadius::same(psg_brand::radius::SMALL))
            .inner_margin(egui::Margin::symmetric(6, 1))
            .show(ui, |ui| {
                let t = crate::theme::Tokens::get(ui.ctx());
                ui.label(egui::RichText::new(text).small().strong().color(t.text));
            });
    }
}

/// An sRGB triple from the design system as an egui colour.
pub fn colour([r, g, b]: psg_brand::Rgb) -> egui::Color32 {
    egui::Color32::from_rgb(r, g, b)
}
