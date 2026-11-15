//! Typed signatures and bounded glyph outlines from approved fonts (bundled, or craft-fonts).

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};

static FONT: &[u8] = include_bytes!("../../../assets/fonts/DancingScript.ttf");

/// Inter Regular, the interface font. Typed signatures fall back to it, slanted, for characters
/// the script font lacks (Dancing Script is Latin and Vietnamese only: no Cyrillic or Greek).
pub static INTER_REGULAR: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");

/// Forward slant of fallback glyphs in a typed signature (about 12°), so they lean like the script.
const FALLBACK_SLANT: f64 = 0.21;

/// Bound signature work while allowing long personal names.
pub const MAX_SIGNATURE_CHARS: usize = 256;

/// Outlines of a line of existing text at a font size of 1 (em units).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScriptOutline {
    pub contours: Vec<Vec<[f64; 2]>>,
    pub width: f64,
    pub ascent: f64,
    pub descent: f64,
}

impl ScriptOutline {
    /// Layout bounds including glyph overhangs, so previews and PDF appearances never clip ink.
    pub fn bounds(&self) -> [f64; 4] {
        self.contours.iter().flatten().fold([0.0, self.descent, self.width, self.ascent], |mut b, p| {
            b[0] = b[0].min(p[0]);
            b[1] = b[1].min(p[1]);
            b[2] = b[2].max(p[0]);
            b[3] = b[3].max(p[1]);
            b
        })
    }
}

