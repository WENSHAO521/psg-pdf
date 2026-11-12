//! PSG PDF's name and identity, in one place. PSG PDF is Panorama Scholarly Group's edition of
//! PdfCraft (MIT OR Apache-2.0); the PDF engine underneath is PdfCraft's, unchanged.

/// The product name, in window titles, menus and the About dialog.
pub const APP_NAME: &str = "PSG PDF";
/// The organisation that publishes PSG PDF.
pub const PUBLISHER: &str = "Panorama Scholarly Group";
/// The folder name for settings, recovery, logs and caches, so PSG PDF never shares them with an
/// installed PdfCraft.
pub const DATA_FOLDER: &str = "PSG PDF";
/// The upstream project PSG PDF is based on (plain-text attribution, as its licence allows).
pub const BASED_ON: &str = "Based on PdfCraft by the ArtCraft team (MIT OR Apache-2.0).";
