//! Capture pipeline
//!
//! Samples the foreground screen, blocks private contexts before OCR, extracts
//! Apple Vision text, embeds cleaned chunks, and batches memory records into
//! LanceDB.

mod admission;
pub mod clipboard;
mod dedupe;
pub mod enrich_policy;
pub mod entity_extractor;
pub(crate) mod macos;
pub mod permissions;
mod sampling;
pub mod text_cleanup;

use admission::{classify_capture_surface_policy, CaptureSurfacePolicy};
pub use dedupe::{
    dhash_9x8, hamming, is_aba, luma_9x8_from_rgba, DedupeMatchKind, DedupeVerdict,
    PerceptualHasher,
};
pub use sampling::AdaptiveSampler;

/// Convenience wrapper: return just the frontmost app name on macOS.
/// Used by the proactive notification system outside the capture crate.
pub fn macos_frontmost_app_name() -> Option<String> {
    let ctx = macos::get_frontmost_app_info();
    if ctx.app_name == "Unknown" {
        None
    } else {
        Some(ctx.app_name)
    }
}

use crate::config::{
    CapturePipelineConfig, DEFAULT_CAPTURE_EMBEDDING_CACHE_SIZE, DEFAULT_IMAGE_EMBEDDING_DIM,
};
use crate::context_runtime;
use crate::embedding::{embed_imported_image, Embedder, EmbeddingBackend, EMBEDDING_DIM};
use crate::inference::extraction_evidence::{
    has_source_evidence, source_evidence_sets_from_raw, validate_source_evidence,
};
use crate::inference::vlm_router::{
    should_run_vlm, vlm_capability_label, vlm_runtime_status_label, VlmRouteDecision, VlmRouteInput,
};
use crate::inference::{
    compose_import_memory_context_with_title, extract_image_semantics, ImageImportSource,
    StructuredMemoryExtraction,
};
use crate::memory::reopen::build_reopen_target;
use crate::memory_compaction::{
    build_lexical_shadow, build_lexical_shadow_with_aliases, mean_pool_embeddings,
};
use crate::memory_embedding_document::{
    build_embedding_manifest, compose_memory_embedding_document, image_embedding_status,
    text_embedding_status, upsert_embedding_manifest, VisualSemanticSource,
};
use crate::memory_quality::{deterministic_dedup_fingerprint, is_supported_dedup_fingerprint};
use crate::models;
use crate::ocr::{OcrEngine, RecognizedText};
use crate::privacy::safety_gate::{self, SafetyDecision};
use crate::privacy::Blocklist;
use crate::storage::{MemoryRecord, SearchResult, Task};
use crate::summariser::narration_filter::clean_or_fallback_display_summary;
use crate::telemetry::quality_logger::append_quality_event;
use crate::telemetry::runtime_metrics;
use crate::AppState;
use chrono::{Local, Timelike};
use regex::Regex;
use serde_json::json;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Default)]
struct SemanticDedupWindow {
    seen_at_ms: HashMap<u64, i64>,
}

impl SemanticDedupWindow {
    fn should_skip(&mut self, signature: u64, now_ms: i64, window_ms: i64) -> bool {
        self.seen_at_ms
            .retain(|_, seen_at| now_ms.saturating_sub(*seen_at) <= window_ms);

        if let Some(last_seen) = self.seen_at_ms.get(&signature).copied() {
            if now_ms.saturating_sub(last_seen) <= window_ms {
                self.seen_at_ms.insert(signature, now_ms);
                return true;
            }
        }

        self.seen_at_ms.insert(signature, now_ms);
        false
    }
}

struct EmbeddingMemo {
    capacity: usize,
    order: VecDeque<String>,
    values: HashMap<String, Vec<f32>>,
}

impl EmbeddingMemo {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            order: VecDeque::with_capacity(capacity),
            values: HashMap::with_capacity(capacity),
        }
    }

    fn get(&self, key: &str) -> Option<Vec<f32>> {
        self.values.get(key).cloned()
    }

    fn insert(&mut self, key: String, value: Vec<f32>) {
        if self.values.contains_key(&key) {
            return;
        }
        if self.order.len() >= self.capacity.max(1) {
            if let Some(evicted) = self.order.pop_front() {
                self.values.remove(&evicted);
            }
        }
        self.order.push_back(key.clone());
        self.values.insert(key, value);
    }
}

/// Per-session adaptive admission for the visual-narrative path.
///
/// Tracks recent CLIP image vectors and the count of visual-only admits in
/// the current session. A candidate frame is admitted iff
/// `novelty(candidate) >= base + alpha * admitted` (capped at `ceiling`),
/// so the gate self-throttles as more frames from the same scene/session
/// land. Resets whenever the session key changes (typically when the
/// frontmost app, window title, or URL changes).
#[derive(Default)]
struct VisualNoveltyTracker {
    session_key: String,
    recent: VecDeque<Vec<f32>>,
    admitted: u32,
}

impl VisualNoveltyTracker {
    fn reset_for(&mut self, session_key: &str) {
        if self.session_key != session_key {
            self.session_key.clear();
            self.session_key.push_str(session_key);
            self.recent.clear();
            self.admitted = 0;
        }
    }

    /// `1.0 - max(cosine_similarity)` against the ring; `1.0` when the
    /// ring is empty so the first frame is always considered fully novel.
    fn novelty(&self, vec: &[f32]) -> f32 {
        if self.recent.is_empty() {
            return 1.0;
        }
        let mut max_sim = -1.0_f32;
        for r in &self.recent {
            let s = cosine_similarity(vec, r);
            if s > max_sim {
                max_sim = s;
            }
        }
        (1.0 - max_sim).clamp(0.0, 1.0)
    }

    fn adaptive_threshold(&self, base: f32, alpha: f32, ceiling: f32) -> f32 {
        (base + alpha * self.admitted as f32).clamp(base, ceiling)
    }

    fn admit(&mut self, vec: Vec<f32>, capacity: usize) {
        let cap = capacity.max(1);
        while self.recent.len() >= cap {
            self.recent.pop_front();
        }
        self.recent.push_back(vec);
        self.admitted = self.admitted.saturating_add(1);
    }
}

/// Outcome of the visual-admission gate. `Admitted` carries the freshly
/// computed CLIP vector so the downstream composer reuses it on the
/// `MemoryRecord` instead of running CLIP twice.
#[derive(Debug)]
enum VisualAdmissionOutcome {
    Admitted { image_vec: Vec<f32>, novelty: f32 },
    SkippedSmall { width: u32, height: u32 },
    SkippedNovelty { novelty: f32, threshold: f32 },
    Failed(String),
}

fn capture_pixel_vlm_route(
    config: &crate::config::Config,
    app_data_dir: &std::path::Path,
    ocr_text_len: usize,
    ocr_confidence: f32,
    ocr_block_count: usize,
    visual_signal: bool,
    is_duplicate: bool,
    system_pressure_skip: bool,
    host_supports_qwen_vlm: bool,
    calls_remaining: u32,
) -> VlmRouteDecision {
    let model_id = models::configured_vlm_model_id(config);
    let vlm_available = models::pixel_vlm_available(model_id.as_deref(), Some(app_data_dir));
    should_run_vlm(&VlmRouteInput {
        ocr_text_len,
        ocr_confidence,
        ocr_block_count,
        visual_signal,
        is_duplicate,
        system_pressure_skip,
        host_supports_qwen_vlm,
        vlm_enabled: config.use_vlm,
        vlm_available,
        vlm_calls_remaining: calls_remaining,
        vlm_timeout_secs: config.vlm_timeout_secs,
        _phantom: std::marker::PhantomData,
    })
}

/// First half of the visual-narrative path: decode the screen PNG once,
/// reject undersized frames, compute the CLIP embedding, and ask the
/// tracker whether this frame is novel enough to admit. CLIP is run in
/// `spawn_blocking` so the async loop stays responsive.
async fn try_admit_visual_capture(
    image_data: &[u8],
    tracker: &VisualNoveltyTracker,
    cfg: &CapturePipelineConfig,
    models_dir: &PathBuf,
) -> VisualAdmissionOutcome {
    let bytes = image_data.to_vec();
    let models_dir = models_dir.clone();
    let min_dim = cfg.visual_admission_min_image_dim;
    let decode_and_embed =
        tokio::task::spawn_blocking(move || -> Result<(Vec<f32>, u32, u32), String> {
            use image::GenericImageView;
            let dynamic =
                image::load_from_memory(&bytes).map_err(|e| format!("decode capture png: {e}"))?;
            let (w, h) = dynamic.dimensions();
            if w < min_dim || h < min_dim {
                return Ok((Vec::new(), w, h));
            }
            let vec = embed_imported_image(&dynamic, &models_dir)?;
            Ok((vec, w, h))
        })
        .await;

    let (image_vec, width, height) = match decode_and_embed {
        Ok(Ok(tuple)) => tuple,
        Ok(Err(err)) => return VisualAdmissionOutcome::Failed(err),
        Err(err) => return VisualAdmissionOutcome::Failed(format!("clip join: {err}")),
    };
    if image_vec.is_empty() {
        return VisualAdmissionOutcome::SkippedSmall { width, height };
    }

    let novelty = tracker.novelty(&image_vec);
    let threshold = tracker.adaptive_threshold(
        cfg.visual_novelty_base,
        cfg.visual_novelty_alpha,
        cfg.visual_novelty_ceiling,
    );
    if novelty >= threshold {
        VisualAdmissionOutcome::Admitted { image_vec, novelty }
    } else {
        VisualAdmissionOutcome::SkippedNovelty { novelty, threshold }
    }
}

fn visual_insight_from_structured(
    extraction: &StructuredMemoryExtraction,
) -> (
    crate::inference::ImageSemanticInsight,
    Option<crate::inference::extraction_evidence::ExtractionEvidence>,
) {
    (
        crate::inference::insight_from_structured(extraction),
        extraction.source_evidence.clone(),
    )
}

#[allow(clippy::too_many_arguments)]
async fn compose_visual_capture_record(
    state: &AppState,
    text_embedder: Option<&Embedder>,
    embedding_memo: &mut EmbeddingMemo,
    image_data: Vec<u8>,
    image_vec: Vec<f32>,
    app_name: &str,
    bundle_id: Option<&str>,
    window_title: &str,
    url: Option<&str>,
    observed_text: &str,
    observed_text_len: usize,
    observed_confidence: f32,
    observed_block_count: usize,
    novelty: f32,
) -> Result<MemoryRecord, String> {
    let now = Local::now();
    let synthetic_filename = format!(
        "{}_{}.png",
        sanitize_visual_filename_token(app_name),
        now.timestamp_millis()
    );

    // System-pressure and host-size throttle: under high pressure, or on
    // machines below the VLM RAM floor, skip MTMD and fall back to the
    // OCR/window grounded path.
    let config = state.config.read().clone();
    let host_supports_qwen_vlm = crate::telemetry::system_metrics::host_supports_lightweight_vlm();
    let (skip_vlm, skip_reason) =
        crate::telemetry::system_metrics::pressure_recommends_skipping_heavy_models();
    let vlm_route = capture_pixel_vlm_route(
        &config,
        state.app_data_dir.as_path(),
        observed_text_len,
        observed_confidence,
        observed_block_count,
        true,
        false,
        skip_vlm,
        host_supports_qwen_vlm,
        config.vlm_max_calls_per_minute,
    );

    // Helper: try the LLM-on-OCR fallback when Llama is loaded. Returns
    // None if no engine is available or the structured extraction returns
    // nothing useful. Stays inside the existing async context (already
    // serialized by the model pipeline lock the caller holds).
    let llm_engine = state.inference_engine();
    let trimmed_observed = observed_text.trim();
    let llm_fallback_context = if trimmed_observed.is_empty() {
        format!(
            "App: {app_name}\nWindow: {window_title}\n(visual-only frame; OCR was below the storage gate)"
        )
    } else {
        format!(
            "App: {app_name}\nWindow: {window_title}\nOCR excerpt:\n{}",
            trimmed_observed.chars().take(4000).collect::<String>()
        )
    };
    let try_llm_fallback = || async {
        let engine = llm_engine.clone()?;
        engine
            .extract_structured_memory(app_name, window_title, &llm_fallback_context)
            .await
            .map(|s| visual_insight_from_structured(&s))
    };

    // Removed ungrounded low-RAM visual capture gate: store captures even without OCR/VLM grounding

    let structure_started = Instant::now();
    let (insight, source_evidence) = if !vlm_route.runs_pixel_vlm() {
        let reason = vlm_route
            .fallback_reason()
            .unwrap_or_else(|| vlm_route.label());
        tracing::info!(
            app = %app_name,
            "compose_visual_capture_record: skipping VLM ({reason}); pressure_reason={skip_reason}; trying LLM-on-OCR fallback"
        );
        if let Some(i) = try_llm_fallback().await {
            i
        } else {
            (
                crate::inference::insight_from_ocr_only(
                    &synthetic_filename,
                    Some(app_name),
                    Some(window_title),
                    "",
                ),
                None,
            )
        }
    } else {
        // Run the same VLM path Meta-glasses imports use. Visual narrative
        // is its sole signal; OCR is skipped (the gate fired *because* OCR
        // was thin), so we don't pass an OCR appendix.
        match extract_image_semantics(
            image_data,
            &synthetic_filename,
            ImageImportSource::ScreenCapture,
            state.app_data_dir.clone(),
        )
        .await
        {
            Ok(i) => (i, None),
            Err(e) => {
                tracing::warn!(
                    app = %app_name,
                    "compose_visual_capture_record: VLM failed ({e}); trying LLM-on-OCR fallback"
                );
                if let Some(i) = try_llm_fallback().await {
                    i
                } else {
                    (
                        crate::inference::insight_from_ocr_only(
                            &synthetic_filename,
                            Some(app_name),
                            Some(window_title),
                            "",
                        ),
                        None,
                    )
                }
            }
        }
    };
    runtime_metrics::since_ms("mem.structure_ms", structure_started);

    let composed = compose_import_memory_context_with_title(
        &synthetic_filename,
        &insight,
        None,
        ImageImportSource::ScreenCapture,
        Some(window_title),
    );
    let synthesis_branch = match insight.model_id.as_str() {
        "llm_ocr_grounded" => "llm_ocr_grounded_visual_fallback",
        "ocr_only" | "" => "visual_metadata_fallback",
        _ if vlm_route.runs_pixel_vlm() => "vlm",
        _ => "visual_metadata_fallback",
    };

    let session_key = build_session_key(app_name, window_title, url);
    let session_id = build_session_id(&now, app_name, bundle_id, &session_key);
    let display_summary = if !insight.summary_short.trim().is_empty() {
        insight
            .summary_short
            .trim()
            .chars()
            .take(200)
            .collect::<String>()
    } else {
        composed
            .memory_context
            .chars()
            .take(160)
            .collect::<String>()
    };
    let topic = if !composed.topic.trim().is_empty() {
        composed.topic.clone()
    } else {
        "unknown".to_string()
    };
    let user_intent = if source_evidence.is_some() {
        String::new()
    } else if !composed.user_intent.trim().is_empty() {
        composed.user_intent.clone()
    } else {
        composed.activity_type.clone()
    };

    // Fold synthesis-derived concept terms (search_aliases + topic_categories)
    // into the lexical shadow so keyword search can hit them even when raw OCR
    // doesn't contain those terms. E.g. "sport" finds cricket captures.
    let mut shadow_extras: Vec<&str> = Vec::new();
    for v in composed
        .search_aliases
        .iter()
        .chain(composed.topic_categories.iter())
    {
        shadow_extras.push(v.as_str());
    }
    let lexical_shadow = build_lexical_shadow_with_aliases(
        app_name,
        &display_summary,
        &composed.memory_context,
        url,
        &shadow_extras,
    );
    let chunking_config = state.config.read().chunking.clone();
    let source_type = if url.is_some() {
        "browser_visual".to_string()
    } else {
        "screen_visual".to_string()
    };
    let mut embedding_seed = MemoryRecord {
        app_name: app_name.to_string(),
        window_title: window_title.to_string(),
        clean_text: composed.memory_context.clone(),
        snippet: display_summary.clone(),
        display_summary: display_summary.clone(),
        summary_source: "visual_capture".to_string(),
        lexical_shadow: lexical_shadow.clone(),
        url: url.map(str::to_string),
        source_type: source_type.clone(),
        topic: topic.clone(),
        workflow: "unknown".to_string(),
        user_intent: user_intent.clone(),
        memory_context: composed.memory_context.clone(),
        search_aliases: composed.search_aliases.clone(),
        activity_type: composed.activity_type.clone(),
        entities: insight.entities.clone(),
        tags: insight.topics.clone(),
        extraction_confidence: insight.confidence,
        synthesis_branch: synthesis_branch.to_string(),
        topic_categories: composed.topic_categories.clone(),
        insight_what_happened: composed.insight_what_happened.clone(),
        insight_why_mattered: composed.insight_why_mattered.clone(),
        insight_card_confidence: insight.confidence,
        ..Default::default()
    };
    crate::memory_insight::derive_insight_for_record(&mut embedding_seed);
    let compose_started = Instant::now();
    let embedding_document =
        compose_memory_embedding_document(&embedding_seed, Some(&chunking_config));
    runtime_metrics::since_ms("mem.compose_ms", compose_started);

    let embedding_inputs = embedding_document.text_embedding_inputs();
    let embed_started = Instant::now();
    let vectors = embed_text_inputs_with_memo(
        text_embedder,
        embedding_memo,
        app_name,
        window_title,
        &embedding_inputs,
    );
    runtime_metrics::since_ms("mem.embed_ms", embed_started);
    let primary = vectors
        .first()
        .cloned()
        .unwrap_or_else(|| vec![0.0; EMBEDDING_DIM]);
    let snippet_embedding = vectors
        .get(1)
        .cloned()
        .unwrap_or_else(|| vec![0.0; EMBEDDING_DIM]);
    let support_embedding = if vectors.len() > 2 {
        mean_pool_embeddings(&vectors[2..])
    } else {
        vec![0.0; EMBEDDING_DIM]
    };
    let text_embedding =
        weighted_primary_embedding(&primary, &snippet_embedding, &support_embedding);

    let visual_source = match synthesis_branch {
        "llm_ocr_grounded_visual_fallback" => VisualSemanticSource::LlmOcrGrounded,
        "visual_metadata_fallback" => VisualSemanticSource::ClipMetadataFallback,
        "vlm" => VisualSemanticSource::PixelVlmOrOcrGrounded,
        _ => VisualSemanticSource::Unknown,
    };
    let manifest = build_embedding_manifest(
        &embedding_document,
        text_embedding_status(&text_embedding),
        image_embedding_status(&image_vec),
        visual_source,
    );
    let raw_evidence = upsert_embedding_manifest(&json!({
        "source_kind": "visual_capture",
        "source_evidence": source_evidence,
        "vision_model_id": insight.model_id,
        "semantic_confidence": insight.confidence,
        "synthesis_branch": synthesis_branch,
        "text_embedding_dim": EMBEDDING_DIM,
        "image_embedding_dim": DEFAULT_IMAGE_EMBEDDING_DIM,
        "visual_understanding": {
            "status": if image_vec.iter().any(|value| *value != 0.0) {
                "clip_image_embedding"
            } else {
                "zero_vector_fallback"
            },
            "raw_pixels_persisted": false,
        },
        "visual_admission_novelty": novelty,
        "vlm_route": vlm_route.label(),
        "vlm_capability": vlm_capability_label(config.use_vlm, host_supports_qwen_vlm, models::pixel_vlm_available(models::configured_vlm_model_id(&config).as_deref(), Some(state.app_data_dir.as_path()))),
        "vlm_runtime_status": vlm_runtime_status_label(&vlm_route, Some(skip_reason)),
        "vlm_block_reason": vlm_route.fallback_reason(),
        "host_supports_vlm": host_supports_qwen_vlm,
        "pressure_reason": skip_reason,
        "app_name": app_name,
        "window_title": window_title,
        "url": url,
        "synthetic_filename": synthetic_filename,
        "timestamp_ms": now.timestamp_millis(),
    })
    .to_string(), &manifest);

    let mut record = MemoryRecord {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: now.timestamp_millis(),
        day_bucket: now.format("%Y-%m-%d").to_string(),
        app_name: app_name.to_string(),
        bundle_id: bundle_id.map(str::to_string),
        window_title: window_title.to_string(),
        session_id,
        text: String::new(),
        clean_text: composed.memory_context.clone(),
        ocr_confidence: observed_confidence,
        ocr_block_count: observed_block_count.min(u32::MAX as usize) as u32,
        snippet: display_summary.clone(),
        display_summary: display_summary.clone(),
        internal_context: composed.memory_context.clone(),
        summary_source: "visual_capture".to_string(),
        noise_score: 0.0,
        session_key,
        lexical_shadow,
        embedding: text_embedding,
        image_embedding: image_vec.clone(),
        screenshot_path: None,
        url: url.map(str::to_string),
        snippet_embedding,
        support_embedding,
        decay_score: 1.0,
        last_accessed_at: 0,
        timestamp_start: now.timestamp_millis(),
        timestamp_end: now.timestamp_millis(),
        source_type,
        topic,
        workflow: "unknown".to_string(),
        user_intent,
        memory_context: composed.memory_context.clone(),
        raw_evidence,
        search_aliases: composed.search_aliases.clone(),
        activity_type: composed.activity_type.clone(),
        entities: insight.entities.clone(),
        tags: insight.topics.clone(),
        embedding_text: embedding_document.primary_text.clone(),
        embedding_model: "all-MiniLM-L6-v2".to_string(),
        embedding_dim: EMBEDDING_DIM as u32,
        evidence_confidence: insight.confidence,
        extraction_confidence: insight.confidence,
        synthesis_branch: synthesis_branch.to_string(),
        topic_categories: composed.topic_categories.clone(),
        insight_what_happened: embedding_seed.insight_what_happened.clone(),
        insight_why_mattered: embedding_seed.insight_why_mattered.clone(),
        insight_what_changed: embedding_seed.insight_what_changed.clone(),
        insight_context_thread: embedding_seed.insight_context_thread.clone(),
        insight_spans_json: embedding_seed.insight_spans_json.clone(),
        insight_card_confidence: insight.confidence,
        schema_version: 2,
        enrichment_status: String::new(), // Lifecycle: pending review
        ..Default::default()
    };
    record.dedup_fingerprint =
        deterministic_dedup_fingerprint(&record, Some(&record.memory_context));
    Ok(record)
}

/// Make the synthetic VLM-input filename stable but human-readable. The
/// filename is only used for analytics/telemetry; the VLM itself does not
/// read pixels from disk, just from memory.
fn sanitize_visual_filename_token(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "screen".to_string()
    } else {
        trimmed.chars().take(48).collect()
    }
}

pub(crate) fn capture_context_skip_reason(
    app_name: &str,
    bundle_id: Option<&str>,
    window_title: &str,
    url: Option<&str>,
    blocklist: &[String],
) -> Option<crate::SkipReason> {
    capture_admission_skip_reason(app_name, bundle_id, window_title, url, None, blocklist)
}

fn capture_admission_skip_reason(
    app_name: &str,
    bundle_id: Option<&str>,
    window_title: &str,
    url: Option<&str>,
    ocr_text: Option<&str>,
    blocklist: &[String],
) -> Option<crate::SkipReason> {
    if Blocklist::is_internal_app(app_name, bundle_id) {
        return Some(crate::SkipReason::SelfApp);
    }
    if Blocklist::is_blocked(app_name, blocklist) {
        return Some(crate::SkipReason::Blocklist);
    }
    if Blocklist::is_context_blocked(url, Some(window_title), blocklist) {
        return Some(crate::SkipReason::Blocklist);
    }
    if safety_gate::evaluate(
        Some(app_name),
        bundle_id,
        url,
        Some(window_title),
        ocr_text,
        blocklist,
    ) != SafetyDecision::Allow
    {
        return Some(crate::SkipReason::SensitiveContext);
    }
    None
}

pub(crate) fn should_skip_capture_context(
    app_name: &str,
    bundle_id: Option<&str>,
    window_title: &str,
    url: Option<&str>,
    blocklist: &[String],
) -> bool {
    capture_context_skip_reason(app_name, bundle_id, window_title, url, blocklist).is_some()
}

/// Queue one alert for a sensitive browser context before the capture flow
/// branches into URL-only, visual, semantic, or OCR storage paths. Alert
/// dismissal/snoozing controls only the prompt; the deterministic safety gate
/// remains fail-closed independently of this queue.
fn queue_sensitive_context_alert(
    state: &AppState,
    url: Option<&str>,
    window_title: &str,
    dismissed_alerts: &[String],
) {
    if !Blocklist::is_sensitive_context(url, Some(window_title)) {
        return;
    }

    let alert_key =
        Blocklist::context_key(url, Some(window_title)).unwrap_or_else(|| window_title.to_string());
    if Blocklist::is_context_blocked(url, Some(window_title), dismissed_alerts) {
        return;
    }

    let now = Local::now();
    let is_snoozed = state
        .snoozed_privacy_alerts
        .read()
        .get(&alert_key)
        .is_some_and(|expire_time| now.timestamp() < *expire_time);
    if is_snoozed {
        return;
    }

    let pushed = {
        let mut pending = state.pending_privacy_alerts.write();
        if pending
            .iter()
            .any(|alert| alert.domain_or_title == alert_key)
        {
            false
        } else {
            // Bound the queue so an unattended alert spike cannot grow it
            // without limit.
            if pending.len() >= 50 {
                pending.remove(0);
            }
            pending.push(crate::PrivacyAlert {
                id: uuid::Uuid::new_v4().to_string(),
                domain_or_title: alert_key,
                detected_at: now.timestamp_millis(),
            });
            true
        }
    };
    if pushed {
        tracing::info!("Surfaced proactive privacy alert for sensitive context");
        crate::ipc::commands::emit_privacy_alerts(state);
    }
}

fn extract_ocr_text(app_name: &str, ocr_result: &RecognizedText) -> text_cleanup::HighSignalText {
    text_cleanup::build_high_signal_text_for_app(app_name, &ocr_result.text)
}

pub(crate) fn normalize_evidence_text(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn compute_window_title_hash(url: Option<&str>, window_title: &str, timestamp_ms: i64) -> String {
    let mut hasher = DefaultHasher::new();
    url.unwrap_or_default().hash(&mut hasher);
    window_title.hash(&mut hasher);
    timestamp_ms.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn entity_regex() -> &'static Regex {
    static ENTITY_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    ENTITY_RE.get_or_init(|| {
        Regex::new(r"\b(?:[A-Z][a-z0-9]+(?:\s+[A-Z][a-z0-9]+){0,2}|[A-Z]{2,}(?:\s+[A-Z]{2,})?)\b")
            .expect("valid entity regex")
    })
}

fn lightweight_entities_from_text(text: &str) -> Vec<String> {
    const STOP_ENTITIES: &[&str] = &[
        "The",
        "This",
        "That",
        "And",
        "For",
        "With",
        "From",
        "You",
        "Your",
        "Google Chrome",
        "Safari",
        "YouTube",
        "Page",
        "Menu",
        "Home",
        "Search",
        "Results",
    ];
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for cap in entity_regex().find_iter(text) {
        let value = cap.as_str().trim();
        if value.len() < 3 || value.len() > 48 {
            continue;
        }
        if STOP_ENTITIES
            .iter()
            .any(|item| item.eq_ignore_ascii_case(value))
        {
            continue;
        }
        let key = value.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(value.to_string());
        }
        if out.len() >= 12 {
            break;
        }
    }
    out
}

fn build_structured_from_browser_semantics(
    _app_name: &str,
    window_title: &str,
    url: Option<&str>,
    semantic: &macos::BrowserSemanticContent,
) -> Option<StructuredMemoryExtraction> {
    if !semantic.has_signal() {
        return None;
    }
    let content_text = semantic.content_text();
    if content_text.trim().is_empty() {
        return None;
    }
    let mut entities = lightweight_entities_from_text(&content_text);
    if let Some(domain) = url.and_then(extract_domain) {
        if !entities
            .iter()
            .any(|value| value.eq_ignore_ascii_case(&domain))
        {
            entities.push(domain);
        }
    }
    let topic = if !semantic.h1.trim().is_empty() {
        semantic.h1.trim().to_string()
    } else if !window_title.trim().is_empty() {
        window_title.trim().to_string()
    } else {
        semantic.title.trim().to_string()
    };
    let mut memory_context = String::new();
    if !topic.is_empty() {
        memory_context.push_str(&topic);
    }
    if !semantic.meta_description.trim().is_empty() {
        if !memory_context.is_empty() {
            memory_context.push_str(". ");
        }
        memory_context.push_str(semantic.meta_description.trim());
    }
    if memory_context.trim().is_empty() {
        memory_context = crate::summariser::sentences::first_sentence(&content_text).to_string();
    }
    Some(StructuredMemoryExtraction {
        activity_type: "research".to_string(),
        project: String::new(),
        topic,
        memory_context,
        workflow: "researching".to_string(),
        user_intent: "researching".to_string(),
        entities,
        search_aliases: Vec::new(),
        confidence: semantic.content_signal_score.clamp(0.35, 0.90),
        dedup_fingerprint: String::new(),
        synthesis_branch: "browser_semantic".to_string(),
        ..Default::default()
    })
}

#[derive(Debug, Clone)]
struct SemanticFusionDraft {
    extraction: StructuredMemoryExtraction,
    sources: Vec<&'static str>,
    reason: &'static str,
}

fn clean_file_reference(token: &str) -> String {
    token
        .trim_matches(|ch: char| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    ',' | ';' | '(' | ')' | '[' | ']' | '{' | '}' | '"' | '\'' | '`' | '•'
                )
        })
        .trim_end_matches([':', '.', ')', ']'])
        .to_string()
}

fn looks_like_file_reference(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    const EXTENSIONS: &[&str] = &[
        ".md", ".rs", ".ts", ".tsx", ".js", ".jsx", ".json", ".toml", ".yaml", ".yml", ".css",
        ".html", ".py", ".swift", ".sh",
    ];
    EXTENSIONS
        .iter()
        .any(|ext| lower.ends_with(ext) || lower.contains(&format!("{ext}:")))
}

fn extract_file_references(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for raw in text.split_whitespace() {
        let mut token = clean_file_reference(raw);
        if let Some((head, _line)) = token.rsplit_once(':') {
            if looks_like_file_reference(head) {
                token = clean_file_reference(head);
            }
        }
        if !looks_like_file_reference(&token) {
            continue;
        }
        let key = token.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(token);
            if out.len() >= 12 {
                break;
            }
        }
    }
    out
}

fn merge_unique_strings(existing: &mut Vec<String>, incoming: impl IntoIterator<Item = String>) {
    let mut seen: HashSet<String> = existing
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect();
    for value in incoming {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if seen.insert(trimmed.to_ascii_lowercase()) {
            existing.push(trimmed.to_string());
        }
    }
}

fn infer_review_activity(clean_text: &str) -> (&'static str, &'static str, &'static str) {
    let lower = clean_text.to_ascii_lowercase();
    if lower.contains("error") || lower.contains("failed") || lower.contains("debug") {
        ("debugging", "debugging", "debugging visible issue context")
    } else if lower.contains("todo")
        || lower.contains("planned")
        || lower.contains("roadmap")
        || lower.contains("implemented")
        || lower.contains("docs")
        || lower.contains("design")
    {
        (
            "reviewing",
            "reviewing",
            "reviewing implementation status and supporting context",
        )
    } else {
        ("reviewing", "reviewing", "reviewing visible screen context")
    }
}

fn build_low_ram_semantic_fusion(
    app_name: &str,
    window_title: &str,
    url: Option<&str>,
    clean_text: &str,
    semantic_page: Option<&macos::BrowserSemanticContent>,
    capture_quality: &text_cleanup::CaptureQualityStats,
    source_kind: &str,
) -> Option<SemanticFusionDraft> {
    let text = clean_text.trim();
    if text.len() < 80 && semantic_page.map(|page| !page.has_signal()).unwrap_or(true) {
        return None;
    }

    let spans = text_cleanup::rank_salient_spans(text, app_name);
    let salient = spans
        .iter()
        .filter(|span| span.score >= 0.30)
        .take(3)
        .map(|span| span.text.clone())
        .collect::<Vec<_>>();
    let files = extract_file_references(text);

    let semantic_title = semantic_page
        .and_then(|page| {
            [
                page.h1.as_str(),
                page.title.as_str(),
                page.meta_description.as_str(),
            ]
            .into_iter()
            .map(str::trim)
            .find(|value| !value.is_empty())
            .map(str::to_string)
        })
        .unwrap_or_default();

    let topic = if !semantic_title.trim().is_empty() {
        semantic_title.chars().take(120).collect::<String>()
    } else if !files.is_empty() {
        files
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(" and ")
    } else if let Some(first) = salient.first() {
        first.chars().take(120).collect::<String>()
    } else if !window_title.trim().is_empty() {
        window_title.trim().chars().take(120).collect::<String>()
    } else {
        app_name.trim().chars().take(120).collect::<String>()
    };

    let (activity, workflow, user_intent) = infer_review_activity(text);
    let subject = if !files.is_empty() {
        format!(
            "visible files {}",
            files.iter().take(4).cloned().collect::<Vec<_>>().join(", ")
        )
    } else if !topic.trim().is_empty() {
        topic.clone()
    } else {
        window_title.trim().to_string()
    };

    let mut sentences = Vec::new();
    let surface = if !window_title.trim().is_empty() {
        format!("{} in {}", app_name.trim(), window_title.trim())
    } else {
        app_name.trim().to_string()
    };
    sentences.push(format!("You were reviewing {subject} on {surface}."));
    if let Some(domain) = url.and_then(extract_domain) {
        sentences.push(format!("The visible page was from {domain}."));
    }
    if !semantic_title.trim().is_empty() && !sentences.join(" ").contains(&semantic_title) {
        sentences.push(format!("Browser context: {semantic_title}."));
    }
    let lower_text = text.to_ascii_lowercase();
    if lower_text.contains("planned")
        || lower_text.contains("implemented")
        || lower_text.contains("roadmap")
        || lower_text.contains("design")
        || lower_text.contains("docs")
    {
        sentences.push(
            "The visible context was about implementation status, docs, or roadmap items."
                .to_string(),
        );
    }

    let keep_ratio = if capture_quality.total_lines == 0 {
        0.0
    } else {
        capture_quality.kept_lines as f32 / capture_quality.total_lines as f32
    };
    let avg_span_score = if spans.is_empty() {
        0.0
    } else {
        spans.iter().take(3).map(|span| span.score).sum::<f32>() / spans.len().min(3) as f32
    };
    let semantic_score = semantic_page
        .map(|page| page.content_signal_score)
        .unwrap_or(0.0);
    let confidence =
        (0.58 + avg_span_score * 0.16 + semantic_score * 0.12 + keep_ratio.clamp(0.0, 1.0) * 0.08)
            .clamp(0.60, 0.86);

    let mut entities = Vec::new();
    merge_unique_strings(&mut entities, files.iter().cloned());
    if !semantic_title.trim().is_empty() {
        merge_unique_strings(&mut entities, [semantic_title.clone()]);
    }
    if let Some(domain) = url.and_then(extract_domain) {
        merge_unique_strings(&mut entities, [domain]);
    }

    let mut aliases = Vec::new();
    merge_unique_strings(&mut aliases, files.iter().cloned());
    merge_unique_strings(&mut aliases, [topic.clone()]);

    let mut sources = vec!["ocr_salient_spans", "app_window"];
    if !files.is_empty() {
        sources.push("file_references");
    }
    if semantic_page.is_some() {
        sources.push("browser_semantic");
    }
    if source_kind == "browser_semantic" {
        sources.push("browser_text_source");
    }

    Some(SemanticFusionDraft {
        extraction: StructuredMemoryExtraction {
            activity_type: activity.to_string(),
            project: String::new(),
            topic,
            memory_context: sentences.join(" "),
            workflow: workflow.to_string(),
            user_intent: user_intent.to_string(),
            files_touched: files,
            entities,
            search_aliases: aliases,
            confidence,
            dedup_fingerprint: String::new(),
            synthesis_branch: "fallback".to_string(),
            ..Default::default()
        },
        sources,
        reason: "low_ram_deterministic_semantic_fusion",
    })
}

