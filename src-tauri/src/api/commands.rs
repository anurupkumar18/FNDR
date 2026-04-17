//! Tauri command handlers

use crate::capture::{
    continuity_anchor_for_memory, eligible_for_story_merge, merge_memory_records_with_policy,
    passes_merge_threshold, score_memory_candidate,
};
use crate::embed::{embedding_runtime_status, Embedder};
use crate::privacy::Blocklist;
use crate::store::MemoryRecord;

use crate::mcp::{self, McpServerStatus};
use crate::meeting::{self, MeetingRecorderStatus, MeetingTranscript};

use crate::search::{HybridSearcher, MemoryCard, MemoryCardSynthesizer};
use crate::speech;
use crate::store::{MeetingSession, SearchResult, Stats, Task, TaskType};
use crate::AppState;
use chrono::{TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Manager, State};
use tokio::time::{timeout, Duration, Instant};

use genpdf::elements;
use genpdf::style;
use genpdf::Element;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureStatus {
    pub is_capturing: bool,
    pub is_paused: bool,
    pub is_incognito: bool,
    pub frames_captured: u64,
    pub frames_dropped: u64,
    pub last_capture_time: u64,
    pub ai_model_available: bool,
    pub ai_model_loaded: bool,
    pub loaded_model_id: Option<String>,
    pub embedding_backend: String,
    pub embedding_degraded: bool,
    pub embedding_detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub time_filter: Option<String>,
    pub app_filter: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceTranscriptionResult {
    pub text: String,
    pub backend: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechSynthesisResult {
    pub audio_path: String,
    pub voice_id: String,
}

static SHARED_EMBEDDER: OnceLock<Result<Embedder, String>> = OnceLock::new();
const BRANCH_LIMIT: usize = 28;
const RERANK_LIMIT: usize = 18;
const GROUP_LIMIT: usize = 6;
const LLM_GROUP_LIMIT: usize = 2;

const EMBED_TIMEOUT: Duration = Duration::from_millis(2200);
const VECTOR_TIMEOUT: Duration = Duration::from_millis(1800);
const KEYWORD_TIMEOUT: Duration = Duration::from_millis(1200);
const SYNTHESIS_TIMEOUT: Duration = Duration::from_millis(2400);
const LLM_SYNTHESIS_TIMEOUT: Duration = Duration::from_millis(1500);
const MEMORY_GRAPH_LIMIT: usize = 1_500;
const TASK_LINK_SCAN_LIMIT: usize = 260;
const TASK_MEETING_LOOKBACK_DAYS: i64 = 14;
const TASK_MEMORY_BACKFILL_LIMIT: usize = 6;
const MEMORY_REPAIR_CHECKPOINT_VERSION: u32 = 2;
const MEMORY_REPAIR_SIMILARITY_SCAN_LIMIT: usize = 96;
const MEMORY_REPAIR_CHECKPOINT_ITEM_STEP: usize = 300;
const MEMORY_REPAIR_CHECKPOINT_MS: u64 = 12_000;
static MEMORY_REPAIR_RUNNING: AtomicBool = AtomicBool::new(false);

fn shared_embedder() -> Result<&'static Embedder, String> {
    match SHARED_EMBEDDER.get_or_init(Embedder::new) {
        Ok(embedder) => Ok(embedder),
        Err(err) => Err(err.clone()),
    }
}

fn is_internal_fndr_result(result: &SearchResult) -> bool {
    Blocklist::is_internal_app(&result.app_name, result.bundle_id.as_deref())
}

fn strip_internal_fndr_results(mut results: Vec<SearchResult>) -> Vec<SearchResult> {
    results.retain(|result| !is_internal_fndr_result(result));
    results
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let head: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{head}...")
    } else {
        head
    }
}

fn card_domain(url: &str) -> Option<String> {
    let no_scheme = url.split("://").nth(1).unwrap_or(url);
    let host = no_scheme.split('/').next()?.trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn is_low_signal_title(title: &str, app_name: &str) -> bool {
    let normalized = title.trim().to_lowercase();
    if normalized.is_empty() {
        return true;
    }

    let app = app_name.trim().to_lowercase();
    if normalized == app || normalized == format!("{app} activity") {
        return true;
    }

    let tokens = normalized.split_whitespace().count();
    if tokens <= 1 {
        return true;
    }

    matches!(
        normalized.as_str(),
        "codex"
            | "cursor"
            | "new chat"
            | "chat"
            | "activity"
            | "home"
            | "dashboard"
            | "new tab"
            | "google chrome"
            | "chrome"
            | "safari"
            | "firefox"
            | "terminal"
            | "finder"
            | "settings"
    )
}

fn is_low_signal_summary(summary: &str, app_name: &str) -> bool {
    let normalized = summary.trim().to_lowercase();
    if normalized.is_empty() {
        return true;
    }

    let app = app_name.trim().to_lowercase();
    if normalized == app {
        return true;
    }

    let words = normalized.split_whitespace().count();
    words <= 2
}

fn title_from_summary(summary: &str, app_name: &str) -> Option<String> {
    let trimmed = summary.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        return None;
    }

    let cleaned = if let Some(rest) = trimmed.strip_prefix("Reviewed ") {
        rest.trim()
    } else if let Some(rest) = trimmed.strip_prefix("reviewed ") {
        rest.trim()
    } else {
        trimmed
    };

    if cleaned.is_empty() {
        return None;
    }

    let candidate = cleaned.to_string();
    if is_low_signal_title(&candidate, app_name) {
        None
    } else {
        Some(candidate)
    }
}

fn card_summary(result: &SearchResult) -> String {
    let snippet = result.snippet.trim();
    let clean = result.clean_text.trim();
    let text = result.text.trim();

    let base = if !snippet.is_empty() && !is_low_signal_summary(snippet, &result.app_name) {
        snippet
    } else if !clean.is_empty() && !is_low_signal_summary(clean, &result.app_name) {
        clean
    } else if !text.is_empty() {
        text
    } else if !snippet.is_empty() {
        snippet
    } else {
        clean
    };

    if base.is_empty() {
        format!("Captured activity in {}", result.app_name)
    } else {
        base.to_string()
    }
}

fn has_continuity_signal(result: &SearchResult) -> bool {
    result.snippet.contains(" • ")
        || result.clean_text.contains(" • ")
        || result.text.contains(" • ")
}

fn card_title(result: &SearchResult, summary: &str) -> String {
    let title = result.window_title.trim();
    if !title.is_empty() {
        let candidate = title.to_string();
        if !is_low_signal_title(&candidate, &result.app_name) {
            return candidate;
        }
    }

    if let Some(from_summary) = title_from_summary(summary, &result.app_name) {
        return from_summary;
    }

    if let Some(domain) = result.url.as_deref().and_then(card_domain) {
        return format!("{} · {}", result.app_name, domain);
    }

    format!("{} memory", result.app_name)
}

fn memory_card_from_result(result: SearchResult) -> MemoryCard {
    let memory_id = result.id.clone();
    let score = result.score;
    let app_name = result.app_name.clone();
    let window_title = result.window_title.clone();
    let url = result.url.clone();
    let summary = card_summary(&result);
    let title = card_title(&result, &summary);
    let mut context = Vec::new();
    if let Some(domain) = url.as_deref().and_then(card_domain) {
        context.push(format!("Site: {}", domain));
    }

    let fallback_snippet = summary.clone();
    let action = if result.url.is_some() {
        "Open source".to_string()
    } else {
        "Revisit context".to_string()
    };
    MemoryCard {
        id: memory_id.clone(),
        title,
        summary,
        action,
        context,
        timestamp: result.timestamp,
        app_name,
        window_title,
        url,
        score,
        source_count: 1,
        continuity: has_continuity_signal(&result),
        raw_snippets: vec![fallback_snippet],
        evidence_ids: vec![memory_id],
        confidence: score.clamp(0.0, 1.0),
    }
}

fn refine_memory_card_title(card: &mut MemoryCard) {
    if !is_low_signal_title(&card.title, &card.app_name) {
        return;
    }

    let window_title = card.window_title.trim();
    if !window_title.is_empty() && !is_low_signal_title(window_title, &card.app_name) {
        card.title = window_title.to_string();
        return;
    }

    if let Some(from_summary) = title_from_summary(&card.summary, &card.app_name) {
        card.title = from_summary;
        return;
    }

    if let Some(domain) = card.url.as_deref().and_then(card_domain) {
        card.title = format!("{} · {}", card.app_name, domain);
        return;
    }

    card.title = format!("{} memory", card.app_name);
}

fn refine_memory_card_titles(cards: &mut [MemoryCard]) {
    for card in cards {
        refine_memory_card_title(card);
    }
}

