use crate::context_runtime::query_plan::{QueryPlan, Route};
use crate::context_runtime::retrieval_routes::{
    finish_route, hit_from_search_result, RetrievalRoute, RouteBranch, RouteCtx, RouteHit,
    RouteHits,
};
use crate::embedding::prefixes::prefix_query_for_search;
use crate::embedding::shared_bge_v5_query_embedder;
use crate::inference::model_config::BGE_V5_DIMENSIONS;
use crate::storage::{MemoryChunkSearchResult, SearchResult, Store};
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::timeout;

/// A memory's other chunks within this gap of its best chunk each add
/// `CLOSE_CHUNK_BONUS`, at most `MAX_CLOSE_CHUNKS` times (the bake-off's rule).
const CLOSE_CHUNK_GAP: f32 = 0.05;
const CLOSE_CHUNK_BONUS: f32 = 0.01;
const MAX_CLOSE_CHUNKS: usize = 3;

pub struct ChunkRoute;

impl RetrievalRoute for ChunkRoute {
    fn route(&self) -> Route {
        Route::Chunk
    }

    fn run<'a>(&'a self, plan: &'a QueryPlan, ctx: &'a RouteCtx<'a>) -> BoxFuture<'a, RouteHits> {
        Box::pin(async move {
            let route_started = Instant::now();
            if !ctx.search_config.use_chunk_first_retrieval {
                tracing::info!("chunk_first_retrieval:fallback reason=config_disabled");
                return finish_route(Route::Chunk, route_started, Vec::new());
            }
            if plan.raw.trim().is_empty() {
                return finish_route(Route::Chunk, route_started, Vec::new());
            }
            match ctx.store.has_chunk_retrieval_index().await {
                Ok(true) => {}
                Ok(false) => {
                    tracing::info!("chunk_first_retrieval:fallback reason=index_empty");
                    return finish_route(Route::Chunk, route_started, Vec::new());
                }
                Err(err) => {
                    tracing::warn!(
                        err = %err,
                        "chunk_first_retrieval:fallback reason=index_unavailable"
                    );
                    return finish_route(Route::Chunk, route_started, Vec::new());
                }
            }

            let search_limit = ctx.limit.max(1) * 3;
            let mut chunk_hits = Vec::new();

            // BM25 over chunk text (VS-18) needs no model, so the words a
            // person remembers still find their chunk without one.
            let keyword_timeout = Duration::from_millis(ctx.search_config.keyword_timeout_ms);
            match timeout(
                keyword_timeout,
                ctx.store.chunk_keyword_search(&plan.raw, search_limit),
            )
            .await
            {
                Ok(Ok(results)) => chunk_hits.extend(results),
                Ok(Err(err)) => {
                    tracing::warn!(err = %err, "chunk_first_retrieval:keyword_failed");
                }
                Err(_) => {
                    tracing::warn!(
                        timeout_ms = keyword_timeout.as_millis(),
                        "chunk_first_retrieval:keyword_timeout"
                    );
                }
            }

            match chunk_vector_hits(plan, ctx, search_limit).await {
                Ok(results) => chunk_hits.extend(results),
                Err(reason) => {
                    tracing::info!(reason, "chunk_first_retrieval:vector_skipped");
                }
            }

            let results = assemble_chunk_parent_results(
                ctx.store,
                chunk_hits,
                ctx.limit,
                ctx.time_filter,
                ctx.app_filter,
            )
            .await;
            let hits = match results {
                Ok(results) => results
                    .into_iter()
                    .map(|result| hit_from_search_result(Route::Chunk, RouteBranch::Chunk, result))
                    .collect::<Vec<RouteHit>>(),
                Err(err) => {
                    tracing::warn!(
                        err = %err,
                        "chunk_first_retrieval:fallback reason=parent_assembly_failed"
                    );
                    Vec::new()
                }
            };

            finish_route(Route::Chunk, route_started, hits)
        })
    }
}

/// Chunk vector hits from the BGE chunk model, or why there are none.
async fn chunk_vector_hits(
    plan: &QueryPlan,
    ctx: &RouteCtx<'_>,
    search_limit: usize,
) -> Result<Vec<MemoryChunkSearchResult>, &'static str> {
    let bge = shared_bge_v5_query_embedder().map_err(|_| "bge_unavailable")?;
    let query_text = prefix_query_for_search(&chunk_query_text(plan, ctx.expansion));
    let query_embedding = bge
        .embed_batch(&[query_text])
        .map_err(|_| "query_embedding_failed")?
        .into_iter()
        .next()
        .unwrap_or_default();
    if query_embedding.len() != BGE_V5_DIMENSIONS {
        return Err("query_dimension_mismatch");
    }
    let search_timeout = Duration::from_millis(ctx.search_config.semantic_timeout_ms);
    match timeout(
        search_timeout,
        ctx.store
            .chunk_vector_search(&query_embedding, search_limit),
    )
    .await
    {
        Ok(Ok(results)) => Ok(results),
        Ok(Err(_)) => Err("chunk_search_failed"),
        Err(_) => Err("chunk_search_timeout"),
    }
}

