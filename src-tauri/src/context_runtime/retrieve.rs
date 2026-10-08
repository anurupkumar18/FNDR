//! VS-09: the one retrieval function every surface calls. It runs the same
//! plan, routes, fusion, and low-signal filter as Ask (`run_query`) and
//! returns ranked memory ids with why each one matched, before any card
//! synthesis or answer composition.

use crate::context_runtime::query_filters::parse_query_filters;
use crate::context_runtime::query_plan::Route;
use crate::context_runtime::retrieval_routes::memory_record_to_search_result;
use crate::context_runtime::{retrieve_fused, FusedRetrieval};
use crate::search::memory_cards::{build_fallback_card, MemoryCard};
use crate::search::{normalize_text, QueryContext};
use crate::storage::{MemoryRecord, SearchResult, Store};
use crate::AppState;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};

/// One request to the shared retrieval path.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type, PartialEq)]
pub struct RetrieveRequest {
    pub query: String,
    /// A store time filter: "1h", "24h", "7d", "today", or "yesterday".
    pub time: Option<String>,
    /// Exact app name, as stored on the memory.
    pub app: Option<String>,
    pub limit: usize,
}

/// The best hit's score must reach this for a query to count as matched;
/// under it a surface says "No strong matches" (VS-12). Set from the labeled
/// sets: every positive query's best hit scores 0.29 or more, and three of
/// eight no-match queries score under 0.20. No single score separates all
/// eight (they reach 0.35), so the bar sits where it hides no real match.
pub const STRONG_MATCH_SCORE: f32 = 0.25;

/// The bar when the chunk route found the best hit (VS-18): it adds its
/// weight to every score, so the same evidence scores about 0.2 higher. With
/// chunks on, the labeled sets put the same three no-match queries at 0.401
/// to 0.407 and every real query at 0.532 or more. Provisional until chunks
/// exist on a real vault (EM-03).
pub const STRONG_MATCH_SCORE_WITH_CHUNKS: f32 = 0.45;

/// How close in meaning the best hit must be when the keyword route scored
/// it without finding the query's words. Measured 2026-10-07
/// (`docs/evidence/W04/strong-match.md`): no-match queries topped out at
/// 0.31 on the owner vault and 0.28 on the labeled sets, and 95 percent of
/// known-item queries on the vault were at 0.39 or more.
pub const STRONG_MATCH_MIN_VECTOR: f32 = 0.33;

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type, PartialEq)]
pub struct RetrieveResult {
    pub hits: Vec<RetrieveHit>,
    /// What was actually searched, so a surface can show "yesterday, Slack".
    pub filters: RetrieveFilters,
    /// Whether the best hit reaches `STRONG_MATCH_SCORE` or contains every
    /// word of the query; when false, surfaces say "No strong matches".
    pub strong_match: bool,
}

/// The text and filters a request was searched with after time and app
/// phrases were read out of the query (VS-13). The text keeps the phrases:
/// removing them cost the planner its time intent ("last week") on the
/// labeled sets. Explicit request filters win over phrases.
#[derive(Debug, Clone, Default, Serialize, Deserialize, Type, PartialEq)]
pub struct RetrieveFilters {
    pub query: String,
    pub time: Option<String>,
    pub app: Option<String>,
}

/// One ranked memory. `score` is its evidence strength (the weighted sum of
/// its route scores), on the same scale Ask's verifier and cards read.
#[derive(Debug, Clone, Serialize, Deserialize, Type, PartialEq)]
pub struct RetrieveHit {
    pub memory_id: String,
    /// The chunk the chunk route matched (VS-18); empty when that route did
    /// not find this memory or is off.
    pub chunk_id: Option<String>,
    /// That chunk's text, so a surface can show the sentence that matched.
    pub matched_text: Option<String>,
    pub score: f32,
    pub why: RetrieveWhy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type, PartialEq)]
pub struct RetrieveWhy {
    /// Routes that found the memory: "vector", "keyword", "chunk",
    /// "temporal", "entity", "graph".
    pub routes: Vec<String>,
    /// Query words present in the memory's text, for hits the keyword
    /// route found; empty for meaning-only matches.
    pub matched_terms: Vec<String>,
}

