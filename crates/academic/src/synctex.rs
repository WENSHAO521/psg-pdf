//! SyncTeX: where in the PDF each source line went, and back. TeX writes `<main>.synctex.gz`
//! when it runs with `-synctex=1` (the compilation always asks for it). Its format is public
//! (the SyncTeX file format, as documented with TeX Live's synctex tool); this reads only what
//! jumping needs: the input files, and for each page the source line and position of the boxes,
//! glue, kerns and characters on it.
//!
//! Positions are in PDF points from the top-left corner of the page.

use std::io::Read;
use std::path::Path;

use crate::Error;

/// The largest SyncTeX file read, after decompression.
const MAX_SYNCTEX: u64 = 256 * 1024 * 1024;
/// The most positions kept.
const MAX_RECORDS: usize = 4_000_000;
/// Scaled points per PDF point (65536 per TeX point, 72.27 TeX points to 72 PDF points).
const SP_PER_BP: f64 = 65536.0 * 72.27 / 72.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Record {
    /// 0-based page.
    page: u32,
    /// The input's number in the file.
    input: u32,
    line: u32,
    x: f32,
    y: f32,
}

/// A read SyncTeX file.
#[derive(Debug, Default, Clone)]
pub struct SyncTex {
    /// Input number and the file's path relative to the project folder; inputs outside it
    /// (the TeX distribution's classes and packages) are not listed.
    inputs: Vec<(u32, String)>,
    records: Vec<Record>,
    unit: f64,
    x_offset: f64,
    y_offset: f64,
}

impl SyncTex {
    /// Read `<main>.synctex.gz` (or an uncompressed `<main>.synctex`) in `project`. `None` when
    /// there is none, as before the first compilation.
    pub fn read(project: &Path, main: &str) -> Result<Option<SyncTex>, Error> {
        let stem = main.trim_end_matches(".tex");
        let gz = project.join(format!("{stem}.synctex.gz"));
        let plain = project.join(format!("{stem}.synctex"));
        let mut text = String::new();
        if let Ok(file) = std::fs::File::open(&gz) {
            flate2::read::GzDecoder::new(file)
                .take(MAX_SYNCTEX)
                .read_to_string(&mut text)
                .map_err(|e| Error::Project(format!("{}: {e}", gz.display())))?;
        } else if let Ok(file) = std::fs::File::open(&plain) {
            file.take(MAX_SYNCTEX).read_to_string(&mut text).map_err(|e| Error::Project(format!("{}: {e}", plain.display())))?;
        } else {
            return Ok(None);
        }
        Ok(Some(SyncTex::parse(&text, project)))
    }

    /// Parse SyncTeX text whose inputs are relative to `project`.
    pub fn parse(text: &str, project: &Path) -> SyncTex {
        let mut s = SyncTex { unit: 1.0, ..SyncTex::default() };
        let project_prefix = project.to_string_lossy().trim_end_matches('/').to_owned();
        let mut page: Option<u32> = None;
        for line in text.lines() {
            if s.records.len() >= MAX_RECORDS {
                break;
            }
            if let Some(rest) = line.strip_prefix("Input:") {
                let Some((n, path)) = rest.split_once(':') else { continue };
                let Ok(n) = n.parse::<u32>() else { continue };
                if let Some(rel) = relative_input(path, &project_prefix) {
                    s.inputs.push((n, rel));
                }
                continue;
            }
            if let Some(v) = line.strip_prefix("Unit:") {
                s.unit = v.trim().parse().unwrap_or(1.0);
                continue;
            }
            if let Some(v) = line.strip_prefix("X Offset:") {
                s.x_offset = v.trim().parse().unwrap_or(0.0);
                continue;
            }
            if let Some(v) = line.strip_prefix("Y Offset:") {
                s.y_offset = v.trim().parse().unwrap_or(0.0);
                continue;
            }
            let mut chars = line.chars();
            let Some(kind) = chars.next() else { continue };
            let rest = chars.as_str();
            match kind {
                '{' => page = rest.trim().parse::<u32>().ok().and_then(|p| p.checked_sub(1)),
                '}' => page = None,
                // What sits on the page: characters, glue, kerns, math and empty boxes. Box
                // starts are left out: a paragraph's box carries the line of the command around
                // it (an \\input), not of the text in it.
                'v' | 'h' | 'x' | 'k' | 'g' | '$' => {
                    let Some(page) = page else { continue };
                    if let Some(r) = s.record(page, rest) {
                        s.records.push(r);
                    }
                }
                _ => {}
            }
        }
        s
    }

    /// `tag,line:h,v[:…]` as a record on `page`, for inputs in the project.
    fn record(&self, page: u32, rest: &str) -> Option<Record> {
        let (tag_line, pos) = rest.split_once(':')?;
        let (tag, line) = tag_line.split_once(',')?;
        let input = tag.trim().parse::<u32>().ok()?;
        if !self.inputs.iter().any(|(n, _)| *n == input) {
            return None;
        }
        let line = line.parse::<u32>().ok()?;
        let mut coords = pos.split([',', ':']);
        let h = coords.next()?.parse::<f64>().ok()?;
        let v = coords.next()?.parse::<f64>().ok()?;
        let to_bp = |sp: f64, offset: f64| ((sp * self.unit + offset) / SP_PER_BP) as f32;
        let (x, y) = (to_bp(h, self.x_offset), to_bp(v, self.y_offset));
        (x.is_finite() && y.is_finite()).then_some(Record { page, input, line, x, y })
    }

    fn file_of(&self, input: u32) -> Option<&str> {
        self.inputs.iter().find(|(n, _)| *n == input).map(|(_, f)| f.as_str())
    }