fn semantic_layout_diagnostics(
    clean_text: &str,
    capture_quality: &text_cleanup::CaptureQualityStats,
) -> serde_json::Value {
    let visible_lines = clean_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let checkbox_like_lines = visible_lines
        .iter()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.starts_with("☐")
                || lower.starts_with("[ ]")
                || lower.starts_with("- [ ]")
                || lower.contains("checkbox")
        })
        .count();
    let heading_like_lines = visible_lines
        .iter()
        .filter(|line| {
            let trimmed = line.trim_start_matches(['#', '*', '-']).trim();
            !trimmed.is_empty()
                && trimmed.len() <= 90
                && trimmed
                    .chars()
                    .filter(|ch| ch.is_alphabetic())
                    .take(1)
                    .next()
                    .is_some()
                && trimmed
                    .split_whitespace()
                    .filter(|word| word.chars().next().map(char::is_uppercase).unwrap_or(false))
                    .count()
                    >= 2
        })
        .count();
    let file_reference_count = extract_file_references(clean_text).len();
    let split_pane_likelihood = ((file_reference_count.min(8) as f32 * 0.10)
        + (checkbox_like_lines.min(8) as f32 * 0.05)
        + (heading_like_lines.min(8) as f32 * 0.04)
        + if visible_lines.len() >= 18 { 0.18 } else { 0.0 })
    .clamp(0.0, 1.0);

    json!({
        "line_count": visible_lines.len(),
        "file_reference_count": file_reference_count,
        "heading_like_lines": heading_like_lines,
        "checkbox_like_lines": checkbox_like_lines,
        "split_pane_likelihood": split_pane_likelihood,
        "line_confidence": {
            "low_conf_lines": capture_quality.low_conf_lines,
            "kept_lines": capture_quality.kept_lines,
            "avg_line_score": capture_quality.avg_line_score,
        }
    })
}

fn apply_semantic_fusion(
    structured_memory: &mut Option<StructuredMemoryExtraction>,
    fusion: SemanticFusionDraft,
    replace_core_fields: bool,
) -> serde_json::Value {
    if let Some(existing) = structured_memory.as_mut() {
        if replace_core_fields || existing.memory_context.trim().is_empty() {
            existing.memory_context = fusion.extraction.memory_context.clone();
        }
        if replace_core_fields || existing.topic.trim().is_empty() || existing.topic == "unknown" {
            existing.topic = fusion.extraction.topic.clone();
        }
        if replace_core_fields || existing.workflow.trim().is_empty() {
            existing.workflow = fusion.extraction.workflow.clone();
        }
        if replace_core_fields || existing.user_intent.trim().is_empty() {
            existing.user_intent = fusion.extraction.user_intent.clone();
        }
        if replace_core_fields || existing.activity_type.trim().is_empty() {
            existing.activity_type = fusion.extraction.activity_type.clone();
        }
        merge_unique_strings(
            &mut existing.files_touched,
            fusion.extraction.files_touched.clone(),
        );
        merge_unique_strings(&mut existing.entities, fusion.extraction.entities.clone());
        merge_unique_strings(
            &mut existing.search_aliases,
            fusion.extraction.search_aliases.clone(),
        );
        existing.confidence = existing.confidence.max(fusion.extraction.confidence);
    } else {
        *structured_memory = Some(fusion.extraction.clone());
    }

    json!({
        "applied": true,
        "reason": fusion.reason,
        "sources": fusion.sources,
        "topic": structured_memory.as_ref().map(|m| m.topic.clone()).unwrap_or_default(),
        "confidence": structured_memory.as_ref().map(|m| m.confidence).unwrap_or(0.0),
    })
}

pub(crate) fn field_supported_by_evidence(field: &str, evidence_norm: &str) -> bool {
    let normalized_field = normalize_evidence_text(field);
    let terms = normalized_field
        .split_whitespace()
        .filter(|term| term.len() >= 3)
        .filter(|term| !matches!(*term, "unknown" | "none" | "null"))
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return true;
    }
    let matched = terms
        .iter()
        .filter(|term| evidence_norm.contains(**term))
        .count();
    let ratio = matched as f32 / terms.len() as f32;
    matched >= 1 && ratio >= 0.34
}

fn strip_unsupported_values(
    values: &mut Vec<String>,
    evidence_norm: &str,
    issues: &mut Vec<String>,
    issue_label: &str,
) {
    let mut kept = Vec::new();
    let mut removed = 0usize;
    for value in values.iter() {
        if field_supported_by_evidence(value, evidence_norm) {
            kept.push(value.clone());
        } else {
            removed += 1;
        }
    }
    if removed > 0 {
        issues.push(format!("{issue_label}:{removed}"));
    }
    *values = kept;
}

/// Reset a structured-extraction field when the model echoed an entire
/// enum vocabulary instead of choosing one value (e.g.
/// `coding|debugging|reviewing_agent_output|...`). `|` never belongs in a
/// single-label human-readable field, so we drop it and flag the issue.
/// Structural rule — independent of model name or vocabulary.
fn clear_if_multi_option(value: &mut String, issues: &mut Vec<String>, field: &str) {
    if value.contains('|') {
        issues.push(format!("{field}_multi_option_dump"));
        value.clear();
    }
}

fn validate_structured_memory_extraction(
    extraction: &mut StructuredMemoryExtraction,
    app_name: &str,
    window_title: &str,
    clean_text: &str,
    source_text: &str,
) -> (f32, Vec<String>) {
    let evidence_norm = normalize_evidence_text(&format!("{app_name} {window_title} {clean_text}"));
    let mut issues = Vec::new();
    let mut supported = 0usize;
    let mut total = 0usize;

    let original_activity_type = extraction.activity_type.clone();
    extraction.activity_type = crate::inference::normalize_activity_type(&original_activity_type);
    if original_activity_type.contains('|') {
        issues.push("activity_type_multi_option_dump".to_string());
    } else if !original_activity_type.trim().is_empty()
        && extraction.activity_type == "unknown"
        && !original_activity_type
            .trim()
            .eq_ignore_ascii_case("unknown")
    {
        issues.push("activity_type_invalid".to_string());
    }
    clear_if_multi_option(&mut extraction.topic, &mut issues, "topic");
    clear_if_multi_option(&mut extraction.workflow, &mut issues, "workflow");
    clear_if_multi_option(&mut extraction.user_intent, &mut issues, "user_intent");

    let mut maybe_scrub = |value: &mut String, label: &str| {
        if value.trim().is_empty() {
            return;
        }
        total += 1;
        if field_supported_by_evidence(value, &evidence_norm) {
            supported += 1;
        } else {
            issues.push(format!("unsupported_{label}"));
            if extraction.confidence < 0.8 {
                value.clear();
            }
        }
    };

    maybe_scrub(&mut extraction.project, "project");
    maybe_scrub(&mut extraction.topic, "topic");
    maybe_scrub(&mut extraction.workflow, "workflow");
    maybe_scrub(&mut extraction.user_intent, "intent");
    maybe_scrub(&mut extraction.memory_context, "memory_context");
    maybe_scrub(&mut extraction.outcome, "outcome");

    total += extraction.entities.len()
        + extraction.files_touched.len()
        + extraction.search_aliases.len();
    strip_unsupported_values(
        &mut extraction.entities,
        &evidence_norm,
        &mut issues,
        "unsupported_entities",
    );
    strip_unsupported_values(
        &mut extraction.files_touched,
        &evidence_norm,
        &mut issues,
        "unsupported_files",
    );
    strip_unsupported_values(
        &mut extraction.search_aliases,
        &evidence_norm,
        &mut issues,
        "unsupported_aliases",
    );

    supported += extraction.entities.len()
        + extraction.files_touched.len()
        + extraction.search_aliases.len();
    // A request written in prose is not a command, however it was filed.
    let commands_before = extraction.commands.len();
    extraction
        .commands
        .retain(|command| crate::inference::extraction_evidence::is_command_like(command));
    if extraction.commands.len() < commands_before {
        issues.push("commands_not_command_like".to_string());
    }
    if !is_supported_dedup_fingerprint(&extraction.dedup_fingerprint) {
        if !extraction.dedup_fingerprint.trim().is_empty() {
            issues.push("unsupported_dedup_fingerprint".to_string());
        }
        extraction.dedup_fingerprint.clear();
    }

    let support_ratio = if total == 0 {
        0.0
    } else {
        supported as f32 / total as f32
    };
    let grounding_confidence =
        (support_ratio * 0.72 + extraction.confidence.clamp(0.0, 1.0) * 0.28).clamp(0.0, 1.0);

    if grounding_confidence < 0.55 {
        issues.push("structured_fields_weakly_grounded".to_string());
    }
    if grounding_confidence < 0.8 {
        issues.push("possible_ungrounded_extraction".to_string());
    }

    // Fusion/browser seeds may refill model-rejected intent/action fields. Check
    // against the exact model input snapshot, not the differently cleaned OCR.
    issues.extend(validate_source_evidence(extraction, source_text));
    extraction.confidence = extraction.confidence.clamp(0.0, 1.0);
    (grounding_confidence, issues)
}

/// True when the LLM narrative already mentions the bulk of the topic's
/// content tokens, so re-emitting a "Topic:" preamble would be redundant
/// noise. Token overlap on lowercased ascii-alnum words, ≥60% threshold.
/// Stopwords + tokens shorter than 3 chars are ignored so single-letter
/// or articles don't dominate the ratio.
fn narrative_mentions(narrative: Option<&str>, topic: &str) -> bool {
    let Some(narrative) = narrative else {
        return false;
    };
    let topic_tokens: Vec<String> = topic
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .filter(|t| t.len() >= 3)
        .map(|t| t.to_string())
        .collect();
    if topic_tokens.is_empty() {
        return false;
    }
    let narrative_lower = narrative.to_ascii_lowercase();
    let hits = topic_tokens
        .iter()
        .filter(|tok| narrative_lower.contains(tok.as_str()))
        .count();
    (hits as f32 / topic_tokens.len() as f32) >= 0.6
}

/// Pick a deterministic semantic anchor for the durable memory context.
/// Priority: structured topic → first salient span head → window-title noun
/// phrase. No app names — fully content-derived.
fn pick_semantic_center(
    extraction: Option<&StructuredMemoryExtraction>,
    app_name: &str,
    window_title: &str,
    clean_text: &str,
) -> String {
    if let Some(mem) = extraction {
        let topic = mem.topic.trim();
        if !topic.is_empty() && !topic.eq_ignore_ascii_case("unknown") {
            return topic.to_string();
        }
    }
    let spans = text_cleanup::rank_salient_spans(clean_text, app_name);
    if let Some(top) = spans.first() {
        let trimmed = crate::summariser::sentences::first_sentence(
            top.text.lines().next().unwrap_or(&top.text),
        );
        if !trimmed.is_empty() {
            return trimmed.chars().take(120).collect::<String>();
        }
    }
    let title = window_title.trim();
    if !title.is_empty() {
        return title.chars().take(120).collect();
    }
    String::new()
}

/// Compose a human-readable continuity footer.
fn build_continuation_footer(prior_chain: &[crate::storage::SearchResult]) -> String {
    let mut lines: Vec<String> = Vec::new();
    if let Some(prev) = prior_chain.first() {
        let head: String = prev
            .memory_context
            .trim()
            .split('\n')
            .next()
            .unwrap_or("")
            .chars()
            .take(80)
            .collect();
        if !head.trim().is_empty() {
            lines.push(format!(
                "This continues earlier related work: {}.",
                head.trim()
            ));
        } else {
            lines.push("This continues earlier related work from the same session.".to_string());
        }
    }
    lines.join("\n")
}

