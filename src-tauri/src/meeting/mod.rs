//! Meeting recorder runtime and persistence.
//!
//! This module provides local-only meeting recording with segmented audio
//! capture and local transcription.

use crate::{
    embed::Embedder,
    speech,
    store::{MeetingBreakdown, MeetingSegment, MeetingSession, MemoryRecord, Store},
    AppState,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime};
use tauri::{AppHandle, Emitter};
use tokio::task::JoinHandle;
use uuid::Uuid;

const MEETINGS_DIR: &str = "meetings";
const SEGMENT_SECONDS: i64 = 20;
const STATUS_EVENT: &str = "meeting://status";
const SEGMENT_EVENT: &str = "meeting://segment";
const FORCED_MODEL: &str = "whisper-large-v3-turbo-gguf";
const CONSENT_LOOKBACK_SEGMENTS: usize = 120;
const TRANSCRIPT_EMBED_TIMEOUT: Duration = Duration::from_secs(4);
const BREAKDOWN_LLM_TIMEOUT: Duration = Duration::from_secs(15);

static SHARED_MEETING_EMBEDDER: OnceLock<Result<Embedder, String>> = OnceLock::new();

fn meeting_embedder() -> Result<&'static Embedder, String> {
    match SHARED_MEETING_EMBEDDER.get_or_init(Embedder::new) {
        Ok(embedder) => Ok(embedder),
        Err(err) => Err(err.clone()),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeetingRecorderStatus {
    pub is_recording: bool,
    pub current_meeting_id: Option<String>,
    pub current_title: Option<String>,
    pub model: Option<String>,
    pub started_at: Option<i64>,
    pub segment_count: usize,
    pub consent_state: String,
    pub consent_evidence: Option<String>,
    pub consent_checked_segments: usize,
    pub ffmpeg_available: bool,
    pub transcription_backend: String,
    pub is_analyzing: bool,
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
    store: Arc<Store>,
}

impl MeetingStore {
    fn new(app_data_dir: PathBuf, store: Arc<Store>) -> Result<Self, String> {
        let root_dir = app_data_dir.join(MEETINGS_DIR);
        fs::create_dir_all(&root_dir).map_err(|e| format!("Failed to create meetings dir: {e}"))?;

        Ok(Self { root_dir, store })
    }

    async fn recover_unfinished(&self) -> Result<(), String> {
        let mut meetings = self
            .store
            .list_meetings()
            .await
            .map_err(|e| e.to_string())?;
        let mut touched = false;
        for meeting in meetings.iter_mut() {
            if meeting.status == "recording" {
                meeting.status = "stopped".to_string();
                meeting.end_timestamp = Some(now_ms());
                meeting.updated_at = now_ms();
                touched = true;
            }
        }
        if touched {
            self.store
                .upsert_meetings(&meetings)
                .await
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    async fn create_meeting(
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

        let rel_meeting_dir = MEETING_RELATIVE_PREFIX.to_string() + &meeting_id;
        let rel_audio_dir = rel_meeting_dir.clone() + "/audio";

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
            meeting_dir: rel_meeting_dir,
            audio_dir: rel_audio_dir,
            transcript_path: None,
            breakdown: None,
        };

        let mut meetings = self
            .store
            .list_meetings()
            .await
            .map_err(|e| e.to_string())?;
        meetings.push(meeting.clone());
        self.store
            .upsert_meetings(&meetings)
            .await
            .map_err(|e| e.to_string())?;
        Ok(meeting)
    }

    async fn set_meeting_status(&self, meeting_id: &str, status: &str) -> Result<(), String> {
        let mut meetings = self
            .store
            .list_meetings()
            .await
            .map_err(|e| e.to_string())?;
        if let Some(meeting) = meetings.iter_mut().find(|m| m.id == meeting_id) {
            meeting.status = status.to_string();
            meeting.updated_at = now_ms();
        }
        self.store
            .upsert_meetings(&meetings)
            .await
            .map_err(|e| e.to_string())
    }

    async fn set_meeting_error(&self, meeting_id: &str, message: &str) -> Result<(), String> {
        let mut meetings = self
            .store
            .list_meetings()
            .await
            .map_err(|e| e.to_string())?;
        if let Some(meeting) = meetings.iter_mut().find(|m| m.id == meeting_id) {
            meeting.status = "error".to_string();
            meeting.updated_at = now_ms();
            meeting.end_timestamp = Some(now_ms());
            meeting.transcript_path = Some(message.to_string());
        }
        self.store
            .upsert_meetings(&meetings)
            .await
            .map_err(|e| e.to_string())
    }

    async fn update_meeting_breakdown(
        &self,
        meeting_id: &str,
        breakdown: MeetingBreakdown,
        transcript_path: Option<String>,
    ) -> Result<(), String> {
        let mut meetings = self
            .store
            .list_meetings()
            .await
            .map_err(|e| e.to_string())?;
        if let Some(meeting) = meetings.iter_mut().find(|m| m.id == meeting_id) {
            meeting.status = "stopped".to_string();
            meeting.end_timestamp = Some(now_ms());
            meeting.updated_at = now_ms();
            meeting.transcript_path = transcript_path;
            meeting.breakdown = Some(breakdown);
            if let Some(end) = meeting.end_timestamp {
                meeting.duration_seconds = ((end - meeting.start_timestamp).max(0) / 1000) as u64;
            }
        }
        self.store
            .upsert_meetings(&meetings)
            .await
            .map_err(|e| e.to_string())
    }

    async fn add_segment(&self, segment: MeetingSegment) -> Result<(), String> {
        let meeting_id = segment.meeting_id.clone();
        let segment_end = segment.end_timestamp;

        self.store
            .upsert_segments(&[segment])
            .await
            .map_err(|e| e.to_string())?;

        let mut meetings = self
            .store
            .list_meetings()
            .await
            .map_err(|e| e.to_string())?;
        if let Some(meeting) = meetings.iter_mut().find(|m| m.id == meeting_id) {
            let segments = self
                .store
                .list_segments()
                .await
                .map_err(|e| e.to_string())?;
            meeting.segment_count = segments
                .iter()
                .filter(|s| s.meeting_id == meeting_id)
                .count();
            meeting.duration_seconds =
                ((segment_end - meeting.start_timestamp).max(0) / 1000) as u64;
            meeting.updated_at = now_ms();
        }
        self.store
            .upsert_meetings(&meetings)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    async fn list_meetings(&self) -> Vec<MeetingSession> {
        let mut meetings = self.store.list_meetings().await.unwrap_or_default();
        meetings.sort_by_key(|m| std::cmp::Reverse(m.start_timestamp));
        meetings
    }

    async fn get_meeting(&self, meeting_id: &str) -> Option<MeetingSession> {
        let meetings = self.store.list_meetings().await.unwrap_or_default();
        meetings.into_iter().find(|m| m.id == meeting_id)
    }

    async fn delete_meeting(&self, meeting_id: &str) -> Result<bool, String> {
        let mut meetings = self
            .store
            .list_meetings()
            .await
            .map_err(|e| e.to_string())?;
        let removed = if let Some(index) = meetings.iter().position(|m| m.id == meeting_id) {
            Some(meetings.remove(index))
        } else {
            None
        };

        let Some(meeting) = removed else {
            return Ok(false);
        };

        self.store
            .upsert_meetings(&meetings)
            .await
            .map_err(|e| e.to_string())?;

        // Removal of segments
        let mut segments = self
            .store
            .list_segments()
            .await
            .map_err(|e| e.to_string())?;
        segments.retain(|s| s.meeting_id != meeting_id);
        self.store
            .upsert_segments_full(&segments)
            .await
            .map_err(|e| e.to_string())?;

        if let Some(transcript_path) = meeting.transcript_path.as_ref() {
            let full_path = self.resolve_relative_path(transcript_path);
            if full_path.exists() {
                let _ = fs::remove_file(full_path);
            }
        }

        let meeting_dir = self.resolve_relative_path(&meeting.meeting_dir);
        if meeting_dir.exists() {
            fs::remove_dir_all(&meeting_dir)
                .map_err(|e| format!("Failed to remove meeting directory: {e}"))?;
        }

        Ok(true)
    }

    async fn get_segments_for_meeting(&self, meeting_id: &str) -> Vec<MeetingSegment> {
        let all = self.store.list_segments().await.unwrap_or_default();
        let mut segments: Vec<MeetingSegment> = all
            .into_iter()
            .filter(|s| s.meeting_id == meeting_id)
            .collect();
        segments.sort_by_key(|s| s.index);
        segments
    }

    async fn get_transcript(&self, meeting_id: &str) -> Result<MeetingTranscript, String> {
        let meeting = self
            .get_meeting(meeting_id)
            .await
            .ok_or_else(|| "Meeting not found".to_string())?;
        let segments = self.get_segments_for_meeting(meeting_id).await;
        let full_text = render_transcript_for_display(&meeting, &segments);

        Ok(MeetingTranscript {
            meeting,
            segments,
            full_text,
        })
    }

    // search API removed globally as per simplified model

    async fn set_segment_text(
        &self,
        meeting_id: &str,
        segment_index: u32,
        text: String,
    ) -> Result<(), String> {
        let mut segments = self
            .store
            .list_segments()
            .await
            .map_err(|e| e.to_string())?;
        if let Some(seg) = segments
            .iter_mut()
            .find(|s| s.meeting_id == meeting_id && s.index == segment_index)
        {
            seg.text = text;
        }
        self.store
            .upsert_segments_full(&segments)
            .await
            .map_err(|e| e.to_string())
    }

    async fn set_transcript_path(
        &self,
        meeting_id: &str,
        transcript_path: Option<String>,
    ) -> Result<(), String> {
        let mut meetings = self
            .store
            .list_meetings()
            .await
            .map_err(|e| e.to_string())?;
        if let Some(meeting) = meetings.iter_mut().find(|m| m.id == meeting_id) {
            meeting.transcript_path = transcript_path;
            meeting.updated_at = now_ms();
        }
        self.store
            .upsert_meetings(&meetings)
            .await
            .map_err(|e| e.to_string())
    }

    fn resolve_relative_path(&self, rel: &str) -> PathBuf {
        if let Some(stripped) = rel.strip_prefix(MEETING_RELATIVE_PREFIX) {
            self.root_dir.join(stripped)
        } else {
            PathBuf::from(rel)
        }
    }

    async fn purge_audio_chunks(&self, meeting_id: &str) -> Result<(), String> {
        let Some(meeting) = self.get_meeting(meeting_id).await else {
            return Ok(());
        };
        let audio_dir = self.resolve_relative_path(&meeting.audio_dir);
        if !audio_dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&audio_dir)
            .map_err(|e| format!("Failed reading audio dir for cleanup: {e}"))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let is_wav = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("wav"))
                .unwrap_or(false);
            if is_wav {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }
}

const MEETING_RELATIVE_PREFIX: &str = "rel://meetings/";

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
    last_error: Option<String>,
}

impl Default for MeetingRuntime {
    fn default() -> Self {
        Self {
            store: None,
            active: None,
            app_handle: None,
            app_state: None,
            last_error: None,
        }
    }
}

static RUNTIME: OnceLock<Mutex<MeetingRuntime>> = OnceLock::new();
static POSTPROCESS_IN_FLIGHT: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

fn runtime() -> &'static Mutex<MeetingRuntime> {
    RUNTIME.get_or_init(|| Mutex::new(MeetingRuntime::default()))
}