/// One glyph outline in em units, with the baseline at y = 0.
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphOutline {
    pub contours: Vec<Vec<[f64; 2]>>,
    pub width: f64,
    pub bbox: [f64; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlyphError {
    /// This build has no Japanese face: it was built without craft-fonts (`CRAFT_FONTS_DIR`).
    NoFont,
    Missing,
    TooComplex,
}

struct Flatten {
    contours: Vec<Vec<[f64; 2]>>,
    cur: Vec<[f64; 2]>,
    scale: f64,
    /// Horizontal shear applied to every point (x += slant × y), for slanted fallback glyphs.
    slant: f64,
    dx: f64,
    /// Vertical offset of the glyph being drawn (shaped marks sit above or below the baseline).
    dy: f64,
    points: usize,
    too_complex: bool,
    max_points: usize,
}

impl Flatten {
    fn new(scale: f64) -> Self {
        Self { contours: Vec::new(), cur: Vec::new(), scale, slant: 0.0, dx: 0.0, dy: 0.0, points: 0, too_complex: false, max_points: 4096 }
    }

    fn pt(&self, x: f32, y: f32) -> [f64; 2] {
        let (x, y) = (x as f64 * self.scale, y as f64 * self.scale);
        [self.dx + x + self.slant * y, self.dy + y]
    }

    fn last(&self) -> [f64; 2] {
        self.cur.last().copied().unwrap_or([self.dx, self.dy])
    }

    fn push(&mut self, p: [f64; 2]) {
        if self.points >= self.max_points {
            self.too_complex = true;
            return;
        }
        self.points += 1;
        self.cur.push(p);
    }

    fn close(&mut self) {
        if self.cur.len() > 2 {
            self.contours.push(std::mem::take(&mut self.cur));
        } else {
            self.cur.clear();
        }
    }
}

const STEPS: usize = 8;

impl OutlinePen for Flatten {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        self.push(self.pt(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.push(self.pt(x, y));
    }

    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        let (p0, c, p) = (self.last(), self.pt(cx, cy), self.pt(x, y));
        for i in 1..=STEPS {
            let t = i as f64 / STEPS as f64;
            let u = 1.0 - t;
            self.push([u * u * p0[0] + 2.0 * u * t * c[0] + t * t * p[0], u * u * p0[1] + 2.0 * u * t * c[1] + t * t * p[1]]);
        }
    }

    fn curve_to(&mut self, c0x: f32, c0y: f32, c1x: f32, c1y: f32, x: f32, y: f32) {
        let (p0, c0, c1, p) = (self.last(), self.pt(c0x, c0y), self.pt(c1x, c1y), self.pt(x, y));
        for i in 1..=STEPS {
            let t = i as f64 / STEPS as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.push([a * p0[0] + b * c0[0] + c * c1[0] + d * p[0], a * p0[1] + b * c0[1] + c * c1[1] + d * p[1]]);
        }
    }

    fn close(&mut self) {
        Flatten::close(self);
    }
}

/// Whether the face has a glyph for `ch` (a cheap character-map probe, no outline work).
pub fn face_has(bytes: &[u8], ch: char) -> bool {
    FontRef::new(bytes).is_ok_and(|font| font.charmap().map(ch).is_some())
}

/// Return one glyph of `face` (a craft-fonts face), bounded so hostile replacement text cannot
/// allocate unbounded outline data.
pub fn craft_glyph(face: &crate::CraftFont, ch: char) -> Result<GlyphOutline, GlyphError> {
    glyph_in_face(face.bytes, ch)
}

/// Return one glyph of the Japanese document face ([`crate::document_japanese_font`], Shippori
/// Mincho from craft-fonts), bounded so hostile replacement text cannot allocate unbounded
/// outline data. [`GlyphError::NoFont`] when the build has no Japanese face.
pub fn japanese_glyph(ch: char) -> Result<GlyphOutline, GlyphError> {
    let Some(face) = crate::document_japanese_font() else { return Err(GlyphError::NoFont) };
    japanese_glyph_from(face, ch)
}

/// The bounded outline and advance from the explicitly selected craft-fonts face.
/// Uses the same complexity and missing-glyph checks as [`japanese_glyph`].
pub fn japanese_glyph_from(face: &crate::CraftFont, ch: char) -> Result<GlyphOutline, GlyphError> {
    glyph_in_face(face.bytes, ch)
}

fn glyph_in_face(bytes: &[u8], ch: char) -> Result<GlyphOutline, GlyphError> {
    let Ok(font) = FontRef::new(bytes) else { return Err(GlyphError::Missing) };
    let loc = LocationRef::default();
    let metrics = font.metrics(Size::unscaled(), loc);
    let scale = 1.0 / metrics.units_per_em.max(1) as f64;
    let Some(gid) = font.charmap().map(ch) else { return Err(GlyphError::Missing) };
    let advances = font.glyph_metrics(Size::unscaled(), loc);
    let width = advances.advance_width(gid).unwrap_or(0.0) as f64 * scale;
    if !width.is_finite() || width <= 0.0 || width > 2.0 {
        return Err(GlyphError::Missing);
    }
    bounded_outline(&font, gid, width)
}

/// Glyph `gid` of `font` in em units with advance `width`, bounded like [`japanese_glyph`].
pub(crate) fn bounded_outline(font: &FontRef, gid: GlyphId, width: f64) -> Result<GlyphOutline, GlyphError> {
    let loc = LocationRef::default();
    let scale = 1.0 / font.metrics(Size::unscaled(), loc).units_per_em.max(1) as f64;
    let Some(glyph) = font.outline_glyphs().get(gid) else { return Err(GlyphError::Missing) };
    let mut pen = Flatten::new(scale);
    let _ = glyph.draw(DrawSettings::unhinted(Size::unscaled(), loc), &mut pen);
    pen.close();
    if pen.too_complex || pen.contours.len() > 256 {
        return Err(GlyphError::TooComplex);
    }
    let mut bbox = [0.0, 0.0, width, 0.0];
    let mut any = false;
    for p in pen.contours.iter().flatten() {
        if !p[0].is_finite() || !p[1].is_finite() {
            return Err(GlyphError::Missing);
        }
        if !any {
            bbox = [p[0], p[1], p[0], p[1]];
            any = true;
        } else {
            bbox[0] = bbox[0].min(p[0]);
            bbox[1] = bbox[1].min(p[1]);
            bbox[2] = bbox[2].max(p[0]);
            bbox[3] = bbox[3].max(p[1]);
        }
    }
    Ok(GlyphOutline { contours: pen.contours, width, bbox })
}

/// One face typed signatures can draw from, tried in this order: the script font, Inter (slanted),
/// then the approved craft-fonts faces for scripts neither has. Characters come from the first face
/// that has them, so a Latin name keeps its script look and a Chinese, Japanese, Korean, Arabic or
/// Hebrew name is drawn from an OFL face shipped with the app, never from a font on the machine.
struct SigFace {
    font: FontRef<'static>,
    /// Em units to signature units: scaled so the face's capitals are as tall as the script's.
    scale: f64,
    slant: f64,
    /// The bundled faces are drawn a character at a time (as before); craft faces are shaped.
    shaper: Option<harfrust::ShaperData>,
}

/// The craft-fonts faces typed signatures fall back to, in order: a handwritten-style Chinese face
/// (LXGW WenKai) first, then the other Chinese faces, Japanese, Korean, then the shaped scripts.
/// Empty without craft-fonts (`CRAFT_FONTS_DIR`).
fn signature_craft_faces() -> Vec<&'static crate::CraftFont> {
    signature_craft_order(crate::CRAFT_FONTS.iter())
}

fn signature_craft_order<'a>(faces: impl IntoIterator<Item = &'a crate::CraftFont>) -> Vec<&'a crate::CraftFont> {
    let rank = |f: &crate::CraftFont| -> Option<u8> {
        if f.style != "Regular" {
            return None;
        }
        let han = f.covers("Hans") || f.covers("Hani") || f.covers("Hant");
        Some(if han && f.family.contains("WenKai") {
            0
        } else if han {
            1
        } else if f.covers("Jpan") {
            2
        } else if f.covers("Hang") || f.covers("Kore") {
            3
        } else if f.covers("Arab") || f.covers("Hebr") || f.covers("Thai") || f.covers("Deva") || f.covers("Geor") {
            4
        } else {
            return None;
        })
    };
    let mut out: Vec<(u8, &crate::CraftFont)> = faces.into_iter().filter_map(|f| rank(f).map(|r| (r, f))).collect();
    // Stable: manifest order within each rank (BIZ UDPGothic before the Mincho faces is not needed
    // here; a signature reads better in the first face the manifest lists).
    out.sort_by_key(|(r, _)| *r);
    out.into_iter().map(|(_, f)| f).collect()
}

