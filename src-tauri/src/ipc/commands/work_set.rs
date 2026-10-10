//! Work sets over IPC (ADR 027): which thread of work a request means,
//! opening the places of the one the person picked, sets saved under a name
//! and the routines mined from when sets are opened.

use std::sync::Arc;

use tauri::State;

use crate::operator::layout::Layout;
use crate::workset::named::{self, NamedWorkSet};
use crate::workset::routines::{self, RoutineOffer, Source};
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
/// again for privacy and judged from its typed reopen outcome. With a
/// `layout`, their windows are then arranged; how that went rides on each
/// opened item and never turns the opens into an error.
#[tauri::command]
pub async fn open_work_set(
    state: State<'_, Arc<AppState>>,
    memory_ids: Vec<String>,
    layout: Option<String>,
) -> Result<Vec<ItemOutcome>, String> {
    if memory_ids.is_empty() {
        return Err("Nothing to open.".to_string());
    }
    let layout = match layout
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        None => None,
        Some(name) => match Layout::parse(name) {
            Some(Layout::RestorePrevious) | None => {
                return Err(format!(
                    "FNDR does not know the layout \u{201c}{name}\u{201d}."
                ))
            }
            layout => layout,
        },
    };
    Ok(workset::open_and_arrange(state.inner(), &memory_ids, layout, Source::Home).await)
}

/// The person's saved sets, newest first, each with only the items FNDR may
/// still reopen.
#[tauri::command]
pub async fn list_named_sets(state: State<'_, Arc<AppState>>) -> Result<Vec<NamedWorkSet>, String> {
    named::list(state.inner()).await
}

/// Saves memories as a set under a name, unique ignoring case.
#[tauri::command]
pub async fn save_named_set(
    state: State<'_, Arc<AppState>>,
    name: String,
    memory_ids: Vec<String>,
) -> Result<NamedWorkSet, String> {
    let saved = named::save(&state.state_store, &name, &memory_ids)?;
    named::list(state.inner())
        .await?
        .into_iter()
        .find(|set| set.id == saved.id)
        .ok_or_else(|| "The set was saved but could not be read back.".to_string())
}

#[tauri::command]
pub async fn delete_named_set(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    named::delete(&state.state_store, &id)
}

/// Sets the person usually opens around now on days like today. Only data
/// for a card; FNDR opens nothing because of one. Empty while FNDR is private.
#[tauri::command]
pub async fn routine_offers(state: State<'_, Arc<AppState>>) -> Result<Vec<RoutineOffer>, String> {
    routines::current(state.inner()).await
}

/// Hides an offer for the rest of today.
#[tauri::command]
pub async fn dismiss_routine_offer(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    routines::dismiss(&state.state_store, &id, chrono::Local::now().date_naive())
}
