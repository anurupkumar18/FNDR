//! Tauri command handlers

use crate::capture::permissions;
use crate::demo;
use crate::embed::Embedder;
use crate::graph::MemoryReconstruction;
use crate::mcp::{self, McpServerStatus};
use crate::meeting::{
    self, MeetingRecorderStatus, MeetingSearchResult, MeetingSession, MeetingTranscript,
};
use crate::ocr::OcrEngine;
use crate::search::HybridSearcher;
use crate::speech;
use crate::store::{SearchResult, Stats};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use tauri::{AppHandle, Manager, State};

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

fn shared_embedder() -> Result<&'static Embedder, String> {
    match SHARED_EMBEDDER.get_or_init(Embedder::new) {
        Ok(embedder) => Ok(embedder),
        Err(err) => Err(err.clone()),
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

    Ok(results)
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

    let summary = match state.inner().ensure_inference_engine().await {
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

    Ok(summary)
}

/// Ask FNDR a question about your memories (RAG)
#[tauri::command]
pub async fn ask_fndr(state: State<'_, Arc<AppState>>, query: String) -> Result<String, String> {
    Ok(run_memory_reconstruction(state.inner().as_ref(), &query, 6)
        .await?
        .answer)
}

/// Reconstruct memory context (cards + synthesized answer) for artifact side panel.
#[tauri::command]
pub async fn reconstruct_memory(
    state: State<'_, Arc<AppState>>,
    query: String,
    limit: Option<usize>,
) -> Result<MemoryReconstruction, String> {
    run_memory_reconstruction(state.inner().as_ref(), &query, limit.unwrap_or(8)).await
}

/// Summarize a memory in detail using LLM
#[tauri::command]
pub async fn summarize_memory(
    state: State<'_, Arc<AppState>>,
    app_name: String,
    window_title: String,
    text: String,
) -> Result<String, String> {
    let summary = match state.inner().ensure_inference_engine().await {
        Ok(Some(engine)) => {
            engine
                .summarize_memory_detail(&app_name, &window_title, &text)
                .await
        }
        Ok(None) => String::new(),
        Err(err) => {
            tracing::warn!("Failed to lazy-load AI model for memory summary: {}", err);
            String::new()
        }
    };
    Ok(summary)
}

async fn run_memory_reconstruction(
    app_state: &AppState,
    query: &str,
    limit: usize,
) -> Result<MemoryReconstruction, String> {
    let stats = app_state
        .store
        .get_stats()
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;

    if stats.total_records == 0 {
        return Ok(MemoryReconstruction {
            answer: "I haven't captured any memories yet. Keep FNDR running for a few minutes while you work, then ask again.".to_string(),
            cards: Vec::new(),
            structural_context: Vec::new(),
        });
    }

    let embedder = shared_embedder()?;
    let mut reconstruction = app_state
        .graph
        .reconstruct(&app_state.store, &embedder, query, limit)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;

    if reconstruction.cards.is_empty() {
        reconstruction.answer = format!(
            "I found {} memories in total, but none seem to match '{}'. Try broader terms.",
            stats.total_records, query
        );
        return Ok(reconstruction);
    }

    let mut context_parts = Vec::new();
    for card in reconstruction.cards.iter().take(6) {
        let time = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(card.timestamp)
            .unwrap_or_else(chrono::Utc::now);
        context_parts.push(format!(
            "[{}] App: {} | Window: {} | Snippet: {} | URL: {}",
            time.format("%Y-%m-%d %H:%M:%S"),
            card.app_name,
            card.window_title,
            card.snippet,
            card.url.clone().unwrap_or_else(|| "n/a".to_string())
        ));
    }
    for note in &reconstruction.structural_context {
        context_parts.push(format!("[Graph] {note}"));
    }

    let context = context_parts.join("\n");
    reconstruction.answer = match app_state.ensure_inference_engine().await {
        Ok(Some(engine)) => engine.answer(query, &context).await,
        Ok(None) => "AI intelligence is disabled until Qwen3-VL is downloaded.".to_string(),
        Err(err) => format!("AI intelligence is temporarily unavailable: {}", err),
    };
    Ok(reconstruction)
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
    state
        .inner()
        .store
        .get_app_names()
        .await
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

// ========== Graph Visualization Commands ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphData {
    pub nodes: Vec<GraphNodeData>,
    pub edges: Vec<GraphEdgeData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphNodeData {
    pub id: String,
    pub label: String,
    pub node_type: String,
    pub created_at: i64,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphEdgeData {
    pub id: String,
    pub source: String,
    pub target: String,
    pub edge_type: String,
    pub label: String,
    pub timestamp: i64,
}

/// Get full graph data for visualization
#[tauri::command]
pub async fn get_graph_data(state: State<'_, Arc<AppState>>) -> Result<GraphData, String> {
    let (nodes, edges) = state.inner().graph.export_for_visualization();

    let node_data: Vec<GraphNodeData> = nodes
        .iter()
        .map(|n| GraphNodeData {
            id: n.id.clone(),
            label: if n.label.len() > 60 {
                format!("{}...", n.label.chars().take(57).collect::<String>())
            } else {
                n.label.clone()
            },
            node_type: format!("{:?}", n.node_type),
            created_at: n.created_at,
            metadata: n.metadata.clone(),
        })
        .collect();

    let edge_data: Vec<GraphEdgeData> = edges
        .iter()
        .map(|e| GraphEdgeData {
            id: e.id.clone(),
            source: e.source.clone(),
            target: e.target.clone(),
            edge_type: format!("{:?}", e.edge_type),
            label: format!("{:?}", e.edge_type),
            timestamp: e.timestamp,
        })
        .collect();

    Ok(GraphData {
        nodes: node_data,
        edges: edge_data,
    })
}

/// Search the knowledge graph
#[tauri::command]
pub async fn search_graph(
    state: State<'_, Arc<AppState>>,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<SearchResult>, String> {
    let limit = limit.unwrap_or(20).clamp(1, 50);
    let embedder = shared_embedder()?;

    HybridSearcher::search(&state.inner().store, &embedder, &query, limit, None, None)
        .await
        .map_err(|e| e.to_string())
}

// ========== App Config & System Readiness Commands ==========

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfigPayload {
    pub experimental_ui_enabled: bool,
    pub use_demo_data_only: bool,
    pub use_vlm: bool,
}

#[tauri::command]
pub async fn get_app_config(state: State<'_, Arc<AppState>>) -> Result<AppConfigPayload, String> {
    let c = state.inner().config.read();
    Ok(AppConfigPayload {
        experimental_ui_enabled: c.experimental_ui_enabled,
        use_demo_data_only: c.use_demo_data_only,
        use_vlm: c.use_vlm,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemReadiness {
    pub screen_capture_permission_granted: bool,
    pub screen_capture_permission_detail: String,
    pub ocr_available: bool,
    pub ocr_detail: String,
    pub inference_ready: bool,
    pub embedder_ready: bool,
    pub vector_store_ready: bool,
    pub data_dir_writable: bool,
    pub data_dir_detail: String,
    pub capture_status: CaptureStatus,
    pub total_records: usize,
    pub vlm_active: bool,
    pub use_demo_data_only: bool,
    pub ready_for_search: bool,
    pub fixes: Vec<String>,
}

#[tauri::command]
pub async fn get_readiness(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<SystemReadiness, String> {
    let (sc_ok, sc_detail) = permissions::preflight_screen_capture_access();

    let ocr_ok = OcrEngine::new().is_ok();
    let ocr_detail = if ocr_ok {
        "Apple Vision OCR initialized.".to_string()
    } else {
        "OCR failed to initialize. Screen text extraction will not run.".to_string()
    };

    let embed_ok = shared_embedder().is_ok();

    let mut fixes: Vec<String> = Vec::new();
    if !sc_ok {
        fixes.push(sc_detail.clone());
    }
    if !ocr_ok {
        fixes.push(ocr_detail.clone());
    }
    if !embed_ok {
        fixes.push("Embedding engine failed to initialize.".to_string());
    }

    let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let (data_ok, data_detail) = match std::fs::create_dir_all(&data_dir) {
        Ok(()) => {
            let probe = data_dir.join(".fndr_write_probe");
            match std::fs::write(&probe, b"ok") {
                Ok(()) => {
                    let _ = std::fs::remove_file(&probe);
                    (true, format!("App data directory writable: {:?}", data_dir))
                }
                Err(e) => {
                    let msg = format!("Cannot write app data directory {:?}: {}", data_dir, e);
                    fixes.push(msg.clone());
                    (false, msg)
                }
            }
        }
        Err(e) => {
            let msg = format!("Cannot create app data directory {:?}: {}", data_dir, e);
            fixes.push(msg.clone());
            (false, msg)
        }
    };

    let stats_res = state.inner().store.get_stats();
    let store_ok = stats_res.is_ok();
    if !store_ok {
        fixes.push("Vector store is not responding.".to_string());
    }
    let total_records = stats_res.map(|s| s.total_records).unwrap_or(0);

    let cfg = state.inner().config.read();
    let use_demo = cfg.use_demo_data_only;
    let vlm_active = state.inner().vlm.read().is_some() && cfg.use_vlm;

    let live_path_ok = sc_ok && ocr_ok;
    let ready_for_search = embed_ok && store_ok && data_ok && (use_demo || live_path_ok);

    if use_demo {
        fixes.retain(|f| {
            !f.contains("Screen Recording")
                && !f.contains("Screen capture")
                && !f.contains("OCR")
                && !f.contains("Apple Vision")
        });
    }

    let cs = CaptureStatus {
        is_capturing: state.inner().is_capturing(),
        is_paused: state.inner().is_paused.load(Ordering::SeqCst),
        is_incognito: state.inner().is_incognito.load(Ordering::SeqCst),
        frames_captured: state.inner().frames_captured.load(Ordering::Relaxed),
        frames_dropped: state.inner().frames_dropped.load(Ordering::Relaxed),
        last_capture_time: state.inner().last_capture_time.load(Ordering::Relaxed),
        ai_model_available: state.inner().ai_model_available(),
        ai_model_loaded: state.inner().ai_model_loaded(),
        loaded_model_id: state.inner().loaded_model_id(),
    };

    Ok(SystemReadiness {
        screen_capture_permission_granted: sc_ok,
        screen_capture_permission_detail: sc_detail,
        ocr_available: ocr_ok,
        ocr_detail,
        inference_ready: state.inner().ai_model_loaded(),
        embedder_ready: embed_ok,
        vector_store_ready: store_ok,
        data_dir_writable: data_ok,
        data_dir_detail: data_detail,
        capture_status: cs,
        total_records,
        vlm_active,
        use_demo_data_only: use_demo,
        ready_for_search,
        fixes,
    })
}

#[tauri::command]
pub async fn set_use_demo_data_only(
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<AppConfigPayload, String> {
    {
        let mut c = state.inner().config.write();
        c.use_demo_data_only = enabled;
        c.save().map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    }
    get_app_config(state).await
}

#[tauri::command]
pub async fn seed_demo_dataset(state: State<'_, Arc<AppState>>) -> Result<usize, String> {
    let embedder = shared_embedder()?;
    let records = demo::build_demo_records(&embedder).map_err(|e| e.to_string())?;
    let n = records.len();
    state
        .inner()
        .store
        .delete_id_prefix(demo::DEMO_ID_PREFIX)
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    state
        .inner()
        .store
        .add_batch(&records)
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    Ok(n)
}

#[tauri::command]
pub async fn reset_demo_data(state: State<'_, Arc<AppState>>) -> Result<usize, String> {
    state
        .inner()
        .store
        .delete_id_prefix(demo::DEMO_ID_PREFIX)
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())
}

#[tauri::command]
pub async fn inject_test_memory(state: State<'_, Arc<AppState>>) -> Result<String, String> {
    let embedder = shared_embedder()?;
    let record = demo::build_inject_record(&embedder).map_err(|e| e.to_string())?;
    let id = record.id.clone();
    let inject_prefix = format!("{}inject", demo::DEMO_ID_PREFIX);
    let _ = state.inner().store.delete_id_prefix(&inject_prefix);
    state
        .inner()
        .store
        .add_batch(&[record])
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    Ok(id)
}