/// Search for memories
#[tauri::command]
pub async fn search(
    state: State<'_, Arc<AppState>>,
    query: String,
    time_filter: Option<String>,
    app_filter: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, String> {
    let limit = limit.unwrap_or(20).clamp(1, 50);

    // Guard: LanceDB vector_search panics/errors on an empty table.
    // Return empty results immediately so the UI shows "No memories found"
    // instead of a "Search failed" error banner.
    let stats = state.store.get_stats().await.map_err(|e| e.to_string())?;

    if stats.total_records == 0 {
        return Ok(Vec::new());
    }

    let embedder = shared_embedder()?;

    let results = HybridSearcher::search(
        &state.inner().store,
        &embedder,
        &query,
        limit,
        time_filter.as_deref(),
        app_filter.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(strip_internal_fndr_results(results))
}

/// Search and return synthesized memory cards for UI rendering
#[tauri::command]
pub async fn search_memory_cards(
    state: State<'_, Arc<AppState>>,
    query: String,
    time_filter: Option<String>,
    app_filter: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<MemoryCard>, String> {
    let limit = limit.unwrap_or(20).clamp(1, 50);
    let started = Instant::now();
    tracing::info!(
        query = %query,
        time_filter = ?time_filter,
        app_filter = ?app_filter,
        limit,
        "search_memory_cards:start"
    );

    let stats = state.store.get_stats().await.map_err(|e| e.to_string())?;
    if stats.total_records == 0 {
        tracing::info!("search_memory_cards:complete total_ms=0 cards=0");
        return Ok(Vec::new());
    }

    let fallback_cards = |raw_results: &[SearchResult]| {
        MemoryCardSynthesizer::deterministic_from_results(
            &query,
            raw_results,
            limit.min(GROUP_LIMIT),
        )
    };

    tracing::info!("search_memory_cards:embed:start");
    let maybe_query_embedding = match shared_embedder() {
        Ok(embedder) => {
            let query_text = query.clone();
            match timeout(
                EMBED_TIMEOUT,
                tokio::task::spawn_blocking(move || embedder.embed_batch(&[query_text])),
            )
            .await
            {
                Ok(Ok(Ok(vectors))) => vectors.into_iter().next(),
                Ok(Ok(Err(err))) => {
                    tracing::warn!("search_memory_cards:embed:failed err={}", err);
                    None
                }
                Ok(Err(err)) => {
                    tracing::warn!("search_memory_cards:embed:join_failed err={}", err);
                    None
                }
                Err(_) => {
                    tracing::warn!(
                        timeout_ms = EMBED_TIMEOUT.as_millis(),
                        "search_memory_cards:embed:timeout"
                    );
                    None
                }
            }
        }
        Err(err) => {
            tracing::warn!("search_memory_cards:embed:init_failed err={}", err);
            None
        }
    };
    tracing::info!(
        has_embedding = maybe_query_embedding.is_some(),
        "search_memory_cards:embed:done"
    );

    let semantic_results: Vec<SearchResult> = if let Some(query_embedding) = maybe_query_embedding {
        match timeout(
            VECTOR_TIMEOUT,
            state.store.vector_search(
                &query_embedding,
                BRANCH_LIMIT,
                time_filter.as_deref(),
                app_filter.as_deref(),
            ),
        )
        .await
        {
            Ok(Ok(results)) => {
                tracing::info!(
                    count = results.len(),
                    "search_memory_cards:semantic_search:done"
                );
                results
            }
            Ok(Err(err)) => {
                tracing::warn!("search_memory_cards:semantic_search:failed err={}", err);
                Vec::new()
            }
            Err(_) => {
                tracing::warn!(
                    timeout_ms = VECTOR_TIMEOUT.as_millis(),
                    "search_memory_cards:semantic_search:timeout"
                );
                Vec::new()
            }
        }
    } else {
        tracing::info!("search_memory_cards:semantic_search:skipped");
        Vec::new()
    };

    let keyword_results = match timeout(
        KEYWORD_TIMEOUT,
        state.store.keyword_search(
            &query,
            BRANCH_LIMIT,
            time_filter.as_deref(),
            app_filter.as_deref(),
        ),
    )
    .await
    {
        Ok(Ok(results)) => {
            tracing::info!(
                count = results.len(),
                "search_memory_cards:keyword_search:done"
            );
            results
        }
        Ok(Err(err)) => {
            tracing::warn!("search_memory_cards:keyword_search:failed err={}", err);
            Vec::new()
        }
        Err(_) => {
            tracing::warn!(
                timeout_ms = KEYWORD_TIMEOUT.as_millis(),
                "search_memory_cards:keyword_search:timeout"
            );
            Vec::new()
        }
    };

    let mut raw_results =
        HybridSearcher::fuse_and_rerank(&query, &semantic_results, &keyword_results, RERANK_LIMIT);
    raw_results = strip_internal_fndr_results(raw_results);
    raw_results.truncate(RERANK_LIMIT);
    tracing::info!(count = raw_results.len(), "search_memory_cards:rerank:done");
    if raw_results.is_empty() {
        tracing::info!(
            "search_memory_cards:complete total_ms={} cards=0",
            started.elapsed().as_millis()
        );
        return Ok(Vec::new());
    }

    // Never block live search on model loading. If inference isn't already warm,
    // synthesis falls back to deterministic card generation immediately.
    let inference = state.inner().inference_engine();

    tracing::info!("search_memory_cards:synthesis:start");
    let synthesis_future = MemoryCardSynthesizer::from_results_with_policy(
        inference.as_deref(),
        &query,
        &raw_results,
        GROUP_LIMIT,
        LLM_GROUP_LIMIT,
        LLM_SYNTHESIS_TIMEOUT,
    );
    let mut cards = match timeout(SYNTHESIS_TIMEOUT, synthesis_future).await {
        Ok(generated) => {
            tracing::info!(
                count = generated.len(),
                "search_memory_cards:synthesis:done"
            );
            generated
        }
        Err(_) => {
            tracing::warn!(
                timeout_ms = SYNTHESIS_TIMEOUT.as_millis(),
                "search_memory_cards:synthesis:timeout"
            );
            fallback_cards(&raw_results)
        }
    };

    if cards.is_empty() {
        cards = fallback_cards(&raw_results);
    }
    refine_memory_card_titles(&mut cards);
    cards.retain(|card| !Blocklist::is_internal_app(&card.app_name, None));
    cards.truncate(limit);
    tracing::info!(
        total_ms = started.elapsed().as_millis(),
        cards = cards.len(),
        "search_memory_cards:complete"
    );
    Ok(cards)
}

/// List memory cards in newest→oldest order for browsing.
#[tauri::command]
pub async fn list_memory_cards(
    state: State<'_, Arc<AppState>>,
    app_filter: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<MemoryCard>, String> {
    let limit = limit.unwrap_or(MEMORY_GRAPH_LIMIT).clamp(1, 2_000);
    let results = state
        .inner()
        .store
        .list_recent_results(limit, app_filter.as_deref())
        .await
        .map_err(|e| e.to_string())?;

    let mut cards: Vec<MemoryCard> = strip_internal_fndr_results(results)
        .into_iter()
        .map(memory_card_from_result)
        .collect();
    refine_memory_card_titles(&mut cards);
    Ok(cards)
}

#[tauri::command]
pub async fn delete_memory(
    state: State<'_, Arc<AppState>>,
    memory_id: String,
) -> Result<bool, String> {
    let existing = state
        .inner()
        .store
        .get_memory_by_id(&memory_id)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;

    let deleted = state
        .inner()
        .store
        .delete_memory_by_id(&memory_id)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;

    if deleted == 0 {
        return Ok(false);
    }

    if let Some(record) = existing {
        if let Some(path) = record.screenshot_path {
            if let Err(err) = std::fs::remove_file(&path) {
                tracing::warn!("Failed to delete screenshot artifact {}: {}", path, err);
            }
        }
    }

    tracing::info!("Deleted memory record {}", memory_id);
    Ok(true)
}

/// Debug-only raw search path without MemoryCard synthesis.
#[tauri::command]
pub async fn search_raw_results(
    state: State<'_, Arc<AppState>>,
    query: String,
    time_filter: Option<String>,
    app_filter: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, String> {
    search(state, query, time_filter, app_filter, limit).await
}

/// Summarize search results using AI
#[tauri::command]
pub async fn summarize_search(
    _state: State<'_, Arc<AppState>>,
    query: String,
    results_snippets: Vec<String>,
) -> Result<String, String> {
    if results_snippets.is_empty() {
        return Ok(String::new());
    }

    let evidence = parse_summary_evidence(&results_snippets);
    let summary = build_grounded_search_summary(&query, &evidence);
    Ok(summary)
}

#[derive(Debug, Clone)]
struct SummaryEvidence {
    id: String,
    score: f32,
    text: String,
}

fn parse_summary_evidence(snippets: &[String]) -> Vec<SummaryEvidence> {
    let mut evidence = Vec::new();
    for (index, raw) in snippets.iter().enumerate() {
        let id = extract_bracket_value(raw, "id")
            .or_else(|| extract_bracket_value(raw, "memory"))
            .unwrap_or_else(|| format!("result-{}", index + 1));
        let score = extract_bracket_value(raw, "score")
            .and_then(|value| value.parse::<f32>().ok())
            .unwrap_or(0.5);
        let text = strip_bracket_prefixes(raw);
        if text.is_empty() {
            continue;
        }
        evidence.push(SummaryEvidence { id, score, text });
    }
    evidence
}

fn extract_bracket_value(raw: &str, key: &str) -> Option<String> {
    let prefix = format!("[{}:", key);
    let start = raw.find(&prefix)?;
    let rest = &raw[start + prefix.len()..];
    let end = rest.find(']')?;
    let value = rest[..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn strip_bracket_prefixes(raw: &str) -> String {
    let mut remaining = raw.trim();
    while remaining.starts_with('[') {
        let Some(end) = remaining.find(']') else {
            break;
        };
        remaining = remaining[end + 1..].trim_start();
    }
    remaining.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn summary_terms(query: &str) -> Vec<String> {
    query
        .to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|term| term.len() > 1)
        .filter(|term| !summary_stop_word(term))
        .map(|term| term.to_string())
        .collect()
}

fn summary_stop_word(term: &str) -> bool {
    matches!(
        term,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "for"
            | "from"
            | "how"
            | "in"
            | "is"
            | "it"
            | "of"
            | "on"
            | "or"
            | "that"
            | "the"
            | "this"
            | "to"
            | "was"
            | "what"
            | "when"
            | "where"
            | "who"
            | "why"
            | "with"
    )
}

fn evidence_relevance(
    query_terms: &[String],
    query_numbers: &HashSet<String>,
    text: &str,
    score: f32,
) -> f32 {
    let normalized = text.to_lowercase();

    let coverage = if query_terms.is_empty() {
        0.5
    } else {
        query_terms
            .iter()
            .filter(|term| normalized.contains(term.as_str()))
            .count() as f32
            / query_terms.len() as f32
    };

    let number_overlap = if query_numbers.is_empty() {
        0.0
    } else if query_numbers
        .iter()
        .any(|number| normalized.contains(number.as_str()))
    {
        1.0
    } else {
        0.0
    };

    (coverage * 0.58 + score.clamp(0.0, 1.0) * 0.30 + number_overlap * 0.12).clamp(0.0, 1.0)
}

fn clean_summary_fragment(text: &str) -> String {
    truncate_chars(
        &text
            .replace('\n', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim_matches('"')
            .trim_matches('\'')
            .trim()
            .to_string(),
        180,
    )
}

fn ensure_period(sentence: &str) -> String {
    let mut out = sentence.trim().trim_end_matches('.').to_string();
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

fn build_grounded_search_summary(query: &str, evidence: &[SummaryEvidence]) -> String {
    if evidence.is_empty() {
        return "Low confidence: No directly relevant memories found in captured snippets."
            .to_string();
    }

    let query_terms = summary_terms(query);
    let query_numbers = query_terms
        .iter()
        .filter(|term| term.chars().any(|ch| ch.is_ascii_digit()))
        .cloned()
        .collect::<HashSet<_>>();

    let mut scored = evidence
        .iter()
        .map(|item| {
            let relevance =
                evidence_relevance(&query_terms, &query_numbers, &item.text, item.score);
            (item, relevance)
        })
        .collect::<Vec<_>>();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let selected = scored
        .iter()
        .filter(|(_, relevance)| *relevance >= 0.16)
        .take(2)
        .collect::<Vec<_>>();

    if selected.is_empty() {
        return format!(
            "Low confidence: No directly relevant memories found for \"{}\".",
            query.trim()
        );
    }

    let mut fragments = Vec::new();
    let mut confidence = 0.0f32;
    for (item, relevance) in &selected {
        fragments.push(clean_summary_fragment(&item.text));
        confidence += *relevance;
    }
    confidence /= selected.len() as f32;

    let mut summary = ensure_period(
        fragments
            .first()
            .map(|text| text.as_str())
            .unwrap_or("Found related activity"),
    );
    if let Some(second) = fragments.get(1) {
        summary.push_str(" Then ");
        summary.push_str(&ensure_period(second));
    }

    if confidence < 0.45 {
        summary = format!("Low confidence: {}", summary.trim());
    }

    summary
}

/// Get capture status
#[tauri::command]
pub async fn get_status(state: State<'_, Arc<AppState>>) -> Result<CaptureStatus, String> {
    let embed_status = embedding_runtime_status();
    Ok(CaptureStatus {
        is_capturing: state.inner().is_capturing(),
        is_paused: state.inner().is_paused.load(Ordering::SeqCst),
        is_incognito: state.inner().is_incognito.load(Ordering::SeqCst),
        frames_captured: state.inner().frames_captured.load(Ordering::Relaxed),
        frames_dropped: state.inner().frames_dropped.load(Ordering::Relaxed),
        last_capture_time: state.inner().last_capture_time.load(Ordering::Relaxed),
        ai_model_available: state.inner().ai_model_available(),
        ai_model_loaded: state.inner().ai_model_loaded(),
        loaded_model_id: state.inner().loaded_model_id(),
        embedding_backend: embed_status.backend,
        embedding_degraded: embed_status.degraded,
        embedding_detail: embed_status.detail,
    })
}

/// Get MCP server status
#[tauri::command]
pub async fn get_mcp_server_status() -> Result<McpServerStatus, String> {
    Ok(mcp::status())
}

/// Start MCP server (optional custom port)
#[tauri::command]
pub async fn start_mcp_server(
    state: State<'_, Arc<AppState>>,
    port: Option<u16>,
) -> Result<McpServerStatus, String> {
    mcp::start(state.inner().clone(), None, port).await
}

/// Stop MCP server
#[tauri::command]
pub async fn stop_mcp_server() -> Result<McpServerStatus, String> {
    Ok(mcp::stop().await)
}

/// Get meeting recorder status
#[tauri::command]
pub async fn get_meeting_status() -> Result<MeetingRecorderStatus, String> {
    meeting::recorder_status()
}

/// Start a meeting recording session
#[tauri::command]
pub async fn start_meeting_recording(
    app: tauri::AppHandle,
    title: String,
    participants: Option<Vec<String>>,
    model: Option<String>,
) -> Result<MeetingRecorderStatus, String> {
    meeting::start_recording(Some(app), title, participants.unwrap_or_default(), model).await
}

/// Stop the active meeting recording session
#[tauri::command]
pub async fn stop_meeting_recording() -> Result<MeetingRecorderStatus, String> {
    meeting::stop_recording().await
}

/// List all local meeting sessions
#[tauri::command]
pub async fn list_meetings() -> Result<Vec<MeetingSession>, String> {
    meeting::list_meetings().await
}

/// Delete a local meeting session and its persisted artifacts
#[tauri::command]
pub async fn delete_meeting(meeting_id: String) -> Result<bool, String> {
    meeting::delete_meeting(&meeting_id).await
}

/// Get full transcript for a meeting
#[tauri::command]
pub async fn get_meeting_transcript(meeting_id: String) -> Result<MeetingTranscript, String> {
    meeting::get_meeting_transcript(&meeting_id).await
}

/// Export a meeting transcript, summary, and action items to a PDF in the Downloads folder
#[tauri::command]
pub async fn export_meeting_pdf(
    _app: tauri::AppHandle,
    meeting_id: String,
) -> Result<String, String> {
    let transcript = meeting::get_meeting_transcript(&meeting_id).await?;
    let meeting = &transcript.meeting;

    // 1. Resolve Downloads folder
    let downloads_dir = dirs::download_dir()
        .ok_or_else(|| "Could not find Downloads folder on this system.".to_string())?;

    // 2. Prepare filename
    let safe_title = meeting
        .title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect::<String>()
        .replace(' ', "_");

    let date_str = chrono::DateTime::from_timestamp(meeting.start_timestamp / 1000, 0)
        .map(|dt| dt.format("%Y%m%d_%H%M").to_string())
        .unwrap_or_else(|| meeting.id.clone());

    let filename = format!("FNDR_Meeting_{}_{}.pdf", safe_title, date_str);
    let target_path = downloads_dir.join(filename);

    // 3. Generate PDF
    // Use a common macOS font path. Arial is standard in Supplemental.
    let font_dir = std::path::Path::new("/System/Library/Fonts/Supplemental");

    // Load font variants manually since genpdf's from_files helper expects "Arial-Regular.ttf"
    // but macOS uses "Arial.ttf", "Arial Bold.ttf", etc.
    let regular = genpdf::fonts::FontData::load(font_dir.join("Arial.ttf"), None)
        .map_err(|e| format!("Failed to load 'Arial.ttf' from {:?}: {}", font_dir, e))?;
    let bold = genpdf::fonts::FontData::load(font_dir.join("Arial Bold.ttf"), None)
        .map_err(|e| format!("Failed to load 'Arial Bold.ttf' from {:?}: {}", font_dir, e))?;
    let italic =
        genpdf::fonts::FontData::load(font_dir.join("Arial Italic.ttf"), None).map_err(|e| {
            format!(
                "Failed to load 'Arial Italic.ttf' from {:?}: {}",
                font_dir, e
            )
        })?;
    let bold_italic = genpdf::fonts::FontData::load(font_dir.join("Arial Bold Italic.ttf"), None)
        .map_err(|e| {
        format!(
            "Failed to load 'Arial Bold Italic.ttf' from {:?}: {}",
            font_dir, e
        )
    })?;

    let font_family = genpdf::fonts::FontFamily {
        regular,
        bold,
        italic,
        bold_italic,
    };

    let mut doc = genpdf::Document::new(font_family);
    doc.set_title(format!("FNDR Meeting: {}", meeting.title));

    let mut decorator = genpdf::SimplePageDecorator::new();
    decorator.set_margins(18);
    doc.set_page_decorator(decorator);

    // Title & Header
    doc.push(
        elements::Text::new(&meeting.title).styled(style::Style::new().bold().with_font_size(20)),
    );
    let date_label = chrono::DateTime::from_timestamp(meeting.start_timestamp / 1000, 0)
        .map(|dt| dt.format("%B %d, %Y at %H:%M").to_string())
        .unwrap_or_else(|| "Unknown Date".to_string());
    doc.push(
        elements::Text::new(format!("Date: {}", date_label))
            .styled(style::Style::new().with_font_size(10)),
    );
    doc.push(elements::Break::new(1.5));

    // Breakdown Sections
    if let Some(breakdown) = &meeting.breakdown {
        if !breakdown.summary.is_empty() && !breakdown.summary.eq_ignore_ascii_case("none") {
            doc.push(
                elements::Text::new("Summary")
                    .styled(style::Style::new().bold().with_font_size(14)),
            );
            doc.push(elements::Paragraph::new(&breakdown.summary));
            doc.push(elements::Break::new(1.0));
        }

        let filtered_todos: Vec<_> = breakdown
            .todos
            .iter()
            .filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("none"))
            .collect();
        if !filtered_todos.is_empty() {
            doc.push(
                elements::Text::new("To-dos").styled(style::Style::new().bold().with_font_size(14)),
            );
            let mut list = elements::UnorderedList::new();
            for item in filtered_todos {
                list.push(elements::Text::new(item));
            }
            doc.push(list);
            doc.push(elements::Break::new(1.0));
        }

        let filtered_reminders: Vec<_> = breakdown
            .reminders
            .iter()
            .filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("none"))
            .collect();
        if !filtered_reminders.is_empty() {
            doc.push(
                elements::Text::new("Reminders")
                    .styled(style::Style::new().bold().with_font_size(14)),
            );
            let mut list = elements::UnorderedList::new();
            for item in filtered_reminders {
                list.push(elements::Text::new(item));
            }
            doc.push(list);
            doc.push(elements::Break::new(1.0));
        }

        let filtered_followups: Vec<_> = breakdown
            .followups
            .iter()
            .filter(|s| !s.trim().is_empty() && !s.eq_ignore_ascii_case("none"))
            .collect();
        if !filtered_followups.is_empty() {
            doc.push(
                elements::Text::new("Follow-ups")
                    .styled(style::Style::new().bold().with_font_size(14)),
            );
            let mut list = elements::UnorderedList::new();
            for item in filtered_followups {
                list.push(elements::Text::new(item));
            }
            doc.push(list);
            doc.push(elements::Break::new(1.0));
        }
    }

    // Full Transcript
    if !transcript.full_text.is_empty() {
        doc.push(elements::Break::new(0.5));
        doc.push(
            elements::Text::new("Full Transcript")
                .styled(style::Style::new().bold().with_font_size(14)),
        );
        doc.push(elements::Paragraph::new(&transcript.full_text));
    }

    // Save
    doc.render_to_file(&target_path)
        .map_err(|e| format!("Failed to generate PDF file: {}", e))?;

    Ok(target_path.to_string_lossy().to_string())
}

/// Transcribe a short voice input clip for voice search and voice control
#[tauri::command]
pub async fn transcribe_voice_input(
    app: AppHandle,
    audio_bytes: Vec<u8>,
    mime_type: Option<String>,
) -> Result<VoiceTranscriptionResult, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let text =
        speech::transcribe_audio_bytes(&app_data_dir, &audio_bytes, mime_type.as_deref()).await?;

    Ok(VoiceTranscriptionResult {
        text,
        backend: "whisper-small-ggml (enhanced mic mode)".to_string(),
    })
}

/// Synthesize a short spoken response for the FNDR UI
#[tauri::command]
pub async fn speak_text(
    app: AppHandle,
    text: String,
    voice_id: Option<String>,
) -> Result<SpeechSynthesisResult, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let voice_id = voice_id.unwrap_or_else(|| "tara".to_string());
    let audio_path = speech::synthesize_speech(&app_data_dir, &text, Some(&voice_id)).await?;

    Ok(SpeechSynthesisResult {
        audio_path: audio_path.to_string_lossy().to_string(),
        voice_id,
    })
}

/// Pause capture
#[tauri::command]
pub async fn pause_capture(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.inner().pause();
    Ok(())
}

/// Resume capture
#[tauri::command]
pub async fn resume_capture(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.inner().resume();
    Ok(())
}

/// Get blocklist
#[tauri::command]
pub async fn get_blocklist(state: State<'_, Arc<AppState>>) -> Result<Vec<String>, String> {
    let config = state.inner().config.read();
    Ok(config.blocklist.clone())
}

/// Set blocklist
#[tauri::command]
pub async fn set_blocklist(
    state: State<'_, Arc<AppState>>,
    apps: Vec<String>,
) -> Result<(), String> {
    let mut config = state.inner().config.write();
    config.blocklist = apps;
    config
        .save()
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    Ok(())
}

/// Delete all data
#[tauri::command]
pub async fn delete_all_data(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    // 1. Clear memory records
    state
        .inner()
        .store
        .delete_all()
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;

    // 2. Clear knowledge graph
    if let Err(e) = state.inner().graph.clear_all().await {
        tracing::warn!("Failed to clear graph store during delete_all: {}", e);
    }

    // 3. Delete persisted capture artifacts
    for artifact_dir in ["frames", "screenshots", "meetings"] {
        let path = state.inner().store.data_dir().join(artifact_dir);
        if path.exists() {
            if let Err(e) = std::fs::remove_dir_all(&path) {
                tracing::warn!("Failed to remove {} dir: {}", artifact_dir, e);
            }
        }
    }

    tracing::info!("All FNDR data deleted");
    Ok(())
}

/// Get statistics
#[tauri::command]
pub async fn get_stats(state: State<'_, Arc<AppState>>) -> Result<Stats, String> {
    state
        .inner()
        .store
        .get_stats()
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())
}

/// Get retention days (0 = keep forever)
#[tauri::command]
pub async fn get_retention_days(state: State<'_, Arc<AppState>>) -> Result<u32, String> {
    Ok(state.inner().config.read().retention_days)
}

/// Set retention days (0 = keep forever)
#[tauri::command]
pub async fn set_retention_days(state: State<'_, Arc<AppState>>, days: u32) -> Result<(), String> {
    let mut config = state.inner().config.write();
    config.retention_days = days;
    config
        .save()
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    Ok(())
}

/// Get unique app names for filter dropdown
#[tauri::command]
pub async fn get_app_names(state: State<'_, Arc<AppState>>) -> Result<Vec<String>, String> {
    let mut apps = state
        .inner()
        .store
        .get_app_names()
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    apps.retain(|name| !Blocklist::is_internal_app(name, None));
    Ok(apps)
}

/// Delete records older than the given number of days; returns count deleted
#[tauri::command]
pub async fn delete_older_than(
    state: State<'_, Arc<AppState>>,
    days: u32,
) -> Result<usize, String> {
    state
        .inner()
        .store
        .delete_older_than(days)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())
}

fn merge_bucket_for_anchor(anchor: Option<&str>, app_name: &str) -> &'static str {
    if let Some(anchor) = anchor {
        let lower = anchor.to_lowercase();
        if lower.starts_with("spotify:") || lower.contains("spotify") {
            return "spotify";
        }
        if lower.starts_with("youtube:") || lower.contains("youtube") {
            return "youtube";
        }
        if lower.starts_with("codex:") || lower.contains("codex") || lower.contains("cursor") {
            return "codex";
        }
        if lower.starts_with("discord:") || lower.contains("discord") {
            return "discord";
        }
        if lower.starts_with("gitlab:") || lower.contains("gitlab") {
            return "gitlab";
        }
        if lower.starts_with("antigravity:") || lower.contains("antigravity") {
            return "antigravity";
        }
    }

    let app = app_name.to_lowercase();
    if app.contains("spotify") {
        return "spotify";
    }
    if app.contains("youtube") {
        return "youtube";
    }
    if app.contains("codex") || app.contains("cursor") {
        return "codex";
    }
    if app.contains("discord") {
        return "discord";
    }
    if app.contains("gitlab") {
        return "gitlab";
    }
    if app.contains("antigravity") {
        return "antigravity";
    }
    "generic"
}

#[tauri::command]
pub async fn run_memory_repair_backfill(
    state: State<'_, Arc<AppState>>,
) -> Result<MemoryRepairSummary, String> {
    if MEMORY_REPAIR_RUNNING.swap(true, Ordering::AcqRel) {
        return Err("Memory continuity repair is already running".to_string());
    }
    struct MemoryRepairRunGuard;
    impl Drop for MemoryRepairRunGuard {
        fn drop(&mut self) {
            MEMORY_REPAIR_RUNNING.store(false, Ordering::Release);
        }
    }
    let _run_guard = MemoryRepairRunGuard;

    let progress_path = memory_repair_progress_path(state.inner());
    let checkpoint_path = memory_repair_checkpoint_path(state.inner());
    let mut all_memories = state
        .inner()
        .store
        .list_all_memories()
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;

    if all_memories.is_empty() {
        let _ = std::fs::remove_file(&checkpoint_path);
        persist_memory_repair_progress(
            &progress_path,
            &MemoryRepairProgress {
                is_running: false,
                phase: "complete".to_string(),
                processed: 0,
                total: 0,
                merged_count: 0,
                anchor_merges: 0,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
            },
        );
        return Ok(MemoryRepairSummary {
            total_before: 0,
            total_after: 0,
            merged_count: 0,
            anchor_merges: 0,
            task_reference_updates: 0,
            screenshots_cleaned: 0,
            spotify_merges: 0,
            youtube_merges: 0,
            codex_merges: 0,
            discord_merges: 0,
            gitlab_merges: 0,
            antigravity_merges: 0,
            app_merges: Vec::new(),
        });
    }

    all_memories.sort_by_key(|memory| memory.timestamp);
    let before_count = all_memories.len();
    let source_fingerprint = memory_repair_source_fingerprint(&all_memories);
    let source_first_id = all_memories
        .first()
        .map(|memory| memory.id.clone())
        .unwrap_or_default();
    let source_last_id = all_memories
        .last()
        .map(|memory| memory.id.clone())
        .unwrap_or_default();

    let before_screenshots: HashSet<String> = all_memories
        .iter()
        .filter_map(|memory| memory.screenshot_path.clone())
        .collect();

    let embedder = shared_embedder()?;
    let backfill_engine: Option<&Arc<crate::inference::InferenceEngine>> = None;

    let mut merged_memories: Vec<MemoryRecord> = Vec::with_capacity(before_count);
    let mut anchor_index: HashMap<String, usize> = HashMap::new();
    let mut app_index: HashMap<String, Vec<usize>> = HashMap::new();
    let mut id_redirect: HashMap<String, String> = HashMap::new();
    let mut processed = 0usize;
    let mut resumed_from_checkpoint = false;

    let mut merged_count = 0usize;
    let mut anchor_merges = 0usize;
    let mut spotify_merges = 0usize;
    let mut youtube_merges = 0usize;
    let mut codex_merges = 0usize;
    let mut discord_merges = 0usize;
    let mut gitlab_merges = 0usize;
    let mut antigravity_merges = 0usize;
    let mut app_merge_counts: HashMap<String, usize> = HashMap::new();

    if let Some(checkpoint) = load_memory_repair_checkpoint(&checkpoint_path) {
        let checkpoint_valid = (checkpoint.version == MEMORY_REPAIR_CHECKPOINT_VERSION
            || checkpoint.version == 1)
            && checkpoint.source_total == before_count
            && checkpoint.source_fingerprint == source_fingerprint
            && checkpoint.source_first_id == source_first_id
            && checkpoint.source_last_id == source_last_id
            && checkpoint.processed <= before_count
            && checkpoint.merged_memories.len() <= checkpoint.processed
            && checkpoint.id_redirect.len() <= checkpoint.processed;

        if checkpoint_valid {
            merged_memories = checkpoint.merged_memories;
            id_redirect = checkpoint.id_redirect;
            processed = checkpoint.processed;
            merged_count = checkpoint.merged_count;
            anchor_merges = checkpoint.anchor_merges;
            spotify_merges = checkpoint.spotify_merges;
            youtube_merges = checkpoint.youtube_merges;
            codex_merges = checkpoint.codex_merges;
            discord_merges = checkpoint.discord_merges;
            gitlab_merges = checkpoint.gitlab_merges;
            antigravity_merges = checkpoint.antigravity_merges;
            app_merge_counts = checkpoint.app_merge_counts;

            for (index, memory) in merged_memories.iter().enumerate() {
                if let Some(anchor) = continuity_anchor_for_memory(memory) {
                    anchor_index.insert(anchor, index);
                }
                app_index
                    .entry(memory.app_name.to_lowercase())
                    .or_default()
                    .push(index);
            }

            resumed_from_checkpoint = true;
            tracing::info!(
                "memory_repair_backfill: resumed from checkpoint at {}/{}",
                processed,
                before_count
            );
        } else {
            let _ = std::fs::remove_file(&checkpoint_path);
        }
    }

    persist_memory_repair_progress(
        &progress_path,
        &MemoryRepairProgress {
            is_running: true,
            phase: if resumed_from_checkpoint {
                "resuming".to_string()
            } else {
                "scanning".to_string()
            },
            processed,
            total: before_count,
            merged_count,
            anchor_merges,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        },
    );

    let mut last_heartbeat = Instant::now();
    let heartbeat_interval = Duration::from_secs(1);
    let heartbeat_count_step = 75usize;
    let checkpoint_interval = Duration::from_millis(MEMORY_REPAIR_CHECKPOINT_MS);
    let mut last_checkpoint = Instant::now();

    for incoming in all_memories.into_iter().skip(processed) {
        processed += 1;
        let incoming_id = incoming.id.clone();
        let normalized_app = incoming.app_name.to_lowercase();
        let incoming_anchor = continuity_anchor_for_memory(&incoming);
        let mut merged_into_idx: Option<usize> = None;

        if eligible_for_story_merge(&incoming) {
            if let Some(anchor) = incoming_anchor.as_ref() {
                if let Some(index) = anchor_index.get(anchor).copied() {
                    if merged_memories
                        .get(index)
                        .map(|existing| existing.app_name == incoming.app_name)
                        .unwrap_or(false)
                    {
                        merged_into_idx = Some(index);
                        anchor_merges += 1;
                    }
                }
            }

            if merged_into_idx.is_none() {
                if let Some(candidates) = app_index.get(&normalized_app) {
                    let mut best: Option<(usize, f32)> = None;
                    for candidate_index in candidates
                        .iter()
                        .rev()
                        .take(MEMORY_REPAIR_SIMILARITY_SCAN_LIMIT)
                    {
                        let existing = &merged_memories[*candidate_index];
                        let score = score_memory_candidate(&incoming, existing);
                        if !passes_merge_threshold(score) {
                            continue;
                        }
                        if best
                            .as_ref()
                            .map(|(_, best_score)| score.score > *best_score)
                            .unwrap_or(true)
                        {
                            best = Some((*candidate_index, score.score));
                        }
                    }
                    merged_into_idx = best.map(|(index, _)| index);
                }
            }
        }

        if let Some(target_index) = merged_into_idx {
            let existing_id = merged_memories[target_index].id.clone();
            let merged = merge_memory_records_with_policy(
                merged_memories[target_index].clone(),
                incoming.clone(),
                embedder,
                backfill_engine,
                false,
                false,
            )
            .await;
            merged_memories[target_index] = merged.clone();
            id_redirect.insert(incoming_id, existing_id);
            merged_count += 1;

            let merge_bucket =
                merge_bucket_for_anchor(incoming_anchor.as_deref(), &incoming.app_name);
            match merge_bucket {
                "spotify" => spotify_merges += 1,
                "youtube" => youtube_merges += 1,
                "codex" => codex_merges += 1,
                "discord" => discord_merges += 1,
                "gitlab" => gitlab_merges += 1,
                "antigravity" => antigravity_merges += 1,
                _ => {}
            }
            *app_merge_counts
                .entry(incoming.app_name.clone())
                .or_insert(0) += 1;

            if let Some(anchor) = continuity_anchor_for_memory(&merged) {
                anchor_index.insert(anchor, target_index);
            }
            if processed % heartbeat_count_step == 0
                || last_heartbeat.elapsed() >= heartbeat_interval
            {
                tracing::info!(
                    "memory_repair_backfill:progress processed={} total={} merged={} anchor_merges={}",
                    processed,
                    before_count,
                    merged_count,
                    anchor_merges
                );
                persist_memory_repair_progress(
                    &progress_path,
                    &MemoryRepairProgress {
                        is_running: true,
                        phase: "scanning".to_string(),
                        processed,
                        total: before_count,
                        merged_count,
                        anchor_merges,
                        timestamp_ms: chrono::Utc::now().timestamp_millis(),
                    },
                );
                last_heartbeat = Instant::now();
            }

            if processed % MEMORY_REPAIR_CHECKPOINT_ITEM_STEP == 0
                || last_checkpoint.elapsed() >= checkpoint_interval
            {
                persist_memory_repair_checkpoint(
                    &checkpoint_path,
                    &MemoryRepairCheckpoint {
                        version: MEMORY_REPAIR_CHECKPOINT_VERSION,
                        source_total: before_count,
                        source_fingerprint,
                        source_first_id: source_first_id.clone(),
                        source_last_id: source_last_id.clone(),
                        processed,
                        merged_memories: merged_memories.clone(),
                        id_redirect: id_redirect.clone(),
                        merged_count,
                        anchor_merges,
                        spotify_merges,
                        youtube_merges,
                        codex_merges,
                        discord_merges,
                        gitlab_merges,
                        antigravity_merges,
                        app_merge_counts: app_merge_counts.clone(),
                    },
                );
                last_checkpoint = Instant::now();
            }
            continue;
        }

        let index = merged_memories.len();
        if let Some(anchor) = incoming_anchor {
            anchor_index.insert(anchor, index);
        }
        app_index.entry(normalized_app).or_default().push(index);
        merged_memories.push(incoming);

        if processed % heartbeat_count_step == 0 || last_heartbeat.elapsed() >= heartbeat_interval {
            tracing::info!(
                "memory_repair_backfill:progress processed={} total={} merged={} anchor_merges={}",
                processed,
                before_count,
                merged_count,
                anchor_merges
            );
            persist_memory_repair_progress(
                &progress_path,
                &MemoryRepairProgress {
                    is_running: true,
                    phase: "scanning".to_string(),
                    processed,
                    total: before_count,
                    merged_count,
                    anchor_merges,
                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                },
            );
            last_heartbeat = Instant::now();
        }

        if processed % MEMORY_REPAIR_CHECKPOINT_ITEM_STEP == 0
            || last_checkpoint.elapsed() >= checkpoint_interval
        {
            persist_memory_repair_checkpoint(
                &checkpoint_path,
                &MemoryRepairCheckpoint {
                    version: MEMORY_REPAIR_CHECKPOINT_VERSION,
                    source_total: before_count,
                    source_fingerprint,
                    source_first_id: source_first_id.clone(),
                    source_last_id: source_last_id.clone(),
                    processed,
                    merged_memories: merged_memories.clone(),
                    id_redirect: id_redirect.clone(),
                    merged_count,
                    anchor_merges,
                    spotify_merges,
                    youtube_merges,
                    codex_merges,
                    discord_merges,
                    gitlab_merges,
                    antigravity_merges,
                    app_merge_counts: app_merge_counts.clone(),
                },
            );
            last_checkpoint = Instant::now();
        }
    }

    persist_memory_repair_progress(
        &progress_path,
        &MemoryRepairProgress {
            is_running: true,
            phase: "writing".to_string(),
            processed: before_count,
            total: before_count,
            merged_count,
            anchor_merges,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        },
    );
    persist_memory_repair_checkpoint(
        &checkpoint_path,
        &MemoryRepairCheckpoint {
            version: MEMORY_REPAIR_CHECKPOINT_VERSION,
            source_total: before_count,
            source_fingerprint,
            source_first_id: source_first_id.clone(),
            source_last_id: source_last_id.clone(),
            processed: before_count,
            merged_memories: merged_memories.clone(),
            id_redirect: id_redirect.clone(),
            merged_count,
            anchor_merges,
            spotify_merges,
            youtube_merges,
            codex_merges,
            discord_merges,
            gitlab_merges,
            antigravity_merges,
            app_merge_counts: app_merge_counts.clone(),
        },
    );

    if let Err(err) = state.inner().store.delete_all().await {
        persist_memory_repair_progress(
            &progress_path,
            &MemoryRepairProgress {
                is_running: false,
                phase: "error".to_string(),
                processed,
                total: before_count,
                merged_count,
                anchor_merges,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
            },
        );
        persist_memory_repair_checkpoint(
            &checkpoint_path,
            &MemoryRepairCheckpoint {
                version: MEMORY_REPAIR_CHECKPOINT_VERSION,
                source_total: before_count,
                source_fingerprint,
                source_first_id: source_first_id.clone(),
                source_last_id: source_last_id.clone(),
                processed,
                merged_memories,
                id_redirect,
                merged_count,
                anchor_merges,
                spotify_merges,
                youtube_merges,
                codex_merges,
                discord_merges,
                gitlab_merges,
                antigravity_merges,
                app_merge_counts,
            },
        );
        return Err(err.to_string());
    }
    if let Err(err) = state.inner().store.add_batch(&merged_memories).await {
        persist_memory_repair_progress(
            &progress_path,
            &MemoryRepairProgress {
                is_running: false,
                phase: "error".to_string(),
                processed,
                total: before_count,
                merged_count,
                anchor_merges,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
            },
        );
        persist_memory_repair_checkpoint(
            &checkpoint_path,
            &MemoryRepairCheckpoint {
                version: MEMORY_REPAIR_CHECKPOINT_VERSION,
                source_total: before_count,
                source_fingerprint,
                source_first_id: source_first_id.clone(),
                source_last_id: source_last_id.clone(),
                processed,
                merged_memories,
                id_redirect,
                merged_count,
                anchor_merges,
                spotify_merges,
                youtube_merges,
                codex_merges,
                discord_merges,
                gitlab_merges,
                antigravity_merges,
                app_merge_counts,
            },
        );
        return Err(err.to_string());
    }

    let after_screenshots: HashSet<String> = merged_memories
        .iter()
        .filter_map(|memory| memory.screenshot_path.clone())
        .collect();
    let screenshots_cleaned = before_screenshots
        .difference(&after_screenshots)
        .filter(|path| std::fs::remove_file(path).is_ok())
        .count();

    let mut task_reference_updates = 0usize;
    let mut tasks = state
        .inner()
        .store
        .list_tasks()
        .await
        .map_err(|e| e.to_string())?;
    for task in &mut tasks {
        if let Some(source_id) = task.source_memory_id.clone() {
            if let Some(new_id) = id_redirect.get(&source_id) {
                if new_id != &source_id {
                    task.source_memory_id = Some(new_id.clone());
                    task_reference_updates += 1;
                }
            }
        }

        if !task.linked_memory_ids.is_empty() {
            let before = task.linked_memory_ids.clone();
            let mut seen = HashSet::new();
            let rewritten: Vec<String> = before
                .iter()
                .map(|memory_id| {
                    id_redirect
                        .get(memory_id)
                        .cloned()
                        .unwrap_or_else(|| memory_id.clone())
                })
                .filter(|memory_id| seen.insert(memory_id.clone()))
                .collect();
            if rewritten != before {
                task_reference_updates += before
                    .iter()
                    .zip(rewritten.iter())
                    .filter(|(left, right)| left != right)
                    .count()
                    + before.len().saturating_sub(rewritten.len());
                task.linked_memory_ids = rewritten;
            }
        }
    }

    if task_reference_updates > 0 {
        if let Err(err) = state.inner().store.upsert_tasks(&tasks).await {
            persist_memory_repair_progress(
                &progress_path,
                &MemoryRepairProgress {
                    is_running: false,
                    phase: "error".to_string(),
                    processed: before_count,
                    total: before_count,
                    merged_count,
                    anchor_merges,
                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                },
            );
            persist_memory_repair_checkpoint(
                &checkpoint_path,
                &MemoryRepairCheckpoint {
                    version: MEMORY_REPAIR_CHECKPOINT_VERSION,
                    source_total: before_count,
                    source_fingerprint,
                    source_first_id: source_first_id.clone(),
                    source_last_id: source_last_id.clone(),
                    processed: before_count,
                    merged_memories,
                    id_redirect,
                    merged_count,
                    anchor_merges,
                    spotify_merges,
                    youtube_merges,
                    codex_merges,
                    discord_merges,
                    gitlab_merges,
                    antigravity_merges,
                    app_merge_counts,
                },
            );
            return Err(err.to_string());
        }
    }

    persist_memory_repair_progress(
        &progress_path,
        &MemoryRepairProgress {
            is_running: false,
            phase: "complete".to_string(),
            processed: before_count,
            total: before_count,
            merged_count,
            anchor_merges,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        },
    );
    let _ = std::fs::remove_file(&checkpoint_path);

    let mut app_merges: Vec<AppMergeCount> = app_merge_counts
        .into_iter()
        .map(|(app_name, merged)| AppMergeCount { app_name, merged })
        .collect();
    app_merges.sort_by(|left, right| right.merged.cmp(&left.merged));

    Ok(MemoryRepairSummary {
        total_before: before_count,
        total_after: merged_memories.len(),
        merged_count,
        anchor_merges,
        task_reference_updates,
        screenshots_cleaned,
        spotify_merges,
        youtube_merges,
        codex_merges,
        discord_merges,
        gitlab_merges,
        antigravity_merges,
        app_merges,
    })
}

// ========== Task Commands ==========

fn task_type_sort_key(task_type: &TaskType) -> u8 {
    match task_type {
        TaskType::Todo => 0,
        TaskType::Reminder => 1,
        TaskType::Followup => 2,
    }
}

fn parse_task_type(task_type: Option<&str>) -> TaskType {
    match task_type {
        Some("Reminder") => TaskType::Reminder,
        Some("Followup") => TaskType::Followup,
        _ => TaskType::Todo,
    }
}

fn normalize_task_text(value: &str) -> String {
    value
        .to_lowercase()
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn task_terms(title: &str) -> Vec<String> {
    normalize_task_text(title)
        .split_whitespace()
        .filter(|term| term.len() >= 3)
        .filter(|term| {
            !matches!(
                *term,
                "the"
                    | "and"
                    | "for"
                    | "with"
                    | "from"
                    | "that"
                    | "this"
                    | "todo"
                    | "task"
                    | "follow"
                    | "followup"
                    | "reminder"
            )
        })
        .map(|term| term.to_string())
        .collect()
}

fn is_manual_task(task: &Task) -> bool {
    task.source_app.eq_ignore_ascii_case("manual")
}

fn is_meeting_task(task: &Task) -> bool {
    task.source_app.starts_with("Meeting:")
}

fn is_memory_task(task: &Task) -> bool {
    task.source_app.starts_with("Memory:") || task.source_app.eq_ignore_ascii_case("auto")
}

fn task_has_supporting_context(task: &Task) -> bool {
    task.source_memory_id.is_some()
        || !task.linked_memory_ids.is_empty()
        || !task.linked_urls.is_empty()
}

fn is_low_signal_task_title(title: &str) -> bool {
    let normalized = normalize_task_text(title);
    if normalized.len() < 6 {
        return true;
    }
    if normalized.split_whitespace().count() < 2 {
        return true;
    }

    if matches!(
        normalized.as_str(),
        "todo"
            | "to do"
            | "task"
            | "follow up"
            | "followup"
            | "reminder"
            | "none"
            | "n a"
            | "check this"
            | "look into this"
            | "work on this"
    ) {
        return true;
    }

    let generic_prefixes = [
        "complete ",
        "remember ",
        "follow up ",
        "followup ",
        "work on ",
        "check ",
        "look into ",
    ];
    generic_prefixes
        .iter()
        .any(|prefix| normalized.starts_with(prefix) && normalized.split_whitespace().count() <= 3)
}

fn task_priority_score(task: &Task, now_ms: i64) -> i64 {
    let age_hours = ((now_ms - task.created_at).max(0) / 3_600_000) as i64;
    let recency_bonus = (96 - age_hours).clamp(0, 96);
    let memory_bonus = (task.linked_memory_ids.len().min(10) as i64) * 4;
    let url_bonus = (task.linked_urls.len().min(6) as i64) * 2;
    let due_bonus = if task.due_date.is_some() { 12 } else { 0 };
    let source_bonus = if is_manual_task(task) {
        22
    } else if is_meeting_task(task) {
        16
    } else if is_memory_task(task) {
        11
    } else {
        6
    };
    let context_penalty =
        if !task_has_supporting_context(task) && !is_manual_task(task) && !is_meeting_task(task) {
            18
        } else {
            0
        };
    let title_penalty = if is_low_signal_task_title(&task.title) {
        24
    } else {
        0
    };
    recency_bonus + memory_bonus + url_bonus + due_bonus + source_bonus
        - context_penalty
        - title_penalty
}

fn required_term_matches(terms: &[String]) -> usize {
    if terms.len() >= 4 {
        2
    } else {
        1
    }
}

fn update_task_links_from_memories(tasks: &mut [Task], recent_memories: &[SearchResult]) -> bool {
    let mut changed = false;

    for task in tasks
        .iter_mut()
        .filter(|t| !t.is_completed && !t.is_dismissed)
    {
        let terms = task_terms(&task.title);
        if terms.is_empty() {
            continue;
        }

        let mut known_memory_ids: HashSet<String> =
            task.linked_memory_ids.iter().cloned().collect();
        let mut known_urls: HashSet<String> = task.linked_urls.iter().cloned().collect();

        for memory in recent_memories.iter().take(TASK_LINK_SCAN_LIMIT) {
            let blob = format!(
                "{} {} {}",
                memory.snippet, memory.clean_text, memory.window_title
            );
            let normalized_blob = normalize_task_text(&blob);
            let matches = terms
                .iter()
                .filter(|term| normalized_blob.contains(term.as_str()))
                .count();
            let required = required_term_matches(&terms);
            if matches < required {
                continue;
            }

            if known_memory_ids.insert(memory.id.clone()) {
                task.linked_memory_ids.push(memory.id.clone());
                changed = true;
            }

            if let Some(url) = memory.url.clone() {
                if known_urls.insert(url.clone()) {
                    task.linked_urls.push(url);
                    changed = true;
                }
            }
        }

        task.linked_memory_ids.truncate(18);
        task.linked_urls.truncate(8);
    }

    changed
}

fn backfill_tasks_from_meetings(tasks: &mut Vec<Task>, meetings: &[MeetingSession]) -> bool {
    let mut changed = false;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let cutoff = now_ms - (TASK_MEETING_LOOKBACK_DAYS * 24 * 60 * 60 * 1000);
    let mut dedupe = tasks
        .iter()
        .map(|task| {
            (
                normalize_task_text(&task.title),
                task_type_sort_key(&task.task_type),
            )
        })
        .collect::<HashSet<_>>();

    let mut ordered = meetings.iter().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        right
            .end_timestamp
            .unwrap_or(right.updated_at)
            .cmp(&left.end_timestamp.unwrap_or(left.updated_at))
    });

    for meeting in ordered {
        let event_ts = meeting.end_timestamp.unwrap_or(meeting.updated_at);
        if event_ts < cutoff {
            continue;
        }
        let Some(breakdown) = meeting.breakdown.as_ref() else {
            continue;
        };
        let source_app = format!("Meeting:{}", meeting.id);

        let mut add_items = |items: &[String], task_type: TaskType| {
            let type_key = task_type_sort_key(&task_type);
            for item in items {
                let title = item.trim();
                if title.is_empty() || is_low_signal_task_title(title) {
                    continue;
                }
                let dedupe_key = (normalize_task_text(title), type_key);
                if !dedupe.insert(dedupe_key) {
                    continue;
                }
                tasks.push(Task {
                    id: uuid::Uuid::new_v4().to_string(),
                    title: title.to_string(),
                    description: String::new(),
                    source_app: source_app.clone(),
                    source_memory_id: None,
                    created_at: event_ts,
                    due_date: None,
                    is_completed: false,
                    is_dismissed: false,
                    task_type: task_type.clone(),
                    linked_urls: Vec::new(),
                    linked_memory_ids: Vec::new(),
                });
                changed = true;
            }
        };

        add_items(&breakdown.todos, TaskType::Todo);
        add_items(&breakdown.reminders, TaskType::Reminder);
        add_items(&breakdown.followups, TaskType::Followup);
    }

    changed
}

#[derive(Debug, Clone)]
struct MemoryTaskCandidate {
    title: String,
    task_type: TaskType,
    score: i64,
    created_at: i64,
    source_app: String,
    source_memory_id: String,
    linked_urls: Vec<String>,
}

fn first_sentence(text: &str) -> String {
    text.split(['.', '!', '?'])
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .take(18)
        .collect::<Vec<_>>()
        .join(" ")
}

fn classify_task_type_from_text(text: &str) -> TaskType {
    let lower = text.to_lowercase();
    if [
        "follow up",
        "follow-up",
        "reply to",
        "reach out",
        "check in with",
        "ping ",
    ]
    .iter()
    .any(|cue| lower.contains(cue))
    {
        TaskType::Followup
    } else if [
        "tomorrow",
        "today",
        "tonight",
        "next week",
        "next month",
        "deadline",
        "due ",
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
    ]
    .iter()
    .any(|cue| lower.contains(cue))
    {
        TaskType::Reminder
    } else {
        TaskType::Todo
    }
}

fn build_memory_task_candidate(memory: &SearchResult) -> Option<MemoryTaskCandidate> {
    if is_internal_fndr_result(memory) {
        return None;
    }

    let mut text = memory.snippet.trim().to_string();
    if text.is_empty() {
        text = memory.clean_text.trim().to_string();
    }
    if text.is_empty() {
        text = memory.window_title.trim().to_string();
    }
    if text.is_empty() {
        return None;
    }

    let sentence = first_sentence(&text);
    if sentence.is_empty() {
        return None;
    }

    let cleaned = sentence
        .trim()
        .trim_matches(|ch| ch == '"' || ch == '\'' || ch == '`')
        .trim_start_matches("TODO:")
        .trim_start_matches("To do:")
        .trim_start_matches("to do:")
        .trim()
        .to_string();
    if cleaned.is_empty() || is_low_signal_task_title(&cleaned) {
        return None;
    }

    let lower = cleaned.to_lowercase();
    let action_cues = [
        "need to",
        "should",
        "must",
        "todo",
        "to do",
        "action item",
        "send",
        "reply",
        "schedule",
        "book",
        "finish",
        "complete",
        "prepare",
        "submit",
        "review",
        "update",
        "fix",
        "call",
        "email",
        "draft",
        "plan",
        "confirm",
        "deploy",
        "ship",
        "follow up",
    ];
    let action_hits = action_cues
        .iter()
        .filter(|cue| lower.contains(*cue))
        .count() as i64;
    if action_hits == 0 {
        return None;
    }

    let reminder_hits = [
        "tomorrow",
        "today",
        "tonight",
        "next week",
        "next month",
        "deadline",
        "due ",
    ]
    .iter()
    .filter(|cue| lower.contains(*cue))
    .count() as i64;
    let followup_hits = [
        "follow up",
        "follow-up",
        "reply to",
        "reach out",
        "check in with",
    ]
    .iter()
    .filter(|cue| lower.contains(*cue))
    .count() as i64;
    let score = action_hits * 4 + reminder_hits * 3 + followup_hits * 4;
    if score < 4 {
        return None;
    }

    Some(MemoryTaskCandidate {
        title: cleaned,
        task_type: classify_task_type_from_text(&lower),
        score,
        created_at: memory.timestamp,
        source_app: format!("Memory:{}", memory.app_name),
        source_memory_id: memory.id.clone(),
        linked_urls: memory.url.clone().map(|url| vec![url]).unwrap_or_default(),
    })
}

fn backfill_tasks_from_memories(tasks: &mut Vec<Task>, recent_memories: &[SearchResult]) -> bool {
    let mut changed = false;
    let mut dedupe = tasks
        .iter()
        .map(|task| {
            (
                normalize_task_text(&task.title),
                task_type_sort_key(&task.task_type),
            )
        })
        .collect::<HashSet<_>>();

    let mut candidates = recent_memories
        .iter()
        .filter_map(build_memory_task_candidate)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| right.created_at.cmp(&left.created_at))
    });

    for candidate in candidates.into_iter().take(TASK_MEMORY_BACKFILL_LIMIT) {
        let type_key = task_type_sort_key(&candidate.task_type);
        let dedupe_key = (normalize_task_text(&candidate.title), type_key);
        if !dedupe.insert(dedupe_key) {
            continue;
        }

        tasks.push(Task {
            id: uuid::Uuid::new_v4().to_string(),
            title: candidate.title,
            description: String::new(),
            source_app: candidate.source_app,
            source_memory_id: Some(candidate.source_memory_id.clone()),
            created_at: candidate.created_at,
            due_date: None,
            is_completed: false,
            is_dismissed: false,
            task_type: candidate.task_type.clone(),
            linked_urls: candidate.linked_urls,
            linked_memory_ids: vec![candidate.source_memory_id],
        });
        changed = true;
    }

    changed
}

