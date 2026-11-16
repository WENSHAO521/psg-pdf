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
mod outline;
mod preview;
mod psg_pdf;
mod recent;
mod source;
mod startup;
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
    let data = data_dir::DataDir::for_this_run(app::APP_NAME);
    let log = startup::install_logging(&data.root);
    if let Err(e) = run_window(data, launch::paths_from_args(&args)) {
        startup::report_failure(&e, log.as_deref());
        std::process::exit(1);
    }
}

/// The window and renderer settings for one attempt. `persistence_path` keeps the window settings
/// in the data folder, so the portable zip writes nothing elsewhere.
fn native_options(renderer: eframe::Renderer, persistence_path: Option<std::path::PathBuf>) -> eframe::NativeOptions {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(theme::window_title())
        .with_app_id(APP_ID)
        .with_inner_size([1280.0, 840.0])
        .with_min_inner_size([900.0, 600.0]);
    match eframe::icon_data::from_png_bytes(APP_ICON_PNG) {
        Ok(icon) => viewport = viewport.with_icon(icon),
        Err(e) => log::warn!("app icon: {e}"),
    }
    // Set explicitly, so the renderer never depends on which one eframe defaults to.
    let mut native = eframe::NativeOptions { viewport, persistence_path, renderer, ..Default::default() };
    if renderer == eframe::Renderer::Wgpu {
        startup::configure_gpu(&mut native);
    }
    native
}

fn run_window(data: data_dir::DataDir, paths: Vec<std::path::PathBuf>) -> eframe::Result {
    // Before the event loop, so the Finder event that launched the app is caught too.
    #[cfg(target_os = "macos")]
    let apple_events = apple_events::AppleEvents::install();
    let persistence_path = Some(data.root.join("app.ron"));
    let choice = startup::renderer_from_env();
    let first = startup::first_renderer(choice);
    // Set once the app is created, which is after the renderer has started.
    let started = std::rc::Rc::new(std::cell::Cell::new(false));
    // Borrowed, so the handlers stay registered until the event loop returns.
    #[cfg(target_os = "macos")]
    let apple_events = &apple_events;
    let creator = |data: data_dir::DataDir, paths: Vec<std::path::PathBuf>, started: std::rc::Rc<std::cell::Cell<bool>>| -> eframe::AppCreator<'_> {
        Box::new(move |cc| {
            started.set(true);
            log::info!("window open; renderer: {}", if cc.wgpu_render_state.is_some() { "wgpu" } else { "OpenGL" });
            #[allow(unused_mut)]
            let mut app = app::AcademicApp::new(&cc.egui_ctx, data, paths);
            #[cfg(target_os = "macos")]
            app.set_os_events(apple_events.connect(&cc.egui_ctx));
            Ok(Box::new(app))
        })
    };
    let result = eframe::run_native(
        app::APP_NAME,
        native_options(first, persistence_path.clone()),
        creator(data.clone(), paths.clone(), std::rc::Rc::clone(&started)),
    );
    match result {
        Err(e) if startup::retry_with_gl(choice, &e, started.get()) => {
            // Old or unusual GPUs and drivers can't give wgpu a device; OpenGL usually still works.
            log::error!("the GPU renderer (wgpu) didn't start: {e}. Starting with OpenGL instead; set PSG_RENDERER=gl to skip wgpu.");
            eframe::run_native(app::APP_NAME, native_options(eframe::Renderer::Glow, persistence_path), creator(data, paths, started))
        }
        other => other,
    }
}
