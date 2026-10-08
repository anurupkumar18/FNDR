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
