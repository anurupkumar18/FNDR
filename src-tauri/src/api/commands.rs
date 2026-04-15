//! Tauri command handlers

use crate::embed::Embedder;
use crate::graph::{GraphEdge, GraphNode};
use crate::privacy::Blocklist;

use crate::mcp::{self, McpServerStatus};
use crate::meeting::{
    self, MeetingRecorderStatus, MeetingSearchResult, MeetingSession, MeetingTranscript,
};

use crate::search::{HybridSearcher, MemoryCard, MemoryCardSynthesizer};
use crate::speech;
use crate::store::{SearchResult, Stats};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Manager, State};
use tokio::time::{timeout, Duration, Instant};

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
const BRANCH_LIMIT: usize = 12;
const RERANK_LIMIT: usize = 10;
const GROUP_LIMIT: usize = 6;
const LLM_GROUP_LIMIT: usize = 0;

const EMBED_TIMEOUT: Duration = Duration::from_millis(1200);
const VECTOR_TIMEOUT: Duration = Duration::from_millis(1200);
const KEYWORD_TIMEOUT: Duration = Duration::from_millis(1200);
const SYNTHESIS_TIMEOUT: Duration = Duration::from_millis(2400);
const LLM_SYNTHESIS_TIMEOUT: Duration = Duration::from_millis(1500);

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

fn is_fallback_summary_source(summary_source: &str) -> bool {
    summary_source.trim().eq_ignore_ascii_case("fallback")
}

