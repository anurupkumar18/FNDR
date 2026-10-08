//! POST /v1/memories/search — mobile memory search endpoint.
//!
//! Calls the same retrieval function as desktop Search, so ranking and
//! filtering are identical on the phone and the Mac.

use crate::companion::dto::{MemorySearchRequest, MemorySearchResponse};
use crate::companion::errors::{CompanionError, CompanionResult};
use crate::ipc::commands::search::search_ranked_results;
use crate::search::MemoryCardSynthesizer;
use crate::AppState;
use axum::extract::State;
use axum::Json;
use std::sync::Arc;
use std::time::Instant;

const DEFAULT_LIMIT: usize = 12;
const MAX_LIMIT: usize = 40;

pub async fn search_memories(
    State(app_state): State<Arc<AppState>>,
    body: Result<Json<MemorySearchRequest>, axum::extract::rejection::JsonRejection>,
) -> CompanionResult<Json<MemorySearchResponse>> {
    let Json(payload) = body.map_err(|err| CompanionError::BadRequest(err.to_string()))?;

    let query = payload.query.trim().to_string();
    if query.is_empty() {
        return Err(CompanionError::BadRequest("query is empty".to_string()));
    }

    let limit = payload.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let app_filter = normalized_filter(payload.app_filter.as_deref());
    let project_filter = normalized_filter(payload.project_filter.as_deref());
    let time_filter = normalized_filter(payload.time_filter.as_deref());

    let started = Instant::now();

    // The one retrieval function Search, Ask and agents use, so the phone
    // ranks exactly what the Mac ranks.
    let mut results = search_ranked_results(
        &app_state,
        &query,
        time_filter.as_deref(),
        app_filter.as_deref(),
        limit,
    )
    .await
    .map_err(|e| CompanionError::Internal(format!("search failed: {e}")))?;

    if let Some(project) = project_filter.as_deref() {
        results.retain(|row| row.project.trim().eq_ignore_ascii_case(project));
    }

    let cards = MemoryCardSynthesizer::deterministic_from_results(&query, &results, limit)
        .into_iter()
        .map(crate::companion::handlers::companion_card_from_memory_card)
        .collect::<Vec<_>>();

    let latency_ms = started.elapsed().as_millis() as u64;

    tracing::info!(
        query = %query,
        limit,
        cards = cards.len(),
        latency_ms,
        "companion.search.completed"
    );

    Ok(Json(MemorySearchResponse {
        query,
        total: cards.len(),
        cards,
        latency_ms,
    }))
}

fn normalized_filter(raw: Option<&str>) -> Option<String> {
    raw.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_filter_strips_whitespace_and_empty_values() {
        assert_eq!(normalized_filter(None), None);
        assert_eq!(normalized_filter(Some("   ")), None);
        assert_eq!(
            normalized_filter(Some("  project-fndr  ")),
            Some("project-fndr".to_string())
        );
    }
}
