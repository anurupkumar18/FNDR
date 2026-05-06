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

fn default_content_hash() -> String {
    String::new()
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
    /// User-facing one-sentence summary rendered in cards/search.
    #[serde(default)]
    pub display_summary: String,
    /// Internal synthesis context retained for downstream processing and diagnostics.
    #[serde(default)]
    pub internal_context: String,
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
    pub anchor_coverage_score: f32,
    #[serde(default = "default_content_hash")]
    pub content_hash: String,
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
    2
}

impl Default for MemoryRecord {
    fn default() -> Self {
        Self {
            id: String::new(),
            timestamp: 0,
            day_bucket: String::new(),
            app_name: String::new(),
            bundle_id: None,
            window_title: String::new(),
            session_id: String::new(),
            text: String::new(),
            clean_text: String::new(),
            ocr_confidence: 0.0,
            ocr_block_count: 0,
            snippet: String::new(),
            display_summary: String::new(),
            internal_context: String::new(),
            summary_source: default_summary_source(),
            noise_score: 0.0,
            session_key: String::new(),
            lexical_shadow: default_lexical_shadow(),
            embedding: default_text_embedding(),
            image_embedding: default_image_embedding(),
            screenshot_path: None,
            url: None,
            snippet_embedding: default_snippet_embedding(),
            support_embedding: default_support_embedding(),
            decay_score: default_decay_score(),
            last_accessed_at: 0,
            schema_version: default_schema_version(),
            activity_type: String::new(),
            files_touched: Vec::new(),
            symbols_changed: Vec::new(),
            session_duration_mins: 0,
            project: String::new(),
            tags: Vec::new(),
            entities: Vec::new(),
            decisions: Vec::new(),
            errors: Vec::new(),
            next_steps: Vec::new(),
            git_stats: None,
            outcome: String::new(),
            extraction_confidence: 0.0,
            anchor_coverage_score: 0.0,
            content_hash: default_content_hash(),
            dedup_fingerprint: String::new(),
            embedding_text: String::new(),
            embedding_model: crate::config::DEFAULT_EMBEDDING_MODEL_NAME.to_string(),
            embedding_dim: DEFAULT_TEXT_EMBEDDING_DIM as u32,
            is_consolidated: false,
            is_soft_deleted: false,
            parent_id: None,
            related_ids: Vec::new(),
            consolidated_from: Vec::new(),
        }
    }
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
    #[serde(default)]
    pub display_summary: String,
    #[serde(default)]
    pub internal_context: String,
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
    pub anchor_coverage_score: f32,
    #[serde(default)]
    pub extracted_entities: Vec<String>,
    #[serde(default = "default_content_hash")]
    pub content_hash: String,
    #[serde(default)]
    pub dedup_fingerprint: String,
    #[serde(default)]
    pub is_consolidated: bool,
    #[serde(default)]
    pub is_soft_deleted: bool,
}

