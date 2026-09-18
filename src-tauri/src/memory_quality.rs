//! Shared memory-quality helpers used by capture, store normalization, and API diagnostics.

use crate::config::{
    MemoryQualityConfig, DEFAULT_MEMORY_CONTEXT_MAX_CHARS, DEFAULT_MEMORY_CONTEXT_MIN_CHARS,
    DEFAULT_PRIMARY_MEMORY_AGENT_USEFULNESS_MIN, DEFAULT_PRIMARY_MEMORY_INTENT_MIN,
    DEFAULT_PRIMARY_MEMORY_OCR_NOISE_MAX, DEFAULT_PRIMARY_MEMORY_SPECIFICITY_MIN,
};
use crate::storage::{MemoryRecord, SearchResult};
use serde_json::Value;

pub const VISUAL_SEMANTICS_FAILED_OUTCOME: &str = "visual_semantics_failed";
pub const LOW_EVIDENCE_VISUAL_FALLBACK_REASON: &str =
    "low_evidence_visual_fallback=clip_vector_without_text_or_pixel_vlm_semantics";

pub fn default_memory_quality_config() -> MemoryQualityConfig {
    MemoryQualityConfig {
        primary_memory_specificity_min: DEFAULT_PRIMARY_MEMORY_SPECIFICITY_MIN,
        primary_memory_intent_min: DEFAULT_PRIMARY_MEMORY_INTENT_MIN,
        primary_memory_agent_usefulness_min: DEFAULT_PRIMARY_MEMORY_AGENT_USEFULNESS_MIN,
        primary_memory_ocr_noise_max: DEFAULT_PRIMARY_MEMORY_OCR_NOISE_MAX,
        memory_context_min_chars: DEFAULT_MEMORY_CONTEXT_MIN_CHARS,
        memory_context_max_chars: DEFAULT_MEMORY_CONTEXT_MAX_CHARS,
    }
}

pub fn classify_storage_outcome(record: &MemoryRecord, config: &MemoryQualityConfig) -> String {
    if is_visual_semantics_failed_record(record) {
        return VISUAL_SEMANTICS_FAILED_OUTCOME.to_string();
    }
    if hard_gate_structured_extraction_failed(record) {
        return "quarantine_low_grounding".to_string();
    }
    if hard_gate_polluted_memory_context(record) {
        return "quarantine_polluted_context".to_string();
    }
    if hard_gate_fluff_insight(record) {
        return "quarantine_fluff_insight".to_string();
    }
    if is_visual_metadata_fallback_record(record) || is_low_evidence_visual_fallback_record(record)
    {
        return "low_quality_evidence".to_string();
    }

    let primary = record.specificity_score >= config.primary_memory_specificity_min
        && record.intent_score >= config.primary_memory_intent_min
        && record.agent_usefulness_score >= config.primary_memory_agent_usefulness_min
        && record.ocr_noise_score <= config.primary_memory_ocr_noise_max;
    if primary {
        "primary_memory_card".to_string()
    } else if !record.dedup_fingerprint.trim().is_empty()
        && record.specificity_score < config.primary_memory_specificity_min
    {
        "merge_into_existing_memory".to_string()
    } else if !record.related_memory_ids.is_empty()
        && record.retrieval_value_score < 0.35
        && record.evidence_confidence < 0.50
    {
        "discard_duplicate".to_string()
    } else if record.agent_usefulness_score >= 0.45 {
        "enriched_memory_card".to_string()
    } else if record.evidence_confidence >= 0.45 {
        "low_quality_evidence".to_string()
    } else {
        "defer_until_more_context".to_string()
    }
}

pub fn quality_gate_reason(record: &MemoryRecord) -> String {
    if is_visual_semantics_failed_record(record) {
        return "hard_gate=visual_semantics_failed".to_string();
    }
    if hard_gate_structured_extraction_failed(record) {
        return "hard_gate=structured_extraction_unavailable_with_zero_grounding".to_string();
    }
    if hard_gate_polluted_memory_context(record) {
        return "hard_gate=machine_marker_in_memory_context".to_string();
    }
    if hard_gate_fluff_insight(record) {
        return "hard_gate=fluff_or_template_insight".to_string();
    }
    if is_visual_metadata_fallback_record(record) {
        return "visual_metadata_fallback=clip_vector_without_pixel_vlm_semantics".to_string();
    }
    if is_low_evidence_visual_fallback_record(record) {
        return LOW_EVIDENCE_VISUAL_FALLBACK_REASON.to_string();
    }

    format!(
        "specificity={:.2}, intent={:.2}, entities={:.2}, usefulness={:.2}, evidence={:.2}, noise={:.2}",
        record.specificity_score,
        record.intent_score,
        record.entity_score,
        record.agent_usefulness_score,
        record.evidence_confidence,
        record.ocr_noise_score
    )
}