fn dismiss_low_quality_auto_tasks(tasks: &mut [Task]) -> bool {
    let mut changed = false;

    for task in tasks
        .iter_mut()
        .filter(|task| !task.is_completed && !task.is_dismissed)
    {
        if is_manual_task(task) || is_meeting_task(task) {
            continue;
        }

        let stale_auto_seed = task.source_app.eq_ignore_ascii_case("auto");
        let weak_title = is_low_signal_task_title(&task.title);
        let missing_context = !task_has_supporting_context(task);
        if stale_auto_seed || weak_title || (is_memory_task(task) && missing_context) {
            task.is_dismissed = true;
            changed = true;
        }
    }

    changed
}

/// Add a new todo
#[tauri::command]
pub async fn add_todo(
    state: State<'_, Arc<AppState>>,
    title: String,
    task_type: Option<String>,
) -> Result<Task, String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("Task title cannot be empty.".to_string());
    }

    let parsed_task_type = parse_task_type(task_type.as_deref());

    let task = Task {
        id: uuid::Uuid::new_v4().to_string(),
        title: trimmed.to_string(),
        description: String::new(),
        source_app: "Manual".to_string(),
        source_memory_id: None,
        created_at: chrono::Utc::now().timestamp_millis(),
        due_date: None,
        is_completed: false,
        is_dismissed: false,
        task_type: parsed_task_type,
        linked_urls: Vec::new(),
        linked_memory_ids: Vec::new(),
    };

    let mut tasks = state.store.list_tasks().await.map_err(|e| e.to_string())?;
    tasks.push(task.clone());
    state
        .store
        .upsert_tasks(&tasks)
        .await
        .map_err(|e| e.to_string())?;

    Ok(task)
}