impl Default for SearchResult {
    fn default() -> Self {
        Self {
            id: String::new(),
            timestamp: 0,
            app_name: String::new(),
            bundle_id: None,
            window_title: String::new(),
            session_id: String::new(),
            text: String::new(),
            clean_text: String::new(),
            ocr_confidence: 0.0,
            ocr_block_count: 0,
            snippet: String::new(),
            display_summary: String::new(),
            internal_context: String::new(),
            summary_source: default_summary_source(),
            noise_score: 0.0,
            session_key: String::new(),
            lexical_shadow: default_lexical_shadow(),
            score: 0.0,
            screenshot_path: None,
            url: None,
            decay_score: default_decay_score(),
            schema_version: default_schema_version(),
            activity_type: String::new(),
            files_touched: Vec::new(),
            session_duration_mins: 0,
            project: String::new(),
            tags: Vec::new(),
            outcome: String::new(),
            extraction_confidence: 0.0,
            anchor_coverage_score: 0.0,
            extracted_entities: Vec::new(),
            content_hash: default_content_hash(),
            dedup_fingerprint: String::new(),
            is_consolidated: false,
            is_soft_deleted: false,
        }
    }
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
    Project,
    File,
    Error,
    Command,
    Decision,
    AgentSession,
    ActivityEvent,
    Issue,
    Concept,
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
    #[serde(rename = "BELONGS_TO")]
    BelongsTo,
    #[serde(rename = "MENTIONED_IN")]
    MentionedIn,
    #[serde(rename = "EDITED_FILE")]
    EditedFile,
    #[serde(rename = "FIXED_BY")]
    FixedBy,
    #[serde(rename = "BLOCKED_BY")]
    BlockedBy,
    #[serde(rename = "INFORMED_BY")]
    InformedBy,
    #[serde(rename = "RESULTED_IN")]
    ResultedIn,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    Public,
    #[default]
    Project,
    Personal,
    Sensitive,
    Secret,
    Blocked,
    Ephemeral,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvidenceRef {
    pub id: String,
    pub source_type: String,
    pub source_id: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub snippet: String,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub privacy_class: PrivacyClass,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntityRef {
    pub canonical_id: String,
    pub canonical_name: String,
    pub entity_type: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ActivityEvent {
    pub id: String,
    pub memory_id: String,
    pub start_time: i64,
    pub end_time: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default)]
    pub activity_type: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub entities: Vec<EntityRef>,
    #[serde(default)]
    pub source_memory_ids: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<EvidenceRef>,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub memory_value: f32,
    #[serde(default)]
    pub privacy_class: PrivacyClass,
    #[serde(default)]
    pub active_files: Vec<String>,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub next_steps: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RelevantFile {
    pub path: String,
    pub why: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DecisionSummary {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IssueSummary {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FailureSummary {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub related_files: Vec<String>,
    #[serde(default)]
    pub last_seen_at: i64,
    #[serde(default)]
    pub evidence: Vec<EvidenceRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextTask {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectContext {
    pub id: String,
    pub project: String,
    #[serde(default)]
    pub active_goal: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub relevant_files: Vec<RelevantFile>,
    #[serde(default)]
    pub recent_decisions: Vec<DecisionSummary>,
    #[serde(default)]
    pub open_issues: Vec<IssueSummary>,
    #[serde(default)]
    pub known_failures: Vec<FailureSummary>,
    #[serde(default)]
    pub open_tasks: Vec<ContextTask>,
    #[serde(default)]
    pub constraints: Vec<String>,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub privacy_class: PrivacyClass,
    #[serde(default)]
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DecisionLedgerEntry {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub proposed_by: String,
    #[serde(default)]
    pub evidence: Vec<EvidenceRef>,
    #[serde(default)]
    pub privacy_class: PrivacyClass,
    #[serde(default)]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextPackItemReason {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExcludedContextItem {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextPack {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub generated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default)]
    pub agent_type: String,
    #[serde(default)]
    pub budget_tokens: u32,
    #[serde(default)]
    pub tokens_used: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_goal: Option<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub relevant_files: Vec<RelevantFile>,
    #[serde(default)]
    pub recent_decisions: Vec<DecisionSummary>,
    #[serde(default)]
    pub open_issues: Vec<IssueSummary>,
    #[serde(default)]
    pub known_failures: Vec<FailureSummary>,
    #[serde(default)]
    pub open_tasks: Vec<ContextTask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_next_action: Option<String>,
    #[serde(default)]
    pub do_not_do: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<EvidenceRef>,
    #[serde(default)]
    pub included: Vec<ContextPackItemReason>,
    #[serde(default)]
    pub excluded: Vec<ExcludedContextItem>,
    #[serde(default)]
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextDelta {
    pub id: String,
    pub session_id: String,
    #[serde(default)]
    pub since: i64,
    #[serde(default)]
    pub generated_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default)]
    pub new_events: Vec<ActivityEvent>,
    #[serde(default)]
    pub changed_entities: Vec<EntityRef>,
    #[serde(default)]
    pub resolved_tasks: Vec<ContextTask>,
    #[serde(default)]
    pub new_failures: Vec<FailureSummary>,
    #[serde(default)]
    pub new_items: Vec<String>,
    #[serde(default)]
    pub tokens_used: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommandEvent {
    pub command: String,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ErrorEvent {
    pub error: String,
    #[serde(default)]
    pub timestamp: i64,
    #[serde(default)]
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CommitRef {
    pub sha: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodeContext {
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default)]
    pub active_files: Vec<String>,
    #[serde(default)]
    pub related_files: Vec<RelevantFile>,
    #[serde(default)]
    pub recent_commands: Vec<CommandEvent>,
    #[serde(default)]
    pub recent_errors: Vec<ErrorEvent>,
    #[serde(default)]
    pub recent_commits: Vec<CommitRef>,
    #[serde(default)]
    pub relevant_decisions: Vec<DecisionSummary>,
    #[serde(default)]
    pub unresolved_tasks: Vec<ContextTask>,
    #[serde(default)]
    pub recommended_context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkingState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_goal: Option<String>,
    #[serde(default)]
    pub recent_events: Vec<ActivityEvent>,
    #[serde(default)]
    pub relevant_files: Vec<RelevantFile>,
    #[serde(default)]
    pub open_tasks: Vec<ContextTask>,
    #[serde(default)]
    pub known_failures: Vec<FailureSummary>,
    #[serde(default)]
    pub recent_commands: Vec<String>,
    #[serde(default)]
    pub recent_errors: Vec<String>,
    #[serde(default)]
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HealthStatus {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub index_ready: bool,
    #[serde(default)]
    pub embedding_model: String,
    #[serde(default)]
    pub embedding_dimension: u32,
    #[serde(default)]
    pub model_status: String,
    #[serde(default)]
    pub failed_jobs: u32,
    #[serde(default)]
    pub queue_lag_ms: u64,
    #[serde(default)]
    pub storage_usage_bytes: u64,
    #[serde(default)]
    pub runtime_tables: Vec<String>,
    #[serde(default)]
    pub degraded_reasons: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_context_pack_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextRuntimeStatus {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub mcp_running: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_context_pack: Option<String>,
    #[serde(default)]
    pub recent_pack_count: usize,
    #[serde(default)]
    pub activity_event_count: usize,
    #[serde(default)]
    pub decision_count: usize,
    #[serde(default)]
    pub failed_writes: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_pack_summary: Option<String>,
    #[serde(default)]
    pub latest_pack_tokens_used: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntityAliasRecord {
    pub alias_key: String,
    pub canonical_id: String,
    pub canonical_name: String,
    pub entity_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub updated_at: i64,
}