/// The faces of [`SigFace`] in order, scaled against the script font's capital height. Built once:
/// the embedded fonts never change while the app runs.
fn signature_faces() -> &'static [SigFace] {
    static FACES: std::sync::OnceLock<Vec<SigFace>> = std::sync::OnceLock::new();
    FACES.get_or_init(build_signature_faces)
}

fn build_signature_faces() -> Vec<SigFace> {
    let loc = LocationRef::default();
    let Ok(script) = FontRef::new(FONT) else { return Vec::new() };
    let sm = script.metrics(Size::unscaled(), loc);
    let script_em = 1.0 / sm.units_per_em.max(1) as f64;
    let script_cap = sm.cap_height.map(|c| c as f64 * script_em).filter(|c| c.is_finite() && *c > 0.0).unwrap_or(0.7);
    let matched = |font: &FontRef| {
        let m = font.metrics(Size::unscaled(), loc);
        let em = 1.0 / m.units_per_em.max(1) as f64;
        let ratio = match m.cap_height {
            Some(theirs) if theirs > 0.0 => script_cap / (theirs as f64 * em),
            // Faces with no capitals (Arabic, Hebrew): their x-height-ish body at 0.7 em.
            _ => script_cap / 0.7,
        };
        em * if ratio.is_finite() { ratio.clamp(0.5, 2.0) } else { 1.0 }
    };
    let mut faces = vec![SigFace { font: script.clone(), scale: script_em, slant: 0.0, shaper: None }];
    if let Ok(inter) = FontRef::new(INTER_REGULAR) {
        faces.push(SigFace { scale: matched(&inter), font: inter, slant: FALLBACK_SLANT, shaper: None });
    }
    for face in signature_craft_faces() {
        if let Ok(font) = FontRef::new(face.bytes) {
            let shaper = Some(harfrust::ShaperData::new(&font));
            faces.push(SigFace { scale: matched(&font), font, slant: 0.0, shaper });
        }
    }
    faces
}

/// Typed signatures are laid out in Unicode Normalization Form C, so a name typed with combining
/// accents (e + U+0301, as some input methods send it) uses the precomposed letter the fonts draw.
fn normalized(text: &str) -> String {
    use unicode_normalization::UnicodeNormalization;
    text.nfc().collect()
}

/// Characters that belong to the run around them instead of choosing a face of their own: white
/// space, combining marks and the joiners.
fn joins_run(c: char) -> bool {
    c.is_whitespace() || unicode_normalization::char::is_combining_mark(c) || matches!(c, '\u{200c}' | '\u{200d}')
}