pub async fn retrieve(
    state: &AppState,
    request: &RetrieveRequest,
) -> Result<RetrieveResult, String> {
    Ok(retrieve_with_fused(state, request).await.0)
}

pub(crate) fn memory_is_visible(record: &MemoryRecord, blocklist: &[String]) -> bool {
    memory_is_permitted(record, blocklist)
        && crate::memory_quality::record_low_signal_reason(record).is_none()
}

/// Current access policy, without quality gating, for explicit diagnostics.
pub(crate) fn memory_is_permitted(record: &MemoryRecord, blocklist: &[String]) -> bool {
    // Notes are admitted against title, body and project; later rules must
    // apply to those same retained fields when following their links.
    let context = if record.is_agent_note() {
        format!(
            "{}\n{}\n{}",
            record.window_title, record.clean_text, record.project
        )
    } else {
        record.window_title.clone()
    };
    !record.is_soft_deleted
        && !crate::privacy::Blocklist::is_internal_app(
            &record.app_name,
            record.bundle_id.as_deref(),
        )
        && !crate::privacy::Blocklist::is_blocked(&record.app_name, blocklist)
        && !crate::privacy::Blocklist::is_context_blocked(
            record.url.as_deref(),
            Some(&context),
            blocklist,
        )
}

pub async fn memory_source_statements(
    state: &AppState,
    memory_id: &str,
) -> Result<Vec<crate::context_runtime::context_pack::SourceStatementRef>, String> {
    let Some(record) = state
        .store
        .get_memory_by_id(memory_id)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(Vec::new());
    };
    let blocklist = state.config.read().blocklist.clone();
    if !memory_is_visible(&record, &blocklist) {
        return Ok(Vec::new());
    }
    Ok(crate::context_runtime::evidence_pack::source_statements_for_record(&record))
}

/// Resolve persisted peer links before considering similar context. Links are
/// directed evidence from the source row, not inferred graph edges. A missing
/// or excluded linked target is never replaced with a similarity suggestion.
pub async fn related_memories(
    state: &AppState,
    memory_id: &str,
    limit: usize,
) -> Result<Vec<MemoryCard>, String> {
    const MAX_RELATED_CARDS: usize = 12;
    let limit = limit.min(MAX_RELATED_CARDS);
    if limit == 0 {
        return Ok(Vec::new());
    }
    let Some(seed) = state
        .store
        .get_memory_by_id(memory_id)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(Vec::new());
    };
    let blocklist = state.config.read().blocklist.clone();
    if !memory_is_visible(&seed, &blocklist) {
        return Ok(Vec::new());
    }

    if !seed.related_memory_ids.is_empty() {
        let ids = related_lookup_ids(&seed.related_memory_ids);
        let mut found = state
            .store
            .get_memories_by_ids(&ids)
            .await
            .map_err(|e| e.to_string())?;
        let mut canonical_ids = HashSet::from([seed.id.clone()]);
        let mut records = Vec::new();
        for id in ids {
            let record = match found.remove(&id) {
                Some(record) => Some(record),
                // Old citations may name a frame consolidated into a survivor.
                None => state
                    .store
                    .get_memory_by_id(&id)
                    .await
                    .map_err(|e| e.to_string())?,
            };
            if let Some(record) = record.filter(|record| {
                memory_is_visible(record, &blocklist) && canonical_ids.insert(record.id.clone())
            }) {
                records.push(record);
            }
        }
        records.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| a.id.cmp(&b.id)));
        return Ok(records
            .iter()
            .take(limit)
            .map(|record| {
                // A stored link has no numerical semantic similarity score.
                let mut result = memory_record_to_search_result(record, 0.0);
                result.matched_routes = vec!["stored_link".into()];
                let mut card = build_fallback_card("", &result);
                card.surfacing_reason =
                    Some(crate::context_runtime::context_pack::SurfacingReason {
                        headline: format!("Linked from ‘{}’", seed.window_title),
                        routes: vec!["stored_link".into()],
                        graph_path: None,
                        anchor_terms_hit: Vec::new(),
                        recency_boost: 0.0,
                    });
                card
            })
            .collect());
    }

    // Assistant notes expose only their explicit references. Ordinary memories
    // retain the existing hybrid-similarity fallback using text that survives
    // compaction, with retrieval scores/routes distinguished from stored links.
    if seed.is_agent_note() {
        return Ok(Vec::new());
    }
    let query = crate::memory_compaction::best_embedding_text(&seed);
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let (_, retrieval) = retrieve_with_fused(
        state,
        &RetrieveRequest {
            query: query.clone(),
            limit: limit + 1,
            ..Default::default()
        },
    )
    .await;
    let mut seen = HashSet::from([seed.id]);
    Ok(retrieval
        .fused
        .iter()
        .filter_map(|hit| {
            let record = retrieval.records.get(&hit.memory_id)?;
            if !memory_is_visible(record, &blocklist) || !seen.insert(record.id.clone()) {
                return None;
            }
            let mut card =
                build_fallback_card(&query, &memory_record_to_search_result(record, hit.score));
            let mut reason = hit.surfacing_reason.clone();
            reason.headline = "Similar context".into();
            card.surfacing_reason = Some(reason);
            Some(card)
        })
        .take(limit)
        .collect())
}

