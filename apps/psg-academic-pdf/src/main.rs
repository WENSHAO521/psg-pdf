//! PSG Academic PDF: offline publication typesetting of accepted articles.
//!
//! A journal template, the publication metadata and the author's manuscript go in; latexmk and
//! pdfLaTeX from the local TeX installation typeset them; the PDF is previewed with the shared
//! PdfCraft renderer and exported. See docs/academic.md.
//!
//! `psg-academic-pdf --help` lists the headless commands (`check-tex`, `templates`, `new`,
//! `compile`), which use the same code as the window.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod app;
mod cli;
mod preview;

/// Freedesktop app id: the Wayland app id and the hicolor icon name.
const APP_ID: &str = "com.panoramascholarly.psgacademicpdf";

/// The window icon (assets/app-icon-academic/README.md).
const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon-academic/psg-academic-pdf-256.png");

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::run(&args) {
        std::process::exit(code);
    }
    if let Err(e) = run_window() {
        use std::io::Write;
        let _ = writeln!(std::io::stderr().lock(), "psg-academic-pdf: {e}");
        std::process::exit(1);
    }
}

fn run_window() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(app::APP_NAME)
        .with_app_id(APP_ID)
        .with_inner_size([1280.0, 840.0])
        .with_min_inner_size([900.0, 600.0]);
    match eframe::icon_data::from_png_bytes(APP_ICON_PNG) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => log::warn!("app icon: {e}"),
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(app::APP_NAME, options, Box::new(|cc| Ok(Box::new(app::AcademicApp::new(&cc.egui_ctx)))))
}