/// The face index for each character of `text`, `None` for characters no face has.
fn faces_for(text: &str, faces: &[SigFace]) -> Vec<Option<usize>> {
    let maps: Vec<_> = faces.iter().map(|f| f.font.charmap()).collect();
    let mut prev: Option<usize> = None;
    text.chars()
        .map(|c| {
            if joins_run(c)
                && let Some(p) = prev
                && maps.get(p).is_some_and(|m| m.map(c).is_some())
            {
                return Some(p);
            }
            let pick = maps.iter().position(|m| m.map(c).is_some());
            if pick.is_some() {
                prev = pick;
            }
            pick
        })
        .collect()
}

/// The first character of `text` (other than white space) that no face typed signatures draw from
/// has a glyph for: [`script_outline`] would leave it out. `None` when they can draw all of them.
/// Chinese, Japanese, Korean and the shaped scripts need the craft-fonts build input.
pub fn script_missing(text: &str) -> Option<char> {
    let text = normalized(text);
    let faces = signature_faces();
    text.chars()
        .zip(faces_for(&text, faces))
        .find(|(c, f)| f.is_none() && !c.is_whitespace() && !matches!(c, '\u{200c}' | '\u{200d}'))
        .map(|(c, _)| c)
}

/// Why a typed signature can't draw `c`, and what to do instead, for errors an agent or the UI shows.
/// `what` names the field ("graphic.text"; empty for none).
pub fn signature_missing_message(c: char, what: &str) -> String {
    let field = if what.is_empty() { String::new() } else { format!(" in {what}") };
    let craft = if crate::CRAFT_FONTS.is_empty() {
        "; this build has no Chinese, Japanese, Korean, Arabic or Hebrew signature face (release builds include them via craft-fonts)"
    } else {
        ""
    };
    format!(
        "no signature font can draw \"{}\" (U+{:04X}){field}{craft}; use other letters, or a drawn or image signature instead",
        c.escape_debug(),
        u32::from(c)
    )
}

/// The outlines of `text` as a typed signature. Latin letters come from the script font; characters
/// it lacks (Cyrillic, Greek, …) from Inter, slanted and scaled to the script's capital height;
/// Chinese, Japanese, Korean, Arabic, Hebrew, Thai and Devanagari from craft-fonts' OFL faces,
/// shaped and laid out in Unicode bidirectional order. Characters no face has are skipped (check
/// [`script_missing`] first where nobody sees a preview). Over-limit input returns an empty outline
/// instead of a silently truncated signature.
pub fn script_outline(text: &str) -> ScriptOutline {
    if text.chars().take(MAX_SIGNATURE_CHARS + 1).count() > MAX_SIGNATURE_CHARS {
        return ScriptOutline::default();
    }
    let text = normalized(text);
    // unicode-bidi indexes the first level of a paragraph, so an empty one would panic.
    if text.is_empty() {
        return ScriptOutline::default();
    }
    let faces = signature_faces();
    let Some(script) = faces.first() else { return ScriptOutline::default() };
    let loc = LocationRef::default();
    let metrics = script.font.metrics(Size::unscaled(), loc);
    let mut pen = Flatten::new(script.scale);
    // The 4096-point budget is for one untrusted document glyph (japanese_glyph), not a
    // whole signature in our bundled fonts. Keep a separate bounded budget for the line.
    pen.max_points = 262_144;
    let picks = faces_for(&text, faces);
    // Byte offset of every character, with its face.
    let chars: Vec<(usize, char, Option<usize>)> = text.char_indices().zip(picks).map(|((i, c), f)| (i, c, f)).collect();
    let base = if unicode_bidi::get_base_direction(text.as_str()) == unicode_bidi::Direction::Rtl {
        unicode_bidi::Level::rtl()
    } else {
        unicode_bidi::Level::ltr()
    };
    let bidi = unicode_bidi::ParagraphBidiInfo::new(&text, Some(base));
    let (levels, runs) = bidi.visual_runs(0..text.len());
    for run in runs {
        let rtl = levels.get(run.start).is_some_and(|l| l.is_rtl());
        // Split the directional run into same-face segments, in logical order.
        let mut segments: Vec<(Option<usize>, usize, usize)> = Vec::new();
        for &(i, c, f) in chars.iter().filter(|(i, _, _)| run.contains(i)) {
            let end = i + c.len_utf8();
            match segments.last_mut() {
                Some((face, _, e)) if *face == f && *e == i => *e = end,
                _ => segments.push((f, i, end)),
            }
        }
        if rtl {
            segments.reverse();
        }
        for (face, start, end) in segments {
            let (Some(face), Some(seg)) = (face.and_then(|f| faces.get(f)), text.get(start..end)) else { continue };
            (pen.scale, pen.slant, pen.dy) = (face.scale, face.slant, 0.0);
            if let Some(data) = &face.shaper {
                draw_shaped(&mut pen, face, data, seg, rtl);
            } else {
                let map = face.font.charmap();
                let glyphs = face.font.outline_glyphs();
                let advances = face.font.glyph_metrics(Size::unscaled(), loc);
                let order: Vec<char> = if rtl { seg.chars().rev().collect() } else { seg.chars().collect() };
                for ch in order {
                    let Some(gid) = map.map(ch) else { continue };
                    if let Some(g) = glyphs.get(gid) {
                        let _ = g.draw(DrawSettings::unhinted(Size::unscaled(), loc), &mut pen);
                        pen.close();
                    }
                    pen.dx += advances.advance_width(gid).unwrap_or(0.0) as f64 * face.scale;
                }
            }
            if pen.too_complex {
                return ScriptOutline::default();
            }
        }
    }
    pen.close();
    if pen.too_complex || !pen.dx.is_finite() {
        return ScriptOutline::default();
    }
    ScriptOutline {
        contours: pen.contours,
        width: pen.dx,
        ascent: metrics.ascent as f64 * script.scale,
        descent: metrics.descent as f64 * script.scale,
    }
}

