//! OCR-aware text chunking for embedding.

use crate::capture::text_cleanup;
use crate::config::{
    DEFAULT_CHARS_PER_TOKEN, DEFAULT_CHUNK_MAX_TOKENS, DEFAULT_CHUNK_MIN_TOKENS,
    DEFAULT_CHUNK_OCR_TARGET_MAX_CHARS, DEFAULT_CHUNK_OCR_TARGET_MIN_CHARS,
    DEFAULT_CHUNK_OVERLAP_TOKENS,
};

const MAX_CHUNK_TOKENS: usize = DEFAULT_CHUNK_MAX_TOKENS;
const CHUNK_OVERLAP: usize = DEFAULT_CHUNK_OVERLAP_TOKENS;
const MIN_CHUNK_TOKENS: usize = DEFAULT_CHUNK_MIN_TOKENS;
const CHARS_PER_TOKEN: usize = DEFAULT_CHARS_PER_TOKEN;

const OCR_TARGET_MIN: usize = DEFAULT_CHUNK_OCR_TARGET_MIN_CHARS;
const OCR_TARGET_MAX: usize = DEFAULT_CHUNK_OCR_TARGET_MAX_CHARS;

/// Text chunker for splitting long texts.
pub struct TextChunker {
    max_chars: usize,
    overlap_chars: usize,
}

#[derive(Debug, Clone)]
pub struct TextChunk {
    pub text: String,
    pub approx_tokens: usize,
    pub chunk_index: usize,
    pub line_kind: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineKind {
    Title,
    Url,
    Search,
    Email,
    Code,
    Plain,
}

impl TextChunker {
    pub fn new() -> Self {
        Self {
            max_chars: MAX_CHUNK_TOKENS * CHARS_PER_TOKEN,
            overlap_chars: CHUNK_OVERLAP * CHARS_PER_TOKEN,
        }
    }

    /// Split plain text into embedding chunks.
    pub fn chunk(&self, text: &str) -> Vec<String> {
        self.chunk_ocr_text("", "", text)
    }

    /// OCR-aware chunking that preserves semantic boundaries and drops low-signal lines.
    pub fn chunk_ocr_text(&self, app_name: &str, window_title: &str, text: &str) -> Vec<String> {
        chunk_screen_text(self, app_name, window_title, text)
    }

    /// Product-named wrapper for the capture -> OCR -> embedding pipeline.
    pub fn chunk_screen_text(&self, app_name: &str, window_title: &str, text: &str) -> Vec<String> {
        self.chunk_ocr_text_with_metadata(app_name, window_title, text)
            .into_iter()
            .map(|chunk| chunk.text)
            .collect()
    }