/// Minimum alphanumeric characters of on-screen text (excluding the app name,
/// image filenames, and "Screen capture (visual)" boilerplate) for a visual
/// fallback capture to count as a real memory.
pub const LOW_SIGNAL_TEXT_MIN_CHARS: usize = 40;

/// Why a stored memory is kept out of Search, Home, the default Vault list,
/// and Ask FNDR. The record stays stored and is listed only in the Vault's
/// "Needs more signal" view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LowSignalReason {
    VisualSemanticsFailed,
    ImageOnly,
    Ungrounded,
}

impl LowSignalReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::VisualSemanticsFailed => "visual_semantics_failed",
            Self::ImageOnly => "image_only",
            Self::Ungrounded => "ungrounded_summary",
        }
    }

    /// Plain-language reason shown in the Vault "Needs more signal" list.
    pub fn user_message(self) -> &'static str {
        match self {
            Self::VisualSemanticsFailed => {
                "FNDR couldn't read this screen, so it was kept out of search."
            }
            Self::ImageOnly => {
                "Image only: no readable text was on screen, so it was kept out of search."
            }
            Self::Ungrounded => {
                "The summary couldn't be matched to on-screen text, so it was kept out of search."
            }
        }
    }
}

/// Read-side fields shared by `MemoryRecord` and `SearchResult`.
pub struct SurfaceSignals<'a> {
    pub storage_outcome: &'a str,
    pub enrichment_status: &'a str,
    pub synthesis_branch: &'a str,
    pub ocr_block_count: u32,
    pub ocr_confidence: f32,
    pub clean_text: &'a str,
    pub display_summary: &'a str,
    pub app_name: &'a str,
}

impl<'a> SurfaceSignals<'a> {
    pub fn from_record(r: &'a MemoryRecord) -> Self {
        Self {
            storage_outcome: &r.storage_outcome,
            enrichment_status: &r.enrichment_status,
            synthesis_branch: &r.synthesis_branch,
            ocr_block_count: r.ocr_block_count,
            ocr_confidence: r.ocr_confidence,
            clean_text: &r.clean_text,
            display_summary: &r.display_summary,
            app_name: &r.app_name,
        }
    }

    pub fn from_result(r: &'a SearchResult) -> Self {
        Self {
            storage_outcome: &r.storage_outcome,
            enrichment_status: &r.enrichment_status,
            synthesis_branch: &r.synthesis_branch,
            ocr_block_count: r.ocr_block_count,
            ocr_confidence: r.ocr_confidence,
            clean_text: &r.clean_text,
            display_summary: &r.display_summary,
            app_name: &r.app_name,
        }
    }
}

fn is_image_filename_token(token: &str) -> bool {
    const EXTS: [&str; 6] = [".png", ".jpg", ".jpeg", ".heic", ".gif", ".webp"];
    EXTS.iter().any(|ext| token.ends_with(ext))
        || (!token.is_empty() && token.chars().all(|c| c.is_ascii_digit() || c == '_'))
}

/// Counts alphanumeric characters that carry meaning beyond the app name,
/// image filenames, and FNDR's own visual-fallback boilerplate.
pub fn meaningful_text_chars(text: &str, app_name: &str) -> usize {
    let app_tokens: Vec<String> = app_name
        .split_whitespace()
        .map(|t| t.to_lowercase())
        .collect();
    text.split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '_')
                .trim_end_matches('.')
                .to_lowercase()
        })
        .filter(|t| !t.is_empty())
        .filter(|t| !is_image_filename_token(t))
        .filter(|t| !app_tokens.iter().any(|a| a == t))
        .filter(|t| !matches!(t.as_str(), "screen" | "capture" | "visual" | "also"))
        .map(|t| t.chars().filter(|c| c.is_alphanumeric()).count())
        .sum()
}