/// Pad short contexts with grounded structured fields. Pure helper; no I/O and
/// no raw OCR tail copying into durable `memory_context`.
fn pad_with_structured(
    base: &str,
    extraction: Option<&StructuredMemoryExtraction>,
    app_name: &str,
    clean_text: &str,
    min_chars: usize,
) -> String {
    if base.chars().count() >= min_chars {
        return base.to_string();
    }
    let mut out = base.to_string();
    let mut extras: Vec<String> = Vec::new();
    let base_norm = normalize_text_for_overlap(base);
    if let Some(mem) = extraction {
        let topic_norm = normalize_text_for_overlap(mem.topic.trim());
        if !mem.topic.trim().is_empty()
            && !mem.topic.trim().eq_ignore_ascii_case("unknown")
            && (topic_norm.is_empty() || !base_norm.contains(&topic_norm))
        {
            extras.push(format!("Topic: {}", mem.topic.trim()));
        }
        if !mem.user_intent.trim().is_empty() {
            extras.push(format!("Intent: {}", mem.user_intent.trim()));
        }
        if !mem.workflow.trim().is_empty() && !mem.workflow.trim().eq_ignore_ascii_case("unknown") {
            extras.push(format!("Workflow: {}", mem.workflow.trim()));
        }
        if !mem.files_touched.is_empty() {
            extras.push(format!(
                "Files: {}",
                mem.files_touched
                    .iter()
                    .take(4)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !mem.entities.is_empty() {
            extras.push(format!(
                "Entities: {}",
                mem.entities
                    .iter()
                    .take(4)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !mem.decisions.is_empty() {
            extras.push(format!(
                "Decisions: {}",
                mem.decisions
                    .iter()
                    .take(2)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if !mem.results.is_empty() {
            extras.push(format!(
                "Results: {}",
                mem.results
                    .iter()
                    .take(2)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if !mem.next_steps.is_empty() {
            extras.push(format!(
                "Next: {}",
                mem.next_steps
                    .iter()
                    .take(2)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
    }
    for extra in extras {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&extra);
        if out.chars().count() >= min_chars {
            return out;
        }
    }
    if out.chars().count() < min_chars {
        let surface = if app_name.trim().is_empty() {
            String::new()
        } else {
            format!("Source app: {}", app_name.trim())
        };
        if !surface.trim().is_empty() && !out.contains(&surface) {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&surface);
        }
    }
    let _ = clean_text;
    out
}

/// Capture-time durable `memory_context`. Composes three sections (what /
/// state / where) bounded by config min/max chars, embeds an optional
/// continuation pointer to the prior card, and falls back gracefully when
/// structured extraction is absent.
pub(crate) fn build_durable_memory_context(
    extraction: Option<&StructuredMemoryExtraction>,
    app_name: &str,
    window_title: &str,
    clean_text: &str,
    display_summary: &str,
    _bundle_id: Option<&str>,
    _url: Option<&str>,
    prior_chain: &[crate::storage::SearchResult],
    config: &crate::config::MemoryQualityConfig,
) -> String {
    let center = pick_semantic_center(extraction, app_name, window_title, clean_text);

    // Narrative-first: the LLM's free-form `memory_context` is the most
    // human-readable description we have and is what humans/agents want to
    // see in retrieval surfaces (iOS handoff, OpenClaw, etc.). We lead with
    // it and only fall back to structured "Topic:/You were/Activity:" lines
    // when no narrative is present. Topic: is appended only if it adds
    // tokens the narrative does not already cover.
    let narrative: Option<String> = extraction.and_then(|m| {
        let trimmed = m.memory_context.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    });
    let mut what_lines: Vec<String> = Vec::new();
    if let Some(ref n) = narrative {
        what_lines.push(n.clone());
    }
    if !center.is_empty() && !narrative_mentions(narrative.as_deref(), &center) {
        what_lines.push(format!("Topic: {}", center));
    }
    if narrative.is_none() {
        if let Some(mem) = extraction {
            let intent = mem.user_intent.trim();
            if !intent.is_empty() {
                what_lines.push(format!("You were {}.", intent));
            } else if !mem.activity_type.trim().is_empty()
                && !mem.activity_type.trim().eq_ignore_ascii_case("unknown")
            {
                what_lines.push(format!("Activity: {}.", mem.activity_type.trim()));
            }
        }
    }
    if what_lines.is_empty() && !display_summary.trim().is_empty() {
        what_lines.push(display_summary.trim().to_string());
    }
    let what_section = what_lines.join("\n");

    let why_section = if let Some(mem) = extraction {
        let mut bits: Vec<String> = Vec::new();
        if !mem.decisions.is_empty() {
            bits.push(format!(
                "Decisions: {}",
                mem.decisions
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if !mem.errors.is_empty() {
            bits.push(format!(
                "Errors: {}",
                mem.errors
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if !mem.blockers.is_empty() {
            bits.push(format!(
                "Blockers: {}",
                mem.blockers
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if !mem.next_steps.is_empty() {
            bits.push(format!(
                "Next: {}",
                mem.next_steps
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if !mem.results.is_empty() {
            bits.push(format!(
                "Results: {}",
                mem.results
                    .iter()
                    .take(2)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        bits.join("\n")
    } else {
        String::new()
    };

    let where_section = build_continuation_footer(prior_chain);

    let mut sections: Vec<String> = Vec::new();
    if !what_section.trim().is_empty() {
        sections.push(what_section);
    }
    if !why_section.trim().is_empty() {
        sections.push(why_section);
    }
    if !where_section.trim().is_empty() {
        sections.push(where_section);
    }

    let min_chars = config.memory_context_min_chars as usize;
    let max_chars = config.memory_context_max_chars as usize;
    let mut combined = sections.join("\n\n");
    if combined.chars().count() < min_chars {
        combined = pad_with_structured(&combined, extraction, app_name, clean_text, min_chars);
    }
    if combined.chars().count() > max_chars {
        let mut truncated: String = combined.chars().take(max_chars.saturating_sub(3)).collect();
        truncated.push_str("...");
        combined = truncated;
    }
    combined
}

fn build_grounded_memory_context(
    extraction: Option<&StructuredMemoryExtraction>,
    app_name: &str,
    window_title: &str,
    clean_text: &str,
    display_summary: &str,
) -> String {
    if let Some(mem) = extraction {
        if !mem.memory_context.trim().is_empty() {
            return mem.memory_context.trim().to_string();
        }
        let mut parts = Vec::new();
        if !mem.user_intent.trim().is_empty() {
            parts.push(format!("You were {}.", mem.user_intent.trim()));
        }
        if !mem.project.trim().is_empty() {
            parts.push(format!("This was work on {}.", mem.project.trim()));
        } else if !mem.topic.trim().is_empty() {
            parts.push(format!("Topic: {}.", mem.topic.trim()));
        }
        if !mem.files_touched.is_empty() {
            parts.push(format!(
                "Files involved: {}.",
                mem.files_touched
                    .iter()
                    .take(4)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !mem.next_steps.is_empty() {
            parts.push(format!(
                "Next steps: {}.",
                mem.next_steps
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
        }
        if !parts.is_empty() {
            return parts.join(" ");
        }
    }

    let fallback = text_cleanup::concise_fallback_snippet(app_name, window_title, clean_text);
    if !fallback.trim().is_empty() {
        fallback
    } else if !display_summary.trim().is_empty() {
        display_summary.trim().to_string()
    } else {
        clean_text.chars().take(240).collect::<String>()
    }
}

fn weighted_primary_embedding(primary: &[f32], snippet: &[f32], support: &[f32]) -> Vec<f32> {
    let dim = primary
        .len()
        .max(snippet.len())
        .max(support.len())
        .max(EMBEDDING_DIM);
    let mut out = vec![0.0f32; dim];
    for i in 0..dim {
        let p = primary.get(i).copied().unwrap_or(0.0);
        let s = snippet.get(i).copied().unwrap_or(0.0);
        let u = support.get(i).copied().unwrap_or(0.0);
        out[i] = p * 0.62 + s * 0.23 + u * 0.15;
    }
    normalize_embedding_vector(&mut out);
    out
}

fn normalize_embedding_vector(vector: &mut [f32]) {
    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm <= 1e-6 {
        return;
    }
    for value in vector {
        *value /= norm;
    }
}

fn emit_capture_quality_signal(state: &AppState, payload: serde_json::Value) {
    let _ = append_quality_event(
        state.app_data_dir.as_path(),
        "signals.jsonl",
        &json!({
            "kind": "Capture",
            "payload": payload
        }),
    );
}

fn emit_sensitive_context_skip(state: &AppState, inspection_stage: &'static str) {
    emit_capture_quality_signal(
        state,
        json!({
            "timestamp_ms": chrono::Utc::now().timestamp_millis(),
            "stored_or_skipped": "skipped_sensitive_context",
            "privacy_gate": "deterministic_safety_gate",
            "inspection_stage": inspection_stage,
            "pixels_persisted": false,
            "text_persisted": false,
        }),
    );
}

fn emit_extraction_quality_anomaly(state: &AppState, payload: serde_json::Value) {
    let _ = append_quality_event(
        state.app_data_dir.as_path(),
        "anomalies.jsonl",
        &json!({
            "kind": "Extraction",
            "payload": payload
        }),
    );
}

fn capture_is_suppressed_by_screen_guide(generation: u64) -> bool {
    generation != 0
}

fn capture_overlapped_screen_guide(
    epoch_before: u64,
    epoch_after: u64,
    epoch_confirmed: u64,
    generation_after: u64,
) -> bool {
    capture_is_suppressed_by_screen_guide(generation_after)
        || epoch_before != epoch_after
        || epoch_after != epoch_confirmed
}

/// Dedicated capture thread / runtime stack. Matches the UI Tokio workers in
/// `main.rs`. The OS default (~2MB) overflows once Retina PNG encode, Vision
/// OCR, and Lance insert run on the same unnamed `block_on` thread.
pub const CAPTURE_THREAD_STACK_BYTES: usize = 8 * 1024 * 1024;

/// Start the capture loop on its own named runtime so OCR/insert cannot stall
/// the UI, with an 8MB stack so those frames cannot overflow.
pub fn spawn_capture_loop(state: Arc<AppState>) {
    std::thread::Builder::new()
        .name("fndr-capture".into())
        .stack_size(CAPTURE_THREAD_STACK_BYTES)
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_multi_thread()
                .thread_name("fndr-capture-worker")
                .thread_stack_size(CAPTURE_THREAD_STACK_BYTES)
                .enable_all()
                .build()
                .expect("Failed to build capture runtime");
            rt.block_on(async {
                if let Err(e) = run_capture_loop(state).await {
                    tracing::error!("Capture loop error: {}", e);
                }
            });
        })
        .expect("failed to spawn capture thread");
}

/// Run the main capture loop
/// Accessibility text is used instead of OCR when it has at least this many
/// characters (VS-16); shorter reads fall back to OCR as before.
const AX_TEXT_MIN_CHARS: usize = 200;
/// Bound for stored Accessibility text, kept for chunking.
const AX_TEXT_MAX_CHARS: usize = 20_000;

fn prefer_ax_text(char_count: usize) -> bool {
    char_count >= AX_TEXT_MIN_CHARS
}

/// `FNDR_AX_TEXT=0` turns Accessibility-first capture off.
fn ax_text_enabled() -> bool {
    std::env::var("FNDR_AX_TEXT")
        .map(|value| value != "0")
        .unwrap_or(true)
}

#[cfg(debug_assertions)]
fn dedupe_evidence(verdict: &DedupeVerdict) -> serde_json::Value {
    json!({
        "threshold": verdict.threshold,
        "match_kind": verdict.match_kind.as_str(),
        "hash_distance": verdict.hash_distance,
        "rgb_distance": verdict.rgb_distance,
    })
}

#[cfg(debug_assertions)]
fn finish_memory_journey_skip(
    state: &AppState,
    journey_id: Option<&str>,
    outcome: &str,
    privacy_blocked: bool,
    dedupe: Option<&DedupeVerdict>,
) {
    let Some(journey_id) = journey_id else {
        return;
    };
    let mut details = json!({ "privacy_blocked": privacy_blocked });
    if let Some(verdict) = dedupe {
        details["dedupe"] = dedupe_evidence(verdict);
    }
    if let Err(error) = state
        .memory_journey
        .finish_skipped_with(journey_id, outcome, details)
    {
        tracing::debug!("Could not finish skipped Memory Journey: {error}");
    }
    state.emit_memory_journey_status();
}

pub async fn run_capture_loop(state: Arc<AppState>) -> Result<(), Box<dyn std::error::Error>> {
    tracing::info!("Initializing capture pipeline...");

    // Initialize components
    let mut hasher = PerceptualHasher::new();
    let sampler = AdaptiveSampler::new();
    let ocr = OcrEngine::new()?;
    // CLIP image embedding model lives next to the BGE assets; resolved once per
    // process. The first stored frame absorbs the ~80-200 ms session load; every
    // subsequent embed is ~30-80 ms on Apple Silicon CPU.
    let models_dir = models::models_dir(state.app_data_dir.as_path());
    let chunking_config = state.config.read().chunking.clone();
    let mut text_embedder = match Embedder::with_chunking_config(&chunking_config) {
        Ok(embedder) => Some(embedder),
        Err(err) => {
            tracing::warn!("Semantic embeddings unavailable in capture loop: {}", err);
            None
        }
    };
    let mut last_embedder_init_attempt = Instant::now();
    let initial_capture_config = state.config.read().capture_pipeline.clone();

    // Batch buffer
    let mut batch: Vec<MemoryRecord> = Vec::new();
    let mut batch_outcomes: Vec<crate::StoreOutcome> = Vec::new();
    let mut continuity_index: HashMap<String, String> = HashMap::new();
    let mut last_flush = Instant::now();

    // Force capture timer
    let mut last_forced_capture = Instant::now();

    // Semantic dedup window suppresses repeated unchanged content bursts.
    let mut semantic_window = SemanticDedupWindow::default();
    let mut embedding_memo = EmbeddingMemo::new(
        initial_capture_config
            .embedding_cache_size
            .max(DEFAULT_CAPTURE_EMBEDDING_CACHE_SIZE),
    );
    // Adaptive admission state for the visual-narrative path (frames with
    // thin OCR but informative pixels). Resets per session key.
    let mut visual_tracker = VisualNoveltyTracker::default();
    let mut last_visual_failure_warn: Option<Instant> = None;

    // Without Screen Recording access macOS still returns a frame, but with
    // app windows blanked out, so OCR silently sees only wallpaper/menu bar.
    // Under `tauri dev` the grant is checked against the host app (terminal
    // or editor), not FNDR.
    let (screen_capture_allowed, screen_capture_detail) =
        permissions::preflight_screen_capture_access();
    if !screen_capture_allowed {
        tracing::warn!(
            "Screen Recording permission missing; captured frames will contain no window content. {} When running `tauri dev`, grant the terminal host app instead.",
            screen_capture_detail
        );
    }

    tracing::info!("Capture loop started");

    // Pushes `capture://status` to the UI only when counters or toggles change,
    // replacing the renderer's fixed-interval status poll. At most one event per
    // loop tick; none while idle.
    type StatusFingerprint = (u64, u64, u64, u64, u64, bool, bool, bool);
    let mut last_status_fingerprint: Option<StatusFingerprint> = None;

    loop {
        let status_fingerprint = (
            state.frames_captured.load(Ordering::Relaxed),
            state.frames_dropped.load(Ordering::Relaxed),
            state.capture_stats.evaluated.load(Ordering::Relaxed),
            state.capture_stats.total_stored(),
            state.capture_stats.total_skipped(),
            state.is_paused.load(Ordering::SeqCst),
            state.is_incognito.load(Ordering::SeqCst),
            state.ai_model_loaded(),
        );
        if last_status_fingerprint != Some(status_fingerprint) {
            crate::ipc::commands::emit_capture_status(state.as_ref());
            last_status_fingerprint = Some(status_fingerprint);
        }

        let config = state.config.read().clone();
        let flush_interval = Duration::from_secs(config.capture_pipeline.flush_interval_secs);
        let max_batch_size = config.capture_pipeline.max_batch_size;

        // Flush batch if needed
        let should_flush = batch.len() >= max_batch_size || last_flush.elapsed() >= flush_interval;
        if should_flush && !batch.is_empty() {
            // Filter batch and outcomes in lockstep so their indices stay
            // aligned. A plain `retain` + `truncate` would keep the first N
            // outcomes regardless of which records were removed.
            let keep: Vec<bool> = batch
                .iter()
                .map(|record| {
                    !Blocklist::is_blocked(&record.app_name, &config.blocklist)
                        && !Blocklist::is_context_blocked(
                            record.url.as_deref(),
                            Some(&record.window_title),
                            &config.blocklist,
                        )
                })
                .collect();
            {
                let mut keep_iter = keep.iter().copied();
                batch_outcomes.retain(|_| keep_iter.next().unwrap_or(false));
            }
            {
                let mut keep_iter = keep.iter().copied();
                batch.retain(|_| keep_iter.next().unwrap_or(false));
            }
            if batch.is_empty() {
                #[cfg(debug_assertions)]
                {
                    let _ = state
                        .memory_journey
                        .fail_active_storage("filtered_before_flush", 0);
                    state.emit_memory_journey_status();
                }
                purge_capture_artifacts(state.store.frames_dir());
                last_flush = Instant::now();
                continue;
            }

            let flush_start = Instant::now();
            match state.store.add_batch_and_get_count(&batch).await {
                Ok(inserted_count) => {
                    if inserted_count > 0 {
                        for outcome in batch_outcomes.iter() {
                            state.capture_stats.record_store(*outcome);
                        }
                        if let Err(err) = context_runtime::sync_memory_records(
                            state.as_ref(),
                            &batch,
                            Some("screen"),
                        )
                        .await
                        {
                            tracing::warn!("Context runtime batch sync failed: {}", err);
                        }
                        for rec in batch.iter() {
                            state.enqueue_graph_from_flushed_memory(rec);
                            state.enqueue_memory_review_from_flushed_memory(rec);
                        }
                        if let Err(err) =
                            crate::ipc::commands::commit_graph_updates_now(state.clone()).await
                        {
                            tracing::debug!("immediate graph commit skipped: {}", err);
                        }
                    }
                    purge_capture_artifacts(state.store.frames_dir());
                    state.invalidate_memory_derived_caches();
                    let flush_ms = flush_start.elapsed().as_millis() as u64;
                    runtime_metrics::record_ms("capture.flush_ms", flush_ms);
                    #[cfg(debug_assertions)]
                    {
                        if let Some(memory_id) = state.memory_journey.active_memory_id() {
                            if let Some(record) = batch.iter().find(|record| record.id == memory_id)
                            {
                                let persisted = state
                                    .store
                                    .get_memory_by_id(&record.id)
                                    .await
                                    .ok()
                                    .flatten()
                                    .is_some();
                                let chunk_count = state
                                    .store
                                    .list_chunks_for_memory(&record.id)
                                    .await
                                    .map(|chunks| chunks.len())
                                    .unwrap_or(0);
                                let _ = state.memory_journey.complete_storage(
                                    record,
                                    persisted,
                                    chunk_count,
                                    flush_ms,
                                );
                            }
                        }
                        state.emit_memory_journey_status();
                    }
                    if inserted_count > 0 {
                        tracing::info!(
                            "Flushed: attempted {} records, inserted {} in {:?}",
                            batch.len(),
                            inserted_count,
                            std::time::Duration::from_millis(flush_ms)
                        );
                    } else {
                        tracing::debug!(
                            "Batch flush skipped: all {} records filtered/deduped during storage",
                            batch.len()
                        );
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to flush batch: {}", e);
                    #[cfg(debug_assertions)]
                    {
                        let _ = state.memory_journey.fail_active_storage(
                            "storage_error",
                            flush_start.elapsed().as_millis() as u64,
                        );
                        state.emit_memory_journey_status();
                    }
                }
            }
            batch.clear();
            batch_outcomes.clear();
            last_flush = Instant::now();
        }

        // Check if paused
        if !state.is_capturing() {
            tokio::time::sleep(Duration::from_millis(500)).await;
            continue;
        }

        // Screen Guide is deliberately ephemeral. Do not let the ordinary
        // memory pipeline sample either its overlay or the underlying target
        // while a guide turn owns the screen, even while that overlay is
        // briefly hidden for its own privacy-checked capture.
        let screen_guide_capture_epoch = state.screen_guide_capture_epoch.load(Ordering::SeqCst);
        if capture_is_suppressed_by_screen_guide(
            state.screen_guide_capture_generation.load(Ordering::SeqCst),
        ) {
            tokio::time::sleep(Duration::from_millis(250)).await;
            continue;
        }

        // An armed Memory Journey waits for the person to bring the target
        // forward. Frames seen meanwhile would seed dedupe history with the
        // target itself, so the ordinary tick is skipped until the arm starts.
        #[cfg(debug_assertions)]
        if state.memory_journey.defers_ordinary_capture() {
            tokio::time::sleep(Duration::from_millis(250)).await;
            continue;
        }

        // Calculate sleep duration based on FPS
        let fps = sampler.get_current_fps(&config);
        if fps <= 0.0 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        }
        let sleep_duration = Duration::from_secs_f64(1.0 / fps);

        // We're past the "paused" / "deep idle" gates and intend to look at a
        // frame this tick — count it for the storage-rate denominator.
        state.capture_stats.record_evaluated();

        // Get active application info
        let context_started = Instant::now();
        let app_context = macos::get_frontmost_app_info();
        let app_name = app_context.app_name.clone();
        let window_title = app_context.window_title.clone();
        runtime_metrics::since_ms("capture.context_ms", context_started);
        #[cfg(debug_assertions)]
        let mut memory_journey_id = {
            let target_app_class = if app_context.browser_url.is_some() {
                "browser"
            } else if app_context.bundle_id.as_deref() == Some("com.fndr.app") {
                "fndr"
            } else {
                "desktop_app"
            };
            match state.memory_journey.begin_capture_attempt(target_app_class) {
                Ok(id) => {
                    if id.is_some() {
                        state.emit_memory_journey_status();
                    }
                    id
                }
                Err(error) => {
                    tracing::warn!("Could not start armed Memory Journey: {error}");
                    None
                }
            }
        };

        // A missing text embedder blocks the frame instead of letting
        // zero-vector memory rows reach storage; periodic re-init lets capture
        // resume without a restart once the embedding model is on disk.
        let embedder_gate = embedder_gate_action(
            text_embedder.is_some(),
            last_embedder_init_attempt.elapsed(),
            EMBEDDER_INIT_RETRY_INTERVAL,
        );
        if embedder_gate == EmbedderGateAction::RetryInit {
            last_embedder_init_attempt = Instant::now();
            let chunking_config = state.config.read().chunking.clone();
            match Embedder::with_chunking_config(&chunking_config) {
                Ok(embedder) => {
                    tracing::info!("Text embedder initialized on retry; capture resumed");
                    text_embedder = Some(embedder);
                }
                Err(err) => {
                    tracing::warn!("Text embedder still unavailable; capture blocked: {}", err);
                }
            }
        }
        if embedder_gate != EmbedderGateAction::Proceed && text_embedder.is_none() {
            state
                .capture_stats
                .record_skip(crate::SkipReason::EmbedderUnavailable, &app_name);
            #[cfg(debug_assertions)]
            finish_memory_journey_skip(
                state.as_ref(),
                memory_journey_id.as_deref(),
                "embedder_unavailable",
                false,
                None,
            );
            tokio::time::sleep(sleep_duration).await;
            continue;
        }

        let force_capture =
            last_forced_capture.elapsed().as_secs() >= config.forced_capture_interval;

        let url = app_context
            .browser_url
            .as_deref()
            .map(strip_url_credentials);
        if url.is_some() {
            tracing::debug!("Frontmost browser URL available for capture policy evaluation");
        }

        // Close the race where Screen Guide starts after the loop-level gate
        // but before a URL-only record or pixel capture is admitted.
        if capture_overlapped_screen_guide(
            screen_guide_capture_epoch,
            state.screen_guide_capture_epoch.load(Ordering::SeqCst),
            state.screen_guide_capture_epoch.load(Ordering::SeqCst),
            state.screen_guide_capture_generation.load(Ordering::SeqCst),
        ) {
            #[cfg(debug_assertions)]
            finish_memory_journey_skip(
                state.as_ref(),
                memory_journey_id.as_deref(),
                "screen_guide_active",
                true,
                None,
            );
            tokio::time::sleep(sleep_duration).await;
            continue;
        }

        if let Some(reason) = capture_context_skip_reason(
            &app_name,
            app_context.bundle_id.as_deref(),
            &window_title,
            url.as_deref(),
            &config.blocklist,
        ) {
            if reason == crate::SkipReason::SensitiveContext {
                queue_sensitive_context_alert(
                    state.as_ref(),
                    url.as_deref(),
                    &window_title,
                    &config.dismissed_privacy_alerts,
                );
                emit_sensitive_context_skip(state.as_ref(), "metadata");
                tracing::info!(
                    "Skipping capture: deterministic privacy gate rejected sensitive metadata"
                );
            } else {
                tracing::debug!(reason = ?reason, "Skipping capture before content processing");
            }
            state.capture_stats.record_skip(reason, &app_name);
            #[cfg(debug_assertions)]
            finish_memory_journey_skip(
                state.as_ref(),
                memory_journey_id.as_deref(),
                reason.as_str(),
                matches!(
                    reason,
                    crate::SkipReason::SensitiveContext | crate::SkipReason::SelfApp
                ),
                None,
            );
            tokio::time::sleep(sleep_duration).await;
            continue;
        }

        queue_sensitive_context_alert(
            state.as_ref(),
            url.as_deref(),
            &window_title,
            &config.dismissed_privacy_alerts,
        );

        let surface_policy =
            classify_capture_surface_policy(&app_name, &window_title, url.as_deref());
        if surface_policy == CaptureSurfacePolicy::SkipFrame {
            emit_capture_quality_signal(
                state.as_ref(),
                json!({
                    "timestamp_ms": chrono::Utc::now().timestamp_millis(),
                    "app_name": app_name,
                    "bundle_id": app_context.bundle_id.clone(),
                    "domain": url.as_deref().and_then(extract_domain).unwrap_or_default(),
                    "surface_policy": "skip_frame",
                    "low_signal": true,
                    "stored_or_skipped": "skipped_surface_policy",
                }),
            );
            state
                .capture_stats
                .record_skip(crate::SkipReason::SurfacePolicy, &app_name);
            #[cfg(debug_assertions)]
            finish_memory_journey_skip(
                state.as_ref(),
                memory_journey_id.as_deref(),
                "surface_policy",
                true,
                None,
            );
            tokio::time::sleep(sleep_duration).await;
            continue;
        }
        if surface_policy == CaptureSurfacePolicy::UrlOnly {
            let mut h = DefaultHasher::new();
            app_name.hash(&mut h);
            window_title.hash(&mut h);
            url.hash(&mut h);
            let semantic_hash = h.finish();
            let now_ms = chrono::Utc::now().timestamp_millis();
            if semantic_window.should_skip(
                semantic_hash,
                now_ms,
                config.capture_pipeline.semantic_dedup_window_ms,
            ) && !force_capture
            {
                state
                    .capture_stats
                    .record_skip(crate::SkipReason::SemanticDup, &app_name);
                #[cfg(debug_assertions)]
                finish_memory_journey_skip(
                    state.as_ref(),
                    memory_journey_id.as_deref(),
                    "semantic_duplicate",
                    false,
                    None,
                );
                tokio::time::sleep(sleep_duration).await;
                continue;
            }
            let now = Local::now();
            let session_key = build_session_key(&app_name, &window_title, url.as_deref());
            let session_id = build_session_id(
                &now,
                &app_name,
                app_context.bundle_id.as_deref(),
                &session_key,
            );
            let domain = url
                .as_deref()
                .and_then(extract_domain)
                .unwrap_or_else(|| "unknown_domain".to_string());
            let snippet = if !window_title.trim().is_empty() {
                window_title.trim().to_string()
            } else {
                format!("Visited {}", domain)
            };
            let memory_context = format!("URL-only surface capture for {} at {}", domain, snippet);
            let mut reopen_target = build_reopen_target(
                url.as_deref(),
                macos::preferred_reopen_file_path(app_context.document_path.as_deref(), &[]),
                app_context.bundle_id.as_deref(),
                &app_name,
                now.timestamp_millis(),
            );
            reopen_target.page =
                crate::memory::reopen::detect_reopen_page(&reopen_target, &window_title, "");
            let mut record = MemoryRecord {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp: now.timestamp_millis(),
                day_bucket: now.format("%Y-%m-%d").to_string(),
                app_name: app_name.clone(),
                bundle_id: app_context.bundle_id.clone(),
                window_title: window_title.clone(),
                session_id,
                text: String::new(),
                clean_text: String::new(),
                ocr_confidence: 0.0,
                ocr_block_count: 0,
                snippet: snippet.clone(),
                display_summary: snippet.clone(),
                internal_context: memory_context.clone(),
                summary_source: "url_only".to_string(),
                noise_score: 0.0,
                session_key,
                lexical_shadow: build_lexical_shadow(&window_title, &snippet, "", url.as_deref()),
                embedding: vec![0.0; EMBEDDING_DIM],
                image_embedding: vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM],
                screenshot_path: None,
                url: url.clone(),
                snippet_embedding: vec![0.0; EMBEDDING_DIM],
                support_embedding: vec![0.0; EMBEDDING_DIM],
                decay_score: 1.0,
                last_accessed_at: 0,
                timestamp_start: now.timestamp_millis(),
                timestamp_end: now.timestamp_millis(),
                source_type: "browser_url_only".to_string(),
                topic: "navigation_surface".to_string(),
                workflow: "browsing".to_string(),
                user_intent: "navigating".to_string(),
                memory_context: memory_context.clone(),
                raw_evidence: json!({
                    "surface_policy": "url_only",
                    "timestamp_ms": now.timestamp_millis(),
                    "app_name": app_name,
                    "window_title": window_title,
                    "url": url,
                })
                .to_string(),
                reopen_kind: reopen_target.kind,
                reopen_url: reopen_target.url,
                reopen_file_path: reopen_target.file_path,
                reopen_app_bundle_id: reopen_target.app_bundle_id,
                reopen_app_name: reopen_target.app_name,
                reopen_app_deep_link: reopen_target.app_deep_link,
                reopen_captured_at_ms: reopen_target.captured_at_ms,
                reopen_confidence: reopen_target.confidence,
                reopen_validation_status: reopen_target.validation_status,
                reopen_page: reopen_target.page,
                schema_version: 2,
                activity_type: "browsing".to_string(),
                embedding_text: format!("url: {} | title: {}", domain, snippet),
                embedding_model: "all-MiniLM-L6-v2".to_string(),
                embedding_dim: EMBEDDING_DIM as u32,
                synthesis_branch: "url_only".to_string(),
                ..Default::default()
            };
            record.dedup_fingerprint =
                deterministic_dedup_fingerprint(&record, Some(&record.memory_context));
            let screen_guide_epoch_after = state.screen_guide_capture_epoch.load(Ordering::SeqCst);
            let screen_guide_generation_after =
                state.screen_guide_capture_generation.load(Ordering::SeqCst);
            if capture_overlapped_screen_guide(
                screen_guide_capture_epoch,
                screen_guide_epoch_after,
                state.screen_guide_capture_epoch.load(Ordering::SeqCst),
                screen_guide_generation_after,
            ) {
                #[cfg(debug_assertions)]
                finish_memory_journey_skip(
                    state.as_ref(),
                    memory_journey_id.as_deref(),
                    "screen_guide_active",
                    true,
                    None,
                );
                tokio::time::sleep(sleep_duration).await;
                continue;
            }
            #[cfg(debug_assertions)]
            if let Some(journey_id) = memory_journey_id.as_deref() {
                let _ = state.memory_journey.record_stage(
                    journey_id,
                    crate::memory_journey::MemoryJourneyStageRecord {
                        name: "text_source".to_string(),
                        status: crate::memory_journey::MemoryJourneyStageStatus::Observed,
                        observed_at_ms: chrono::Utc::now().timestamp_millis(),
                        duration_ms: None,
                        outcome: "url_only".to_string(),
                        details: json!({ "source": "browser_url_metadata" }),
                        artifact_ids: Vec::new(),
                    },
                );
                let _ = state.memory_journey.record_stage(
                    journey_id,
                    crate::memory_journey::MemoryJourneyStageRecord {
                        name: "extraction".to_string(),
                        status: crate::memory_journey::MemoryJourneyStageStatus::Skipped,
                        observed_at_ms: chrono::Utc::now().timestamp_millis(),
                        duration_ms: None,
                        outcome: "not_required_for_url_only".to_string(),
                        details: json!(null),
                        artifact_ids: Vec::new(),
                    },
                );
                let _ = state
                    .memory_journey
                    .attach_memory_id(journey_id, &record.id);
                state.emit_memory_journey_status();
            }
            batch.push(record);
            batch_outcomes.push(crate::StoreOutcome::UrlOnly);
            if force_capture {
                last_forced_capture = Instant::now();
            }
            emit_capture_quality_signal(
                state.as_ref(),
                json!({
                    "timestamp_ms": now.timestamp_millis(),
                    "app_name": app_name,
                    "bundle_id": app_context.bundle_id.clone(),
                    "domain": domain,
                    "surface_policy": "url_only",
                    "low_signal": false,
                    "stored_or_skipped": "stored_url_only_surface",
                    "grounding_confidence": 0.0
                }),
            );
            state.frames_captured.fetch_add(1, Ordering::Relaxed);
            state
                .last_capture_time
                .store(now.timestamp_millis() as u64, Ordering::Relaxed);
            tokio::time::sleep(sleep_duration).await;
            continue;
        }

        // The frontmost app can change in the gap between reading window
        // context above and capturing pixels here (surface-policy checks,
        // dedup hashing, and privacy-alert queuing all run in between). A
        // cheap identity re-check (no Accessibility tree walk) catches a
        // fast app-switch so pixels never get stored under a stale app
        // name and window title.
        if macos::frontmost_bundle_id() != app_context.bundle_id {
            tracing::debug!(
                "Frontmost app changed since context read (was {:?}); discarding frame",
                app_context.bundle_id
            );
            state
                .capture_stats
                .record_skip(crate::SkipReason::AppSwitchedDuringCapture, &app_name);
            #[cfg(debug_assertions)]
            finish_memory_journey_skip(
                state.as_ref(),
                memory_journey_id.as_deref(),
                "app_switched_during_capture",
                true,
                None,
            );
            tokio::time::sleep(sleep_duration).await;
            continue;
        }

        // Capture screen. Check on both sides of the synchronous OS call: if
        // Screen Guide appeared while the call was in flight, discard these
        // bytes before hashing, OCR, inference, or storage can observe them.
        let screen_guide_capture_epoch = state.screen_guide_capture_epoch.load(Ordering::SeqCst);
        if capture_is_suppressed_by_screen_guide(
            state.screen_guide_capture_generation.load(Ordering::SeqCst),
        ) {
            #[cfg(debug_assertions)]
            finish_memory_journey_skip(
                state.as_ref(),
                memory_journey_id.as_deref(),
                "screen_guide_active",
                true,
                None,
            );
            tokio::time::sleep(sleep_duration).await;
            continue;
        }
        let pixels_started = Instant::now();
        let capture_result = macos::capture_screen();
        #[cfg(debug_assertions)]
        let pixels_duration_ms = pixels_started.elapsed().as_millis() as u64;
        runtime_metrics::since_ms("capture.pixels_ms", pixels_started);
        let image_data = match capture_result {
            Ok(data) => data,
            Err(e) => {
                tracing::warn!("Screen capture failed: {}", e);
                state
                    .capture_stats
                    .record_skip(crate::SkipReason::ScreenCaptureFailed, &app_name);
                #[cfg(debug_assertions)]
                finish_memory_journey_skip(
                    state.as_ref(),
                    memory_journey_id.as_deref(),
                    "screen_capture_failed",
                    false,
                    None,
                );
                tokio::time::sleep(sleep_duration).await;
                continue;
            }
        };
        let screen_guide_capture_epoch_after =
            state.screen_guide_capture_epoch.load(Ordering::SeqCst);
        let screen_guide_generation_after =
            state.screen_guide_capture_generation.load(Ordering::SeqCst);
        if capture_overlapped_screen_guide(
            screen_guide_capture_epoch,
            screen_guide_capture_epoch_after,
            state.screen_guide_capture_epoch.load(Ordering::SeqCst),
            screen_guide_generation_after,
        ) {
            drop(image_data);
            #[cfg(debug_assertions)]
            finish_memory_journey_skip(
                state.as_ref(),
                memory_journey_id.as_deref(),
                "screen_guide_overlap",
                true,
                None,
            );
            tokio::time::sleep(sleep_duration).await;
            continue;
        }

        #[cfg(debug_assertions)]
        if let Some(journey_id) = memory_journey_id.as_deref() {
            if let Err(error) =
                state
                    .memory_journey
                    .record_frame(journey_id, &image_data, pixels_duration_ms)
            {
                tracing::warn!("Memory Journey frame recording stopped: {error}");
                memory_journey_id = None;
            } else {
                state.emit_memory_journey_status();
            }
        }

        // Deduplication check
        let dedupe_started = Instant::now();
        let dedupe_verdict = hasher.check(&image_data, config.dedupe_threshold);
        let is_duplicate = dedupe_verdict.is_duplicate;
        runtime_metrics::since_ms("capture.dedupe_ms", dedupe_started);

        if is_duplicate && !force_capture {
            state.frames_dropped.fetch_add(1, Ordering::Relaxed);
            state
                .capture_stats
                .record_skip(crate::SkipReason::PerceptualDup, &app_name);
            #[cfg(debug_assertions)]
            finish_memory_journey_skip(
                state.as_ref(),
                memory_journey_id.as_deref(),
                "perceptual_duplicate",
                false,
                Some(&dedupe_verdict),
            );
            tokio::time::sleep(sleep_duration).await;
            continue;
        }

        #[cfg(debug_assertions)]
        if let Some(journey_id) = memory_journey_id.as_deref() {
            let _ = state.memory_journey.record_stage(
                journey_id,
                crate::memory_journey::MemoryJourneyStageRecord {
                    name: "admission".to_string(),
                    status: crate::memory_journey::MemoryJourneyStageStatus::Observed,
                    observed_at_ms: chrono::Utc::now().timestamp_millis(),
                    duration_ms: Some(dedupe_started.elapsed().as_millis() as u64),
                    outcome: "allowed".to_string(),
                    details: json!({
                        "target_app_class": if app_context.browser_url.is_some() { "browser" } else { "desktop_app" },
                        "privacy_decision": "allowed",
                        "surface_decision": "allowed",
                        "dedupe_decision": if force_capture { "forced" } else { "novel" },
                    "dedupe": dedupe_evidence(&dedupe_verdict),
                    }),
                    artifact_ids: Vec::new(),
                },
            );
        }

        tracing::info!("Processing new frame from {}", app_name);

        if force_capture {
            last_forced_capture = Instant::now();
        }

        let semantic_page = if surface_policy == CaptureSurfacePolicy::Normal {
            macos::get_browser_semantic_content(&app_name)
        } else {
            None
        };
        if let Some(page) = semantic_page.as_ref() {
            if page.nav_ratio > 0.58 && page.content_signal_score < 0.18 {
                emit_capture_quality_signal(
                    state.as_ref(),
                    json!({
                        "timestamp_ms": chrono::Utc::now().timestamp_millis(),
                        "app_name": app_name,
                        "bundle_id": app_context.bundle_id.clone(),
                        "domain": url.as_deref().and_then(extract_domain).unwrap_or_default(),
                        "surface_policy": "skip_frame",
                        "source_kind": "browser_semantic",
                        "semantic_nav_ratio": page.nav_ratio,
                        "semantic_content_score": page.content_signal_score,
                        "stored_or_skipped": "skipped_surface_policy",
                        "low_signal": true
                    }),
                );
                state
                    .capture_stats
                    .record_skip(crate::SkipReason::SurfacePolicy, &app_name);
                #[cfg(debug_assertions)]
                finish_memory_journey_skip(
                    state.as_ref(),
                    memory_journey_id.as_deref(),
                    "browser_semantic_low_signal",
                    false,
                    None,
                );
                tokio::time::sleep(sleep_duration).await;
                continue;
            }
        }
        let mut source_kind = "ocr";
        let mut source_low_signal = false;
        #[cfg(debug_assertions)]
        let mut memory_journey_positioned_line_count: Option<usize> = None;
        let ocr_start = Instant::now();
        // VS-16: exact Accessibility text beats OCR when the window exposes
        // enough of it. Browser semantic content keeps priority (it already
        // gives page text); privacy gates above have run for this frame.
        let ax_candidate =
            if semantic_page.as_ref().is_some_and(|page| page.has_signal()) || !ax_text_enabled() {
                None
            } else {
                let ax_started = Instant::now();
                let found = crate::accessibility::frontmost_focused_text(AX_TEXT_MAX_CHARS);
                runtime_metrics::since_ms("capture.ax_ms", ax_started);
                found.filter(|ax| prefer_ax_text(ax.text.chars().count()))
            };
        let (text, qwen_cleaned_text, capture_quality, observed_confidence, observed_block_count) =
            if let Some(ax) = ax_candidate {
                source_kind = "ax";
                runtime_metrics::bump("capture.text_source_ax");
                let high_signal = text_cleanup::build_high_signal_text_for_app(&app_name, &ax.text);
                let mut stats = high_signal.stats;
                if stats.total_lines == 0 {
                    stats.total_lines = 1;
                }
                if stats.kept_lines == 0 && !high_signal.text.trim().is_empty() {
                    stats.kept_lines = 1;
                }
                stats.low_conf_lines = 0;
                // Accessibility text is exact, not recognized: no OCR confidence.
                stats.avg_line_score = 0.9;
                let kept = high_signal.stats.kept_lines.max(1);
                (
                    high_signal.text.clone(),
                    high_signal.text,
                    stats,
                    0.95,
                    kept,
                )
            } else if let Some(semantic) = semantic_page.as_ref().filter(|page| page.has_signal()) {
                source_kind = "browser_semantic";
                let semantic_text = semantic.content_text();
                let high_signal =
                    text_cleanup::build_high_signal_text_for_app(&app_name, &semantic_text);
                let mut stats = high_signal.stats;
                if stats.total_lines == 0 {
                    stats.total_lines = 1;
                }
                if stats.kept_lines == 0 && !high_signal.text.trim().is_empty() {
                    stats.kept_lines = 1;
                }
                stats.low_conf_lines = 0;
                stats.avg_line_score = (0.68 + semantic.content_signal_score * 0.28
                    - semantic.nav_ratio * 0.18)
                    .clamp(0.0, 1.0);
                (
                    high_signal.text.clone(),
                    high_signal.text,
                    stats,
                    (0.62 + semantic.content_signal_score * 0.30 - semantic.nav_ratio * 0.12)
                        .clamp(0.0, 1.0),
                    high_signal.stats.kept_lines.max(1),
                )
            } else {
                let ocr_stage_started = Instant::now();
                let (ocr_result, qwen_cleaned) = match ocr.recognize_with_metadata(&image_data) {
                    Ok(result) => result,
                    Err(e) => {
                        tracing::warn!("OCR failed: {}", e);
                        state
                            .capture_stats
                            .record_skip(crate::SkipReason::OcrFailed, &app_name);
                        #[cfg(debug_assertions)]
                        finish_memory_journey_skip(
                            state.as_ref(),
                            memory_journey_id.as_deref(),
                            "ocr_failed",
                            false,
                            None,
                        );
                        tokio::time::sleep(sleep_duration).await;
                        continue;
                    }
                };
                runtime_metrics::since_ms("capture.ocr_ms", ocr_stage_started);
                runtime_metrics::bump("capture.text_source_ocr");
                #[cfg(debug_assertions)]
                if let Some(journey_id) = memory_journey_id.as_deref() {
                    memory_journey_positioned_line_count =
                        Some(ocr_result.debug_positioned_lines.len());
                    let lines = serde_json::to_vec_pretty(&ocr_result.debug_positioned_lines)
                        .unwrap_or_default();
                    let _ = state.memory_journey.record_artifact(
                        journey_id,
                        "ocr",
                        "positioned-lines.json",
                        &lines,
                    );
                    let _ = state.memory_journey.record_artifact(
                        journey_id,
                        "ocr",
                        "vision-normalized.txt",
                        ocr_result.text.as_bytes(),
                    );
                }
                // DEBUG: Log OCR pipeline filtering to diagnose zero-confidence issues
                tracing::debug!(
                    "OCR raw result [{}]: confidence={:.3}, blocks={}, text_len={}, stats={{kept_lines={}, dropped={}, low_conf={}}}",
                    app_name,
                    ocr_result.confidence,
                    ocr_result.block_count,
                    ocr_result.text.len(),
                    ocr_result.ocr_stats.lines_used,
                    ocr_result.ocr_stats.lines_dropped,
                    ocr_result.ocr_stats.low_conf_count
                );
                source_low_signal = ocr_result.is_low_signal(config.min_text_length);
                let cleanup_started = Instant::now();
                let high_signal = extract_ocr_text(&app_name, &ocr_result);
                runtime_metrics::since_ms("capture.cleanup_ms", cleanup_started);
                (
                    high_signal.text.clone(),
                    qwen_cleaned,
                    high_signal.stats,
                    ocr_result.confidence,
                    ocr_result.block_count,
                )
            };
        let ocr_latency = ocr_start.elapsed();
        tracing::info!(
            "Capture text source={} chars={} latency_ms={} confidence={:.2} blocks={}",
            source_kind,
            text.len(),
            ocr_latency.as_millis(),
            observed_confidence,
            observed_block_count
        );
        #[cfg(debug_assertions)]
        if let Some(journey_id) = memory_journey_id.as_deref() {
            let _ = state.memory_journey.record_stage(
                journey_id,
                crate::memory_journey::MemoryJourneyStageRecord {
                    name: "text_source".to_string(),
                    status: crate::memory_journey::MemoryJourneyStageStatus::Observed,
                    observed_at_ms: chrono::Utc::now().timestamp_millis(),
                    duration_ms: Some(ocr_latency.as_millis() as u64),
                    outcome: source_kind.to_string(),
                    details: json!({ "source": source_kind }),
                    artifact_ids: Vec::new(),
                },
            );
            let _ = state.memory_journey.record_stage(
                journey_id,
                crate::memory_journey::MemoryJourneyStageRecord {
                    name: "ocr".to_string(),
                    status: crate::memory_journey::MemoryJourneyStageStatus::Observed,
                    observed_at_ms: chrono::Utc::now().timestamp_millis(),
                    duration_ms: Some(ocr_latency.as_millis() as u64),
                    outcome: "recognized".to_string(),
                    details: json!({
                        "confidence": observed_confidence,
                        "block_count": observed_block_count,
                        "positioned_line_count": memory_journey_positioned_line_count,
                        "positioned_lines_available": memory_journey_positioned_line_count.is_some(),
                    }),
                    artifact_ids: Vec::new(),
                },
            );
            let _ = state.memory_journey.record_artifact(
                journey_id,
                "ocr",
                "recognized.txt",
                qwen_cleaned_text.as_bytes(),
            );
            let raw_chars = qwen_cleaned_text.chars().count();
            let clean_chars = text.chars().count();
            let _ = state.memory_journey.record_stage(
                journey_id,
                crate::memory_journey::MemoryJourneyStageRecord {
                    name: "cleanup".to_string(),
                    status: crate::memory_journey::MemoryJourneyStageStatus::Observed,
                    observed_at_ms: chrono::Utc::now().timestamp_millis(),
                    duration_ms: None,
                    outcome: "cleaned".to_string(),
                    details: json!({
                        "raw_chars": raw_chars,
                        "clean_chars": clean_chars,
                        "preservation_ratio": if raw_chars == 0 { 0.0 } else { clean_chars as f64 / raw_chars as f64 },
                        "total_lines": capture_quality.total_lines,
                        "kept_lines": capture_quality.kept_lines,
                        "low_conf_lines": capture_quality.low_conf_lines,
                        "dropped_noise_lines": capture_quality.dropped_noise_lines,
                        "dropped_low_signal_lines": capture_quality.dropped_low_signal_lines,
                    }),
                    artifact_ids: Vec::new(),
                },
            );
            let _ = state.memory_journey.record_artifact(
                journey_id,
                "cleanup",
                "cleaned.txt",
                text.as_bytes(),
            );
            let raw_lines = qwen_cleaned_text.lines().collect::<Vec<_>>();
            let clean_lines = text.lines().collect::<Vec<_>>();
            let cleanup_diff = serde_json::to_vec_pretty(&json!({
                "removed_lines": raw_lines
                    .iter()
                    .filter(|line| !clean_lines.contains(line))
                    .collect::<Vec<_>>(),
                "added_lines": clean_lines
                    .iter()
                    .filter(|line| !raw_lines.contains(line))
                    .collect::<Vec<_>>(),
            }))
            .unwrap_or_default();
            let _ = state.memory_journey.record_artifact(
                journey_id,
                "cleanup",
                "cleanup-diff.json",
                &cleanup_diff,
            );
            state.emit_memory_journey_status();
        }

        // Metadata-only checks run before pixels are captured. Secret
        // patterns can only be found after transient OCR/semantic extraction,
        // so run the same deterministic gate again before image embeddings,
        // model inference, vector construction, or durable storage.
        if capture_admission_skip_reason(
            &app_name,
            app_context.bundle_id.as_deref(),
            &window_title,
            url.as_deref(),
            Some(&qwen_cleaned_text),
            &config.blocklist,
        ) == Some(crate::SkipReason::SensitiveContext)
        {
            emit_sensitive_context_skip(state.as_ref(), "transient_text");
            tracing::info!("Skipping capture: deterministic privacy gate rejected transient text");
            state
                .capture_stats
                .record_skip(crate::SkipReason::SensitiveContext, &app_name);
            #[cfg(debug_assertions)]
            if let Some(journey_id) = memory_journey_id.as_deref() {
                if let Err(error) = state
                    .memory_journey
                    .redact_and_finish_privacy_skip(journey_id, "sensitive_transient_text")
                {
                    tracing::debug!("Could not finish privacy-blocked Memory Journey: {error}");
                }
                state.emit_memory_journey_status();
            }
            drop(image_data);
            tokio::time::sleep(sleep_duration).await;
            continue;
        }

        // If the text source is too weak/noisy to drive the OCR-narrative
        // pipeline, attempt the visual-narrative path. The visual-admission
        // gate now rejects only tiny frames and near-duplicates; low-RAM or
        // missing VLM falls back to OCR/window metadata instead of forcing a
        // hard skip.
        if source_low_signal || text.len() < config.min_text_length {
            let session_key_visual = build_session_key(&app_name, &window_title, url.as_deref());
            visual_tracker.reset_for(&session_key_visual);
            let outcome = try_admit_visual_capture(
                &image_data,
                &visual_tracker,
                &config.capture_pipeline,
                &models_dir,
            )
            .await;
            match outcome {
                VisualAdmissionOutcome::Admitted { image_vec, novelty } => {
                    // Hold the global model pipeline lock for the whole
                    // visual-narrative path: VLM + BGE batch + (lookups).
                    // Same invariant as the OCR pipeline above.
                    let _visual_guard = state.model_pipeline_lock.lock().await;
                    let semantic_started = Instant::now();
                    let visual_compose_future = compose_visual_capture_record(
                        state.as_ref(),
                        text_embedder.as_ref(),
                        &mut embedding_memo,
                        image_data.clone(),
                        image_vec.clone(),
                        &app_name,
                        app_context.bundle_id.as_deref(),
                        &window_title,
                        url.as_deref(),
                        &text,
                        text.len(),
                        observed_confidence,
                        observed_block_count,
                        novelty,
                    );
                    #[cfg(debug_assertions)]
                    let visual_compose_result = if let Some(journey_id) = memory_journey_id.clone()
                    {
                        crate::telemetry::llm_trace::with_memory_journey(
                            state.memory_journey.clone(),
                            journey_id,
                            visual_compose_future,
                        )
                        .await
                    } else {
                        visual_compose_future.await
                    };
                    #[cfg(not(debug_assertions))]
                    let visual_compose_result = visual_compose_future.await;
                    runtime_metrics::since_ms("capture.semantic_ms", semantic_started);
                    match visual_compose_result {
                        Ok(record) => {
                            #[cfg(debug_assertions)]
                            if let Some(journey_id) = memory_journey_id.as_deref() {
                                let _ = state.memory_journey.record_vector_contracts(
                                    journey_id,
                                    &record.embedding_model,
                                    (&record.embedding, &record.embedding_text),
                                    (&record.snippet_embedding, &record.snippet),
                                    (&record.support_embedding, &record.memory_context),
                                    (&record.image_embedding, "captured_frame"),
                                    semantic_started.elapsed().as_millis() as u64,
                                );
                                let _ = state
                                    .memory_journey
                                    .attach_memory_id(journey_id, &record.id);
                                state.emit_memory_journey_status();
                            }
                            visual_tracker.admit(
                                image_vec,
                                config.capture_pipeline.visual_novelty_ring_capacity,
                            );
                            batch.push(record);
                            batch_outcomes.push(crate::StoreOutcome::VisualPath);
                            state.frames_captured.fetch_add(1, Ordering::Relaxed);
                            state.last_capture_time.store(
                                chrono::Utc::now().timestamp_millis() as u64,
                                Ordering::Relaxed,
                            );
                            emit_capture_quality_signal(
                                state.as_ref(),
                                json!({
                                    "timestamp_ms": chrono::Utc::now().timestamp_millis(),
                                    "app_name": app_name,
                                    "bundle_id": app_context.bundle_id.clone(),
                                    "ocr_confidence": observed_confidence,
                                    "ocr_block_count": observed_block_count,
                                    "clean_text_len": text.len(),
                                    "noise_score": 0.0,
                                    "low_signal": false,
                                    "stored_or_skipped": "stored_visual_capture",
                                    "source_kind": "visual_capture",
                                    "visual_admission_novelty": novelty,
                                    "visual_admission_threshold": visual_tracker
                                        .adaptive_threshold(
                                            config.capture_pipeline.visual_novelty_base,
                                            config.capture_pipeline.visual_novelty_alpha,
                                            config.capture_pipeline.visual_novelty_ceiling,
                                        ),
                                }),
                            );
                        }
                        Err(err) => {
                            tracing::warn!(
                                "visual-admission: VLM composition failed for {}: {}",
                                app_name,
                                err
                            );
                            emit_capture_quality_signal(
                                state.as_ref(),
                                json!({
                                    "timestamp_ms": chrono::Utc::now().timestamp_millis(),
                                    "app_name": app_name,
                                    "bundle_id": app_context.bundle_id.clone(),
                                    "stored_or_skipped": "skipped_visual_compose_failed",
                                    "source_kind": "visual_capture",
                                    "reason": err,
                                }),
                            );
                            state
                                .capture_stats
                                .record_skip(crate::SkipReason::VisualComposeFailed, &app_name);
                            #[cfg(debug_assertions)]
                            finish_memory_journey_skip(
                                state.as_ref(),
                                memory_journey_id.as_deref(),
                                "visual_compose_failed",
                                false,
                                None,
                            );
                        }
                    }
                }
                VisualAdmissionOutcome::SkippedSmall { width, height } => {
                    emit_capture_quality_signal(
                        state.as_ref(),
                        json!({
                            "timestamp_ms": chrono::Utc::now().timestamp_millis(),
                            "app_name": app_name,
                            "bundle_id": app_context.bundle_id.clone(),
                            "stored_or_skipped": "skipped_visual_small_dim",
                            "source_kind": source_kind,
                            "width": width,
                            "height": height,
                        }),
                    );
                    state
                        .capture_stats
                        .record_skip(crate::SkipReason::VisualSmall, &app_name);
                    #[cfg(debug_assertions)]
                    finish_memory_journey_skip(
                        state.as_ref(),
                        memory_journey_id.as_deref(),
                        "visual_too_small",
                        false,
                        None,
                    );
                }
                VisualAdmissionOutcome::SkippedNovelty { novelty, threshold } => {
                    emit_capture_quality_signal(
                        state.as_ref(),
                        json!({
                            "timestamp_ms": chrono::Utc::now().timestamp_millis(),
                            "app_name": app_name,
                            "bundle_id": app_context.bundle_id.clone(),
                            "stored_or_skipped": "skipped_visual_low_novelty",
                            "source_kind": source_kind,
                            "visual_admission_novelty": novelty,
                            "visual_admission_threshold": threshold,
                        }),
                    );
                    state
                        .capture_stats
                        .record_skip(crate::SkipReason::VisualNovelty, &app_name);
                    #[cfg(debug_assertions)]
                    finish_memory_journey_skip(
                        state.as_ref(),
                        memory_journey_id.as_deref(),
                        "visual_low_novelty",
                        false,
                        None,
                    );
                }
                VisualAdmissionOutcome::Failed(err) => {
                    if warn_interval_elapsed(
                        last_visual_failure_warn,
                        Instant::now(),
                        VISUAL_FAILURE_WARN_INTERVAL,
                    ) {
                        last_visual_failure_warn = Some(Instant::now());
                        tracing::warn!(
                            "visual-admission: gate failed for {}; low-text frames are being dropped: {}",
                            app_name,
                            err
                        );
                    } else {
                        tracing::debug!("visual-admission: gate failed for {}: {}", app_name, err);
                    }
                    emit_capture_quality_signal(
                        state.as_ref(),
                        json!({
                            "timestamp_ms": chrono::Utc::now().timestamp_millis(),
                            "app_name": app_name,
                            "bundle_id": app_context.bundle_id.clone(),
                            "stored_or_skipped": "skipped_visual_failed",
                            "source_kind": source_kind,
                            "reason": err,
                        }),
                    );
                    state
                        .capture_stats
                        .record_skip(crate::SkipReason::VisualComposeFailed, &app_name);
                    #[cfg(debug_assertions)]
                    finish_memory_journey_skip(
                        state.as_ref(),
                        memory_journey_id.as_deref(),
                        "visual_admission_failed",
                        false,
                        None,
                    );
                }
            }
            tokio::time::sleep(sleep_duration).await;
            continue;
        }
        let noise_score = text_cleanup::estimate_noise_score(&app_name, &text);
        let keep_ratio = if capture_quality.total_lines == 0 {
            0.0
        } else {
            capture_quality.kept_lines as f32 / capture_quality.total_lines as f32
        };
        if capture_quality.avg_line_score < 0.30 || (keep_ratio < 0.12 && text.len() < 220) {
            emit_capture_quality_signal(
                state.as_ref(),
                json!({
                    "timestamp_ms": chrono::Utc::now().timestamp_millis(),
                    "app_name": app_name,
                    "bundle_id": app_context.bundle_id.clone(),
                    "ocr_confidence": observed_confidence,
                    "ocr_block_count": observed_block_count,
                    "clean_text_len": text.len(),
                    "noise_score": noise_score,
                    "low_signal": true,
                    "stored_or_skipped": "skipped_low_signal",
                    "source_kind": source_kind,
                    "quality_stats": {
                        "total_lines": capture_quality.total_lines,
                        "kept_lines": capture_quality.kept_lines,
                        "avg_line_score": capture_quality.avg_line_score,
                        "keep_ratio": keep_ratio
                    }
                }),
            );
            state
                .capture_stats
                .record_skip(crate::SkipReason::LowSignalText, &app_name);
            #[cfg(debug_assertions)]
            finish_memory_journey_skip(
                state.as_ref(),
                memory_journey_id.as_deref(),
                "low_signal_text",
                false,
                None,
            );
            tokio::time::sleep(sleep_duration).await;
            continue;
        }
        if noise_score > config.capture_pipeline.noise_skip_threshold {
            emit_capture_quality_signal(
                state.as_ref(),
                json!({
                    "timestamp_ms": chrono::Utc::now().timestamp_millis(),
                    "app_name": app_name,
                    "bundle_id": app_context.bundle_id.clone(),
                    "ocr_confidence": observed_confidence,
                    "ocr_block_count": observed_block_count,
                    "clean_text_len": text.len(),
                    "noise_score": noise_score,
                    "low_signal": false,
                    "stored_or_skipped": "skipped_noise",
                    "source_kind": source_kind,
                    "quality_stats": {
                        "total_lines": capture_quality.total_lines,
                        "kept_lines": capture_quality.kept_lines,
                        "avg_line_score": capture_quality.avg_line_score
                    }
                }),
            );
            state
                .capture_stats
                .record_skip(crate::SkipReason::Noise, &app_name);
            #[cfg(debug_assertions)]
            finish_memory_journey_skip(
                state.as_ref(),
                memory_journey_id.as_deref(),
                "ocr_noise",
                false,
                None,
            );
            tokio::time::sleep(sleep_duration).await;
            continue;
        }

        // ── Semantic dedup ────────────────────────────────────────────────
        // Hash (app_name, window_title, clean_text). If the hash is
        // identical to the previous frame, the user is staring at the
        // same content (blinking cursor, ticking clock, etc.).  Skip the
        // entire LLM → VLM → embedding pipeline to save CPU/battery.
        {
            let mut h = DefaultHasher::new();
            app_name.hash(&mut h);
            window_title.hash(&mut h);
            text.hash(&mut h);
            let semantic_hash = h.finish();
            let now_ms = chrono::Utc::now().timestamp_millis();
            if semantic_window.should_skip(
                semantic_hash,
                now_ms,
                config.capture_pipeline.semantic_dedup_window_ms,
            ) && !force_capture
            {
                tracing::debug!("Semantic dedup: identical content, skipping pipeline");
                state.frames_dropped.fetch_add(1, Ordering::Relaxed);
                state
                    .capture_stats
                    .record_skip(crate::SkipReason::SemanticDup, &app_name);
                #[cfg(debug_assertions)]
                finish_memory_journey_skip(
                    state.as_ref(),
                    memory_journey_id.as_deref(),
                    "semantic_duplicate",
                    false,
                    None,
                );
                tokio::time::sleep(sleep_duration).await;
                continue;
            }
        }

        // Summarize each persisted memory with the local AI model when available.
        let engine = if let Some(engine) = state.inference_engine() {
            Some(engine)
        } else {
            match state.ensure_inference_engine().await {
                Ok(engine) => engine,
                Err(err) => {
                    tracing::warn!(
                        "Failed to initialize inference engine in capture loop: {}",
                        err
                    );
                    None
                }
            }
        };

        let browser_structured_seed = semantic_page.as_ref().and_then(|page| {
            build_structured_from_browser_semantics(&app_name, &window_title, url.as_deref(), page)
        });

        // ── Pause capture, run all heavy model work serialized ────────────────
        // Acquire the global model pipeline lock for the duration of LLM +
        // text-embedding + CLIP. This guarantees the Metal/CoreML backend
        // sees one tenant at a time — the capture loop, glasses_import IPC,
        // and any other model consumer take turns, which keeps RSS even
        // and prevents the `mtmd eval chunks: -3` failure mode the user
        // reported when multiple model engines hit Metal concurrently.
        let _pipeline_guard = state.model_pipeline_lock.lock().await;

        let structure_started = Instant::now();
        let mut structured_memory = if let Some(engine) = engine.as_ref() {
            let extraction_future =
                engine.extract_structured_memory(&app_name, &window_title, &qwen_cleaned_text);
            #[cfg(debug_assertions)]
            let mut s = if let Some(journey_id) = memory_journey_id.clone() {
                crate::telemetry::llm_trace::with_memory_journey(
                    state.memory_journey.clone(),
                    journey_id,
                    extraction_future,
                )
                .await
            } else {
                extraction_future.await
            };
            #[cfg(not(debug_assertions))]
            let mut s = extraction_future.await;
            if let Some(ref mut extraction) = s {
                if extraction.synthesis_branch.is_empty() {
                    extraction.synthesis_branch = "llm".to_string();
                }
            }
            s
        } else {
            None
        };
        runtime_metrics::since_ms("mem.structure_ms", structure_started);
        if structured_memory.is_none() {
            structured_memory = browser_structured_seed.clone();
        } else if let (Some(existing), Some(seed)) =
            (structured_memory.as_mut(), browser_structured_seed.as_ref())
        {
            if existing.memory_context.trim().is_empty() {
                existing.memory_context = seed.memory_context.clone();
            }
            if existing.topic.trim().is_empty() {
                existing.topic = seed.topic.clone();
            }
            if existing.user_intent.trim().is_empty() {
                existing.user_intent = seed.user_intent.clone();
            }
            if existing.activity_type.trim().is_empty() {
                existing.activity_type = seed.activity_type.clone();
            }
            if existing.entities.is_empty() {
                existing.entities = seed.entities.clone();
            }
            if existing.confidence < 0.55 {
                existing.confidence = existing
                    .confidence
                    .max((seed.confidence * 0.9).clamp(0.0, 1.0));
            }
        }
        let validate_started = Instant::now();
        let (mut extraction_grounding_confidence, mut extraction_issues) =
            if let Some(memory) = structured_memory.as_mut() {
                validate_structured_memory_extraction(
                    memory,
                    &app_name,
                    &window_title,
                    &text,
                    &qwen_cleaned_text,
                )
            } else {
                (0.0, vec!["structured_extraction_unavailable".to_string()])
            };
        runtime_metrics::since_ms("mem.validate_ms", validate_started);
        #[cfg(debug_assertions)]
        if let Some(journey_id) = memory_journey_id.as_deref() {
            let extraction_json = serde_json::to_vec_pretty(&json!({
                "structured_memory": structured_memory,
                "grounding_confidence": extraction_grounding_confidence,
                "issues": extraction_issues,
                "browser_seed_used": browser_structured_seed.is_some(),
            }))
            .unwrap_or_default();
            let _ = state.memory_journey.record_artifact(
                journey_id,
                "extraction",
                "validated-extraction.json",
                &extraction_json,
            );
            let _ = state.memory_journey.record_stage(
                journey_id,
                crate::memory_journey::MemoryJourneyStageRecord {
                    name: "extraction".to_string(),
                    status: crate::memory_journey::MemoryJourneyStageStatus::Observed,
                    observed_at_ms: chrono::Utc::now().timestamp_millis(),
                    duration_ms: Some(structure_started.elapsed().as_millis() as u64),
                    outcome: if structured_memory.is_some() { "parsed" } else { "fallback" }.to_string(),
                    details: json!({
                        "parse_result": if structured_memory.is_some() { "parsed" } else { "unavailable" },
                        "validator_result": if extraction_issues.is_empty() { "ok" } else { "warnings" },
                        "unsupported_field_warnings": extraction_issues,
                        "fallback": structured_memory.is_none(),
                    }),
                    artifact_ids: Vec::new(),
                },
            );
            state.emit_memory_journey_status();
        }
        let mut semantic_fusion_diagnostics = json!({
            "applied": false,
            "reason": null,
            "sources": [],
        });
        let needs_deterministic_fusion = structured_memory.is_none()
            || extraction_grounding_confidence < 0.55
            || extraction_issues
                .iter()
                .any(|issue| issue == "structured_fields_weakly_grounded");
        if needs_deterministic_fusion {
            if let Some(fusion) = build_low_ram_semantic_fusion(
                &app_name,
                &window_title,
                url.as_deref(),
                &text,
                semantic_page.as_ref(),
                &capture_quality,
                source_kind,
            ) {
                semantic_fusion_diagnostics = apply_semantic_fusion(
                    &mut structured_memory,
                    fusion,
                    extraction_grounding_confidence < 0.55,
                );
                let validate_started = Instant::now();
                let validated = if let Some(memory) = structured_memory.as_mut() {
                    validate_structured_memory_extraction(
                        memory,
                        &app_name,
                        &window_title,
                        &text,
                        &qwen_cleaned_text,
                    )
                } else {
                    (0.0, vec!["structured_extraction_unavailable".to_string()])
                };
                runtime_metrics::since_ms("mem.validate_ms", validate_started);
                extraction_grounding_confidence = validated.0.max(extraction_grounding_confidence);
                extraction_issues = validated.1;
            }
        }
        // Count "this memory's structured fields are likely hallucinated"
        // signals. Two or more of these stacked with grounding < 0.5 means
        // the row would land as a Willow/Shottr-style polluted card —
        // skip storage outright so neither retrieval nor the iOS/OpenClaw
        // surfaces see it.
        //
        // `possible_ungrounded_extraction` is intentionally excluded: it fires
        // at grounding < 0.80 which is too broad for a 1B model that often
        // uses semantically-correct but lexically-different wording. Including
        // it caused (grounding < 0.55) to always produce stacked_critical = 2,
        // dropping every frame the LLM touched. The effective gate is now:
        // both weakly-grounded AND an unsupported hallucinated outcome field.
        let stacked_critical_extraction_issues = extraction_issues
            .iter()
            .filter(|issue| {
                matches!(
                    issue.as_str(),
                    "structured_fields_weakly_grounded" | "unsupported_outcome"
                )
            })
            .count();
        if !extraction_issues.is_empty() {
            emit_extraction_quality_anomaly(
                state.as_ref(),
                json!({
                    "timestamp_ms": chrono::Utc::now().timestamp_millis(),
                    "record_id": uuid::Uuid::new_v4().to_string(),
                    "schema_version": 2,
                    "activity_type": structured_memory
                        .as_ref()
                        .map(|m| m.activity_type.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                    "project_present": structured_memory
                        .as_ref()
                        .map(|m| !m.project.trim().is_empty())
                        .unwrap_or(false),
                    "summary_present": structured_memory
                        .as_ref()
                        .map(|m| !m.memory_context.trim().is_empty())
                        .unwrap_or(false),
                    "files_touched_count": structured_memory
                        .as_ref()
                        .map(|m| m.files_touched.len())
                        .unwrap_or(0),
                    "symbols_changed_count": structured_memory
                        .as_ref()
                        .map(|m| m.symbols_changed.len())
                        .unwrap_or(0),
                    "errors_count": structured_memory
                        .as_ref()
                        .map(|m| m.errors.len())
                        .unwrap_or(0),
                    "next_steps_count": structured_memory
                        .as_ref()
                        .map(|m| m.next_steps.len())
                        .unwrap_or(0),
                    "extraction_confidence": structured_memory
                        .as_ref()
                        .map(|m| m.confidence)
                        .unwrap_or(0.0),
                    "parse_failed": structured_memory.is_none(),
                    "grounding_confidence": extraction_grounding_confidence,
                    "context_conflict": structured_memory
                        .as_ref()
                        .map(|m| m.memory_context.trim().is_empty() && !m.topic.trim().is_empty())
                        .unwrap_or(false),
                    "anomaly_labels": extraction_issues.clone(),
                    "app_name": app_name
                }),
            );
        }
        let drop_due_to_stacked_issues =
            stacked_critical_extraction_issues >= 2 && extraction_grounding_confidence < 0.50;
        // Degraded-but-store: if structured extraction failed (parse error,
        // 1B model misshape, missing engine) but the OCR itself is strong
        // — long, decent confidence, and not noisy — we'd rather emit a
        // text-grounded memory tagged as `extraction_parse_failed` than
        // drop it entirely. Without this, every "JSON wasn't a valid
        // StructuredMemoryExtraction" log silently kills an otherwise good
        // frame at the grounding gate. The strict gate still applies to
        // weak OCR so we don't pollute the vault with empty rows.
        let ocr_grounded_enough = structured_memory.is_none()
            && text.len() >= 220
            && observed_confidence >= 0.45
            && noise_score <= config.capture_pipeline.noise_skip_threshold
            && capture_quality.avg_line_score >= 0.30
            && !drop_due_to_stacked_issues;
        // Text-heavy override: when OCR evidence is rich (many chars + blocks,
        // screen-typical confidence), admit the frame even if structured extraction
        // produced a low grounding score. Prevents high-value developer-context
        // frames from being silently dropped when the 1B model mis-shapes JSON.
        let text_heavy_override = should_text_heavy_override(
            text.len(),
            observed_confidence,
            observed_block_count,
            noise_score,
            config.capture_pipeline.noise_skip_threshold,
            drop_due_to_stacked_issues,
        );
        if (extraction_grounding_confidence <= 0.10 && !ocr_grounded_enough && !text_heavy_override)
            || drop_due_to_stacked_issues
        {
            emit_capture_quality_signal(
                state.as_ref(),
                json!({
                    "timestamp_ms": chrono::Utc::now().timestamp_millis(),
                    "app_name": app_name,
                    "bundle_id": app_context.bundle_id.clone(),
                    "domain": url.as_deref().and_then(extract_domain).unwrap_or_default(),
                    "ocr_confidence": observed_confidence,
                    "ocr_block_count": observed_block_count,
                    "clean_text_len": text.len(),
                    "noise_score": noise_score,
                    "low_signal": false,
                    "surface_policy": "normal",
                    "source_kind": source_kind,
                    "stored_or_skipped": if drop_due_to_stacked_issues {
                        "skipped_stacked_extraction_issues"
                    } else {
                        "skipped_grounding_gate"
                    },
                    "grounding_confidence": extraction_grounding_confidence,
                    "stacked_critical_extraction_issues": stacked_critical_extraction_issues,
                    "extraction_issues": extraction_issues,
                    "text_heavy_override": text_heavy_override,
                    "quality_stats": {
                        "total_lines": capture_quality.total_lines,
                        "kept_lines": capture_quality.kept_lines,
                        "low_conf_lines": capture_quality.low_conf_lines,
                        "dropped_noise_lines": capture_quality.dropped_noise_lines,
                        "dropped_low_signal_lines": capture_quality.dropped_low_signal_lines,
                        "avg_line_score": capture_quality.avg_line_score,
                    }
                }),
            );
            state.capture_stats.record_skip(
                if drop_due_to_stacked_issues {
                    crate::SkipReason::StackedExtraction
                } else {
                    crate::SkipReason::Grounding
                },
                &app_name,
            );
            #[cfg(debug_assertions)]
            finish_memory_journey_skip(
                state.as_ref(),
                memory_journey_id.as_deref(),
                if drop_due_to_stacked_issues {
                    "stacked_extraction_issues"
                } else {
                    "grounding_gate"
                },
                false,
                None,
            );
            tokio::time::sleep(sleep_duration).await;
            continue;
        }
        // Reaching here with `ocr_grounded_enough` means we're using the
        // OCR text + window/app context as the durable signal instead of
        // a structured LLM extraction. Downstream code branches on
        // `structured_memory.is_none()` and already has fallbacks for
        // snippet/internal_context, so we just tag the path for telemetry.
        let extraction_parse_failed_degraded = structured_memory.is_none() && ocr_grounded_enough;

        // OCR-narrative branch: structured extraction is the durable
        // signal. The visual-narrative branch (low-OCR frames) ran
        // earlier with its own VLM and never reaches this point, so the
        // historical `vlm_analysis: Option<String> = None` stub used to
        // be dead code and has been removed.
        let (final_snippet, summary_source) = if let Some(ref mem) = structured_memory {
            let candidate = if !mem.memory_context.trim().is_empty() {
                crate::summariser::sentences::first_sentence(&mem.memory_context).to_string()
            } else {
                mem.topic.trim().to_string()
            };
            (
                candidate,
                if source_kind == "browser_semantic" {
                    "browser_semantic".to_string()
                } else {
                    "llm".to_string()
                },
            )
        } else {
            let fallback = text_cleanup::concise_fallback_snippet(&app_name, &window_title, &text);
            // Tag degraded-path stores so retrieval / debug surfaces can
            // tell apart "no LLM available, OCR-only summary" from a
            // generic fallback (no engine, no OCR worth using).
            let source_label = if extraction_parse_failed_degraded {
                "extraction_parse_failed"
            } else {
                "fallback"
            };
            if fallback.is_empty() {
                (
                    text.chars().take(140).collect::<String>(),
                    source_label.to_string(),
                )
            } else {
                (fallback, source_label.to_string())
            }
        };

        let now = Local::now();
        let (display_summary, narration_filtered) = clean_or_fallback_display_summary(
            &final_snippet,
            &window_title,
            url.as_deref(),
            now.timestamp_millis(),
        );
        if narration_filtered {
            tracing::debug!(
                app = %app_name,
                title = %window_title,
                "capture_pipeline:display_summary_narration_filter_hit"
            );
        }
        let internal_context = build_grounded_memory_context(
            structured_memory.as_ref(),
            &app_name,
            &window_title,
            &text,
            &display_summary,
        );

        // Fetch up to 3 recent same-session-or-project records to chain the
        // new durable memory_context to its predecessors. Failures here are
        // non-fatal — capture proceeds with an empty prior chain.
        let prior_chain = {
            let session_id_for_chain = build_session_id(
                &Local::now(),
                &app_name,
                app_context.bundle_id.as_deref(),
                &build_session_key(&app_name, &window_title, url.as_deref()),
            );
            let project_for_chain = structured_memory
                .as_ref()
                .map(|m| m.project.clone())
                .unwrap_or_default();
            match state
                .store
                .list_recent_by_session_or_project(
                    &session_id_for_chain,
                    &project_for_chain,
                    None,
                    3,
                )
                .await
            {
                Ok(rows) => rows,
                Err(err) => {
                    tracing::debug!("memory_context:prior_chain_fetch_skipped err={err}");
                    Vec::new()
                }
            }
        };
        let durable_memory_context = build_durable_memory_context(
            structured_memory.as_ref(),
            &app_name,
            &window_title,
            &text,
            &display_summary,
            app_context.bundle_id.as_deref(),
            url.as_deref(),
            &prior_chain,
            &config.memory_quality,
        );
        let mut reopen_target = build_reopen_target(
            url.as_deref(),
            macos::preferred_reopen_file_path(
                app_context.document_path.as_deref(),
                structured_memory
                    .as_ref()
                    .map(|memory| memory.files_touched.as_slice())
                    .unwrap_or(&[]),
            ),
            app_context.bundle_id.as_deref(),
            &app_name,
            now.timestamp_millis(),
        );
        reopen_target.page =
            crate::memory::reopen::detect_reopen_page(&reopen_target, &window_title, &text);
        if reopen_target.kind == crate::memory::reopen::ReopenKind::BrowserUrl
            && reopen_target.page.is_none()
        {
            reopen_target.text_anchor = crate::memory::reopen::text_anchor_from(
                &text,
                semantic_page
                    .as_ref()
                    .map(|page| page.visible_passage.as_str()),
                &app_name,
            );
        }
        let related_memory_ids_from_chain = prior_chain
            .iter()
            .map(|row| row.id.clone())
            .take(3)
            .collect::<Vec<_>>();

        // Hierarchical linking: set parent_id to the most recent prior memory
        // in the chain. `prior_chain` was already fetched by session OR project,
        // so any first() match implies a real session/project relationship.
        // Additional 1-hour recency gate avoids forced linking across stale chains.
        let parent_id_from_chain: Option<String> = prior_chain
            .first()
            .filter(|prior| {
                let gap_ms = (now.timestamp_millis() - prior.timestamp).max(0);
                gap_ms < 60 * 60 * 1000
            })
            .map(|prior| prior.id.clone());

        let session_key = build_session_key(&app_name, &window_title, url.as_deref());

        let enriched_clean_text = text.clone();
        let lexical_shadow = build_lexical_shadow(
            &window_title,
            &display_summary,
            &enriched_clean_text,
            url.as_deref(),
        );
        let mut embedding_seed = MemoryRecord {
            app_name: app_name.clone(),
            window_title: window_title.clone(),
            clean_text: enriched_clean_text.clone(),
            snippet: display_summary.clone(),
            display_summary: display_summary.clone(),
            summary_source: summary_source.clone(),
            lexical_shadow: lexical_shadow.clone(),
            url: url.clone(),
            source_type: if url.is_some() {
                "browser".to_string()
            } else {
                "screen".to_string()
            },
            topic: structured_memory
                .as_ref()
                .map(|m| {
                    if m.topic.trim().is_empty() {
                        "unknown".to_string()
                    } else {
                        m.topic.clone()
                    }
                })
                .unwrap_or_else(|| "unknown".to_string()),
            workflow: structured_memory
                .as_ref()
                .map(|m| {
                    if m.workflow.trim().is_empty() {
                        "unknown".to_string()
                    } else {
                        m.workflow.clone()
                    }
                })
                .unwrap_or_else(|| "unknown".to_string()),
            user_intent: structured_memory
                .as_ref()
                .map(|m| {
                    if m.source_evidence.is_some() {
                        String::new()
                    } else if m.user_intent.trim().is_empty() {
                        m.activity_type.clone()
                    } else {
                        m.user_intent.clone()
                    }
                })
                .unwrap_or_default(),
            memory_context: durable_memory_context.clone(),
            commands: structured_memory
                .as_ref()
                .map(|m| m.commands.clone())
                .unwrap_or_default(),
            blockers: structured_memory
                .as_ref()
                .map(|m| m.blockers.clone())
                .unwrap_or_default(),
            todos: structured_memory
                .as_ref()
                .map(|m| m.todos.clone())
                .unwrap_or_default(),
            open_questions: structured_memory
                .as_ref()
                .map(|m| m.open_questions.clone())
                .unwrap_or_default(),
            results: structured_memory
                .as_ref()
                .map(|m| m.results.clone())
                .unwrap_or_default(),
            search_aliases: structured_memory
                .as_ref()
                .map(|m| m.search_aliases.clone())
                .unwrap_or_default(),
            activity_type: structured_memory
                .as_ref()
                .map(|m| m.activity_type.clone())
                .unwrap_or_default(),
            files_touched: structured_memory
                .as_ref()
                .map(|m| m.files_touched.clone())
                .unwrap_or_default(),
            symbols_changed: structured_memory
                .as_ref()
                .map(|m| m.symbols_changed.clone())
                .unwrap_or_default(),
            project: structured_memory
                .as_ref()
                .map(|m| m.project.clone())
                .unwrap_or_default(),
            tags: structured_memory
                .as_ref()
                .map(|m| m.tags.clone())
                .unwrap_or_default(),
            entities: structured_memory
                .as_ref()
                .map(|m| m.entities.clone())
                .unwrap_or_default(),
            decisions: structured_memory
                .as_ref()
                .map(|m| m.decisions.clone())
                .unwrap_or_default(),
            errors: structured_memory
                .as_ref()
                .map(|m| m.errors.clone())
                .unwrap_or_default(),
            next_steps: structured_memory
                .as_ref()
                .map(|m| m.next_steps.clone())
                .unwrap_or_default(),
            outcome: structured_memory
                .as_ref()
                .map(|m| m.outcome.clone())
                .unwrap_or_default(),
            extraction_confidence: structured_memory
                .as_ref()
                .map(|m| m.confidence)
                .unwrap_or(0.0),
            synthesis_branch: structured_memory
                .as_ref()
                .map(|m| m.synthesis_branch.clone())
                .unwrap_or_else(|| "fallback".to_string()),
            topic_categories: structured_memory
                .as_ref()
                .map(|m| m.topic_categories.clone())
                .unwrap_or_default(),
            insight_what_happened: structured_memory
                .as_ref()
                .filter(|m| !m.insight_what_happened.is_empty())
                .map(|m| m.insight_what_happened.clone())
                .unwrap_or_default(),
            insight_why_mattered: structured_memory
                .as_ref()
                .filter(|m| !m.insight_why_mattered.is_empty())
                .map(|m| m.insight_why_mattered.clone())
                .unwrap_or_default(),
            ..Default::default()
        };
        crate::memory_insight::derive_insight_for_record(&mut embedding_seed);
        let compose_started = Instant::now();
        let embedding_document =
            compose_memory_embedding_document(&embedding_seed, Some(&config.chunking));
        runtime_metrics::since_ms("mem.compose_ms", compose_started);
        let primary_embed_input = embedding_document.primary_text.clone();
        #[cfg(debug_assertions)]
        if let Some(journey_id) = memory_journey_id.as_deref() {
            let document_json = serde_json::to_vec_pretty(&json!({
                "primary": embedding_document.primary_text,
                "snippet": embedding_document.snippet_text,
                "support": embedding_document.support_texts,
                "chunk_source": embedding_document.chunk_source_text,
                "visual_semantic": embedding_document.visual_semantic_text,
            }))
            .unwrap_or_default();
            let _ = state.memory_journey.record_stage(
                journey_id,
                crate::memory_journey::MemoryJourneyStageRecord {
                    name: "embedding_document".to_string(),
                    status: crate::memory_journey::MemoryJourneyStageStatus::Observed,
                    observed_at_ms: chrono::Utc::now().timestamp_millis(),
                    duration_ms: Some(compose_started.elapsed().as_millis() as u64),
                    outcome: "composed".to_string(),
                    details: json!({
                        "primary_chars": embedding_document.primary_text.chars().count(),
                        "snippet_chars": embedding_document.snippet_text.chars().count(),
                        "support_count": embedding_document.support_texts.len(),
                        "chunk_source_chars": embedding_document.chunk_source_text.chars().count(),
                        "visual_semantic_available": embedding_document.visual_semantic_text.is_some(),
                    }),
                    artifact_ids: Vec::new(),
                },
            );
            let _ = state.memory_journey.record_artifact(
                journey_id,
                "embedding_document",
                "embedding-document.json",
                &document_json,
            );
        }

        let embedding_inputs = embedding_document.text_embedding_inputs();
        let semantic_embeddings_available = semantic_embeddings_enabled(text_embedder.as_ref());
        let embed_start = Instant::now();
        let embedding_vectors = embed_text_inputs_with_memo(
            text_embedder.as_ref(),
            &mut embedding_memo,
            &app_name,
            &window_title,
            &embedding_inputs,
        );
        let embed_latency = embed_start.elapsed();
        runtime_metrics::since_ms("capture.embed_ms", embed_start);
        runtime_metrics::since_ms("mem.embed_ms", embed_start);
        let primary_embedding = embedding_vectors
            .first()
            .cloned()
            .unwrap_or_else(|| vec![0.0; EMBEDDING_DIM]);
        let snippet_embedding = embedding_vectors
            .get(1)
            .cloned()
            .unwrap_or_else(|| vec![0.0; EMBEDDING_DIM]);
        let support_embedding = if embedding_vectors.len() > 2 {
            mean_pool_embeddings(&embedding_vectors[2..])
        } else {
            vec![0.0; EMBEDDING_DIM]
        };
        let text_embedding =
            weighted_primary_embedding(&primary_embedding, &snippet_embedding, &support_embedding);
        *state.last_embedding.write() = if semantic_embeddings_available {
            text_embedding.clone()
        } else {
            Vec::new()
        };
        tracing::info!(
            app = %app_name,
            ocr_ms = ocr_latency.as_millis(),
            embed_ms = embed_latency.as_millis(),
            support_chunks = embedding_document.support_texts.len(),
            semantic_embeddings_available,
            "capture_pipeline:distilled_frame"
        );

        // ── CLIP image embedding ──────────────────────────────────────────────
        // Compute a 512-d L2-normalized image vector from the same screen pixels
        // OCR consumed. Stored alongside text embeddings so future image-to-image
        // retrieval can find visually similar screens. The text pipeline above is
        // the source of truth; on any CLIP failure we fall back to a zero vector
        // so retrieval/storage stay healthy.
        let (image_embedding, clip_embedding_status) = {
            // Last use of the frame bytes: move them into the blocking task
            // instead of cloning a multi-megabyte PNG buffer per frame.
            let bytes = image_data;
            let models_dir = models_dir.clone();
            let clip_start = Instant::now();
            let join = tokio::task::spawn_blocking(move || -> Result<Vec<f32>, String> {
                let dynamic = image::load_from_memory(&bytes)
                    .map_err(|e| format!("decode capture png: {e}"))?;
                embed_imported_image(&dynamic, &models_dir)
            })
            .await;
            match join {
                Ok(Ok(vec)) => {
                    tracing::debug!(
                        app = %app_name,
                        clip_ms = clip_start.elapsed().as_millis(),
                        "capture_pipeline:image_embedding_ok"
                    );
                    (vec, "ok".to_string())
                }
                Ok(Err(e)) => {
                    tracing::warn!(
                        "CLIP image embedding skipped for capture: {e}; storing zero vector"
                    );
                    (
                        vec![0.0f32; DEFAULT_IMAGE_EMBEDDING_DIM],
                        format!("error:{e}"),
                    )
                }
                Err(e) => {
                    tracing::warn!("CLIP image embedding join failed: {e}; storing zero vector");
                    (
                        vec![0.0f32; DEFAULT_IMAGE_EMBEDDING_DIM],
                        format!("join_error:{e}"),
                    )
                }
            }
        };
        #[cfg(debug_assertions)]
        if let Some(journey_id) = memory_journey_id.as_deref() {
            let _ = state.memory_journey.record_vector_contracts(
                journey_id,
                "all-MiniLM-L6-v2",
                (&text_embedding, &embedding_document.primary_text),
                (&snippet_embedding, &embedding_document.snippet_text),
                (
                    &support_embedding,
                    &embedding_document.support_texts.join("\n"),
                ),
                (&image_embedding, "captured_frame"),
                embed_latency.as_millis() as u64,
            );
            state.emit_memory_journey_status();
        }
        let host_supports_qwen_vlm =
            crate::telemetry::system_metrics::host_supports_lightweight_vlm();
        let (vlm_pressure_skip, vlm_pressure_reason) =
            crate::telemetry::system_metrics::pressure_recommends_skipping_heavy_models();
        let text_capture_vlm_route = capture_pixel_vlm_route(
            &config,
            state.app_data_dir.as_path(),
            text.len(),
            observed_confidence,
            observed_block_count,
            clip_embedding_status == "ok",
            false,
            vlm_pressure_skip,
            host_supports_qwen_vlm,
            config.vlm_max_calls_per_minute,
        );

        // Release the model pipeline lock now that LLM + text-embed + CLIP
        // have all completed for this frame. Downstream Focus Mode / storage
        // / graph work doesn't touch the heavy model surfaces.
        drop(_pipeline_guard);

        // ── Focus Mode drift detection ────────────────────────────────────────
        // Mirrors CC's context-similarity approach: embed the focus task once,
        // then compare every incoming capture. 3 consecutive off-task captures
        // surfaces a ProactiveSuggestion that the frontend can toast.
        if semantic_embeddings_available {
            let focus_emb_opt = state.focus_task_embedding.read().clone();
            if let Some(ref focus_emb) = focus_emb_opt {
                let sim = cosine_similarity(&text_embedding, focus_emb);
                if sim < config.capture_pipeline.focus_drift_similarity_threshold {
                    let prev = state
                        .focus_drift_count
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if prev + 1 >= config.capture_pipeline.focus_drift_capture_count {
                        state
                            .focus_drift_count
                            .store(0, std::sync::atomic::Ordering::Relaxed);
                        let task_title = state.focus_task.read().clone().unwrap_or_default();
                        let suggestion = crate::ProactiveSuggestion {
                            memory_id: "focus_drift".to_string(),
                            snippet: format!(
                                "You've been off-task for a while. Your focus: \"{}\"",
                                task_title
                            ),
                            similarity: sim,
                            task_title: Some(task_title),
                        };
                        let _ = state.proactive_tx.send(Some(suggestion));
                    }
                } else {
                    state
                        .focus_drift_count
                        .store(0, std::sync::atomic::Ordering::Relaxed);
                }
            }
        } else {
            state
                .focus_drift_count
                .store(0, std::sync::atomic::Ordering::Relaxed);
        }

        let mut dedup_fingerprint = structured_memory
            .as_ref()
            .map(|m| m.dedup_fingerprint.trim().to_string())
            .unwrap_or_default();
        if !is_supported_dedup_fingerprint(&dedup_fingerprint) {
            dedup_fingerprint = deterministic_dedup_fingerprint(
                &MemoryRecord {
                    app_name: app_name.clone(),
                    window_title: window_title.clone(),
                    project: structured_memory
                        .as_ref()
                        .map(|m| m.project.clone())
                        .unwrap_or_default(),
                    topic: structured_memory
                        .as_ref()
                        .map(|m| m.topic.clone())
                        .unwrap_or_default(),
                    activity_type: structured_memory
                        .as_ref()
                        .map(|m| m.activity_type.clone())
                        .unwrap_or_default(),
                    url: url.clone(),
                    clean_text: enriched_clean_text.clone(),
                    ..Default::default()
                },
                Some(&internal_context),
            );
        }

        let manifest = build_embedding_manifest(
            &embedding_document,
            text_embedding_status(&text_embedding),
            image_embedding_status(&image_embedding),
            VisualSemanticSource::TextCapture,
        );
        let raw_evidence_payload = upsert_embedding_manifest(&json!({
            "timestamp_ms": now.timestamp_millis(),
            "app_name": app_name.clone(),
            "window_title": window_title.clone(),
            "url": url.clone(),
            "source_kind": source_kind,
            "ocr_confidence": observed_confidence,
            "ocr_block_count": observed_block_count,
            "semantic_page": semantic_page.as_ref().map(|page| {
                json!({
                    "title": page.title.chars().take(200).collect::<String>(),
                    "h1": page.h1.chars().take(220).collect::<String>(),
                    "meta_description": page.meta_description.chars().take(280).collect::<String>(),
                    "nav_ratio": page.nav_ratio,
                    "content_signal_score": page.content_signal_score,
                })
            }),
            "ocr_quality": {
                "total_lines": capture_quality.total_lines,
                "kept_lines": capture_quality.kept_lines,
                "low_conf_lines": capture_quality.low_conf_lines,
                "dropped_noise_lines": capture_quality.dropped_noise_lines,
                "dropped_low_signal_lines": capture_quality.dropped_low_signal_lines,
                "avg_line_score": capture_quality.avg_line_score,
            },
            "semantic_layout": semantic_layout_diagnostics(&enriched_clean_text, &capture_quality),
            "semantic_fusion": semantic_fusion_diagnostics.clone(),
            "fusion_sources": semantic_fusion_diagnostics
                .get("sources")
                .cloned()
                .unwrap_or_else(|| json!([])),
            "vlm_route": text_capture_vlm_route.label(),
            "vlm_block_reason": text_capture_vlm_route.fallback_reason(),
            "host_supports_vlm": host_supports_qwen_vlm,
            "pressure_reason": vlm_pressure_reason,
            "clip_embedding_status": clip_embedding_status,
            "text_embedding_dim": EMBEDDING_DIM,
            "image_embedding_dim": DEFAULT_IMAGE_EMBEDDING_DIM,
            "visual_understanding": {
                "status": if clip_embedding_status == "ok" {
                    "clip_image_embedding"
                } else {
                    "zero_vector_fallback"
                },
                "raw_pixels_persisted": false,
            },
            "source_evidence": structured_memory.as_ref().and_then(|m| m.source_evidence.as_ref()),
            "extraction_grounding_confidence": extraction_grounding_confidence,
            "extraction_issues": extraction_issues.clone(),
            "primary_embed_input": primary_embed_input.chars().take(900).collect::<String>(),
            "internal_context": internal_context.chars().take(700).collect::<String>(),
            "clean_text_excerpt": enriched_clean_text.chars().take(700).collect::<String>(),
        })
        .to_string(), &manifest);

        let clean_text_len = enriched_clean_text.len();
        let session_id = build_session_id(
            &now,
            &app_name,
            app_context.bundle_id.as_deref(),
            &session_key,
        );
        let record = MemoryRecord {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: now.timestamp_millis(),
            day_bucket: now.format("%Y-%m-%d").to_string(),
            app_name: app_name.clone(),
            bundle_id: app_context.bundle_id.clone(),
            window_title: window_title.clone(),
            session_id,
            text: String::new(),
            clean_text: enriched_clean_text,
            ocr_confidence: observed_confidence,
            ocr_block_count: observed_block_count as u32,
            snippet: display_summary.clone(),
            display_summary: display_summary.clone(),
            internal_context,
            summary_source,
            noise_score,
            session_key,
            lexical_shadow,
            embedding: text_embedding,
            image_embedding,
            screenshot_path: None,
            url: url.clone(),
            snippet_embedding,
            support_embedding,
            decay_score: 1.0,
            last_accessed_at: 0,
            timestamp_start: now.timestamp_millis(),
            timestamp_end: now.timestamp_millis(),
            source_type: if url.is_some() {
                "browser".to_string()
            } else {
                "screen".to_string()
            },
            topic: structured_memory
                .as_ref()
                .map(|m| {
                    if m.topic.trim().is_empty() {
                        "unknown".to_string()
                    } else {
                        m.topic.clone()
                    }
                })
                .unwrap_or_else(|| "unknown".to_string()),
            workflow: structured_memory
                .as_ref()
                .map(|m| {
                    if m.workflow.trim().is_empty() {
                        "unknown".to_string()
                    } else {
                        m.workflow.clone()
                    }
                })
                .unwrap_or_else(|| "unknown".to_string()),
            user_intent: structured_memory
                .as_ref()
                .map(|m| {
                    if m.source_evidence.is_some() {
                        String::new()
                    } else if m.user_intent.trim().is_empty() {
                        m.activity_type.clone()
                    } else {
                        m.user_intent.clone()
                    }
                })
                .unwrap_or_default(),
            intent_analysis: crate::storage::IntentAnalysis::default(),
            memory_context: durable_memory_context.clone(),
            commands: structured_memory
                .as_ref()
                .map(|m| m.commands.clone())
                .unwrap_or_default(),
            blockers: structured_memory
                .as_ref()
                .map(|m| m.blockers.clone())
                .unwrap_or_default(),
            todos: structured_memory
                .as_ref()
                .map(|m| m.todos.clone())
                .unwrap_or_default(),
            open_questions: structured_memory
                .as_ref()
                .map(|m| m.open_questions.clone())
                .unwrap_or_default(),
            results: structured_memory
                .as_ref()
                .map(|m| m.results.clone())
                .unwrap_or_default(),
            related_tools: Vec::new(),
            related_agents: Vec::new(),
            related_projects: Vec::new(),
            raw_evidence: raw_evidence_payload,
            reopen_kind: reopen_target.kind,
            reopen_url: reopen_target.url,
            reopen_file_path: reopen_target.file_path,
            reopen_app_bundle_id: reopen_target.app_bundle_id,
            reopen_app_name: reopen_target.app_name,
            reopen_app_deep_link: reopen_target.app_deep_link,
            reopen_captured_at_ms: reopen_target.captured_at_ms,
            reopen_confidence: reopen_target.confidence,
            reopen_validation_status: reopen_target.validation_status,
            reopen_page: reopen_target.page,
            reopen_text_anchor: reopen_target.text_anchor,
            search_aliases: structured_memory
                .as_ref()
                .map(|m| m.search_aliases.clone())
                .unwrap_or_default(),
            related_memory_ids: related_memory_ids_from_chain,
            graph_node_ids: Vec::new(),
            graph_edge_ids: Vec::new(),
            project_confidence: 0.0,
            topic_confidence: 0.0,
            workflow_confidence: 0.0,
            project_evidence: Vec::new(),
            related_project_ids: Vec::new(),
            evidence_confidence: observed_confidence,
            confidence_score: 0.0,
            importance_score: 0.0,
            specificity_score: 0.0,
            intent_score: 0.0,
            entity_score: 0.0,
            agent_usefulness_score: 0.0,
            ocr_noise_score: noise_score,
            graph_readiness_score: 0.0,
            retrieval_value_score: 0.0,
            storage_outcome: String::new(),
            quality_gate_reason: String::new(),
            extracted_entities_structured: Vec::new(),
            action_items: Vec::new(),

            // V2 Fields
            schema_version: 2,
            activity_type: structured_memory
                .as_ref()
                .map(|m| m.activity_type.clone())
                .unwrap_or_default(),
            files_touched: structured_memory
                .as_ref()
                .map(|m| m.files_touched.clone())
                .unwrap_or_default(),
            symbols_changed: structured_memory
                .as_ref()
                .map(|m| m.symbols_changed.clone())
                .unwrap_or_default(),
            session_duration_mins: 0,
            project: structured_memory
                .as_ref()
                .map(|m| m.project.clone())
                .unwrap_or_default(),
            tags: structured_memory
                .as_ref()
                .map(|m| m.tags.clone())
                .unwrap_or_default(),
            entities: structured_memory
                .as_ref()
                .map(|m| m.entities.clone())
                .unwrap_or_default(),
            decisions: structured_memory
                .as_ref()
                .map(|m| m.decisions.clone())
                .unwrap_or_default(),
            errors: structured_memory
                .as_ref()
                .map(|m| m.errors.clone())
                .unwrap_or_default(),
            next_steps: structured_memory
                .as_ref()
                .map(|m| m.next_steps.clone())
                .unwrap_or_default(),
            git_stats: structured_memory
                .as_ref()
                .map(|m| crate::storage::schema::GitStats {
                    added: m.git_stats.added,
                    removed: m.git_stats.removed,
                    commits: m.git_stats.commits,
                }),
            outcome: structured_memory
                .as_ref()
                .map(|m| m.outcome.clone())
                .unwrap_or_default(),
            extraction_confidence: structured_memory
                .as_ref()
                .map(|m| m.confidence)
                .unwrap_or(0.0),
            anchor_coverage_score: 0.0,
            content_hash: String::new(),
            dedup_fingerprint: structured_memory
                .as_ref()
                .map(|_| dedup_fingerprint.clone())
                .unwrap_or(dedup_fingerprint),
            embedding_text: primary_embed_input,
            embedding_model: "all-MiniLM-L6-v2".to_string(), // Default assumption, actual model set in pipeline
            embedding_dim: EMBEDDING_DIM as u32,
            enrichment_status: "pending".to_string(),
            reviewed_at_ms: 0,
            reviewer_generation: 0,
            fallback_reason: None,
            raw_screenshot_stored: false,
            is_consolidated: false,
            is_soft_deleted: false,
            parent_id: parent_id_from_chain,
            related_ids: Vec::new(),
            consolidated_from: Vec::new(),
            synthesis_branch: structured_memory
                .as_ref()
                .map(|m| m.synthesis_branch.clone())
                .unwrap_or_else(|| "fallback".to_string()),
            topic_categories: structured_memory
                .as_ref()
                .map(|m| m.topic_categories.clone())
                .unwrap_or_default(),
            insight_what_happened: structured_memory
                .as_ref()
                .filter(|m| !m.insight_what_happened.is_empty())
                .map(|m| m.insight_what_happened.clone())
                .unwrap_or_else(|| embedding_seed.insight_what_happened.clone()),
            insight_why_mattered: structured_memory
                .as_ref()
                .filter(|m| !m.insight_why_mattered.is_empty())
                .map(|m| m.insight_why_mattered.clone())
                .unwrap_or_else(|| embedding_seed.insight_why_mattered.clone()),
            insight_what_changed: embedding_seed.insight_what_changed.clone(),
            insight_context_thread: embedding_seed.insight_context_thread.clone(),
            insight_spans_json: embedding_seed.insight_spans_json.clone(),
            insight_card_confidence: 0.0,
        };
        let incoming_record_id = record.id.clone();
        let batch_size_before = batch.len();
        let merge_started = Instant::now();
        let merge_result = merge_or_append_memory_record(
            state.as_ref(),
            &mut batch,
            &mut continuity_index,
            record.clone(),
            text_embedder.as_ref(),
            engine.as_ref(),
        )
        .await;
        runtime_metrics::since_ms("capture.merge_ms", merge_started);
        let merged_or_new = match merge_result {
            Ok(merged) => {
                let batch_size_after = batch.len();
                if batch_size_after > batch_size_before {
                    batch_outcomes.push(crate::StoreOutcome::OcrPath);
                }
                merged
            }
            Err(err) => {
                tracing::warn!("Memory continuity merge failed for {}: {}", record.id, err);
                batch.push(record.clone());
                batch_outcomes.push(crate::StoreOutcome::OcrPath);
                record
            }
        };
        #[cfg(debug_assertions)]
        if let Some(journey_id) = memory_journey_id.as_deref() {
            if let Err(error) = state
                .memory_journey
                .attach_memory_id(journey_id, &merged_or_new.id)
            {
                tracing::debug!("Could not attach Memory Journey storage candidate: {error}");
            }
            state.emit_memory_journey_status();
        }
        if merged_or_new.id != incoming_record_id {
            emit_extraction_quality_anomaly(
                state.as_ref(),
                json!({
                    "timestamp_ms": now.timestamp_millis(),
                    "app_name": app_name,
                    "schema_version": 2,
                    "record_id": incoming_record_id,
                    "merged_into_id": merged_or_new.id,
                    "grounding_confidence": extraction_grounding_confidence,
                    "source_kind": source_kind,
                    "anomaly_labels": ["merge_audit_event"],
                }),
            );
        }
        // Fire-and-forget: auto-link to a task cluster based on embedding similarity.
        if semantic_embeddings_available {
            let record_clone = merged_or_new.clone();
            let cluster_store = state.store.clone();
            tauri::async_runtime::spawn(async move {
                let graph = crate::graph::GraphStore::new(cluster_store);
                if let Err(e) = graph.auto_link_to_task(&record_clone).await {
                    tracing::debug!("Auto task link: {e}");
                }
            });
        }

        // A capture merged into an earlier memory was already asked once.
        let is_new_memory = merged_or_new.id == incoming_record_id;
        if let Err(err) = maybe_create_tasks_from_memory(
            state.as_ref(),
            &merged_or_new,
            engine.as_ref(),
            text_embedder.as_ref(),
            is_new_memory,
        )
        .await
        {
            tracing::debug!("Auto task extraction skipped: {}", err);
        }

        emit_capture_quality_signal(
            state.as_ref(),
            json!({
                "timestamp_ms": now.timestamp_millis(),
                "app_name": app_name,
                "bundle_id": app_context.bundle_id.clone(),
                "domain": url.as_deref().and_then(extract_domain).unwrap_or_default(),
                "window_title_hash": compute_window_title_hash(url.as_deref(), &window_title, now.timestamp_millis()),
                "ocr_confidence": observed_confidence,
                "ocr_block_count": observed_block_count,
                "clean_text_len": clean_text_len,
                "noise_score": noise_score,
                "low_signal": false,
                "stored_or_skipped": "stored_candidate",
                "source_kind": source_kind,
                "grounding_confidence": extraction_grounding_confidence,
                "semantic_nav_ratio": semantic_page.as_ref().map(|page| page.nav_ratio).unwrap_or(0.0),
                "semantic_content_score": semantic_page.as_ref().map(|page| page.content_signal_score).unwrap_or(0.0),
                "quality_stats": {
                    "total_lines": capture_quality.total_lines,
                    "kept_lines": capture_quality.kept_lines,
                    "low_conf_lines": capture_quality.low_conf_lines,
                    "dropped_noise_lines": capture_quality.dropped_noise_lines,
                    "dropped_low_signal_lines": capture_quality.dropped_low_signal_lines,
                    "avg_line_score": capture_quality.avg_line_score,
                }
            }),
        );

        state.frames_captured.fetch_add(1, Ordering::Relaxed);
        if extraction_parse_failed_degraded {
            tracing::debug!(
                app = %app_name,
                "stored OCR-grounded memory via degraded path (LLM structured extraction unavailable)"
            );
        }
        state
            .last_capture_time
            .store(now.timestamp_millis() as u64, Ordering::Relaxed);

        tokio::time::sleep(sleep_duration).await;
    }
}

fn embed_text_inputs_with_memo(
    text_embedder: Option<&Embedder>,
    memo: &mut EmbeddingMemo,
    app_name: &str,
    window_title: &str,
    texts: &[String],
) -> Vec<Vec<f32>> {
    if !semantic_embeddings_enabled(text_embedder) {
        return vec![vec![0.0; EMBEDDING_DIM]; texts.len()];
    }

    let Some(text_embedder) = text_embedder else {
        return vec![vec![0.0; EMBEDDING_DIM]; texts.len()];
    };

    let mut out: Vec<Option<Vec<f32>>> = vec![None; texts.len()];
    let mut missing = Vec::new();
    let mut missing_positions = Vec::new();
    let mut missing_dedup: HashMap<String, usize> = HashMap::new();
    let mut structured_keys: HashSet<String> = HashSet::new();
    let app_key = app_name.trim().to_lowercase();
    let title_key = window_title.trim().to_lowercase();

    for (idx, text) in texts.iter().enumerate() {
        let text_key = text.trim().to_string();
        if text_key.is_empty() {
            out[idx] = Some(vec![0.0; EMBEDDING_DIM]);
            continue;
        }
        let structured = idx < 2;
        let key = if structured {
            format!("structured|||{text_key}")
        } else {
            format!("{app_key}|||{title_key}|||{text_key}")
        };
        if structured {
            structured_keys.insert(key.clone());
        }

        if let Some(cached) = memo.get(&key) {
            out[idx] = Some(cached);
            continue;
        }

        if let Some(unique_idx) = missing_dedup.get(&key).copied() {
            missing_positions.push((idx, unique_idx));
            continue;
        }

        let unique_idx = missing.len();
        missing_dedup.insert(key.clone(), unique_idx);
        missing_positions.push((idx, unique_idx));
        missing.push((key, text_key));
    }

    if !missing.is_empty() {
        // The first two inputs are the composed primary and snippet texts.
        // They are embedded plain: the screen-text chunker (title line and
        // screen-noise cleanup) is for OCR and made memories from one app look
        // alike (docs/evidence/W04/vs-85-known-item-search.md). Support
        // texts are screen text and keep their app and window context.
        let contextual_inputs = missing
            .iter()
            .map(|(key, text)| {
                if structured_keys.contains(key) {
                    (String::new(), String::new(), text.clone())
                } else {
                    (app_name.to_string(), window_title.to_string(), text.clone())
                }
            })
            .collect::<Vec<_>>();
        if let Ok(vectors) = text_embedder.embed_batch_with_context(&contextual_inputs) {
            for ((memo_key, _), vector) in missing.iter().cloned().zip(vectors.iter().cloned()) {
                memo.insert(memo_key, vector);
            }
            for (idx, unique_idx) in missing_positions {
                out[idx] = Some(
                    vectors
                        .get(unique_idx)
                        .cloned()
                        .unwrap_or_else(|| vec![0.0; EMBEDDING_DIM]),
                );
            }
        }
    }

    out.into_iter()
        .map(|maybe| maybe.unwrap_or_else(|| vec![0.0; EMBEDDING_DIM]))
        .collect()
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MergeScore {
    pub score: f32,
    pub lexical: f32,
    pub vector: f32,
    pub anchor_match: bool,
}

async fn merge_or_append_memory_record(
    state: &AppState,
    batch: &mut Vec<MemoryRecord>,
    continuity_index: &mut HashMap<String, String>,
    incoming: MemoryRecord,
    text_embedder: Option<&Embedder>,
    engine: Option<&Arc<crate::inference::InferenceEngine>>,
) -> Result<MemoryRecord, String> {
    if !eligible_for_story_merge(&incoming) {
        if let Some(anchor) = continuity_anchor_for_memory(&incoming) {
            continuity_index.insert(anchor, incoming.id.clone());
        }
        batch.push(incoming.clone());
        runtime_metrics::bump("mem.outcome.new");
        return Ok(incoming);
    }

    let incoming_anchor = continuity_anchor_for_memory(&incoming);
    let incoming_id = incoming.id.clone();
    let semantic_merge_enabled = semantic_embeddings_enabled(text_embedder);

    if let Some(anchor) = incoming_anchor.as_ref() {
        if let Some(anchor_id) = continuity_index.get(anchor).cloned() {
            if let Some(batch_idx) = batch
                .iter()
                .position(|record| record.id == anchor_id && !record.is_agent_note())
            {
                let merge_write_started = Instant::now();
                let merged = merge_memory_records(
                    batch[batch_idx].clone(),
                    incoming.clone(),
                    text_embedder,
                    engine,
                )
                .await;
                tracing::info!(
                    "Merged memory {} into in-flight continuity card {} via anchor {}",
                    incoming.id,
                    merged.id,
                    anchor
                );
                if merged.screenshot_path != incoming.screenshot_path {
                    cleanup_screenshot_path(incoming.screenshot_path.clone());
                }
                batch[batch_idx] = merged.clone();
                continuity_index.insert(anchor.clone(), merged.id.clone());
                runtime_metrics::since_ms("mem.merge_write_ms", merge_write_started);
                runtime_metrics::bump("mem.outcome.merged_batch");
                return Ok(merged);
            }

            let merge_search_started = Instant::now();
            let existing = state
                .store
                .get_memory_by_id(&anchor_id)
                .await
                .map_err(|e| e.to_string())?;
            runtime_metrics::since_ms("mem.merge_search_ms", merge_search_started);
            if let Some(existing) = existing.filter(|record| !record.is_agent_note()) {
                let merge_write_started = Instant::now();
                let merged =
                    merge_memory_records(existing.clone(), incoming.clone(), text_embedder, engine)
                        .await;
                tracing::info!(
                    "Merged memory {} into persisted continuity card {} via anchor {}",
                    incoming.id,
                    merged.id,
                    anchor
                );
                state
                    .store
                    .delete_memory_by_id(&existing.id)
                    .await
                    .map_err(|e| e.to_string())?;
                state.invalidate_memory_derived_caches();
                state
                    .store
                    .add_batch(&[merged.clone()])
                    .await
                    .map_err(|e| e.to_string())?;
                if let Err(err) =
                    context_runtime::sync_memory_record(state, &merged, Some("screen")).await
                {
                    tracing::warn!("Context runtime merge sync failed: {}", err);
                }
                state.invalidate_memory_derived_caches();
                if merged.screenshot_path != incoming.screenshot_path {
                    cleanup_screenshot_path(incoming.screenshot_path.clone());
                }
                continuity_index.insert(anchor.clone(), merged.id.clone());
                runtime_metrics::since_ms("mem.merge_write_ms", merge_write_started);
                runtime_metrics::bump("mem.outcome.merged_persisted");
                return Ok(merged);
            }
        }
    }

    if semantic_merge_enabled {
        let merge_search_started = Instant::now();
        let batch_target = best_batch_merge_target(batch, &incoming);
        runtime_metrics::since_ms("mem.merge_search_ms", merge_search_started);
        if let Some(batch_idx) = batch_target {
            let merge_write_started = Instant::now();
            let merged = merge_memory_records(
                batch[batch_idx].clone(),
                incoming.clone(),
                text_embedder,
                engine,
            )
            .await;
            tracing::info!(
                "Merged memory {} into in-flight continuity card {} via similarity score",
                incoming.id,
                merged.id
            );
            if merged.screenshot_path != incoming.screenshot_path {
                cleanup_screenshot_path(incoming.screenshot_path.clone());
            }
            batch[batch_idx] = merged.clone();
            if let Some(anchor) = incoming_anchor.as_ref() {
                continuity_index.insert(anchor.clone(), merged.id.clone());
            }
            runtime_metrics::since_ms("mem.merge_write_ms", merge_write_started);
            runtime_metrics::bump("mem.outcome.merged_batch");
            return Ok(merged);
        }

        let merge_search_started = Instant::now();
        let persisted_target = best_persisted_merge_target(state, &incoming).await?;
        runtime_metrics::since_ms("mem.merge_search_ms", merge_search_started);
        if let Some(existing) = persisted_target {
            let merge_write_started = Instant::now();
            let merged =
                merge_memory_records(existing.clone(), incoming.clone(), text_embedder, engine)
                    .await;
            tracing::info!(
                "Merged memory {} into persisted continuity card {} via similarity score",
                incoming.id,
                merged.id
            );
            state
                .store
                .delete_memory_by_id(&existing.id)
                .await
                .map_err(|e| e.to_string())?;
            state.invalidate_memory_derived_caches();
            state
                .store
                .add_batch(&[merged.clone()])
                .await
                .map_err(|e| e.to_string())?;
            if let Err(err) =
                context_runtime::sync_memory_record(state, &merged, Some("screen")).await
            {
                tracing::warn!("Context runtime merge sync failed: {}", err);
            }
            state.invalidate_memory_derived_caches();
            if merged.screenshot_path != incoming.screenshot_path {
                cleanup_screenshot_path(incoming.screenshot_path.clone());
            }
            if let Some(anchor) = continuity_anchor_for_memory(&merged) {
                continuity_index.insert(anchor, merged.id.clone());
            }
            runtime_metrics::since_ms("mem.merge_write_ms", merge_write_started);
            runtime_metrics::bump("mem.outcome.merged_persisted");
            return Ok(merged);
        }
    } else {
        let merge_search_started = Instant::now();
        let batch_target = best_batch_lexical_merge_target(batch, &incoming);
        runtime_metrics::since_ms("mem.merge_search_ms", merge_search_started);
        if let Some(batch_idx) = batch_target {
            let merge_write_started = Instant::now();
            let merged = merge_memory_records(
                batch[batch_idx].clone(),
                incoming.clone(),
                text_embedder,
                engine,
            )
            .await;
            tracing::info!(
                "Merged memory {} into in-flight continuity card {} via lexical fallback",
                incoming.id,
                merged.id
            );
            if merged.screenshot_path != incoming.screenshot_path {
                cleanup_screenshot_path(incoming.screenshot_path.clone());
            }
            batch[batch_idx] = merged.clone();
            if let Some(anchor) = incoming_anchor.as_ref() {
                continuity_index.insert(anchor.clone(), merged.id.clone());
            }
            runtime_metrics::since_ms("mem.merge_write_ms", merge_write_started);
            runtime_metrics::bump("mem.outcome.merged_batch");
            return Ok(merged);
        }

        let merge_search_started = Instant::now();
        let persisted_target = best_persisted_lexical_merge_target(state, &incoming).await?;
        runtime_metrics::since_ms("mem.merge_search_ms", merge_search_started);
        if let Some(existing) = persisted_target {
            let merge_write_started = Instant::now();
            let merged =
                merge_memory_records(existing.clone(), incoming.clone(), text_embedder, engine)
                    .await;
            tracing::info!(
                "Merged memory {} into persisted continuity card {} via lexical fallback",
                incoming.id,
                merged.id
            );
            state
                .store
                .delete_memory_by_id(&existing.id)
                .await
                .map_err(|e| e.to_string())?;
            state
                .store
                .add_batch(&[merged.clone()])
                .await
                .map_err(|e| e.to_string())?;
            if let Err(err) =
                context_runtime::sync_memory_record(state, &merged, Some("screen")).await
            {
                tracing::warn!("Context runtime merge sync failed: {}", err);
            }
            if merged.screenshot_path != incoming.screenshot_path {
                cleanup_screenshot_path(incoming.screenshot_path.clone());
            }
            if let Some(anchor) = continuity_anchor_for_memory(&merged) {
                continuity_index.insert(anchor, merged.id.clone());
            }
            runtime_metrics::since_ms("mem.merge_write_ms", merge_write_started);
            runtime_metrics::bump("mem.outcome.merged_persisted");
            return Ok(merged);
        }
    }

    if let Some(anchor) = incoming_anchor {
        continuity_index.insert(anchor, incoming_id);
    }
    batch.push(incoming.clone());
    runtime_metrics::bump("mem.outcome.new");
    Ok(incoming)
}

/// Replays a synthetic capture session through FNDR's in-flight merge path.
///
/// This intentionally uses the same batch and continuity state as the capture
/// loop, while keeping the evaluation deterministic when no embedder is
/// available. It is consumed by the synthetic MEM-04 integration test, not by
/// an IPC command or the application runtime.
pub async fn replay_memory_records(
    state: &AppState,
    records: &[MemoryRecord],
) -> Result<Vec<MemoryRecord>, String> {
    let mut batch = Vec::with_capacity(records.len());
    let mut continuity_index = HashMap::new();
    let mut outcomes = Vec::with_capacity(records.len());

    for record in records {
        outcomes.push(
            merge_or_append_memory_record(
                state,
                &mut batch,
                &mut continuity_index,
                record.clone(),
                None,
                None,
            )
            .await?,
        );
    }

    Ok(outcomes)
}

pub(crate) fn eligible_for_story_merge(record: &MemoryRecord) -> bool {
    !record.is_agent_note()
        && (record.clean_text.trim().len() >= 36 || record.snippet.trim().len() >= 18)
}

fn best_batch_merge_target(batch: &[MemoryRecord], incoming: &MemoryRecord) -> Option<usize> {
    let mut best: Option<(usize, MergeScore)> = None;
    for (index, candidate) in batch.iter().enumerate() {
        if candidate.is_agent_note() {
            continue;
        }
        let scored = score_memory_candidate(incoming, candidate);
        if incoming.app_name != candidate.app_name
            && !allows_cross_app_merge_from_memory(incoming, candidate, scored)
        {
            continue;
        }
        if !passes_merge_threshold(scored) {
            continue;
        }
        if best
            .as_ref()
            .map(|(_, prev)| scored.score > prev.score)
            .unwrap_or(true)
        {
            best = Some((index, scored));
        }
    }

    best.map(|(index, _)| index)
}

fn best_batch_lexical_merge_target(
    batch: &[MemoryRecord],
    incoming: &MemoryRecord,
) -> Option<usize> {
    let mut best: Option<(usize, MergeScore)> = None;
    for (index, candidate) in batch.iter().enumerate() {
        if candidate.is_agent_note() {
            continue;
        }
        if incoming.app_name != candidate.app_name {
            continue;
        }
        if !is_cross_app_merge_window(incoming.timestamp, candidate.timestamp) {
            continue;
        }
        let scored = score_memory_candidate_lexical(incoming, candidate);
        if !passes_lexical_merge_threshold(scored) {
            continue;
        }
        if best
            .as_ref()
            .map(|(_, prev)| scored.score > prev.score)
            .unwrap_or(true)
        {
            best = Some((index, scored));
        }
    }

    best.map(|(index, _)| index)
}

async fn best_persisted_merge_target(
    state: &AppState,
    incoming: &MemoryRecord,
) -> Result<Option<MemoryRecord>, String> {
    let same_app_candidates = state
        .store
        .vector_search(
            &incoming.embedding,
            24,
            Some("7d"),
            Some(&incoming.app_name),
        )
        .await
        .map_err(|e| e.to_string())?;

    let best_same_app = same_app_candidates
        .iter()
        .filter(|candidate| !candidate.is_agent_note())
        .filter(|candidate| candidate.id != incoming.id)
        .filter_map(|candidate| {
            let scored = score_search_candidate(incoming, candidate);
            if !passes_merge_threshold(scored) {
                return None;
            }
            Some((candidate.id.clone(), scored.score))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1));

    if let Some((best_id, _)) = best_same_app {
        return state
            .store
            .get_memory_by_id(&best_id)
            .await
            .map_err(|e| e.to_string());
    }

    let cross_app_candidates = state
        .store
        .vector_search(&incoming.embedding, 32, Some("24h"), None)
        .await
        .map_err(|e| e.to_string())?;

    let best_cross_app = cross_app_candidates
        .iter()
        .filter(|candidate| !candidate.is_agent_note())
        .filter(|candidate| candidate.id != incoming.id)
        .filter(|candidate| candidate.app_name != incoming.app_name)
        .filter_map(|candidate| {
            let scored = score_search_candidate(incoming, candidate);
            if !passes_merge_threshold(scored) {
                return None;
            }
            if !allows_cross_app_merge_from_search(incoming, candidate, scored) {
                return None;
            }
            Some((candidate.id.clone(), scored.score))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1));

    if let Some((best_id, _)) = best_cross_app {
        return state
            .store
            .get_memory_by_id(&best_id)
            .await
            .map_err(|e| e.to_string());
    }
    Ok(None)
}

async fn best_persisted_lexical_merge_target(
    state: &AppState,
    incoming: &MemoryRecord,
) -> Result<Option<MemoryRecord>, String> {
    let query = lexical_merge_query(incoming);
    if query.is_empty() {
        return Ok(None);
    }

    let candidates = state
        .store
        .keyword_search(&query, 36, Some("24h"), Some(&incoming.app_name))
        .await
        .map_err(|e| e.to_string())?;

    let best = candidates
        .iter()
        .filter(|candidate| !candidate.is_agent_note())
        .filter(|candidate| candidate.id != incoming.id)
        .filter_map(|candidate| {
            let scored = score_search_candidate_lexical(incoming, candidate);
            if !passes_lexical_merge_threshold(scored) {
                return None;
            }
            Some((candidate.id.clone(), scored.score))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1));

    if let Some((best_id, _)) = best {
        state
            .store
            .get_memory_by_id(&best_id)
            .await
            .map_err(|e| e.to_string())
    } else {
        Ok(None)
    }
}

pub(crate) async fn merge_memory_records(
    existing: MemoryRecord,
    incoming: MemoryRecord,
    text_embedder: Option<&Embedder>,
    engine: Option<&Arc<crate::inference::InferenceEngine>>,
) -> MemoryRecord {
    merge_memory_records_with_policy(existing, incoming, text_embedder, engine, true, true).await
}

pub(crate) async fn merge_memory_records_with_policy(
    existing: MemoryRecord,
    incoming: MemoryRecord,
    text_embedder: Option<&Embedder>,
    engine: Option<&Arc<crate::inference::InferenceEngine>>,
    recompute_embedding: bool,
    allow_llm_summary: bool,
) -> MemoryRecord {
    let raw_evidence = merge_text_source_evidence(&existing.raw_evidence, &incoming.raw_evidence);
    let source_backed = has_source_evidence(&raw_evidence);
    // Historical model guesses must not become pending work merely because a
    // source-backed observation merges into an older record (in either order).
    let user_intent = if source_backed {
        String::new()
    } else {
        prefer_non_empty(&incoming.user_intent, &existing.user_intent)
    };
    let todos = if source_backed {
        Vec::new()
    } else {
        merge_string_lists(&existing.todos, &incoming.todos)
    };
    let next_steps = if source_backed {
        Vec::new()
    } else {
        merge_string_lists(&existing.next_steps, &incoming.next_steps)
    };
    let merged_clean_text = merge_story_text(&existing.clean_text, &incoming.clean_text, 6400);
    let snippet_fallback = merge_story_text(&existing.snippet, &incoming.snippet, 260);
    let llm_snippet = if allow_llm_summary {
        if let Some(model) = engine {
            let generated = model
                .summarize_memory_node(
                    &incoming.app_name,
                    &incoming.window_title,
                    &merged_clean_text,
                )
                .await;
            if generated.trim().is_empty() {
                None
            } else {
                Some(generated)
            }
        } else {
            None
        }
    } else {
        None
    };

    let merged_snippet = llm_snippet.unwrap_or_else(|| snippet_fallback.clone());
    let merged_summary_source = if snippet_fallback.trim().is_empty() {
        existing.summary_source.clone()
    } else if merged_snippet == snippet_fallback {
        "fallback".to_string()
    } else {
        "llm".to_string()
    };
    let merged_window_title = choose_story_title(&existing.window_title, &incoming.window_title);
    let merged_url = incoming.url.clone().or(existing.url.clone());
    let merged_timestamp = incoming.timestamp.max(existing.timestamp);
    let (merged_display_summary, filtered_narration) = clean_or_fallback_display_summary(
        &merged_snippet,
        &merged_window_title,
        merged_url.as_deref(),
        merged_timestamp,
    );
    if filtered_narration {
        tracing::debug!(
            existing_id = %existing.id,
            incoming_id = %incoming.id,
            "capture_merge:display_summary_narration_filter_hit"
        );
    }
    let merged_internal_context = merge_story_text(
        &prefer_non_empty(&existing.internal_context, &existing.clean_text),
        &prefer_non_empty(&incoming.internal_context, &incoming.clean_text),
        6400,
    );
    let merged_lexical_shadow = build_lexical_shadow(
        &merged_window_title,
        &merged_display_summary,
        &format!(
            "{}\n{}\n{}",
            merged_clean_text, existing.lexical_shadow, incoming.lexical_shadow
        ),
        merged_url.as_deref(),
    );
    let mut merge_embedding_seed = MemoryRecord {
        app_name: incoming.app_name.clone(),
        window_title: merged_window_title.clone(),
        clean_text: merged_clean_text.clone(),
        snippet: merged_display_summary.clone(),
        display_summary: merged_display_summary.clone(),
        internal_context: merged_internal_context.clone(),
        summary_source: merged_summary_source.clone(),
        lexical_shadow: merged_lexical_shadow.clone(),
        url: merged_url.clone(),
        source_type: prefer_non_empty(&incoming.source_type, &existing.source_type),
        topic: prefer_non_empty(&incoming.topic, &existing.topic),
        workflow: prefer_non_empty(&incoming.workflow, &existing.workflow),
        user_intent: user_intent.clone(),
        memory_context: prefer_non_empty(&incoming.memory_context, &existing.memory_context),
        commands: merge_string_lists(&existing.commands, &incoming.commands),
        blockers: merge_string_lists(&existing.blockers, &incoming.blockers),
        todos: todos.clone(),
        open_questions: merge_string_lists(&existing.open_questions, &incoming.open_questions),
        results: merge_string_lists(&existing.results, &incoming.results),
        search_aliases: merge_string_lists(&existing.search_aliases, &incoming.search_aliases),
        activity_type: prefer_non_empty(&incoming.activity_type, &existing.activity_type),
        files_touched: merge_string_lists(&existing.files_touched, &incoming.files_touched),
        symbols_changed: merge_string_lists(&existing.symbols_changed, &incoming.symbols_changed),
        project: prefer_non_empty(&incoming.project, &existing.project),
        tags: merge_string_lists(&existing.tags, &incoming.tags),
        entities: merge_string_lists(&existing.entities, &incoming.entities),
        decisions: merge_string_lists(&existing.decisions, &incoming.decisions),
        errors: merge_string_lists(&existing.errors, &incoming.errors),
        next_steps: next_steps.clone(),
        outcome: prefer_non_empty(&incoming.outcome, &existing.outcome),
        extraction_confidence: existing
            .extraction_confidence
            .max(incoming.extraction_confidence),
        synthesis_branch: prefer_non_empty(&incoming.synthesis_branch, &existing.synthesis_branch),
        topic_categories: merge_string_lists(
            &existing.topic_categories,
            &incoming.topic_categories,
        ),
        insight_what_happened: prefer_non_empty(
            &incoming.insight_what_happened,
            &existing.insight_what_happened,
        ),
        insight_why_mattered: prefer_non_empty(
            &incoming.insight_why_mattered,
            &existing.insight_why_mattered,
        ),
        insight_what_changed: prefer_non_empty(
            &incoming.insight_what_changed,
            &existing.insight_what_changed,
        ),
        insight_context_thread: prefer_non_empty(
            &incoming.insight_context_thread,
            &existing.insight_context_thread,
        ),
        insight_spans_json: prefer_non_empty(
            &incoming.insight_spans_json,
            &existing.insight_spans_json,
        ),
        insight_card_confidence: existing
            .insight_card_confidence
            .max(incoming.insight_card_confidence),
        ..Default::default()
    };
    crate::memory_insight::derive_insight_for_record(&mut merge_embedding_seed);
    let merge_embedding_document = compose_memory_embedding_document(&merge_embedding_seed, None);

    let merged_embedding = if recompute_embedding && semantic_embeddings_enabled(text_embedder) {
        text_embedder
            .and_then(|embedder| {
                embedder
                    .embed_batch_with_context(&[(
                        incoming.app_name.clone(),
                        merged_window_title.clone(),
                        merge_embedding_document.primary_text.clone(),
                    )])
                    .ok()
                    .and_then(|mut vectors| vectors.drain(..).next())
            })
            .unwrap_or_else(|| existing.embedding.clone())
    } else {
        existing.embedding.clone()
    };

    let merged_snippet_embedding =
        if recompute_embedding && semantic_embeddings_enabled(text_embedder) {
            text_embedder
                .and_then(|embedder| {
                    embedder
                        .embed_batch_with_context(&[(
                            incoming.app_name.clone(),
                            merged_window_title.clone(),
                            merge_embedding_document.snippet_text.clone(),
                        )])
                        .ok()
                        .and_then(|mut vectors| vectors.drain(..).next())
                })
                .unwrap_or_else(|| existing.snippet_embedding.clone())
        } else {
            existing.snippet_embedding.clone()
        };

    let merged_support_embedding = if recompute_embedding
        && semantic_embeddings_enabled(text_embedder)
        && !merge_embedding_document.support_texts.is_empty()
    {
        let contexts = merge_embedding_document
            .support_texts
            .iter()
            .map(|text| {
                (
                    incoming.app_name.clone(),
                    merged_window_title.clone(),
                    text.clone(),
                )
            })
            .collect::<Vec<_>>();
        text_embedder
            .and_then(|embedder| {
                embedder
                    .embed_batch_with_context(&contexts)
                    .ok()
                    .map(|vectors| mean_pool_embeddings(&vectors))
            })
            .unwrap_or_else(|| existing.support_embedding.clone())
    } else {
        existing.support_embedding.clone()
    };

    let schema_version = existing
        .schema_version
        .max(incoming.schema_version)
        .max(MemoryRecord::default().schema_version);
    let activity_type = prefer_non_empty(&incoming.activity_type, &existing.activity_type);
    let files_touched = merge_string_lists(&existing.files_touched, &incoming.files_touched);
    let symbols_changed = merge_string_lists(&existing.symbols_changed, &incoming.symbols_changed);
    let session_duration_mins = existing
        .session_duration_mins
        .max(incoming.session_duration_mins);
    let project = prefer_non_empty(&incoming.project, &existing.project);
    let tags = merge_string_lists(&existing.tags, &incoming.tags);
    let entities = merge_string_lists(&existing.entities, &incoming.entities);
    let decisions = merge_string_lists(&existing.decisions, &incoming.decisions);
    let errors = merge_string_lists(&existing.errors, &incoming.errors);
    let git_stats = incoming.git_stats.clone().or(existing.git_stats.clone());
    let outcome = prefer_non_empty(&incoming.outcome, &existing.outcome);
    let extraction_confidence = existing
        .extraction_confidence
        .max(incoming.extraction_confidence);
    let dedup_fingerprint =
        prefer_non_empty(&incoming.dedup_fingerprint, &existing.dedup_fingerprint);
    let embedding_text = if recompute_embedding {
        merge_embedding_document.primary_text.clone()
    } else {
        prefer_non_empty(&incoming.embedding_text, &existing.embedding_text)
    };
    let embedding_model = prefer_non_empty(&incoming.embedding_model, &existing.embedding_model);
    let embedding_dim = incoming
        .embedding_dim
        .max(existing.embedding_dim)
        .max(EMBEDDING_DIM as u32);
    let parent_id = incoming.parent_id.clone().or(existing.parent_id.clone());
    let related_ids = merge_string_lists(&existing.related_ids, &incoming.related_ids);
    // The merged record always keeps `existing.id` (see the `id:` field
    // below), so `incoming.id` would otherwise vanish. Track it in
    // consolidated_from so any citation already holding that id can still
    // resolve (MEM-07 invariant 8), via the get_memory_by_id fallback.
    let mut consolidated_from =
        merge_string_lists(&existing.consolidated_from, &incoming.consolidated_from);
    if incoming.id != existing.id && !consolidated_from.contains(&incoming.id) {
        consolidated_from.push(incoming.id.clone());
    }
    let synthesis_branch = prefer_non_empty(&incoming.synthesis_branch, &existing.synthesis_branch);
    let topic_categories =
        merge_string_lists(&existing.topic_categories, &incoming.topic_categories);
    let insight_what_happened = prefer_non_empty(
        &incoming.insight_what_happened,
        &existing.insight_what_happened,
    );
    let insight_why_mattered = prefer_non_empty(
        &incoming.insight_why_mattered,
        &existing.insight_why_mattered,
    );
    let insight_what_changed = prefer_non_empty(
        &incoming.insight_what_changed,
        &existing.insight_what_changed,
    );
    let insight_context_thread = prefer_non_empty(
        &incoming.insight_context_thread,
        &existing.insight_context_thread,
    );
    let insight_spans_json =
        prefer_non_empty(&incoming.insight_spans_json, &existing.insight_spans_json);
    let insight_card_confidence = existing
        .insight_card_confidence
        .max(incoming.insight_card_confidence);
    let raw_evidence = {
        if recompute_embedding {
            let manifest = build_embedding_manifest(
                &merge_embedding_document,
                text_embedding_status(&merged_embedding),
                image_embedding_status(&incoming.image_embedding),
                VisualSemanticSource::TextCapture,
            );
            upsert_embedding_manifest(&raw_evidence, &manifest)
        } else {
            raw_evidence
        }
    };

    MemoryRecord {
        id: existing.id.clone(),
        timestamp: merged_timestamp,
        day_bucket: incoming.day_bucket.clone(),
        app_name: incoming.app_name.clone(),
        bundle_id: incoming.bundle_id.clone().or(existing.bundle_id.clone()),
        window_title: merged_window_title,
        session_id: existing.session_id.clone(),
        text: String::new(),
        clean_text: merged_clean_text,
        ocr_confidence: existing.ocr_confidence.max(incoming.ocr_confidence),
        ocr_block_count: existing.ocr_block_count.max(incoming.ocr_block_count),
        snippet: merged_display_summary.clone(),
        display_summary: merged_display_summary,
        internal_context: merged_internal_context,
        summary_source: merged_summary_source,
        noise_score: ((existing.noise_score + incoming.noise_score) / 2.0).clamp(0.0, 1.0),
        session_key: choose_story_title(&existing.session_key, &incoming.session_key),
        lexical_shadow: merged_lexical_shadow,
        embedding: merged_embedding,
        image_embedding: incoming.image_embedding.clone(),
        screenshot_path: existing
            .screenshot_path
            .clone()
            .or(incoming.screenshot_path.clone()),
        url: merged_url,
        snippet_embedding: merged_snippet_embedding,
        support_embedding: merged_support_embedding,
        decay_score: existing.decay_score.max(incoming.decay_score),
        last_accessed_at: existing.last_accessed_at.max(incoming.last_accessed_at),
        timestamp_start: if existing.timestamp_start > 0 && incoming.timestamp_start > 0 {
            existing.timestamp_start.min(incoming.timestamp_start)
        } else {
            existing.timestamp.min(incoming.timestamp)
        },
        timestamp_end: if existing.timestamp_end > 0 && incoming.timestamp_end > 0 {
            existing.timestamp_end.max(incoming.timestamp_end)
        } else {
            existing.timestamp.max(incoming.timestamp)
        },
        source_type: prefer_non_empty(&incoming.source_type, &existing.source_type),
        topic: prefer_non_empty(&incoming.topic, &existing.topic),
        workflow: prefer_non_empty(&incoming.workflow, &existing.workflow),
        user_intent,
        intent_analysis: if source_backed {
            crate::storage::IntentAnalysis::default()
        } else if incoming.intent_analysis.confidence >= existing.intent_analysis.confidence {
            incoming.intent_analysis.clone()
        } else {
            existing.intent_analysis.clone()
        },
        memory_context: prefer_non_empty(&incoming.memory_context, &existing.memory_context),
        commands: merge_string_lists(&existing.commands, &incoming.commands),
        blockers: merge_string_lists(&existing.blockers, &incoming.blockers),
        todos,
        open_questions: merge_string_lists(&existing.open_questions, &incoming.open_questions),
        results: merge_string_lists(&existing.results, &incoming.results),
        related_tools: merge_string_lists(&existing.related_tools, &incoming.related_tools),
        related_agents: merge_string_lists(&existing.related_agents, &incoming.related_agents),
        related_projects: merge_string_lists(
            &existing.related_projects,
            &incoming.related_projects,
        ),
        raw_evidence,
        reopen_kind: if incoming.reopen_kind != crate::memory::reopen::ReopenKind::Unknown {
            incoming.reopen_kind.clone()
        } else {
            existing.reopen_kind.clone()
        },
        reopen_url: incoming.reopen_url.clone().or(existing.reopen_url.clone()),
        reopen_file_path: incoming
            .reopen_file_path
            .clone()
            .or(existing.reopen_file_path.clone()),
        reopen_app_bundle_id: incoming
            .reopen_app_bundle_id
            .clone()
            .or(existing.reopen_app_bundle_id.clone()),
        reopen_app_name: incoming
            .reopen_app_name
            .clone()
            .or(existing.reopen_app_name.clone()),
        reopen_app_deep_link: incoming
            .reopen_app_deep_link
            .clone()
            .or(existing.reopen_app_deep_link.clone()),
        reopen_captured_at_ms: if incoming.reopen_captured_at_ms > 0 {
            incoming.reopen_captured_at_ms
        } else {
            existing.reopen_captured_at_ms
        },
        reopen_confidence: existing.reopen_confidence.max(incoming.reopen_confidence),
        reopen_validation_status: if incoming.reopen_validation_status
            != crate::memory::reopen::ReopenValidationStatus::Unchecked
        {
            incoming.reopen_validation_status.clone()
        } else {
            existing.reopen_validation_status.clone()
        },
        reopen_page: crate::memory::reopen::merge_reopen_page(
            incoming.reopen_url.as_deref(),
            incoming.reopen_file_path.as_deref(),
            incoming.reopen_page,
            existing.reopen_url.as_deref(),
            existing.reopen_file_path.as_deref(),
            existing.reopen_page,
        ),
        reopen_text_anchor: crate::memory::reopen::merge_reopen_text_anchor(
            incoming.reopen_url.as_deref(),
            incoming.reopen_file_path.as_deref(),
            incoming.reopen_text_anchor.as_deref(),
            existing.reopen_url.as_deref(),
            existing.reopen_file_path.as_deref(),
            existing.reopen_text_anchor.as_deref(),
        ),
        search_aliases: merge_string_lists(&existing.search_aliases, &incoming.search_aliases),
        related_memory_ids: merge_string_lists(
            &existing.related_memory_ids,
            &incoming.related_memory_ids,
        ),
        graph_node_ids: merge_string_lists(&existing.graph_node_ids, &incoming.graph_node_ids),
        graph_edge_ids: merge_string_lists(&existing.graph_edge_ids, &incoming.graph_edge_ids),
        project_confidence: existing.project_confidence.max(incoming.project_confidence),
        topic_confidence: existing.topic_confidence.max(incoming.topic_confidence),
        workflow_confidence: existing
            .workflow_confidence
            .max(incoming.workflow_confidence),
        project_evidence: merge_string_lists(
            &existing.project_evidence,
            &incoming.project_evidence,
        ),
        related_project_ids: merge_string_lists(
            &existing.related_project_ids,
            &incoming.related_project_ids,
        ),
        evidence_confidence: existing
            .evidence_confidence
            .max(incoming.evidence_confidence),
        confidence_score: existing.confidence_score.max(incoming.confidence_score),
        importance_score: existing.importance_score.max(incoming.importance_score),
        specificity_score: existing.specificity_score.max(incoming.specificity_score),
        intent_score: existing.intent_score.max(incoming.intent_score),
        entity_score: existing.entity_score.max(incoming.entity_score),
        agent_usefulness_score: existing
            .agent_usefulness_score
            .max(incoming.agent_usefulness_score),
        ocr_noise_score: existing.ocr_noise_score.max(incoming.ocr_noise_score),
        graph_readiness_score: existing
            .graph_readiness_score
            .max(incoming.graph_readiness_score),
        retrieval_value_score: existing
            .retrieval_value_score
            .max(incoming.retrieval_value_score),
        storage_outcome: prefer_non_empty(&incoming.storage_outcome, &existing.storage_outcome),
        quality_gate_reason: prefer_non_empty(
            &incoming.quality_gate_reason,
            &existing.quality_gate_reason,
        ),
        extracted_entities_structured: if incoming.extracted_entities_structured.len()
            >= existing.extracted_entities_structured.len()
        {
            incoming.extracted_entities_structured.clone()
        } else {
            existing.extracted_entities_structured.clone()
        },
        action_items: if incoming.action_items.len() >= existing.action_items.len() {
            incoming.action_items.clone()
        } else {
            existing.action_items.clone()
        },
        schema_version,
        activity_type,
        files_touched,
        symbols_changed,
        session_duration_mins,
        project,
        tags,
        entities,
        decisions,
        errors,
        next_steps,
        git_stats,
        outcome,
        extraction_confidence,
        anchor_coverage_score: existing
            .anchor_coverage_score
            .max(incoming.anchor_coverage_score),
        content_hash: prefer_non_empty(&incoming.content_hash, &existing.content_hash),
        dedup_fingerprint,
        embedding_text,
        embedding_model,
        embedding_dim,
        enrichment_status: prefer_non_empty(
            &incoming.enrichment_status,
            &existing.enrichment_status,
        ),
        reviewed_at_ms: incoming.reviewed_at_ms.max(existing.reviewed_at_ms),
        reviewer_generation: incoming
            .reviewer_generation
            .max(existing.reviewer_generation),
        fallback_reason: incoming
            .fallback_reason
            .clone()
            .or_else(|| existing.fallback_reason.clone()),
        raw_screenshot_stored: existing.raw_screenshot_stored || incoming.raw_screenshot_stored,
        is_consolidated: existing.is_consolidated || incoming.is_consolidated,
        is_soft_deleted: existing.is_soft_deleted || incoming.is_soft_deleted,
        parent_id,
        related_ids,
        consolidated_from,
        synthesis_branch,
        topic_categories,
        insight_what_happened,
        insight_why_mattered,
        insight_what_changed,
        insight_context_thread,
        insight_spans_json,
        insight_card_confidence,
    }
}

fn prefer_non_empty(incoming: &str, existing: &str) -> String {
    let incoming_trim = incoming.trim();
    if !incoming_trim.is_empty() {
        incoming_trim.to_string()
    } else {
        existing.trim().to_string()
    }
}

fn merge_text_source_evidence(existing: &str, incoming: &str) -> String {
    let preferred = prefer_non_empty(incoming, existing);
    let mut evidence = serde_json::from_str::<serde_json::Value>(&preferred)
        .ok()
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| json!({}));
    let mut kinds = crate::memory_quality::text_source_kinds_from_raw_evidence(existing);
    kinds.extend(crate::memory_quality::text_source_kinds_from_raw_evidence(
        incoming,
    ));
    evidence["source_kind"] = json!(if kinds.len() > 1 {
        "mixed"
    } else {
        kinds.first().copied().unwrap_or("unknown")
    });
    evidence["text_source_kinds"] = json!(kinds);
    // References remain relative to each original snapshot; merging text must
    // never reinterpret an older line number against the concatenated record.
    // Latest metadata wins for a repeated hash, including rejected statements.
    let mut snapshots = source_evidence_sets_from_raw(incoming);
    snapshots.extend(source_evidence_sets_from_raw(existing));
    let mut seen = HashSet::new();
    snapshots.retain(|snapshot| seen.insert(snapshot.source_sha256.clone()));
    snapshots.truncate(4);
    let incoming_raw = serde_json::from_str::<serde_json::Value>(incoming).unwrap_or_default();
    let existing_raw = serde_json::from_str::<serde_json::Value>(existing).unwrap_or_default();
    // Keep an unsupported current contract as current, rather than silently
    // substituting a parsed historical snapshot or reverting to legacy fields.
    let current_marker = [&incoming_raw, &existing_raw]
        .into_iter()
        .find_map(|raw| raw.get("source_evidence").filter(|value| value.is_object()));
    if let Some(current) = current_marker {
        evidence["source_evidence"] = current.clone();
        if let Some(hash) = current
            .get("source_sha256")
            .and_then(|value| value.as_str())
        {
            snapshots.retain(|snapshot| snapshot.source_sha256 != hash);
        }
        snapshots.truncate(3);
        evidence["source_evidence_history"] = json!(snapshots);
    } else if let Some(current) = snapshots.first() {
        evidence["source_evidence"] = json!(current);
        evidence["source_evidence_history"] = json!(&snapshots[1..]);
    } else if let Some(history) = [&incoming_raw, &existing_raw].into_iter().find_map(|raw| {
        raw.get("source_evidence_history")
            .and_then(|value| value.as_array())
            .filter(|values| !values.is_empty())
    }) {
        // Even unreadable historical evidence is a managed-record marker. Do
        // not remove that boundary just because no quotes can be exposed.
        evidence["source_evidence_history"] = json!(history.iter().take(3).collect::<Vec<_>>());
    }
    evidence.to_string()
}

fn merge_string_lists(existing: &[String], incoming: &[String]) -> Vec<String> {
    let mut merged = Vec::with_capacity(existing.len() + incoming.len());
    for value in existing.iter().chain(incoming.iter()) {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !merged
            .iter()
            .any(|item: &String| item.eq_ignore_ascii_case(trimmed))
        {
            merged.push(trimmed.to_string());
        }
    }
    merged
}

fn semantic_embeddings_enabled(text_embedder: Option<&Embedder>) -> bool {
    matches!(
        text_embedder.map(|embedder| embedder.backend()),
        Some(EmbeddingBackend::Real)
    )
}

/// How often the capture loop retries text-embedder initialization while blocked.
const EMBEDDER_INIT_RETRY_INTERVAL: Duration = Duration::from_secs(30);

/// Minimum gap between repeated visual-admission failure warnings; the same
/// failure (e.g. missing CLIP weights) recurs on every low-text frame.
const VISUAL_FAILURE_WARN_INTERVAL: Duration = Duration::from_secs(300);

fn warn_interval_elapsed(last_warn: Option<Instant>, now: Instant, interval: Duration) -> bool {
    last_warn.is_none_or(|last| now.saturating_duration_since(last) >= interval)
}

/// Tick decision for the capture loop when the text embedder may be missing.
///
/// A missing embedder blocks frame processing so zero-vector memory rows never
/// reach storage; once the retry interval elapses (e.g. after the embedding
/// model finishes downloading) the loop attempts re-initialization so capture
/// resumes without an app restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EmbedderGateAction {
    /// Embedder available: process the frame normally.
    Proceed,
    /// Embedder missing and the retry interval elapsed: attempt re-init, then
    /// block if still unavailable.
    RetryInit,
    /// Embedder missing and retry not yet due: block this frame.
    Block,
}

fn embedder_gate_action(
    embedder_present: bool,
    elapsed_since_init_attempt: Duration,
    retry_interval: Duration,
) -> EmbedderGateAction {
    if embedder_present {
        EmbedderGateAction::Proceed
    } else if elapsed_since_init_attempt >= retry_interval {
        EmbedderGateAction::RetryInit
    } else {
        EmbedderGateAction::Block
    }
}

fn choose_story_title(existing: &str, incoming: &str) -> String {
    let existing_trim = existing.trim();
    let incoming_trim = incoming.trim();
    if existing_trim.is_empty() {
        return incoming_trim.to_string();
    }
    if incoming_trim.is_empty() {
        return existing_trim.to_string();
    }
    if incoming_trim.len() > existing_trim.len() {
        incoming_trim.to_string()
    } else {
        existing_trim.to_string()
    }
}

fn merge_story_text(existing: &str, incoming: &str, max_chars: usize) -> String {
    let existing_trim = existing.trim();
    let incoming_trim = incoming.trim();
    if existing_trim.is_empty() {
        return trim_chars(incoming_trim, max_chars);
    }
    if incoming_trim.is_empty() {
        return trim_chars(existing_trim, max_chars);
    }

    let normalized_existing = normalize_text_for_overlap(existing_trim);
    let normalized_incoming = normalize_text_for_overlap(incoming_trim);
    if normalized_existing.contains(&normalized_incoming) {
        return trim_chars(existing_trim, max_chars);
    }
    if normalized_incoming.contains(&normalized_existing) {
        return trim_chars(incoming_trim, max_chars);
    }

    let mut merged = existing_trim.to_string();
    let mut merged_norm = normalized_existing;
    for segment in incoming_trim
        .split(['\n', '.', '!', '?', ';'])
        .map(str::trim)
        .filter(|segment| segment.len() >= 12)
    {
        let normalized_segment = normalize_text_for_overlap(segment);
        if normalized_segment.is_empty() || merged_norm.contains(&normalized_segment) {
            continue;
        }
        merged.push_str(" • ");
        merged.push_str(segment);
        merged_norm.push(' ');
        merged_norm.push_str(&normalized_segment);
        if merged.chars().count() >= max_chars {
            break;
        }
    }
    trim_chars(&merged, max_chars)
}

fn score_search_candidate(incoming: &MemoryRecord, candidate: &SearchResult) -> MergeScore {
    let snippet_similarity = token_overlap(&incoming.snippet, &candidate.snippet);
    let title_similarity = token_overlap(&incoming.window_title, &candidate.window_title);
    let text_similarity = token_overlap(
        &trim_chars(&incoming.clean_text, 1000),
        &trim_chars(&candidate.clean_text, 1000),
    );
    let shadow_similarity = token_overlap(&incoming.lexical_shadow, &candidate.lexical_shadow);
    let lexical = snippet_similarity * 0.42
        + title_similarity * 0.26
        + text_similarity * 0.2
        + shadow_similarity * 0.12;
    let vector = candidate.score.clamp(0.0, 1.0);

    let anchor_match = continuity_anchor_for_memory(incoming)
        .zip(continuity_anchor_for_search_result(candidate))
        .map(|(left, right)| left == right)
        .unwrap_or(false);

    let same_domain = incoming
        .url
        .as_deref()
        .and_then(extract_domain)
        .zip(candidate.url.as_deref().and_then(extract_domain))
        .map(|(left, right)| left == right)
        .unwrap_or(false);

    let mut score = vector * 0.5 + lexical * 0.42;
    if same_domain {
        score += 0.08;
    }
    if anchor_match {
        score += 0.32;
    }

    MergeScore {
        score,
        lexical,
        vector,
        anchor_match,
    }
}

pub(crate) fn score_memory_candidate(
    incoming: &MemoryRecord,
    candidate: &MemoryRecord,
) -> MergeScore {
    let snippet_similarity = token_overlap(&incoming.snippet, &candidate.snippet);
    let title_similarity = token_overlap(&incoming.window_title, &candidate.window_title);
    let text_similarity = token_overlap(
        &trim_chars(&incoming.clean_text, 1000),
        &trim_chars(&candidate.clean_text, 1000),
    );
    let shadow_similarity = token_overlap(&incoming.lexical_shadow, &candidate.lexical_shadow);
    let lexical = snippet_similarity * 0.42
        + title_similarity * 0.26
        + text_similarity * 0.2
        + shadow_similarity * 0.12;
    let vector = cosine_similarity(&incoming.embedding, &candidate.embedding).clamp(0.0, 1.0);

    let anchor_match = continuity_anchor_for_memory(incoming)
        .zip(continuity_anchor_for_memory(candidate))
        .map(|(left, right)| left == right)
        .unwrap_or(false);

    let same_domain = incoming
        .url
        .as_deref()
        .and_then(extract_domain)
        .zip(candidate.url.as_deref().and_then(extract_domain))
        .map(|(left, right)| left == right)
        .unwrap_or(false);

    let mut score = vector * 0.5 + lexical * 0.42;
    if same_domain {
        score += 0.08;
    }
    if anchor_match {
        score += 0.32;
    }

    MergeScore {
        score,
        lexical,
        vector,
        anchor_match,
    }
}

fn score_search_candidate_lexical(incoming: &MemoryRecord, candidate: &SearchResult) -> MergeScore {
    let base = score_search_candidate(incoming, candidate);
    let same_url = matching_effective_url(incoming.url.as_deref(), candidate.url.as_deref());
    let same_domain = same_domain(incoming.url.as_deref(), candidate.url.as_deref());

    let mut score = base.lexical * 0.9;
    if same_domain {
        score += 0.08;
    }
    if same_url {
        score += 0.14;
    }
    if base.anchor_match {
        score += 0.24;
    }

    MergeScore {
        score,
        lexical: base.lexical,
        vector: base.vector,
        anchor_match: base.anchor_match,
    }
}

fn score_memory_candidate_lexical(incoming: &MemoryRecord, candidate: &MemoryRecord) -> MergeScore {
    let base = score_memory_candidate(incoming, candidate);
    let same_url = matching_effective_url(incoming.url.as_deref(), candidate.url.as_deref());
    let same_domain = same_domain(incoming.url.as_deref(), candidate.url.as_deref());

    let mut score = base.lexical * 0.9;
    if same_domain {
        score += 0.08;
    }
    if same_url {
        score += 0.14;
    }
    if base.anchor_match {
        score += 0.24;
    }

    MergeScore {
        score,
        lexical: base.lexical,
        vector: base.vector,
        anchor_match: base.anchor_match,
    }
}

pub(crate) fn passes_merge_threshold(score: MergeScore) -> bool {
    if score.anchor_match {
        return score.score >= 0.58 && score.lexical >= 0.18;
    }
    if score.lexical >= 0.72 && score.score >= 0.80 {
        return true;
    }
    score.score >= 0.86 && score.vector >= 0.82 && score.lexical >= 0.28
}

fn passes_lexical_merge_threshold(score: MergeScore) -> bool {
    if score.anchor_match {
        return score.lexical >= 0.24 && score.score >= 0.46;
    }
    score.lexical >= 0.66 && score.score >= 0.74
}

fn lexical_merge_query(record: &MemoryRecord) -> String {
    let text = format!(
        "{} {} {} {}",
        record.window_title,
        record.snippet,
        trim_chars(&record.clean_text, 500),
        record.lexical_shadow
    );
    text.split_whitespace()
        .take(48)
        .collect::<Vec<_>>()
        .join(" ")
}

fn allows_cross_app_merge_from_memory(
    incoming: &MemoryRecord,
    candidate: &MemoryRecord,
    score: MergeScore,
) -> bool {
    if !is_cross_app_merge_window(incoming.timestamp, candidate.timestamp) {
        return false;
    }
    if matching_effective_url(incoming.url.as_deref(), candidate.url.as_deref()) {
        return true;
    }
    if !same_domain(incoming.url.as_deref(), candidate.url.as_deref()) {
        return false;
    }
    (score.anchor_match && score.lexical >= 0.52) || (score.vector >= 0.93 && score.lexical >= 0.70)
}

fn allows_cross_app_merge_from_search(
    incoming: &MemoryRecord,
    candidate: &SearchResult,
    score: MergeScore,
) -> bool {
    if !is_cross_app_merge_window(incoming.timestamp, candidate.timestamp) {
        return false;
    }
    if matching_effective_url(incoming.url.as_deref(), candidate.url.as_deref()) {
        return true;
    }
    if !same_domain(incoming.url.as_deref(), candidate.url.as_deref()) {
        return false;
    }
    (score.anchor_match && score.lexical >= 0.52) || (score.vector >= 0.93 && score.lexical >= 0.70)
}

fn is_cross_app_merge_window(left_ts: i64, right_ts: i64) -> bool {
    (left_ts - right_ts).abs() <= 45 * 60 * 1000
}

fn matching_effective_url(left: Option<&str>, right: Option<&str>) -> bool {
    let Some(left) = left else {
        return false;
    };
    let Some(right) = right else {
        return false;
    };
    normalize_url_for_merge(left) == normalize_url_for_merge(right)
}

fn normalize_url_for_merge(raw: &str) -> String {
    let lowered = raw.trim().to_lowercase();
    if lowered.is_empty() {
        return String::new();
    }
    let no_scheme = lowered
        .strip_prefix("https://")
        .or_else(|| lowered.strip_prefix("http://"))
        .unwrap_or(&lowered);
    let no_query = no_scheme.split('?').next().unwrap_or(no_scheme);
    let no_fragment = no_query.split('#').next().unwrap_or(no_query);
    no_fragment.trim_end_matches('/').to_string()
}

fn same_domain(left: Option<&str>, right: Option<&str>) -> bool {
    left.and_then(extract_domain)
        .zip(right.and_then(extract_domain))
        .map(|(l, r)| l.eq_ignore_ascii_case(&r))
        .unwrap_or(false)
}

pub(crate) fn continuity_anchor_for_memory(record: &MemoryRecord) -> Option<String> {
    if record.is_agent_note() {
        return None;
    }
    continuity_anchor(
        &record.app_name,
        record.url.as_deref(),
        &record.window_title,
        &record.snippet,
    )
}

fn continuity_anchor_for_search_result(result: &SearchResult) -> Option<String> {
    continuity_anchor(
        &result.app_name,
        result.url.as_deref(),
        &result.window_title,
        &result.snippet,
    )
}

fn continuity_anchor(
    app_name: &str,
    url: Option<&str>,
    window_title: &str,
    snippet: &str,
) -> Option<String> {
    if let Some(raw_url) = url {
        if let Some(domain) = extract_domain(raw_url) {
            let domain_key = domain.to_lowercase();
            let path = extract_first_path_segments(raw_url, 3).unwrap_or_default();
            if !path.is_empty() {
                return Some(format!("url:{domain_key}:{path}"));
            }
            if !domain_key.is_empty() {
                return Some(format!("url:{domain_key}"));
            }
        }
    }

    let app_key = normalize_app_key(app_name);

    let generic_title = normalize_anchor_text(window_title);
    if generic_title.len() >= 8 {
        return Some(format!("app:{app_key}:title:{generic_title}"));
    }

    let generic_snippet = normalize_anchor_text(snippet);
    if generic_snippet.len() >= 10 {
        return Some(format!("app:{app_key}:snippet:{generic_snippet}"));
    }

    None
}

fn extract_first_path_segments(url: &str, count: usize) -> Option<String> {
    let without_scheme = url.split("://").nth(1).unwrap_or(url);
    let mut parts = without_scheme.split('/');
    let _host = parts.next()?;
    let segments: Vec<String> = parts
        .filter(|part| !part.trim().is_empty())
        .map(|part| part.trim().to_lowercase())
        .take(count)
        .collect();
    if segments.is_empty() {
        None
    } else {
        Some(segments.join("/"))
    }
}

fn normalize_app_key(app_name: &str) -> String {
    let normalized = app_name
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    let cleaned = normalized
        .split('_')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

fn normalize_anchor_text(text: &str) -> String {
    text.to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| token.len() > 2)
        .filter(|token| !is_generic_stop_word(token))
        .take(8)
        .collect::<Vec<_>>()
        .join("_")
}

fn token_overlap(left: &str, right: &str) -> f32 {
    let left_tokens = tokenize(left);
    let right_tokens = tokenize(right);
    if left_tokens.is_empty() || right_tokens.is_empty() {
        return 0.0;
    }

    let intersection = left_tokens.intersection(&right_tokens).count() as f32;
    let union = left_tokens.union(&right_tokens).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn tokenize(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| token.len() > 2)
        .filter(|token| !is_generic_stop_word(token))
        .map(|token| token.to_string())
        .collect()
}

fn is_generic_stop_word(token: &str) -> bool {
    matches!(
        token,
        "the"
            | "and"
            | "for"
            | "with"
            | "this"
            | "that"
            | "from"
            | "your"
            | "you"
            | "are"
            | "was"
            | "were"
            | "have"
            | "has"
            | "into"
            | "about"
            | "after"
            | "before"
            | "then"
            | "just"
            | "there"
            | "here"
            | "user"
            | "app"
            | "window"
            | "tab"
            | "page"
            | "open"
            | "opened"
            | "search"
            | "searched"
            | "www"
            | "http"
            | "https"
            | "com"
    )
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let len = left.len().min(right.len());
    if len == 0 {
        return 0.0;
    }

    let mut dot = 0.0;
    let mut left_norm = 0.0;
    let mut right_norm = 0.0;
    for index in 0..len {
        let a = left[index];
        let b = right[index];
        dot += a * b;
        left_norm += a * a;
        right_norm += b * b;
    }

    if left_norm <= f32::EPSILON || right_norm <= f32::EPSILON {
        return 0.0;
    }

    dot / (left_norm.sqrt() * right_norm.sqrt())
}

fn normalize_text_for_overlap(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn trim_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect::<String>()
}

fn cleanup_screenshot_path(path: Option<String>) {
    let Some(path) = path else {
        return;
    };
    let _ = std::fs::remove_file(path);
}

fn purge_capture_artifacts(frames_dir: PathBuf) {
    if frames_dir.exists() {
        if let Err(err) = std::fs::remove_dir_all(&frames_dir) {
            tracing::debug!("Capture artifact purge skipped: {}", err);
            return;
        }
    }
    let _ = std::fs::create_dir_all(frames_dir);
}

/// The screen text a task must be quoted from. Never the model's summary.
const TASK_EVIDENCE_CHARS: usize = 1500;
const MIN_TASK_EVIDENCE_CHARS: usize = 40;
/// How far back, and against how many tasks, a new suggestion is compared.
const TASK_REPEAT_LOOKBACK_MS: i64 = 14 * 24 * 60 * 60 * 1000;
const MAX_TASKS_COMPARED: usize = 200;

fn task_extraction_gate() -> &'static std::sync::Mutex<crate::tasks::suggest::ExtractionGate> {
    static GATE: std::sync::OnceLock<std::sync::Mutex<crate::tasks::suggest::ExtractionGate>> =
        std::sync::OnceLock::new();
    GATE.get_or_init(Default::default)
}

/// Turn checked suggestions into tasks for `record`, leaving out any whose
/// title is already on the list in any state, so a dismissed task does not
/// come back.
fn tasks_from_suggestions(
    suggestions: Vec<crate::tasks::suggest::Suggestion>,
    record: &MemoryRecord,
    existing: &[Task],
) -> Vec<Task> {
    let mut known: HashSet<String> = existing
        .iter()
        .map(|task| crate::tasks::normalize_task_text(&task.title))
        .collect();
    suggestions
        .into_iter()
        .filter(|suggestion| known.insert(crate::tasks::normalize_task_text(&suggestion.title)))
        .map(|suggestion| Task {
            id: uuid::Uuid::new_v4().to_string(),
            title: suggestion.title,
            // The words on screen that state the task, shown as its reason.
            description: suggestion.quote,
            source_app: format!("Memory:{}", record.app_name),
            source_memory_id: Some(record.id.clone()),
            created_at: record.timestamp,
            due_date: None,
            is_completed: false,
            is_dismissed: false,
            task_type: suggestion.task_type,
            linked_urls: record.url.clone().map(|u| vec![u]).unwrap_or_default(),
            linked_memory_ids: vec![record.id.clone()],
        })
        .collect()
}

async fn maybe_create_tasks_from_memory(
    state: &AppState,
    record: &MemoryRecord,
    engine: Option<&Arc<crate::inference::InferenceEngine>>,
    text_embedder: Option<&Embedder>,
    is_new_memory: bool,
) -> Result<(), String> {
    use crate::tasks::suggest::{drop_repeats, is_task_source, parse_suggestions, surface_of};

    let Some(engine) = engine else {
        return Ok(());
    };
    // Ask once per memory, only where a person's own tasks appear, and only
    // when there is enough screen text to quote from.
    if !is_new_memory
        || !record.summary_source.eq_ignore_ascii_case("llm")
        || !is_task_source(&record.app_name, record.url.as_deref())
        || record.clean_text.trim().chars().count() < MIN_TASK_EVIDENCE_CHARS
    {
        return Ok(());
    }
    let admitted = task_extraction_gate()
        .lock()
        .map(|mut gate| gate.admit(&record.app_name, record.timestamp))
        .unwrap_or(false);
    if !admitted {
        return Ok(());
    }

    let evidence = format!(
        "{}\n{}",
        record.window_title,
        record
            .clean_text
            .chars()
            .take(TASK_EVIDENCE_CHARS)
            .collect::<String>()
    );
    let raw = engine.suggest_tasks(&evidence).await;
    let surface = surface_of(&record.app_name, record.url.as_deref());
    let suggestions = parse_suggestions(&raw, &evidence, surface);
    if suggestions.is_empty() {
        return Ok(());
    }

    let mut all_tasks = state.store.list_tasks().await.map_err(|e| e.to_string())?;
    // The model words one task differently each time. Compare by meaning
    // with recent tasks in any state, so a dismissed one stays dismissed.
    let recent_titles: Vec<String> = all_tasks
        .iter()
        .filter(|task| record.timestamp - task.created_at <= TASK_REPEAT_LOOKBACK_MS)
        .rev()
        .take(MAX_TASKS_COMPARED)
        .map(|task| task.title.clone())
        .collect();
    let suggestions = drop_repeats(suggestions, &recent_titles, |texts| {
        text_embedder.and_then(|embedder| embedder.embed_batch(texts).ok())
    });
    let created = tasks_from_suggestions(suggestions, record, &all_tasks);
    if created.is_empty() {
        return Ok(());
    }
    all_tasks.extend(created.iter().cloned());
    state
        .store
        .upsert_tasks(&all_tasks)
        .await
        .map_err(|e| e.to_string())?;

    // Link created tasks into the graph for task-memory navigation.
    for task in &created {
        if let Err(err) = state.graph.link_task(task).await {
            tracing::warn!("Failed linking auto-created task in graph: {}", err);
        }
    }

    Ok(())
}

fn build_session_key(app_name: &str, window_title: &str, url: Option<&str>) -> String {
    let app = app_name.trim().to_lowercase().replace(' ', "_");
    let title = window_title
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| ch.is_alphanumeric() || *ch == ' ')
        .collect::<String>()
        .split_whitespace()
        .take(5)
        .collect::<Vec<_>>()
        .join("_");
    let domain = url
        .and_then(extract_domain)
        .unwrap_or_default()
        .replace('.', "_");

    if !domain.is_empty() {
        format!("{}:{}:{}", app, domain, title)
    } else {
        format!("{}:{}", app, title)
    }
}

fn build_session_id(
    now: &chrono::DateTime<Local>,
    app_name: &str,
    bundle_id: Option<&str>,
    session_key: &str,
) -> String {
    let app = bundle_id
        .unwrap_or(app_name)
        .to_ascii_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let slot = ((now.hour() * 60 + now.minute()) / 30).min(47);
    let session_anchor = session_key
        .split(':')
        .nth(1)
        .unwrap_or("general")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!(
        "{}-{}-{}-s{:02}",
        now.format("%Y%m%d"),
        app,
        session_anchor,
        slot
    )
}

pub(crate) fn extract_domain(url: &str) -> Option<String> {
    let without_scheme = url.split("://").nth(1).unwrap_or(url);
    let host = without_scheme.split('/').next()?.trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

/// Query-parameter and fragment key fragments that commonly carry secrets
/// (OAuth tokens, session ids, API keys, passwords) in a URL.
const URL_CREDENTIAL_KEY_MARKERS: &[&str] = &[
    "token",
    "password",
    "passwd",
    "secret",
    "api_key",
    "apikey",
    "auth",
    "credential",
    "session",
    "jwt",
];

fn strip_url_userinfo(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let (scheme, rest) = url.split_at(scheme_end + 3);
    let Some(at_pos) = rest.find('@') else {
        return url.to_string();
    };
    let slash_pos = rest.find('/');
    if slash_pos.is_some_and(|slash| slash < at_pos) {
        return url.to_string();
    }
    format!("{scheme}{}", &rest[at_pos + 1..])
}

/// Removes any `key=value` pair whose key looks like a credential from a
/// query string or fragment, keeping the rest in order.
fn strip_credential_pairs(query_or_fragment: &str) -> String {
    query_or_fragment
        .split('&')
        .filter(|pair| {
            let key = pair.split('=').next().unwrap_or("").to_ascii_lowercase();
            !URL_CREDENTIAL_KEY_MARKERS
                .iter()
                .any(|marker| key.contains(marker))
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Strips userinfo (`user:pass@host`) and any query-string or fragment
/// parameter whose key looks like a credential, so a captured URL never
/// carries a secret into a stored `MemoryRecord` (MEM-07 invariant 7).
pub(crate) fn strip_url_credentials(url: &str) -> String {
    let url = strip_url_userinfo(url);
    let (before_fragment, fragment) = match url.split_once('#') {
        Some((head, tail)) => (head, Some(strip_credential_pairs(tail))),
        None => (url.as_str(), None),
    };
    let (base, query) = match before_fragment.split_once('?') {
        Some((base, query)) => (base, Some(strip_credential_pairs(query))),
        None => (before_fragment, None),
    };

    let mut result = base.to_string();
    if let Some(query) = query.filter(|q| !q.is_empty()) {
        result.push('?');
        result.push_str(&query);
    }
    if let Some(fragment) = fragment.filter(|f| !f.is_empty()) {
        result.push('#');
        result.push_str(&fragment);
    }
    result
}

/// True when `url` still contains a credential-looking userinfo section or
/// query/fragment parameter. Used by the MEM-07 memory contract to verify
/// `strip_url_credentials` ran before storage.
pub(crate) fn url_has_credential_leak(url: &str) -> bool {
    strip_url_credentials(url) != url
}

/// Returns true when the OCR volume alone justifies admitting the frame,
/// bypassing a low extraction_grounding_confidence score.
///
/// Conditions that still block admission:
/// - `drop_due_to_stacked` is true (≥2 critical extraction failures)
/// - `noise_score` exceeds the pipeline threshold
/// - `text_volume_qualifies` returns false (too short / low confidence)
fn should_text_heavy_override(
    text_len: usize,
    observed_confidence: f32,
    observed_block_count: usize,
    noise_score: f32,
    noise_threshold: f32,
    drop_due_to_stacked: bool,
) -> bool {
    !drop_due_to_stacked
        && noise_score <= noise_threshold
        && crate::ocr::text_volume_qualifies(text_len, observed_confidence, observed_block_count)
}

#[cfg(test)]
mod tests {
    #[test]
    fn accessibility_text_replaces_ocr_only_at_two_hundred_characters() {
        assert!(!super::prefer_ax_text(0));
        assert!(!super::prefer_ax_text(199));
        assert!(super::prefer_ax_text(200));
    }

    use super::*;

    #[test]
    fn dedicated_capture_thread_survives_3mb_stack_use() {
        // Default secondary stacks are ~2MB. This frame depth is ~3MB and must
        // not abort when the capture thread is given CAPTURE_THREAD_STACK_BYTES.
        let name = std::thread::Builder::new()
            .name("fndr-capture".into())
            .stack_size(CAPTURE_THREAD_STACK_BYTES)
            .spawn(|| {
                occupy_stack_frames(48);
                std::thread::current().name().map(str::to_string)
            })
            .expect("spawn capture-sized thread")
            .join()
            .expect("capture-sized thread overflowed or panicked");
        assert_eq!(name.as_deref(), Some("fndr-capture"));
        assert_eq!(CAPTURE_THREAD_STACK_BYTES, 8 * 1024 * 1024);
    }

    #[inline(never)]
    fn occupy_stack_frames(frames: usize) {
        if frames == 0 {
            return;
        }
        let buf = [0u8; 64 * 1024];
        std::hint::black_box(buf[0]);
        occupy_stack_frames(frames - 1);
    }

    #[test]
    fn visual_failure_warning_fires_first_time_then_waits_for_interval() {
        let start = Instant::now();
        let interval = Duration::from_secs(300);
        assert!(warn_interval_elapsed(None, start, interval));
        assert!(!warn_interval_elapsed(
            Some(start),
            start + Duration::from_secs(299),
            interval
        ));
        assert!(warn_interval_elapsed(
            Some(start),
            start + interval,
            interval
        ));
    }

    #[test]
    fn strip_url_credentials_removes_credential_query_params_keeps_others() {
        assert_eq!(
            strip_url_credentials("https://example.com/path?api_key=xyz&foo=bar"),
            "https://example.com/path?foo=bar"
        );
    }

    #[test]
    fn strip_url_credentials_drops_bare_query_string_when_only_credential_params() {
        assert_eq!(
            strip_url_credentials("https://example.com/path?token=abc"),
            "https://example.com/path"
        );
    }

    #[test]
    fn strip_url_credentials_strips_userinfo() {
        assert_eq!(
            strip_url_credentials("https://user:pass@example.com/path"),
            "https://example.com/path"
        );
    }

    #[test]
    fn strip_url_credentials_keeps_at_sign_in_path_untouched() {
        assert_eq!(
            strip_url_credentials("https://example.com/a@b/path"),
            "https://example.com/a@b/path"
        );
    }

    #[test]
    fn strip_url_credentials_strips_credential_fragment() {
        assert_eq!(
            strip_url_credentials("https://example.com/path#access_token=abc"),
            "https://example.com/path"
        );
    }

    #[test]
    fn strip_url_credentials_is_a_no_op_on_a_clean_url() {
        let clean = "https://example.com/path?foo=bar#section";
        assert_eq!(strip_url_credentials(clean), clean);
    }

    #[test]
    fn url_has_credential_leak_detects_unstripped_urls() {
        assert!(url_has_credential_leak(
            "https://example.com/path?password=secret"
        ));
        assert!(!url_has_credential_leak("https://example.com/path?foo=bar"));
    }

    #[test]
    fn screen_guide_generation_suppresses_durable_capture() {
        assert!(!capture_is_suppressed_by_screen_guide(0));
        assert!(capture_is_suppressed_by_screen_guide(1));
        assert!(capture_is_suppressed_by_screen_guide(u64::MAX));
        assert!(!capture_overlapped_screen_guide(4, 4, 4, 0));
        assert!(capture_overlapped_screen_guide(4, 5, 5, 0));
        assert!(capture_overlapped_screen_guide(4, 4, 5, 0));
        assert!(capture_overlapped_screen_guide(4, 4, 4, 12));
    }

    fn merge_test_record(id: &str) -> MemoryRecord {
        MemoryRecord {
            id: id.to_string(),
            timestamp: 1,
            day_bucket: "2026-05-01".to_string(),
            app_name: "Google Chrome".to_string(),
            bundle_id: Some("com.google.Chrome".to_string()),
            window_title: "Hacker News".to_string(),
            session_id: "20260501-com.google.Chrome".to_string(),
            text: String::new(),
            clean_text: "Hacker News page text about AI budgets and developer tooling.".to_string(),
            ocr_confidence: 0.8,
            ocr_block_count: 12,
            snippet: "Browsed Hacker News.".to_string(),
            summary_source: "llm".to_string(),
            noise_score: 0.0,
            session_key: "google_chrome:news.ycombinator.com".to_string(),
            lexical_shadow: "Hacker News AI budgets developer tooling".to_string(),
            embedding: vec![0.1; EMBEDDING_DIM],
            image_embedding: vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM],
            screenshot_path: None,
            url: Some("https://news.ycombinator.com".to_string()),
            snippet_embedding: vec![0.2; EMBEDDING_DIM],
            support_embedding: vec![0.3; EMBEDDING_DIM],
            decay_score: 1.0,
            last_accessed_at: 0,
            ..Default::default()
        }
    }

    #[test]
    fn persisted_continuity_merge_counts_its_outcome() {
        let dir = tempfile::tempdir().expect("temporary store");
        let store = Arc::new(crate::storage::Store::new(dir.path()).expect("store"));
        let state_store =
            Arc::new(crate::storage::StateStore::new(dir.path()).expect("state store"));
        let graph = crate::graph::GraphStore::new(store.clone());
        let state = AppState::new(
            dir.path().to_path_buf(),
            crate::config::Config::default(),
            store.clone(),
            state_store,
            graph,
            None,
        );
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(async {
                let existing = merge_test_record("persisted-existing");
                store
                    .add_batch(&[existing.clone()])
                    .await
                    .expect("seed store");
                let mut incoming = merge_test_record("incoming");
                incoming.timestamp = 2;

                let anchor = continuity_anchor_for_memory(&incoming).expect("continuity anchor");
                let mut continuity_index = HashMap::from([(anchor, existing.id.clone())]);
                let before = runtime_metrics::counter("mem.outcome.merged_persisted");

                let merged = merge_or_append_memory_record(
                    &state,
                    &mut Vec::new(),
                    &mut continuity_index,
                    incoming,
                    None,
                    None,
                )
                .await
                .expect("persisted continuity merge");

                assert_eq!(merged.id, existing.id);
                assert_eq!(
                    runtime_metrics::counter("mem.outcome.merged_persisted"),
                    before + 1
                );
            });
    }

    fn assert_capture_keeps_agent_note_separate(persisted: bool, seed_anchor: bool) {
        let dir = tempfile::tempdir().expect("temporary store");
        let store = Arc::new(crate::storage::Store::new(dir.path()).expect("store"));
        let state_store =
            Arc::new(crate::storage::StateStore::new(dir.path()).expect("state store"));
        let graph = crate::graph::GraphStore::new(store.clone());
        let state = AppState::new(
            dir.path().to_path_buf(),
            crate::config::Config::default(),
            store.clone(),
            state_store,
            graph,
            None,
        );
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime")
            .block_on(async {
                let mut note = merge_test_record("agent-note");
                note.source_type = crate::storage::AGENT_NOTE_SOURCE_TYPE.to_string();
                note.timestamp = chrono::Utc::now().timestamp_millis();
                note.raw_evidence = r#"{"added_by":"test-agent","source_kind":"unknown"}"#.into();
                let mut incoming = merge_test_record("screen-capture");
                incoming.timestamp = note.timestamp + 1;
                incoming.raw_evidence = r#"{"source_kind":"ax"}"#.into();
                let mut batch = Vec::new();
                if persisted {
                    store.add_batch(&[note.clone()]).await.expect("seed note");
                    note = store.get_memory_by_id(&note.id).await.unwrap().unwrap();
                } else {
                    batch.push(note.clone());
                }
                let note_before = serde_json::to_value(&note).unwrap();
                let mut continuity_index = HashMap::new();
                if seed_anchor {
                    // Exercise stale or externally seeded indexes, independently
                    // of whether agent notes now generate anchors themselves.
                    let anchor = continuity_anchor_for_memory(&incoming).expect("screen anchor");
                    continuity_index.insert(anchor, note.id.clone());
                } else if persisted {
                    assert!(best_persisted_merge_target(&state, &incoming)
                        .await
                        .unwrap()
                        .is_none());
                    assert!(best_persisted_lexical_merge_target(&state, &incoming)
                        .await
                        .unwrap()
                        .is_none());
                } else {
                    assert_eq!(best_batch_merge_target(&batch, &incoming), None);
                    assert_eq!(best_batch_lexical_merge_target(&batch, &incoming), None);
                }
                let result = merge_or_append_memory_record(
                    &state,
                    &mut batch,
                    &mut continuity_index,
                    incoming.clone(),
                    None,
                    None,
                )
                .await
                .expect("capture decision");
                assert_eq!(
                    result.id, incoming.id,
                    "screen must stay separate from agent note"
                );
                assert!(!result.is_agent_note());
                if !persisted {
                    assert_eq!(serde_json::to_value(&batch[0]).unwrap(), note_before);
                }
                store
                    .add_batch(&batch)
                    .await
                    .expect("persist separate screen");
                let preserved = store.get_memory_by_id(&note.id).await.unwrap().unwrap();
                if persisted {
                    assert_eq!(serde_json::to_value(&preserved).unwrap(), note_before);
                }
                assert!(preserved.is_agent_note());
                assert!(store
                    .get_memory_by_id(&incoming.id)
                    .await
                    .unwrap()
                    .is_some());
            });
    }

    #[test]
    fn capture_keeps_agent_notes_out_of_batch_merge_candidates() {
        assert_capture_keeps_agent_note_separate(false, false);
    }

    #[test]
    fn capture_keeps_agent_notes_out_of_persisted_merge_candidates() {
        assert_capture_keeps_agent_note_separate(true, false);
    }

    #[test]
    fn capture_keeps_agent_notes_separate_from_batch_continuity_anchor() {
        assert_capture_keeps_agent_note_separate(false, true);
    }

    #[test]
    fn capture_keeps_agent_notes_separate_from_persisted_continuity_anchor() {
        assert_capture_keeps_agent_note_separate(true, true);
    }

    #[test]
    fn committed_capture_fixtures_match_the_pre_frame_privacy_gate() {
        #[derive(serde::Deserialize)]
        struct Fixture {
            id: String,
            app_class: String,
            #[serde(default)]
            app_name: Option<String>,
            #[serde(default)]
            bundle_id: Option<String>,
            window_title: String,
            #[serde(default)]
            url: Option<String>,
            expected_outcome: String,
        }

        let manifest = include_str!("../../tests/fixtures/screens/manifest.json");
        let fixtures: Vec<Fixture> =
            serde_json::from_str(manifest).expect("fixture manifest parses");
        let default_blocklist = crate::config::Config::default().blocklist;
        let mut checked = 0;
        println!("fixture | expected admission | actual admission");

        for fixture in fixtures {
            if fixture.app_class != "privacy_negative" && fixture.expected_outcome != "store" {
                continue;
            }
            let app_name =
                fixture
                    .app_name
                    .as_deref()
                    .unwrap_or_else(|| match fixture.bundle_id.as_deref() {
                        Some("com.microsoft.VSCode") => "Visual Studio Code",
                        Some("com.apple.Terminal") => "Terminal",
                        Some("com.google.Chrome") => "Google Chrome",
                        Some("com.tinyspeck.slackmacgap") => "Slack",
                        Some("com.apple.Preview") => "Preview",
                        _ => "Unknown",
                    });
            let actual = super::capture_admission_skip_reason(
                app_name,
                fixture.bundle_id.as_deref(),
                &fixture.window_title,
                fixture.url.as_deref(),
                None,
                &default_blocklist,
            )
            .map(|reason| format!("skip:{}", reason.as_str()))
            .unwrap_or_else(|| "store".to_string());

            println!("{} | {} | {}", fixture.id, fixture.expected_outcome, actual);
            assert_eq!(
                actual, fixture.expected_outcome,
                "fixture {} must match the real pre-frame gate",
                fixture.id
            );
            checked += 1;
        }

        assert_eq!(
            checked, 30,
            "all committed store and privacy fixtures are checked"
        );
    }

    #[test]
    fn privacy_exclusion_blocks_capture_before_ocr() {
        let blocklist = vec!["1Password".to_string(), "bank.example.com".to_string()];

        assert!(should_skip_capture_context(
            "1Password",
            Some("com.1password.1password"),
            "Vault",
            None,
            &blocklist,
        ));
        assert!(should_skip_capture_context(
            "Chrome",
            Some("com.google.Chrome"),
            "Account overview",
            Some("https://bank.example.com/accounts"),
            &blocklist,
        ));
        assert!(!should_skip_capture_context(
            "Chrome",
            Some("com.google.Chrome"),
            "FNDR architecture notes",
            Some("https://docs.example.com/fndr"),
            &blocklist,
        ));
    }

    #[test]
    fn known_sensitive_metadata_is_blocked_before_ocr_without_user_configuration() {
        let blocklist: Vec<String> = Vec::new();
        let sensitive_contexts = [
            (
                "banking URL",
                "Safari",
                Some("com.apple.Safari"),
                "Account overview",
                Some("https://secure.chase.com/accounts"),
            ),
            (
                "banking title",
                "Safari",
                Some("com.apple.Safari"),
                "Online Banking — Account overview",
                Some("https://accounts.example.com/overview"),
            ),
            (
                "medical title",
                "Safari",
                Some("com.apple.Safari"),
                "MyChart — Test results",
                Some("https://health.example.com/results"),
            ),
            (
                "authentication page",
                "Google Chrome",
                Some("com.google.Chrome"),
                "Sign in",
                Some("https://app.example.com/login"),
            ),
            (
                "password-manager bundle",
                "Vault",
                Some("com.1password.1password"),
                "Personal vault",
                None,
            ),
        ];

        for (label, app_name, bundle_id, window_title, url) in sensitive_contexts {
            assert_eq!(
                capture_context_skip_reason(app_name, bundle_id, window_title, url, &blocklist,),
                Some(crate::SkipReason::SensitiveContext),
                "{label} must fail closed before OCR",
            );
        }
    }

    #[test]
    fn user_blocklist_and_self_app_keep_their_distinct_skip_reasons() {
        assert_eq!(
            capture_context_skip_reason("FNDR", Some("com.fndr.desktop"), "Privacy", None, &[],),
            Some(crate::SkipReason::SelfApp),
        );
        assert_eq!(
            capture_context_skip_reason(
                "1Password",
                Some("com.1password.1password"),
                "Vault",
                None,
                &["1Password".to_string()],
            ),
            Some(crate::SkipReason::Blocklist),
        );
    }

    #[test]
    fn secret_pattern_text_is_rejected_by_capture_admission() {
        assert_eq!(
            capture_admission_skip_reason(
                "Terminal",
                Some("com.apple.Terminal"),
                "Local shell",
                None,
                Some("export API_KEY=do-not-store-this"),
                &[],
            ),
            Some(crate::SkipReason::SensitiveContext),
        );
        assert_eq!(
            capture_admission_skip_reason(
                "Terminal",
                Some("com.apple.Terminal"),
                "Local shell",
                None,
                Some("cargo test --lib capture"),
                &[],
            ),
            None,
        );
    }

    #[test]
    fn self_app_skip_is_distinct_from_user_blocklist() {
        let blocklist: Vec<String> = Vec::new();
        assert_eq!(
            capture_context_skip_reason(
                "FNDR",
                Some("com.fndr.desktop"),
                "Settings",
                None,
                &blocklist,
            ),
            Some(crate::SkipReason::SelfApp)
        );
        assert_eq!(
            capture_context_skip_reason(
                "Finder",
                Some("com.apple.finder"),
                "Desktop",
                None,
                &blocklist,
            ),
            None
        );
    }

    #[test]
    fn surface_policy_skips_known_navigation_results_pages() {
        let policy = classify_capture_surface_policy(
            "Google Chrome",
            "Search results - YouTube",
            Some("https://www.youtube.com/results?search_query=screenpipe"),
        );
        assert_eq!(policy, CaptureSurfacePolicy::SkipFrame);
    }

    #[test]
    fn surface_policy_uses_url_only_for_channel_listing_pages() {
        let policy = classify_capture_surface_policy(
            "Google Chrome",
            "screen_pipe - YouTube",
            Some("https://www.youtube.com/@screen_pipe/videos"),
        );
        assert_eq!(policy, CaptureSurfacePolicy::UrlOnly);
    }

    #[test]
    fn surface_policy_allows_normal_article_capture() {
        let policy = classify_capture_surface_policy(
            "Google Chrome",
            "Screenpipe Architecture Deep Dive",
            Some("https://docs.screenpi.pe/architecture/memory-cards"),
        );
        assert_eq!(policy, CaptureSurfacePolicy::Normal);
    }

    #[test]
    fn session_id_is_sub_day_and_domain_anchored() {
        let now = chrono::TimeZone::with_ymd_and_hms(&Local, 2026, 5, 6, 21, 46, 0)
            .single()
            .expect("local datetime");
        let id = build_session_id(
            &now,
            "Google Chrome",
            Some("com.google.Chrome"),
            "google_chrome:youtube_com:screenpipe",
        );
        assert!(id.starts_with("20260506-com.google.chrome-youtube_com-s"));
    }

    #[test]
    fn semantic_structured_fallback_builds_grounded_fields() {
        let semantic = macos::BrowserSemanticContent {
            title: "Screenpipe architecture deep dive".to_string(),
            meta_description: "A walkthrough of memory card indexing and retrieval.".to_string(),
            h1: "Screenpipe architecture deep dive".to_string(),
            article_excerpt:
                "Screenpipe memory cards connect OCR cleanup, retrieval ranking, and context grounding."
                    .to_string(),
            nav_ratio: 0.08,
            content_signal_score: 0.84,
            ..Default::default()
        };
        let extraction = build_structured_from_browser_semantics(
            "Google Chrome",
            "Screenpipe architecture deep dive",
            Some("https://docs.screenpi.pe/architecture"),
            &semantic,
        )
        .expect("semantic extraction");

        assert!(!extraction.topic.trim().is_empty());
        assert!(!extraction.memory_context.trim().is_empty());
        assert!(extraction.confidence >= 0.35);
        assert!(!extraction.entities.is_empty());
    }

    #[test]
    fn low_ram_semantic_fusion_builds_codex_docs_memory_without_topic_scaffold() {
        let raw = r#"
Push latest changes
README.md
DESIGN_DIRECTION.md
Removed untracked planning/artifact files and folders.
Still planned but not implemented (from current committed docs):
1. Optional Qwen3-VL + mmproj photo-import vision path is documented as optional setup, not baseline behavior yet (README.md:202).
2. Image-aware retrieval is explicitly future (CLIP vector stored now, richer retrieval later) (README.md:129).
3. Future graph enrichment runtime is noted as future/additive in design direction (DESIGN_DIRECTION.md:106).
Future Roadmap
Near Term
Advanced idle detection
Medium Term
Semantic timeline (group by topic, not just time)
Activity patterns and insights dashboard
"#;
        let quality = text_cleanup::CaptureQualityStats {
            total_lines: 51,
            kept_lines: 50,
            low_conf_lines: 51,
            dropped_noise_lines: 0,
            dropped_low_signal_lines: 1,
            avg_line_score: 0.58,
        };

        let fusion = build_low_ram_semantic_fusion(
            "Codex",
            "Push latest changes",
            None,
            raw,
            None,
            &quality,
            "ocr",
        )
        .expect("fusion should build from OCR, file refs, and app/window context");

        assert!(fusion.extraction.memory_context.contains("README.md"));
        assert!(fusion
            .extraction
            .memory_context
            .contains("DESIGN_DIRECTION.md"));
        assert!(fusion
            .extraction
            .user_intent
            .contains("implementation status"));
        assert!(!fusion.extraction.memory_context.starts_with("Topic:"));
        assert!(!fusion
            .extraction
            .search_aliases
            .iter()
            .any(|alias| alias.contains("tspbn")));
        assert!(fusion
            .extraction
            .files_touched
            .iter()
            .any(|file| file == "README.md"));
        assert!(fusion
            .extraction
            .files_touched
            .iter()
            .any(|file| file == "DESIGN_DIRECTION.md"));
    }

    #[test]
    fn lightweight_entities_extracts_named_tokens() {
        let entities = lightweight_entities_from_text(
            "Screenpipe integrates Obsidian, Apple Intelligence, and Toggl reports.",
        );
        assert!(entities
            .iter()
            .any(|e| e.eq_ignore_ascii_case("Screenpipe")));
        assert!(entities.iter().any(|e| e.eq_ignore_ascii_case("Obsidian")));
    }

    fn source_evidence_fixture(index: u8) -> serde_json::Value {
        json!({
            "version": 1,
            "source_sha256": format!("{index:064x}"),
            "statements": [{"kind": "action", "line": 2, "quote": format!("Alex: please review item {index}.")}],
            "issues": []
        })
    }

    #[tokio::test]
    async fn source_backed_merge_does_not_resurrect_legacy_intent_or_actions() {
        for source_first in [false, true] {
            let mut sourced = merge_test_record("sourced");
            sourced.raw_evidence = json!({
                "source_kind": "ax",
                "source_evidence": source_evidence_fixture(1),
            })
            .to_string();
            sourced.user_intent.clear();
            sourced.todos.clear();
            sourced.next_steps.clear();
            let mut legacy = merge_test_record("legacy");
            legacy.user_intent = "Invented launch strategy".into();
            legacy.intent_analysis.intent_label = "Invented launch strategy".into();
            legacy.intent_analysis.confidence = 0.99;
            legacy.todos = vec!["Invented deploy obligation".into()];
            legacy.next_steps = vec!["Invented funding action".into()];
            let (existing, incoming) = if source_first {
                (sourced, legacy)
            } else {
                (legacy, sourced)
            };
            let merged =
                merge_memory_records_with_policy(existing, incoming, None, None, true, false).await;
            assert!(merged.user_intent.is_empty(), "source_first={source_first}");
            assert!(merged.todos.is_empty());
            assert!(merged.next_steps.is_empty());
            assert!(merged.intent_analysis.intent_label.is_empty());
            assert_eq!(merged.intent_analysis.confidence, 0.0);
            assert!(!merged.embedding_text.contains("Invented"));
            let raw: serde_json::Value = serde_json::from_str(&merged.raw_evidence).unwrap();
            assert_eq!(raw["source_evidence"], source_evidence_fixture(1));
            assert!(!merged.internal_context.contains("Next:"));
            assert!(!merged.internal_context.contains("Intent:"));
        }
    }

    #[test]
    fn source_backed_visual_fallback_retains_observation_metadata() {
        let source = "Alex: please review the parser tests.";
        let mut extraction = StructuredMemoryExtraction {
            topic: "Parser tests".into(),
            memory_context: "The parser test discussion is visible.".into(),
            ..Default::default()
        };
        extraction.source_refs.actions = vec![json!(1)];
        crate::inference::extraction_evidence::finalize_extraction(&mut extraction, source);
        let (insight, evidence) = visual_insight_from_structured(&extraction);
        assert_eq!(
            serde_json::to_value(evidence).unwrap(),
            serde_json::to_value(extraction.source_evidence).unwrap()
        );
        assert!(insight.actions.is_empty());
        assert!(!insight.summary_detailed.contains(source));
        assert_eq!(insight.summary_short, "Parser tests");
    }

    #[tokio::test]
    async fn source_backed_merge_keeps_unsupported_contract_protected() {
        for with_history in [false, true] {
            let mut protected = merge_test_record("protected");
            let unsupported = json!({"version": 99, "source_sha256": format!("{:064x}", 7)});
            protected.raw_evidence = json!({
                "source_evidence": unsupported,
                "source_evidence_history": if with_history { vec![source_evidence_fixture(1)] } else { Vec::new() },
            }).to_string();
            let mut legacy = merge_test_record("legacy");
            legacy.raw_evidence = json!({"source_kind": "ocr"}).to_string();
            legacy.user_intent = "Invented intent".into();
            legacy.next_steps = vec!["Invented next step".into()];
            let merged =
                merge_memory_records_with_policy(protected, legacy, None, None, false, false).await;
            assert!(has_source_evidence(&merged.raw_evidence));
            assert!(merged.user_intent.is_empty());
            assert!(merged.next_steps.is_empty());
            let raw: serde_json::Value = serde_json::from_str(&merged.raw_evidence).unwrap();
            assert_eq!(raw["source_evidence"], unsupported);
            if with_history {
                assert_eq!(
                    raw["source_evidence_history"],
                    json!([source_evidence_fixture(1)])
                );
            }
        }
    }

    #[test]
    fn source_backed_merge_preserves_bounded_original_snapshots() {
        let mut raw = json!({"source_evidence": source_evidence_fixture(1)}).to_string();
        for index in 2..=6 {
            let incoming = json!({"source_evidence": source_evidence_fixture(index)}).to_string();
            raw = merge_text_source_evidence(&raw, &incoming);
            // Repeated observations must not consume the bounded history again.
            raw = merge_text_source_evidence(&raw, &incoming);
        }
        let evidence: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(evidence["source_evidence"], source_evidence_fixture(6));
        let history = evidence["source_evidence_history"].as_array().unwrap();
        assert_eq!(history.len(), 3);
        for index in 3..=5 {
            assert!(
                history.contains(&source_evidence_fixture(index)),
                "missing original snapshot {index}"
            );
        }
    }

    #[tokio::test]
    async fn merge_preserves_text_source_lineage_across_repeated_merges() {
        let mut existing = merge_test_record("existing");
        existing.clean_text = "Accessibility captured the original parser implementation.".into();
        existing.raw_evidence = r#"{"source_kind":"ax"}"#.into();
        let mut incoming = merge_test_record("incoming");
        incoming.clean_text = "OCR captured the regression test and its failure output.".into();
        incoming.raw_evidence = r#"{"source_kind":"ocr","ocr_quality":{"kept_lines":4},"embedding_manifest":{"marker":"preserve"}}"#.into();

        let merged =
            merge_memory_records_with_policy(existing, incoming, None, None, false, false).await;
        assert!(merged.clean_text.contains("original parser"));
        assert!(merged.clean_text.contains("regression test"));
        let evidence: serde_json::Value = serde_json::from_str(&merged.raw_evidence).unwrap();
        assert_eq!(evidence["source_kind"], "mixed");
        assert_eq!(evidence["text_source_kinds"], json!(["ax", "ocr"]));
        assert_eq!(evidence["ocr_quality"]["kept_lines"], 4);
        assert_eq!(evidence["embedding_manifest"]["marker"], "preserve");

        let mut later = merge_test_record("later");
        later.raw_evidence = r#"{"source_kind":"ax"}"#.into();
        let merged = merge_memory_records_with_policy(merged, later, None, None, true, false).await;
        let evidence: serde_json::Value = serde_json::from_str(&merged.raw_evidence).unwrap();
        assert_eq!(evidence["source_kind"], "mixed");
        assert_eq!(evidence["text_source_kinds"], json!(["ax", "ocr"]));
        assert!(evidence["embedding_manifest"].is_object());
    }

    #[tokio::test]
    async fn merge_text_source_lineage_never_guesses_a_legacy_extraction_method() {
        for raw in [
            "",
            "not json",
            "[]",
            "{}",
            r#"{"source_kind":"visual_capture"}"#,
            r#"{"source_kind":"private arbitrary label"}"#,
        ] {
            let mut existing = merge_test_record("existing");
            existing.raw_evidence = raw.into();
            let mut incoming = merge_test_record("incoming");
            incoming.raw_evidence = r#"{"source_kind":"ocr"}"#.into();
            let merged =
                merge_memory_records_with_policy(existing, incoming, None, None, false, false)
                    .await;
            let evidence: serde_json::Value = serde_json::from_str(&merged.raw_evidence).unwrap();
            assert_eq!(evidence["source_kind"], "mixed", "{raw}");
            assert_eq!(
                evidence["text_source_kinds"],
                json!(["ocr", "unknown"]),
                "{raw}"
            );
        }

        let mut existing = merge_test_record("existing");
        existing.raw_evidence = r#"{"source_kind":"browser_semantic"}"#.into();
        let incoming = existing.clone();
        let merged =
            merge_memory_records_with_policy(existing, incoming, None, None, false, false).await;
        let evidence: serde_json::Value = serde_json::from_str(&merged.raw_evidence).unwrap();
        assert_eq!(evidence["source_kind"], "browser_semantic");
        assert_eq!(evidence["text_source_kinds"], json!(["browser_semantic"]));
    }

    #[tokio::test]
    async fn merge_preserves_v2_metadata_when_incoming_is_sparse() {
        let mut existing = merge_test_record("existing");
        existing.schema_version = 2;
        existing.activity_type = "research".to_string();
        existing.tags = vec!["ai".to_string(), "tooling".to_string()];
        existing.entities = vec!["Hacker News".to_string()];
        existing.decisions = vec!["Track browser research sessions".to_string()];
        existing.project = "FNDR".to_string();
        existing.outcome = "in_progress".to_string();
        existing.extraction_confidence = 0.9;
        existing.dedup_fingerprint = "hn_research".to_string();
        existing.embedding_model = "bge-large-en-v1.5".to_string();
        existing.embedding_dim = EMBEDDING_DIM as u32;

        let mut incoming = merge_test_record("incoming");
        incoming.timestamp = 2;
        incoming.clean_text = "More Hacker News page text about Claude Code.".to_string();
        incoming.snippet = "Continued browsing Hacker News.".to_string();
        incoming.schema_version = 0;
        incoming.activity_type = String::new();
        incoming.tags = Vec::new();
        incoming.entities = Vec::new();
        incoming.decisions = Vec::new();
        incoming.project = String::new();
        incoming.outcome = String::new();
        incoming.extraction_confidence = 0.0;
        incoming.dedup_fingerprint = String::new();
        incoming.embedding_model = String::new();
        incoming.embedding_dim = 0;

        let merged =
            merge_memory_records_with_policy(existing, incoming, None, None, false, false).await;

        assert_eq!(merged.schema_version, 2);
        assert_eq!(merged.activity_type, "research");
        assert_eq!(merged.project, "FNDR");
        assert_eq!(merged.tags, vec!["ai".to_string(), "tooling".to_string()]);
        assert_eq!(merged.entities, vec!["Hacker News".to_string()]);
        assert_eq!(
            merged.decisions,
            vec!["Track browser research sessions".to_string()]
        );
        assert_eq!(merged.outcome, "in_progress");
        assert_eq!(merged.extraction_confidence, 0.9);
        assert_eq!(merged.dedup_fingerprint, "hn_research");
        assert_eq!(merged.embedding_model, "bge-large-en-v1.5");
        assert_eq!(merged.embedding_dim, EMBEDDING_DIM as u32);
    }

    #[tokio::test]
    async fn merge_keeps_incoming_id_in_consolidated_from_for_citation_redirect() {
        // MEM-07 invariant 8: the merged record always keeps `existing.id`
        // and drops `incoming.id`. Anything holding an earlier citation to
        // `incoming.id` (a search result, an agent's cited memory_id) needs
        // a way to still resolve it, so it must survive somewhere on the
        // merged record.
        let existing = merge_test_record("existing-id");
        let mut incoming = merge_test_record("incoming-id");
        incoming.timestamp = 2;

        let merged =
            merge_memory_records_with_policy(existing, incoming, None, None, false, false).await;

        assert_eq!(merged.id, "existing-id");
        assert!(
            merged
                .consolidated_from
                .contains(&"incoming-id".to_string()),
            "consolidated_from should carry the dropped id, got {:?}",
            merged.consolidated_from
        );
    }

    #[tokio::test]
    async fn merge_of_already_consolidated_records_does_not_lose_earlier_ids() {
        let mut existing = merge_test_record("existing-id");
        existing.consolidated_from = vec!["ancient-id".to_string()];
        let mut incoming = merge_test_record("incoming-id");
        incoming.timestamp = 2;
        incoming.consolidated_from = vec!["another-old-id".to_string()];

        let merged =
            merge_memory_records_with_policy(existing, incoming, None, None, false, false).await;

        assert_eq!(merged.id, "existing-id");
        for expected in ["ancient-id", "another-old-id", "incoming-id"] {
            assert!(
                merged.consolidated_from.iter().any(|id| id == expected),
                "expected {expected} in {:?}",
                merged.consolidated_from
            );
        }
    }

    #[tokio::test]
    async fn merge_unions_v2_list_metadata_without_duplicates() {
        let mut existing = merge_test_record("existing");
        existing.tags = vec!["AI".to_string(), "tooling".to_string()];
        existing.files_touched = vec!["src-tauri/src/store/lance_store.rs".to_string()];

        let mut incoming = merge_test_record("incoming");
        incoming.tags = vec!["ai".to_string(), "browser".to_string()];
        incoming.files_touched = vec![
            "src-tauri/src/store/lance_store.rs".to_string(),
            "src-tauri/src/capture/mod.rs".to_string(),
        ];

        let merged =
            merge_memory_records_with_policy(existing, incoming, None, None, false, false).await;

        assert_eq!(
            merged.tags,
            vec![
                "AI".to_string(),
                "tooling".to_string(),
                "browser".to_string()
            ]
        );
        assert_eq!(
            merged.files_touched,
            vec![
                "src-tauri/src/store/lance_store.rs".to_string(),
                "src-tauri/src/capture/mod.rs".to_string()
            ]
        );
    }

    #[test]
    fn a_suggestion_becomes_a_task_once_and_carries_its_quote() {
        use crate::tasks::suggest::Suggestion;
        let record = MemoryRecord {
            id: "mem-mail".into(),
            timestamp: 1_790_000_000_000,
            app_name: "Mail".into(),
            ..Default::default()
        };
        let suggest = |title: &str| Suggestion {
            task_type: crate::storage::TaskType::Todo,
            title: title.into(),
            quote: "can you send me the draft report by Friday".into(),
        };
        let created = tasks_from_suggestions(vec![suggest("Send the draft report")], &record, &[]);
        assert_eq!(created.len(), 1);
        assert_eq!(
            created[0].description,
            "can you send me the draft report by Friday"
        );
        assert_eq!(created[0].source_memory_id.as_deref(), Some("mem-mail"));
        assert_eq!(created[0].source_app, "Memory:Mail");

        // Already on the list, even dismissed: it does not come back.
        let mut dismissed = created[0].clone();
        dismissed.is_dismissed = true;
        let again = tasks_from_suggestions(
            vec![suggest("send the draft report."), suggest("Book the room")],
            &record,
            &[dismissed],
        );
        assert_eq!(
            again
                .iter()
                .map(|task| task.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Book the room"]
        );
    }

    #[test]
    fn validation_drops_prose_from_the_commands_field() {
        let mut extraction = crate::inference::StructuredMemoryExtraction {
            commands: vec![
                "cargo test --lib capture".into(),
                "Please refactor the capture module so that summaries are never cut".into(),
            ],
            ..Default::default()
        };
        let (_, issues) = validate_structured_memory_extraction(
            &mut extraction,
            "Terminal",
            "zsh",
            "cargo test --lib capture",
            "cargo test --lib capture",
        );
        assert_eq!(extraction.commands, vec!["cargo test --lib capture"]);
        assert!(issues
            .iter()
            .any(|issue| issue == "commands_not_command_like"));
    }

    #[test]
    fn source_backed_validator_checks_original_snapshot_after_fusion() {
        use crate::inference::extraction_evidence::finalize_extraction;
        let original_source = "Alex: please review the parser tests.";
        let mut extraction = StructuredMemoryExtraction {
            confidence: 0.95,
            topic: "parser tests".into(),
            ..Default::default()
        };
        extraction.source_refs.actions = vec![json!(1)];
        finalize_extraction(&mut extraction, original_source);
        // A browser/fusion seed can fill fields after model finalization.
        extraction.user_intent = "review the parser tests".into();
        extraction.todos = vec!["review the parser tests".into()];
        extraction.next_steps = vec!["review the parser tests".into()];
        let (_, issues) = validate_structured_memory_extraction(
            &mut extraction,
            "Editor",
            "parser tests",
            "parser tests cleaned differently",
            original_source,
        );
        assert!(extraction.user_intent.is_empty());
        assert!(extraction.todos.is_empty() && extraction.next_steps.is_empty());
        assert_eq!(
            extraction
                .source_evidence
                .as_ref()
                .unwrap()
                .statements
                .len(),
            1
        );
        assert!(!issues
            .iter()
            .any(|issue| issue == "source_evidence_hash_mismatch"));

        let (_, issues) = validate_structured_memory_extraction(
            &mut extraction,
            "Editor",
            "parser tests",
            original_source,
            "A changed model source snapshot.",
        );
        assert!(issues
            .iter()
            .any(|issue| issue == "source_evidence_hash_mismatch"));
        assert!(extraction
            .source_evidence
            .as_ref()
            .unwrap()
            .statements
            .is_empty());
    }

    #[test]
    fn extraction_validator_strips_unsupported_fields_when_low_confidence() {
        let mut extraction = StructuredMemoryExtraction {
            confidence: 0.42,
            project: "Skunkworks".to_string(),
            user_intent: "Finalize launch budget".to_string(),
            topic: "Revenue model".to_string(),
            files_touched: vec!["src-tauri/src/capture/mod.rs".to_string()],
            entities: vec!["Jane Doe".to_string()],
            dedup_fingerprint: "bad fingerprint ###".to_string(),
            ..Default::default()
        };

        let (grounding, issues) = validate_structured_memory_extraction(
            &mut extraction,
            "Google Chrome",
            "Random docs page",
            "Navigation links and generic toolbar labels",
            "",
        );

        assert!(grounding < 0.55);
        assert!(issues
            .iter()
            .any(|item| item == "structured_fields_weakly_grounded"));
        assert!(issues
            .iter()
            .any(|item| item == "possible_ungrounded_extraction"));
        assert!(issues
            .iter()
            .any(|item| item == "unsupported_dedup_fingerprint"));
        assert!(extraction.project.is_empty());
        assert!(extraction.user_intent.is_empty());
        assert!(extraction.topic.is_empty());
        assert!(extraction.files_touched.is_empty());
        assert!(extraction.entities.is_empty());
        assert!(extraction.dedup_fingerprint.is_empty());
    }

    #[test]
    fn extraction_validator_keeps_supported_fields_when_grounded() {
        let mut extraction = StructuredMemoryExtraction {
            confidence: 0.91,
            project: "FNDR".to_string(),
            user_intent: "Improve memory card search ranking".to_string(),
            topic: "ranking quality".to_string(),
            files_touched: vec!["src-tauri/src/search/memory_cards.rs".to_string()],
            entities: vec!["MemoryCardSynthesizer".to_string()],
            dedup_fingerprint: "fndr:ranking:memory_cards".to_string(),
            ..Default::default()
        };

        let (grounding, issues) = validate_structured_memory_extraction(
            &mut extraction,
            "Codex",
            "memory_cards.rs",
            "Improved memory card search ranking quality in src-tauri/src/search/memory_cards.rs using MemoryCardSynthesizer",
            "",
        );

        assert!(grounding > 0.80);
        assert!(!issues
            .iter()
            .any(|item| item == "possible_ungrounded_extraction"));
        assert_eq!(extraction.project, "FNDR");
        assert_eq!(extraction.user_intent, "Improve memory card search ranking");
        assert_eq!(extraction.files_touched.len(), 1);
        assert_eq!(extraction.entities.len(), 1);
        assert_eq!(extraction.dedup_fingerprint, "fndr:ranking:memory_cards");
    }

    #[test]
    fn weighted_primary_embedding_prefers_primary_vector() {
        let merged = weighted_primary_embedding(&[1.0, 0.0], &[0.0, 1.0], &[0.0, 1.0]);
        assert!(
            merged[0] > merged[1],
            "primary component should dominate weighted output"
        );
        let norm = (merged.iter().map(|value| value * value).sum::<f32>()).sqrt();
        assert!((norm - 1.0).abs() < 1e-3);
    }

    fn durable_context_config(min: u32, max: u32) -> crate::config::MemoryQualityConfig {
        let mut cfg = crate::config::MemoryQualityConfig::default();
        cfg.memory_context_min_chars = min;
        cfg.memory_context_max_chars = max;
        cfg
    }

    fn synth_prior_search_result(id: &str, context: &str) -> crate::storage::SearchResult {
        crate::storage::SearchResult {
            id: id.to_string(),
            memory_context: context.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn durable_memory_context_respects_min_max_bounds_with_empty_chain() {
        let extraction = StructuredMemoryExtraction {
            user_intent: "Refactor the synthesis pipeline".to_string(),
            project: "FNDR".to_string(),
            topic: "memory synthesis".to_string(),
            decisions: vec!["Use durable memory_context as embedding seed".to_string()],
            next_steps: vec!["Wire compress_to_salient_evidence into the tail".to_string()],
            ..Default::default()
        };
        let cfg = durable_context_config(220, 1800);
        let context = build_durable_memory_context(
            Some(&extraction),
            "GenericEditor",
            "synthesis-doc.md",
            "We are aligning the embedding text composition.",
            "Refactored OCR cleanup.",
            Some("com.example.editor"),
            None,
            &[],
            &cfg,
        );
        assert!(
            context.chars().count() >= cfg.memory_context_min_chars as usize,
            "durable context shorter than min ({}): {}",
            context.chars().count(),
            context
        );
        assert!(context.chars().count() <= cfg.memory_context_max_chars as usize);
        assert!(
            context.to_lowercase().contains("topic")
                || context.to_lowercase().contains("memory synthesis"),
            "should surface semantic center"
        );
    }

    #[test]
    fn durable_memory_context_references_prior_work_without_machine_marker() {
        let extraction = StructuredMemoryExtraction {
            user_intent: "Continue refactor".to_string(),
            topic: "alias generation".to_string(),
            decisions: vec!["Adopt noun-phrase sourcing".to_string()],
            ..Default::default()
        };
        let cfg = durable_context_config(160, 1800);
        let prior = synth_prior_search_result(
            "abcd1234efgh5678",
            "Earlier card outlining the durable memory context plan.",
        );
        let context = build_durable_memory_context(
            Some(&extraction),
            "GenericEditor",
            "doc",
            "More work",
            "",
            None,
            None,
            std::slice::from_ref(&prior),
            &cfg,
        );
        assert!(
            context.contains("This continues earlier related work"),
            "continuation footer missing: {}",
            context
        );
        assert!(
            !context.contains("Continues from "),
            "machine continuation marker leaked into memory_context: {}",
            context
        );
    }

    #[test]
    fn durable_memory_context_keeps_reopen_marker_out_of_context() {
        let cfg = durable_context_config(160, 1800);
        let context = build_durable_memory_context(
            None,
            "GenericEditor",
            "doc",
            "Worked on the design doc and reviewed the spec.",
            "Reviewed design doc.",
            None,
            Some("https://example.org/path"),
            &[],
            &cfg,
        );
        assert!(
            !context.contains("Reopen:"),
            "reopen marker leaked into memory_context: {}",
            context
        );
    }

    #[test]
    fn durable_memory_context_truncates_to_max_with_three_priors() {
        let extraction = StructuredMemoryExtraction {
            user_intent: "Long-running multi-session refactor".to_string(),
            topic: "memory synthesis".to_string(),
            decisions: vec!["a; ".repeat(60).trim_end().to_string()],
            errors: vec!["x; ".repeat(60).trim_end().to_string()],
            next_steps: vec!["n; ".repeat(60).trim_end().to_string()],
            results: vec!["r; ".repeat(60).trim_end().to_string()],
            ..Default::default()
        };
        let cfg = durable_context_config(220, 600);
        let prior_a = synth_prior_search_result("a1b2c3d4", "Prior alpha context.");
        let prior_b = synth_prior_search_result("e5f6g7h8", "Prior beta context.");
        let prior_c = synth_prior_search_result("i9j0k1l2", "Prior gamma context.");
        let priors = vec![prior_a, prior_b, prior_c];
        let context = build_durable_memory_context(
            Some(&extraction),
            "GenericEditor",
            "doc",
            "evidence body",
            "Reviewed design doc.",
            None,
            None,
            &priors,
            &cfg,
        );
        assert!(context.chars().count() <= cfg.memory_context_max_chars as usize);
    }

    // ── Narrative-first composer ──────────────────────────────────────────

    #[test]
    fn durable_memory_context_leads_with_narrative_and_drops_topic_when_covered() {
        let extraction = StructuredMemoryExtraction {
            user_intent: "researching".to_string(),
            topic: "memory synthesis".to_string(),
            activity_type: "researching".to_string(),
            memory_context:
                "Continued the memory synthesis investigation, comparing two ranking strategies."
                    .to_string(),
            ..Default::default()
        };
        let cfg = durable_context_config(80, 1800);
        let context = build_durable_memory_context(
            Some(&extraction),
            "GenericEditor",
            "research-doc.md",
            "evidence body",
            "Reviewed design doc.",
            None,
            None,
            &[],
            &cfg,
        );
        let head_line = context.lines().next().unwrap_or("");
        assert!(
            head_line.starts_with("Continued the memory synthesis"),
            "narrative should lead, got: {head_line:?}"
        );
        // Topic tokens ("memory", "synthesis") already appear in the narrative,
        // so the explicit Topic: line must be suppressed.
        assert!(
            !context.contains("Topic: memory synthesis"),
            "Topic: line must be dropped when narrative covers it; got: {context}"
        );
        // And the robotic "You were ..."/"Activity: ..." preamble must not fire
        // when a narrative is present.
        assert!(!context.contains("You were "));
        assert!(!context.contains("Activity: "));
    }

    #[test]
    fn durable_memory_context_appends_topic_when_narrative_lacks_it() {
        let extraction = StructuredMemoryExtraction {
            topic: "ranking quality".to_string(),
            memory_context: "Focused on chart styling improvements for the dashboard.".to_string(),
            ..Default::default()
        };
        let cfg = durable_context_config(80, 1800);
        let context = build_durable_memory_context(
            Some(&extraction),
            "GenericEditor",
            "doc",
            "evidence body",
            "Reviewed design doc.",
            None,
            None,
            &[],
            &cfg,
        );
        assert!(
            context.contains("Topic: ranking quality"),
            "Topic should be appended when narrative omits the topic tokens; got: {context}"
        );
    }

    #[test]
    fn validate_structured_memory_extraction_clears_pipe_delimited_activity_type() {
        let mut extraction = StructuredMemoryExtraction {
            confidence: 0.42,
            activity_type: "coding|debugging|reviewing_agent_output|researching|planning|writing"
                .to_string(),
            topic: "valid topic".to_string(),
            ..Default::default()
        };
        let (_grounding, issues) = validate_structured_memory_extraction(
            &mut extraction,
            "Chrome",
            "title",
            "valid topic appeared in evidence",
            "",
        );
        assert_eq!(extraction.activity_type, "unknown");
        assert!(
            issues
                .iter()
                .any(|i| i == "activity_type_multi_option_dump"),
            "expected activity_type_multi_option_dump in issues: {issues:?}"
        );
    }

    #[test]
    fn validate_structured_memory_extraction_clears_pipe_delimited_topic_and_workflow() {
        let mut extraction = StructuredMemoryExtraction {
            confidence: 0.5,
            topic: "topic|alternative".to_string(),
            workflow: "wf_a|wf_b".to_string(),
            user_intent: "intent|other".to_string(),
            ..Default::default()
        };
        let (_, issues) = validate_structured_memory_extraction(
            &mut extraction,
            "App",
            "Title",
            "Evidence body",
            "",
        );
        assert!(extraction.topic.is_empty());
        assert!(extraction.workflow.is_empty());
        assert!(extraction.user_intent.is_empty());
        assert!(issues.iter().any(|i| i == "topic_multi_option_dump"));
        assert!(issues.iter().any(|i| i == "workflow_multi_option_dump"));
        assert!(issues.iter().any(|i| i == "user_intent_multi_option_dump"));
    }

    #[test]
    fn narrative_mentions_detects_majority_overlap() {
        assert!(narrative_mentions(
            Some("The memory synthesis pipeline keeps cards short."),
            "memory synthesis pipeline",
        ));
        assert!(!narrative_mentions(
            Some("Browsed unrelated news articles."),
            "memory synthesis pipeline",
        ));
        assert!(!narrative_mentions(None, "any topic"));
    }

    // ── VisualNoveltyTracker ──────────────────────────────────────────────

    fn synth_vec(seed: usize, dim: usize) -> Vec<f32> {
        // Deterministic, non-zero, mostly-orthogonal-ish across seeds. Not
        // normalized — `cosine_similarity` already handles arbitrary norms.
        (0..dim)
            .map(|i| (((seed * 31 + i * 7) % 17) as f32 - 8.0) / 8.0)
            .collect()
    }

    #[test]
    fn visual_novelty_tracker_admits_first_frame_when_ring_is_empty() {
        let tracker = VisualNoveltyTracker::default();
        let v = synth_vec(1, 32);
        assert!((tracker.novelty(&v) - 1.0).abs() < 1e-6);
        let t0 = tracker.adaptive_threshold(0.30, 0.05, 0.85);
        assert!((t0 - 0.30).abs() < 1e-6, "first threshold = base");
    }

    #[test]
    fn visual_novelty_tracker_rejects_near_duplicate_after_admit() {
        let mut tracker = VisualNoveltyTracker::default();
        tracker.reset_for("session-a");
        let v = synth_vec(7, 32);
        tracker.admit(v.clone(), 16);
        // Same vector → cosine ~1.0 → novelty ~0.0, well below 0.30 base.
        let novelty = tracker.novelty(&v);
        assert!(
            novelty < 0.05,
            "near-duplicate should produce ~0 novelty, got {novelty}"
        );
    }

    #[test]
    fn visual_novelty_tracker_adaptive_threshold_rises_with_admits() {
        let mut tracker = VisualNoveltyTracker::default();
        tracker.reset_for("session-x");
        for i in 0..5 {
            tracker.admit(synth_vec(100 + i, 32), 16);
        }
        let t5 = tracker.adaptive_threshold(0.30, 0.05, 0.85);
        assert!((t5 - 0.55).abs() < 1e-6, "0.30 + 5*0.05 = 0.55, got {t5}");
        // Ceiling caps the threshold.
        for i in 0..20 {
            tracker.admit(synth_vec(500 + i, 32), 16);
        }
        let t_capped = tracker.adaptive_threshold(0.30, 0.05, 0.85);
        assert!(t_capped <= 0.85 + 1e-6);
    }

    #[test]
    fn visual_novelty_tracker_resets_on_session_change() {
        let mut tracker = VisualNoveltyTracker::default();
        tracker.reset_for("session-a");
        tracker.admit(synth_vec(1, 32), 16);
        tracker.admit(synth_vec(2, 32), 16);
        assert_eq!(tracker.admitted, 2);
        tracker.reset_for("session-b");
        assert_eq!(tracker.admitted, 0);
        assert!(tracker.recent.is_empty());
    }

    #[test]
    fn visual_novelty_tracker_respects_ring_capacity() {
        let mut tracker = VisualNoveltyTracker::default();
        tracker.reset_for("ring-test");
        for i in 0..10 {
            tracker.admit(synth_vec(i, 32), 4);
        }
        assert_eq!(tracker.recent.len(), 4, "ring should be capped at 4");
        assert_eq!(tracker.admitted, 10);
    }

    #[test]
    fn image_import_source_screen_capture_has_distinct_labels() {
        use crate::inference::ImageImportSource;
        assert_eq!(
            ImageImportSource::ScreenCapture.api_label(),
            "screen_capture_visual"
        );
        assert_eq!(
            ImageImportSource::ScreenCapture.header_label(),
            "Screen capture (visual)"
        );
    }

    #[test]
    fn compose_visual_capture_screen_capture_leads_with_screen_capture_header() {
        use crate::inference::{
            compose_import_memory_context, ImageImportSource, ImageSemanticInsight,
        };
        let insight = ImageSemanticInsight {
            summary_short: "User watching a cricket match overlay.".to_string(),
            summary_detailed:
                "Browser window playing a livestream with scoreboard and play-by-play.".to_string(),
            scene_type: "livestream".to_string(),
            topics: vec!["cricket livestream".to_string()],
            ..Default::default()
        };
        let composed = compose_import_memory_context(
            "GoogleChrome_1.png",
            &insight,
            None,
            ImageImportSource::ScreenCapture,
        );
        assert!(
            composed
                .memory_context
                .starts_with("Screen capture (visual):"),
            "expected screen-capture header, got: {}",
            composed.memory_context
        );
        // Narrative content from the VLM must appear right after the header.
        assert!(composed.memory_context.contains("livestream"));
    }

    #[test]
    fn text_heavy_override_fires_for_large_clean_ocr() {
        // 1,400 chars, 38 blocks, confidence 0.49, low noise — should override
        assert!(should_text_heavy_override(
            1400, 0.49, 38, 0.20, 0.50, false
        ));
    }

    #[test]
    fn text_heavy_override_blocked_by_stacked_issues() {
        // Even with good OCR, stacked issues prevent override
        assert!(!should_text_heavy_override(
            1400, 0.49, 38, 0.20, 0.50, true
        ));
    }

    #[test]
    fn text_heavy_override_blocked_by_high_noise() {
        // High noise_score prevents override even for large text
        assert!(!should_text_heavy_override(
            1400, 0.49, 38, 0.80, 0.50, false
        ));
    }

    #[test]
    fn text_heavy_override_blocked_for_tiny_text() {
        // 50 chars doesn't qualify
        assert!(!should_text_heavy_override(50, 0.49, 5, 0.20, 0.50, false));
    }

    #[test]
    fn embedder_gate_proceeds_when_embedder_present() {
        // A live embedder always admits the frame, even long past the retry interval.
        assert_eq!(
            embedder_gate_action(true, Duration::from_secs(3600), Duration::from_secs(30)),
            EmbedderGateAction::Proceed
        );
    }

    #[test]
    fn embedder_gate_blocks_when_embedder_missing_and_retry_not_due() {
        // A missing embedder blocks frame processing so zero-vector memory
        // rows never reach storage.
        assert_eq!(
            embedder_gate_action(false, Duration::from_secs(5), Duration::from_secs(30)),
            EmbedderGateAction::Block
        );
    }

    #[test]
    fn embedder_gate_retries_init_when_embedder_missing_and_interval_elapsed() {
        // Once the retry interval passes (e.g. after the model download
        // completes), the loop attempts to re-initialize before blocking.
        assert_eq!(
            embedder_gate_action(false, Duration::from_secs(30), Duration::from_secs(30)),
            EmbedderGateAction::RetryInit
        );
    }
}