fn postprocess_in_flight() -> &'static Mutex<HashSet<String>> {
    POSTPROCESS_IN_FLIGHT.get_or_init(|| Mutex::new(HashSet::new()))
}

struct PostprocessGuard {
    meeting_id: String,
}

impl Drop for PostprocessGuard {
    fn drop(&mut self) {
        postprocess_in_flight().lock().remove(&self.meeting_id);
    }
}

async fn acquire_postprocess_guard(meeting_id: &str) -> PostprocessGuard {
    loop {
        let acquired = {
            let mut in_flight = postprocess_in_flight().lock();
            if in_flight.contains(meeting_id) {
                false
            } else {
                in_flight.insert(meeting_id.to_string());
                true
            }
        };

        if acquired {
            return PostprocessGuard {
                meeting_id: meeting_id.to_string(),
            };
        }

        tokio::time::sleep(Duration::from_millis(120)).await;
    }
}

pub async fn init(app_data_dir: PathBuf, store: Arc<Store>) -> Result<(), String> {
    let store = Arc::new(MeetingStore::new(app_data_dir, store)?);
    store.recover_unfinished().await?;

    let mut rt = runtime().lock();
    rt.store = Some(store);
    rt.last_error = None;
    Ok(())
}

pub fn bind_runtime(app_handle: AppHandle, app_state: Arc<AppState>) -> Result<(), String> {
    let mut rt = runtime().lock();
    rt.app_handle = Some(app_handle);
    rt.app_state = Some(app_state);
    Ok(())
}

