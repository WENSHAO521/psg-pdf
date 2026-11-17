//! The shared interface fonts: Noto Serif CJK TC (思源宋體) for all interface text in every
//! language, when the build has it (the craft-fonts input with `packaging/fetch-psg-fonts.sh`,
//! as every release build does); Inter otherwise and as its fallback; JetBrains Mono for code,
//! logs and TeX source. Inter and JetBrains Mono are OFL files in `assets/fonts/`, recorded in
//! ATTRIBUTION.toml, embedded once for both apps: Inter Regular through `pdfcraft-fonts`, the
//! others here.

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
    use_interface_face(&mut fonts);
    fonts
}

/// The registered name of the interface face in `style`.
pub fn interface_face_name(style: &str) -> String {
    format!("{INTERFACE_FAMILY} {style}")
}

/// The family of PSG's interface face.
pub const INTERFACE_FAMILY: &str = "Noto Serif CJK TC";

/// Put Noto Serif CJK TC first for all interface text, in every language: Regular for the
/// proportional and medium families (the build carries no Medium weight; Regular keeps the
/// installers smaller), SemiBold for the semibold family, and Regular right after JetBrains Mono
/// for the CJK, Hangul and accented letters code and logs show. The existing faces stay behind
/// it as fallbacks. Without the face (a build without the craft-fonts input, the web build)
/// `fonts` is unchanged. Both apps call this on their own font definitions.
pub fn use_interface_face(fonts: &mut FontDefinitions) {
    let (Some(regular), semibold) = (pdfcraft_fonts::psg_ui_font("Regular"), pdfcraft_fonts::psg_ui_font("SemiBold")) else {
        return;
    };
    let regular_name = interface_face_name("Regular");
    fonts.font_data.insert(regular_name.clone(), Arc::new(FontData::from_static(regular.bytes)));
    let semibold_name = match semibold {
        Some(face) => {
            let name = interface_face_name("SemiBold");
            fonts.font_data.insert(name.clone(), Arc::new(FontData::from_static(face.bytes)));
            name
        }
        None => regular_name.clone(),
    };
    let mut first = |family: FontFamily, at: usize, name: &str| {
        let stack = fonts.families.entry(family).or_default();
        stack.retain(|n| n != name);
        stack.insert(at.min(stack.len()), name.to_owned());
    };
    first(FontFamily::Proportional, 0, &regular_name);
    first(FontFamily::Name(MEDIUM.into()), 0, &regular_name);
    first(FontFamily::Name(SEMIBOLD.into()), 0, &semibold_name);
    first(FontFamily::Monospace, 1, &regular_name);
}

/// The interface face (Noto Serif CJK TC Regular, else Inter Regular) at `size` points.
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

/// The medium interface family (Noto Serif CJK TC Regular, else Inter Medium) at `size` points (labels that need a little weight: tabs, navigation).
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

/// The semibold interface family (Noto Serif CJK TC SemiBold, else Inter SemiBold) at `size` points (titles, headings, primary buttons).
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// JetBrains Mono at `size` points.
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}