fn strip_fallback_results(mut results: Vec<SearchResult>) -> Vec<SearchResult> {
    results.retain(|result| !is_fallback_summary_source(&result.summary_source));
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

fn card_summary(result: &SearchResult) -> String {
    let base = if !result.snippet.trim().is_empty() {
        result.snippet.trim()
    } else if !result.clean_text.trim().is_empty() {
        result.clean_text.trim()
    } else {
        result.text.trim()
    };

    if base.is_empty() {
        format!("Captured activity in {}", result.app_name)
    } else {
        truncate_chars(base, 240)
    }
}

fn card_title(result: &SearchResult, summary: &str) -> String {
    let title = result.window_title.trim();
    if !title.is_empty() {
        return truncate_chars(title, 88);
    }

    if !summary.trim().is_empty() {
        return truncate_chars(summary, 88);
    }

    format!("Memory in {}", result.app_name)
}

fn memory_card_from_result(result: SearchResult) -> MemoryCard {
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
        id: result.id,
        title,
        summary,
        action,
        context,
        timestamp: result.timestamp,
        app_name,
        window_title,
        url,
        score: result.score,
        source_count: 1,
        raw_snippets: vec![fallback_snippet],
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

    let mut raw_results = if semantic_results.is_empty() {
        keyword_results.clone()
    } else {
        HybridSearcher::fuse_and_rerank(&query, &semantic_results, &keyword_results, RERANK_LIMIT)
    };
    raw_results = strip_internal_fndr_results(raw_results);
    raw_results = strip_fallback_results(raw_results);
    raw_results.truncate(RERANK_LIMIT);
    tracing::info!(count = raw_results.len(), "search_memory_cards:rerank:done");
    if raw_results.is_empty() {
        tracing::info!("search_memory_cards:complete total_ms={} cards=0", started.elapsed().as_millis());
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
            tracing::info!(count = generated.len(), "search_memory_cards:synthesis:done");
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
    let limit = limit.unwrap_or(500).clamp(1, 2_000);
    let results = state
        .inner()
        .store
        .list_recent_results(limit, app_filter.as_deref())
        .await
        .map_err(|e| e.to_string())?;

    let cards = strip_fallback_results(strip_internal_fndr_results(results))
        .into_iter()
        .map(memory_card_from_result)
        .collect();
    Ok(cards)
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
    state: State<'_, Arc<AppState>>,
    query: String,
    results_snippets: Vec<String>,
) -> Result<String, String> {
    if results_snippets.is_empty() {
        return Ok(String::new());
    }

    let mut summary = match state.inner().ensure_inference_engine().await {
        Ok(Some(engine)) => {
            engine
                .summarize_search_results(&query, &results_snippets)
                .await
        }
        Ok(None) => String::new(),
        Err(err) => {
            tracing::warn!("Failed to lazy-load AI model for search summary: {}", err);
            String::new()
        }
    };

    if summary_is_low_signal(&summary) {
        summary = deterministic_search_summary(&results_snippets);
    }

    Ok(summary)
}

fn summary_is_low_signal(summary: &str) -> bool {
    let lower = summary.trim().to_lowercase();
    lower.is_empty()
        || lower.contains("no relevant information")
        || lower.contains("no direct information")
        || lower.contains("not provided")
        || lower.contains("worked in google chrome")
}

fn deterministic_search_summary(snippets: &[String]) -> String {
    let mut facts = Vec::new();
    for snippet in snippets.iter().take(4) {
        let cleaned = snippet
            .split(']')
            .nth(1)
            .unwrap_or(snippet)
            .trim()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if cleaned.is_empty() {
            continue;
        }
        let lower = cleaned.to_lowercase();
        if lower.contains("worked in google chrome") || lower.contains("no relevant information") {
            continue;
        }
        facts.push(cleaned);
        if facts.len() >= 2 {
            break;
        }
    }

    if facts.is_empty() {
        return "Found recent activity in your captured memories.".to_string();
    }

    if facts.len() == 1 {
        return format!("{}.", facts[0].trim_end_matches('.'));
    }

    format!(
        "{}. Then {}.",
        facts[0].trim_end_matches('.'),
        facts[1].trim_end_matches('.')
    )
}

/// Get capture status
#[tauri::command]
pub async fn get_status(state: State<'_, Arc<AppState>>) -> Result<CaptureStatus, String> {
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
    meeting::list_meetings()
}

/// Get full transcript for a meeting
#[tauri::command]
pub async fn get_meeting_transcript(meeting_id: String) -> Result<MeetingTranscript, String> {
    meeting::get_meeting_transcript(&meeting_id).await
}

/// Search across meeting transcripts
#[tauri::command]
pub async fn search_meeting_transcripts(
    query: String,
    limit: Option<usize>,
) -> Result<Vec<MeetingSearchResult>, String> {
    meeting::search_meeting_transcripts(&query, limit.unwrap_or(20))
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
        backend: "whisper-large-v3-turbo-gguf".to_string(),
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
    if let Err(e) = state.inner().graph.clear_all() {
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

    // 4. Clear task store
    let _ = get_task_store().lock().clear_all();

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

// ========== Task Commands ==========

use crate::tasks::{parse_tasks_from_llm_response, Task, TaskStore};
use parking_lot::Mutex;

// Global task store (singleton pattern for now)
static TASK_STORE: OnceLock<Mutex<TaskStore>> = OnceLock::new();

fn default_task_store_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("fndr")
}

fn build_task_store(data_dir: &Path) -> Mutex<TaskStore> {
    let store = TaskStore::new(data_dir)
        .or_else(|_| TaskStore::new(&default_task_store_dir()))
        .or_else(|_| TaskStore::new(Path::new(".")))
        .unwrap_or_else(|err| panic!("Failed to initialize task store: {err}"));
    Mutex::new(store)
}

pub fn init_task_store(data_dir: &Path) {
    let _ = TASK_STORE.get_or_init(|| build_task_store(data_dir));
}

fn get_task_store() -> &'static Mutex<TaskStore> {
    TASK_STORE.get_or_init(|| build_task_store(&default_task_store_dir()))
}

/// Get extracted todos/reminders from recent memories
#[tauri::command]
pub async fn get_todos(state: State<'_, Arc<AppState>>) -> Result<Vec<Task>, String> {
    // Get recent memories (last 24 hours)
    let memories = state
        .inner()
        .store
        .get_recent_memories(24)
        .await
        .map_err(|e| e.to_string())?;

    {
        let mut task_store = get_task_store().lock();
        let _ = task_store.cleanup_old_tasks();
    }

    if memories.is_empty() {
        return Ok(get_task_store()
            .lock()
            .get_active_tasks()
            .iter()
            .map(|t| (*t).clone())
            .collect());
    }

    // Combine memory text for LLM
    let combined_text: String = memories
        .iter()
        .take(10) // Limit to 10 most recent
        .map(|m| format!("[{}] {}: {}", m.app_name, m.window_title, m.snippet))
        .collect::<Vec<_>>()
        .join("\n");

    // Extract new todos via LLM
    let llm_response = match state.inner().ensure_inference_engine().await {
        Ok(Some(engine)) => engine.extract_todos(&combined_text).await,
        Ok(None) => String::new(),
        Err(err) => {
            tracing::warn!("Failed to lazy-load AI model for todo extraction: {}", err);
            String::new()
        }
    };

    // Parse and add new tasks
    let new_tasks = parse_tasks_from_llm_response(&llm_response, "FNDR");
    let mut linked_urls = state
        .inner()
        .store
        .get_recent_urls(5)
        .await
        .map_err(|e| e.to_string())?;
    for memory in memories.iter().rev() {
        if let Some(url) = memory.url.as_ref() {
            linked_urls.push(url.clone());
        }
        if linked_urls.len() >= 8 {
            break;
        }
    }
    linked_urls = {
        let mut seen = std::collections::HashSet::new();
        linked_urls
            .into_iter()
            .filter(|url| seen.insert(url.clone()))
            .take(5)
            .collect()
    };
    let linked_memory_ids: Vec<String> = memories
        .iter()
        .rev()
        .take(3)
        .map(|m| m.id.clone())
        .collect();
    let source_memory_id = memories.last().map(|m| m.id.clone());

    let mut store = get_task_store().lock();
    for mut task in new_tasks {
        task.source_memory_id = source_memory_id.clone();
        task.linked_urls = linked_urls.clone();
        task.linked_memory_ids = linked_memory_ids.clone();

        if let Err(err) = state.inner().graph.link_task(&task) {
            tracing::warn!("Failed linking task in graph: {}", err);
        }
        let _ = store.add_task(task);
    }

    // Return all active tasks
    Ok(store
        .get_active_tasks()
        .iter()
        .map(|t| (*t).clone())
        .collect())
}

/// Dismiss a task
#[tauri::command]
pub async fn dismiss_todo(task_id: String) -> Result<bool, String> {
    get_task_store()
        .lock()
        .dismiss_task(&task_id)
        .map_err(|e| e.to_string())
}

/// Mark a task for CUA execution
#[tauri::command]
pub async fn execute_todo(
    state: State<'_, Arc<AppState>>,
    task_id: String,
) -> Result<Task, String> {
    let store = get_task_store().lock();
    let mut task = store
        .get_active_tasks()
        .into_iter()
        .find(|t| t.id == task_id)
        .cloned()
        .ok_or_else(|| "Task not found".to_string())?;

    if task.linked_urls.is_empty() {
        task.linked_urls = state.inner().graph.related_urls_for_task(&task.id);
    }

    Ok(task)
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

#[derive(Debug, Serialize)]
pub struct GraphDataResponse {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[tauri::command]
pub async fn get_graph_data(state: State<'_, Arc<AppState>>) -> Result<GraphDataResponse, String> {
    let (nodes, edges) = state.inner().graph.export_for_visualization();
    Ok(GraphDataResponse { nodes, edges })
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