/// The single read-side admission policy for Search, Home, Vault, and Ask FNDR.
pub fn low_signal_reason(s: &SurfaceSignals<'_>) -> Option<LowSignalReason> {
    let outcome = s.storage_outcome.trim();
    if outcome.eq_ignore_ascii_case(VISUAL_SEMANTICS_FAILED_OUTCOME) {
        return Some(LowSignalReason::VisualSemanticsFailed);
    }
    if outcome.starts_with("quarantine_") {
        return Some(LowSignalReason::Ungrounded);
    }

    let thin_text = s.ocr_block_count == 0
        || s.ocr_confidence <= 0.01
        || meaningful_text_chars(s.clean_text, s.app_name) < LOW_SIGNAL_TEXT_MIN_CHARS;
    let branch = s.synthesis_branch.trim();
    let visual_fallback = s
        .enrichment_status
        .trim()
        .eq_ignore_ascii_case("visual_metadata_fallback")
        || branch.eq_ignore_ascii_case("visual_metadata_fallback")
        || branch.eq_ignore_ascii_case("llm_ocr_grounded_visual_fallback");
    let filename_summary = s
        .display_summary
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("screen capture (visual)");

    if (visual_fallback || filename_summary) && thin_text {
        return Some(LowSignalReason::ImageOnly);
    }
    None
}

pub fn result_low_signal_reason(r: &SearchResult) -> Option<LowSignalReason> {
    low_signal_reason(&SurfaceSignals::from_result(r))
}

/// Record variant: also consults `raw_evidence`-based detectors that
/// `SearchResult` can't see.
pub fn record_low_signal_reason(r: &MemoryRecord) -> Option<LowSignalReason> {
    if let Some(reason) = low_signal_reason(&SurfaceSignals::from_record(r)) {
        return Some(reason);
    }
    if is_visual_semantics_failed_record(r) {
        return Some(LowSignalReason::VisualSemanticsFailed);
    }
    let thin_text = meaningful_text_chars(&r.clean_text, &r.app_name) < LOW_SIGNAL_TEXT_MIN_CHARS;
    if is_low_evidence_visual_fallback_record(r)
        || (is_visual_metadata_fallback_record(r) && thin_text)
    {
        return Some(LowSignalReason::ImageOnly);
    }
    None
}

/// Splits results into (surfaceable, low-signal-with-reason), preserving order.
pub fn partition_surfaceable(
    results: Vec<SearchResult>,
) -> (Vec<SearchResult>, Vec<(SearchResult, LowSignalReason)>) {
    let mut kept = Vec::with_capacity(results.len());
    let mut hidden = Vec::new();
    for result in results {
        match result_low_signal_reason(&result) {
            Some(reason) => hidden.push((result, reason)),
            None => kept.push(result),
        }
    }
    (kept, hidden)
}

pub fn is_supported_dedup_fingerprint(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.len() < 6 || trimmed.len() > 120 {
        return false;
    }
    trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | ':' | '.' | '/'))
}

pub fn is_visual_semantics_failed_record(record: &MemoryRecord) -> bool {
    if record
        .storage_outcome
        .trim()
        .eq_ignore_ascii_case(VISUAL_SEMANTICS_FAILED_OUTCOME)
    {
        return true;
    }
    serde_json::from_str::<Value>(&record.raw_evidence)
        .ok()
        .and_then(|json| {
            json.get("extraction_issues")
                .and_then(|value| value.as_array())
                .map(|issues| {
                    issues.iter().any(|issue| {
                        issue
                            .as_str()
                            .map(|s| s.eq_ignore_ascii_case(VISUAL_SEMANTICS_FAILED_OUTCOME))
                            .unwrap_or(false)
                    })
                })
        })
        .unwrap_or(false)
}

pub fn cap_visual_semantics_failed_scores(record: &mut MemoryRecord) {
    record.evidence_confidence = record.evidence_confidence.clamp(0.0, 0.30);
    record.agent_usefulness_score = record.agent_usefulness_score.clamp(0.0, 0.25);
    record.retrieval_value_score = record.retrieval_value_score.clamp(0.0, 0.25);
    record.graph_readiness_score = record.graph_readiness_score.clamp(0.0, 0.15);
    record.specificity_score = record.specificity_score.clamp(0.0, 0.15);
    record.intent_score = record.intent_score.clamp(0.0, 0.10);
    record.entity_score = 0.0;
    record.confidence_score = record.confidence_score.clamp(0.0, 0.20);
    record.importance_score = record.importance_score.clamp(0.0, 0.20);
    record.extraction_confidence = record.extraction_confidence.clamp(0.0, 0.15);
    record.insight_card_confidence = record.insight_card_confidence.clamp(0.0, 0.15);
    record.intent_analysis.confidence = record.intent_analysis.confidence.clamp(0.0, 0.10);
}

