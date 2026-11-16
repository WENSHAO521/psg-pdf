//! Starting the window safely, and leaving a trace when it can't start.
//!
//! The release build has no console on Windows (`windows_subsystem = "windows"`), so before this a
//! renderer that couldn't start or a panic during start-up closed the app without a word: nothing
//! on screen, nothing on disk. Now:
//!
//! - every `log::` record and any panic go to `<data dir>/logs/psg-academic-pdf.log` (the
//!   previous run's log is kept as `psg-academic-pdf.1.log`), and to standard error;
//! - the GPU renderer gets the same Windows safeguards as PSG PDF ([`configure_gpu`]), and when
//!   it can't start at all the window is opened again with OpenGL ([`retry_with_gl`]);
//! - when no renderer starts, a message box says why and where the log is ([`report_failure`]).

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};

/// The current log file's name inside `<data dir>/logs`.
pub const LOG_FILE: &str = "psg-academic-pdf.log";
/// The previous run's log.
pub const PREVIOUS_LOG_FILE: &str = "psg-academic-pdf.1.log";
/// The log stops growing past this size, so a runaway warning can't fill the disk.
const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;

/// The log directory inside the data folder.
pub fn log_dir(data_root: &Path) -> PathBuf {
    data_root.join("logs")
}

/// Move the previous log aside and open a fresh one. Errors leave standard error as the only sink.
pub fn open_log(dir: &Path) -> std::io::Result<(PathBuf, File)> {
    std::fs::create_dir_all(dir)?;
    let current = dir.join(LOG_FILE);
    if current.exists() {
        // A failed rename only loses the older log; the new run still logs.
        let _ = std::fs::rename(&current, dir.join(PREVIOUS_LOG_FILE));
    }
    let file = File::create(&current)?;
    Ok((current, file))
}

/// One log line.
pub fn format_line(level: log::Level, target: &str, message: &str) -> String {
    let ts = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ");
    format!("{ts} {level:<5} {target}: {message}\n")
}

struct Logger {
    file: Mutex<Option<(File, u64)>>,
}

impl Logger {
    fn write(&self, line: &str) {
        let _ = std::io::stderr().lock().write_all(line.as_bytes());
        let mut guard = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((file, written)) = guard.as_mut() {
            let len = u64::try_from(line.len()).unwrap_or(u64::MAX);
            if written.saturating_add(len) <= MAX_LOG_BYTES && file.write_all(line.as_bytes()).is_ok() {
                *written = written.saturating_add(len);
                let _ = file.flush();
            }
        }
    }
}

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        enabled(metadata.level(), metadata.target())
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            self.write(&format_line(record.level(), record.target(), &record.args().to_string()));
        }
    }

    fn flush(&self) {}
}

/// Info and above from this app and PSG's own crates, warnings and errors from everything else
/// (wgpu and naga are chatty at info).
pub fn enabled(level: log::Level, target: &str) -> bool {
    let ours = ["psg_academic", "psg_desktop_ui", "pdfcraft", "eframe"].iter().any(|p| target.starts_with(p));
    level <= if ours { log::Level::Info } else { log::Level::Warn }
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

/// Install the logger and the panic hook, writing to `<data_root>/logs`. Returns the log file's
/// path when it could be opened. Calling it again does nothing.
pub fn install_logging(data_root: &Path) -> Option<PathBuf> {
    let opened = open_log(&log_dir(data_root));
    let (path, file) = match opened {
        Ok((path, file)) => (Some(path), Some((file, 0))),
        Err(e) => {
            let _ = writeln!(std::io::stderr().lock(), "psg-academic-pdf: no log file in {}: {e}", data_root.display());
            (None, None)
        }
    };
    let logger = LOGGER.get_or_init(|| Logger { file: Mutex::new(file) });
    if log::set_logger(logger).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            log::error!("panic: {info}\n{}", std::backtrace::Backtrace::force_capture());
            previous(info);
        }));
    }
    log::info!("{} {} starting on {} {}", crate::app::APP_NAME, env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH);
    path
}

/// Which renderer to start with, from `PSG_RENDERER` (or PdfCraft's `PDFCRAFT_RENDERER`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererChoice {
    /// wgpu, then OpenGL if wgpu can't start (unset, or anything unrecognised).
    Auto,
    /// wgpu only (`wgpu`).
    Wgpu,
    /// OpenGL only (`gl`, `opengl` or `glow`): for drivers where wgpu starts but misbehaves.
    Gl,
}

