//! Import source controls. Connectors only scan; the runner owns all writes.

use crate::import::{consent, ImportKind, ImportRunner, ImportStatus, SourceInfo};
use crate::AppState;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn import_list_sources(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SourceInfo>, String> {
    Ok(ImportRunner::global().list_sources(&state).await)
}

#[tauri::command]
pub async fn import_get_consent(
    state: State<'_, Arc<AppState>>,
    source: String,
) -> Result<bool, String> {
    let kind = ImportKind::parse(&source).ok_or("Unknown import source")?;
    consent::get(&state.state_store, kind)
}

#[tauri::command]
pub async fn import_set_consent(
    state: State<'_, Arc<AppState>>,
    source: String,
    enabled: bool,
) -> Result<(), String> {
    let kind = ImportKind::parse(&source).ok_or("Unknown import source")?;
    if !enabled {
        ImportRunner::global().cancel(&source);
    }
    consent::set(&state.state_store, kind, enabled)
}

#[tauri::command]
pub async fn import_start(
    state: State<'_, Arc<AppState>>,
    source: String,
    window_days: u32,
) -> Result<ImportStatus, String> {
    ImportRunner::global().start(state.inner().clone(), &source, window_days)
}

#[tauri::command]
pub async fn import_status(source: String) -> Result<ImportStatus, String> {
    Ok(ImportRunner::global().status(&source))
}

#[tauri::command]
pub async fn import_cancel(source: String) -> Result<bool, String> {
    Ok(ImportRunner::global().cancel(&source))
}

#[tauri::command]
pub async fn import_forget(
    state: State<'_, Arc<AppState>>,
    source: String,
) -> Result<usize, String> {
    ImportRunner::global().forget(&state, &source).await
}

#[tauri::command]
pub async fn import_record_first_answer(
    state: State<'_, Arc<AppState>>,
    elapsed_ms: u64,
) -> Result<(), String> {
    if state
        .state_store
        .load_json::<u64>("import_first_answer_ms")?
        .is_none()
    {
        let elapsed_ms = elapsed_ms.min(86_400_000);
        state
            .state_store
            .save_json("import_first_answer_ms", &elapsed_ms)?;
        let metrics = crate::telemetry::runtime_metrics::global();
        metrics.record_ms("import_first_answer", elapsed_ms);
        metrics.bump("import_first_answer_count");
    }
    Ok(())
}

#[tauri::command]
pub async fn import_first_answer_ms(
    state: State<'_, Arc<AppState>>,
) -> Result<Option<u64>, String> {
    state.state_store.load_json("import_first_answer_ms")
}
