//! Configuration management

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const DEFAULT_TEXT_EMBEDDING_DIM: usize = 1024;
pub const DEFAULT_IMAGE_EMBEDDING_DIM: usize = 512;
pub const DEFAULT_EMBEDDING_MODEL_NAME: &str = "bge-large-en-v1.5";
pub const DEFAULT_EMBEDDING_MODEL_FILENAME: &str = "bge-large-en-v1.5-quantized.onnx";
pub const DEFAULT_EMBEDDING_TOKENIZER_FILENAME: &str = "tokenizer.json";
pub const DEFAULT_EMBEDDING_MAX_SEQ_LEN: usize = 512;
pub const DEFAULT_EMBEDDING_CACHE_CAPACITY: usize = 1024;
pub const DEFAULT_EMBEDDING_MAX_BATCH: usize = 16;

pub const DEFAULT_CHUNK_MAX_TOKENS: usize = 450;
pub const DEFAULT_CHUNK_OVERLAP_TOKENS: usize = 96;
pub const DEFAULT_CHUNK_MIN_TOKENS: usize = 15;
pub const DEFAULT_CHARS_PER_TOKEN: usize = 4;

pub const DEFAULT_SEARCH_CANDIDATE_MULTIPLIER: usize = 4;
pub const DEFAULT_SEARCH_MAX_RERANK_POOL: usize = 36;
pub const DEFAULT_SEARCH_VECTOR_WEIGHT: f32 = 0.44;
pub const DEFAULT_SEARCH_SNIPPET_WEIGHT: f32 = 0.18;
pub const DEFAULT_SEARCH_KEYWORD_WEIGHT: f32 = 0.38;
pub const DEFAULT_SEARCH_SEMANTIC_TIMEOUT_MS: u64 = 950;
pub const DEFAULT_SEARCH_SNIPPET_TIMEOUT_MS: u64 = 760;
pub const DEFAULT_SEARCH_KEYWORD_TIMEOUT_MS: u64 = 900;
pub const DEFAULT_SEARCH_KEYWORD_VARIANT_TIMEOUT_MS: u64 = 320;

/// Local text embedding configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbeddingConfig {
    #[serde(default = "default_embedding_model_name")]
    pub model_name: String,
    #[serde(default = "default_embedding_model_filename")]
    pub model_filename: String,
    #[serde(default = "default_embedding_tokenizer_filename")]
    pub tokenizer_filename: String,
    #[serde(default = "default_text_embedding_dim")]
    pub dimension: usize,
    #[serde(default = "default_embedding_max_seq_len")]
    pub max_sequence_length: usize,
    #[serde(default = "default_embedding_cache_capacity")]
    pub cache_capacity: usize,
    #[serde(default = "default_embedding_max_batch")]
    pub max_batch_size: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model_name: default_embedding_model_name(),
            model_filename: default_embedding_model_filename(),
            tokenizer_filename: default_embedding_tokenizer_filename(),
            dimension: default_text_embedding_dim(),
            max_sequence_length: default_embedding_max_seq_len(),
            cache_capacity: default_embedding_cache_capacity(),
            max_batch_size: default_embedding_max_batch(),
        }
    }
}

impl EmbeddingConfig {
    pub fn normalized(mut self) -> Self {
        self.model_name = self.model_name.trim().to_string();
        if self.model_name.is_empty() {
            self.model_name = default_embedding_model_name();
        }
        self.model_filename = self.model_filename.trim().to_string();
        if self.model_filename.is_empty() {
            self.model_filename = default_embedding_model_filename();
        }
        self.tokenizer_filename = self.tokenizer_filename.trim().to_string();
        if self.tokenizer_filename.is_empty() {
            self.tokenizer_filename = default_embedding_tokenizer_filename();
        }
        self.dimension = self.dimension.clamp(128, 4096);
        self.max_sequence_length = self.max_sequence_length.clamp(16, 1024);
        self.cache_capacity = self.cache_capacity.clamp(64, 16_384);
        self.max_batch_size = self.max_batch_size.clamp(1, 128);
        self
    }
}

