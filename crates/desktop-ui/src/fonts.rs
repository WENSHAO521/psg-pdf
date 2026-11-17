//! The shared interface fonts: the Kai faces 霞鶩文楷 TC and Lite, with Gowun Batang for Korean,
//! for all interface text ([`interface_faces`]), when the build has them (the craft-fonts input
//! with `packaging/fetch-psg-fonts.sh`, as every release build does); Inter otherwise and as
//! their fallback; JetBrains Mono for code, logs and TeX source. Inter and JetBrains Mono are OFL
//! files in `assets/fonts/`, recorded in ATTRIBUTION.toml, embedded once for both apps: Inter
//! Regular through `pdfcraft-fonts`, the others here.

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

/// The faces PSG draws its interface in for one interface language: regular text, medium labels
/// (tabs, navigation) and headings, and the faces tried right after them.
pub struct InterfaceFaces {
    pub regular: &'static pdfcraft_fonts::CraftFont,
    pub medium: &'static pdfcraft_fonts::CraftFont,
    pub heading: &'static pdfcraft_fonts::CraftFont,
    pub then: Vec<&'static pdfcraft_fonts::CraftFont>,
}

/// LXGW WenKai TC (霞鶩文楷 TC): Traditional Chinese, Japanese and every Latin-script language.
pub const KAI_TC: &str = "LXGW WenKai TC";
/// LXGW WenKai Lite (霞鶩文楷 Lite): Simplified Chinese, with its Simplified glyph forms.
pub const KAI_SC: &str = "LXGW WenKai Lite";
/// Gowun Batang (고운바탕): Korean, a soft brush-drawn Myeongjo that sits well beside the Kai faces.
pub const KOREAN: &str = "Gowun Batang";

/// The interface faces for language `lang` (a code such as `zh-hant`, `ko`, `en`): 霞鶩文楷 TC for
/// Traditional Chinese, Japanese, English, German, French, Spanish, Portuguese and Vietnamese
/// (it has their letters, kana and tone marks), 霞鶩文楷 Lite for Simplified Chinese, and Gowun
/// Batang for Korean, which the Kai faces lack (霞鶩文楷 TC follows it for Hanja). `None` when the
/// build lacks them (a build without the craft-fonts input, the web build): the interface then
/// keeps Inter.
pub fn interface_faces(lang: &str) -> Option<InterfaceFaces> {
    let face = pdfcraft_fonts::psg_ui_font;
    let tc = face(KAI_TC, "Regular")?;
    let tc_medium = face(KAI_TC, "Medium").unwrap_or(tc);
    Some(match lang {
        "zh-hans" => match face(KAI_SC, "Regular") {
            Some(sc) => {
                let sc_medium = face(KAI_SC, "Medium").unwrap_or(sc);
                InterfaceFaces { regular: sc, medium: sc_medium, heading: sc_medium, then: vec![tc] }
            }
            None => InterfaceFaces { regular: tc, medium: tc_medium, heading: tc_medium, then: Vec::new() },
        },
        "ko" => match face(KOREAN, "Regular") {
            Some(ko) => InterfaceFaces { regular: ko, medium: ko, heading: face(KOREAN, "Bold").unwrap_or(ko), then: vec![tc] },
            None => InterfaceFaces { regular: tc, medium: tc_medium, heading: tc_medium, then: Vec::new() },
        },
        _ => InterfaceFaces { regular: tc, medium: tc_medium, heading: tc_medium, then: Vec::new() },
    })
}

/// Put the interface faces for language `lang` ([`interface_faces`]) first: the regular face for
/// proportional text, the medium face for the medium family, the heading face for the semibold
/// family, each followed by the regular face and then the faces after it; the regular face also
/// comes right after JetBrains Mono, for the CJK, Hangul and accented letters code and logs show.
/// Everything already there stays behind them as fallbacks (symbols, Arabic, Hebrew…). Without
/// the faces `fonts` is unchanged. Both apps call this on their own font definitions, and again
/// whenever the interface language changes.
pub fn use_interface_face(fonts: &mut FontDefinitions, lang: &str) {
    let Some(faces) = interface_faces(lang) else { return };
    let mut add = |f: &'static pdfcraft_fonts::CraftFont| {
        let name = f.name();
        if !fonts.font_data.contains_key(&name) {
            fonts.font_data.insert(name.clone(), Arc::new(FontData::from_static(f.bytes)));
        }
        name
    };
    let regular = add(faces.regular);
    let medium = add(faces.medium);
    let heading = add(faces.heading);
    let then: Vec<String> = faces.then.iter().map(|f| add(f)).collect();
    let mut lead = |family: FontFamily, at: usize, names: &[&String]| {
        let stack = fonts.families.entry(family).or_default();
        stack.retain(|n| !names.contains(&n));
        for (i, name) in names.iter().enumerate() {
            stack.insert((at + i).min(stack.len()), (*name).clone());
        }
    };
    let with = |first: &String| -> Vec<String> {
        let mut v = vec![first.clone()];
        for n in std::iter::once(&regular).chain(then.iter()) {
            if !v.contains(n) {
                v.push(n.clone());
            }
        }
        v
    };
    for (family, first) in
        [(FontFamily::Proportional, &regular), (FontFamily::Name(MEDIUM.into()), &medium), (FontFamily::Name(SEMIBOLD.into()), &heading)]
    {
        let names = with(first);
        lead(family, 0, &names.iter().collect::<Vec<_>>());
    }
    let names = with(&regular);
    lead(FontFamily::Monospace, 1, &names.iter().collect::<Vec<_>>());
}

/// The interface face (霞鶩文楷 or Gowun Batang, else Inter Regular) at `size` points.
pub fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

/// The medium interface family (霞鶩文楷 Medium or Gowun Batang, else Inter Medium) at `size` points (labels that need a little weight: tabs, navigation).
pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

/// The semibold interface family (霞鶩文楷 Medium or Gowun Batang Bold, else Inter SemiBold) at `size` points (titles, headings, primary buttons).
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// JetBrains Mono at `size` points.
pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}