/// `retrieve` plus each hit's stored row as a `SearchResult`, in hit order,
/// for surfaces that render rows (Search's cards, VS-10). The rows come from
/// the lookup the hidden-memory drop already made; a hit whose row is gone is
/// left out.
pub async fn retrieve_search_results(
    state: &AppState,
    request: &RetrieveRequest,
) -> Result<(RetrieveResult, Vec<SearchResult>), String> {
    let (result, retrieval) = retrieve_with_fused(state, request).await;
    let embedding_labels = retrieval
        .fused
        .iter()
        .map(|hit| {
            let labels = hit
                .surfacing_reason
                .routes
                .iter()
                .filter(|label| label.starts_with("embedding:"))
                .cloned()
                .collect::<Vec<_>>();
            (hit.memory_id.as_str(), labels)
        })
        .collect::<HashMap<_, _>>();
    // The chunk route's evidence, so cards can show the sentence (VS-18).
    let chunk_rows = retrieval
        .route_hits
        .iter()
        .filter(|group| group.route == Route::Chunk)
        .flat_map(|group| group.hits.iter())
        .filter_map(|hit| {
            hit.signals
                .search_result
                .as_ref()
                .map(|result| (hit.memory_id.as_str(), result))
        })
        .collect::<HashMap<_, _>>();
    let mut rows: Vec<SearchResult> = result
        .hits
        .iter()
        .filter_map(|hit| {
            let record = retrieval.records.get(&hit.memory_id)?;
            let mut row = memory_record_to_search_result(record, hit.score);
            if let Some(chunk_row) = chunk_rows.get(hit.memory_id.as_str()) {
                row.matched_chunk_ids = chunk_row.matched_chunk_ids.clone();
                row.chunk_evidence = chunk_row.chunk_evidence.clone();
            }
            row.matched_routes = hit.why.routes.clone();
            row.embedding_reason_labels = embedding_labels
                .get(hit.memory_id.as_str())
                .cloned()
                .unwrap_or_default();
            Some(row)
        })
        .collect();
    let blocklist = state.config.read().blocklist.clone();
    authorize_related_memory_ids(&mut rows, &state.store, &blocklist).await;
    Ok((result, rows))
}

const MAX_LINK_LOOKUPS: usize = 64;

fn related_lookup_ids(ids: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    ids.iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty() && seen.insert((*id).to_string()))
        .take(MAX_LINK_LOOKUPS)
        .map(str::to_string)
        .collect()
}