/// Get all active todos
#[tauri::command]
pub async fn get_todos(state: State<'_, Arc<AppState>>) -> Result<Vec<Task>, String> {
    let mut tasks = state.store.list_tasks().await.map_err(|e| e.to_string())?;
    let recent_memories = state
        .store
        .list_recent_results(TASK_LINK_SCAN_LIMIT, None)
        .await
        .map_err(|e| e.to_string())?;
    let meetings = state
        .store
        .list_meetings()
        .await
        .map_err(|e| e.to_string())?;

    let links_changed = update_task_links_from_memories(&mut tasks, &recent_memories);
    let memory_backfill_changed = backfill_tasks_from_memories(&mut tasks, &recent_memories);
    let meeting_backfill_changed = backfill_tasks_from_meetings(&mut tasks, &meetings);
    let cleanup_changed = dismiss_low_quality_auto_tasks(&mut tasks);
    if links_changed || memory_backfill_changed || meeting_backfill_changed || cleanup_changed {
        state
            .store
            .upsert_tasks(&tasks)
            .await
            .map_err(|e| e.to_string())?;
    }

    let mut visible = tasks
        .into_iter()
        .filter(|task| !task.is_completed && !task.is_dismissed)
        .filter(|task| !is_low_signal_task_title(&task.title))
        .filter(|task| {
            is_manual_task(task) || is_meeting_task(task) || task_has_supporting_context(task)
        })
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    visible.retain(|task| {
        seen.insert((
            normalize_task_text(&task.title),
            task_type_sort_key(&task.task_type),
        ))
    });
    let now_ms = chrono::Utc::now().timestamp_millis();
    visible.sort_by(|left, right| {
        task_priority_score(right, now_ms)
            .cmp(&task_priority_score(left, now_ms))
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| {
                task_type_sort_key(&left.task_type).cmp(&task_type_sort_key(&right.task_type))
            })
    });
    Ok(visible)
}

