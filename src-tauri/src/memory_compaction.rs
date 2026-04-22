//! Helpers for compacting persisted memory payloads.

use crate::store::MemoryRecord;

const SUMMARY_CLEAN_TEXT_CHARS: usize = 360;
const FALLBACK_CLEAN_TEXT_CHARS: usize = 560;
const GENERIC_CLEAN_TEXT_CHARS: usize = 420;
const EMBEDDING_MIN_NORM: f32 = 1e-6;

pub fn compact_clean_text(summary_source: &str, snippet: &str, clean_text: &str) -> String {
    let normalized_snippet = normalize_memory_text(snippet);
    if !normalized_snippet.is_empty() {
        let limit = match summary_source.trim().to_ascii_lowercase().as_str() {
            "llm" | "vlm" => SUMMARY_CLEAN_TEXT_CHARS,
            "fallback" => FALLBACK_CLEAN_TEXT_CHARS,
            _ => GENERIC_CLEAN_TEXT_CHARS,
        };
        return trim_chars(&normalized_snippet, limit);
    }

    let normalized_clean = normalize_memory_text(clean_text);
    if normalized_clean.is_empty() {
        String::new()
    } else {
        let limit = if summary_source.trim().eq_ignore_ascii_case("fallback") {
            FALLBACK_CLEAN_TEXT_CHARS
        } else {
            GENERIC_CLEAN_TEXT_CHARS
        };
        trim_chars(&normalized_clean, limit)
    }
}

pub fn compact_memory_record_payload(record: &MemoryRecord) -> MemoryRecord {
    let mut compacted = record.clone();
    compacted.text = String::new();
    compacted.clean_text =
        compact_clean_text(&record.summary_source, &record.snippet, &record.clean_text);
    compacted.screenshot_path = None;
    compacted
}

pub fn best_embedding_text(record: &MemoryRecord) -> String {
    let clean = normalize_memory_text(&record.clean_text);
    if !clean.is_empty() {
        return clean;
    }
    let snippet = normalize_memory_text(&record.snippet);
    if !snippet.is_empty() {
        return snippet;
    }
    normalize_memory_text(&record.window_title)
}

pub fn best_snippet_embedding_text(record: &MemoryRecord) -> String {
    let snippet = normalize_memory_text(&record.snippet);
    if !snippet.is_empty() {
        return snippet;
    }
    let clean = normalize_memory_text(&record.clean_text);
    if !clean.is_empty() {
        return trim_chars(&clean, SUMMARY_CLEAN_TEXT_CHARS);
    }
    normalize_memory_text(&record.window_title)
}

pub fn is_low_signal_embedding(vector: &[f32]) -> bool {
    if vector.is_empty() {
        return true;
    }
    let mut norm = 0.0f32;
    for value in vector {
        if !value.is_finite() {
            return true;
        }
        norm += value * value;
    }
    norm.sqrt() <= EMBEDDING_MIN_NORM
}

fn normalize_memory_text(raw: &str) -> String {
    raw.chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn trim_chars(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(snippet: &str, clean_text: &str) -> MemoryRecord {
        MemoryRecord {
            id: "memory-1".to_string(),
            timestamp: 1,
            day_bucket: "2026-04-21".to_string(),
            app_name: "Chrome".to_string(),
            bundle_id: None,
            window_title: "Title".to_string(),
            session_id: "session-1".to_string(),
            text: "raw ocr payload".to_string(),
            clean_text: clean_text.to_string(),
            ocr_confidence: 0.9,
            ocr_block_count: 4,
            snippet: snippet.to_string(),
            summary_source: "llm".to_string(),
            noise_score: 0.1,
            session_key: "session-key".to_string(),
            embedding: vec![0.1; 384],
            image_embedding: vec![0.0; 512],
            screenshot_path: Some("/tmp/screenshot.png".to_string()),
            url: None,
            snippet_embedding: vec![0.2; 384],
            decay_score: 1.0,
            last_accessed_at: 0,
        }
    }

    #[test]
    fn compaction_prefers_snippet_and_clears_payload_fields() {
        let source = record(
            "Discussed fixing memory reclaim and preserving embeddings.",
            "very long raw ocr text should not remain",
        );
        let compacted = compact_memory_record_payload(&source);

        assert!(compacted.text.is_empty());
        assert_eq!(compacted.clean_text, normalize_memory_text(&source.snippet));
        assert!(compacted.screenshot_path.is_none());
    }

    #[test]
    fn low_signal_embedding_detects_zero_vectors() {
        assert!(is_low_signal_embedding(&[0.0; 384]));
        assert!(!is_low_signal_embedding(&[0.01; 384]));
    }
}
