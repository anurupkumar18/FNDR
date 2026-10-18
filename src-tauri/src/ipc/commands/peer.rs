//! Tauri entry points for the local directory of person-configured A2A peers.

use crate::agent::{peer, peer_store};
use crate::AppState;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn list_configured_peers(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<peer_store::ConfiguredPeer>, String> {
    peer_store::list_peers(&state.state_store)
}

#[tauri::command]
pub async fn add_configured_peer(
    state: State<'_, Arc<AppState>>,
    card_url: String,
) -> Result<peer_store::ConfiguredPeer, String> {
    let card = peer::inspect_configured_peer(&card_url).await?;
    peer_store::save_checked_peer(
        &state.state_store,
        &card,
        chrono::Utc::now().timestamp_millis(),
    )
}

#[tauri::command]
pub async fn remove_configured_peer(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<bool, String> {
    peer_store::remove_peer(&state.state_store, &id)
}

#[tauri::command]
pub async fn preview_peer_delegation(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    task: String,
    output_goal: String,
    memory_ids: Vec<String>,
) -> Result<crate::agent::delegation::DelegationPreview, String> {
    crate::agent::delegation::preview_delegation(&state, &peer_id, &task, &output_goal, &memory_ids)
        .await
}

#[tauri::command]
pub async fn send_peer_delegation(
    state: State<'_, Arc<AppState>>,
    peer_id: String,
    task: String,
    output_goal: String,
    memory_ids: Vec<String>,
    reviewed_destination: String,
    reviewed_text: String,
) -> Result<crate::agent::delegation::DelegationSendResult, String> {
    crate::agent::delegation::send_delegation(
        &state,
        &peer_id,
        &task,
        &output_goal,
        &memory_ids,
        &reviewed_destination,
        &reviewed_text,
    )
    .await
}

#[tauri::command]
pub async fn list_peer_runs(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<crate::agent::peer_runs::PeerRunView>, String> {
    if state.is_incognito.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(Vec::new());
    }
    crate::agent::peer_runs::list_runs(&state.state_store)
        .map(|runs| runs.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub async fn refresh_peer_task(
    state: State<'_, Arc<AppState>>,
    local_id: String,
) -> Result<crate::agent::delegation::DelegationTaskResult, String> {
    crate::agent::delegation::refresh_delegation(&state, &local_id).await
}

#[tauri::command]
pub async fn cancel_peer_task(
    state: State<'_, Arc<AppState>>,
    local_id: String,
) -> Result<crate::agent::delegation::DelegationTaskResult, String> {
    crate::agent::delegation::cancel_delegation(&state, &local_id).await
}
