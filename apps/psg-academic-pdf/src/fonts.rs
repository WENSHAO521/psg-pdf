//! Interface fonts: the PSG Desktop Design System's Kai faces for the interface language (霞鶩文楷;
//! Gowun Batang for Korean) when the build has them, Inter and JetBrains Mono (shared with PSG
//! PDF), then the CJK and Korean faces of the craft-fonts build input
//! (OFL, embedded in release builds), so Chinese, Japanese and Korean names, titles and the
//! translated interface show as text instead of boxes. The face for the interface language comes
//! first, so a line of Chinese is not drawn half in a Japanese face. Without craft-fonts these
//! scripts show as boxes (the same degraded mode as PSG PDF without it).

use egui::FontDefinitions;

/// Install the fonts for interface language `lang` (a code from [`crate::i18n::LANGUAGES`]).
pub fn install(ctx: &egui::Context, lang: &str) {
    ctx.set_fonts(definitions(lang));
}

/// The craft-fonts faces added after egui's own, in the order they are tried.
#[cfg(test)]
pub fn fallback_names(lang: &str) -> Vec<String> {
    faces(lang).iter().map(|f| f.name()).collect()
}

fn faces(lang: &str) -> Vec<&'static pdfcraft_fonts::CraftFont> {
    let prefer_hans = matches!(lang, "zh-hans" | "zh-hant");
    let mut out = pdfcraft_fonts::ui_cjk_fonts(prefer_hans);
    let korean: Vec<&'static pdfcraft_fonts::CraftFont> =
        pdfcraft_fonts::CRAFT_FONTS.iter().filter(|f| f.covers("Hang") || f.covers("Kore")).collect();
    // Korean first in Korean, so Hanja and Hangul share one face's metrics where it has both.
    if lang == "ko" {
        let mut k = korean;
        k.extend(out);
        out = k;
    } else {
        out.extend(korean);
    }
    out
}

fn definitions(lang: &str) -> FontDefinitions {
    let mut fonts =
        psg_desktop_ui::fonts::definitions(faces(lang).into_iter().map(|f| psg_desktop_ui::fonts::Fallback { name: f.name(), bytes: f.bytes }));
    // The Kai interface faces for this language first (霞鶩文楷; Gowun Batang for Korean).
    psg_desktop_ui::fonts::use_interface_face(&mut fonts, lang);
    fonts
}

#[cfg(test)]
mod tests {
    #[test]
    fn interface_language_face_comes_first() {
        if pdfcraft_fonts::ui_chinese_fonts().is_empty() {
            eprintln!("skipping: built without craft-fonts (set CRAFT_FONTS_DIR)");
            assert!(super::fallback_names("zh-hans").iter().all(|n| !n.contains("WenKai")));
            return;
        }
        let zh = super::fallback_names("zh-hans");
        assert!(zh.first().is_some_and(|n| n.contains("WenKai")), "{zh:?}");
        let ko = super::fallback_names("ko");
        assert!(ko.first().is_some_and(|n| n.contains("Nanum")), "{ko:?}");
        let ja = super::fallback_names("ja");
        assert!(ja.first().is_some_and(|n| n.contains("BIZ")), "{ja:?}");
    }
}