/// OCR-aware chunking configuration used before embedding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ChunkingConfig {
    #[serde(default = "default_chunk_max_tokens")]
    pub max_tokens: usize,
    #[serde(default = "default_chunk_overlap_tokens")]
    pub overlap_tokens: usize,
    #[serde(default = "default_chunk_min_tokens")]
    pub min_tokens: usize,
    #[serde(default = "default_chars_per_token")]
    pub chars_per_token: usize,
}

impl Default for ChunkingConfig {
    fn default() -> Self {
        Self {
            max_tokens: default_chunk_max_tokens(),
            overlap_tokens: default_chunk_overlap_tokens(),
            min_tokens: default_chunk_min_tokens(),
            chars_per_token: default_chars_per_token(),
        }
    }
}

impl ChunkingConfig {
    pub fn normalized(mut self) -> Self {
        self.chars_per_token = self.chars_per_token.clamp(2, 8);
        self.max_tokens = self.max_tokens.clamp(64, 1024);
        self.min_tokens = self.min_tokens.clamp(1, self.max_tokens / 2);
        self.overlap_tokens = self.overlap_tokens.min(self.max_tokens / 2);
        self
    }
}

/// Search and reranking knobs. Stored as primitive values so the TOML remains readable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchConfig {
    #[serde(default = "default_search_candidate_multiplier")]
    pub candidate_multiplier: usize,
    #[serde(default = "default_search_max_rerank_pool")]
    pub max_rerank_pool: usize,
    #[serde(default = "default_search_vector_weight")]
    pub vector_weight: f32,
    #[serde(default = "default_search_snippet_weight")]
    pub snippet_weight: f32,
    #[serde(default = "default_search_keyword_weight")]
    pub keyword_weight: f32,
    #[serde(default = "default_search_semantic_timeout_ms")]
    pub semantic_timeout_ms: u64,
    #[serde(default = "default_search_snippet_timeout_ms")]
    pub snippet_timeout_ms: u64,
    #[serde(default = "default_search_keyword_timeout_ms")]
    pub keyword_timeout_ms: u64,
    #[serde(default = "default_search_keyword_variant_timeout_ms")]
    pub keyword_variant_timeout_ms: u64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            candidate_multiplier: default_search_candidate_multiplier(),
            max_rerank_pool: default_search_max_rerank_pool(),
            vector_weight: default_search_vector_weight(),
            snippet_weight: default_search_snippet_weight(),
            keyword_weight: default_search_keyword_weight(),
            semantic_timeout_ms: default_search_semantic_timeout_ms(),
            snippet_timeout_ms: default_search_snippet_timeout_ms(),
            keyword_timeout_ms: default_search_keyword_timeout_ms(),
            keyword_variant_timeout_ms: default_search_keyword_variant_timeout_ms(),
        }
    }
}

impl SearchConfig {
    pub fn normalized(mut self) -> Self {
        self.candidate_multiplier = self.candidate_multiplier.clamp(1, 12);
        self.max_rerank_pool = self.max_rerank_pool.clamp(4, 200);
        self.vector_weight = self.vector_weight.clamp(0.0, 1.0);
        self.snippet_weight = self.snippet_weight.clamp(0.0, 1.0);
        self.keyword_weight = self.keyword_weight.clamp(0.0, 1.0);
        let total = self.vector_weight + self.snippet_weight + self.keyword_weight;
        if total > f32::EPSILON {
            self.vector_weight /= total;
            self.snippet_weight /= total;
            self.keyword_weight /= total;
        }
        self.semantic_timeout_ms = self.semantic_timeout_ms.clamp(100, 10_000);
        self.snippet_timeout_ms = self.snippet_timeout_ms.clamp(100, 10_000);
        self.keyword_timeout_ms = self.keyword_timeout_ms.clamp(100, 10_000);
        self.keyword_variant_timeout_ms = self.keyword_variant_timeout_ms.clamp(50, 5_000);
        self
    }
}