/// Dismiss a task
#[tauri::command]
pub async fn dismiss_todo(
    state: State<'_, Arc<AppState>>,
    task_id: String,
) -> Result<bool, String> {
    let mut tasks = state.store.list_tasks().await.map_err(|e| e.to_string())?;
    if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
        task.is_dismissed = true;
        state
            .store
            .upsert_tasks(&tasks)
            .await
            .map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Mark a task for execution
#[tauri::command]
pub async fn execute_todo(
    state: State<'_, Arc<AppState>>,
    task_id: String,
) -> Result<Task, String> {
    let tasks = state.store.list_tasks().await.map_err(|e| e.to_string())?;
    let task = tasks
        .into_iter()
        .find(|t| t.id == task_id)
        .ok_or_else(|| "Task not found".to_string())?;

    Ok(task)
}

/// Update an existing task's title and/or type
#[tauri::command]
pub async fn update_todo(
    state: State<'_, Arc<AppState>>,
    task_id: String,
    title: String,
    task_type: Option<String>,
) -> Result<Task, String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("Task title cannot be empty.".to_string());
    }

    let parsed_type = parse_task_type(task_type.as_deref());
    let mut tasks = state.store.list_tasks().await.map_err(|e| e.to_string())?;
    let task = tasks
        .iter_mut()
        .find(|task| task.id == task_id)
        .ok_or_else(|| "Task not found".to_string())?;

    task.title = trimmed.to_string();
    task.task_type = parsed_type;
    task.created_at = chrono::Utc::now().timestamp_millis();
    let updated = task.clone();

    state
        .store
        .upsert_tasks(&tasks)
        .await
        .map_err(|e| e.to_string())?;
    Ok(updated)
}

