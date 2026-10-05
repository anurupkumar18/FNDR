//! VS-09: the one retrieval function every surface calls. It runs the same
//! plan, routes, fusion, and low-signal filter as Ask (`run_query`) and
//! returns ranked memory ids with why each one matched, before any card
//! synthesis or answer composition.

use crate::context_runtime::query_filters::parse_query_filters;
use crate::context_runtime::query_plan::Route;
use crate::context_runtime::retrieval_routes::memory_record_to_search_result;
use crate::context_runtime::{retrieve_fused, FusedRetrieval};
use crate::search::{normalize_text, QueryContext};
use crate::storage::SearchResult;
use crate::AppState;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;

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

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type, PartialEq)]
pub struct RetrieveResult {
    pub hits: Vec<RetrieveHit>,
    /// What was actually searched, so a surface can show "yesterday, Slack".
    pub filters: RetrieveFilters,
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
    /// The matched chunk once chunk retrieval lands (VS-18).
    pub chunk_id: Option<String>,
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
    Ok(retrieve_inner(state, request).await.0)
}

/// `retrieve` plus each hit's stored row as a `SearchResult`, in hit order,
/// for surfaces that render rows (Search's cards, VS-10). The rows come from
/// the lookup the hidden-memory drop already made; a hit whose row is gone is
/// left out.
pub async fn retrieve_search_results(
    state: &AppState,
    request: &RetrieveRequest,
) -> Result<(RetrieveResult, Vec<SearchResult>), String> {
    let (result, retrieval) = retrieve_inner(state, request).await;
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
    let rows = result
        .hits
        .iter()
        .filter_map(|hit| {
            let record = retrieval.records.get(&hit.memory_id)?;
            let mut row = memory_record_to_search_result(record, hit.score);
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

async fn retrieve_inner(
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
    // Report matched words without the filter phrases ("yesterday", "Slack").
    let terms = QueryContext::from_query(&words_without_phrases).anchor_terms;

    let hits = retrieval
        .fused
        .iter()
        .take(limit)
        .map(|hit| RetrieveHit {
            memory_id: hit.memory_id.clone(),
            chunk_id: None,
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
    (RetrieveResult { hits, filters }, retrieval)
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
