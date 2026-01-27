//! Tauri command handlers

use crate::embed::Embedder;
use crate::search::HybridSearcher;
use crate::store::{SearchResult, Stats};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureStatus {
    pub is_capturing: bool,
    pub is_paused: bool,
    pub is_incognito: bool,
    pub frames_captured: u64,
    pub frames_dropped: u64,
    pub last_capture_time: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub time_filter: Option<String>,
    pub app_filter: Option<String>,
    pub limit: Option<usize>,
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
    let limit = limit.unwrap_or(20);

    // Create embedder for this search
    let embedder = Embedder::new().map_err(|e| e.to_string())?;

    let results = HybridSearcher::search(
        &state.inner().store,
        &embedder,
        &query,
        limit,
        time_filter.as_deref(),
        app_filter.as_deref(),
    )
    .map_err(|e| e.to_string())?;

    Ok(results)
}

/// Ask FNDR a question about your memories (RAG)
#[tauri::command]
pub async fn ask_fndr(
    state: State<'_, Arc<AppState>>,
    query: String,
) -> Result<String, String> {
    // 1. Check if we have ANY memories first
    let stats = state.inner().store.get_stats()
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    
    if stats.total_records == 0 {
        return Ok("I haven't captured any memories yet! Please keep me running in the background for a few minutes while you browse or work.".to_string());
    }

    // 2. Retrieve relevant context via hybrid search (semantic + keyword, RRF) for better RAG
    let embedder = Embedder::new().map_err(|e| e.to_string())?;
    let search_results = HybridSearcher::search(
        &state.inner().store,
        &embedder,
        &query,
        5,
        None,
        None,
    )
    .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;

    if search_results.is_empty() {
        return Ok(format!("I found {} memories in total, but none of them seem to match '{}'. Try a broader question!", stats.total_records, query));
    }

    // 2. Assemble context string
    let mut context_parts = Vec::new();
    for res in search_results {
        let time = chrono::DateTime::<chrono::Utc>::from_utc(
            chrono::NaiveDateTime::from_timestamp_opt(res.timestamp / 1000, 0).unwrap(),
            chrono::Utc
        );
        context_parts.push(format!(
            "[{}] App: {}, Text: {}",
            time.format("%Y-%m-%d %H:%M:%S"),
            res.app_name,
            res.text.chars().take(500).collect::<String>()
        ));
    }
    let context = context_parts.join("\n\n---\n\n");

    // 3. Generate answer using local LLM
    let answer = state.inner().inference.answer(&query, &context).await;

    Ok(answer)
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
    config.save().map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    Ok(())
}

/// Delete all data
#[tauri::command]
pub async fn delete_all_data(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    state.inner().store.delete_all().map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    Ok(())
}

/// Get statistics
#[tauri::command]
pub async fn get_stats(state: State<'_, Arc<AppState>>) -> Result<Stats, String> {
    state.inner().store.get_stats().map_err(|e: Box<dyn std::error::Error>| e.to_string())
}

/// Get retention days (0 = keep forever)
#[tauri::command]
pub async fn get_retention_days(state: State<'_, Arc<AppState>>) -> Result<u32, String> {
    Ok(state.inner().config.read().retention_days)
}

/// Set retention days (0 = keep forever)
#[tauri::command]
pub async fn set_retention_days(
    state: State<'_, Arc<AppState>>,
    days: u32,
) -> Result<(), String> {
    let mut config = state.inner().config.write();
    config.retention_days = days;
    config.save().map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    Ok(())
}

/// Get unique app names for filter dropdown
#[tauri::command]
pub async fn get_app_names(state: State<'_, Arc<AppState>>) -> Result<Vec<String>, String> {
    state
        .inner()
        .store
        .get_app_names()
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())
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
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())
}
