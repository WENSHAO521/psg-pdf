//! Export a copy with ordinary page text replaced by paths. The working document is never edited.

use std::sync::Arc;

use pdfcraft_content::{Op, Pieces};
use pdfcraft_cos::{Dict, Document, Object, SaveOptions, Stream, write_full};
use pdfcraft_render::text_outlines::{TextOutliner, TextShowOutline};

use crate::{DocId, Session, guard};

const MAX_PAGE_CONTENT: usize = 64 * 1024 * 1024;
const MAX_OUTLINED_CONTENT: usize = 128 * 1024 * 1024;

/// A snapshot that can be outlined off the UI thread.
pub struct OutlineExport(Arc<Vec<u8>>);

impl OutlineExport {
    /// Convert the snapshot, reporting completed and total pages.
    pub fn run(self, progress: impl FnMut(usize, usize)) -> Result<Vec<u8>, String> {
        guard(|| outline_copy(self.0, progress))?
    }
}

impl Session {
    /// Take a snapshot including unsaved edits while leaving the open document untouched.
    pub fn prepare_outline_export(&self, id: DocId) -> Result<OutlineExport, String> {
        let doc = self.get(id).ok_or("Open a PDF first.")?;
        doc.check_copying().map_err(|e| e.to_string())?;
        if doc.is_signed() || doc.info.encrypted || !doc.editable() {
            return Err("Text outlining requires an editable, unsigned and unencrypted PDF.".into());
        }
        self.save_bytes(id).map(OutlineExport).map_err(|e| e.to_string())
    }
}

fn outline_copy(bytes: Arc<Vec<u8>>, mut progress: impl FnMut(usize, usize)) -> Result<Vec<u8>, String> {
    let mut doc = Document::open(bytes.clone()).map_err(|e| e.to_string())?;
    let root = doc.root().ok_or("The PDF has no document catalog.")?;
    let catalog = doc.get(root);
    let catalog = catalog.as_dict().ok_or("The PDF catalog could not be read.")?;
    if catalog.get(b"OCProperties").is_some() || catalog.get(b"StructTreeRoot").is_some() {
        return Err("This version cannot outline PDFs with optional layers or accessibility tags.".into());
    }
    let outliner = TextOutliner::new(bytes)?;
    let mut count = 0;
    let mut size = 0_usize;
    let pages = pdfcraft_model::pages(&doc);
    let total = pages.len();
    for (index, page) in pages.into_iter().enumerate() {
        progress(index, total);
        let Some(new) = outline_page(&doc, &page.dict, &outliner, index).map_err(|e| format!("Page {}: {e}", index + 1))? else {
            continue;
        };
        size = size.saturating_add(new.len());
        if size > MAX_OUTLINED_CONTENT {
            return Err("The outlined PDF exceeds the content size limit.".into());
        }
        let stream = doc.add(Object::Stream(Stream::flate(Dict::new(), &new)));
        doc.update_dict(page.obj, |page| page.set(b"Contents".to_vec(), Object::Ref(stream))).map_err(|e| e.to_string())?;
        count += 1;
    }
    if count == 0 {
        return Err("No page text was found to outline.".into());
    }
    write_full(&doc, &SaveOptions::default()).map_err(|e| e.to_string())
}

fn outline_page(doc: &Document, page: &Dict, outliner: &TextOutliner, index: usize) -> Result<Option<Vec<u8>>, String> {
    let Some(contents) = page.get(b"Contents").map(|contents| doc.resolve(contents)) else { return Ok(None) };
    let pieces = content_pieces(doc, &contents)?;
    let parsed = pieces.parse();
    if parsed.skipped > 0 || parsed.ambiguous_images > 0 {
        return Err("Part of the page's drawing instructions could not be read safely.".into());
    }
    let resources = page.get(b"Resources").map(|r| doc.resolve(r));
    let xobjects = resources.as_ref().and_then(|r| r.as_dict()).and_then(|r| r.get(b"XObject")).map(|o| doc.resolve(o));
    validate_drawing(doc, &parsed.ops, xobjects.as_ref().and_then(|o| o.as_dict()))?;
    if !parsed.ops.iter().any(is_show) {
        return Ok(None);
    }
    rewrite(pieces.data(), &parsed.ops, outliner.trace_page(index)?).map(Some)
}