    /// The source file (relative to the project) and line that put what is at (`x`, `y`) on
    /// `page` (0-based), in PDF points from the page's top-left corner.
    pub fn source_at(&self, page: usize, x: f32, y: f32) -> Option<(String, u32)> {
        let page = u32::try_from(page).ok()?;
        // Nearest by distance, with a line above or below counting more than a step sideways.
        let best = self.records.iter().filter(|r| r.page == page).min_by(|a, b| {
            let d = |r: &Record| (r.x - x).powi(2) + 4.0 * (r.y - y).powi(2);
            d(a).total_cmp(&d(b))
        })?;
        Some((self.file_of(best.input)?.to_owned(), best.line))
    }

    /// The page (0-based) and height on it, in PDF points from the top, where `line` of `file`
    /// (relative to the project) was set; the nearest line that was set when that one wasn't
    /// (a blank line, a comment).
    pub fn page_of(&self, file: &str, line: u32) -> Option<(usize, f32)> {
        let file = file.trim_start_matches("./");
        let inputs: Vec<u32> = self.inputs.iter().filter(|(_, f)| f == file).map(|(n, _)| *n).collect();
        let of_file = self.records.iter().filter(|r| inputs.contains(&r.input));
        let nearest = of_file.clone().map(|r| r.line.abs_diff(line)).min()?;
        let at = of_file.filter(|r| r.line.abs_diff(line) == nearest).min_by(|a, b| a.page.cmp(&b.page).then(a.y.total_cmp(&b.y)))?;
        Some((usize::try_from(at.page).ok()?, at.y))
    }
}

/// `path` relative to the project folder `prefix`, or `None` when it is outside it.
fn relative_input(path: &str, prefix: &str) -> Option<String> {
    let path = path.trim();
    let rel = if let Some((_, after)) = path.split_once("/./") {
        after
    } else if let Some(after) = path.strip_prefix(prefix).and_then(|p| p.strip_prefix('/')) {
        after
    } else if !path.starts_with('/') && !path.contains(':') {
        path.trim_start_matches("./")
    } else {
        return None;
    };
    let rel = rel.replace('\\', "/");
    crate::paths::safe_relative_path(&rel).is_some().then_some(rel)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// From a pdfLaTeX run on a two-page article: line 3 is the first paragraph, line 6 a second
    /// one 5 cm lower, line 8 on page 2.
    const SAMPLE: &str = "SyncTeX Version:1
Input:1:/work/p/./main.tex
Input:2:/usr/share/texlive/texmf-dist/tex/latex/base/article.cls
Input:3:/work/p/./body.tex
Output:pdf
Magnification:1000
Unit:1
X Offset:0
Y Offset:0
Content:
!386
{1
[1,7:4736286,46220574:26673152,41484288,0
(1,4:8799518,8865054:22609920,455111,0
h3,3:8799518,8865054:983040,0,0
x3,3:11162458,8865054
g2,3:11380911,8865054
)
(1,7:8799518,18974885:22609920,455111,127431
x3,6:11785051,18974885
)
]
}1
{2
x3,8:9782558,8865054
}2
";

    #[test]
    fn a_click_finds_its_line_and_a_line_finds_its_page() {
        let s = SyncTex::parse(SAMPLE, Path::new("/work/p"));
        // "First" starts at 148.7 pt, baseline 134.8 pt from the top.
        assert_eq!(s.source_at(0, 150.0, 132.0), Some(("body.tex".into(), 3)));
        assert_eq!(s.source_at(0, 190.0, 285.0), Some(("body.tex".into(), 6)));
        assert_eq!(s.source_at(1, 150.0, 132.0), Some(("body.tex".into(), 8)));
        assert_eq!(s.source_at(5, 0.0, 0.0), None);
        let (page, y) = s.page_of("body.tex", 6).unwrap();
        assert_eq!(page, 0);
        assert!((y - 288.4).abs() < 1.0, "{y}");
        // A line that wasn't set: the nearest one that was.
        assert_eq!(s.page_of("body.tex", 7).map(|p| p.0), Some(0));
        assert_eq!(s.page_of("./body.tex", 9).map(|p| p.0), Some(1));
        assert_eq!(s.page_of("other.tex", 1), None);
    }

    #[test]
    fn files_outside_the_project_and_garbage_are_ignored() {
        let text =
            "Input:1:/etc/passwd\nInput:2:/work/p/./a.tex\nInput:x:y\n{1\nx1,1:1,1\nx2,1:nan,1\nx2,2:1\nx2,3:65781,65781\n(((\n}1\n{0\nx2,9:1,1\n";
        let s = SyncTex::parse(text, Path::new("/work/p"));
        assert_eq!(s.inputs, [(2, "a.tex".to_owned())]);
        assert_eq!(s.records.len(), 1, "{:?}", s.records);
        assert_eq!(s.source_at(0, 1.0, 1.0), Some(("a.tex".into(), 3)));
    }

    #[test]
    fn a_compressed_file_is_read_from_the_project() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        assert!(SyncTex::read(dir.path(), "main.tex").unwrap().is_none());
        let text = SAMPLE.replace("/work/p", &dir.path().to_string_lossy());
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(text.as_bytes()).unwrap();
        std::fs::write(dir.path().join("main.synctex.gz"), gz.finish().unwrap()).unwrap();
        let s = SyncTex::read(dir.path(), "main.tex").unwrap().unwrap();
        assert_eq!(s.source_at(1, 150.0, 132.0), Some(("body.tex".into(), 8)));
    }
}