pub fn renderer_choice(value: Option<&str>) -> RendererChoice {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("wgpu") => RendererChoice::Wgpu,
        Some("gl" | "opengl" | "glow") => RendererChoice::Gl,
        _ => RendererChoice::Auto,
    }
}

/// The renderer choice of this run's environment.
pub fn renderer_from_env() -> RendererChoice {
    let value = std::env::var("PSG_RENDERER").ok().or_else(|| std::env::var("PDFCRAFT_RENDERER").ok());
    renderer_choice(value.as_deref())
}

/// The renderer the first attempt uses.
pub fn first_renderer(choice: RendererChoice) -> eframe::Renderer {
    if choice == RendererChoice::Gl { eframe::Renderer::Glow } else { eframe::Renderer::Wgpu }
}

/// Whether a failed run is retried with OpenGL: only when nobody chose wgpu explicitly, and the
/// failure came from wgpu before the app was created (while starting the renderer, not later).
pub fn retry_with_gl(choice: RendererChoice, error: &eframe::Error, app_started: bool) -> bool {
    choice == RendererChoice::Auto && !app_started && matches!(error, eframe::Error::Wgpu(_))
}

/// The GPU safeguards PSG PDF starts with (apps/psg-pdf/src/main.rs, `configure_gpu`):
///
/// - On Windows only Direct3D 12 and OpenGL are tried. wgpu otherwise also loads the Vulkan
///   driver, and a faulty one (an Intel driver) crashes the process before the window appears,
///   with nothing to catch.
/// - The device is created with the adapter's real 2D texture limit. egui-wgpu asks for 8192
///   pixels, so a large window on a high-DPI screen panicked in `Surface::configure` at start.
/// - wgpu's validation of indirect draws is off: nothing draws indirectly, and its shader fails to
///   compile on some GPUs (the device is lost at start).
///
/// `WGPU_BACKEND` and `WGPU_VALIDATION_INDIRECT_CALL` still override these.
pub fn configure_gpu(native: &mut eframe::NativeOptions) {
    let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut native.wgpu_options.wgpu_setup else { return };
    setup.instance_descriptor.flags = setup.instance_descriptor.flags.difference(eframe::wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL).with_env();
    let device_descriptor = std::sync::Arc::clone(&setup.device_descriptor);
    setup.device_descriptor = std::sync::Arc::new(move |adapter| {
        let mut descriptor = device_descriptor(adapter);
        descriptor.required_limits = surface_texture_limits(descriptor.required_limits, &adapter.limits());
        descriptor
    });
    if cfg!(target_os = "windows") && std::env::var_os("WGPU_BACKEND").is_none() {
        setup.instance_descriptor.backends = eframe::wgpu::Backends::DX12 | eframe::wgpu::Backends::GL;
    }
}

/// Surface textures must fit the device's enabled limits: egui-wgpu's requirements, lowered to what
/// the adapter supports, with the adapter's full 2D texture extent.
pub fn surface_texture_limits(required: eframe::wgpu::Limits, supported: &eframe::wgpu::Limits) -> eframe::wgpu::Limits {
    let mut required = required.or_worse_values_from(supported);
    required.max_texture_dimension_2d = supported.max_texture_dimension_2d;
    required
}

/// The message shown when the window could not be opened at all.
pub fn failure_message(error: &str, log: Option<&Path>) -> String {
    let mut text = format!("{} could not open its window.\n\n{error}", crate::app::APP_NAME);
    if let Some(log) = log {
        text.push_str(&format!("\n\nDetails are in the log file:\n{}", log.display()));
    }
    text.push_str("\n\nIf the graphics driver is the cause, starting with the environment variable PSG_RENDERER=gl uses OpenGL instead.");
    text
}

