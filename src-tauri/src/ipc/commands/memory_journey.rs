//! Debug-only Memory Journey IPC. The module and handler entries are absent
//! from release builds so raw-artifact controls cannot be invoked there.

use crate::memory_journey::{
    MemoryJourneyExportReceipt, MemoryJourneyManifestV1, MemoryJourneyQueryKind,
    MemoryJourneyQueryPath, MemoryJourneyQueryRun, MemoryJourneyStatus,
};
use crate::AppState;
use std::sync::Arc;
use tauri::State;

#[derive(Debug, Clone, serde::Serialize)]
pub struct MemoryJourneyQueryReceipt {
    pub manifest: MemoryJourneyManifestV1,
    pub cards: Vec<crate::search::MemoryCard>,
    pub answer: Option<String>,
}

#[tauri::command]
pub async fn arm_memory_journey(
    label: String,
    state: State<'_, Arc<AppState>>,
) -> Result<MemoryJourneyStatus, String> {
    state.memory_journey.arm(label)?;
    state.emit_memory_journey_status();
    state.memory_journey.status()
}

#[tauri::command]
pub async fn get_memory_journey_status(
    state: State<'_, Arc<AppState>>,
) -> Result<MemoryJourneyStatus, String> {
    state.memory_journey.status()
}

#[tauri::command]
pub async fn create_reconstructed_memory_journey(
    memory_id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<MemoryJourneyManifestV1, String> {
    let memory_id = memory_id.trim();
    if memory_id.is_empty() {
        return Err("Choose a memory identifier to inspect".to_string());
    }
    let record = state
        .store
        .get_memory_by_id(memory_id)
        .await
        .map_err(|error| format!("Could not load memory: {error}"))?
        .ok_or_else(|| "Memory was not found".to_string())?;
    let manifest = state.memory_journey.create_reconstructed(&record)?;
    state.emit_memory_journey_status();
    Ok(manifest)
}

#[tauri::command]
pub async fn run_memory_journey_query(
    journey_id: String,
    query: String,
    path: MemoryJourneyQueryPath,
    query_kind: Option<MemoryJourneyQueryKind>,
    state: State<'_, Arc<AppState>>,
) -> Result<MemoryJourneyQueryReceipt, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("Enter an exact query, paraphrase, or grounded question".to_string());
    }
    // Validate the selected journey before running model/search work.
    state.memory_journey.load_manifest(&journey_id)?;
    let started_at_ms = chrono::Utc::now().timestamp_millis();
    let started = std::time::Instant::now();

    let (cards, answer, result_ids, scores, citation_ids, refusal, explanation) = match path {
        MemoryJourneyQueryPath::Search => {
            let limit = 10usize;
            let (ranked, retrieval) = super::search::search_ranked_results_explained(
                state.inner(),
                query,
                None,
                None,
                limit.max(18),
            )
            .await?;
            let result_ids = ranked
                .iter()
                .map(|result| result.id.clone())
                .collect::<Vec<_>>();
            let scores = ranked.iter().map(|result| result.score).collect::<Vec<_>>();
            let cards = super::search::synthesize_memory_cards_from_ranked(
                state.inner(),
                query,
                ranked,
                limit,
            )
            .await;
            let citation_ids = cards
                .iter()
                .flat_map(|card| card.evidence_ids.iter().cloned())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            let presentation = serde_json::json!({
                "card_count": cards.len(),
                "cards": cards.iter().map(|card| serde_json::json!({
                    "id": card.id,
                    "score": card.score,
                    "evidence_ids": card.evidence_ids,
                    "source_count": card.source_count,
                    "surfaceable": true,
                    "title_chars": card.title.chars().count(),
                    "summary_chars": card.summary.chars().count(),
                    "summary_source": card.synthesis_branch,
                    "storage_outcome": card.storage_outcome,
                    "surfacing_reason": card.surfacing_reason,
                })).collect::<Vec<_>>(),
                "top_1_stable_with_ranked": cards.first().and_then(|card| result_ids.first().map(|id| &card.id == id)),
            });
            (
                cards,
                None,
                result_ids,
                scores,
                citation_ids,
                None,
                serde_json::json!({
                    "retrieval": retrieval,
                    "presentation": presentation,
                }),
            )
        }
        MemoryJourneyQueryPath::Ask => {
            let composed = crate::context_runtime::run_query(
                state.inner(),
                query,
                10,
                crate::context_runtime::ComposeMode::Answer,
            )
            .await?;
            let result_ids = composed
                .cards
                .iter()
                .map(|card| card.id.clone())
                .collect::<Vec<_>>();
            let scores = composed
                .cards
                .iter()
                .map(|card| card.score)
                .collect::<Vec<_>>();
            let mut citation_ids = std::collections::HashSet::new();
            for ids in composed
                .evidence
                .files
                .iter()
                .map(|item| &item.memory_ids)
                .chain(
                    composed
                        .evidence
                        .commands
                        .iter()
                        .map(|item| &item.memory_ids),
                )
                .chain(
                    composed
                        .evidence
                        .decisions
                        .iter()
                        .map(|item| &item.memory_ids),
                )
                .chain(composed.evidence.errors.iter().map(|item| &item.memory_ids))
                .chain(composed.evidence.todos.iter().map(|item| &item.memory_ids))
                .chain(composed.evidence.urls.iter().map(|item| &item.memory_ids))
            {
                citation_ids.extend(ids.iter().cloned());
            }
            let refusal = matches!(
                &composed.verify_outcome,
                crate::context_runtime::context_pack::VerifyOutcome::NotEnoughEvidence { .. }
            );
            let presentation = serde_json::json!({
                "card_count": composed.cards.len(),
                "cards": composed.cards.iter().map(|card| serde_json::json!({
                    "id": card.id,
                    "evidence_ids": card.evidence_ids,
                    "surfaceable": true,
                    "title_chars": card.title.chars().count(),
                    "summary_chars": card.summary.chars().count(),
                    "summary_source": card.synthesis_branch,
                    "surfacing_reason": card.surfacing_reason,
                })).collect::<Vec<_>>(),
                "evidence_id_count": citation_ids.len(),
                "verify_outcome": composed.verify_outcome,
                "refusal": refusal,
                "answer_chars": composed.answer.chars().count(),
            });
            (
                composed.cards,
                Some(composed.answer),
                result_ids,
                scores,
                citation_ids.into_iter().collect::<Vec<_>>(),
                Some(refusal),
                serde_json::json!({
                    "retrieval": composed.debug_trace,
                    "presentation": presentation,
                }),
            )
        }
    };

    let run = MemoryJourneyQueryRun {
        id: uuid::Uuid::new_v4().to_string(),
        path,
        kind: query_kind.unwrap_or_default(),
        query: query.to_string(),
        started_at_ms,
        duration_ms: started.elapsed().as_millis() as u64,
        result_ids,
        scores,
        citation_ids,
        refusal,
        explanation,
    };
    let manifest = state.memory_journey.append_query_run(&journey_id, run)?;
    state.emit_memory_journey_status();
    Ok(MemoryJourneyQueryReceipt {
        manifest,
        cards,
        answer,
    })
}

#[tauri::command]
pub async fn export_memory_journey(
    journey_id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<Option<MemoryJourneyExportReceipt>, String> {
    let default_name = format!("memory-journey-{journey_id}.fndrjourney.zip");
    let destination = rfd::FileDialog::new()
        .set_title("Export private Memory Journey evidence")
        .set_file_name(&default_name)
        .add_filter("FNDR Memory Journey", &["zip"])
        .save_file();
    let Some(destination) = destination else {
        return Ok(None);
    };
    state
        .memory_journey
        .export_to(&journey_id, &destination)
        .map(Some)
}

#[tauri::command]
pub async fn delete_memory_journey(
    journey_id: String,
    state: State<'_, Arc<AppState>>,
) -> Result<MemoryJourneyStatus, String> {
    state.memory_journey.delete(&journey_id)?;
    state.emit_memory_journey_status();
    state.memory_journey.status()
}

#[tauri::command]
pub async fn delete_all_memory_journeys(
    state: State<'_, Arc<AppState>>,
) -> Result<MemoryJourneyStatus, String> {
    state.memory_journey.delete_all()?;
    state.emit_memory_journey_status();
    state.memory_journey.status()
}
