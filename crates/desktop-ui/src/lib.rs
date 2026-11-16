//! The PSG Desktop Design System: one egui implementation of the PSG look for both desktop apps.
//!
//! [`psg_brand`] holds the tokens as plain data (colours, radii, spacing, sizes, tones), so the
//! installers, the website and the tests can read them too. This crate turns those tokens into
//! egui: the style of both light and dark themes ([`style::install`]), the shared interface fonts
//! ([`fonts`]), and the components both workspaces are built from ([`widgets`], [`layout`],
//! [`notice`]). PSG PDF (`crates/ui-egui`) and PSG Academic PDF (`apps/psg-academic-pdf`) use it,
//! so a button, a badge, a notification or a split pane looks and behaves the same in both, and
//! only the accent colour tells the products apart. The written guide is `docs/design-system.md`.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod fonts;
pub mod keys;
pub mod layout;
pub mod notice;
pub mod palette;
pub mod style;
pub mod widgets;

pub use palette::Palette;
pub use psg_brand::{Product, Tone};
pub use psg_i18n::ui_text;

#[cfg(test)]
mod tests;