/// Auto-fill configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutofillConfig {
    /// Whether global screen auto-fill is enabled.
    #[serde(default = "default_autofill_enabled")]
    pub enabled: bool,
    /// Global shortcut in tauri/global-hotkey format, e.g. `Alt+F`.
    #[serde(default = "default_autofill_shortcut")]
    pub shortcut: String,
    /// How far back semantic retrieval should search.
    #[serde(default = "default_autofill_lookback_days")]
    pub lookback_days: u32,
    /// Confidence threshold above which FNDR injects without confirmation.
    #[serde(default = "default_autofill_auto_inject_threshold")]
    pub auto_inject_threshold: f32,
    /// Whether FNDR should prefer system-style typing when the target app remains frontmost.
    #[serde(default = "default_autofill_prefer_typed_injection")]
    pub prefer_typed_injection: bool,
    /// Maximum number of candidates to return for quick-pick conflict resolution.
    #[serde(default = "default_autofill_max_candidates")]
    pub max_candidates: usize,
}

impl Default for AutofillConfig {
    fn default() -> Self {
        Self {
            enabled: default_autofill_enabled(),
            shortcut: default_autofill_shortcut(),
            lookback_days: default_autofill_lookback_days(),
            auto_inject_threshold: default_autofill_auto_inject_threshold(),
            prefer_typed_injection: default_autofill_prefer_typed_injection(),
            max_candidates: default_autofill_max_candidates(),
        }
    }
}

impl AutofillConfig {
    pub fn normalized(mut self) -> Self {
        self.shortcut = self.shortcut.trim().to_string();
        if self.shortcut.is_empty() {
            self.shortcut = default_autofill_shortcut();
        }
        self.lookback_days = self.lookback_days.clamp(7, 365);
        self.auto_inject_threshold = self.auto_inject_threshold.clamp(0.55, 0.995);
        self.max_candidates = self.max_candidates.clamp(1, 6);
        self
    }
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Base capture FPS (0.5 - 1.0 recommended)
    pub fps_base: f64,
    /// Seconds of idle before reducing FPS
    pub idle_pause_seconds: u64,
    /// FPS when idle
    pub idle_fps: f64,
    /// Perceptual hash threshold for deduplication (0-64, lower = stricter)
    pub dedupe_threshold: u32,
    /// Force capture every N seconds even if duplicate
    pub forced_capture_interval: u64,
    /// Days to retain records
    pub retention_days: u32,
    /// Blocked application names
    pub blocklist: Vec<String>,
    /// Enable pattern redaction (emails, credit cards)
    pub redact_mode: bool,
    /// Minimum text length to store
    pub min_text_length: usize,
    /// Enable VLM for intelligent image understanding
    #[serde(default = "default_use_vlm")]
    pub use_vlm: bool,
    /// VLM model size: "4B" (primary)
    #[serde(default = "default_vlm_model_size")]
    pub vlm_model_size: String,
    /// Days to retain screenshot files on disk (records kept; only pixel data deleted). 0 = keep forever.
    #[serde(default = "default_screenshot_retention_days")]
    pub screenshot_retention_days: u32,
    /// Enable proactive surface: nudges when current screen is semantically close to old unresolved context.
    #[serde(default = "default_proactive_surface_enabled")]
    pub proactive_surface_enabled: bool,
    /// Half-life for Ebbinghaus memory decay in days. Records decay toward 0.15 floor over time.
    #[serde(default = "default_decay_half_life_days")]
    pub decay_half_life_days: u32,
    /// Intelligent Screen Auto-Fill configuration.
    #[serde(default)]
    pub autofill: AutofillConfig,
    /// Authoritative local embedding model contract.
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    /// OCR-aware chunking knobs.
    #[serde(default)]
    pub chunking: ChunkingConfig,
    /// Hybrid search weights, limits, and timeouts.
    #[serde(default)]
    pub search: SearchConfig,
}