    /// OCR-aware chunking with lightweight metadata used for diagnostics/ranking.
    pub fn chunk_ocr_text_with_metadata(
        &self,
        app_name: &str,
        window_title: &str,
        text: &str,
    ) -> Vec<TextChunk> {
        let cleaned_text = text_cleanup::reduce_chrome_noise_for_app(app_name, text);
        let mut lines = Vec::new();
        let title = normalize_line(window_title);
        if !title.is_empty() && !self.is_low_signal_line(&title) {
            lines.push((title, LineKind::Title));
        }

        let mut seen_lines = std::collections::HashSet::new();
        for raw_line in cleaned_text.lines() {
            let line = normalize_line(raw_line);
            if line.is_empty() || self.is_low_signal_line(&line) {
                continue;
            }
            let dedup_key = line.to_lowercase();
            if !seen_lines.insert(dedup_key) {
                continue;
            }
            lines.push((line.clone(), classify_line(&line)));
        }

        if lines.is_empty() {
            return self
                .chunk_by_chars(text)
                .into_iter()
                .enumerate()
                .map(|(index, chunk)| TextChunk {
                    approx_tokens: chunk.len() / CHARS_PER_TOKEN,
                    text: chunk,
                    chunk_index: index,
                    line_kind: line_kind_label(LineKind::Plain),
                })
                .collect();
        }

        let mut chunks: Vec<(String, LineKind)> = Vec::new();
        let mut current = String::new();
        let mut current_kind = LineKind::Plain;

        for (line, kind) in lines {
            if line.len() > OCR_TARGET_MAX {
                if !current.trim().is_empty() {
                    chunks.push((current.trim().to_string(), current_kind));
                    current.clear();
                }
                for chunk in self.chunk_by_chars(&line) {
                    chunks.push((chunk, kind));
                }
                current_kind = LineKind::Plain;
                continue;
            }

            let should_boundary_break =
                matches!(kind, LineKind::Code | LineKind::Email | LineKind::Search)
                    || matches!(
                        current_kind,
                        LineKind::Code | LineKind::Email | LineKind::Search
                    )
                    || (kind != current_kind && !current.is_empty());

            if should_boundary_break && !current.is_empty() && current.len() >= OCR_TARGET_MIN {
                chunks.push((current.trim().to_string(), current_kind));
                let prev_tail = overlap_tail(&current, overlap_chars_for_text(&current));
                current.clear();
                if !prev_tail.is_empty() {
                    current.push_str(&prev_tail);
                    current.push('\n');
                }
            }

            if !current.is_empty() {
                current.push('\n');
            }
            current.push_str(&line);
            current_kind = kind;

            if current.len() >= OCR_TARGET_MAX {
                chunks.push((current.trim().to_string(), current_kind));
                let prev_tail = overlap_tail(&current, overlap_chars_for_text(&current));
                current.clear();
                if !prev_tail.is_empty() {
                    current.push_str(&prev_tail);
                }
            }
        }

        if !current.trim().is_empty() {
            chunks.push((current.trim().to_string(), current_kind));
        }

        let mut rendered = if chunks.is_empty() {
            self.chunk_by_chars(text)
                .into_iter()
                .enumerate()
                .map(|(index, chunk)| TextChunk {
                    approx_tokens: chunk.len() / CHARS_PER_TOKEN,
                    text: chunk,
                    chunk_index: index,
                    line_kind: line_kind_label(LineKind::Plain),
                })
                .collect::<Vec<_>>()
        } else {
            chunks
                .into_iter()
                .enumerate()
                .map(|(index, (chunk, kind))| TextChunk {
                    approx_tokens: chunk.len() / CHARS_PER_TOKEN,
                    text: chunk,
                    chunk_index: index,
                    line_kind: line_kind_label(kind),
                })
                .collect::<Vec<_>>()
        };

        // Drop near-identical chunks from the same frame to keep index pressure low.
        let mut seen = std::collections::HashSet::new();
        rendered.retain(|chunk| {
            let key = normalize_line(&chunk.text).to_lowercase();
            seen.insert(key)
        });

        merge_short_orphans(rendered)
    }

    pub fn is_low_signal_line(&self, line: &str) -> bool {
        let normalized = normalize_line(line);
        if normalized.len() < 6 {
            return true;
        }
        if text_cleanup::symbol_ratio(&normalized) > 0.62 {
            return true;
        }
        if text_cleanup::looks_like_file_inventory(&normalized)
            && !self.is_code_like_line(&normalized)
        {
            return true;
        }

        let lower = normalized.to_lowercase();
        if matches!(lower.as_str(), "new tab" | "home" | "trending" | "untitled") {
            return true;
        }

        false
    }

    pub fn is_code_like_line(&self, line: &str) -> bool {
        let trimmed = line.trim();
        if trimmed.starts_with('$') || trimmed.starts_with('>') {
            return true;
        }
        let lower = trimmed.to_lowercase();
        lower.starts_with("cargo ")
            || lower.starts_with("npm ")
            || lower.starts_with("pnpm ")
            || lower.starts_with("git ")
            || lower.starts_with("fn ")
            || lower.starts_with("let ")
            || lower.contains(" => ")
            || lower.contains("::")
            || (trimmed.contains('{') && trimmed.contains('}'))
            || (trimmed.contains('(') && trimmed.contains(')') && trimmed.contains(';'))
    }

