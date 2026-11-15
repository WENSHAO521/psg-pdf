//! The PSG design system: what PSG PDF and PSG Academic PDF share, and what tells them apart.
//!
//! Both products take their names, brand colours, shape and spacing tokens and the release label
//! ("Beta") from here, so the two apps, their About windows and their window titles agree. The
//! written guide, with the logo rules, is `docs/brand.md`. Product identity used by installers
//! (app ids, upgrade codes, package names) lives in `packaging/products.toml`; the colours below
//! are checked against that file by this crate's tests.
//!
//! Colours are plain sRGB triples so every crate can use them without a UI dependency.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

/// An sRGB colour, `[r, g, b]`.
pub type Rgb = [u8; 3];

/// The organisation that publishes both products.
pub const PUBLISHER: &str = "Panorama Scholarly Group";
/// The short form of [`PUBLISHER`], as in the product names.
pub const PUBLISHER_SHORT: &str = "PSG";
/// The repository both products are developed and released from.
pub const REPOSITORY: &str = "https://github.com/WENSHAO521/psg-pdf";
/// Where both products' installers are published.
pub const RELEASES: &str = "https://github.com/WENSHAO521/psg-pdf/releases";
/// Where problems are reported.
pub const ISSUES: &str = "https://github.com/WENSHAO521/psg-pdf/issues";

/// The logo's shared gold line work (both products).
pub const GOLD: Rgb = [0xd8, 0xae, 0x5c];
/// The logo's shared cream (paper) colour (both products).
pub const CREAM: Rgb = [0xf5, 0xef, 0xe1];

/// One of the two PSG products.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Product {
    /// PSG PDF: professional PDF reading, editing and management.
    Pdf,
    /// PSG Academic PDF: scholarly typesetting and publication-grade PDF production.
    Academic,
}

impl Product {
    /// Both products, in the order they are presented.
    pub const ALL: [Product; 2] = [Product::Pdf, Product::Academic];

    /// The product id in `packaging/products.toml` (also the Cargo package and release file stem).
    pub const fn id(self) -> &'static str {
        match self {
            Product::Pdf => "psg-pdf",
            Product::Academic => "psg-academic-pdf",
        }
    }

    /// The product name, in window titles, menus, installers and the About window.
    pub const fn name(self) -> &'static str {
        match self {
            Product::Pdf => "PSG PDF",
            Product::Academic => "PSG Academic PDF",
        }
    }

    /// The one-line description used by installers, shortcuts and the update dialog
    /// (`tagline` in `packaging/products.toml`).
    pub const fn tagline(self) -> &'static str {
        match self {
            Product::Pdf => "Reading, organizing and protecting PDFs",
            Product::Academic => "Typesetting accepted articles with pdfLaTeX",
        }
    }

    /// The brand colour (`accent` in `packaging/products.toml`): deep blue for PSG PDF, academic
    /// purple for PSG Academic PDF. It fills the app icon tile and marks the product in documents.
    pub const fn brand(self) -> Rgb {
        match self {
            Product::Pdf => [0x1e, 0x43, 0x84],
            Product::Academic => [0x5b, 0x2a, 0x78],
        }
    }

    /// The deep end of the icon tile's gradient.
    pub const fn brand_deep(self) -> Rgb {
        match self {
            Product::Pdf => [0x0b, 0x1d, 0x45],
            Product::Academic => [0x26, 0x10, 0x3a],
        }
    }

    /// The interface accent colours of this product in the light or dark theme.
    pub const fn accent(self, dark: bool) -> Accent {
        match (self, dark) {
            (Product::Pdf, false) => Accent {
                accent: [0x1e, 0x43, 0x84],
                accent_text: [0x1e, 0x43, 0x84],
                accent_soft: [0xe5, 0xeb, 0xf5],
                selected: [0xdc, 0xe5, 0xf3],
                on_accent: [0xff, 0xff, 0xff],
            },
            (Product::Pdf, true) => Accent {
                accent: [0x44, 0x72, 0xc4],
                accent_text: [0xa9, 0xc1, 0xec],
                accent_soft: [0x1f, 0x2d, 0x47],
                selected: [0x24, 0x36, 0x5a],
                on_accent: [0xff, 0xff, 0xff],
            },
            (Product::Academic, false) => Accent {
                accent: [0x5b, 0x2a, 0x78],
                accent_text: [0x5b, 0x2a, 0x78],
                accent_soft: [0xef, 0xe7, 0xf4],
                selected: [0xe6, 0xda, 0xee],
                on_accent: [0xff, 0xff, 0xff],
            },
            (Product::Academic, true) => Accent {
                accent: [0x9a, 0x55, 0xc0],
                accent_text: [0xcd, 0xb0, 0xe2],
                accent_soft: [0x2f, 0x22, 0x38],
                selected: [0x3a, 0x28, 0x48],
                on_accent: [0xff, 0xff, 0xff],
            },
        }
    }

    /// The window title for a build of `version`: the product name, then the release label when
    /// the build is not a stable release ("PSG PDF Beta").
    pub fn window_title(self, version: &str) -> String {
        match Release::of(version).badge() {
            Some(badge) => format!("{} {badge}", self.name()),
            None => self.name().to_owned(),
        }
    }
}

