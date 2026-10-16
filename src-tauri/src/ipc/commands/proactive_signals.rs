//! What changed in a Resume Work thread since it was last viewed.

use crate::proactive_signals::thread_digest::{self, ThreadDigest};
use crate::AppState;
use std::sync::Arc;
use tauri::State;

/// New pages, files, tasks and commits in a thread since `mark_thread_seen`
/// last ran for it, or over the last day for a thread never marked.
#[tauri::command]
pub async fn what_changed_since(
    state: State<'_, Arc<AppState>>,
    thread_key: String,
) -> Result<ThreadDigest, String> {
    thread_digest::what_changed_since(&state, &thread_key).await
}

/// Records that the person viewed a thread now.
#[tauri::command]
pub fn mark_thread_seen(state: State<'_, Arc<AppState>>, thread_key: String) -> Result<(), String> {
    if thread_key.trim().is_empty() {
        return Err("A thread key is required".into());
    }
    let now_ms = chrono::Utc::now().timestamp_millis();
    thread_digest::mark_seen(&state.state_store, &thread_key, now_ms)
}