/// Linked targets need their own authorization: they need not be ranked hits.
/// Batch current IDs and bound merged-ID fallback work across the whole page.
pub(crate) async fn authorize_related_memory_ids(
    rows: &mut [SearchResult],
    store: &Store,
    blocklist: &[String],
) {
    let row_links = rows
        .iter()
        .map(|row| related_lookup_ids(&row.related_memory_ids))
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    let ids = row_links
        .iter()
        .flatten()
        .filter(|id| seen.insert((*id).clone()))
        .cloned()
        .collect::<Vec<_>>();
    if ids.is_empty() {
        for row in rows {
            row.related_memory_ids.clear();
        }
        return;
    }
    let mut found = match store.get_memories_by_ids(&ids).await {
        Ok(records) => records,
        Err(_) => {
            tracing::warn!("retrieval:related_link_visibility_lookup_failed");
            for row in rows {
                row.related_memory_ids.clear();
            }
            return;
        }
    };
    let mut canonical_by_id = HashMap::new();
    let mut alias_lookups = 0;
    for id in ids {
        let record = match found.remove(&id) {
            Some(record) => Some(record),
            None if alias_lookups < MAX_LINK_LOOKUPS => {
                alias_lookups += 1;
                store.get_memory_by_id(&id).await.ok().flatten()
            }
            None => None,
        };
        if let Some(record) = record.filter(|record| memory_is_visible(record, blocklist)) {
            canonical_by_id.insert(id, record.id);
        }
    }
    for (row, links) in rows.iter_mut().zip(row_links) {
        let mut seen = HashSet::from([row.id.clone()]);
        row.related_memory_ids = links
            .iter()
            .filter_map(|id| canonical_by_id.get(id))
            .filter(|canonical| seen.insert((*canonical).clone()))
            .cloned()
            .collect();
    }
}

/// `retrieve` plus the fused retrieval behind it, for Ask (`run_query`),
/// which adds evidence, verification, and cards on top (VS-11).
pub(crate) async fn retrieve_with_fused(
    state: &AppState,
    request: &RetrieveRequest,
) -> (RetrieveResult, FusedRetrieval) {
    let limit = request.limit.max(1);
    let (filters, words_without_phrases) = read_filters(state, request).await;
    let mut retrieval = retrieve_fused(
        state,
        &filters.query,
        limit,
        filters.time.as_deref(),
        filters.app.as_deref(),
    )
    .await;
    let parsed_filter = (request.time.is_none() && filters.time.is_some())
        || (request.app.is_none() && filters.app.is_some());
    let filters = if retrieval.fused.is_empty() && parsed_filter {
        // A phrase we read as a filter matched nothing: search everything
        // with the words as typed rather than show an empty page.
        retrieval = retrieve_fused(
            state,
            &request.query,
            limit,
            request.time.as_deref(),
            request.app.as_deref(),
        )
        .await;
        RetrieveFilters {
            query: request.query.clone(),
            time: request.time.clone(),
            app: request.app.clone(),
        }
    } else {
        filters
    };
    let keyword_texts = retrieval
        .route_hits
        .iter()
        .filter(|group| group.route == Route::Keyword)
        .flat_map(|group| group.hits.iter())
        .filter_map(|hit| {
            hit.signals
                .search_result
                .as_ref()
                .map(|result| (hit.memory_id.as_str(), searchable_text(result)))
        })
        .collect::<HashMap<_, _>>();
    let matched_chunks = retrieval
        .route_hits
        .iter()
        .filter(|group| group.route == Route::Chunk)
        .flat_map(|group| group.hits.iter())
        .filter_map(|hit| {
            let evidence = hit.signals.search_result.as_ref()?.chunk_evidence.first()?;
            Some((
                hit.memory_id.as_str(),
                (evidence.chunk_id.clone(), evidence.text.clone()),
            ))
        })
        .collect::<HashMap<_, _>>();
    // Report matched words without the filter phrases ("yesterday", "Slack").
    let terms = QueryContext::from_query(&words_without_phrases).anchor_terms;

    let hits: Vec<RetrieveHit> = retrieval
        .fused
        .iter()
        .take(limit)
        .map(|hit| RetrieveHit {
            memory_id: hit.memory_id.clone(),
            chunk_id: matched_chunks
                .get(hit.memory_id.as_str())
                .map(|(id, _)| id.clone()),
            matched_text: matched_chunks
                .get(hit.memory_id.as_str())
                .map(|(_, text)| text.clone()),
            score: hit.score,
            why: RetrieveWhy {
                routes: hit
                    .contributing_routes
                    .iter()
                    .map(|route| route_name(*route).to_string())
                    .collect(),
                matched_terms: keyword_texts
                    .get(hit.memory_id.as_str())
                    .map(|text| matched_terms(&terms, text))
                    .unwrap_or_default(),
            },
        })
        .collect();
    let top_vector = retrieval
        .fused
        .first()
        .map_or(0.0, |hit| hit.signals.vector);
    let strong_match = hits
        .first()
        .is_some_and(|hit| is_strong_match(hit, &terms, top_vector));
    (
        RetrieveResult {
            hits,
            filters,
            strong_match,
        },
        retrieval,
    )
}