pub fn is_visual_metadata_fallback_record(record: &MemoryRecord) -> bool {
    if record
        .enrichment_status
        .trim()
        .eq_ignore_ascii_case("visual_metadata_fallback")
        || record
            .synthesis_branch
            .trim()
            .eq_ignore_ascii_case("visual_metadata_fallback")
    {
        return true;
    }
    serde_json::from_str::<Value>(&record.raw_evidence)
        .ok()
        .map(|json| {
            let visual_status = json
                .get("visual_understanding")
                .and_then(|value| value.get("status"))
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let has_issue = json
                .get("extraction_issues")
                .and_then(|value| value.as_array())
                .map(|issues| {
                    issues.iter().any(|issue| {
                        issue
                            .as_str()
                            .map(|s| s.eq_ignore_ascii_case("visual_metadata_fallback"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            visual_status.eq_ignore_ascii_case("clip_metadata_fallback") || has_issue
        })
        .unwrap_or(false)
}

pub fn is_low_evidence_visual_fallback_record(record: &MemoryRecord) -> bool {
    let branch = record.synthesis_branch.trim();
    let mut branch_is_low_evidence_visual =
        branch.eq_ignore_ascii_case("llm_ocr_grounded_visual_fallback");
    let mut pixel_vlm_absent = false;
    let mut has_clip_image_embedding = false;

    if let Ok(json) = serde_json::from_str::<Value>(&record.raw_evidence) {
        if let Some(raw_branch) = json
            .get("synthesis_branch")
            .and_then(|value| value.as_str())
        {
            branch_is_low_evidence_visual |=
                raw_branch.eq_ignore_ascii_case("llm_ocr_grounded_visual_fallback");
        }
        let vlm_route = json
            .get("vlm_route")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let runtime_status = json
            .get("vlm_runtime_status")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        let capability = json
            .get("vlm_capability")
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        pixel_vlm_absent = vlm_route.eq_ignore_ascii_case("fallback_ocr_only")
            || runtime_status.starts_with("deferred")
            || capability.eq_ignore_ascii_case("model_missing");

        let visual_status = json
            .get("visual_understanding")
            .and_then(|value| value.get("status"))
            .and_then(|value| value.as_str())
            .unwrap_or_default();
        has_clip_image_embedding = visual_status.eq_ignore_ascii_case("clip_image_embedding");
    }

    let no_ocr_signal = record.ocr_block_count == 0 || record.ocr_confidence <= 0.01;

    branch_is_low_evidence_visual && no_ocr_signal && (pixel_vlm_absent || has_clip_image_embedding)
}

pub fn deterministic_dedup_fingerprint(
    record: &MemoryRecord,
    evidence_hint: Option<&str>,
) -> String {
    let app = normalize_tokenish(&record.app_name);
    let title = normalize_tokenish(&record.window_title);
    let project = normalize_tokenish(&record.project);
    let topic = normalize_tokenish(&record.topic);
    let activity = normalize_tokenish(&record.activity_type);
    let url = record
        .url
        .as_deref()
        .map(extract_domain)
        .unwrap_or_default();
    let signal_source = evidence_hint
        .filter(|hint| !hint.trim().is_empty())
        .unwrap_or(record.clean_text.as_str());
    let evidence = stable_signal_terms(signal_source, 6);
    let mut parts = vec![
        if app.is_empty() {
            "app".to_string()
        } else {
            app
        },
        if url.is_empty() { title } else { url },
    ];
    if !project.is_empty() {
        parts.push(project);
    } else if !topic.is_empty() && topic != "unknown" {
        parts.push(topic);
    }
    if !activity.is_empty() && activity != "unknown" {
        parts.push(activity);
    }
    if !evidence.is_empty() {
        parts.push(evidence.join("_"));
    }
    parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .take(5)
        .collect::<Vec<_>>()
        .join(":")
}

fn normalize_tokenish(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .take(8)
        .collect::<Vec<_>>()
        .join("_")
}

fn stable_signal_terms(value: &str, max_terms: usize) -> Vec<String> {
    let mut out = Vec::new();
    for token in value
        .to_ascii_lowercase()
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 3)
    {
        if STOP_TOKENS.contains(&token) {
            continue;
        }
        if !out.iter().any(|item| item == token) {
            out.push(token.to_string());
        }
        if out.len() >= max_terms {
            break;
        }
    }
    out
}

fn extract_domain(url: &str) -> String {
    let raw = url.trim();
    if raw.is_empty() {
        return String::new();
    }
    raw.split("://")
        .nth(1)
        .unwrap_or(raw)
        .split('/')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

const STOP_TOKENS: &[&str] = &[
    "the", "and", "for", "with", "from", "this", "that", "into", "your", "you", "user", "www",
];

fn hard_gate_structured_extraction_failed(record: &MemoryRecord) -> bool {
    let has_structured_unavailable = extraction_issues(record)
        .iter()
        .any(|issue| issue.eq_ignore_ascii_case("structured_extraction_unavailable"));
    if !has_structured_unavailable {
        return false;
    }

    if let Some(grounding) = extraction_grounding_confidence(record) {
        return grounding <= 0.0;
    }

    record.evidence_confidence <= 0.0
}

fn hard_gate_polluted_memory_context(record: &MemoryRecord) -> bool {
    let context = record.memory_context.trim();
    context.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("Reopen: ") || trimmed.starts_with("Continues from ")
    })
}

/// Block records whose insight text is pure fluff/template — filename patterns,
/// "Captured recent activity", or 0-information-density strings. These would
/// otherwise pollute search results with non-actionable rows.
fn hard_gate_fluff_insight(record: &MemoryRecord) -> bool {
    let what = record.insight_what_happened.trim();
    if what.is_empty() {
        return false; // empty is handled by other gates
    }
    let lower = what.to_ascii_lowercase();

    // Filename with embedded timestamp: ".png" + 6+ digits
    if lower.contains(".png") && lower.chars().filter(|c| c.is_ascii_digit()).count() >= 6 {
        return true;
    }
    // Template strings
    if lower.starts_with("screen capture (visual)")
        || lower.starts_with("captured recent activity")
        || lower.starts_with("url-only surface capture")
    {
        return true;
    }
    // Tautologies: "AppName: AppName" or just AppName repeated
    let app_lower = record.app_name.trim().to_ascii_lowercase();
    if !app_lower.is_empty() && what.split_whitespace().count() <= 4 {
        let app_count = lower.matches(&app_lower).count();
        // 2+ mentions of the app in a <=4-word insight is content-free
        if app_count >= 2 {
            return true;
        }
        // "You used <App>." alone (with no specifics) is the minimum tolerable —
        // allow it for now, but tag as low-quality elsewhere.
    }
    false
}

fn extraction_issues(record: &MemoryRecord) -> Vec<String> {
    let parsed = serde_json::from_str::<Value>(&record.raw_evidence).ok();
    parsed
        .as_ref()
        .and_then(|json| json.get("extraction_issues"))
        .and_then(|value| value.as_array())
        .map(|issues| {
            issues
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn extraction_grounding_confidence(record: &MemoryRecord) -> Option<f32> {
    serde_json::from_str::<Value>(&record.raw_evidence)
        .ok()
        .and_then(|json| json.get("extraction_grounding_confidence").cloned())
        .and_then(|value| value.as_f64())
        .map(|value| value as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_fingerprint_fallback_is_stable_and_supported() {
        let record = MemoryRecord {
            app_name: "Codex".to_string(),
            window_title: "capture/mod.rs".to_string(),
            project: "FNDR".to_string(),
            topic: "memory cards".to_string(),
            activity_type: "debugging".to_string(),
            clean_text: "Investigated OCR quality regressions in capture loop".to_string(),
            ..Default::default()
        };
        let value =
            deterministic_dedup_fingerprint(&record, Some("Fixed OCR grounding confidence"));
        assert!(
            is_supported_dedup_fingerprint(&value),
            "generated fallback should be valid: {value}"
        );
    }

    #[test]
    fn storage_outcome_primary_when_scores_are_high() {
        let cfg = default_memory_quality_config();
        let record = MemoryRecord {
            specificity_score: 0.9,
            intent_score: 0.9,
            agent_usefulness_score: 0.9,
            ocr_noise_score: 0.1,
            ..Default::default()
        };
        let outcome = classify_storage_outcome(&record, &cfg);
        assert_eq!(outcome, "primary_memory_card");
    }

    #[test]
    fn storage_outcome_merges_when_dedup_present_but_specificity_low() {
        let cfg = default_memory_quality_config();
        let record = MemoryRecord {
            specificity_score: 0.18,
            intent_score: 0.85,
            agent_usefulness_score: 0.8,
            ocr_noise_score: 0.2,
            dedup_fingerprint: "fndr:capture:ocr".to_string(),
            ..Default::default()
        };
        let outcome = classify_storage_outcome(&record, &cfg);
        assert_eq!(outcome, "merge_into_existing_memory");
    }

    #[test]
    fn storage_outcome_quarantines_when_structured_extraction_is_unavailable_with_zero_grounding() {
        let cfg = default_memory_quality_config();
        let record = MemoryRecord {
            evidence_confidence: 0.0,
            raw_evidence: "{\"extraction_issues\":[\"structured_extraction_unavailable\"]}"
                .to_string(),
            ..Default::default()
        };

        let outcome = classify_storage_outcome(&record, &cfg);
        assert_eq!(outcome, "quarantine_low_grounding");
        assert_eq!(
            quality_gate_reason(&record),
            "hard_gate=structured_extraction_unavailable_with_zero_grounding"
        );
    }

    #[test]
    fn storage_outcome_quarantines_when_ocr_confidence_is_nonzero_but_grounding_is_zero() {
        let cfg = default_memory_quality_config();
        let record = MemoryRecord {
            evidence_confidence: 0.58,
            raw_evidence:
                "{\"extraction_grounding_confidence\":0.0,\"extraction_issues\":[\"structured_extraction_unavailable\"]}"
                    .to_string(),
            ..Default::default()
        };

        let outcome = classify_storage_outcome(&record, &cfg);
        assert_eq!(outcome, "quarantine_low_grounding");
    }

    #[test]
    fn storage_outcome_quarantines_when_memory_context_contains_machine_markers() {
        let cfg = default_memory_quality_config();
        let record = MemoryRecord {
            memory_context: "Continues from abcd1234\nReopen: https://example.com".to_string(),
            ..Default::default()
        };

        let outcome = classify_storage_outcome(&record, &cfg);
        assert_eq!(outcome, "quarantine_polluted_context");
        assert_eq!(
            quality_gate_reason(&record),
            "hard_gate=machine_marker_in_memory_context"
        );
    }

    #[test]
    fn storage_outcome_keeps_failed_visual_semantics_out_of_enriched_cards() {
        let cfg = default_memory_quality_config();
        let record = MemoryRecord {
            storage_outcome: VISUAL_SEMANTICS_FAILED_OUTCOME.to_string(),
            agent_usefulness_score: 1.0,
            intent_score: 1.0,
            specificity_score: 1.0,
            evidence_confidence: 1.0,
            ..Default::default()
        };

        assert_eq!(
            classify_storage_outcome(&record, &cfg),
            VISUAL_SEMANTICS_FAILED_OUTCOME
        );
        assert_eq!(
            quality_gate_reason(&record),
            "hard_gate=visual_semantics_failed"
        );
    }

    #[test]
    fn storage_outcome_keeps_visual_metadata_fallback_as_low_quality_evidence() {
        let cfg = default_memory_quality_config();
        let record = MemoryRecord {
            raw_evidence: r#"{"visual_understanding":{"status":"clip_metadata_fallback"},"extraction_issues":["visual_metadata_fallback"]}"#.to_string(),
            enrichment_status: "visual_metadata_fallback".to_string(),
            agent_usefulness_score: 1.0,
            intent_score: 1.0,
            specificity_score: 1.0,
            evidence_confidence: 1.0,
            ..Default::default()
        };

        assert_eq!(
            classify_storage_outcome(&record, &cfg),
            "low_quality_evidence"
        );
        assert_eq!(
            quality_gate_reason(&record),
            "visual_metadata_fallback=clip_vector_without_pixel_vlm_semantics"
        );
    }

    #[test]
    fn storage_outcome_keeps_empty_ocr_visual_fallback_as_low_quality_evidence() {
        let cfg = default_memory_quality_config();
        let record = MemoryRecord {
            synthesis_branch: "llm_ocr_grounded_visual_fallback".to_string(),
            raw_evidence: r#"{"source_kind":"visual_capture","synthesis_branch":"llm_ocr_grounded_visual_fallback","visual_understanding":{"status":"clip_image_embedding","raw_pixels_persisted":false},"vlm_route":"fallback_ocr_only","vlm_runtime_status":"deferred_low_ram","vlm_capability":"model_missing"}"#.to_string(),
            ocr_confidence: 0.0,
            ocr_block_count: 0,
            agent_usefulness_score: 1.0,
            intent_score: 1.0,
            specificity_score: 1.0,
            evidence_confidence: 1.0,
            ..Default::default()
        };

        assert!(is_low_evidence_visual_fallback_record(&record));
        assert_eq!(
            classify_storage_outcome(&record, &cfg),
            "low_quality_evidence"
        );
        assert_eq!(
            quality_gate_reason(&record),
            LOW_EVIDENCE_VISUAL_FALLBACK_REASON
        );
    }

    #[test]
    fn storage_outcome_allows_ocr_grounded_visual_fallback_with_text_signal() {
        let record = MemoryRecord {
            synthesis_branch: "llm_ocr_grounded_visual_fallback".to_string(),
            raw_evidence: r#"{"synthesis_branch":"llm_ocr_grounded_visual_fallback","visual_understanding":{"status":"clip_image_embedding"},"vlm_route":"fallback_ocr_only"}"#.to_string(),
            ocr_confidence: 0.72,
            ocr_block_count: 4,
            ..Default::default()
        };

        assert!(!is_low_evidence_visual_fallback_record(&record));
    }

    #[test]
    fn failed_visual_semantics_score_caps_prevent_intent_inflation() {
        let mut record = MemoryRecord {
            evidence_confidence: 0.95,
            agent_usefulness_score: 0.95,
            retrieval_value_score: 0.95,
            graph_readiness_score: 0.95,
            specificity_score: 0.95,
            intent_score: 1.0,
            entity_score: 0.8,
            confidence_score: 0.95,
            importance_score: 0.95,
            extraction_confidence: 0.95,
            insight_card_confidence: 0.95,
            ..Default::default()
        };

        cap_visual_semantics_failed_scores(&mut record);

        assert!(record.evidence_confidence <= 0.30);
        assert!(record.agent_usefulness_score <= 0.25);
        assert!(record.retrieval_value_score <= 0.25);
        assert!(record.graph_readiness_score <= 0.15);
        assert!(record.specificity_score <= 0.15);
        assert!(record.intent_score <= 0.10);
        assert_eq!(record.entity_score, 0.0);
    }

    #[test]
    fn fluff_gate_blocks_timestamped_filename_insight() {
        let cfg = default_memory_quality_config();
        let record = MemoryRecord {
            insight_what_happened: "Screen capture (visual): Claude_1778938598807.png. Claude"
                .to_string(),
            app_name: "Claude".to_string(),
            ..Default::default()
        };
        assert!(hard_gate_fluff_insight(&record));
        assert_eq!(
            classify_storage_outcome(&record, &cfg),
            "quarantine_fluff_insight"
        );
    }

    #[test]
    fn fluff_gate_blocks_app_name_tautology() {
        let cfg = default_memory_quality_config();
        let record = MemoryRecord {
            insight_what_happened: "Claude: Claude".to_string(),
            app_name: "Claude".to_string(),
            ..Default::default()
        };
        assert!(hard_gate_fluff_insight(&record));
        assert_eq!(
            classify_storage_outcome(&record, &cfg),
            "quarantine_fluff_insight"
        );
    }

    #[test]
    fn fluff_gate_allows_specific_insight() {
        let record = MemoryRecord {
            insight_what_happened:
                "You debugged a Rust borrow checker error in the embedding pipeline.".to_string(),
            app_name: "Cursor".to_string(),
            ..Default::default()
        };
        assert!(!hard_gate_fluff_insight(&record));
    }

    #[test]
    fn fluff_gate_allows_empty_insight_for_other_gates() {
        // Empty insight is handled by other gates (e.g., structured_extraction_unavailable)
        let record = MemoryRecord {
            insight_what_happened: String::new(),
            ..Default::default()
        };
        assert!(!hard_gate_fluff_insight(&record));
    }

    fn surface_result(
        storage_outcome: &str,
        enrichment_status: &str,
        synthesis_branch: &str,
        ocr_block_count: u32,
        ocr_confidence: f32,
        clean_text: &str,
        display_summary: &str,
    ) -> SearchResult {
        SearchResult {
            id: "m-1".to_string(),
            app_name: "ChatGPT".to_string(),
            storage_outcome: storage_outcome.to_string(),
            enrichment_status: enrichment_status.to_string(),
            synthesis_branch: synthesis_branch.to_string(),
            ocr_block_count,
            ocr_confidence,
            clean_text: clean_text.to_string(),
            display_summary: display_summary.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn low_signal_flags_filename_only_visual_fallback_from_rehearsal() {
        let r = surface_result(
            "low_quality_evidence",
            "",
            "llm_ocr_grounded_visual_fallback",
            1,
            0.62,
            "ChatGPT:",
            "Screen capture (visual) ChatGPT_1789709739566.png. ChatGPT. Also Screen capture (visual) ChatGPT_1789709551930.png. ChatGPT",
        );
        assert_eq!(
            result_low_signal_reason(&r),
            Some(LowSignalReason::ImageOnly)
        );
    }

    #[test]
    fn low_signal_flags_visual_metadata_fallback_without_text() {
        let r = surface_result(
            "low_quality_evidence",
            "visual_metadata_fallback",
            "visual_metadata_fallback",
            0,
            0.0,
            "",
            "",
        );
        assert_eq!(
            result_low_signal_reason(&r),
            Some(LowSignalReason::ImageOnly)
        );
    }

    #[test]
    fn low_signal_flags_visual_semantics_failed_and_quarantine() {
        let failed = surface_result(VISUAL_SEMANTICS_FAILED_OUTCOME, "", "", 0, 0.0, "", "");
        assert_eq!(
            result_low_signal_reason(&failed),
            Some(LowSignalReason::VisualSemanticsFailed)
        );
        let quarantined = surface_result(
            "quarantine_low_grounding",
            "",
            "llm_ocr_grounded",
            6,
            0.9,
            "error[E0502]: cannot borrow `self.buffer` as mutable because it is also borrowed as immutable",
            "Fixed borrow error",
        );
        assert_eq!(
            result_low_signal_reason(&quarantined),
            Some(LowSignalReason::Ungrounded)
        );
    }

    #[test]
    fn low_signal_keeps_high_signal_ocr_memory() {
        let r = surface_result(
            "primary_memory_card",
            "",
            "llm_ocr_grounded",
            9,
            0.94,
            "error[E0502]: cannot borrow `self.frame_buffer` as mutable because it is also borrowed as immutable --> src/capture/mod.rs:412:9",
            "Debugged E0502 borrow error in capture/mod.rs",
        );
        assert_eq!(result_low_signal_reason(&r), None);
    }

    #[test]
    fn low_signal_keeps_ocr_grounded_visual_fallback_with_real_text() {
        let r = surface_result(
            "low_quality_evidence",
            "",
            "llm_ocr_grounded_visual_fallback",
            7,
            0.88,
            "Parent-child chunking with 512-token parents and 128-token children gave the best recall at 10 in our experiments",
            "Read chunking results",
        );
        assert_eq!(result_low_signal_reason(&r), None);
    }

    #[test]
    fn low_signal_keeps_reviewed_and_review_failed_text_memories() {
        let text = "Canvas CS 4500 Alpha Release rubric: most rank 1 features complete, framework for rank 2";
        let reviewed = surface_result(
            "primary_memory_card",
            "reviewed_local",
            "llm_ocr_grounded",
            5,
            0.9,
            text,
            "Read the Alpha rubric",
        );
        assert_eq!(result_low_signal_reason(&reviewed), None);
        let failed_review = surface_result(
            "enriched_memory_card",
            "review_failed",
            "llm_ocr_grounded",
            5,
            0.9,
            text,
            "Read the Alpha rubric",
        );
        assert_eq!(result_low_signal_reason(&failed_review), None);
    }

    #[test]
    fn low_signal_does_not_hide_plain_low_quality_text_memory() {
        let r = surface_result(
            "low_quality_evidence",
            "",
            "llm_ocr_grounded",
            4,
            0.8,
            "Jordan Lee: can we move the alpha dry run to Thursday at 4pm in the MEB lab? I booked room 3147",
            "Dry run moved to Thursday",
        );
        assert_eq!(result_low_signal_reason(&r), None);
    }

    #[test]
    fn partition_surfaceable_splits_and_preserves_order() {
        let good = surface_result(
            "primary_memory_card",
            "",
            "llm_ocr_grounded",
            9,
            0.94,
            "error[E0502]: cannot borrow `self.frame_buffer` as mutable because it is also borrowed as immutable",
            "E0502",
        );
        let bad = surface_result(
            "",
            "visual_metadata_fallback",
            "visual_metadata_fallback",
            0,
            0.0,
            "",
            "",
        );
        let (kept, hidden) = partition_surfaceable(vec![bad.clone(), good.clone(), bad]);
        assert_eq!(kept.len(), 1);
        assert_eq!(hidden.len(), 2);
        assert!(hidden
            .iter()
            .all(|(_, reason)| *reason == LowSignalReason::ImageOnly));
    }
}