// ========== Agent Commands ==========

use parking_lot::Mutex as AgentMutex;
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock as AgentOnceLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub is_running: bool,
    pub task_title: Option<String>,
    pub last_message: Option<String>,
    pub status: String, // "idle" | "running" | "completed" | "error"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRepairSummary {
    pub total_before: usize,
    pub total_after: usize,
    pub merged_count: usize,
    pub anchor_merges: usize,
    pub task_reference_updates: usize,
    pub screenshots_cleaned: usize,
    pub spotify_merges: usize,
    pub youtube_merges: usize,
    pub codex_merges: usize,
    pub discord_merges: usize,
    pub gitlab_merges: usize,
    pub antigravity_merges: usize,
    pub app_merges: Vec<AppMergeCount>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppMergeCount {
    pub app_name: String,
    pub merged: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRepairProgress {
    pub is_running: bool,
    pub phase: String,
    pub processed: usize,
    pub total: usize,
    pub merged_count: usize,
    pub anchor_merges: usize,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MemoryRepairCheckpoint {
    version: u32,
    source_total: usize,
    source_fingerprint: u64,
    source_first_id: String,
    source_last_id: String,
    processed: usize,
    merged_memories: Vec<MemoryRecord>,
    id_redirect: HashMap<String, String>,
    merged_count: usize,
    anchor_merges: usize,
    spotify_merges: usize,
    youtube_merges: usize,
    codex_merges: usize,
    discord_merges: usize,
    gitlab_merges: usize,
    antigravity_merges: usize,
    app_merge_counts: HashMap<String, usize>,
}

fn memory_repair_source_fingerprint(memories: &[MemoryRecord]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for memory in memories {
        for byte in memory.id.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        for byte in memory.timestamp.to_le_bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    hash
}

fn memory_repair_progress_path(state: &AppState) -> PathBuf {
    state.store.data_dir().join("memory_repair_progress.json")
}

fn memory_repair_checkpoint_path(state: &AppState) -> PathBuf {
    state.store.data_dir().join("memory_repair_checkpoint.json")
}

fn persist_memory_repair_progress(path: &PathBuf, progress: &MemoryRepairProgress) {
    if let Ok(serialized) = serde_json::to_string_pretty(progress) {
        let _ = std::fs::write(path, serialized);
    }
}

fn persist_memory_repair_checkpoint(path: &PathBuf, checkpoint: &MemoryRepairCheckpoint) {
    let tmp = path.with_extension("json.tmp");
    if let Ok(serialized) = serde_json::to_string(checkpoint) {
        if std::fs::write(&tmp, serialized).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

fn load_memory_repair_checkpoint(path: &PathBuf) -> Option<MemoryRepairCheckpoint> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<MemoryRepairCheckpoint>(&content).ok()
}

#[tauri::command]
pub async fn get_memory_repair_progress(
    state: State<'_, Arc<AppState>>,
) -> Result<MemoryRepairProgress, String> {
    let path = memory_repair_progress_path(state.inner());
    if !path.exists() {
        return Ok(MemoryRepairProgress {
            is_running: false,
            phase: "idle".to_string(),
            processed: 0,
            total: 0,
            merged_count: 0,
            anchor_merges: 0,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
        });
    }

    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut progress: MemoryRepairProgress =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;

    // If heartbeat is stale for over 2 minutes, mark as not running.
    if progress.is_running {
        let now_ms = chrono::Utc::now().timestamp_millis();
        if now_ms.saturating_sub(progress.timestamp_ms) > 120_000 {
            progress.is_running = false;
            progress.phase = "stale".to_string();
            progress.timestamp_ms = now_ms;
            persist_memory_repair_progress(&path, &progress);
        }
    }

    Ok(progress)
}

static AGENT_PROCESS: AgentOnceLock<AgentMutex<Option<Child>>> = AgentOnceLock::new();
static AGENT_STATUS: AgentOnceLock<AgentMutex<AgentStatus>> = AgentOnceLock::new();

fn get_agent_process() -> &'static AgentMutex<Option<Child>> {
    AGENT_PROCESS.get_or_init(|| AgentMutex::new(None))
}

fn get_agent_status_store() -> &'static AgentMutex<AgentStatus> {
    AGENT_STATUS.get_or_init(|| {
        AgentMutex::new(AgentStatus {
            is_running: false,
            task_title: None,
            last_message: None,
            status: "idle".to_string(),
        })
    })
}

/// Start the agent to execute a task
#[tauri::command]
pub async fn start_agent_task(
    task_title: String,
    context_urls: Option<Vec<String>>,
    context_notes: Option<Vec<String>>,
) -> Result<AgentStatus, String> {
    let mut process_guard = get_agent_process().lock();

    // Kill existing process if any
    if let Some(ref mut child) = *process_guard {
        let _ = child.kill();
    }

    // Find the agent runner script
    let sidecar_path = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("No parent dir")?
        .join("../Resources/sidecar/agent_runner.py");

    let script_path = if sidecar_path.exists() {
        sidecar_path
    } else {
        // Fallback for development
        std::path::PathBuf::from("src-tauri/sidecar/agent_runner.py")
    };

    // Find the python executable in the virtual environment
    let venv_python = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("No parent dir")?
        .join("../.venv/bin/python3");

    let python_exe = if venv_python.exists() {
        venv_python
    } else {
        // Fallback for development (assuming project root relative to execution)
        std::path::PathBuf::from(".venv/bin/python3")
    };

    let mut task_prompt = task_title.clone();
    if let Some(urls) = context_urls {
        if !urls.is_empty() {
            let url_context = urls
                .into_iter()
                .take(6)
                .map(|u| format!("- {}", u))
                .collect::<Vec<_>>()
                .join("\n");
            task_prompt.push_str("\n\nGround-truth URLs from memory graph:\n");
            task_prompt.push_str(&url_context);
        }
    }
    if let Some(notes) = context_notes {
        if !notes.is_empty() {
            task_prompt.push_str("\n\nMemory graph notes:\n");
            task_prompt.push_str(
                &notes
                    .into_iter()
                    .take(5)
                    .map(|n| format!("- {}", n))
                    .collect::<Vec<_>>()
                    .join("\n"),
            );
        }
    }

    // Start the agent process
    let child = Command::new(python_exe)
        .arg(&script_path)
        .arg(&task_prompt)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to start agent: {}", e))?;

    *process_guard = Some(child);

    // Update status
    let mut status = get_agent_status_store().lock();
    *status = AgentStatus {
        is_running: true,
        task_title: Some(task_title),
        last_message: Some("Agent started...".to_string()),
        status: "running".to_string(),
    };

    Ok(status.clone())
}

/// Get current agent status
#[tauri::command]
pub async fn get_agent_status() -> Result<AgentStatus, String> {
    let mut process_guard = get_agent_process().lock();
    let mut status = get_agent_status_store().lock();

    if let Some(ref mut child) = *process_guard {
        // Check if process is still running
        match child.try_wait() {
            Ok(Some(exit_status)) => {
                status.is_running = false;
                status.status = if exit_status.success() {
                    "completed".to_string()
                } else {
                    "error".to_string()
                };
            }
            Ok(None) => {
                // Still running, try to read output
                status.is_running = true;
            }
            Err(e) => {
                status.is_running = false;
                status.status = "error".to_string();
                status.last_message = Some(format!("Error: {}", e));
            }
        }
    }

    Ok(status.clone())
}

/// Stop the agent
#[tauri::command]
pub async fn stop_agent() -> Result<AgentStatus, String> {
    let mut process_guard = get_agent_process().lock();

    if let Some(ref mut child) = *process_guard {
        let _ = child.kill();
    }
    *process_guard = None;

    let mut status = get_agent_status_store().lock();
    *status = AgentStatus {
        is_running: false,
        task_title: status.task_title.clone(),
        last_message: Some("Agent stopped by user".to_string()),
        status: "idle".to_string(),
    };

    Ok(status.clone())
}

/// Generate a smart daily briefing paragraph using the local LLM.
/// `mode`: "morning" (actionable: what to focus on) or "evening" (recap + tomorrow).
/// Defaults to time-of-day detection when None.
#[tauri::command]
pub async fn generate_daily_briefing(
    state: State<'_, Arc<AppState>>,
    mode: Option<String>,
) -> Result<String, String> {
    // Detect mode from local hour if not specified
    let resolved_mode = mode.unwrap_or_else(|| {
        let hour = chrono::Local::now().hour();
        if hour >= 17 {
            "evening".to_string()
        } else {
            "morning".to_string()
        }
    });

    // Fetch the most recent cards (today + a few recent ones for context)
    let limit = 10usize;
    let results = state
        .store
        .list_recent_results(limit, None)
        .await
        .map_err(|e| e.to_string())?;

    let mut cards: Vec<MemoryCard> = strip_internal_fndr_results(results)
        .into_iter()
        .map(memory_card_from_result)
        .collect();
    refine_memory_card_titles(&mut cards);

    if cards.is_empty() {
        return Ok(String::new());
    }

    // Build compact per-card lines for the LLM context
    let card_lines: Vec<String> = cards
        .iter()
        .take(8)
        .map(|c| format!("- [{}] {}: {}", c.app_name, c.title, c.summary))
        .collect();

    // Grab inference engine
    let engine = {
        let guard = state.inference.read();
        guard.as_ref().map(Arc::clone)
    };

    let Some(engine) = engine else {
        return Ok(String::new());
    };

    let briefing = engine
        .generate_daily_briefing(&card_lines, &resolved_mode)
        .await;
    Ok(briefing)
}

/// Link all segments of a completed meeting to overlapping memory records via
/// `OccurredDuringAudio` edges. Called automatically when a meeting is stopped.
#[tauri::command]
pub async fn link_audio_to_memories(
    meeting_id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<usize, String> {
    let segments = crate::meeting::get_meeting_segments(&meeting_id).await;
    let count = segments.len();
    state
        .graph
        .link_audio_to_memories(&segments)
        .await
        .map_err(|e| e.to_string())?;
    Ok(count)
}

#[tauri::command]
pub fn get_fun_greeting(name: Option<String>) -> Result<String, String> {
    use rand::prelude::IndexedRandom;
    let base_name = name.unwrap_or_else(|| "there".to_string());

    let hour = chrono::Local::now().hour();

    let prefix = if hour >= 4 && hour < 12 {
        "Good Morning"
    } else if hour >= 12 && hour < 16 {
        "Good Afternoon"
    } else if hour >= 16 && hour < 20 {
        "Good Evening"
    } else {
        "Good Night"
    };

    let fun_suffixes = vec![
        "Ready to conquer the day?",
        "Let's dive into your memories.",
        "What are we exploring today?",
        "Time to make some magic happen.",
        "Welcome back to the matrix.",
        "Let's get productive.",
        "System fully operational.",
    ];

    let mut rng = rand::rng();
    let random_suffix = fun_suffixes.choose(&mut rng).unwrap_or(&"");

    Ok(format!("{}, {}! {}", prefix, base_name, random_suffix))
}

// ── Proactive Privacy Shield Commands ───────────────────────────────────────────

#[tauri::command]
pub async fn get_privacy_alerts(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<crate::PrivacyAlert>, String> {
    let pending = state.pending_privacy_alerts.read();
    Ok(pending.clone())
}

#[tauri::command]
pub async fn dismiss_privacy_alert(
    site: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    {
        let mut pending = state.pending_privacy_alerts.write();
        pending.retain(|a| a.domain_or_title != site);
    }
    {
        let mut snoozed = state.snoozed_privacy_alerts.write();
        // snooze for 30 days
        let expire = chrono::Local::now().timestamp() + (30 * 24 * 60 * 60);
        snoozed.insert(site, expire);
    }
    Ok(())
}

#[tauri::command]
pub async fn add_to_blocklist(site: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    // 1. Remove from pending alerts
    {
        let mut pending = state.pending_privacy_alerts.write();
        pending.retain(|a| a.domain_or_title != site);
    }

    // 2. Add to config blocklist
    {
        let mut config = state.config.write();
        if !config
            .blocklist
            .iter()
            .any(|b| b.eq_ignore_ascii_case(&site))
        {
            config.blocklist.push(site.clone());
        }
        if let Err(e) = config.save() {
            tracing::error!("Failed to save config: {}", e);
        }
    }

    // 3. Retroactively delete memories with this site if we grabbed it during the alert period
    if let Err(e) = state.store.delete_memories_by_domain(&site).await {
        tracing::error!(
            "Failed to retroactively delete memories for blocked site {}: {}",
            site,
            e
        );
    }

    Ok(())
}

// ── Daily Summary Commands ───────────────────────────────────────────

#[derive(Debug, Clone)]
struct DailyActivityCluster {
    app_name: String,
    label: String,
    first_ts: i64,
    last_ts: i64,
    count: usize,
    samples: Vec<String>,
}

impl DailyActivityCluster {
    fn add(&mut self, result: &SearchResult) {
        self.first_ts = self.first_ts.min(result.timestamp);
        self.last_ts = self.last_ts.max(result.timestamp);
        self.count += 1;

        let sample = daily_activity_sample(result);
        if sample.is_empty() {
            return;
        }

        let sample_key = sample.to_lowercase();
        let already_seen = self
            .samples
            .iter()
            .any(|existing| existing.to_lowercase() == sample_key);
        if !already_seen && self.samples.len() < 3 {
            self.samples.push(sample);
        }
    }
}

fn build_daily_activity_summary(records: &[SearchResult], day_label: &str) -> String {
    if records.is_empty() {
        return format!("No memories recorded for {day_label}.");
    }

    let mut sorted = records.to_vec();
    sorted.sort_by_key(|record| record.timestamp);

    let first_ts = sorted.first().map(|record| record.timestamp).unwrap_or(0);
    let last_ts = sorted
        .last()
        .map(|record| record.timestamp)
        .unwrap_or(first_ts);
    let span_ms = (last_ts - first_ts).max(0);
    let span_minutes = ((span_ms + 59_999) / 60_000).max(1);

    let mut clusters: HashMap<String, DailyActivityCluster> = HashMap::new();
    for result in &sorted {
        let key = daily_activity_key(result);
        let label = daily_activity_label(result);
        clusters
            .entry(key)
            .and_modify(|cluster| cluster.add(result))
            .or_insert_with(|| {
                let mut cluster = DailyActivityCluster {
                    app_name: result.app_name.clone(),
                    label,
                    first_ts: result.timestamp,
                    last_ts: result.timestamp,
                    count: 0,
                    samples: Vec::new(),
                };
                cluster.add(result);
                cluster
            });
    }

    let mut clusters = clusters.into_values().collect::<Vec<_>>();
    clusters.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| b.last_ts.cmp(&a.last_ts))
            .then_with(|| a.app_name.cmp(&b.app_name))
    });

    let mut lines = Vec::new();
    let memory_word = if sorted.len() == 1 {
        "memory"
    } else {
        "memories"
    };
    if sorted.len() == 1 {
        lines.push(format!(
            "- FNDR captured 1 memory {day_label} at {}.",
            format_local_time(first_ts)
        ));
    } else {
        lines.push(format!(
            "- FNDR captured {} {memory_word} across {} {day_label}, from {} to {}.",
            sorted.len(),
            human_duration_minutes(span_minutes),
            format_local_time(first_ts),
            format_local_time(last_ts)
        ));
    }

    let top_limit = if span_minutes <= 45 {
        3
    } else if span_minutes <= 240 {
        5
    } else {
        7
    };

    for cluster in clusters.iter().take(top_limit) {
        lines.push(daily_cluster_bullet(cluster));
    }

    if clusters.len() > top_limit {
        let mut app_counts: HashMap<String, usize> = HashMap::new();
        for cluster in clusters.iter().skip(top_limit) {
            *app_counts.entry(cluster.app_name.clone()).or_insert(0) += cluster.count;
        }
        let mut app_counts = app_counts.into_iter().collect::<Vec<_>>();
        app_counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let labels = app_counts
            .iter()
            .take(4)
            .map(|(app, count)| format!("{app} ({})", capture_count(*count)))
            .collect::<Vec<_>>();
        if !labels.is_empty() {
            lines.push(format!(
                "- Lighter activity also appeared in {}.",
                labels.join(", ")
            ));
        }
    }

    lines.join("\n")
}

fn daily_activity_key(result: &SearchResult) -> String {
    let label = daily_activity_label(result).to_lowercase();
    format!("{}|{}", result.app_name.to_lowercase(), label)
}

fn daily_activity_label(result: &SearchResult) -> String {
    if let Some(domain) = result.url.as_deref().and_then(card_domain) {
        return truncate_chars(&domain, 72);
    }

    let title = normalize_daily_fragment(&result.window_title);
    if !is_low_signal_title(&title, &result.app_name) {
        return truncate_chars(&title, 72);
    }

    let summary = card_summary(result);
    if let Some(title) = title_from_summary(&summary, &result.app_name) {
        return truncate_chars(&normalize_daily_fragment(&title), 72);
    }

    result.app_name.clone()
}

fn daily_activity_sample(result: &SearchResult) -> String {
    let summary = normalize_daily_fragment(&card_summary(result));
    if summary.is_empty() || is_low_signal_summary(&summary, &result.app_name) {
        return String::new();
    }

    let label = daily_activity_label(result).to_lowercase();
    if summary.to_lowercase() == label {
        return String::new();
    }

    truncate_chars(&summary.replace('"', "'"), 110)
}

fn daily_cluster_bullet(cluster: &DailyActivityCluster) -> String {
    let topic = if cluster.label.eq_ignore_ascii_case(&cluster.app_name) {
        "general activity".to_string()
    } else {
        format!("\"{}\"", cluster.label.replace('"', "'"))
    };

    let time_window = if cluster.first_ts == cluster.last_ts {
        format!("at {}", format_local_time(cluster.first_ts))
    } else {
        format!(
            "from {} to {}",
            format_local_time(cluster.first_ts),
            format_local_time(cluster.last_ts)
        )
    };

    let sample = cluster
        .samples
        .first()
        .map(|value| format!(", including \"{value}\""))
        .unwrap_or_default();

    format!(
        "- {}: {} {} around {}{}.",
        cluster.app_name,
        capture_count(cluster.count),
        time_window,
        topic,
        sample
    )
}

fn normalize_daily_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_control() { ' ' } else { ch })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn capture_count(count: usize) -> String {
    if count == 1 {
        "1 capture".to_string()
    } else {
        format!("{count} captures")
    }
}

