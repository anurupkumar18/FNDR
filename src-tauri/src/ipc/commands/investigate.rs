//! Investigate (ADR 039), for Search & Ask. `investigate` runs the loop and
//! returns the case file; each step arrives as an `investigate://step`
//! event with counts and route names only (ADR 021). The run only reads:
//! opening places and adding to-dos go through `open_work_set` and
//! `add_todo` when the person taps.

use crate::investigate::{self, CaseFile, StepEvent};
use crate::AppState;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};

pub const INVESTIGATE_STEP_EVENT: &str = "investigate://step";

/// Runs in flight, by the id the panel gave each one.
static RUNS: Lazy<Mutex<HashMap<String, Arc<AtomicBool>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StepPayload {
    run_id: String,
    #[serde(flatten)]
    step: StepEvent,
}

#[tauri::command]
pub async fn investigate(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    goal: String,
    run_id: String,
) -> Result<CaseFile, String> {
    let goal = goal.trim().to_string();
    if goal.is_empty() {
        return Err("Say what to investigate.".to_string());
    }
    let stop = Arc::new(AtomicBool::new(false));
    RUNS.lock().insert(run_id.clone(), Arc::clone(&stop));
    let file = investigate::run(state.inner(), &goal, &stop, |step| {
        let _ = app.emit(
            INVESTIGATE_STEP_EVENT,
            StepPayload {
                run_id: run_id.clone(),
                step: step.clone(),
            },
        );
    })
    .await;
    RUNS.lock().remove(&run_id);
    crate::telemetry::runtime_metrics::bump("fndr.investigate.runs");
    Ok(file)
}

/// Ends a run before its next step. A run that already ended is not an error.
#[tauri::command]
pub fn investigate_stop(run_id: String) -> Result<(), String> {
    if let Some(stop) = RUNS.lock().get(&run_id) {
        stop.store(true, Ordering::SeqCst);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::investigate::{StepKind, StepStatus};

    #[test]
    fn a_step_reaches_the_panel_flat_with_its_run_id_and_no_text_field() {
        let payload = StepPayload {
            run_id: "investigate-1".to_string(),
            step: StepEvent {
                index: 2,
                kind: StepKind::Thread,
                status: StepStatus::Completed,
                found: 16,
                added: 8,
                total: 16,
                marked: 0,
                routes: Vec::new(),
                inquiry: None,
                note: None,
                ms: 51,
            },
        };
        // The shape `InvestigateStepEvent` in tauri.ts reads.
        assert_eq!(
            serde_json::to_value(&payload).unwrap(),
            serde_json::json!({
                "runId": "investigate-1", "index": 2, "kind": "thread",
                "status": "completed", "found": 16, "added": 8, "total": 16,
                "marked": 0, "routes": [], "ms": 51
            })
        );
    }

    #[test]
    fn stop_reaches_only_the_run_it_names() {
        let mine = Arc::new(AtomicBool::new(false));
        let other = Arc::new(AtomicBool::new(false));
        RUNS.lock()
            .insert("stop-test-mine".to_string(), Arc::clone(&mine));
        RUNS.lock()
            .insert("stop-test-other".to_string(), Arc::clone(&other));

        investigate_stop("stop-test-mine".to_string()).unwrap();
        // A run that already ended is not an error.
        investigate_stop("stop-test-gone".to_string()).unwrap();

        assert!(mine.load(Ordering::SeqCst));
        assert!(!other.load(Ordering::SeqCst));
        RUNS.lock().remove("stop-test-mine");
        RUNS.lock().remove("stop-test-other");
    }
}