/// Shape `seg` (one face, one direction) with HarfRust and draw its glyphs at the pen.
fn draw_shaped(pen: &mut Flatten, face: &SigFace, data: &harfrust::ShaperData, seg: &str, rtl: bool) {
    use harfrust::{Direction, ShapeOptions, UnicodeBuffer};
    let shaper = data.shaper(&face.font).build();
    // HarfRust positions are in font units, which the pen scales by `face.scale`.
    let k = face.scale;
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(seg);
    buffer.set_direction(if rtl { Direction::RightToLeft } else { Direction::LeftToRight });
    buffer.guess_segment_properties();
    let shaped = shaper.shape(buffer, ShapeOptions::new());
    let glyphs = face.font.outline_glyphs();
    let loc = LocationRef::default();
    let origin = pen.dx;
    let mut x = 0.0;
    for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
        if info.glyph_id != 0
            && let Some(g) = glyphs.get(GlyphId::new(info.glyph_id))
        {
            pen.dx = origin + (x + f64::from(pos.x_offset)) * k;
            pen.dy = f64::from(pos.y_offset) * k;
            let _ = g.draw(DrawSettings::unhinted(Size::unscaled(), loc), &mut *pen);
            pen.close();
        }
        x += f64::from(pos.x_advance);
        if pen.too_complex {
            return;
        }
    }
    pen.dx = origin + x * k;
    pen.dy = 0.0;
}

#[cfg(test)]
mod tests {
    #[test]
    fn long_names_keep_every_glyph_with_bounded_outline_work() {
        let text = "Alexandria Catherine Elizabeth Montgomery-Wellington";
        let o = super::script_outline(text);
        let expected: usize = text.chars().map(|c| super::script_outline(&c.to_string()).contours.len()).sum();
        assert_eq!(o.contours.len(), expected, "the entire name must be drawn");
        let max_x = o.contours.iter().flatten().map(|p| p[0]).fold(0.0, f64::max);
        assert!(max_x > o.width - 0.5, "ink reaches the last letter: {max_x} / {}", o.width);
        assert!(!super::script_outline(&"W".repeat(super::MAX_SIGNATURE_CHARS)).contours.is_empty());
        assert!(super::script_outline(&"W".repeat(super::MAX_SIGNATURE_CHARS + 1)).contours.is_empty());
    }