fn chunk_query_text(plan: &QueryPlan, expansion: &[String]) -> String {
    let mut parts = vec![plan.raw.trim().to_string()];
    let extras = expansion
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>()
        .join(", ");
    if !extras.is_empty() {
        parts.push(extras);
    }
    parts.join(" ")
}

pub(crate) async fn assemble_chunk_parent_results(
    store: &Store,
    chunk_hits: Vec<MemoryChunkSearchResult>,
    limit: usize,
    time_filter: Option<&str>,
    app_filter: Option<&str>,
) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
    // One score per chunk (the better of its vector and BM25 hits), then
    // one per memory: its best chunk plus a small bonus for each other chunk
    // close behind (VS-18), so a memory that matches in several places wins
    // a near tie.
    let mut by_chunk: HashMap<String, MemoryChunkSearchResult> = HashMap::new();
    for hit in chunk_hits {
        if hit.chunk.memory_id.trim().is_empty() {
            continue;
        }
        let replace = by_chunk
            .get(&hit.chunk.id)
            .map(|existing| hit.score > existing.score)
            .unwrap_or(true);
        if replace {
            by_chunk.insert(hit.chunk.id.clone(), hit);
        }
    }
    let mut by_parent: HashMap<String, Vec<MemoryChunkSearchResult>> = HashMap::new();
    for hit in by_chunk.into_values() {
        by_parent
            .entry(hit.chunk.memory_id.clone())
            .or_default()
            .push(hit);
    }
    let mut ranked = by_parent
        .into_values()
        .map(|mut chunks| {
            chunks.sort_by(|left, right| {
                right
                    .score
                    .partial_cmp(&left.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.chunk.id.cmp(&right.chunk.id))
            });
            let mut best = chunks.swap_remove(0);
            let close = chunks
                .iter()
                .filter(|chunk| chunk.score >= best.score - CLOSE_CHUNK_GAP)
                .count()
                .min(MAX_CLOSE_CHUNKS);
            best.score = (best.score + CLOSE_CHUNK_BONUS * close as f32).min(1.0);
            best
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                left.distance
                    .partial_cmp(&right.distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.chunk.memory_id.cmp(&right.chunk.memory_id))
    });
    ranked.truncate(limit.max(1));

    let ordered_ids = ranked
        .iter()
        .map(|hit| hit.chunk.memory_id.clone())
        .collect::<Vec<_>>();
    let mut parents = store
        .get_v5_search_results_by_ids(&ordered_ids, time_filter, app_filter)
        .await?
        .into_iter()
        .map(|parent| (parent.id.clone(), parent))
        .collect::<HashMap<_, _>>();

    let mut results = Vec::new();
    for hit in ranked {
        let Some(mut parent) = parents.remove(&hit.chunk.memory_id) else {
            tracing::warn!(
                memory_id = %hit.chunk.memory_id,
                chunk_id = %hit.chunk.id,
                "chunk_first_retrieval:orphan_chunk_skipped"
            );
            continue;
        };
        parent.score = hit.score;
        parent.matched_routes = vec!["Chunk".to_string()];
        parent.matched_chunk_ids = vec![hit.chunk.id.clone()];
        parent.chunk_evidence = vec![hit.evidence()];
        results.push(parent);
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SearchConfig, DEFAULT_IMAGE_EMBEDDING_DIM};
    use crate::storage::{MemoryChunkRecord, MemoryRecord};

    fn parent(id: &str, text: &str) -> MemoryRecord {
        MemoryRecord {
            id: id.to_string(),
            timestamp: 1_000,
            app_name: "Chrome".to_string(),
            window_title: format!("Parent {id}"),
            session_id: "chunk-route".to_string(),
            text: text.to_string(),
            clean_text: text.to_string(),
            snippet: text.to_string(),
            summary_source: "llm".to_string(),
            embedding: vec![0.2; BGE_V5_DIMENSIONS],
            snippet_embedding: vec![0.2; BGE_V5_DIMENSIONS],
            support_embedding: vec![0.2; BGE_V5_DIMENSIONS],
            image_embedding: vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM],
            decay_score: 1.0,
            ..Default::default()
        }
    }

    fn chunk(id: &str, memory_id: &str, score: f32, text: &str) -> MemoryChunkSearchResult {
        MemoryChunkSearchResult {
            chunk: MemoryChunkRecord {
                id: id.to_string(),
                memory_id: memory_id.to_string(),
                chunk_index: 0,
                line_kind: "plain".to_string(),
                text: text.to_string(),
                embedding: vec![score; BGE_V5_DIMENSIONS],
                created_at: 1_000,
                app_name: "Chrome".to_string(),
                window_title: "Chunk route".to_string(),
                day_bucket: "2026-05-20".to_string(),
                content_hash: format!("hash-{id}"),
            },
            score,
            distance: 1.0 - score,
        }
    }

    #[tokio::test]
    async fn chunk_route_assembles_parent_with_winning_chunk_evidence() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).expect("store"))
            .await
            .expect("store task");
        store
            .add_v5_batch_preserving_ids(&[
                parent("parent-a", "Parent A"),
                parent("parent-b", "Parent B"),
            ])
            .await
            .expect("v5 parents");

        let results = assemble_chunk_parent_results(
            &store,
            vec![
                chunk("chunk-a-weak", "parent-a", 0.40, "Weak chunk"),
                chunk(
                    "chunk-a-strong",
                    "parent-a",
                    0.91,
                    "Winning chunk about parent-child retrieval",
                ),
                chunk("chunk-b", "parent-b", 0.73, "Other chunk"),
                chunk("orphan", "missing-parent", 0.99, "Orphan chunk"),
            ],
            4,
            None,
            None,
        )
        .await
        .expect("assemble");

        assert_eq!(results[0].id, "parent-a");
        assert_eq!(results[0].matched_chunk_ids, vec!["chunk-a-strong"]);
        assert_eq!(results[0].matched_routes, vec!["Chunk"]);
        assert!(results[0].chunk_evidence[0]
            .text
            .contains("parent-child retrieval"));
        assert!(results.iter().all(|result| result.id != "missing-parent"));
    }

    #[tokio::test]
    async fn several_matching_chunks_lift_their_memory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).expect("store"))
            .await
            .expect("store task");
        store
            .add_v5_batch_preserving_ids(&[parent("many", "Many"), parent("one", "One")])
            .await
            .expect("v5 parents");

        let results = assemble_chunk_parent_results(
            &store,
            vec![
                chunk("many-0", "many", 0.91, "First matching chunk"),
                chunk("many-1", "many", 0.89, "Second matching chunk"),
                chunk("many-2", "many", 0.88, "Third matching chunk"),
                chunk("many-3", "many", 0.50, "Unrelated chunk"),
                chunk("one-0", "one", 0.92, "Single strong chunk"),
            ],
            4,
            None,
            None,
        )
        .await
        .expect("assemble");

        // Best chunk 0.91 plus 0.01 for each of the two others within 0.05.
        assert_eq!(results[0].id, "many");
        assert!(
            (results[0].score - 0.93).abs() < 1e-5,
            "{}",
            results[0].score
        );
        assert_eq!(results[0].matched_chunk_ids, vec!["many-0"]);
        assert_eq!(results[1].id, "one");
    }

    #[tokio::test]
    async fn chunk_route_finds_words_even_without_the_chunk_model() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).expect("store"))
            .await
            .expect("store task");
        store
            .add_v5_batch_preserving_ids(&[parent("vendor", "Vendor"), parent("lunch", "Lunch")])
            .await
            .expect("v5 parents");
        let mut vendor = chunk(
            "vendor-1",
            "vendor",
            0.0,
            "The Zephyr vendor contract renews at the same price",
        );
        vendor.chunk.chunk_index = 1;
        store
            .upsert_memory_chunks(&[
                chunk("vendor-0", "vendor", 0.0, "Pricing table for the quarter").chunk,
                vendor.chunk,
                chunk("lunch-0", "lunch", 0.0, "Team lunch on Friday").chunk,
            ])
            .await
            .expect("chunks");
        let mut config = SearchConfig::default();
        config.use_chunk_first_retrieval = true;
        let config = config.normalized();
        let plan = crate::context_runtime::query_plan::plan(
            "zephyr contract",
            &crate::context_runtime::query_plan::PlanHints::default(),
        );
        let ctx = RouteCtx::new(&store, &config);

        let hits = ChunkRoute.run(&plan, &ctx).await;

        let top = hits.hits.first().expect("a chunk hit");
        assert_eq!(top.memory_id, "vendor");
        let result = top.signals.search_result.as_ref().expect("parent row");
        assert_eq!(result.matched_chunk_ids, vec!["vendor-1"]);
        assert!(result.chunk_evidence[0].text.contains("Zephyr"));
        assert!(hits.hits.iter().all(|hit| hit.memory_id != "lunch"));
    }

    #[tokio::test]
    async fn chunk_route_is_empty_when_index_unavailable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).expect("store"))
            .await
            .expect("store task");
        let config = SearchConfig::default().normalized();
        let plan = crate::context_runtime::query_plan::plan(
            "parent child retrieval",
            &crate::context_runtime::query_plan::PlanHints::default(),
        );
        let ctx = RouteCtx::new(&store, &config);

        let hits = ChunkRoute.run(&plan, &ctx).await;
        assert_eq!(hits.route, Route::Chunk);
        assert!(hits.hits.is_empty());
    }
}
