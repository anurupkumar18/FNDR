//! Meeting recorder runtime and persistence.
//!
//! This module provides local-only meeting recording with automatic session
//! detection, segmented audio capture, and local transcription.

use crate::{store::MemoryRecord, AppState};
use parking_lot::{Mutex, RwLock};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter};
use tokio::task::JoinHandle;
use uuid::Uuid;

const MEETINGS_DIR: &str = "meetings";
const MEETINGS_INDEX: &str = "meetings.json";
const SEGMENTS_INDEX: &str = "segments.json";
const SEGMENT_SECONDS: i64 = 20;
const STATUS_EVENT: &str = "meeting://status";
const SEGMENT_EVENT: &str = "meeting://segment";
const FORCED_MODEL: &str = "parakeet-v3-small";
const AUTO_POLL_SECONDS: u64 = 5;
const AUTO_START_HITS: u8 = 2;
const AUTO_STOP_IDLE_SECONDS: u64 = 90;
const PY_BACKEND_BOOTSTRAP_COOLDOWN_MS: i64 = 300_000;

#[derive(Debug)]
struct BackendBootstrapState {
    last_attempt_ms: i64,
}

static PY_BACKEND_BOOTSTRAP_STATE: OnceLock<Mutex<BackendBootstrapState>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingSession {
    pub id: String,
    pub title: String,
    pub participants: Vec<String>,
    pub model: String,
    pub status: String, // "recording" | "stopped" | "error"
    pub start_timestamp: i64,
    pub end_timestamp: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub segment_count: usize,
    pub duration_seconds: u64,
    pub meeting_dir: String,
    pub audio_dir: String,
    pub transcript_path: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingRecorderStatus {
    pub is_recording: bool,
    pub current_meeting_id: Option<String>,
    pub current_title: Option<String>,
    pub model: Option<String>,
    pub started_at: Option<i64>,
    pub segment_count: usize,
    pub ffmpeg_available: bool,
    pub transcription_backend: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingTranscript {
    pub meeting: MeetingSession,
    pub segments: Vec<MeetingSegment>,
    pub full_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingSearchResult {
    pub meeting_id: String,
    pub meeting_title: String,
    pub segment_id: String,
    pub index: u32,
    pub text: String,
    pub score: f32,
    pub start_timestamp: i64,
    pub end_timestamp: i64,
}

struct MeetingStore {
    root_dir: PathBuf,
    meetings_index: PathBuf,
    segments_index: PathBuf,
    meetings: RwLock<Vec<MeetingSession>>,
    segments: RwLock<Vec<MeetingSegment>>,
}

impl MeetingStore {
    fn new(app_data_dir: PathBuf) -> Result<Self, String> {
        let root_dir = app_data_dir.join(MEETINGS_DIR);
        fs::create_dir_all(&root_dir).map_err(|e| format!("Failed to create meetings dir: {e}"))?;

        let meetings_index = root_dir.join(MEETINGS_INDEX);
        let segments_index = root_dir.join(SEGMENTS_INDEX);
        let meetings: Vec<MeetingSession> = read_json_or_default(&meetings_index);
        let segments: Vec<MeetingSegment> = read_json_or_default(&segments_index);

        Ok(Self {
            root_dir,
            meetings_index,
            segments_index,
            meetings: RwLock::new(meetings),
            segments: RwLock::new(segments),
        })
    }

    fn recover_unfinished(&self) -> Result<(), String> {
        let mut touched = false;
        {
            let mut meetings = self.meetings.write();
            for meeting in meetings.iter_mut() {
                if meeting.status == "recording" {
                    meeting.status = "stopped".to_string();
                    meeting.end_timestamp = Some(now_ms());
                    meeting.updated_at = now_ms();
                    touched = true;
                }
            }
        }
        if touched {
            self.persist_meetings()?;
        }
        Ok(())
    }

    fn create_meeting(
        &self,
        title: String,
        participants: Vec<String>,
        model: String,
    ) -> Result<MeetingSession, String> {
        let now = now_ms();
        let meeting_id = Uuid::new_v4().to_string();
        let meeting_dir = self.root_dir.join(&meeting_id);
        let audio_dir = meeting_dir.join("audio");
        fs::create_dir_all(&audio_dir)
            .map_err(|e| format!("Failed to create meeting audio dir: {e}"))?;

        let meeting = MeetingSession {
            id: meeting_id,
            title,
            participants,
            model,
            status: "recording".to_string(),
            start_timestamp: now,
            end_timestamp: None,
            created_at: now,
            updated_at: now,
            segment_count: 0,
            duration_seconds: 0,
            meeting_dir: meeting_dir.to_string_lossy().to_string(),
            audio_dir: audio_dir.to_string_lossy().to_string(),
            transcript_path: None,
        };

        {
            let mut meetings = self.meetings.write();
            meetings.push(meeting.clone());
        }
        self.persist_meetings()?;
        Ok(meeting)
    }

    fn set_meeting_error(&self, meeting_id: &str, message: &str) -> Result<(), String> {
        {
            let mut meetings = self.meetings.write();
            if let Some(meeting) = meetings.iter_mut().find(|m| m.id == meeting_id) {
                meeting.status = "error".to_string();
                meeting.updated_at = now_ms();
                meeting.end_timestamp = Some(now_ms());
                meeting.transcript_path = Some(message.to_string());
            }
        }
        self.persist_meetings()
    }

    fn finish_meeting(
        &self,
        meeting_id: &str,
        transcript_path: Option<String>,
    ) -> Result<(), String> {
        {
            let mut meetings = self.meetings.write();
            if let Some(meeting) = meetings.iter_mut().find(|m| m.id == meeting_id) {
                meeting.status = "stopped".to_string();
                meeting.end_timestamp = Some(now_ms());
                meeting.updated_at = now_ms();
                meeting.transcript_path = transcript_path;
                if let Some(end) = meeting.end_timestamp {
                    meeting.duration_seconds =
                        ((end - meeting.start_timestamp).max(0) / 1000) as u64;
                }
            }
        }
        self.persist_meetings()
    }

    fn add_segment(&self, segment: MeetingSegment) -> Result<(), String> {
        let meeting_id = segment.meeting_id.clone();
        let segment_end = segment.end_timestamp;
        {
            let mut segments = self.segments.write();
            segments.push(segment);
        }
        self.persist_segments()?;

        {
            let mut meetings = self.meetings.write();
            if let Some(meeting) = meetings.iter_mut().find(|m| m.id == meeting_id) {
                let count = self
                    .segments
                    .read()
                    .iter()
                    .filter(|s| s.meeting_id == meeting.id)
                    .count();
                meeting.segment_count = count;
                meeting.duration_seconds =
                    ((segment_end - meeting.start_timestamp).max(0) / 1000) as u64;
                meeting.updated_at = now_ms();
            }
        }
        self.persist_meetings()?;
        Ok(())
    }

    fn list_meetings(&self) -> Vec<MeetingSession> {
        let mut meetings = self.meetings.read().clone();
        meetings.sort_by_key(|m| std::cmp::Reverse(m.start_timestamp));
        meetings
    }

    fn get_meeting(&self, meeting_id: &str) -> Option<MeetingSession> {
        self.meetings
            .read()
            .iter()
            .find(|m| m.id == meeting_id)
            .cloned()
    }

    fn get_segments_for_meeting(&self, meeting_id: &str) -> Vec<MeetingSegment> {
        let mut segments: Vec<MeetingSegment> = self
            .segments
            .read()
            .iter()
            .filter(|s| s.meeting_id == meeting_id)
            .cloned()
            .collect();
        segments.sort_by_key(|s| s.index);
        segments
    }

    fn get_transcript(&self, meeting_id: &str) -> Result<MeetingTranscript, String> {
        let meeting = self
            .get_meeting(meeting_id)
            .ok_or_else(|| "Meeting not found".to_string())?;
        let segments = self.get_segments_for_meeting(meeting_id);
        let full_text = segments
            .iter()
            .map(|s| s.text.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        Ok(MeetingTranscript {
            meeting,
            segments,
            full_text,
        })
    }

    fn search_transcripts(&self, query: &str, limit: usize) -> Vec<MeetingSearchResult> {
        if query.trim().is_empty() {
            return Vec::new();
        }

        let query_lower = query.to_lowercase();
        let meetings_by_id: HashMap<String, String> = self
            .meetings
            .read()
            .iter()
            .map(|m| (m.id.clone(), m.title.clone()))
            .collect();

        let mut results: Vec<MeetingSearchResult> = self
            .segments
            .read()
            .iter()
            .filter_map(|segment| {
                let text_lower = segment.text.to_lowercase();
                if !text_lower.contains(&query_lower) {
                    return None;
                }

                let score = text_lower.matches(&query_lower).count() as f32 + 1.0;
                Some(MeetingSearchResult {
                    meeting_id: segment.meeting_id.clone(),
                    meeting_title: meetings_by_id
                        .get(&segment.meeting_id)
                        .cloned()
                        .unwrap_or_else(|| "Untitled Meeting".to_string()),
                    segment_id: segment.id.clone(),
                    index: segment.index,
                    text: segment.text.clone(),
                    score,
                    start_timestamp: segment.start_timestamp,
                    end_timestamp: segment.end_timestamp,
                })
            })
            .collect();

        results.sort_by(|a, b| b.score.total_cmp(&a.score));
        results.truncate(limit);
        results
    }

    fn write_transcript_file(&self, meeting_id: &str, markdown: &str) -> Result<String, String> {
        let meeting = self
            .get_meeting(meeting_id)
            .ok_or_else(|| "Meeting not found".to_string())?;
        let transcript_path = PathBuf::from(&meeting.meeting_dir).join("transcript.md");
        let mut file = File::create(&transcript_path)
            .map_err(|e| format!("Failed to create transcript file: {e}"))?;
        file.write_all(markdown.as_bytes())
            .map_err(|e| format!("Failed to write transcript file: {e}"))?;
        Ok(transcript_path.to_string_lossy().to_string())
    }

    fn write_transcript_to_documents(
        &self,
        meeting_id: &str,
        markdown: &str,
    ) -> Result<String, String> {
        let meeting = self
            .get_meeting(meeting_id)
            .ok_or_else(|| "Meeting not found".to_string())?;

        let docs_root = dirs::document_dir()
            .ok_or_else(|| "Could not locate Documents directory".to_string())?;
        let out_dir = docs_root.join("FNDR Meetings");
        fs::create_dir_all(&out_dir)
            .map_err(|e| format!("Failed to create Documents/FNDR Meetings: {e}"))?;

        let stamp = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(meeting.start_timestamp)
            .unwrap_or_else(chrono::Utc::now)
            .format("%Y%m%d_%H%M%S")
            .to_string();
        let name = sanitize_filename(&meeting.title);
        let path = out_dir.join(format!("{stamp}_{name}.md"));

        let mut file = File::create(&path)
            .map_err(|e| format!("Failed to create docs transcript file: {e}"))?;
        file.write_all(markdown.as_bytes())
            .map_err(|e| format!("Failed to write docs transcript file: {e}"))?;
        Ok(path.to_string_lossy().to_string())
    }

    fn set_segment_text(
        &self,
        meeting_id: &str,
        segment_index: u32,
        text: String,
    ) -> Result<(), String> {
        {
            let mut segments = self.segments.write();
            if let Some(seg) = segments
                .iter_mut()
                .find(|s| s.meeting_id == meeting_id && s.index == segment_index)
            {
                seg.text = text;
            }
        }
        self.persist_segments()
    }

    fn set_transcript_path(
        &self,
        meeting_id: &str,
        transcript_path: Option<String>,
    ) -> Result<(), String> {
        {
            let mut meetings = self.meetings.write();
            if let Some(meeting) = meetings.iter_mut().find(|m| m.id == meeting_id) {
                meeting.transcript_path = transcript_path;
                meeting.updated_at = now_ms();
            }
        }
        self.persist_meetings()
    }

    fn persist_meetings(&self) -> Result<(), String> {
        let snapshot = self.meetings.read().clone();
        write_json(&self.meetings_index, &snapshot)
    }

    fn persist_segments(&self) -> Result<(), String> {
        let snapshot = self.segments.read().clone();
        write_json(&self.segments_index, &snapshot)
    }
}

struct ActiveMeeting {
    meeting_id: String,
    title: String,
    model: String,
    started_at: i64,
    stop_flag: Arc<AtomicBool>,
    recorder: Child,
    worker: JoinHandle<()>,
}

struct MeetingRuntime {
    store: Option<Arc<MeetingStore>>,
    active: Option<ActiveMeeting>,
    app_handle: Option<AppHandle>,
    app_state: Option<Arc<AppState>>,
    auto_task: Option<tauri::async_runtime::JoinHandle<()>>,
    repair_task: Option<tauri::async_runtime::JoinHandle<()>>,
    last_error: Option<String>,
}

impl Default for MeetingRuntime {
    fn default() -> Self {
        Self {
            store: None,
            active: None,
            app_handle: None,
            app_state: None,
            auto_task: None,
            repair_task: None,
            last_error: None,
        }
    }
}

static RUNTIME: OnceLock<Mutex<MeetingRuntime>> = OnceLock::new();

fn runtime() -> &'static Mutex<MeetingRuntime> {
    RUNTIME.get_or_init(|| Mutex::new(MeetingRuntime::default()))
}

pub fn init(app_data_dir: PathBuf) -> Result<(), String> {
    let store = Arc::new(MeetingStore::new(app_data_dir)?);
    store.recover_unfinished()?;

    let mut rt = runtime().lock();
    rt.store = Some(store);
    rt.last_error = None;
    Ok(())
}

pub fn bind_runtime(app_handle: AppHandle, app_state: Arc<AppState>) -> Result<(), String> {
    let should_start_tasks = {
        let mut rt = runtime().lock();
        rt.app_handle = Some(app_handle.clone());
        rt.app_state = Some(app_state.clone());
        rt.auto_task.is_none()
    };

    if !should_start_tasks {
        return Ok(());
    }

    let auto_task = tauri::async_runtime::spawn(auto_monitor_loop(app_handle, app_state.clone()));
    let repair_task = tauri::async_runtime::spawn(repair_failed_transcripts_once(app_state));

    {
        let mut rt = runtime().lock();
        rt.auto_task = Some(auto_task);
        rt.repair_task = Some(repair_task);
    }
    Ok(())
}

pub fn list_meetings() -> Result<Vec<MeetingSession>, String> {
    let store = get_store()?;
    Ok(store.list_meetings())
}

pub fn get_meeting_transcript(meeting_id: &str) -> Result<MeetingTranscript, String> {
    let store = get_store()?;
    if let Some(meeting) = store.get_meeting(meeting_id) {
        let segments = store.get_segments_for_meeting(meeting_id);
        let needs_reprocess = meeting.status != "recording"
            && segments.iter().any(|s| should_retry_segment_text(&s.text));

        if needs_reprocess {
            maybe_bootstrap_python_backend(true);
            let _ = transcribe_meeting_postprocess(store.as_ref(), meeting_id, &meeting.model);
            let repaired = store.get_transcript(meeting_id)?;
            let markdown = build_meeting_markdown(&repaired);
            let session_path = store.write_transcript_file(meeting_id, &markdown).ok();
            let finder_path = store
                .write_transcript_to_documents(meeting_id, &markdown)
                .ok();
            let transcript_path = finder_path.or(session_path);
            if let Some(path) = transcript_path.clone() {
                let _ = store.set_transcript_path(meeting_id, Some(path.clone()));
                if let Some(state) = runtime().lock().app_state.clone() {
                    let _ = ingest_transcript_into_fndr_memory(state, &repaired, Some(&path));
                }
            }
        }
    }
    store.get_transcript(meeting_id)
}

pub fn search_meeting_transcripts(
    query: &str,
    limit: usize,
) -> Result<Vec<MeetingSearchResult>, String> {
    let store = get_store()?;
    Ok(store.search_transcripts(query, limit.clamp(1, 100)))
}

pub fn recorder_status() -> Result<MeetingRecorderStatus, String> {
    let rt = runtime().lock();
    let ffmpeg_available = resolve_ffmpeg_binary().is_some();
    let backend = detect_transcription_backend();

    if let Some(active) = rt.active.as_ref() {
        let count = rt
            .store
            .as_ref()
            .and_then(|s| s.get_meeting(&active.meeting_id))
            .map(|m| m.segment_count)
            .unwrap_or(0);

        return Ok(MeetingRecorderStatus {
            is_recording: true,
            current_meeting_id: Some(active.meeting_id.clone()),
            current_title: Some(active.title.clone()),
            model: Some(active.model.clone()),
            started_at: Some(active.started_at),
            segment_count: count,
            ffmpeg_available,
            transcription_backend: backend,
            last_error: rt.last_error.clone(),
        });
    }

    Ok(MeetingRecorderStatus {
        is_recording: false,
        current_meeting_id: None,
        current_title: None,
        model: None,
        started_at: None,
        segment_count: 0,
        ffmpeg_available,
        transcription_backend: backend,
        last_error: rt.last_error.clone(),
    })
}

pub async fn start_recording(
    app_handle: Option<AppHandle>,
    title: String,
    participants: Vec<String>,
    _model: Option<String>,
) -> Result<MeetingRecorderStatus, String> {
    let clean_title = if title.trim().is_empty() {
        "Detected Meeting".to_string()
    } else {
        title.trim().to_string()
    };
    let clean_participants: Vec<String> = participants
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();

    let (store, meeting, app_for_worker) = {
        let mut rt = runtime().lock();
        let store = rt
            .store
            .as_ref()
            .cloned()
            .ok_or_else(|| "Meeting runtime is not initialized".to_string())?;

        if rt.active.is_some() {
            return Err("A meeting recording is already active".to_string());
        }

        if let Some(handle) = app_handle.clone() {
            rt.app_handle = Some(handle);
        }

        let app_for_worker = rt.app_handle.clone();
        let meeting =
            store.create_meeting(clean_title, clean_participants, FORCED_MODEL.to_string())?;
        (store, meeting, app_for_worker)
    };

    let segment_pattern = PathBuf::from(&meeting.audio_dir).join("segment_%05d.wav");
    let recorder = match spawn_ffmpeg_recorder(&segment_pattern) {
        Ok(child) => child,
        Err(err) => {
            let _ = store.set_meeting_error(&meeting.id, &err);
            runtime().lock().last_error = Some(err.clone());
            return Err(err);
        }
    };

    let stop_flag = Arc::new(AtomicBool::new(false));
    let worker = tokio::spawn(segment_worker(
        store.clone(),
        meeting.id.clone(),
        PathBuf::from(&meeting.audio_dir),
        meeting.model.clone(),
        meeting.start_timestamp,
        stop_flag.clone(),
        app_for_worker.clone(),
    ));

    {
        let mut rt = runtime().lock();
        rt.active = Some(ActiveMeeting {
            meeting_id: meeting.id.clone(),
            title: meeting.title.clone(),
            model: meeting.model.clone(),
            started_at: meeting.start_timestamp,
            stop_flag,
            recorder,
            worker,
        });
        rt.last_error = None;
    }

    let status = recorder_status()?;
    if let Some(handle) = app_for_worker {
        let _ = handle.emit(STATUS_EVENT, &status);
    }
    Ok(status)
}

pub async fn stop_recording() -> Result<MeetingRecorderStatus, String> {
    let (store, app_handle, app_state, active) = {
        let mut rt = runtime().lock();
        let store = rt
            .store
            .as_ref()
            .cloned()
            .ok_or_else(|| "Meeting runtime is not initialized".to_string())?;
        let app_handle = rt.app_handle.clone();
        let app_state = rt.app_state.clone();
        let active = rt.active.take();
        (store, app_handle, app_state, active)
    };

    let Some(active) = active else {
        return recorder_status();
    };

    let ActiveMeeting {
        meeting_id,
        model,
        stop_flag,
        mut recorder,
        worker,
        ..
    } = active;

    stop_flag.store(true, Ordering::SeqCst);

    if let Err(err) = recorder.kill() {
        tracing::warn!("Failed to terminate ffmpeg recorder cleanly: {}", err);
    }
    let _ = recorder.wait();
    let _ = worker.await;

    // Force one bootstrap attempt when meeting ends before final transcription pass.
    maybe_bootstrap_python_backend(true);
    if let Err(err) = transcribe_meeting_postprocess(store.as_ref(), &meeting_id, &model) {
        tracing::warn!("Post-meeting transcription pass failed: {}", err);
    }

    let transcript = store.get_transcript(&meeting_id)?;
    let markdown = build_meeting_markdown(&transcript);
    let session_path = store.write_transcript_file(&meeting_id, &markdown).ok();
    let finder_path = store
        .write_transcript_to_documents(&meeting_id, &markdown)
        .ok();
    let transcript_path = finder_path.or(session_path);
    store.finish_meeting(&meeting_id, transcript_path.clone())?;

    if let Some(state) = app_state {
        if let Err(err) =
            ingest_transcript_into_fndr_memory(state, &transcript, transcript_path.as_deref())
        {
            tracing::warn!(
                "Failed to ingest meeting transcript into FNDR memory: {}",
                err
            );
        }
    }

    let status = recorder_status()?;
    if let Some(handle) = app_handle {
        let _ = handle.emit(STATUS_EVENT, &status);
    }
    Ok(status)
}

async fn auto_monitor_loop(app_handle: AppHandle, app_state: Arc<AppState>) {
    let mut hits: u8 = 0;
    let mut idle_seconds: u64 = 0;

    loop {
        let signal = detect_meeting_signal(&app_state).await;
        let status = recorder_status().unwrap_or(MeetingRecorderStatus {
            is_recording: false,
            current_meeting_id: None,
            current_title: None,
            model: None,
            started_at: None,
            segment_count: 0,
            ffmpeg_available: resolve_ffmpeg_binary().is_some(),
            transcription_backend: detect_transcription_backend(),
            last_error: None,
        });

        if let Some(signal) = signal {
            hits = hits.saturating_add(1);
            idle_seconds = 0;

            if !status.is_recording && hits >= AUTO_START_HITS {
                if let Err(err) =
                    start_recording(Some(app_handle.clone()), signal.title, vec![], None).await
                {
                    runtime().lock().last_error = Some(err);
                }
                hits = 0;
            }
        } else {
            hits = 0;
            if status.is_recording {
                idle_seconds = idle_seconds.saturating_add(AUTO_POLL_SECONDS);
                if idle_seconds >= AUTO_STOP_IDLE_SECONDS {
                    if let Err(err) = stop_recording().await {
                        runtime().lock().last_error = Some(err);
                    }
                    idle_seconds = 0;
                }
            } else {
                idle_seconds = 0;
            }
        }

        tokio::time::sleep(Duration::from_secs(AUTO_POLL_SECONDS)).await;
    }
}

async fn repair_failed_transcripts_once(app_state: Arc<AppState>) {
    maybe_bootstrap_python_backend(true);

    let store = match get_store() {
        Ok(store) => store,
        Err(err) => {
            tracing::warn!("Meeting transcript repair skipped: {}", err);
            return;
        }
    };

    let meetings = store.list_meetings();
    for meeting in meetings {
        if meeting.status == "recording" {
            continue;
        }

        let segments = store.get_segments_for_meeting(&meeting.id);
        if segments.is_empty() || !segments.iter().any(|s| should_retry_segment_text(&s.text)) {
            continue;
        }

        if let Err(err) =
            transcribe_meeting_postprocess(store.as_ref(), &meeting.id, &meeting.model)
        {
            tracing::warn!(
                "Meeting transcript repair failed for {}: {}",
                meeting.id,
                err
            );
            continue;
        }

        let transcript = match store.get_transcript(&meeting.id) {
            Ok(t) => t,
            Err(err) => {
                tracing::warn!(
                    "Failed to reload repaired transcript {}: {}",
                    meeting.id,
                    err
                );
                continue;
            }
        };
        let markdown = build_meeting_markdown(&transcript);
        let session_path = store.write_transcript_file(&meeting.id, &markdown).ok();
        let finder_path = store
            .write_transcript_to_documents(&meeting.id, &markdown)
            .ok();
        let transcript_path = finder_path.or(session_path);

        if let Some(path) = transcript_path.clone() {
            let _ = store.set_transcript_path(&meeting.id, Some(path.clone()));
            if let Err(err) =
                ingest_transcript_into_fndr_memory(app_state.clone(), &transcript, Some(&path))
            {
                tracing::warn!(
                    "Failed to ingest repaired transcript {} into memory: {}",
                    meeting.id,
                    err
                );
            }
        }
    }
}

#[derive(Debug, Clone)]
struct MeetingSignal {
    title: String,
}

async fn detect_meeting_signal(app_state: &AppState) -> Option<MeetingSignal> {
    let now = now_ms();
    let memories = app_state.store.get_recent_memories(1).await.ok()?;

    for memory in memories.iter().rev().take(80) {
        if now - memory.timestamp > 60_000 {
            break;
        }

        let mut haystack = format!(
            "{}\n{}\n{}",
            memory.window_title, memory.text, memory.snippet
        )
        .to_lowercase();

        if let Some(url) = memory.url.as_ref() {
            haystack.push('\n');
            haystack.push_str(&url.to_lowercase());
        }

        if contains_meeting_signal(&haystack) {
            let title = derive_meeting_title(&memory.window_title);
            return Some(MeetingSignal { title });
        }
    }

    None
}

fn contains_meeting_signal(haystack: &str) -> bool {
    let keyword_hits = [
        "meeting",
        "zoom",
        "google meet",
        "meet.google.com",
        "teams",
        "webex",
        "huddle",
        "standup",
        "all-hands",
        "call in progress",
        "recording",
    ]
    .into_iter()
    .filter(|k| haystack.contains(k))
    .count();

    let url_hits = [
        "zoom.us/j/",
        "meet.google.com/",
        "teams.microsoft.com/",
        "webex.com/",
        "whereby.com/",
        "around.co/",
    ]
    .into_iter()
    .any(|k| haystack.contains(k));

    url_hits || keyword_hits >= 2
}

fn derive_meeting_title(window_title: &str) -> String {
    let cleaned = window_title
        .replace(" - Zoom", "")
        .replace(" | Microsoft Teams", "")
        .replace(" - Google Meet", "")
        .trim()
        .to_string();

    if cleaned.is_empty() {
        "Detected Meeting".to_string()
    } else {
        cleaned
    }
}

async fn segment_worker(
    store: Arc<MeetingStore>,
    meeting_id: String,
    audio_dir: PathBuf,
    model: String,
    meeting_started_at: i64,
    stop_flag: Arc<AtomicBool>,
    app_handle: Option<AppHandle>,
) {
    let mut processed: HashSet<PathBuf> = HashSet::new();

    loop {
        let stopping = stop_flag.load(Ordering::SeqCst);
        let mut files = collect_segment_files(&audio_dir);

        if !stopping {
            if files.len() <= 1 {
                tokio::time::sleep(Duration::from_millis(900)).await;
                continue;
            }
            files.pop();
        }

        let mut did_work = false;
        for file_path in files {
            if processed.contains(&file_path) {
                continue;
            }

            if !stopping && is_recently_modified(&file_path, 1200) {
                continue;
            }

            let index = parse_segment_index(&file_path);
            let start_timestamp = meeting_started_at + (index as i64 * SEGMENT_SECONDS * 1000);
            let end_timestamp = start_timestamp + (SEGMENT_SECONDS * 1000);

            let segment = MeetingSegment {
                id: Uuid::new_v4().to_string(),
                meeting_id: meeting_id.clone(),
                index,
                start_timestamp,
                end_timestamp,
                text: String::new(),
                audio_chunk_path: file_path.to_string_lossy().to_string(),
                model: model.clone(),
                created_at: now_ms(),
            };

            if let Err(err) = store.add_segment(segment.clone()) {
                tracing::warn!("Failed to persist meeting segment: {}", err);
            } else if let Some(handle) = app_handle.as_ref() {
                let _ = handle.emit(SEGMENT_EVENT, &segment);
                if let Ok(status) = recorder_status() {
                    let _ = handle.emit(STATUS_EVENT, &status);
                }
            }

            processed.insert(file_path);
            did_work = true;
        }

        if stopping {
            let remaining = collect_segment_files(&audio_dir)
                .into_iter()
                .filter(|p| !processed.contains(p))
                .count();
            if remaining == 0 {
                break;
            }
        }

        if !did_work {
            tokio::time::sleep(Duration::from_millis(900)).await;
        }
    }
}

fn spawn_ffmpeg_recorder(segment_pattern: &Path) -> Result<Child, String> {
    let ffmpeg_path = resolve_ffmpeg_binary().ok_or_else(|| {
        "ffmpeg was not found. Install ffmpeg and restart FNDR to use meeting recording."
            .to_string()
    })?;

    if !ffmpeg_path.exists() && ffmpeg_path.as_os_str() != "ffmpeg" {
        return Err(
            "ffmpeg was not found. Install ffmpeg and restart FNDR to use meeting recording."
                .to_string(),
        );
    }

    let mut cmd = Command::new(ffmpeg_path);
    cmd.arg("-hide_banner").arg("-loglevel").arg("error");

    #[cfg(target_os = "macos")]
    {
        cmd.args(["-f", "avfoundation", "-i", ":0"]);
    }
    #[cfg(target_os = "linux")]
    {
        cmd.args(["-f", "pulse", "-i", "default"]);
    }
    #[cfg(target_os = "windows")]
    {
        cmd.args(["-f", "dshow", "-i", "audio=default"]);
    }

    cmd.args([
        "-ac",
        "1",
        "-ar",
        "16000",
        "-c:a",
        "pcm_s16le",
        "-f",
        "segment",
        "-segment_time",
        &SEGMENT_SECONDS.to_string(),
        "-reset_timestamps",
        "1",
    ]);
    cmd.arg(segment_pattern.to_string_lossy().to_string());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());

    cmd.spawn()
        .map_err(|e| format!("Failed to start ffmpeg meeting recorder: {e}"))
}

fn transcribe_segment(
    segment_path: &Path,
    model: &str,
    output_dir: &Path,
) -> Result<String, String> {
    let mut errors: Vec<String> = Vec::new();

    if let Ok(custom_cmd) = std::env::var("FNDR_PARAKEET_COMMAND") {
        let output = Command::new("sh")
            .arg("-c")
            .arg(custom_cmd)
            .env(
                "FNDR_AUDIO_PATH",
                segment_path.to_string_lossy().to_string(),
            )
            .env("FNDR_TRANSCRIPT_MODEL", model)
            .env(
                "FNDR_TRANSCRIPT_OUTPUT_DIR",
                output_dir.to_string_lossy().to_string(),
            )
            .output()
            .map_err(|e| format!("Parakeet command failed to start: {e}"))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !stdout.is_empty() {
                return Ok(stdout);
            }
            errors.push("FNDR_PARAKEET_COMMAND returned empty output".to_string());
        } else {
            errors.push(format!(
                "FNDR_PARAKEET_COMMAND failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
    }

    if let Some(text) = transcribe_with_sidecar(segment_path, &mut errors) {
        return Ok(text);
    }

    if let Some(text) = transcribe_with_python_whisper(segment_path, output_dir, &mut errors) {
        return Ok(text);
    }

    maybe_bootstrap_python_backend(false);

    if let Some(text) = transcribe_with_sidecar(segment_path, &mut errors) {
        return Ok(text);
    }

    if let Some(text) = transcribe_with_python_whisper(segment_path, output_dir, &mut errors) {
        return Ok(text);
    }

    let why = if errors.is_empty() {
        "No backend produced transcript text".to_string()
    } else {
        errors.join(" | ")
    };
    Err(format!(
        "Parakeet backend unavailable after auto-bootstrap. {}",
        why
    ))
}

fn detect_transcription_backend() -> String {
    if std::env::var("FNDR_PARAKEET_COMMAND").is_ok() {
        return "parakeet-v3-small".to_string();
    }
    if resolve_parakeet_sidecar().is_some() {
        return "parakeet-v3-small".to_string();
    }
    if has_python_module("whisper") {
        return "whisper-small-fallback".to_string();
    }
    "unavailable".to_string()
}

fn transcribe_with_sidecar(segment_path: &Path, errors: &mut Vec<String>) -> Option<String> {
    let sidecar_path = resolve_parakeet_sidecar()?;
    let python_cmd = python_for_sidecar().unwrap_or_else(|| PathBuf::from("python3"));
    let output = Command::new(python_cmd)
        .arg(sidecar_path)
        .arg(segment_path.to_string_lossy().to_string())
        .output();
    let Ok(output) = output else {
        errors.push("failed to launch sidecar python process".to_string());
        return None;
    };
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !stdout.is_empty() {
            errors.push(format!("parakeet sidecar: {}", stdout));
        }
        if !stderr.is_empty() {
            errors.push(format!("parakeet sidecar stderr: {}", stderr));
        }
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        errors.push("parakeet sidecar returned empty transcript".to_string());
        None
    } else {
        Some(stdout)
    }
}

fn maybe_bootstrap_python_backend(force: bool) {
    if has_python_module("faster_whisper") || has_python_module("whisper") {
        return;
    }

    let state = PY_BACKEND_BOOTSTRAP_STATE
        .get_or_init(|| Mutex::new(BackendBootstrapState { last_attempt_ms: 0 }));
    {
        let mut state = state.lock();
        let now = now_ms();
        if !force && now - state.last_attempt_ms < PY_BACKEND_BOOTSTRAP_COOLDOWN_MS {
            return;
        }
        state.last_attempt_ms = now;
    }

    if !command_exists("python3") {
        return;
    }

    let _ = Command::new("python3")
        .args(["-m", "ensurepip", "--upgrade"])
        .status();
    let _ = Command::new("python3")
        .args(["-m", "pip", "install", "--user", "--upgrade", "pip"])
        .status();
    let _ = Command::new("python3")
        .args([
            "-m",
            "pip",
            "install",
            "--user",
            "faster-whisper",
            "openai-whisper",
        ])
        .status();

    let Some(docs_root) = dirs::document_dir() else {
        return;
    };
    let venv_path = docs_root.join("FNDR Meetings").join("venv");
    let is_windows = cfg!(target_os = "windows");

    // 1. Create venv if it doesn't exist
    if !venv_path.exists() {
        tracing::info!("Creating FNDR meetings Python venv at {:?}", venv_path);
        let status = Command::new("python3")
            .args(["-m", "venv", &venv_path.to_string_lossy()])
            .status()
            .ok();

        if status.map_or(true, |s| !s.success()) {
            tracing::warn!("Failed to create python venv");
            return;
        }
    }

    // 2. Install dependencies inside the venv
    let pip_path = if is_windows {
        venv_path.join("Scripts").join("pip")
    } else {
        venv_path.join("bin").join("pip")
    };

    if !pip_path.exists() {
        tracing::warn!("Pip binary not found in venv {:?}", pip_path);
        return;
    }

    tracing::info!("Bootstrapping faster-whisper + openai-whisper via pip in venv...");
    let _ = Command::new(pip_path)
        .args(["install", "faster-whisper", "openai-whisper"])
        .status();
}

fn transcribe_with_python_whisper(
    segment_path: &Path,
    output_dir: &Path,
    errors: &mut Vec<String>,
) -> Option<String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(py) = python_for_sidecar() {
        candidates.push(py);
    }
    candidates.push(PathBuf::from("python3"));

    for py in candidates {
        let status = Command::new(&py)
            .args([
                "-m",
                "whisper",
                &segment_path.to_string_lossy(),
                "--model",
                "small",
                "--output_dir",
                &output_dir.to_string_lossy(),
                "--output_format",
                "txt",
                "--fp16",
                "False",
            ])
            .status();

        let Ok(status) = status else {
            errors.push(format!("failed to launch python whisper via {:?}", py));
            continue;
        };
        if !status.success() {
            errors.push(format!("python whisper exited non-zero via {:?}", py));
            continue;
        }

        let stem = segment_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("segment");
        let txt_path = output_dir.join(format!("{stem}.txt"));
        match fs::read_to_string(txt_path) {
            Ok(content) => {
                let text = content.trim().to_string();
                if !text.is_empty() {
                    return Some(text);
                }
                errors.push("python whisper produced empty transcript".to_string());
            }
            Err(_) => errors.push("python whisper did not write txt output".to_string()),
        }
    }
    None
}

fn python_for_sidecar() -> Option<PathBuf> {
    let docs_root = dirs::document_dir()?;
    let venv_path = docs_root.join("FNDR Meetings").join("venv");
    if cfg!(target_os = "windows") {
        let p = venv_path.join("Scripts").join("python");
        if p.exists() {
            return Some(p);
        }
    } else {
        let p = venv_path.join("bin").join("python3");
        if p.exists() {
            return Some(p);
        }
        let p2 = venv_path.join("bin").join("python");
        if p2.exists() {
            return Some(p2);
        }
    }
    None
}

fn transcribe_meeting_postprocess(
    store: &MeetingStore,
    meeting_id: &str,
    model: &str,
) -> Result<(), String> {
    let segments = store.get_segments_for_meeting(meeting_id);
    for segment in segments {
        if !should_retry_segment_text(&segment.text) {
            continue;
        }

        let audio_path = PathBuf::from(&segment.audio_chunk_path);
        let audio_dir = audio_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let text = match transcribe_segment(&audio_path, model, &audio_dir) {
            Ok(text) => text,
            Err(err) => {
                let size = fs::metadata(&audio_path).map(|m| m.len()).unwrap_or(0);
                // Ignore tiny/corrupt trailing chunks that can happen when a meeting stops mid-segment.
                if size < 32_000 {
                    String::new()
                } else {
                    format!(
                        "[Transcription unavailable for segment {}: {}]",
                        segment.index, err
                    )
                }
            }
        };

        store.set_segment_text(meeting_id, segment.index, text)?;
    }
    Ok(())
}

fn resolve_parakeet_sidecar() -> Option<PathBuf> {
    let packaged = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .map(|p| p.join("../Resources/sidecar/parakeet_runner.py"));

    if let Some(path) = packaged {
        if path.exists() {
            return Some(path);
        }
    }

    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sidecar/parakeet_runner.py");
    if dev.exists() {
        Some(dev)
    } else {
        None
    }
}

fn should_retry_segment_text(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.is_empty()
        || trimmed.starts_with("[Transcription unavailable for segment")
        || trimmed.contains("backend unavailable")
        || trimmed.starts_with("[parakeet-runner unavailable")
        || trimmed.starts_with("[parakeet-runner produced empty output")
}

fn has_python_module(module: &str) -> bool {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(py) = python_for_sidecar() {
        candidates.push(py);
    }
    candidates.push(PathBuf::from("python3"));

    for py in candidates {
        let status = Command::new(py)
            .arg("-c")
            .arg(format!("import {module}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if status.map(|s| s.success()).unwrap_or(false) {
            return true;
        }
    }
    false
}

fn build_meeting_markdown(transcript: &MeetingTranscript) -> String {
    let participants = if transcript.meeting.participants.is_empty() {
        "n/a".to_string()
    } else {
        transcript.meeting.participants.join(", ")
    };

    [
        format!("# {}", transcript.meeting.title),
        "".to_string(),
        format!("- Session ID: {}", transcript.meeting.id),
        format!("- Model: {}", transcript.meeting.model),
        format!(
            "- Started: {}",
            chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
                transcript.meeting.start_timestamp
            )
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339()
        ),
        format!("- Participants: {}", participants),
        "".to_string(),
        "## Transcript".to_string(),
        "".to_string(),
        if transcript.full_text.is_empty() {
            "(No transcript generated)".to_string()
        } else {
            transcript.full_text.clone()
        },
        "".to_string(),
    ]
    .join("\n")
}

fn ingest_transcript_into_fndr_memory(
    app_state: Arc<AppState>,
    transcript: &MeetingTranscript,
    transcript_path: Option<&str>,
) -> Result<(), String> {
    let now = transcript.meeting.end_timestamp.unwrap_or_else(now_ms);

    let snippet: String = transcript.full_text.chars().take(260).collect::<String>();

    let record = MemoryRecord {
        id: Uuid::new_v4().to_string(),
        timestamp: now,
        day_bucket: chrono::Utc::now().format("%Y-%m-%d").to_string(),
        app_name: "FNDR Meetings".to_string(),
        bundle_id: Some("com.fndr.meetings".to_string()),
        window_title: transcript.meeting.title.clone(),
        session_id: format!("meeting-{}", transcript.meeting.id),
        text: build_meeting_markdown(transcript),
        snippet: if snippet.is_empty() {
            "Meeting transcript captured".to_string()
        } else {
            snippet
        },
        embedding: vec![0.0; 384],
        image_embedding: vec![0.0; 512],
        screenshot_path: None,
        url: transcript_path.map(|p| p.to_string()),
    };

    app_state
        .store
        .add_batch(&[record.clone()])
        .map_err(|e| format!("Store add failed: {e}"))?;

    if let Err(err) = app_state.graph.ingest_memory(&record) {
        tracing::warn!("Graph ingest failed for meeting transcript: {}", err);
    }

    Ok(())
}

fn sanitize_filename(input: &str) -> String {
    let mut out = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    out = out.trim_matches('_').to_string();
    if out.is_empty() {
        "meeting".to_string()
    } else {
        out
    }
}

fn collect_segment_files(audio_dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(audio_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("wav"))
                .unwrap_or(false)
        })
        .collect();

    files.sort();
    files
}