/// Log the failure and show it in a message box (the release build has no console on Windows).
pub fn report_failure(error: &eframe::Error, log: Option<&Path>) {
    log::error!("the window could not be opened: {error}");
    let text = failure_message(&error.to_string(), log);
    let _ = writeln!(std::io::stderr().lock(), "psg-academic-pdf: {text}");
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(crate::app::APP_NAME)
        .set_description(text)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opengl_is_the_fallback_unless_a_renderer_was_chosen() {
        assert_eq!(renderer_choice(None), RendererChoice::Auto);
        assert_eq!(renderer_choice(Some("vulkan")), RendererChoice::Auto);
        assert_eq!(renderer_choice(Some(" WGPU ")), RendererChoice::Wgpu);
        for gl in ["gl", "OpenGL", "glow"] {
            assert_eq!(renderer_choice(Some(gl)), RendererChoice::Gl);
        }
        assert_eq!(first_renderer(RendererChoice::Auto), eframe::Renderer::Wgpu);
        assert_eq!(first_renderer(RendererChoice::Gl), eframe::Renderer::Glow);
        let wgpu_failed = eframe::Error::Wgpu(eframe::egui_wgpu::WgpuError::NoSurfaceFormatsAvailable);
        assert!(retry_with_gl(RendererChoice::Auto, &wgpu_failed, false));
        assert!(!retry_with_gl(RendererChoice::Auto, &wgpu_failed, true), "a failure after start is not the renderer's");
        assert!(!retry_with_gl(RendererChoice::Wgpu, &wgpu_failed, false), "an explicit choice is respected");
        assert!(!retry_with_gl(RendererChoice::Auto, &eframe::Error::AppCreation("x".into()), false));
    }

    #[test]
    fn the_window_gets_the_windows_gpu_safeguards() {
        let native = crate::native_options(eframe::Renderer::Wgpu, None);
        let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &native.wgpu_options.wgpu_setup else {
            panic!("default setup creates its own instance")
        };
        if std::env::var_os("WGPU_VALIDATION_INDIRECT_CALL").is_none() {
            assert!(!setup.instance_descriptor.flags.contains(eframe::wgpu::InstanceFlags::VALIDATION_INDIRECT_CALL));
        }
        if cfg!(target_os = "windows") && std::env::var_os("WGPU_BACKEND").is_none() {
            let backends = setup.instance_descriptor.backends;
            assert!(backends.contains(eframe::wgpu::Backends::DX12), "{backends:?}");
            assert!(!backends.contains(eframe::wgpu::Backends::VULKAN), "a faulty Vulkan driver crashes the start: {backends:?}");
        }
    }

    #[test]
    fn large_surfaces_get_the_adapters_texture_limit() {
        let required = eframe::wgpu::Limits::default();
        let supported = eframe::wgpu::Limits { max_texture_dimension_2d: 16384, ..required.clone() };
        assert_eq!(surface_texture_limits(required.clone(), &supported).max_texture_dimension_2d, 16384);
        let small = eframe::wgpu::Limits { max_texture_dimension_2d: 4096, ..required.clone() };
        assert_eq!(surface_texture_limits(required, &small).max_texture_dimension_2d, 4096);
    }

    #[test]
    fn each_run_keeps_the_previous_log() {
        let dir = tempfile::tempdir().unwrap();
        let logs = log_dir(dir.path());
        let (path, mut file) = open_log(&logs).unwrap();
        assert_eq!(path, logs.join(LOG_FILE));
        file.write_all(b"first run\n").unwrap();
        drop(file);
        let (_, mut file) = open_log(&logs).unwrap();
        file.write_all(b"second run\n").unwrap();
        drop(file);
        assert_eq!(std::fs::read_to_string(logs.join(PREVIOUS_LOG_FILE)).unwrap(), "first run\n");
        assert_eq!(std::fs::read_to_string(logs.join(LOG_FILE)).unwrap(), "second run\n");
    }

    #[test]
    fn the_log_keeps_our_info_and_others_warnings() {
        assert!(enabled(log::Level::Info, "psg_academic_pdf::recent"));
        assert!(enabled(log::Level::Info, "eframe::native"));
        assert!(!enabled(log::Level::Info, "wgpu_core::device"));
        assert!(enabled(log::Level::Warn, "wgpu_core::device"));
        assert!(!enabled(log::Level::Debug, "psg_academic_pdf"));
        let line = format_line(log::Level::Error, "psg_academic_pdf", "panic: boom");
        assert!(line.ends_with(" ERROR psg_academic_pdf: panic: boom\n"), "{line}");
    }

    #[test]
    fn the_failure_message_says_where_the_log_is() {
        let text = failure_message("no adapter", Some(Path::new("/data/logs/psg-academic-pdf.log")));
        assert!(text.contains("no adapter") && text.contains("/data/logs/psg-academic-pdf.log") && text.contains("PSG_RENDERER=gl"), "{text}");
        assert!(!failure_message("x", None).contains("log file"));
    }
}
