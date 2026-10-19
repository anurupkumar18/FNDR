//! How long a first pass takes, from metadata alone, so setup can show it
//! per folder before anything is read. Ported from DigUp's `Estimator`: its
//! per-unit costs were measured on an M4 Pro with llama.cpp Q8_0; FNDR's
//! default Mac (M1, 8 GB) is taken as `SLOWER` times slower until indexing
//! measures it. PDF pages and media minutes are guessed from file sizes.

use super::{crawl, FileKind, IndexOptions};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Seconds per unit on DigUp's M4 Pro (2026-10-07).
const IMAGE: f64 = 0.12;
const SCREENSHOT: f64 = 0.32;
const PDF_PAGE: f64 = 0.09;
const DOC_CHUNK: f64 = 0.045;
const AUDIO_SECOND: f64 = 0.0089;
const VIDEO_SECOND: f64 = 0.019;
/// An M1 against an M4 Pro, for these GPU-bound costs.
const SLOWER: f64 = 2.5;

/// Size guesses: an average PDF page, a minute of compressed audio, of video.
const BYTES_PER_PDF_PAGE: u64 = 100_000;
const BYTES_PER_AUDIO_SECOND: u64 = 16_000;
const BYTES_PER_VIDEO_SECOND: u64 = 600_000;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct FolderEstimate {
    pub folder: String,
    pub files: BTreeMap<&'static str, usize>,
    /// The first pass, in seconds.
    pub seconds: f64,
    /// The rest of long PDFs and documents, read after everything else.
    pub later_seconds: f64,
    /// Files left out, by reason (keys, datasets, code projects, ...).
    pub skipped: BTreeMap<&'static str, usize>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Estimate {
    pub folders: Vec<FolderEstimate>,
    pub seconds: f64,
    pub later_seconds: f64,
}

pub fn estimate(roots: &[PathBuf], options: &IndexOptions, home: &Path) -> Estimate {
    let mut total = Estimate::default();
    for root in roots {
        let report = crawl::crawl(std::slice::from_ref(root), options, home);
        let mut folder = FolderEstimate {
            folder: root.to_string_lossy().to_string(),
            ..FolderEstimate::default()
        };
        for file in &report.candidates {
            *folder.files.entry(file.kind.as_str()).or_insert(0) += 1;
            let size = file.size.max(0) as u64;
            let (first, later) = match file.kind {
                FileKind::Image => (IMAGE, 0.0),
                FileKind::Screenshot => (SCREENSHOT, 0.0),
                FileKind::Pdf => {
                    let pages = (size / BYTES_PER_PDF_PAGE).max(1) as usize;
                    let all = pages.min(options.max_pages);
                    let read = if pages > options.big_pdf_pages {
                        options.pdf_step.min(all)
                    } else {
                        all
                    };
                    (read as f64 * PDF_PAGE, (all - read) as f64 * PDF_PAGE)
                }
                FileKind::Doc => {
                    let all = ((size / 2000) as usize).clamp(1, options.max_chunks);
                    let read = all.min(options.doc_step);
                    (read as f64 * DOC_CHUNK, (all - read) as f64 * DOC_CHUNK)
                }
                FileKind::Audio => ((size / BYTES_PER_AUDIO_SECOND) as f64 * AUDIO_SECOND, 0.0),
                FileKind::Video => ((size / BYTES_PER_VIDEO_SECOND) as f64 * VIDEO_SECOND, 0.0),
                FileKind::Code => (0.0, 0.0),
            };
            folder.seconds += first * SLOWER;
            folder.later_seconds += later * SLOWER;
        }
        for (reason, count) in &report.skipped {
            if *reason != "other type" {
                *folder.skipped.entry(reason).or_insert(0) += count;
            }
        }
        total.seconds += folder.seconds;
        total.later_seconds += folder.later_seconds;
        total.folders.push(folder);
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_counted_by_kind_and_timed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Documents");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.jpg"), b"x").unwrap();
        std::fs::write(root.join("Screenshot 1.png"), b"x").unwrap();
        std::fs::write(root.join("book.pdf"), vec![0_u8; 10_000_000]).unwrap(); // about 100 pages
        std::fs::write(root.join("id_rsa"), b"x").unwrap();
        let estimate = estimate(
            std::slice::from_ref(&root),
            &IndexOptions::default(),
            Path::new("/nonexistent-home"),
        );
        let folder = &estimate.folders[0];
        assert_eq!(folder.files.get("image"), Some(&1));
        assert_eq!(folder.files.get("screenshot"), Some(&1));
        assert_eq!(folder.skipped.get("key or password file"), Some(&1));
        // The book's first 30 pages now, its other 70 later.
        let first = (IMAGE + SCREENSHOT + 30.0 * PDF_PAGE) * SLOWER;
        assert!((folder.seconds - first).abs() < 1e-9, "{}", folder.seconds);
        assert!((folder.later_seconds - 70.0 * PDF_PAGE * SLOWER).abs() < 1e-9);
        assert_eq!(estimate.seconds, folder.seconds);
    }
}
