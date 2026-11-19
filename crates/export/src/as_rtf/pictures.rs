//! Pictures in the flowing RTF export. The picture text and rotation work have separate budgets.

use std::fmt::Write as _;
use std::io::{self, Cursor, Write};

use image::{ImageEncoder, ImageFormat, ImageReader, RgbaImage};

use crate::layout::Image;

const MAX_PICTURE_TEXT: usize = 256 << 20;
const MAX_PROBE_BYTES: usize = 256 << 20;
const MAX_ROTATION_PIXELS: u64 = 64 << 20;
const MAX_SIDE: u32 = 4096;
const ROW_BYTES: usize = 32;
const HEADER_BYTES: usize = 128;

#[derive(Default)]
pub(super) struct Budget {
    picture_text: usize,
    probed_bytes: usize,
    rotated_pixels: u64,
}

impl Budget {
    pub(super) fn write(&mut self, image: &Image, rtf: &mut String) -> bool {
        self.try_write(image, rtf).is_some()
    }

    fn try_write(&mut self, image: &Image, rtf: &mut String) -> Option<()> {
        let format = match image.ext {
            "png" => ImageFormat::Png,
            "jpg" | "jpeg" => ImageFormat::Jpeg,
            _ => return None,
        };
        let angle = image.rotation.rem_euclid(360.0);
        let [width, height] = image.size;
        if !angle.is_finite() || !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        let remaining = MAX_PICTURE_TEXT.checked_sub(self.picture_text)?;
        if picture_text_size(image.bytes.len())? > remaining {
            return None;
        }

        // Probe work is charged for every placement, including repeated and invalid images.
        let probed = self.probed_bytes.checked_add(image.bytes.len())?;
        if probed > MAX_PROBE_BYTES {
            return None;
        }
        self.probed_bytes = probed;
        let (pixels_w, pixels_h) = image_reader(image, format).into_dimensions().ok()?;
        if pixels_w == 0 || pixels_h == 0 || pixels_w > MAX_SIDE || pixels_h > MAX_SIDE {
            return None;
        }

        let physical = bounds(image.size, angle);
        let rotated;
        let (bytes, kind, pixels) = if angle == 0.0 {
            (&*image.bytes, if format == ImageFormat::Png { "pngblip" } else { "jpegblip" }, [pixels_w, pixels_h])
        } else {
            let shape = Rotation::new([pixels_w, pixels_h], image.size, physical, angle)?;
            let work = self.rotated_pixels.checked_add(area([pixels_w, pixels_h]))?.checked_add(area(shape.pixels))?;
            if work > MAX_ROTATION_PIXELS {
                return None;
            }
            self.rotated_pixels = work;
            let source = image_reader(image, format).decode().ok()?.into_rgba8();
            let output = shape.apply(&source);
            let mut sink = LimitedPng { bytes: Vec::new(), cap: remaining.saturating_sub(HEADER_BYTES) / 2 };
            image::codecs::png::PngEncoder::new(&mut sink)
                .write_image(output.as_raw(), output.width(), output.height(), image::ExtendedColorType::Rgba8)
                .ok()?;
            rotated = sink.bytes;
            (rotated.as_slice(), "pngblip", shape.pixels)
        };

        let cost = picture_text_size(bytes.len())?;
        if cost > remaining {
            return None;
        }
        self.picture_text += cost;
        // Fit inline pictures inside an ordinary RTF page, retaining their aspect ratio.
        let scale = (432.0 / physical[0]).min(648.0 / physical[1]).min(1.0);
        let [goal_w, goal_h] = physical.map(|length| (length * scale * 20.0).round().max(1.0) as u32);
        let [w, h] = pixels;
        let _ = writeln!(rtf, "{{\\pard{{\\pict\\{kind}\\picw{w}\\pich{h}\\picwgoal{goal_w}\\pichgoal{goal_h} ");
        const HEX: &[u8] = b"0123456789abcdef";
        for row in bytes.chunks(ROW_BYTES) {
            for &byte in row {
                rtf.push(char::from(HEX.get(usize::from(byte >> 4)).copied().unwrap_or(b'0')));
                rtf.push(char::from(HEX.get(usize::from(byte & 15)).copied().unwrap_or(b'0')));
            }
            rtf.push('\n');
        }
        rtf.push_str("}\\par}\n");
        Some(())
    }
}

fn picture_text_size(bytes: usize) -> Option<usize> {
    bytes.checked_mul(2)?.checked_add(bytes.div_ceil(ROW_BYTES))?.checked_add(HEADER_BYTES)
}