fn parse_segment_index(path: &Path) -> u32 {
    let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
        return 0;
    };
    stem.rsplit('_')
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0)
}

fn is_recently_modified(path: &Path, threshold_ms: u64) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = meta.modified() else {
        return false;
    };
    let Ok(elapsed) = SystemTime::now().duration_since(modified) else {
        return false;
    };
    elapsed.as_millis() < threshold_ms as u128
}

fn command_exists(bin: &str) -> bool {
    Command::new("which")
        .arg(bin)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn resolve_ffmpeg_binary() -> Option<PathBuf> {
    if let Ok(custom) = std::env::var("FNDR_FFMPEG_PATH") {
        let p = PathBuf::from(custom);
        if p.exists() {
            return Some(p);
        }
    }

    if command_exists("ffmpeg") {
        return Some(PathBuf::from("ffmpeg"));
    }

    #[cfg(target_os = "macos")]
    {
        for candidate in [
            "/opt/homebrew/bin/ffmpeg",
            "/usr/local/bin/ffmpeg",
            "/opt/local/bin/ffmpeg",
            "/usr/bin/ffmpeg",
        ] {
            let path = PathBuf::from(candidate);
            if path.exists() {
                return Some(path);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        for candidate in ["/usr/bin/ffmpeg", "/usr/local/bin/ffmpeg"] {
            let path = PathBuf::from(candidate);
            if path.exists() {
                return Some(path);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        for candidate in [
            "C:\\ffmpeg\\bin\\ffmpeg.exe",
            "C:\\Program Files\\ffmpeg\\bin\\ffmpeg.exe",
            "C:\\Program Files (x86)\\ffmpeg\\bin\\ffmpeg.exe",
        ] {
            let path = PathBuf::from(candidate);
            if path.exists() {
                return Some(path);
            }
        }
    }

    None
}

fn get_store() -> Result<Arc<MeetingStore>, String> {
    runtime()
        .lock()
        .store
        .as_ref()
        .cloned()
        .ok_or_else(|| "Meeting runtime is not initialized".to_string())
}

fn read_json_or_default<T>(path: &Path) -> Vec<T>
where
    T: DeserializeOwned,
{
    if !path.exists() {
        return Vec::new();
    }
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).unwrap_or_default()
}

fn write_json<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create index parent dir: {e}"))?;
    }
    let file = File::create(path).map_err(|e| format!("Failed to create JSON index: {e}"))?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, value)
        .map_err(|e| format!("Failed to write JSON index: {e}"))
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
