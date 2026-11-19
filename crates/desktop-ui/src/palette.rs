//! The resolved colours of one product in one theme, as egui colours.

use egui::Color32;
use psg_brand::{Product, Tone};

/// An sRGB token as an egui colour.
pub const fn colour([r, g, b]: psg_brand::Rgb) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Every colour a component needs, for one product in one theme.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub product: Product,
    pub dark: bool,
    pub titlebar: Color32,
    pub chrome: Color32,
    pub panel: Color32,
    pub canvas: Color32,
    pub card: Color32,
    pub border: Color32,
    pub divider: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_faint: Color32,
    pub icon: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub field: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub accent_soft: Color32,
    pub selected: Color32,
    pub on_accent: Color32,
    /// The shared PSG gold (release badge, logo line work).
    pub gold: Color32,
    /// Drop shadow under pages, cards and popups.
    pub shadow: Color32,
}

/// One tone's colours as egui colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneColours {
    pub text: Color32,
    pub soft: Color32,
    pub solid: Color32,
}

impl Palette {
    /// The palette of `product` in the light (`dark == false`) or dark theme.
    pub const fn new(product: Product, dark: bool) -> Self {
        let s = psg_brand::surfaces(dark);
        let a = product.accent(dark);
        Self {
            product,
            dark,
            titlebar: colour(s.titlebar),
            chrome: colour(s.chrome),
            panel: colour(s.panel),
            canvas: colour(s.canvas),
            card: colour(s.card),
            border: colour(s.border),
            divider: colour(s.divider),
            text: colour(s.text),
            text_muted: colour(s.text_muted),
            text_faint: colour(s.text_faint),
            icon: colour(s.icon),
            hover: colour(s.hover),
            pressed: colour(s.pressed),
            field: colour(s.field),
            accent: colour(a.accent),
            accent_text: colour(a.accent_text),
            accent_soft: colour(a.accent_soft),
            selected: colour(a.selected),
            on_accent: colour(a.on_accent),
            gold: colour(psg_brand::GOLD),
            shadow: if dark { Color32::from_black_alpha(120) } else { Color32::from_black_alpha(34) },
        }
    }

    /// The palette in use where `ui` draws: the product the app installed
    /// ([`crate::style::install`]) in the theme egui is showing.
    pub fn of(ui: &egui::Ui) -> Self {
        Self::new(crate::style::product(ui.ctx()), ui.visuals().dark_mode)
    }

    /// The palette for a context's current theme.
    pub fn of_ctx(ctx: &egui::Context) -> Self {
        Self::new(crate::style::product(ctx), ctx.global_style().visuals.dark_mode)
    }

    /// The colours of `tone` in this palette's product and theme.
    pub const fn tone(&self, tone: Tone) -> ToneColours {
        let c = tone.colours(self.product, self.dark);
        ToneColours { text: colour(c.text), soft: colour(c.soft), solid: colour(c.solid) }
    }
}