    /// Issue #846: Cyrillic (and Greek) names come out of the fallback face instead of vanishing.
    #[test]
    fn cyrillic_and_greek_names_are_drawn_from_the_fallback_face() {
        for name in ["Саша Дермановић", "Љубица Ђорђевић", "Ђурђа Џаковић", "Νίκος"] {
            let o = super::script_outline(name);
            let letters = name.chars().filter(|c| !c.is_whitespace()).count();
            assert!(o.contours.len() >= letters, "{name}: {} contours for {letters} letters", o.contours.len());
            for ch in name.chars().filter(|c| !c.is_whitespace()) {
                let one = super::script_outline(&ch.to_string());
                assert!(!one.contours.is_empty() && one.width > 0.0, "{name}: {ch} has no outline");
            }
            let [_, bottom, right, top] = o.bounds();
            assert!(right <= o.width + 0.5 && top <= o.ascent + 0.2 && bottom >= o.descent - 0.2, "{name}: {:?}", o.bounds());
        }
        // Capitals from the fallback are about as tall as the script's.
        let cap = |c: &str| super::script_outline(c).contours.iter().flatten().map(|p| p[1]).fold(0.0, f64::max);
        let (latin, cyrillic) = (cap("H"), cap("Н"));
        assert!((cyrillic / latin - 1.0).abs() < 0.2, "Н {cyrillic} vs H {latin}");
        // Mixed names keep every letter, in order: the Cyrillic follows the Latin.
        let mixed = super::script_outline("Ana Ана");
        let latin_only = super::script_outline("Ana ");
        assert!(mixed.width > latin_only.width + 0.5, "{} vs {}", mixed.width, latin_only.width);
    }

    #[test]
    fn missing_script_glyphs_are_named() {
        assert_eq!(super::script_missing("Ada Lovelace-King"), None);
        assert_eq!(super::script_missing("Zoë Ñúñez"), None);
        // Cyrillic and Greek come from the Inter fallback (#963), so they aren't missing.
        assert_eq!(super::script_missing("Саша Дермановић Νίκος"), None);
        assert_eq!(super::script_missing(" \t"), None);
        if crate::ui_chinese_fonts().is_empty() {
            // Without craft-fonts there is no Chinese face: the character is named, not dropped.
            assert_eq!(super::script_missing("Ada 张三"), Some('张'));
            assert_eq!(super::script_missing("Саша 张"), Some('张'));
        } else {
            assert_eq!(super::script_missing("Ada 张三"), None);
        }
        // Nothing ships a face for these private-use code points.
        assert_eq!(super::script_missing("Ada \u{10fffd}"), Some('\u{10fffd}'));
    }

    /// Ink of `text` must cover every visible character: each one alone has outlines, and the line
    /// is at least as wide as all of them without the spaces.
    fn assert_drawn(text: &str) {
        let o = super::script_outline(text);
        assert!(!o.contours.is_empty() && o.width > 0.0, "{text}: no outlines");
        assert_eq!(super::script_missing(text), None, "{text}");
        let mut sum = 0.0;
        for ch in text.chars().filter(|c| !c.is_whitespace()) {
            let one = super::script_outline(&ch.to_string());
            assert!(!one.contours.is_empty() && one.width > 0.0, "{text}: {ch} (U+{:04X}) has no outline", u32::from(ch));
            sum += one.width;
        }
        assert!(o.width > sum * 0.6, "{text}: line {} narrower than its letters {sum}", o.width);
        assert!(o.contours.iter().flatten().all(|p| p[0].is_finite() && p[1].is_finite()), "{text}");
    }

    /// Names in German, French, Spanish and other Latin-script languages keep every accented letter.
    #[test]
    fn european_names_keep_their_accents() {
        for name in ["Jürgen Großmann", "Françoise Lefèvre", "José Ñúñez", "Łukasz Żółć", "Nguyễn Thị Ánh"] {
            assert_drawn(name);
        }
    }

    /// A name typed with combining accents is laid out like the precomposed one (NFC).
    #[test]
    fn combining_accents_compose_before_layout() {
        assert_eq!(super::script_outline("Zoe\u{0308} Mu\u{0308}ller"), super::script_outline("Zoë Müller"));
        assert_eq!(super::script_missing("Jose\u{0301}"), None);
    }