fn default_embedding_model_name() -> String {
    DEFAULT_EMBEDDING_MODEL_NAME.to_string()
}

fn default_embedding_model_filename() -> String {
    DEFAULT_EMBEDDING_MODEL_FILENAME.to_string()
}

fn default_embedding_tokenizer_filename() -> String {
    DEFAULT_EMBEDDING_TOKENIZER_FILENAME.to_string()
}

fn default_text_embedding_dim() -> usize {
    DEFAULT_TEXT_EMBEDDING_DIM
}

fn default_embedding_max_seq_len() -> usize {
    DEFAULT_EMBEDDING_MAX_SEQ_LEN
}

fn default_embedding_cache_capacity() -> usize {
    DEFAULT_EMBEDDING_CACHE_CAPACITY
}

fn default_embedding_max_batch() -> usize {
    DEFAULT_EMBEDDING_MAX_BATCH
}

fn default_chunk_max_tokens() -> usize {
    DEFAULT_CHUNK_MAX_TOKENS
}

fn default_chunk_overlap_tokens() -> usize {
    DEFAULT_CHUNK_OVERLAP_TOKENS
}

fn default_chunk_min_tokens() -> usize {
    DEFAULT_CHUNK_MIN_TOKENS
}

fn default_chars_per_token() -> usize {
    DEFAULT_CHARS_PER_TOKEN
}

fn default_search_candidate_multiplier() -> usize {
    DEFAULT_SEARCH_CANDIDATE_MULTIPLIER
}

fn default_search_max_rerank_pool() -> usize {
    DEFAULT_SEARCH_MAX_RERANK_POOL
}

fn default_search_vector_weight() -> f32 {
    DEFAULT_SEARCH_VECTOR_WEIGHT
}

fn default_search_snippet_weight() -> f32 {
    DEFAULT_SEARCH_SNIPPET_WEIGHT
}

fn default_search_keyword_weight() -> f32 {
    DEFAULT_SEARCH_KEYWORD_WEIGHT
}

fn default_search_semantic_timeout_ms() -> u64 {
    DEFAULT_SEARCH_SEMANTIC_TIMEOUT_MS
}

fn default_search_snippet_timeout_ms() -> u64 {
    DEFAULT_SEARCH_SNIPPET_TIMEOUT_MS
}

fn default_search_keyword_timeout_ms() -> u64 {
    DEFAULT_SEARCH_KEYWORD_TIMEOUT_MS
}

fn default_search_keyword_variant_timeout_ms() -> u64 {
    DEFAULT_SEARCH_KEYWORD_VARIANT_TIMEOUT_MS
}

fn default_use_vlm() -> bool {
    true
}

fn default_vlm_model_size() -> String {
    "4B".to_string()
}

fn default_screenshot_retention_days() -> u32 {
    30
}

fn default_proactive_surface_enabled() -> bool {
    true
}

fn default_decay_half_life_days() -> u32 {
    21
}

fn default_autofill_enabled() -> bool {
    true
}

fn default_autofill_shortcut() -> String {
    "Alt+F".to_string()
}

fn default_autofill_lookback_days() -> u32 {
    90
}

fn default_autofill_auto_inject_threshold() -> f32 {
    0.90
}

fn default_autofill_prefer_typed_injection() -> bool {
    true
}

fn default_autofill_max_candidates() -> usize {
    4
}

