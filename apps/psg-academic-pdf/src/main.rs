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
#[cfg(target_os = "macos")]
mod apple_events;
mod cli;
mod data_dir;
mod fonts;
mod i18n;
mod launch;
mod preview;
mod psg_pdf;
mod source;
mod theme;
mod updates;

/// Freedesktop app id: the Wayland app id and the hicolor icon name.
const APP_ID: &str = "com.panoramascholarly.psgacademicpdf";

/// The window icon (assets/app-icon-academic/README.md).
const APP_ICON_PNG: &[u8] = include_bytes!("../../../assets/app-icon-academic/psg-academic-pdf-256.png");

fn main() {
    // Windows: the update helper is a copy of this program started with `--finish-update`.
    #[cfg(windows)]
    if let Some(result) = craft_update::self_update::finish(&updates::UPDATE_APP, &std::env::args_os().skip(1).collect::<Vec<_>>()) {
        std::process::exit(i32::from(result.is_err()));
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(code) = cli::run(&args) {
        std::process::exit(code);
    }
    if let Err(e) = run_window(launch::paths_from_args(&args)) {
        use std::io::Write;
        let _ = writeln!(std::io::stderr().lock(), "psg-academic-pdf: {e}");
        std::process::exit(1);
    }
}

fn run_window(paths: Vec<std::path::PathBuf>) -> eframe::Result {
    // Before the event loop, so the Finder event that launched the app is caught too.
    #[cfg(target_os = "macos")]
    let apple_events = apple_events::AppleEvents::install();
    let data = data_dir::DataDir::for_this_run(app::APP_NAME);
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(theme::window_title())
        .with_app_id(APP_ID)
        .with_inner_size([1280.0, 840.0])
        .with_min_inner_size([900.0, 600.0]);
    match eframe::icon_data::from_png_bytes(APP_ICON_PNG) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => log::warn!("app icon: {e}"),
    }
    // Window settings live in the data folder, so the portable zip writes nothing elsewhere.
    let persistence_path = Some(data.root.join("app.ron"));
    let options = eframe::NativeOptions { viewport, persistence_path, ..Default::default() };
    // Borrowed, so the handlers stay registered until the event loop returns.
    #[cfg(target_os = "macos")]
    let apple_events = &apple_events;
    eframe::run_native(
        app::APP_NAME,
        options,
        Box::new(move |cc| {
            #[allow(unused_mut)]
            let mut app = app::AcademicApp::new(&cc.egui_ctx, data, paths);
            #[cfg(target_os = "macos")]
            app.set_os_events(apple_events.connect(&cc.egui_ctx));
            Ok(Box::new(app))
        }),
    )
}