    pub fn is_search_like_line(&self, line: &str) -> bool {
        let lower = line.trim().to_lowercase();
        lower.starts_with("search ")
            || lower.starts_with("search:")
            || lower.starts_with("query:")
            || lower.starts_with("find ")
            || lower.contains(" results for ")
            || lower.ends_with(" near me")
    }

    pub fn is_email_like_line(&self, line: &str) -> bool {
        let lower = line.trim().to_lowercase();
        lower.starts_with("from:")
            || lower.starts_with("to:")
            || lower.starts_with("subject:")
            || lower.starts_with("cc:")
            || lower.starts_with("bcc:")
    }

    fn chunk_by_chars(&self, text: &str) -> Vec<String> {
        if text.trim().is_empty() {
            return Vec::new();
        }

        if text.len() <= self.max_chars {
            return vec![text.to_string()];
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < text.len() {
            let end = (start + self.max_chars).min(text.len());

            // Try to break at word boundary
            let chunk_end = if end < text.len() {
                text[start..end]
                    .rfind(|c: char| c.is_whitespace())
                    .map(|pos| start + pos)
                    .unwrap_or(end)
            } else {
                end
            };

            let chunk = text[start..chunk_end].trim().to_string();
            if !chunk.is_empty() {
                chunks.push(chunk);
            }

            // Move start with overlap
            start = if chunk_end >= self.overlap_chars {
                chunk_end - self.overlap_chars
            } else {
                chunk_end
            };

            // Safety: ensure we're making progress
            if start >= text.len() || chunk_end == text.len() {
                break;
            }
        }

        chunks
    }
}

pub fn chunk_screen_text(
    chunker: &TextChunker,
    app_name: &str,
    window_title: &str,
    text: &str,
) -> Vec<String> {
    chunker
        .chunk_ocr_text_with_metadata(app_name, window_title, text)
        .into_iter()
        .map(|chunk| chunk.text)
        .collect()
}

fn line_kind_label(kind: LineKind) -> &'static str {
    match kind {
        LineKind::Title => "title",
        LineKind::Url => "url",
        LineKind::Search => "search",
        LineKind::Email => "email",
        LineKind::Code => "code",
        LineKind::Plain => "plain",
    }
}

fn classify_line(line: &str) -> LineKind {
    let lower = line.to_lowercase();
    if line.starts_with("http://") || line.starts_with("https://") || lower.contains("www.") {
        return LineKind::Url;
    }
    if lower.contains(" - ") && line.len() < 120 {
        return LineKind::Title;
    }

    let helper = TextChunker::new();
    if helper.is_email_like_line(line) {
        return LineKind::Email;
    }
    if helper.is_search_like_line(line) {
        return LineKind::Search;
    }
    if helper.is_code_like_line(line) {
        return LineKind::Code;
    }

    LineKind::Plain
}

fn normalize_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn overlap_tail(text: &str, target_chars: usize) -> String {
    if target_chars == 0 {
        return String::new();
    }

    let mut chars = 0usize;
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines().rev() {
        let normalized = normalize_line(line);
        if normalized.is_empty() {
            continue;
        }
        chars += normalized.len();
        lines.push(normalized);
        if chars >= target_chars {
            break;
        }
    }
    lines.reverse();
    lines.join("\n")
}

fn overlap_chars_for_text(text: &str) -> usize {
    let approx_tokens = approx_tokens(text);
    let target_tokens = ((approx_tokens as f32) * 0.24).round() as usize;
    target_tokens.clamp(MIN_CHUNK_TOKENS, CHUNK_OVERLAP) * CHARS_PER_TOKEN
}

fn approx_tokens(text: &str) -> usize {
    (text.len() / CHARS_PER_TOKEN).max(1)
}

fn stitch_chunks(left: &str, right: &str) -> String {
    let left = left.trim();
    let right = right.trim();

    if left.is_empty() {
        return right.to_string();
    }
    if right.is_empty() {
        return left.to_string();
    }
    if left == right || left.ends_with(right) {
        return left.to_string();
    }
    if right.starts_with(left) {
        return right.to_string();
    }

    format!("{left}\n{right}")
}

fn merge_short_orphans(mut chunks: Vec<TextChunk>) -> Vec<TextChunk> {
    if chunks.len() <= 1 {
        for (index, chunk) in chunks.iter_mut().enumerate() {
            chunk.chunk_index = index;
            chunk.approx_tokens = approx_tokens(&chunk.text);
        }
        return chunks;
    }

    let mut merged: Vec<TextChunk> = Vec::with_capacity(chunks.len());
    for mut chunk in chunks.drain(..) {
        chunk.approx_tokens = approx_tokens(&chunk.text);
        if chunk.approx_tokens < MIN_CHUNK_TOKENS {
            if let Some(previous) = merged.last_mut() {
                previous.text = stitch_chunks(&previous.text, &chunk.text);
                previous.approx_tokens = approx_tokens(&previous.text);
                continue;
            }
        }
        merged.push(chunk);
    }

    if merged.len() >= 2 && merged[0].approx_tokens < MIN_CHUNK_TOKENS {
        let first = merged.remove(0);
        if let Some(next) = merged.first_mut() {
            next.text = stitch_chunks(&first.text, &next.text);
            next.approx_tokens = approx_tokens(&next.text);
        } else {
            merged.push(first);
        }
    }

    for (index, chunk) in merged.iter_mut().enumerate() {
        chunk.chunk_index = index;
        chunk.approx_tokens = approx_tokens(&chunk.text);
    }

    merged
}

impl Default for TextChunker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_short_text_no_chunking() {
        let chunker = TextChunker::new();
        let text = "Hello world";
        let chunks = chunker.chunk(text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello world");
    }

