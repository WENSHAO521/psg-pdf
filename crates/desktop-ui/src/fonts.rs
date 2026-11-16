//! The shared interface fonts: Inter for text, JetBrains Mono for code, logs and TeX source (both
//! OFL, `assets/fonts/`, recorded in ATTRIBUTION.toml). Each file is embedded once for both apps:
//! Inter Regular through `pdfcraft-fonts`, the others here.

use std::sync::Arc;

use egui::{FontData, FontDefinitions, FontFamily, FontId};

/// Inter Regular (OFL-1.1).
pub const INTER_REGULAR: &[u8] = pdfcraft_fonts::INTER_REGULAR;
/// Inter Medium (OFL-1.1).
pub const INTER_MEDIUM: &[u8] = include_bytes!("../../../assets/fonts/Inter-Medium.ttf");
/// Inter SemiBold (OFL-1.1).
pub const INTER_SEMIBOLD: &[u8] = include_bytes!("../../../assets/fonts/Inter-SemiBold.ttf");
/// JetBrains Mono Regular (OFL-1.1).
pub const JETBRAINS_MONO: &[u8] = include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf");

/// The family name of the medium weight.
pub const MEDIUM: &str = "medium";
/// The family name of the semibold weight.
pub const SEMIBOLD: &str = "semibold";

/// A face to try after the shared ones (CJK, Hangul, Arabic… from the craft-fonts build input).
pub struct Fallback {
    pub name: String,
    pub bytes: &'static [u8],
}

/// The shared faces, then `fallbacks` in order, for the proportional, monospace, medium and
/// semibold families. egui's built-in faces stay last, for symbols none of these have.
pub fn definitions(fallbacks: impl IntoIterator<Item = Fallback>) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in
        [("Inter", INTER_REGULAR), ("Inter-Medium", INTER_MEDIUM), ("Inter-SemiBold", INTER_SEMIBOLD), ("JetBrainsMono", JETBRAINS_MONO)]
    {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }
    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, "Inter".to_owned());
    let monospace = fonts.families.entry(FontFamily::Monospace).or_default();
    monospace.insert(0, "JetBrainsMono".to_owned());
    // Latin and Cyrillic letters JetBrains Mono lacks come from Inter before anything else.
    monospace.insert(1, "Inter".to_owned());
    for face in fallbacks {
        if !fonts.font_data.contains_key(&face.name) {
            let mut data = FontData::from_static(face.bytes);
            data.tweak.fallback_align = egui::epaint::text::FallbackAlign::Baseline;
            fonts.font_data.insert(face.name.clone(), Arc::new(data));
        }
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            let stack = fonts.families.entry(family).or_default();
            if !stack.contains(&face.name) {
                stack.push(face.name.clone());
            }
        }
    }
    let rest: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for (family, primary) in [(MEDIUM, "Inter-Medium"), (SEMIBOLD, "Inter-SemiBold")] {
        let mut stack = vec![primary.to_owned()];
        stack.extend(rest.iter().cloned());
        fonts.families.insert(FontFamily::Name(family.into()), stack);
    }
    fonts
}

/// Inter Regular at `size` points.
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

/// Inter Medium at `size` points (labels that need a little weight: tabs, navigation).
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

/// Inter SemiBold at `size` points (titles, headings, primary buttons).
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// JetBrains Mono at `size` points.
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}
