//! What Files search reads beyond plain documents: pictures, PDF pages,
//! sound and video. [`Reader`] is the boundary the indexer reads through;
//! [`FilesReader`] is the real one, macOS's frameworks in
//! `fndr-files-reader`, spoken to over [`LineProcess`]. Everything a reader
//! writes (pictures, sound windows) goes under the `out_dir` it is given.

use super::process::LineProcess;
use serde::Deserialize;
use serde_json::json;
use std::path::{Path, PathBuf};

/// A picture prepared for the model, with its OCR text for a screenshot.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Picture {
    /// The downscaled copy; `None` when the picture was too small to bother.
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    /// EXIF DateTimeOriginal, as the camera wrote it.
    pub taken: Option<String>,
    #[serde(default)]
    pub too_small: bool,
    #[serde(default)]
    pub ocr: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Page {
    /// From 1.
    pub number: usize,
    #[serde(default)]
    pub text: String,
    /// The page rendered as a picture: a scan or slide, or text PDFKit read garbled.
    pub image: Option<PathBuf>,
    /// The text came from OCR.
    #[serde(default)]
    pub ocr: bool,
    /// Text read in glyph order (Bengali): too garbled to show or match, still enough for a meaning vector.
    pub garbled: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PdfRead {
    pub total: usize,
    /// The pages read, 0-based: `first..end`.
    pub first: usize,
    pub end: usize,
    pub pages: Vec<Page>,
    pub title: Option<String>,
}

/// A stretch of a recording, or a video frame standing for a stretch.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Piece {
    pub path: PathBuf,
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SoundRead {
    pub pieces: Vec<Piece>,
    pub duration: f64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct VideoRead {
    pub frames: Vec<Piece>,
    pub sound: Vec<Piece>,
    pub duration: f64,
}

/// Which PDF pages to read. `None` for both: a short PDF whole, a long one its first step.
#[derive(Debug, Clone, Copy, Default)]
pub struct PageRange {
    pub first: Option<usize>,
    pub end: Option<usize>,
}

pub trait Reader {
    fn picture(
        &mut self,
        path: &Path,
        screenshot: bool,
        min_side: u32,
        out_dir: &Path,
    ) -> Result<Picture, String>;
    fn pdf(
        &mut self,
        path: &Path,
        range: PageRange,
        options: &super::IndexOptions,
        out_dir: &Path,
    ) -> Result<PdfRead, String>;
    fn audio(&mut self, path: &Path, out_dir: &Path) -> Result<SoundRead, String>;
    fn video(&mut self, path: &Path, out_dir: &Path) -> Result<VideoRead, String>;
}

pub struct FilesReader {
    process: LineProcess,
}

pub fn locate() -> Option<PathBuf> {
    super::process::locate("fndr-files-reader")
}

impl FilesReader {
    pub fn start(program: &Path) -> Result<Self, String> {
        let (process, _) = LineProcess::start("Files reader", program, &[])?;
        Ok(Self { process })
    }

    fn call<T: for<'de> Deserialize<'de>>(
        &mut self,
        request: serde_json::Value,
    ) -> Result<T, String> {
        let reply = self.process.call(request)?;
        serde_json::from_value(reply)
            .map_err(|error| format!("the Files reader answered oddly: {error}"))
    }

    /// A PDF page or a video frame as a JPEG for a preview, under `out_dir`.
    pub fn preview(
        &mut self,
        path: &Path,
        page: Option<usize>,
        seconds: Option<f64>,
        out_dir: &Path,
    ) -> Result<PathBuf, String> {
        #[derive(Deserialize)]
        struct Out {
            path: PathBuf,
        }
        let request = match (page, seconds) {
            (Some(page), _) => {
                json!({ "op": "preview_page", "path": path, "page": page, "out_dir": out_dir })
            }
            (None, Some(seconds)) => {
                json!({ "op": "preview_frame", "path": path, "seconds": seconds, "out_dir": out_dir })
            }
            (None, None) => return Err("a preview needs a page or a moment".into()),
        };
        self.call::<Out>(request).map(|out| out.path)
    }
}

impl Reader for FilesReader {
    fn picture(
        &mut self,
        path: &Path,
        screenshot: bool,
        min_side: u32,
        out_dir: &Path,
    ) -> Result<Picture, String> {
        self.call(json!({ "op": "picture", "path": path, "screenshot": screenshot, "min_side": min_side, "out_dir": out_dir }))
    }

    fn pdf(
        &mut self,
        path: &Path,
        range: PageRange,
        options: &super::IndexOptions,
        out_dir: &Path,
    ) -> Result<PdfRead, String> {
        let mut request = json!({
            "op": "pdf", "path": path, "out_dir": out_dir, "big": options.big_pdf_pages, "step": options.pdf_step,
            "max_pages": options.max_pages, "min_text": 200, "render_pixels": 1400,
        });
        if let Some(first) = range.first {
            request["first"] = json!(first);
        }
        if let Some(end) = range.end {
            request["end"] = json!(end);
        }
        self.call(request)
    }

    fn audio(&mut self, path: &Path, out_dir: &Path) -> Result<SoundRead, String> {
        self.call(
            json!({ "op": "audio", "path": path, "out_dir": out_dir, "window": 30.0, "hop": 25.0 }),
        )
    }

    fn video(&mut self, path: &Path, out_dir: &Path) -> Result<VideoRead, String> {
        self.call(json!({ "op": "video", "path": path, "out_dir": out_dir, "interval": 3.0, "window": 30.0, "hop": 25.0 }))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::process::tests::fake;
    use super::*;

    #[test]
    fn replies_become_typed_results() {
        let dir = tempfile::tempdir().unwrap();
        let answer = r#"{"ok": True, "total": 3, "first": 0, "end": 3, "title": "Lease", "pages": [{"number": 1, "text": "Pets need consent."}, {"number": 2, "text": "", "image": "/tmp/p2.png", "ocr": True}]} if request["op"] == "pdf" else {"ok": True, "path": "/tmp/x.png", "width": 4000, "height": 3000, "ocr": "Error 402"}"#;
        let program = fake(dir.path(), "reader", r#"{"ready": True}"#, answer);
        let mut reader = FilesReader::start(&program).unwrap();
        let pdf = reader
            .pdf(
                Path::new("/d/lease.pdf"),
                PageRange::default(),
                &super::super::IndexOptions::default(),
                dir.path(),
            )
            .unwrap();
        assert_eq!(
            (pdf.total, pdf.pages.len(), pdf.title.as_deref()),
            (3, 2, Some("Lease"))
        );
        assert_eq!(
            pdf.pages[1].image.as_deref(),
            Some(Path::new("/tmp/p2.png"))
        );
        let picture = reader
            .picture(Path::new("/d/s.png"), true, 256, dir.path())
            .unwrap();
        assert_eq!((picture.width, picture.ocr.as_str()), (4000, "Error 402"));
    }
}