fn human_duration_minutes(minutes: i64) -> String {
    if minutes <= 1 {
        "about 1 minute".to_string()
    } else if minutes < 60 {
        format!("about {minutes} minutes")
    } else {
        let hours = minutes / 60;
        let rest = minutes % 60;
        if rest == 0 {
            format!("about {hours} hour{}", if hours == 1 { "" } else { "s" })
        } else {
            format!(
                "about {hours} hour{} {rest} minute{}",
                if hours == 1 { "" } else { "s" },
                if rest == 1 { "" } else { "s" }
            )
        }
    }
}

fn format_local_time(timestamp: i64) -> String {
    let raw = chrono::Local
        .timestamp_millis_opt(timestamp)
        .single()
        .unwrap_or_else(chrono::Local::now)
        .format("%I:%M %p")
        .to_string();
    raw.trim_start_matches('0').to_string()
}

fn daily_summary_day_label(target_day: chrono::NaiveDate) -> String {
    if target_day == chrono::Local::now().date_naive() {
        "today".to_string()
    } else {
        format!("on {}", target_day.format("%Y-%m-%d"))
    }
}

#[tauri::command]
pub async fn generate_daily_summary_for_date(
    state: State<'_, Arc<AppState>>,
    date_str: String,
) -> Result<String, String> {
    let target_day = chrono::NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
        .map_err(|e| format!("Invalid date format: {}", e))?;
    let start = target_day
        .and_hms_opt(0, 0, 0)
        .ok_or("Failed to create start time")?;
    let end = (target_day + chrono::Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .ok_or("Failed to create end time")?;

    let start_ms = chrono::Local
        .from_local_datetime(&start)
        .earliest()
        .unwrap_or_else(|| chrono::Local.from_local_datetime(&start).latest().unwrap())
        .timestamp_millis();
    let end_ms = chrono::Local
        .from_local_datetime(&end)
        .earliest()
        .unwrap_or_else(|| chrono::Local.from_local_datetime(&end).latest().unwrap())
        .timestamp_millis()
        - 1;

    let records = state
        .store
        .get_search_results_in_range(start_ms, end_ms)
        .await
        .map_err(|e| e.to_string())?;
    let records = strip_internal_fndr_results(records);

    if records.is_empty() {
        return Ok("No memories recorded for this date.".to_string());
    }

    Ok(build_daily_activity_summary(
        &records,
        &daily_summary_day_label(target_day),
    ))
}

#[cfg(test)]
mod daily_summary_tests {
    use super::*;

    fn result(
        id: &str,
        timestamp: i64,
        app_name: &str,
        window_title: &str,
        snippet: &str,
        url: Option<&str>,
    ) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            timestamp,
            app_name: app_name.to_string(),
            bundle_id: None,
            window_title: window_title.to_string(),
            session_id: "test-session".to_string(),
            text: snippet.to_string(),
            clean_text: snippet.to_string(),
            ocr_confidence: 0.95,
            ocr_block_count: 4,
            snippet: snippet.to_string(),
            summary_source: "fallback".to_string(),
            noise_score: 0.02,
            session_key: "test-session-key".to_string(),
            score: 1.0,
            screenshot_path: None,
            url: url.map(str::to_string),
            decay_score: 1.0,
        }
    }

    #[test]
    fn daily_summary_is_grounded_for_short_capture_span() {
        let start = chrono::Utc::now().timestamp_millis();
        let records = vec![
            result(
                "1",
                start,
                "VS Code",
                "MemoryCardsPanel.tsx",
                "Investigated why all memory cards were not loading.",
                None,
            ),
            result(
                "2",
                start + 10 * 60_000,
                "Discord",
                "FNDR team chat",
                "Checked a short team follow-up.",
                None,
            ),
            result(
                "3",
                start + 30 * 60_000,
                "VS Code",
                "MemoryCardsPanel.tsx",
                "Tested the all-app memory browse flow.",
                None,
            ),
        ];

        let summary = build_daily_activity_summary(&records, "today");
        let lines = summary.lines().collect::<Vec<_>>();

        assert!(summary.contains("about 30 minutes today"));
        assert!(summary.contains("VS Code"));
        assert!(summary.contains("Discord"));
        assert!(!summary.contains("GitLab"));
        assert!(
            lines.len() <= 4,
            "short spans should not force 6-8 bullets: {summary}"
        );
    }
}
