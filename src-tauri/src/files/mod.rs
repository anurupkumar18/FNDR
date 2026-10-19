//! Files search: the files in folders the person chose, found by what is in
//! them. A port of DigUp's engine (ADR 033).
//!
//! Files are never memories. They live in their own SQLite index
//! ([`store::FilesStore`]): one row per file, one per searchable piece of it
//! (a page, a passage, a picture, a moment), each piece with an optional
//! 768-d EmbeddingGemma 2 vector and its words in FTS5. [`searcher`] fuses the
//! two the way DigUp measured best. Surfaces merge file hits with memory hits;
//! nothing here reads or writes the memory store.
//!
//! Reading is bounded by [`rules`]: only chosen folders, never keys, password
//! files, hidden folders, `~/Library`, bundles, datasets or cloud-only files,
//! and inside a code project only screenshots.

pub mod crawl;
pub mod extract;
pub mod helper;
pub mod indexer;
pub mod model;
pub mod process;
pub mod reader;
pub mod rules;
pub mod search_text;
pub mod searcher;
pub mod service;
pub mod settings;
pub mod store;

use serde::{Deserialize, Serialize};

/// EmbeddingGemma 2's output size. Every vector in a Files index has it.
pub const DIMENSION: usize = 768;

/// What a file is, for indexing and for grouping results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    Screenshot,
    Image,
    Pdf,
    Doc,
    Audio,
    Video,
    /// Source code, a repo's docs and notebooks: only in the code index.
    Code,
}

impl FileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FileKind::Screenshot => "screenshot",
            FileKind::Image => "image",
            FileKind::Pdf => "pdf",
            FileKind::Doc => "doc",
            FileKind::Audio => "audio",
            FileKind::Video => "video",
            FileKind::Code => "code",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "screenshot" => FileKind::Screenshot,
            "image" => FileKind::Image,
            "pdf" => FileKind::Pdf,
            "doc" => FileKind::Doc,
            "audio" => FileKind::Audio,
            "video" => FileKind::Video,
            "code" => FileKind::Code,
            _ => return None,
        })
    }

    /// Pictures, recordings and videos: the kinds a dataset folder is made of.
    pub fn is_media(self) -> bool {
        matches!(self, FileKind::Image | FileKind::Audio | FileKind::Video)
    }
}

/// One searchable piece of a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentKind {
    /// File and folder names, for words only.
    Name,
    Image,
    /// Text Apple Vision read from a screenshot.
    Ocr,
    /// A PDF page's text.
    Page,
    /// A PDF page rendered as a picture (scans, slides).
    PageImage,
    /// A passage of a document.
    Chunk,
    /// A window of sound, from a recording or a video's soundtrack.
    Audio,
    /// A video frame.
    Frame,
    /// A stretch of a code file; `loc` to `loc_end` are its first and last lines.
    Lines,
    /// A stretch of a notebook; `loc` to `loc_end` are its first and last cells.
    Cells,
}

impl SegmentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SegmentKind::Name => "name",
            SegmentKind::Image => "image",
            SegmentKind::Ocr => "ocr",
            SegmentKind::Page => "page",
            SegmentKind::PageImage => "page_image",
            SegmentKind::Chunk => "chunk",
            SegmentKind::Audio => "audio",
            SegmentKind::Frame => "frame",
            SegmentKind::Lines => "lines",
            SegmentKind::Cells => "cells",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "name" => SegmentKind::Name,
            "image" => SegmentKind::Image,
            "ocr" => SegmentKind::Ocr,
            "page" => SegmentKind::Page,
            "page_image" => SegmentKind::PageImage,
            "chunk" => SegmentKind::Chunk,
            "audio" => SegmentKind::Audio,
            "frame" => SegmentKind::Frame,
            "lines" => SegmentKind::Lines,
            "cells" => SegmentKind::Cells,
            _ => return None,
        })
    }
}

/// What produced a segment's vector. Text vectors score higher than image or
/// audio vectors for the same topic, so meaning is centered per modality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Modality {
    None,
    Text,
    Image,
    Audio,
}

impl Modality {
    pub const ALL: [Modality; 4] = [
        Modality::None,
        Modality::Text,
        Modality::Image,
        Modality::Audio,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Modality::None => "none",
            Modality::Text => "text",
            Modality::Image => "image",
            Modality::Audio => "audio",
        }
    }

    pub fn parse(text: &str) -> Self {
        match text {
            "text" => Modality::Text,
            "image" => Modality::Image,
            "audio" => Modality::Audio,
            _ => Modality::None,
        }
    }
}