/// A hit is a strong match when its score reaches `STRONG_MATCH_SCORE` (or
/// `STRONG_MATCH_SCORE_WITH_CHUNKS` when the chunk route found it), or
/// when the keyword route found every word of the query in it. The second
/// rule keeps exact matches strong when no embedding model is loaded: then
/// only the keyword route scores, and a perfect match fuses to about 0.20.
///
/// One case is taken back out. The keyword route also scores rows that hold
/// few or none of the query's whole words, which lifted unrelated queries
/// over the bar. When under a third of the words are there, the entity
/// route did not find the hit, and it is not close in meaning either
/// (`vector` under `STRONG_MATCH_MIN_VECTOR`), the hit is weak.
///
/// The time route is not evidence here. It finds every memory in the time
/// the query names, so "probate documents yesterday" would otherwise be a
/// strong match for whatever was on screen yesterday.
fn is_strong_match(hit: &RetrieveHit, terms: &[String], vector: f32) -> bool {
    let words = terms
        .iter()
        .filter(|term| !term.contains(' '))
        .collect::<Vec<_>>();
    let matched = words
        .iter()
        .filter(|word| hit.why.matched_terms.contains(word))
        .count();
    if !words.is_empty() && matched == words.len() {
        return true;
    }
    let found_by = |name: &str| hit.why.routes.iter().any(|route| route == name);
    let bar = if found_by("chunk") {
        STRONG_MATCH_SCORE_WITH_CHUNKS
    } else {
        STRONG_MATCH_SCORE
    };
    let only_loose_keyword_support = found_by("keyword")
        && !found_by("chunk")
        && !found_by("entity")
        && matched * 3 < words.len()
        && vector < STRONG_MATCH_MIN_VECTOR;
    hit.score >= bar && !only_loose_keyword_support
}

/// Read time and app phrases out of the query (VS-13). Returns the filters
/// to search with and the query's words without those phrases.
async fn read_filters(state: &AppState, request: &RetrieveRequest) -> (RetrieveFilters, String) {
    let apps = if request.app.is_none() {
        crate::ipc::commands::stats::cached_app_names(state)
            .await
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let parsed = parse_query_filters(&request.query, chrono::Local::now(), &apps);
    let filters = RetrieveFilters {
        query: request.query.clone(),
        time: request.time.clone().or_else(|| {
            parsed
                .time
                .map(|range| format!("range:{}:{}", range.start_ms, range.end_ms))
        }),
        app: request.app.clone().or(parsed.app),
    };
    (filters, parsed.text)
}

fn route_name(route: Route) -> &'static str {
    match route {
        Route::Chunk => "chunk",
        Route::Vector => "vector",
        Route::Keyword => "keyword",
        Route::Temporal => "temporal",
        Route::Entity => "entity",
        Route::Graph => "graph",
    }
}

fn searchable_text(result: &SearchResult) -> String {
    format!(
        "{} {} {} {}",
        result.window_title,
        result.snippet,
        result.clean_text,
        result.url.as_deref().unwrap_or_default()
    )
}

