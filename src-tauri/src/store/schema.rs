//! Database schema for memory records

use serde::{Deserialize, Serialize};

fn default_text_embedding() -> Vec<f32> {
    vec![0.0; 384]
}

fn default_image_embedding() -> Vec<f32> {
    vec![0.0; 512]
}

fn default_summary_source() -> String {
    "fallback".to_string()
}

/// A single memory record stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    /// Unique identifier
    pub id: String,
    /// Unix timestamp in milliseconds
    pub timestamp: i64,
    /// Day bucket for grouping (YYYY-MM-DD)
    #[serde(default)]
    pub day_bucket: String,
    /// Application name
    pub app_name: String,
    /// Application bundle identifier
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    /// Window title
    pub window_title: String,
    /// Session identifier for temporal grouping
    #[serde(default)]
    pub session_id: String,
    /// Extracted text content
    pub text: String,
    /// OCR-cleaned text used for embedding/search quality decisions
    #[serde(default)]
    pub clean_text: String,
    /// OCR average confidence (0-1)
    #[serde(default)]
    pub ocr_confidence: f32,
    /// OCR block count retained after filtering
    #[serde(default)]
    pub ocr_block_count: u32,
    /// Concise summary
    pub snippet: String,
    /// Summary provenance: llm, vlm, fallback
    #[serde(default = "default_summary_source")]
    pub summary_source: String,
    /// Higher values indicate noisier OCR payloads
    #[serde(default)]
    pub noise_score: f32,
    /// Session-level grouping key for downstream synthesis
    #[serde(default)]
    pub session_key: String,
    /// Text embedding vector
    #[serde(default = "default_text_embedding")]
    pub embedding: Vec<f32>,
    /// Image embedding vector (CLIP-compatible dimension)
    #[serde(default = "default_image_embedding")]
    pub image_embedding: Vec<f32>,
    /// Persisted screenshot path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot_path: Option<String>,
    /// URL of the page (for browser windows)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Search result returned to the UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub timestamp: i64,
    pub app_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    pub window_title: String,
    #[serde(default)]
    pub session_id: String,
    pub text: String,
    #[serde(default)]
    pub clean_text: String,
    #[serde(default)]
    pub ocr_confidence: f32,
    #[serde(default)]
    pub ocr_block_count: u32,
    pub snippet: String,
    #[serde(default = "default_summary_source")]
    pub summary_source: String,
    #[serde(default)]
    pub noise_score: f32,
    #[serde(default)]
    pub session_key: String,
    pub score: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot_path: Option<String>,
    /// URL of the page (for browser windows)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Statistics about stored data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub total_records: usize,
    pub total_days: usize,
    pub apps: Vec<AppCount>,
    pub today_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppCount {
    pub name: String,
    pub count: usize,
}