impl Default for Config {
    fn default() -> Self {
        Self {
            fps_base: 0.5,
            idle_pause_seconds: 5,
            idle_fps: 0.2,
            dedupe_threshold: 5,
            forced_capture_interval: 60,
            retention_days: 7,
            blocklist: vec![
                "1Password".to_string(),
                "Keychain Access".to_string(),
                "System Preferences".to_string(),
                "System Settings".to_string(),
            ],
            redact_mode: false,
            min_text_length: 20,
            use_vlm: true,
            vlm_model_size: "4B".to_string(),
            screenshot_retention_days: 30,
            proactive_surface_enabled: true,
            decay_half_life_days: 21,
            autofill: AutofillConfig::default(),
            embedding: EmbeddingConfig::default(),
            chunking: ChunkingConfig::default(),
            search: SearchConfig::default(),
        }
    }
}

impl Config {
    pub fn normalized(mut self) -> Self {
        self.autofill = self.autofill.normalized();
        self.embedding = self.embedding.normalized();
        self.chunking = self.chunking.normalized();
        self.search = self.search.normalized();
        self.fps_base = self.fps_base.clamp(0.05, 4.0);
        self.idle_fps = self.idle_fps.clamp(0.02, self.fps_base.max(0.02));
        self.idle_pause_seconds = self.idle_pause_seconds.clamp(1, 3600);
        self.dedupe_threshold = self.dedupe_threshold.min(64);
        self.forced_capture_interval = self.forced_capture_interval.clamp(5, 3600);
        self.retention_days = self.retention_days.min(3650);
        self.min_text_length = self.min_text_length.clamp(1, 2000);
        self.screenshot_retention_days = self.screenshot_retention_days.min(3650);
        self.decay_half_life_days = self.decay_half_life_days.clamp(1, 3650);
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.embedding.dimension == 0 {
            return Err("Embedding dimension must be greater than zero".to_string());
        }
        if self.embedding.dimension != DEFAULT_TEXT_EMBEDDING_DIM {
            return Err(format!(
                "This FNDR build expects {}-dimensional text embeddings, but config.toml sets {}. Change the embedding model, schema, and config together before using a non-default dimension.",
                DEFAULT_TEXT_EMBEDDING_DIM,
                self.embedding.dimension
            ));
        }
        if self.embedding.model_filename.trim().is_empty()
            || self.embedding.tokenizer_filename.trim().is_empty()
        {
            return Err("Embedding model and tokenizer filenames must be configured".to_string());
        }
        if self.search.vector_weight + self.search.snippet_weight + self.search.keyword_weight
            <= f32::EPSILON
        {
            return Err("At least one hybrid search weight must be non-zero".to_string());
        }
        Ok(())
    }

    /// Load configuration from file or create default
    pub fn load_or_create() -> Result<Self, Box<dyn std::error::Error>> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            let config = config.normalized();
            config.validate().map_err(|err| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid FNDR config: {err}"),
                )
            })?;
            Ok(config)
        } else {
            let config = Config::default().normalized();
            config.validate().map_err(|err| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Invalid FNDR config: {err}"),
                )
            })?;
            config.save()?;
            Ok(config)
        }
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)?;
        Ok(())
    }

    fn config_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
        let dirs = directories::ProjectDirs::from("com", "fndr", "FNDR")
            .ok_or("Could not determine config directory")?;
        Ok(dirs.config_dir().join("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_validates() {
        Config::default()
            .normalized()
            .validate()
            .expect("default config should stay internally consistent");
    }

    #[test]
    fn rejects_embedding_dimension_mismatch() {
        let mut config = Config::default();
        config.embedding.dimension = 384;

        let err = config
            .normalized()
            .validate()
            .expect_err("wrong embedding dimension must fail at startup");

        assert!(err.contains("1024-dimensional text embeddings"));
    }

    #[test]
    fn rejects_zero_search_weights() {
        let mut config = Config::default();
        config.search.vector_weight = 0.0;
        config.search.snippet_weight = 0.0;
        config.search.keyword_weight = 0.0;

        let err = config
            .normalized()
            .validate()
            .expect_err("zeroed hybrid weights should not pass validation");

        assert!(err.contains("hybrid search weight"));
    }
}