/// A product's interface accent in one theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Accent {
    /// Primary buttons, focus rings, the active tab marker, sliders, progress.
    pub accent: Rgb,
    /// Links and accent-coloured text (meets 4.5:1 on the theme's panels).
    pub accent_text: Rgb,
    /// A soft wash behind an accent control (selected toggle, hovered primary item).
    pub accent_soft: Rgb,
    /// Selected rows and text selection.
    pub selected: Rgb,
    /// Text and icons drawn on [`Accent::accent`]: white in both themes, so the dark theme's
    /// accent is a mid tone that keeps 4.5:1 with white and 3:1 with the dark panels.
    pub on_accent: Rgb,
}

/// Neutral surfaces both products share, so the two apps look like one family.
pub mod neutral {
    use super::Rgb;

    /// Light theme panels.
    pub const LIGHT_PANEL: Rgb = [0xff, 0xff, 0xff];
    /// Light theme window background behind panels.
    pub const LIGHT_CANVAS: Rgb = [0xf1, 0xf1, 0xf3];
    /// Dark theme panels.
    pub const DARK_PANEL: Rgb = [0x26, 0x26, 0x2a];
    /// Dark theme window background behind panels.
    pub const DARK_CANVAS: Rgb = [0x19, 0x19, 0x1c];
}

/// Corner radii, in logical pixels.
pub mod radius {
    /// Badges, tags, small chips.
    pub const SMALL: u8 = 4;
    /// Buttons, fields, menus, toolbar controls.
    pub const CONTROL: u8 = 6;
    /// Cards, panels, popovers.
    pub const CARD: u8 = 8;
    /// Dialogs and windows drawn by the app.
    pub const DIALOG: u8 = 10;
}

/// The spacing scale, in logical pixels: every margin and gap is one of these.
pub mod space {
    pub const XS: f32 = 4.0;
    pub const S: f32 = 8.0;
    pub const M: f32 = 12.0;
    pub const L: f32 = 16.0;
    pub const XL: f32 = 24.0;
    pub const XXL: f32 = 32.0;
}

/// Type sizes, in points: Inter for the interface, JetBrains Mono for code and logs.
pub mod text {
    pub const SMALL: f32 = 12.0;
    pub const BODY: f32 = 13.0;
    pub const HEADING: f32 = 15.0;
    pub const TITLE: f32 = 20.0;
    pub const DISPLAY: f32 = 28.0;
}

/// What kind of release a build is, from its version (`CARGO_PKG_VERSION`). CI sets beta builds
/// to `X.Y.Z-beta.N` (`cargo xtask version set`, packaging/release-tag.sh) and stable releases
/// to `X.Y.Z`; anything else is a local or development build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Release {
    /// `X.Y.Z`: a stable release.
    Stable,
    /// `X.Y.Z-beta.N`: a beta from the `beta` branch.
    Beta(Option<u32>),
    /// Any other pre-release or build label: a development build.
    Development,
}

impl Release {
    /// The release kind of `version`.
    pub fn of(version: &str) -> Release {
        let core = version.split('+').next().unwrap_or(version);
        let Some((_, pre)) = core.split_once('-') else {
            return if version.contains('+') { Release::Development } else { Release::Stable };
        };
        match pre.strip_prefix("beta") {
            Some("") => Release::Beta(None),
            Some(rest) => match rest.strip_prefix('.').map(str::parse::<u32>) {
                Some(Ok(n)) => Release::Beta(Some(n)),
                _ => Release::Development,
            },
            None => Release::Development,
        }
    }

    /// The badge shown beside the product name, in the window title, the About window and the
    /// home page: none for a stable release. Badges are drawn in [`GOLD`] in both products.
    pub fn badge(self) -> Option<&'static str> {
        match self {
            Release::Stable => None,
            Release::Beta(_) => Some("Beta"),
            Release::Development => Some("Development build"),
        }
    }
}

/// `version` as people read it: `0.19.0`, `0.19.0 Beta 3`, or the version unchanged for a
/// development build.
pub fn display_version(version: &str) -> String {
    match Release::of(version) {
        Release::Beta(Some(n)) => format!("{} Beta {n}", version.split('-').next().unwrap_or(version)),
        Release::Beta(None) => format!("{} Beta", version.split('-').next().unwrap_or(version)),
        Release::Stable | Release::Development => version.to_owned(),
    }
}

#[cfg(test)]
mod tests;
