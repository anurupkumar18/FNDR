//! Database schema for memory records

use crate::config::{DEFAULT_IMAGE_EMBEDDING_DIM, DEFAULT_TEXT_EMBEDDING_DIM};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GitStats {
    #[serde(default)]
    pub added: i32,
    #[serde(default)]
    pub removed: i32,
    #[serde(default)]
    pub commits: i32,
}

fn default_text_embedding() -> Vec<f32> {
    vec![0.0; DEFAULT_TEXT_EMBEDDING_DIM]
}

fn default_image_embedding() -> Vec<f32> {
    vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM]
}

fn default_snippet_embedding() -> Vec<f32> {
    vec![0.0; DEFAULT_TEXT_EMBEDDING_DIM]
}

fn default_support_embedding() -> Vec<f32> {
    vec![0.0; DEFAULT_TEXT_EMBEDDING_DIM]
}

fn default_decay_score() -> f32 {
    1.0
}

fn default_summary_source() -> String {
    "fallback".to_string()
}

fn default_lexical_shadow() -> String {
    String::new()
}

/// A single memory record stored in the database
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
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
    /// Compact lexical hints preserved from dropped raw text for keyword recall
    #[serde(default = "default_lexical_shadow")]
    pub lexical_shadow: String,
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
    /// Embedding of the LLM/VLM snippet text (second semantic tower for search)
    #[serde(default = "default_snippet_embedding")]
    pub snippet_embedding: Vec<f32>,
    /// Centroid of representative high-signal chunks from the full text before compaction
    #[serde(default = "default_support_embedding")]
    pub support_embedding: Vec<f32>,
    /// Ebbinghaus decay score: starts at 1.0, decays toward 0.15 floor when not accessed
    #[serde(default = "default_decay_score")]
    pub decay_score: f32,
    /// Unix ms timestamp of last search access; used for decay computation
    #[serde(default)]
    pub last_accessed_at: i64,

    // V2 Memory Fields
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub activity_type: String,
    #[serde(default)]
    pub files_touched: Vec<String>,
    #[serde(default)]
    pub symbols_changed: Vec<String>,
    #[serde(default)]
    pub session_duration_mins: u32,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub entities: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub next_steps: Vec<String>,
    #[serde(default)]
    pub git_stats: Option<GitStats>,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub extraction_confidence: f32,
    #[serde(default)]
    pub dedup_fingerprint: String,
    #[serde(default)]
    pub embedding_text: String,
    #[serde(default)]
    pub embedding_model: String,
    #[serde(default)]
    pub embedding_dim: u32,
    #[serde(default)]
    pub is_consolidated: bool,
    #[serde(default)]
    pub is_soft_deleted: bool,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub related_ids: Vec<String>,
    #[serde(default)]
    pub consolidated_from: Vec<String>,
}

fn default_schema_version() -> u32 {
    1
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
    #[serde(default = "default_lexical_shadow")]
    pub lexical_shadow: String,
    pub score: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot_path: Option<String>,
    /// URL of the page (for browser windows)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Ebbinghaus decay score for this record (used in reranking)
    #[serde(default = "default_decay_score")]
    pub decay_score: f32,

    // V2 Search Results
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub activity_type: String,
    #[serde(default)]
    pub files_touched: Vec<String>,
    #[serde(default)]
    pub session_duration_mins: u32,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub extraction_confidence: f32,
    #[serde(default)]
    pub dedup_fingerprint: String,
    #[serde(default)]
    pub is_consolidated: bool,
    #[serde(default)]
    pub is_soft_deleted: bool,
}

/// Statistics about stored data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub total_records: usize,
    pub total_days: usize,
    pub apps: Vec<AppCount>,
    pub today_count: usize,
    pub unique_apps: usize,
    pub unique_sessions: usize,
    pub unique_window_titles: usize,
    pub unique_urls: usize,
    pub unique_domains: usize,
    pub records_with_url: usize,
    pub records_with_screenshot: usize,
    pub records_with_clean_text: usize,
    pub records_last_hour: usize,
    pub records_last_24h: usize,
    pub records_last_7d: usize,
    pub avg_records_per_active_day: f64,
    pub avg_records_per_hour: f64,
    pub focus_app_share_pct: f64,
    pub app_switches: usize,
    pub app_switch_rate_per_hour: f64,
    pub avg_gap_minutes: f64,
    pub longest_gap_minutes: u64,
    pub first_capture_ts: Option<i64>,
    pub last_capture_ts: Option<i64>,
    pub capture_span_hours: f64,
    pub current_streak_days: usize,
    pub longest_streak_days: usize,
    pub avg_ocr_confidence: f64,
    pub low_confidence_records: usize,
    pub avg_noise_score: f64,
    pub high_noise_records: usize,
    pub avg_ocr_blocks: f64,
    pub llm_count: usize,
    pub vlm_count: usize,
    pub fallback_count: usize,
    pub other_summary_count: usize,
    pub top_domains: Vec<DomainCount>,
    pub busiest_day: Option<DayCount>,
    pub quietest_day: Option<DayCount>,
    pub busiest_hour: Option<HourCount>,
    pub hourly_distribution: Vec<HourCount>,
    pub weekday_distribution: Vec<WeekdayCount>,
    pub daypart_distribution: Vec<DaypartCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppCount {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainCount {
    pub domain: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayCount {
    pub day: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourCount {
    pub hour: u8,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeekdayCount {
    pub weekday: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaypartCount {
    pub daypart: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskType {
    Todo,
    Reminder,
    Followup,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub description: String,
    pub source_app: String,
    pub source_memory_id: Option<String>,
    pub created_at: i64,
    pub due_date: Option<i64>,
    pub is_completed: bool,
    pub is_dismissed: bool,
    pub task_type: TaskType,
    #[serde(default)]
    pub linked_urls: Vec<String>,
    #[serde(default)]
    pub linked_memory_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MeetingBreakdown {
    pub summary: String,
    pub todos: Vec<String>,
    pub reminders: Vec<String>,
    pub followups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingSession {
    pub id: String,
    pub title: String,
    pub participants: Vec<String>,
    pub model: String,
    pub status: String,
    pub start_timestamp: i64,
    pub end_timestamp: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub segment_count: usize,
    pub duration_seconds: u64,
    pub meeting_dir: String,
    pub audio_dir: String,
    pub transcript_path: Option<String>,
    pub breakdown: Option<MeetingBreakdown>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingSegment {
    pub id: String,
    pub meeting_id: String,
    pub index: u32,
    pub start_timestamp: i64,
    pub end_timestamp: i64,
    pub text: String,
    pub audio_chunk_path: String,
    pub model: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeType {
    MemoryChunk,
    Entity,
    Task,
    Url,
    /// Clipboard item copied while in a session
    Clipboard,
    /// Audio/meeting transcript segment
    AudioSegment,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum EdgeType {
    #[serde(rename = "PART_OF_SESSION")]
    PartOfSession,
    #[serde(rename = "REFERENCE_FOR_TASK")]
    ReferenceForTask,
    #[serde(rename = "OCCURRED_AT")]
    OccurredAt,
    /// Clipboard item was copied during this memory chunk's session
    #[serde(rename = "CLIPBOARD_COPIED")]
    ClipboardCopied,
    /// Memory chunk co-occurred with an audio/meeting segment
    #[serde(rename = "OCCURRED_DURING_AUDIO")]
    OccurredDuringAudio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNode {
    pub id: String,
    pub node_type: NodeType,
    pub label: String,
    pub created_at: i64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub edge_type: EdgeType,
    pub timestamp: i64,
    pub metadata: serde_json::Value,
}