pub async fn list_meetings() -> Result<Vec<MeetingSession>, String> {
    let store = get_store()?;
    Ok(store.list_meetings().await)
}

/// Return all segments for a given meeting, sorted by index.
pub async fn get_meeting_segments(meeting_id: &str) -> Vec<crate::store::MeetingSegment> {
    match get_store() {
        Ok(store) => store.get_segments_for_meeting(meeting_id).await,
        Err(_) => Vec::new(),
    }
}

pub async fn delete_meeting(meeting_id: &str) -> Result<bool, String> {
    let should_stop_active = {
        let rt = runtime().lock();
        rt.active
            .as_ref()
            .map(|active| active.meeting_id == meeting_id)
            .unwrap_or(false)
    };

    if should_stop_active {
        stop_recording().await?;
    }

    let store = get_store()?;
    store.delete_meeting(meeting_id).await
}

pub async fn get_meeting_transcript(meeting_id: &str) -> Result<MeetingTranscript, String> {
    let store = get_store()?;
    store.get_transcript(meeting_id).await
}

pub async fn search_meeting_transcripts(
    query: &str,
    limit: usize,
) -> Result<Vec<MeetingSearchResult>, String> {
    let normalized_query = query.trim().to_lowercase();
    if normalized_query.is_empty() {
        return Ok(Vec::new());
    }

    let store = get_store()?;
    let meetings = store.list_meetings().await;
    let meeting_titles: HashMap<String, String> = meetings
        .into_iter()
        .map(|meeting| (meeting.id, meeting.title))
        .collect();

    let terms = transcript_search_terms(&normalized_query);
    let mut results = Vec::new();
    let all_segments = store
        .store
        .list_segments()
        .await
        .map_err(|e| e.to_string())?;
    for segment in all_segments {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }
        let score = transcript_match_score(text, &normalized_query, &terms);
        if score <= 0.0 {
            continue;
        }

        let meeting_title = meeting_titles
            .get(&segment.meeting_id)
            .cloned()
            .unwrap_or_else(|| "Meeting".to_string());

        results.push(MeetingSearchResult {
            meeting_id: segment.meeting_id.clone(),
            meeting_title,
            segment_id: segment.id.clone(),
            index: segment.index,
            text: text.to_string(),
            score,
            start_timestamp: segment.start_timestamp,
            end_timestamp: segment.end_timestamp,
        });
    }

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.start_timestamp.cmp(&a.start_timestamp))
            .then_with(|| b.index.cmp(&a.index))
    });
    results.truncate(limit.max(1));
    Ok(results)
}

