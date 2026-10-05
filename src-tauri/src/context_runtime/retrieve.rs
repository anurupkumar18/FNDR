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
use crate::storage::{MemoryRecord, SearchResult};
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

/// Resolve persisted peer links before considering similar context. Links are
/// directed evidence from the source row, not inferred graph edges. A missing
/// or excluded linked target is never replaced with a similarity suggestion.
pub async fn related_memories(
    state: &AppState,
    memory_id: &str,
    limit: usize,
) -> Result<Vec<MemoryCard>, String> {
    const MAX_RELATED_CARDS: usize = 12;
    const MAX_LINK_LOOKUPS: usize = 64;
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
    let visible = |record: &MemoryRecord| {
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
            && crate::memory_quality::record_low_signal_reason(record).is_none()
            && !crate::privacy::Blocklist::is_internal_app(
                &record.app_name,
                record.bundle_id.as_deref(),
            )
            && !crate::privacy::Blocklist::is_blocked(&record.app_name, &blocklist)
            && !crate::privacy::Blocklist::is_context_blocked(
                record.url.as_deref(),
                Some(&context),
                &blocklist,
            )
    };
    if !visible(&seed) {
        return Ok(Vec::new());
    }

    if !seed.related_memory_ids.is_empty() {
        let mut seen = HashSet::new();
        let ids = seed
            .related_memory_ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty() && seen.insert((*id).to_string()))
            .take(MAX_LINK_LOOKUPS)
            .map(str::to_string)
            .collect::<Vec<_>>();
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
            if let Some(record) =
                record.filter(|record| visible(record) && canonical_ids.insert(record.id.clone()))
            {
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
            if !visible(record) || !seen.insert(record.id.clone()) {
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
    let rows = result
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
    Ok((result, rows))
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
    let strong_match = hits.first().is_some_and(|hit| is_strong_match(hit, &terms));
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
fn is_strong_match(hit: &RetrieveHit, terms: &[String]) -> bool {
    let words = terms
        .iter()
        .filter(|term| !term.contains(' '))
        .collect::<Vec<_>>();
    let bar = if hit.why.routes.iter().any(|route| route == "chunk") {
        STRONG_MATCH_SCORE_WITH_CHUNKS
    } else {
        STRONG_MATCH_SCORE
    };
    hit.score >= bar
        || (!words.is_empty()
            && words
                .iter()
                .all(|word| hit.why.matched_terms.contains(word)))
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
        assert!(is_strong_match(&hit(0.30, &["vector", "keyword"]), &words));
        assert!(!is_strong_match(&hit(0.20, &["vector"]), &words));
        // With chunks the same evidence scores higher (VS-18), so the bar rises.
        assert!(!is_strong_match(&hit(0.41, &["chunk", "vector"]), &words));
        assert!(is_strong_match(&hit(0.53, &["chunk", "vector"]), &words));
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