/// Query terms that appear in `text` as whole words (a multi-word term as a
/// contiguous phrase), in query order.
fn matched_terms(terms: &[String], text: &str) -> Vec<String> {
    let padded = format!(" {} ", normalize_text(text));
    terms
        .iter()
        .filter(|term| {
            let normalized = normalize_text(term);
            !normalized.is_empty() && padded.contains(&format!(" {normalized} "))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_statement_test_state(path: &std::path::Path) -> AppState {
        let store = std::sync::Arc::new(crate::storage::Store::new(path).unwrap());
        let state_store = std::sync::Arc::new(crate::storage::StateStore::new(path).unwrap());
        let graph = crate::graph::GraphStore::new(store.clone());
        AppState::new(
            path.to_path_buf(),
            crate::config::Config::default(),
            store,
            state_store,
            graph,
            None,
        )
    }

    fn source_statement_record(id: &str) -> MemoryRecord {
        let text = "Reviewed the deployment checklist and recorded release verification evidence.";
        MemoryRecord {
            id: id.into(), app_name: "Editor".into(), window_title: "Release checklist".into(),
            text: text.into(), clean_text: text.into(), snippet: text.into(), memory_context: text.into(),
            raw_evidence: serde_json::json!({
                "source_evidence": {"version":1,"source_sha256":"a".repeat(64),"statements":[{"kind":"action","line":3,"quote":"Check the release evidence."}],"issues":[]},
                "source_evidence_history":[{"version":1,"source_sha256":"b".repeat(64),"statements":[{"kind":"intent","line":1,"quote":"We discussed a possible rollback."}],"issues":[]}]
            }).to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn search_rows_authorize_nested_links_without_changing_rank_or_storage() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let state = source_statement_test_state(dir.path());
        let visible_a = source_statement_record("visible-a");
        let mut visible_b = source_statement_record("visible-b");
        visible_b.consolidated_from = vec!["earlier-b".into()];
        let mut deleted = source_statement_record("deleted-target");
        deleted.is_soft_deleted = true;
        let mut blocked = source_statement_record("blocked-target");
        blocked.app_name = "PrivateWorkspace".into();
        blocked.consolidated_from = vec!["earlier-blocked".into()];
        let mut seed = source_statement_record("seed");
        seed.consolidated_from = vec!["earlier-seed".into()];
        seed.related_memory_ids = [
            " visible-b ",
            "deleted-target",
            "missing-target",
            "earlier-b",
            "visible-a",
            "earlier-seed",
            "visible-a",
            "blocked-target",
            "earlier-blocked",
            "",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        runtime
            .block_on(state.store.add_batch_preserving_ids(&[
                seed.clone(),
                visible_a.clone(),
                visible_b,
                deleted,
                blocked,
            ]))
            .unwrap();
        let before = runtime
            .block_on(state.store.get_memory_by_id("seed"))
            .unwrap()
            .unwrap();
        let mut second = visible_a;
        second.related_memory_ids = vec!["earlier-b".into(), "seed".into()];
        let mut rows = vec![
            memory_record_to_search_result(&seed, 0.73),
            memory_record_to_search_result(&second, 0.42),
        ];
        runtime.block_on(authorize_related_memory_ids(
            &mut rows,
            &state.store,
            &["privateworkspace".into()],
        ));
        assert_eq!(rows[0].related_memory_ids, ["visible-b", "visible-a"]);
        assert_eq!(rows[1].related_memory_ids, ["visible-b", "seed"]);
        assert_eq!(
            rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
            ["seed", "visible-a"]
        );
        assert_eq!(rows[0].score, 0.73);
        assert_eq!(rows[1].score, 0.42);
        let payload = serde_json::to_string(&rows).unwrap();
        for hidden in [
            "deleted-target",
            "blocked-target",
            "missing-target",
            "earlier-b",
            "earlier-seed",
            "earlier-blocked",
        ] {
            assert!(!payload.contains(hidden), "nested link leaked: {hidden}");
        }
        let stored = runtime
            .block_on(state.store.get_memory_by_id("seed"))
            .unwrap()
            .unwrap();
        assert_eq!(stored.related_memory_ids, before.related_memory_ids);
    }

    #[test]
    fn search_rows_bound_alias_resolution_but_keep_direct_links_after_the_cap() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let state = source_statement_test_state(dir.path());
        let mut survivor = source_statement_record("survivor");
        survivor.consolidated_from = vec!["late-alias".into()];
        runtime
            .block_on(state.store.add_batch_preserving_ids(&[survivor]))
            .unwrap();
        let mut first = source_statement_record("first");
        first.related_memory_ids = (0..64).map(|i| format!("missing-{i}")).collect();
        let mut second = source_statement_record("second");
        second.related_memory_ids = vec!["late-alias".into()];
        let mut third = source_statement_record("third");
        third.related_memory_ids = vec!["survivor".into()];
        let mut rows = vec![
            memory_record_to_search_result(&first, 0.9),
            memory_record_to_search_result(&second, 0.8),
            memory_record_to_search_result(&third, 0.7),
        ];
        runtime.block_on(authorize_related_memory_ids(&mut rows, &state.store, &[]));
        assert!(rows[0].related_memory_ids.is_empty());
        assert!(rows[1].related_memory_ids.is_empty());
        assert_eq!(rows[2].related_memory_ids, ["survivor"]);
        // The late alias really exists; omission is the bounded fallback policy.
        assert_eq!(
            runtime
                .block_on(state.store.get_memory_by_id("late-alias"))
                .unwrap()
                .unwrap()
                .id,
            "survivor"
        );
    }

    #[test]
    fn selected_source_statements_resolve_aliases_and_preserve_snapshot_citations() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let state = source_statement_test_state(dir.path());
        let mut record = source_statement_record("survivor");
        record.consolidated_from = vec!["earlier-frame".into()];
        runtime
            .block_on(state.store.add_batch_preserving_ids(&[record]))
            .unwrap();
        let statements = runtime
            .block_on(memory_source_statements(&state, "earlier-frame"))
            .unwrap();
        assert_eq!(statements.len(), 2);
        assert!(statements.iter().all(|s| s.memory_ids == ["survivor"]));
        assert!(statements
            .iter()
            .any(|s| s.quote == "Check the release evidence."
                && s.line == 3
                && s.source_sha256 == "a".repeat(64)));
        assert!(statements
            .iter()
            .any(|s| s.quote == "We discussed a possible rollback."
                && s.line == 1
                && s.source_sha256 == "b".repeat(64)));
    }

    #[test]
    fn selected_source_statements_hide_missing_deleted_internal_low_signal_and_blocked_rows() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let state = source_statement_test_state(dir.path());
        let kinds = [
            "deleted",
            "internal",
            "low-signal",
            "blocked-app",
            "blocked-title",
            "blocked-url",
            "blocked-note-body",
            "blocked-note-project",
        ];
        let rows = kinds
            .iter()
            .map(|kind| {
                let mut record = source_statement_record(kind);
                match *kind {
                    "deleted" => record.is_soft_deleted = true,
                    "internal" => record.bundle_id = Some("com.fndr.app".into()),
                    "low-signal" => record.storage_outcome = "visual_semantics_failed".into(),
                    "blocked-app" => record.app_name = "PrivateWorkspace".into(),
                    "blocked-title" => record.window_title = "PrivateWorkspace planning".into(),
                    "blocked-url" => {
                        record.url = Some("https://privateworkspace.example/notes".into())
                    }
                    "blocked-note-body" => {
                        record.source_type = "agent".into();
                        record.clean_text =
                            "PrivateWorkspace project plans remain under discussion.".into();
                    }
                    "blocked-note-project" => {
                        record.source_type = "agent".into();
                        record.project = "PrivateWorkspace".into();
                    }
                    _ => unreachable!(),
                }
                record
            })
            .collect::<Vec<_>>();
        runtime
            .block_on(state.store.add_batch_preserving_ids(&rows))
            .unwrap();
        state.config.write().blocklist = vec!["privateworkspace".into()];
        for id in kinds.into_iter().chain(["missing"]) {
            assert!(
                runtime
                    .block_on(memory_source_statements(&state, id))
                    .unwrap()
                    .is_empty(),
                "{id}"
            );
        }
    }

    fn terms(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn the_chunk_route_raises_the_strong_match_bar() {
        let hit = |score: f32, routes: &[&str]| RetrieveHit {
            memory_id: "m".to_string(),
            chunk_id: None,
            matched_text: None,
            score,
            why: RetrieveWhy {
                routes: routes.iter().map(|route| route.to_string()).collect(),
                matched_terms: Vec::new(),
            },
        };
        let words = terms(&["dentist", "appointment"]);
        assert!(is_strong_match(&hit(0.30, &["vector"]), &words, 0.45));
        assert!(!is_strong_match(&hit(0.20, &["vector"]), &words, 0.45));
        // With chunks the same evidence scores higher (VS-18), so the bar rises.
        assert!(!is_strong_match(
            &hit(0.41, &["chunk", "vector"]),
            &words,
            0.45
        ));
        assert!(is_strong_match(
            &hit(0.53, &["chunk", "vector"]),
            &words,
            0.45
        ));
    }

    fn found_by(routes: &[&str], score: f32, matched: &[&str]) -> RetrieveHit {
        RetrieveHit {
            memory_id: "m".to_string(),
            chunk_id: None,
            matched_text: None,
            score,
            why: RetrieveWhy {
                routes: routes.iter().map(|route| route.to_string()).collect(),
                matched_terms: terms(matched),
            },
        }
    }

    #[test]
    fn keyword_score_without_the_query_words_does_not_make_a_strong_match() {
        // "estate probate trust documents" on a vault with nothing about it:
        // the keyword route scored a row holding none of the words and the
        // meaning score was low, yet the fused score cleared the bar.
        let words = terms(&["estate", "probate", "trust", "documents"]);
        let routes = ["vector", "keyword"];
        assert!(!is_strong_match(
            &found_by(&routes, 0.40, &[]),
            &words,
            0.31
        ));
        // One word of four is still not the query.
        assert!(!is_strong_match(
            &found_by(&routes, 0.30, &["documents"]),
            &words,
            0.18
        ));
    }

    #[test]
    fn other_evidence_keeps_a_match_strong() {
        let words = terms(&["estate", "probate", "trust", "documents"]);
        let routes = ["vector", "keyword"];
        // Close in meaning, whatever the keyword route found.
        assert!(is_strong_match(&found_by(&routes, 0.40, &[]), &words, 0.50));
        // A third or more of the words are there.
        assert!(is_strong_match(
            &found_by(&routes, 0.30, &["estate", "probate"]),
            &words,
            0.18
        ));
        // The entity route also found it.
        assert!(is_strong_match(
            &found_by(&["vector", "keyword", "entity"], 0.30, &[]),
            &words,
            0.18
        ));
        // Every word is there, even under the score bar.
        assert!(is_strong_match(
            &found_by(&routes, 0.20, &["estate", "probate", "trust", "documents"]),
            &words,
            0.0
        ));
    }

    #[test]
    fn being_in_the_named_time_is_not_evidence_of_the_topic() {
        // "estate probate trust documents yesterday": the time route finds
        // every memory from yesterday, whatever it is about.
        let words = terms(&["estate", "probate", "trust", "documents"]);
        let routes = ["vector", "keyword", "temporal"];
        assert!(!is_strong_match(
            &found_by(&routes, 0.40, &[]),
            &words,
            0.18
        ));
        // A query that is only a time has no topic words to miss.
        assert!(is_strong_match(&found_by(&routes, 0.40, &[]), &[], 0.18));
        // The topic is there as well.
        assert!(is_strong_match(
            &found_by(&routes, 0.40, &["estate", "probate"]),
            &words,
            0.18
        ));
    }

    #[test]
    fn matched_terms_keeps_query_order_and_whole_words_only() {
        let text = "The Zephyr vendor contract renews; contractor list attached";
        assert_eq!(
            matched_terms(&terms(&["contract", "zephyr", "renewal"]), text),
            vec!["contract", "zephyr"]
        );
        // "contract" inside "contractor" alone is not a match.
        assert!(matched_terms(&terms(&["contract"]), "contractor list").is_empty());
    }

    #[test]
    fn matched_terms_handles_case_punctuation_and_phrases() {
        let text = "Customers asked for SINGLE sign-on, during onboarding.";
        assert_eq!(
            matched_terms(&terms(&["single sign", "onboarding"]), text),
            vec!["single sign", "onboarding"]
        );
        // A phrase matches only when its words are adjacent.
        assert!(matched_terms(&terms(&["customers onboarding"]), text).is_empty());
        assert!(matched_terms(&terms(&[""]), text).is_empty());
    }
}
