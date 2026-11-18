//! Trace the glyphs of each text-showing operator using the same font and positioning as the viewer.

use std::io::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use hayro::hayro_interpret::font::Glyph;
use hayro::hayro_interpret::hayro_syntax::Pdf;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, GlyphDrawMode, Image, InterpreterCache, InterpreterSettings, InterpreterWarning, Paint, PathDrawMode,
    SoftMask, TransformExt, interpret_page,
};
use kurbo::{Affine, BezPath, PathEl, Point, Rect};

use crate::RenderConfig;

const MAX_OUTLINE_BYTES: usize = 32 * 1024 * 1024;
const MAX_SHOWS: usize = 100_000;

/// The vector drawing in local user coordinates, and whether the renderer saw any glyphs.
#[derive(Default)]
pub struct TextShowOutline {
    pub drawing: Vec<u8>,
    pub has_glyphs: bool,
}

/// A file parsed once while its pages are traced. Font substitution uses the bundled faces,
/// never a font installed on the user's machine.
pub struct TextOutliner(Pdf);

impl TextOutliner {
    pub fn new(bytes: Arc<Vec<u8>>) -> Result<Self, String> {
        Pdf::new(bytes).map(Self).map_err(|_| "The PDF could not be read for text outlining.".into())
    }

    pub fn trace_page(&self, index: usize) -> Result<Vec<TextShowOutline>, String> {
        let page = self.0.pages().get(index).ok_or("The page could not be read.")?;
        let (width, height) = page.render_dimensions();
        let incomplete = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&incomplete);
        let settings = InterpreterSettings {
            render_annotations: false,
            warning_sink: Arc::new(move |warning| {
                if matches!(warning, InterpreterWarning::ContentTruncated | InterpreterWarning::UnsupportedFont) {
                    flag.store(true, Ordering::Relaxed);
                }
            }),
            ..RenderConfig::default().settings()
        };
        let cache = InterpreterCache::new();
        let mut context = Context::new(
            page.initial_transform(true).to_kurbo(),
            Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
            &cache,
            page.xref(),
            settings,
        );
        let mut device = Outlines::default();
        interpret_page(page, &mut context, &mut device);
        if let Some(error) = device.error {
            return Err(error.into());
        }
        if incomplete.load(Ordering::Relaxed) {
            return Err("A font or content stream could not be interpreted completely.".into());
        }
        Ok(device.shows)
    }
}

#[derive(Default)]
struct Outlines {
    shows: Vec<TextShowOutline>,
    error: Option<&'static str>,
    bytes: usize,
}

impl Device<'_> for Outlines {
    fn begin_text_show(&mut self) {
        if self.shows.len() >= MAX_SHOWS {
            self.error = Some("Too many text operations to outline safely.");
        }
        if self.error.is_none() {
            self.shows.push(TextShowOutline::default());
        }
    }

    fn draw_glyph(&mut self, glyph: &Glyph<'_>, _: Affine, glyph_transform: Affine, _: &Paint<'_>, mode: &GlyphDrawMode) {
        if self.error.is_some() {
            return;
        }
        let Some(show) = self.shows.last_mut() else {
            self.error = Some("Text was drawn outside a supported text operation.");
            return;
        };
        show.has_glyphs = true;
        if matches!(mode, GlyphDrawMode::Invisible) {
            return;
        }
        let Glyph::Outline(glyph) = glyph else {
            self.error = Some("This page uses a Type 3 font whose glyphs cannot be converted to outlines.");
            return;
        };
        let paint: &[u8] = match mode {
            GlyphDrawMode::Fill => b"f\n",
            GlyphDrawMode::Stroke(_) => b"S\n",
            _ => {
                self.error = Some("This page uses text as a clipping path.");
                return;
            }
        };

        // The stream keeps its original graphics transformation. Only the glyph-to-user-space
        // transformation belongs in the path coordinates.
        let path = glyph_transform * glyph.outline();
        if path.elements().is_empty() {
            return;
        }
        let start = show.drawing.len();
        if !write_curve(&path, &mut show.drawing, MAX_OUTLINE_BYTES.saturating_sub(self.bytes)) {
            self.error = Some("A glyph path is invalid or exceeds the page's outline size limit.");
            return;
        }
        show.drawing.extend_from_slice(paint);
        self.bytes += show.drawing.len() - start;
    }

    fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn draw_path(&mut self, _: &BezPath, _: Affine, _: &Paint<'_>, _: &PathDrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'_, '_>, _: Affine) {}
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}
}

fn write_curve(path: &BezPath, out: &mut Vec<u8>, budget: usize) -> bool {
    let start = out.len();
    let mut prev = Point::ZERO;
    let mut subpath = Point::ZERO;
    for element in path.elements() {
        let (points, count, op) = match *element {
            PathEl::MoveTo(point) => {
                subpath = point;
                ([point; 3], 1, b"m\n")
            }
            PathEl::LineTo(point) => ([point; 3], 1, b"l\n"),
            PathEl::QuadTo(control, end) => ([prev.lerp(control, 2.0 / 3.0), end.lerp(control, 2.0 / 3.0), end], 3, b"c\n"),
            PathEl::CurveTo(a, b, end) => ([a, b, end], 3, b"c\n"),
            PathEl::ClosePath => {
                prev = subpath;
                ([subpath; 3], 0, b"h\n")
            }
        };
        for point in points.into_iter().take(count) {
            if !point.is_finite() || point.x.abs().max(point.y.abs()) > 1.0e9 || write!(out, "{:.8} {:.8} ", point.x, point.y).is_err() {
                return false;
            }
            prev = point;
        }
        out.extend_from_slice(op);
        if out.len() - start > budget {
            return false;
        }
    }
    true
}
