//! Hand-off over IPC (ADR 032, Part 8): the draft, one running job, its
//! progress as events, and the result. One job runs at a time, so Stop needs
//! no id. Progress goes out as `hand_off://progress`, the end as
//! `hand_off://finished`.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::handoff::draft::{self, DraftSource, HandOffDraft, OutputGoal, Selection, TaskSeed};
use crate::handoff::events::JobEvent;
use crate::handoff::job::{self, JobRequest};
use crate::handoff::store::{self, JobRecord};
use crate::handoff::toolchain::{self, Status};
use crate::operator::runs::Rating;
use crate::AppState;

pub const PROGRESS_EVENT: &str = "hand_off://progress";
pub const FINISHED_EVENT: &str = "hand_off://finished";

/// The Stop of the job that is running, if one is.
static RUNNING: Mutex<Option<Arc<AtomicBool>>> = Mutex::new(None);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandOffStatus {
    /// `missing`, `outdated` or `installed`.
    pub toolchain: &'static str,
    pub running: Option<JobRecord>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinishedPayload {
    pub record: Option<JobRecord>,
    /// Why the job never got a record, in one sentence.
    pub error: Option<String>,
}

fn status_of(state: &AppState) -> Result<HandOffStatus, String> {
    let toolchain = match toolchain::status(&job::toolchain_dir(state)) {
        Status::Missing => "missing",
        Status::Outdated => "outdated",
        Status::Installed => "installed",
    };
    let running = store::list(&state.state_store)?
        .into_iter()
        .find(|job| job.status.is_running());
    Ok(HandOffStatus { toolchain, running })
}

#[tauri::command]
pub async fn hand_off_status(state: State<'_, Arc<AppState>>) -> Result<HandOffStatus, String> {
    status_of(state.inner())
}

/// Installs FNDR's Python and document libraries for workers (about 200 MB).
#[tauri::command]
pub async fn hand_off_install_toolchain(
    state: State<'_, Arc<AppState>>,
) -> Result<HandOffStatus, String> {
    let state = state.inner().clone();
    let installed = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let uv = super::hermes_agent::ensure_uv_available(&installed)?;
        toolchain::install(&uv, &job::toolchain_dir(&installed))
    })
    .await
    .map_err(|e| e.to_string())??;
    status_of(&state)
}

/// What a hand-off would send, resolved on this Mac. Nothing leaves it.
#[tauri::command]
pub async fn hand_off_draft(
    state: State<'_, Arc<AppState>>,
    source: DraftSource,
) -> Result<HandOffDraft, String> {
    let tools = job::tools_line(&job::toolchain_dir(state.inner()));
    draft::build(state.inner(), source, &tools).await
}

/// The draft again with the person's own ticks and output.
#[tauri::command]
pub async fn hand_off_update_draft(
    state: State<'_, Arc<AppState>>,
    task_text: String,
    task_memory_id: Option<String>,
    goal: OutputGoal,
    memory_ids: Vec<String>,
    file_memory_ids: Vec<String>,
) -> Result<HandOffDraft, String> {
    let (seed, selection) = request_parts(task_text, task_memory_id, memory_ids, file_memory_ids);
    let tools = job::tools_line(&job::toolchain_dir(state.inner()));
    draft::recheck(state.inner(), &seed, goal, &selection, &tools).await
}

/// Starts the job the person reviewed. Returns once it has started; what it
/// does arrives as events.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn hand_off_start(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    task_text: String,
    task_memory_id: Option<String>,
    goal: OutputGoal,
    memory_ids: Vec<String>,
    file_memory_ids: Vec<String>,
    reviewed: String,
) -> Result<(), String> {
    let current = status_of(state.inner())?;
    if current.toolchain != "installed" {
        return Err("Install the document tools first (about 200 MB).".into());
    }
    if current.running.is_some() {
        return Err("A hand-off is already working. Stop it or wait for it to finish.".into());
    }
    let stop = Arc::new(AtomicBool::new(false));
    {
        let mut running = RUNNING
            .lock()
            .map_err(|_| "Hand-off state is unavailable.")?;
        if running.is_some() {
            return Err("A hand-off is already starting.".into());
        }
        *running = Some(Arc::clone(&stop));
    }
    let (seed, selection) = request_parts(task_text, task_memory_id, memory_ids, file_memory_ids);
    let request = JobRequest {
        seed,
        goal,
        selection,
        reviewed,
    };
    let state = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        let progress = app.clone();
        let result = job::start_job(&state, request, stop, |event: &JobEvent| {
            let _ = progress.emit(PROGRESS_EVENT, event);
        })
        .await;
        if let Ok(mut running) = RUNNING.lock() {
            *running = None;
        }
        let payload = match result {
            Ok(record) => FinishedPayload {
                record: Some(record),
                error: None,
            },
            Err(error) => FinishedPayload {
                record: None,
                error: Some(error),
            },
        };
        let _ = app.emit(FINISHED_EVENT, payload);
    });
    Ok(())
}

/// Stops the running job; what it wrote is kept.
#[tauri::command]
pub fn hand_off_stop() -> Result<(), String> {
    match RUNNING
        .lock()
        .map_err(|_| "Hand-off state is unavailable.")?
        .as_ref()
    {
        Some(stop) => {
            stop.store(true, Ordering::SeqCst);
            Ok(())
        }
        None => Err("No hand-off is working.".into()),
    }
}

/// Opens one verified deliverable of a job in its default app, and nothing
/// else: a path the verifier did not pass is refused.
#[tauri::command]
pub async fn hand_off_open(
    state: State<'_, Arc<AppState>>,
    job_id: String,
    path: String,
) -> Result<(), String> {
    let job = store::get(&state.state_store, &job_id)?;
    if !job.deliverables.contains(&path) {
        return Err("FNDR opens only files it checked.".into());
    }
    let full = Path::new(&job.folder).join(&path);
    if !full.is_file() {
        return Err("That file is no longer in the job folder.".into());
    }
    std::process::Command::new("open")
        .arg(&full)
        .spawn()
        .map_err(|e| format!("Could not open it: {e}"))?;
    Ok(())
}

#[tauri::command]
pub async fn hand_off_rate(
    state: State<'_, Arc<AppState>>,
    job_id: String,
    worked: bool,
) -> Result<JobRecord, String> {
    store::update(&state.state_store, &job_id, |job| {
        job.rating = Some(if worked {
            Rating::Worked
        } else {
            Rating::DidNotWork
        });
    })
}

fn request_parts(
    task_text: String,
    task_memory_id: Option<String>,
    memory_ids: Vec<String>,
    file_memory_ids: Vec<String>,
) -> (TaskSeed, Selection) {
    (
        TaskSeed {
            text: task_text,
            memory_id: task_memory_id,
        },
        Selection {
            memory_ids: Some(memory_ids.into_iter().collect::<HashSet<_>>()),
            file_memory_ids: Some(file_memory_ids.into_iter().collect::<HashSet<_>>()),
        },
    )
}