pub fn recorder_status() -> Result<MeetingRecorderStatus, String> {
    let rt = runtime().lock();
    let ffmpeg_available = resolve_ffmpeg_binary().is_some();
    let backend = detect_transcription_backend();

    if let Some(active) = rt.active.as_ref() {
        return Ok(MeetingRecorderStatus {
            is_recording: true,
            current_meeting_id: Some(active.meeting_id.clone()),
            current_title: Some(active.title.clone()),
            model: Some(active.model.clone()),
            started_at: Some(active.started_at),
            segment_count: 0,
            consent_state: "n/a".to_string(),
            consent_evidence: None,
            consent_checked_segments: 0,
            ffmpeg_available,
            transcription_backend: backend,
            is_analyzing: false,
            last_error: rt.last_error.clone(),
        });
    }

    let is_analyzing = postprocess_in_flight().lock().len() > 0;

    Ok(MeetingRecorderStatus {
        is_recording: false,
        current_meeting_id: None,
        current_title: None,
        model: None,
        started_at: None,
        segment_count: 0,
        consent_state: "unknown".to_string(),
        consent_evidence: None,
        consent_checked_segments: 0,
        ffmpeg_available,
        transcription_backend: backend,
        is_analyzing,
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

    let (store, app_for_worker) = {
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
        (store, app_for_worker)
    };

    let meeting = store
        .create_meeting(clean_title, clean_participants, FORCED_MODEL.to_string())
        .await?;

    let active_exists_after_create = {
        let rt = runtime().lock();
        rt.active.is_some()
    };
    if active_exists_after_create {
        let _ = store
            .set_meeting_error(
                &meeting.id,
                "Another meeting recording became active before this one started.",
            )
            .await;
        return Err("A meeting recording is already active".to_string());
    }

    let segment_pattern = store
        .resolve_relative_path(&meeting.audio_dir)
        .join("segment_%05d.wav");
    let recorder = match spawn_ffmpeg_recorder(&segment_pattern) {
        Ok(child) => child,
        Err(err) => {
            let _ = store.set_meeting_error(&meeting.id, &err).await;
            runtime().lock().last_error = Some(err.clone());
            return Err(err);
        }
    };

    let stop_flag = Arc::new(AtomicBool::new(false));
    let worker_stop_flag = stop_flag.clone();
    let worker = tokio::spawn(async move {
        // Just wait until stop_flag or meeting ends. No real-time transcription.
        while !worker_stop_flag.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    });

    let mut pending_active = Some(ActiveMeeting {
        meeting_id: meeting.id.clone(),
        title: meeting.title.clone(),
        model: meeting.model.clone(),
        started_at: meeting.start_timestamp,
        stop_flag,
        recorder,
        worker,
    });
    let active_already_present = {
        let mut rt = runtime().lock();
        if rt.active.is_some() {
            true
        } else {
            rt.active = pending_active.take();
            rt.last_error = None;
            false
        }
    };

    if active_already_present {
        if let Some(active) = pending_active {
            active.stop_flag.store(true, Ordering::SeqCst);
            let mut recorder = active.recorder;
            let _ = recorder.kill();
            let _ = recorder.wait();
            let _ = active.worker.await;
        }
        let _ = store
            .set_meeting_error(
                &meeting.id,
                "Another meeting recording became active before this one started.",
            )
            .await;
        return Err("A meeting recording is already active".to_string());
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

    // Move heavy work to background task
    let meeting_id_task = meeting_id.clone();
    let model_task = model.clone();
    let store_task = store.clone();
    let app_handle_task = app_handle.clone();
    let app_state_task = app_state.clone();

    tokio::spawn(async move {
        tracing::info!("Starting background post-processing for meeting: {}", meeting_id_task);
        
        // 0. Set meeting status to analyzing
        let _ = store_task.set_meeting_status(&meeting_id_task, "analyzing").await;
        if let Some(handle) = app_handle_task.clone() {
            if let Ok(status) = recorder_status() {
                let _ = handle.emit("meeting://status", &status);
            }
        }

        // 1. Perform unified transcription of ALL segments at high quality
        if let Err(err) = transcribe_meeting_postprocess(store_task.as_ref(), &meeting_id_task, &model_task).await {
            tracing::warn!("Post-meeting transcription pass failed: {}", err);
        }

        let run_post_summary = async {
            let transcript = store_task.get_transcript(&meeting_id_task).await?;
            let transcript_plain = transcript_plain_text(&transcript.segments);
            let has_transcript_signal = has_min_transcript_signal(&transcript_plain);
            let has_breakdown_signal = has_breakdown_ready_signal(&transcript_plain);

            // 2. Perform AI Breakdown analysis
            let mut breakdown = heuristic_breakdown_from_transcript(&transcript_plain);
            if has_breakdown_signal {
                if let Some(engine) = app_state_task.as_ref().and_then(|s| s.inference_engine()) {
                    tracing::info!("Starting AI breakdown for meeting: {}", meeting_id_task);
                    let breakdown_prompt = format!(
                        "Review this meeting transcript and provide a structured breakdown.\n\nTRANSCRIPT:\n{}\n\n\
                        Format your response exactly as:\n\
                        SUMMARY: [one paragraph summary]\n\
                        TODOS:\n- [task]\n\
                        REMINDERS:\n- [reminder]\n\
                        FOLLOWUPS:\n- [followup]",
                        transcript_plain
                    );
                    
                    // Add timeout to AI breakdown
                    let ai_result =
                        tokio::time::timeout(BREAKDOWN_LLM_TIMEOUT, engine.extract_todos(&breakdown_prompt)).await;

                    match ai_result {
                        Ok(raw) => {
                            let llm_breakdown = sanitize_breakdown(parse_breakdown_from_raw(&raw));
                            if breakdown_has_signal(&llm_breakdown) {
                                breakdown = llm_breakdown;
                            }
                        }
                        Err(_) => {
                            tracing::warn!("AI breakdown timed out for meeting: {}", meeting_id_task);
                            breakdown.summary = "AI breakdown timed out. Full transcript is still available below.".to_string();
                        }
                    }
                }
            } else {
                if has_transcript_signal {
                    breakdown.summary = "Transcript was captured but looked low-confidence or repetitive, so AI breakdown was skipped to avoid inaccurate summary."
                        .to_string();
                } else {
                    let mut msg = "Transcript did not capture readable speech. Check the full transcript below for per-segment diagnostics and audio-device issues.".to_string();
                    #[cfg(target_os = "macos")]
                    {
                        msg.push_str("\n\nTip: On macOS, to capture system audio (like movies or calls), you must install a virtual audio device (e.g., BlackHole) and use a Multi-Output Device.");
                    }
                    breakdown.summary = msg;
                }
            }

            if !breakdown_has_signal(&breakdown) {
                breakdown.summary =
                    "Meeting recorded, but no reliable summary could be extracted from transcript."
                        .to_string();
            }

            if breakdown.todos.is_empty() && breakdown.reminders.is_empty() && breakdown.followups.is_empty() {
                if let Some(action_hint) = find_action_hint(&transcript_plain) {
                    breakdown.todos.push(action_hint);
                }
            }

            breakdown = sanitize_breakdown(breakdown);
            if breakdown.summary.is_empty() {
                breakdown.summary =
                    "No summary extracted yet. Expand full transcript to inspect raw meeting text."
                        .to_string();
            }

            if !has_transcript_signal {
                let diagnostic = transcript
                    .segments
                    .iter()
                    .map(|segment| segment.text.trim())
                    .find(|line| line.starts_with("[Transcription unavailable"))
                    .map(|line| line.to_string());
                if let Some(line) = diagnostic {
                    breakdown.reminders.push(line);
                    breakdown = sanitize_breakdown(breakdown);
                }
            }

            if breakdown.summary.len() > 480 {
                breakdown.summary = breakdown.summary.chars().take(477).collect::<String>() + "...";
            }

            if let Some(state) = app_state_task.clone() {
                if let Err(err) = ingest_transcript_into_fndr_memory(state, &transcript, None).await {
                    tracing::warn!("Failed to ingest meeting transcript into memory: {}", err);
                }
            }

            let _ = store_task.update_meeting_breakdown(&meeting_id_task, breakdown, None).await;
            let _ = store_task.purge_audio_chunks(&meeting_id_task).await;
            
            Ok::<(), String>(())
        };

        if let Err(err) = run_post_summary.await {
            tracing::error!("Meeting background analysis failed: {}", err);
        }

        // Emit final status update
        if let Some(handle) = app_handle_task {
             if let Ok(status) = recorder_status() {
                 let _ = handle.emit("meeting://status", &status);
             }
        }
    });

    recorder_status()
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
        let av_input = resolve_macos_audio_input_spec();
        tracing::info!("Meeting recorder using avfoundation input {}", av_input);
        cmd.args(["-f", "avfoundation", "-i", av_input.as_str()]);
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

#[cfg(target_os = "macos")]
fn resolve_macos_audio_input_spec() -> String {
    if let Ok(explicit) = std::env::var("FNDR_MEETING_AUDIO_DEVICE") {
        let trimmed = explicit.trim().trim_start_matches(':');
        if !trimmed.is_empty() {
            return format!(":{trimmed}");
        }
    }

    if let Some(loopback_index) = detect_macos_loopback_audio_device_index() {
        return format!(":{loopback_index}");
    }

    ":0".to_string()
}

#[cfg(target_os = "macos")]
fn detect_macos_loopback_audio_device_index() -> Option<String> {
    let ffmpeg_path = resolve_ffmpeg_binary()?;
    let output = Command::new(ffmpeg_path)
        .arg("-f")
        .arg("avfoundation")
        .arg("-list_devices")
        .arg("true")
        .arg("-i")
        .arg("")
        .output()
        .ok()?;

    let listing = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let mut in_audio = false;
    for line in listing.lines() {
        let lowered = line.to_lowercase();
        if lowered.contains("avfoundation audio devices") {
            in_audio = true;
            continue;
        }
        if lowered.contains("avfoundation video devices") {
            in_audio = false;
            continue;
        }
        if !in_audio {
            continue;
        }

        let Some(index) = extract_avfoundation_index(line) else {
            continue;
        };

        let has_loopback_hint = [
            "blackhole",
            "loopback",
            "soundflower",
            "vb-cable",
            "background music",
            "virtual audio",
        ]
        .into_iter()
        .any(|needle| lowered.contains(needle));

        if has_loopback_hint {
            return Some(index);
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn extract_avfoundation_index(line: &str) -> Option<String> {
    for section in line.split('[').skip(1) {
        let candidate = section.split(']').next().unwrap_or("").trim();
        if !candidate.is_empty() && candidate.chars().all(|c| c.is_ascii_digit()) {
            return Some(candidate.to_string());
        }
    }
    None
}

async fn transcribe_segment(
    segment_path: &Path,
    model: &str,
    app_data_dir: &Path,
) -> Result<String, String> {
    if let Ok(custom_cmd) = std::env::var("FNDR_MEETING_TRANSCRIBE_COMMAND")
        .or_else(|_| std::env::var("FNDR_PARAKEET_COMMAND"))
    {
        let audio = segment_path.to_path_buf();
        let model_name = model.to_string();
        let app_data = app_data_dir.to_path_buf();
        let output = tokio::task::spawn_blocking(move || {
            Command::new("sh")
                .arg("-c")
                .arg(custom_cmd)
                .env("FNDR_AUDIO_PATH", audio.to_string_lossy().to_string())
                .env("FNDR_TRANSCRIPT_MODEL", model_name)
                .env(
                    "FNDR_TRANSCRIPT_APP_DATA_DIR",
                    app_data.to_string_lossy().to_string(),
                )
                .output()
        })
        .await
        .map_err(|e| format!("Custom meeting transcription task failed: {e}"))?
        .map_err(|e| format!("Custom meeting transcription command failed to start: {e}"))?;

        if output.status.success() {
            let stdout = normalize_transcribed_text(&String::from_utf8_lossy(&output.stdout));
            if !stdout.is_empty() {
                return Ok(stdout);
            }
            return Err("Custom meeting transcription command returned empty output".to_string());
        }

        return Err(format!(
            "Custom meeting transcription command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    let primary = speech::transcribe_audio_file(app_data_dir, segment_path).await;
    match primary {
        Ok(text) => {
            let normalized = normalize_transcribed_text(&text);
            if has_min_transcript_signal(&normalized) {
                return Ok(normalized);
            }

            let fallback = speech::transcribe_audio_file_voice_command(app_data_dir, segment_path)
                .await
                .map(|value| normalize_transcribed_text(&value))?;
            if fallback.is_empty() {
                Err("Whisper GGUF runner returned empty transcript".to_string())
            } else {
                Ok(fallback)
            }
        }
        Err(primary_err) => {
            let fallback = speech::transcribe_audio_file_voice_command(app_data_dir, segment_path)
                .await
                .map(|value| normalize_transcribed_text(&value))
                .map_err(|fallback_err| {
                    format!(
                        "Whisper default mode failed: {}; voice-command fallback failed: {}",
                        primary_err, fallback_err
                    )
                })?;
            if fallback.is_empty() {
                Err("Whisper GGUF runner returned empty transcript".to_string())
            } else {
                Ok(fallback)
            }
        }
    }
}

fn normalize_transcribed_text(raw: &str) -> String {
    let sanitized = raw
        .replace("[BLANK_AUDIO]", " ")
        .replace("[ Silence ]", " ")
        .replace("[SILENCE]", " ")
        .replace("[MUSIC]", " ")
        .replace("[NOISE]", " ");
    sanitized
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn has_min_transcript_signal(text: &str) -> bool {
    let words = text.split_whitespace().count();
    words >= 4
}

fn has_breakdown_ready_signal(text: &str) -> bool {
    let tokens = text.split_whitespace().collect::<Vec<_>>();
    if tokens.len() < 8 {
        return false;
    }

    let mut word_freq: HashMap<String, usize> = HashMap::new();
    let mut alpha_tokens = 0usize;
    let mut bracketed_tokens = 0usize;

    for token in &tokens {
        let trimmed = token.trim_matches(|c: char| {
            c.is_ascii_punctuation() || matches!(c, '[' | ']' | '(' | ')' | '{' | '}')
        });
        if trimmed.is_empty() {
            continue;
        }

        if token.contains('[') || token.contains(']') {
            bracketed_tokens += 1;
        }
        if trimmed.chars().any(|c| c.is_ascii_alphabetic()) {
            alpha_tokens += 1;
        }
        *word_freq.entry(trimmed.to_lowercase()).or_insert(0) += 1;
    }

    let total = tokens.len().max(1);
    let max_repeat = word_freq.values().copied().max().unwrap_or(0);
    let unique = word_freq.len();
    let alpha_ratio = alpha_tokens as f32 / total as f32;
    let unique_ratio = unique as f32 / total as f32;
    let repeat_ratio = max_repeat as f32 / total as f32;
    let bracket_ratio = bracketed_tokens as f32 / total as f32;

    let lower = text.to_lowercase();
    let has_known_hallucination_marker = lower.contains("top left")
        || lower.contains("subscribe")
        || lower.contains("thank you for watching");

    alpha_ratio >= 0.6
        && unique_ratio >= 0.35
        && repeat_ratio <= 0.45
        && bracket_ratio <= 0.2
        && !has_known_hallucination_marker
}

fn transcript_plain_text(segments: &[MeetingSegment]) -> String {
    segments
        .iter()
        .map(|segment| segment.text.trim())
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("[Segment"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_transcript_for_display(meeting: &MeetingSession, segments: &[MeetingSegment]) -> String {
    if segments.is_empty() {
        return format!(
            "No recorded segments found for this meeting. Title: {}",
            meeting.title
        );
    }

    let mut rendered = Vec::with_capacity(segments.len());
    for segment in segments {
        let offset_start_sec =
            ((segment.start_timestamp - meeting.start_timestamp).max(0) as f64) / 1000.0;
        let offset_end_sec =
            ((segment.end_timestamp - meeting.start_timestamp).max(0) as f64) / 1000.0;
        let text = if segment.text.trim().is_empty() {
            "(empty transcription; possible silence or capture issue)".to_string()
        } else {
            segment.text.trim().to_string()
        };
        rendered.push(format!(
            "[Segment {} | +{:.1}s to +{:.1}s] {}",
            segment.index, offset_start_sec, offset_end_sec, text
        ));
    }

    rendered.join("\n")
}

fn parse_breakdown_from_raw(raw: &str) -> MeetingBreakdown {
    let mut breakdown = MeetingBreakdown::default();
    let mut section = "";

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("SUMMARY:") {
            breakdown.summary = trimmed["SUMMARY:".len()..].trim().to_string();
            section = "summary";
            continue;
        }
        if trimmed.starts_with("TODOS:") {
            section = "todos";
            continue;
        }
        if trimmed.starts_with("REMINDERS:") {
            section = "reminders";
            continue;
        }
        if trimmed.starts_with("FOLLOWUPS:") || trimmed.starts_with("FOLLOW-UPS:") {
            section = "followups";
            continue;
        }

        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            let item = trimmed[2..].trim().to_string();
            match section {
                "todos" => breakdown.todos.push(item),
                "reminders" => breakdown.reminders.push(item),
                "followups" => breakdown.followups.push(item),
                _ => {}
            }
        } else if section == "summary" && breakdown.summary.is_empty() {
            breakdown.summary = trimmed.to_string();
        }
    }

    breakdown
}

fn sanitize_breakdown(mut breakdown: MeetingBreakdown) -> MeetingBreakdown {
    breakdown.summary = sanitize_breakdown_summary(&breakdown.summary);
    breakdown.todos = sanitize_breakdown_items(&breakdown.todos);
    breakdown.reminders = sanitize_breakdown_items(&breakdown.reminders);
    breakdown.followups = sanitize_breakdown_items(&breakdown.followups);
    breakdown
}

fn sanitize_breakdown_summary(summary: &str) -> String {
    let normalized = summary.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return String::new();
    }

    let lower = normalized.to_lowercase();
    if lower == "[summary]"
        || lower == "summary"
        || lower == "n/a"
        || lower == "none"
        || lower.starts_with("the team reviewed project timelines")
    {
        return String::new();
    }

    normalized
}

fn sanitize_breakdown_items(items: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut cleaned = Vec::new();

    for item in items {
        let normalized = item
            .trim()
            .trim_start_matches("- ")
            .trim_start_matches("* ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");

        if normalized.is_empty() {
            continue;
        }

        let lower = normalized.to_lowercase();
        if matches!(
            lower.as_str(),
            "[task]"
                | "[todo]"
                | "[reminder]"
                | "[followup]"
                | "[follow-up]"
                | "task"
                | "todo"
                | "reminder"
                | "followup"
                | "follow-up"
                | "n/a"
                | "none"
        ) {
            continue;
        }

        let looks_like_bracket_placeholder =
            normalized.starts_with('[') && normalized.ends_with(']') && normalized.len() <= 24;
        if looks_like_bracket_placeholder {
            continue;
        }

        if seen.insert(lower) {
            cleaned.push(normalized);
        }
    }

    cleaned
}

fn breakdown_has_signal(breakdown: &MeetingBreakdown) -> bool {
    !breakdown.summary.trim().is_empty()
        || !breakdown.todos.is_empty()
        || !breakdown.reminders.is_empty()
        || !breakdown.followups.is_empty()
}

fn heuristic_breakdown_from_transcript(text: &str) -> MeetingBreakdown {
    let mut breakdown = MeetingBreakdown::default();
    if text.trim().is_empty() {
        return breakdown;
    }

    let normalized = text.replace('\n', " ").replace('?', ".").replace('!', ".");
    let sentences = normalized
        .split('.')
        .map(str::trim)
        .filter(|sentence| !sentence.is_empty())
        .collect::<Vec<_>>();

    let mut summary_parts = Vec::new();
    for sentence in sentences.iter().take(2) {
        let clipped = sentence
            .split_whitespace()
            .take(28)
            .collect::<Vec<_>>()
            .join(" ");
        if !clipped.is_empty() {
            summary_parts.push(clipped);
        }
    }
    if !summary_parts.is_empty() {
        breakdown.summary = format!("{}.", summary_parts.join(". "));
    }

    breakdown
}

fn find_action_hint(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }

    let normalized = text.replace('\n', ". ");
    let lines = normalized
        .split('.')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    for line in lines {
        let lower = line.to_lowercase();
        if lower.contains("next")
            || lower.contains("follow up")
            || lower.contains("todo")
            || lower.contains("action")
            || lower.contains("deadline")
            || lower.contains("send")
            || lower.contains("schedule")
        {
            let clipped = line
                .split_whitespace()
                .take(18)
                .collect::<Vec<_>>()
                .join(" ");
            if !clipped.is_empty() {
                return Some(clipped);
            }
        }
    }
    None
}

fn detect_transcription_backend() -> String {
    if std::env::var("FNDR_MEETING_TRANSCRIBE_COMMAND").is_ok()
        || std::env::var("FNDR_PARAKEET_COMMAND").is_ok()
    {
        return "custom-transcriber".to_string();
    }
    if speech::resolve_sidecar("whisper_gguf_runner.py").is_some() {
        return "whisper-large-v3-turbo-gguf (on-demand)".to_string();
    }
    "unavailable".to_string()
}

async fn transcribe_meeting_postprocess(
    store: &MeetingStore,
    meeting_id: &str,
    model: &str,
) -> Result<(), String> {
    use futures::stream::{self, StreamExt};
    let _guard = acquire_postprocess_guard(meeting_id).await;

    let app_data_dir = store
        .root_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| store.root_dir.clone());
    
    let meeting = store
        .get_meeting(meeting_id)
        .await
        .ok_or_else(|| format!("Meeting {meeting_id} not found during post-process"))?;
    let audio_dir = store.resolve_relative_path(&meeting.audio_dir);
    let files = collect_segment_files(&audio_dir);
    let existing_segments = store.get_segments_for_meeting(meeting_id).await;
    let existing_indices: std::collections::HashSet<u32> = existing_segments.iter().map(|s| s.index).collect();

    for file_path in files {
        let index = parse_segment_index(&file_path);
        if !existing_indices.contains(&index) {
            let start_ts = meeting.start_timestamp + (index as i64 * SEGMENT_SECONDS * 1000);
            let end_ts = start_ts + (SEGMENT_SECONDS * 1000);
            let segment = MeetingSegment {
                id: Uuid::new_v4().to_string(),
                meeting_id: meeting_id.to_string(),
                index,
                start_timestamp: start_ts,
                end_timestamp: end_ts,
                text: "".to_string(),
                audio_chunk_path: file_path.to_string_lossy().to_string(),
                model: model.to_string(),
                created_at: now_ms(),
            };
            if let Err(e) = store.add_segment(segment).await {
                tracing::warn!("Failed to add discovered segment {} to store: {}", index, e);
            }
        }
    }

    let segments = store.get_segments_for_meeting(meeting_id).await;
    let store_arc = Arc::new(store.store.clone()); // We need Arc for store access in tasks
    let root_dir = store.root_dir.clone();

    // Parallel transcription with concurrency limit of 2
    let stream = stream::iter(segments);
    stream
        .map(|segment| {
            let app_data_dir = app_data_dir.clone();
            let model = model.to_string();
            let meeting_id_inner = meeting_id.to_string();
            let store_inner = store_arc.clone();
            let root_dir_inner = root_dir.clone();
            
            async move {
                let latest_segments = store_inner.list_segments().await.unwrap_or_default();
                let latest_text = latest_segments.into_iter()
                    .find(|candidate| candidate.meeting_id == meeting_id_inner && candidate.index == segment.index)
                    .map(|candidate| candidate.text)
                    .unwrap_or_default();

                if !should_retry_segment_text(&latest_text) {
                    return Ok(());
                }

                let audio_path = PathBuf::from(&segment.audio_chunk_path);
                let text = match transcribe_segment(&audio_path, &model, &app_data_dir).await {
                    Ok(text) => text,
                    Err(err) => {
                        let size = fs::metadata(&audio_path).map(|m| m.len()).unwrap_or(0);
                        if size < 32_000 {
                            format!("[Segment {} had no usable audio ({} bytes).]", segment.index, size)
                        } else {
                            format!("[Transcription unavailable for segment {}: {}]", segment.index, err)
                        }
                    }
                };

                // Inline persistence to avoid double-borrow of MeetingStore
                let mut all_segments = store_inner.list_segments().await.map_err(|e| e.to_string())?;
                if let Some(seg) = all_segments.iter_mut().find(|s| s.meeting_id == meeting_id_inner && s.index == segment.index) {
                    seg.text = text;
                }
                store_inner.upsert_segments_full(&all_segments).await.map_err(|e| e.to_string())?;
                
                Ok::<(), String>(())
            }
        })
        .buffer_unordered(2)
        .collect::<Vec<_>>()
        .await;

    Ok(())
}

fn transcript_search_terms(query: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in query.split_whitespace() {
        if token.len() > 1 && !out.iter().any(|existing| existing == token) {
            out.push(token.to_string());
        }
    }
    out
}

fn transcript_match_score(text: &str, normalized_query: &str, terms: &[String]) -> f32 {
    let normalized_text = text.to_lowercase();
    if normalized_text.contains(normalized_query) {
        return 1.0;
    }
    if terms.is_empty() {
        return 0.0;
    }

    let mut matched = 0usize;
    for term in terms {
        if normalized_text.contains(term) {
            matched += 1;
        }
    }

    matched as f32 / terms.len() as f32
}

fn should_retry_segment_text(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.is_empty()
        || trimmed.starts_with("[Transcription unavailable for segment")
        || trimmed.contains("backend unavailable")
        || trimmed.contains("Whisper GGUF runner returned empty transcript")
        || trimmed.contains("Custom meeting transcription command returned empty output")
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

async fn ingest_transcript_into_fndr_memory(
    app_state: Arc<AppState>,
    transcript: &MeetingTranscript,
    transcript_path: Option<&str>,
) -> Result<(), String> {
    let now = transcript.meeting.end_timestamp.unwrap_or_else(now_ms);

    let snippet: String = transcript.full_text.chars().take(260).collect::<String>();
    let embedding_text = if transcript.full_text.trim().is_empty() {
        build_meeting_markdown(transcript)
    } else {
        transcript.full_text.clone()
    };
    let text_embedding = match meeting_embedder() {
        Ok(embedder) => {
            let embed_text = embedding_text.clone();
            match tokio::time::timeout(
                TRANSCRIPT_EMBED_TIMEOUT,
                tokio::task::spawn_blocking(move || embedder.embed_batch(&[embed_text])),
            )
            .await
            {
                Ok(Ok(Ok(mut vectors))) => {
                    vectors.drain(..).next().unwrap_or_else(|| vec![0.0; 384])
                }
                Ok(Ok(Err(err))) => {
                    tracing::warn!("Meeting transcript embedding failed: {}", err);
                    vec![0.0; 384]
                }
                Ok(Err(join_err)) => {
                    tracing::warn!(
                        "Meeting transcript embedding task join failed: {}",
                        join_err
                    );
                    vec![0.0; 384]
                }
                Err(_) => {
                    tracing::warn!(
                        "Meeting transcript embedding timed out after {}ms",
                        TRANSCRIPT_EMBED_TIMEOUT.as_millis()
                    );
                    vec![0.0; 384]
                }
            }
        }
        Err(err) => {
            tracing::warn!(
                "Meeting transcript embedder unavailable; storing zero vector: {}",
                err
            );
            vec![0.0; 384]
        }
    };

    let record = MemoryRecord {
        id: Uuid::new_v4().to_string(),
        timestamp: now,
        day_bucket: chrono::Local::now().format("%Y-%m-%d").to_string(),
        app_name: "FNDR Meetings".to_string(),
        bundle_id: Some("com.fndr.meetings".to_string()),
        window_title: transcript.meeting.title.clone(),
        session_id: format!("meeting-{}", transcript.meeting.id),
        text: build_meeting_markdown(transcript),
        clean_text: transcript.full_text.clone(),
        ocr_confidence: 1.0,
        ocr_block_count: transcript.segments.len() as u32,
        snippet: if snippet.is_empty() {
            "Meeting transcript captured".to_string()
        } else {
            snippet
        },
        summary_source: "fallback".to_string(),
        noise_score: 0.0,
        session_key: format!("meeting:{}", transcript.meeting.id),
        embedding: text_embedding,
        image_embedding: vec![0.0; 512],
        screenshot_path: None,
        url: transcript_path.map(|p| p.to_string()),
        snippet_embedding: vec![0.0; 384],
        decay_score: 1.0,
        last_accessed_at: 0,
    };

    app_state
        .store
        .add_batch(&[record.clone()])
        .await
        .map_err(|e| format!("Store add failed: {e}"))?;

    if let Err(err) = app_state.graph.ingest_memory(&record).await {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_breakdown_drops_placeholders() {
        let raw = MeetingBreakdown {
            summary: "Summary".to_string(),
            todos: vec!["[task]".to_string(), "Ship investor deck".to_string()],
            reminders: vec!["[reminder]".to_string()],
            followups: vec![
                "[followup]".to_string(),
                "Follow up with design".to_string(),
            ],
        };
        let clean = sanitize_breakdown(raw);
        assert!(clean.summary.is_empty());
        assert_eq!(clean.todos, vec!["Ship investor deck".to_string()]);
        assert!(clean.reminders.is_empty());
        assert_eq!(clean.followups, vec!["Follow up with design".to_string()]);
    }

    #[test]
    fn render_transcript_shows_segment_markers_for_empty_text() {
        let meeting = MeetingSession {
            id: "m1".to_string(),
            title: "Test meeting".to_string(),
            participants: Vec::new(),
            model: "whisper".to_string(),
            status: "stopped".to_string(),
            start_timestamp: 1_000,
            end_timestamp: Some(10_000),
            created_at: 1_000,
            updated_at: 10_000,
            segment_count: 1,
            duration_seconds: 9,
            meeting_dir: "rel://meetings/m1".to_string(),
            audio_dir: "rel://meetings/m1/audio".to_string(),
            transcript_path: None,
            breakdown: None,
        };
        let segments = vec![MeetingSegment {
            id: "s1".to_string(),
            meeting_id: "m1".to_string(),
            index: 0,
            start_timestamp: 1_000,
            end_timestamp: 5_000,
            text: "".to_string(),
            audio_chunk_path: "/tmp/segment.wav".to_string(),
            model: "whisper".to_string(),
            created_at: 1_000,
        }];

        let rendered = render_transcript_for_display(&meeting, &segments);
        assert!(rendered.contains("[Segment 0"));
        assert!(rendered.contains("empty transcription"));
    }
}

fn get_store() -> Result<Arc<MeetingStore>, String> {
    runtime()
        .lock()
        .store
        .as_ref()
        .cloned()
        .ok_or_else(|| "Meeting runtime is not initialized".to_string())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