    /// Chinese (Simplified and Traditional), Japanese and Korean names, alone and mixed with Latin,
    /// are drawn from craft-fonts' OFL faces. Skipped (with the reason) when built without it.
    #[test]
    fn cjk_names_are_drawn_from_craft_fonts() {
        if crate::ui_chinese_fonts().is_empty() {
            eprintln!("skipping: built without craft-fonts (set CRAFT_FONTS_DIR)");
            assert!(super::script_outline("张三").contours.is_empty());
            return;
        }
        for name in ["张三丰", "張學友", "山田 太郎", "やまだ たろう", "ヤマダ", "김민준", "이서연", "王小明 Wang Xiaoming", "Lee 이민호", "Ana 李"]
        {
            assert_drawn(name);
        }
        // Mixed names keep their logical order: the Chinese follows the Latin.
        let latin = super::script_outline("Wang ");
        let mixed = super::script_outline("Wang 王");
        let first_cjk_x = mixed.contours.get(latin.contours.len()).and_then(|c| c.iter().map(|p| p[0]).reduce(f64::min)).unwrap_or(0.0);
        assert!(first_cjk_x >= latin.width - 0.3, "王 starts at {first_cjk_x}, Latin ends at {}", latin.width);
        // Ideographs stand about as tall as the script's capitals, not tiny or giant.
        let top = |t: &str| super::script_outline(t).contours.iter().flatten().map(|p| p[1]).fold(0.0, f64::max);
        let ratio = top("王") / top("H");
        assert!((0.7..1.6).contains(&ratio), "王/H height ratio {ratio}");
    }

    /// Arabic is shaped (letters join) and laid out right to left; Hebrew right to left too.
    #[test]
    fn right_to_left_names_are_shaped_and_ordered() {
        if crate::document_arabic_font().is_none() {
            eprintln!("skipping: built without craft-fonts (set CRAFT_FONTS_DIR)");
            assert_eq!(super::script_missing("محمد"), Some('م'));
            return;
        }
        assert_drawn("محمد");
        assert_drawn("שרה");
        // Joined forms differ from the letters drawn one at a time.
        let joined = super::script_outline("محمد");
        let isolated: Vec<_> = "محمد".chars().flat_map(|c| super::script_outline(&c.to_string()).contours).collect();
        assert_ne!(joined.contours, isolated);
        // In a left-to-right line the Arabic name sits after the Latin one.
        let latin = super::script_outline("Ali ");
        let mixed = super::script_outline("Ali محمد");
        let min_x = mixed.contours.iter().skip(latin.contours.len()).flatten().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        assert!(min_x >= latin.width - 0.3, "Arabic starts at {min_x}, Latin ends at {}", latin.width);
    }

    #[test]
    fn signature_faces_prefer_handwriting_then_chinese_japanese_korean() {
        let face =
            |family: &'static str, style: &'static str, scripts: &'static [&'static str]| crate::CraftFont { family, style, scripts, bytes: &[] };
        let faces = [
            face("Shippori Mincho", "Regular", &["Jpan", "Latn"]),
            face("BIZ UDPGothic", "Bold", &["Jpan", "Latn"]),
            face("Noto Sans CJK SC", "Regular", &["Hans", "Latn"]),
            face("Noto Sans Arabic", "Regular", &["Arab"]),
            face("NanumGothic", "Regular", &["Hang", "Kore"]),
            face("Noto Sans Symbols", "Regular", &["Zsym"]),
            face("LXGW WenKai Lite", "Regular", &["Hani", "Hans"]),
        ];
        let order: Vec<_> = super::signature_craft_order(faces.iter()).iter().map(|f| f.family).collect();
        assert_eq!(order, ["LXGW WenKai Lite", "Noto Sans CJK SC", "Shippori Mincho", "NanumGothic", "Noto Sans Arabic"]);
    }

    #[test]
    fn a_name_becomes_outlines() {
        let o = super::script_outline("Ada L.");
        assert!(o.contours.len() >= 5, "{}", o.contours.len());
        assert!(o.width > 1.0 && o.width < 6.0, "{}", o.width);
        assert!(o.ascent > 0.5 && o.descent < 0.0);
        let max_x = o.contours.iter().flatten().map(|p| p[0]).fold(0.0, f64::max);
        assert!(max_x <= o.width + 0.2);
        assert!(super::script_outline("").contours.is_empty());
    }

    #[test]
    fn japanese_glyphs_come_from_craft_fonts() {
        if crate::document_japanese_font().is_none() {
            eprintln!("skipping the glyph checks: built without craft-fonts (set CRAFT_FONTS_DIR)");
            assert_eq!(super::japanese_glyph('こ'), Err(super::GlyphError::NoFont));
            return;
        }
        for ch in "日本語の文字こ".chars() {
            let g = super::japanese_glyph(ch).expect("craft-fonts face has the Japanese glyph");
            assert!(g.width > 0.2 && g.width < 2.0);
            assert!(!g.contours.is_empty());
        }
        assert_eq!(super::japanese_glyph('\u{1f4a9}'), Err(super::GlyphError::Missing));
    }
}