    #[test]
    fn test_empty_ocr_text_produces_no_chunks() {
        let chunker = TextChunker::new();
        assert!(chunker
            .chunk_ocr_text("Chrome", "New Tab", "   \n\t")
            .is_empty());
    }

    #[test]
    fn test_ocr_chunking_removes_repeated_garbage_lines() {
        let chunker = TextChunker::new();
        let repeated = "syncing status syncing status\n".repeat(12);
        let text = format!("{repeated}\nPlanning launch checklist for FNDR search pipeline");
        let chunks = chunker.chunk_ocr_text("Chrome", "Launch Plan", &text);
        let merged = chunks.join("\n").to_lowercase();
        assert_eq!(merged.matches("syncing status syncing status").count(), 1);
        assert!(merged.contains("planning launch checklist"));
    }

    #[test]
    fn test_long_text_chunking() {
        let chunker = TextChunker::new();
        let text = "word ".repeat(500); // >2000 chars
        let chunks = chunker.chunk(&text);
        assert!(chunks.len() > 1);
        // Each chunk should be within limit
        for chunk in &chunks {
            assert!(chunk.len() <= chunker.max_chars + 50); // Allow some flexibility
        }
    }

    #[test]
    fn test_ocr_chunking_drops_chrome_lines() {
        let chunker = TextChunker::new();
        let text = "New Tab\nHome\nTrending\nPlanning launch checklist for FNDR search pipeline";
        let chunks = chunker.chunk_ocr_text("Chrome", "New Tab", text);
        let merged = chunks.join("\n").to_lowercase();
        assert!(merged.contains("planning launch checklist"));
        assert!(!merged.contains("new tab"));
        assert!(!merged.contains("trending"));
    }

    #[test]
    fn test_line_helpers() {
        let chunker = TextChunker::new();
        assert!(chunker.is_code_like_line("let x = foo(bar);"));
        assert!(chunker.is_email_like_line("Subject: Weekly update"));
        assert!(chunker.is_search_like_line("Search: best tennis racket"));
        assert!(chunker.is_low_signal_line("new tab"));
    }

    #[test]
    fn test_chunking_merges_short_orphan_tails() {
        let chunker = TextChunker::new();
        let text = format!("{} tail words", "alpha ".repeat(410));
        let chunks = chunker.chunk(&text);
        assert!(chunks
            .iter()
            .all(|chunk| chunk.split_whitespace().count() >= 15));
    }
}