/// EmbeddingGemma 2's task prompts (model card). Pictures and sound take none.
pub mod prompts {
    pub fn query(text: &str) -> String {
        format!("task: search result | query: {text}")
    }

    /// A search of the code index: the model card's prompt for finding code by what it does.
    pub fn code_query(text: &str) -> String {
        format!("task: code retrieval | query: {text}")
    }

    pub fn document(title: Option<&str>, text: &str) -> String {
        let title = title.filter(|title| !title.is_empty()).unwrap_or("none");
        format!("title: {title} | text: {text}")
    }
}

/// `code: retry with backoff` searches the code index and nothing else. The
/// text after the prefix, or `None` for a search that is not one of code.
pub fn code_query_text(query: &str) -> Option<&str> {
    query_after_prefix(query, "code:")
}

/// `file: lease pets` searches files only. The text after the prefix, or `None`.
pub fn file_query_text(query: &str) -> Option<&str> {
    query_after_prefix(query, "file:")
}

fn query_after_prefix<'a>(query: &'a str, prefix: &str) -> Option<&'a str> {
    let trimmed = query.trim_start();
    let head = trimmed.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| trimmed[prefix.len()..].trim())
}

/// Indexing settings. Anything that changes vectors is part of
/// [`IndexOptions::fingerprint`]; an index never mixes settings.
#[derive(Debug, Clone)]
pub struct IndexOptions {
    /// This is the code index: source files, their repos' docs, notebooks.
    pub code: bool,
    pub include_code: bool,
    pub include_datasets: bool,
    pub min_image_side: u32,
    /// Long files are read a step at a time: the first step in the first pass,
    /// the rest after everything else. PDFs up to `big_pdf_pages` are read whole.
    pub big_pdf_pages: usize,
    pub pdf_step: usize,
    pub doc_step: usize,
    pub max_pages: usize,
    pub max_chunks: usize,
    pub excluded_folders: Vec<String>,
    pub excluded_extensions: Vec<String>,
}

impl Default for IndexOptions {
    fn default() -> Self {
        Self {
            code: false,
            include_code: false,
            include_datasets: false,
            min_image_side: 256,
            big_pdf_pages: 50,
            pdf_step: 30,
            doc_step: 40,
            max_pages: 5_000,
            max_chunks: 5_000,
            excluded_folders: Vec::new(),
            excluded_extensions: Vec::new(),
        }
    }
}

impl IndexOptions {
    /// File extensions as people write them (`"HEIC, .txt *.md"`).
    pub fn extensions(text: &str) -> Vec<String> {
        let mut out: Vec<String> = text
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(|word| word.trim_start_matches(['*', '.']).to_lowercase())
            .filter(|word| !word.is_empty())
            .collect();
        out.sort();
        out.dedup();
        out
    }

    pub fn fingerprint(&self) -> &'static str {
        if self.code {
            "code1 lines1800"
        } else {
            "photo140 shot280 page280 frame70/3s audio30/25s"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_pick_the_index() {
        assert_eq!(
            code_query_text("  Code: retry with backoff"),
            Some("retry with backoff")
        );
        assert_eq!(code_query_text("code:"), Some(""));
        assert_eq!(code_query_text("codes of conduct"), None);
        assert_eq!(file_query_text("file: lease pets"), Some("lease pets"));
        assert_eq!(file_query_text("fi"), None);
    }

    #[test]
    fn prompts_follow_the_model_card() {
        assert_eq!(prompts::query("dog"), "task: search result | query: dog");
        assert_eq!(prompts::document(None, "hi"), "title: none | text: hi");
        assert_eq!(prompts::document(Some(""), "hi"), "title: none | text: hi");
        assert_eq!(
            prompts::document(Some("src/a.rs"), "fn a"),
            "title: src/a.rs | text: fn a"
        );
    }

    #[test]
    fn extensions_are_read_as_people_write_them() {
        assert_eq!(
            IndexOptions::extensions("HEIC, .txt *.md"),
            ["heic", "md", "txt"]
        );
    }

    #[test]
    fn kinds_round_trip() {
        for kind in [
            FileKind::Screenshot,
            FileKind::Image,
            FileKind::Pdf,
            FileKind::Doc,
            FileKind::Audio,
            FileKind::Video,
            FileKind::Code,
        ] {
            assert_eq!(FileKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(
            SegmentKind::parse("page_image"),
            Some(SegmentKind::PageImage)
        );
        assert_eq!(Modality::parse("bogus"), Modality::None);
    }
}