fn image_reader(image: &Image, format: ImageFormat) -> ImageReader<Cursor<&[u8]>> {
    let mut reader = ImageReader::with_format(Cursor::new(image.bytes.as_ref()), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(64 << 20);
    reader.limits(limits);
    reader
}

fn area([width, height]: [u32; 2]) -> u64 {
    u64::from(width) * u64::from(height)
}

fn bounds([width, height]: [f64; 2], angle: f64) -> [f64; 2] {
    match angle {
        0.0 | 180.0 => [width, height],
        90.0 | 270.0 => [height, width],
        _ => {
            let (sin, cos) = angle.to_radians().sin_cos();
            [width * cos.abs() + height * sin.abs(), width * sin.abs() + height * cos.abs()]
        }
    }
}

struct Rotation {
    pixels: [u32; 2],
    before: [f64; 2],
    after: [f64; 2],
    angle: f64,
}

impl Rotation {
    fn new([w, h]: [u32; 2], before: [f64; 2], after: [f64; 2], angle: f64) -> Option<Self> {
        let pixels = match angle {
            90.0 | 270.0 => [h, w],
            180.0 => [w, h],
            _ => {
                let density = (f64::from(w) / before[0]).min(f64::from(h) / before[1]);
                let size = after.map(|length| (length * density).round());
                if size.iter().any(|&n| !n.is_finite() || n > f64::from(MAX_SIDE)) {
                    return None;
                }
                size.map(|n| n.max(1.0) as u32)
            }
        };
        Some(Self { pixels, before, after, angle })
    }

    fn apply(&self, source: &RgbaImage) -> RgbaImage {
        // PDF rotations are counterclockwise. Raster rotation functions count clockwise.
        match self.angle {
            90.0 => return image::imageops::rotate270(source),
            180.0 => return image::imageops::rotate180(source),
            270.0 => return image::imageops::rotate90(source),
            _ => {}
        }
        let [w, h] = self.pixels;
        let (sin, cos) = self.angle.to_radians().sin_cos();
        let [before_w, before_h] = self.before;
        let [after_w, after_h] = self.after;
        let mut output = RgbaImage::new(w, h);
        for (x, y, pixel) in output.enumerate_pixels_mut() {
            let dx = (f64::from(x) + 0.5) * after_w / f64::from(w) - after_w / 2.0;
            let dy = (f64::from(y) + 0.5) * after_h / f64::from(h) - after_h / 2.0;
            let sx = (cos * dx - sin * dy) / before_w + 0.5;
            let sy = (sin * dx + cos * dy) / before_h + 0.5;
            if (0.0..1.0).contains(&sx)
                && (0.0..1.0).contains(&sy)
                && let Some(sample) = source.get_pixel_checked((sx * f64::from(source.width())) as u32, (sy * f64::from(source.height())) as u32)
            {
                *pixel = *sample;
            }
        }
        output
    }
}

struct LimitedPng {
    bytes: Vec<u8>,
    cap: usize,
}

impl Write for LimitedPng {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() > self.cap.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("RTF picture exceeds the remaining output budget"));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Page, rtf_checked};

    fn two_colour_picture(rotation: f64) -> Image {
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(&[255, 0, 0, 255, 0, 0, 255, 255], 2, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        Image { ext: "png", bytes: bytes.into(), rect: [0.0, 0.0, 72.0, 72.0], size: [72.0, 36.0], rotation }
    }

    #[test]
    fn rtf_embeds_the_rotated_pixels_and_their_display_dimensions() {
        let image = two_colour_picture(90.0);
        let (rtf, omitted) = rtf_checked(&[Page { images: vec![image], ..Page::default() }]);
        assert!(omitted.is_empty());
        let data = rtf.split_once("\\pict\\pngblip\\picw1\\pich2\\picwgoal720\\pichgoal1440 \n").unwrap().1;
        let data: String = data.split_once('}').unwrap().0.split_whitespace().collect();
        let (pairs, remainder) = data.as_bytes().as_chunks::<2>();
        assert!(remainder.is_empty());
        let bytes: Vec<u8> = pairs.iter().map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect();
        let pixels = image::load_from_memory(&bytes).unwrap().into_rgba8();
        assert_eq!(pixels.dimensions(), (1, 2));
        assert_eq!(pixels.get_pixel(0, 0).0, [0, 0, 255, 255]);
        assert_eq!(pixels.get_pixel(0, 1).0, [255, 0, 0, 255]);
    }

    #[test]
    fn rtf_lists_affected_pages_once_and_keeps_other_pictures() {
        let valid = two_colour_picture(0.0);
        let invalid = Image { bytes: b"invalid PNG".as_slice().into(), ..valid.clone() };
        let pages =
            [Page { images: vec![valid.clone()], ..Page::default() }, Page { images: vec![invalid.clone(), invalid, valid], ..Page::default() }];
        let (rtf, omitted) = rtf_checked(&pages);
        assert_eq!(omitted, [1]);
        assert_eq!(rtf.matches("\\pict\\pngblip").count(), 2);
        assert!(rtf.contains("\\page\n"));
    }

    #[test]
    fn each_rtf_placement_spends_the_expanded_text_budget() {
        let image = two_colour_picture(0.0);
        let cost = picture_text_size(image.bytes.len()).unwrap();
        let mut budget = Budget { picture_text: MAX_PICTURE_TEXT - cost, ..Budget::default() };
        let mut rtf = String::new();
        assert!(budget.write(&image, &mut rtf));
        assert!(!budget.write(&image, &mut rtf));
        assert_eq!(rtf.matches("\\pict").count(), 1);
    }
}
