//! Work sets over IPC (ADR 027): which thread of work a request means, and
//! opening the places of the one the person picked.

use std::sync::Arc;

use tauri::State;

use crate::workset::{self, ItemOutcome, Resolution};
use crate::AppState;

/// Resolves a request to a work set on this Mac. Reads only; nothing opens
/// and nothing leaves the Mac.
#[tauri::command]
pub async fn resolve_work_set(
    state: State<'_, Arc<AppState>>,
    query: String,
) -> Result<Resolution, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("Say which work to reopen.".to_string());
    }
    Ok(workset::resolve(state.inner(), query).await)
}

/// Opens the memories of a set the person picked, at most six, each checked
/// again for privacy and judged from its typed reopen outcome.
#[tauri::command]
pub async fn open_work_set(
    state: State<'_, Arc<AppState>>,
    memory_ids: Vec<String>,
) -> Result<Vec<ItemOutcome>, String> {
    if memory_ids.is_empty() {
        return Err("Nothing to open.".to_string());
    }
    Ok(workset::open_ids(state.inner(), &memory_ids).await)
}