fn content_pieces(doc: &Document, contents: &Object) -> Result<Pieces, String> {
    let mut decoded = Vec::new();
    let mut length = 0_usize;
    let streams = contents.as_array().map_or(std::slice::from_ref(contents), Vec::as_slice);
    for stream in streams {
        let stream = doc.resolve(stream);
        let Object::Stream(stream) = stream.as_ref() else { return Err("A page content stream is missing.".into()) };
        if stream.dict.iter().any(|(name, _)| !matches!(name.as_slice(), b"Length" | b"Filter" | b"DecodeParms" | b"DL")) {
            return Err("A page content stream contains metadata that cannot be rewritten safely.".into());
        }
        let bytes = stream.decoded().map_err(|e| e.to_string())?;
        length = length.saturating_add(bytes.len());
        if length > MAX_PAGE_CONTENT {
            return Err("The page exceeds the content size limit.".into());
        }
        decoded.push(bytes);
    }
    Ok(Pieces::join(&decoded))
}

fn is_show(op: &Op) -> bool {
    matches!(op.op.as_slice(), b"Tj" | b"TJ" | b"'" | b"\"")
}

fn validate_drawing(doc: &Document, ops: &[Op], xobjects: Option<&Dict>) -> Result<(), String> {
    let mut path_open = false;
    let mut in_text = false;
    for op in ops {
        match op.op.as_slice() {
            b"BT" if path_open || in_text => return Err("A text block overlaps another drawing object.".into()),
            b"BT" => in_text = true,
            b"ET" if !in_text => return Err("The page has an unbalanced text block.".into()),
            b"ET" => in_text = false,
            b"m" | b"l" | b"c" | b"v" | b"y" | b"h" | b"re" if in_text => {
                return Err("A text block contains other drawing commands.".into());
            }
            b"m" | b"l" | b"c" | b"v" | b"y" | b"h" | b"re" => path_open = true,
            b"n" | b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" => path_open = false,
            b"Tj" | b"TJ" | b"'" | b"\"" if !in_text => return Err("Page text is outside a supported text block.".into()),
            b"Do" => {
                let image = op.last_name().and_then(|name| xobjects?.get(name)).map(|o| doc.resolve(o));
                if !image.as_ref().and_then(|o| o.as_dict()).is_some_and(|d| d.name(b"Subtype") == Some(b"Image")) {
                    return Err("This page draws a nested PDF form or an unsupported graphic.".into());
                }
            }
            b"Tr" if !matches!(op.num_back(0), Some(0.0 | 1.0 | 3.0)) => {
                return Err("This page uses filled-and-stroked or clipping text that cannot be converted safely.".into());
            }
            b"scn" | b"SCN" if op.last_name().is_some() => return Err("This page uses a tiling pattern that could contain text.".into()),
            b"BDC" | b"DP"
                if op.last_name().is_some()
                    || op.back(0).and_then(Object::as_dict).is_some_and(|p| p.get(b"ActualText").is_some() || p.get(b"Alt").is_some()) =>
            {
                return Err("This page uses marked content with text metadata that cannot be converted safely.".into());
            }
            _ => {}
        }
    }
    if in_text {
        return Err("The page has an unbalanced text block.".into());
    }
    Ok(())
}

fn has_text_operand(op: &Op) -> bool {
    let nonempty = |object: &Object| matches!(object, Object::String(s) if !s.bytes.is_empty());
    op.back(0).is_some_and(|object| if op.is("TJ") { object.as_array().is_some_and(|array| array.iter().any(nonempty)) } else { nonempty(object) })
}

fn rewrite(bytes: &[u8], ops: &[Op], shows: Vec<TextShowOutline>) -> Result<Vec<u8>, String> {
    let mut shows = shows.into_iter();
    let mut output = Vec::new();
    let mut cursor = 0;
    for op in ops {
        let drawing = if is_show(op) {
            let show = shows.next().ok_or("Text operations did not match the page renderer.")?;
            if has_text_operand(op) && !show.has_glyphs {
                return Err("A text operation could not be decoded into glyphs.".into());
            }
            show.drawing
        } else if matches!(op.op.as_slice(), b"BT" | b"ET" | b"Tf" | b"Tc" | b"Tw" | b"Tz" | b"TL" | b"Tr" | b"Ts" | b"Td" | b"TD" | b"Tm" | b"T*") {
            Vec::new()
        } else {
            continue;
        };
        output.extend_from_slice(bytes.get(cursor..op.span.start).ok_or("Text operator positions overlap.")?);
        output.push(b'\n');
        output.extend_from_slice(&drawing);
        cursor = op.span.end;
    }
    if shows.next().is_some() {
        return Err("The renderer found additional text outside the page content.".into());
    }
    output.extend_from_slice(bytes.get(cursor..).ok_or("A text operation is outside the page content.")?);
    Ok(output)
}
