use crate::context_runtime::query_plan::{QueryPlan, Route};
use crate::context_runtime::retrieval_routes::{
    finish_route, hit_from_search_result, memory_record_to_search_result, RetrievalRoute,
    RouteBranch, RouteCtx, RouteHit, RouteHits,
};
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::time::Instant;

const HOUR_MS: f32 = 3_600_000.0;

pub struct TemporalRoute;

impl RetrievalRoute for TemporalRoute {
    fn route(&self) -> Route {
        Route::Temporal
    }

    fn run<'a>(&'a self, plan: &'a QueryPlan, ctx: &'a RouteCtx<'a>) -> BoxFuture<'a, RouteHits> {
        Box::pin(async move {
            let started = Instant::now();
            let Some(window) = plan.time_window.as_ref() else {
                return finish_route(Route::Temporal, started, Vec::new());
            };

            let mut by_id: HashMap<String, RouteHit> = HashMap::new();
            match ctx
                .store
                .get_search_results_in_range(window.from_ms, window.to_ms)
                .await
            {
                Ok(results) => {
                    for mut result in results {
                        if !app_matches(&result.app_name, ctx.app_filter) {
                            continue;
                        }
                        let temporal_score =
                            temporal_score_for_query(&plan.raw, ctx.now_ms, result.timestamp);
                        result.score = result.score.max(temporal_score);
                        insert_best(
                            &mut by_id,
                            hit_from_search_result(Route::Temporal, RouteBranch::Temporal, result),
                        );
                    }
                }
                Err(err) => {
                    tracing::warn!(err = %err, "retrieval_route:temporal_memory_range_failed");
                }
            }

            let events = match ctx
                .store
                .list_activity_events(
                    ctx.limit.saturating_mul(2).max(4),
                    plan.target_project.as_deref(),
                )
                .await
            {
                Ok(events) => events,
                Err(err) => {
                    tracing::warn!(err = %err, "retrieval_route:temporal_activity_failed");
                    Vec::new()
                }
            };

            for event in events {
                if event.end_time < window.from_ms || event.end_time > window.to_ms {
                    continue;
                }
                if event.memory_id.trim().is_empty() {
                    continue;
                }
                let score = temporal_score_for_query(&plan.raw, ctx.now_ms, event.end_time);
                match ctx.store.get_memory_by_id(&event.memory_id).await {
                    Ok(Some(record)) => {
                        if !app_matches(&record.app_name, ctx.app_filter) {
                            continue;
                        }
                        insert_best(
                            &mut by_id,
                            RouteHit {
                                memory_id: record.id.clone(),
                                score,
                                signals: crate::context_runtime::retrieval_routes::RouteSignals {
                                    branch: RouteBranch::Temporal,
                                    confidence: score,
                                    search_result: Some(memory_record_to_search_result(
                                        &record, score,
                                    )),
                                },
                                graph_path: None,
                            },
                        );
                    }
                    Ok(None) => {}
                    Err(err) => {
                        tracing::warn!(err = %err, memory_id = %event.memory_id, "retrieval_route:temporal_memory_fetch_failed");
                    }
                }
            }

            let mut hits = by_id.into_values().collect::<Vec<_>>();
            hits.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            hits.truncate(ctx.limit.max(1));
            finish_route(Route::Temporal, started, hits)
        })
    }
}

pub fn apply_recency_decay(now_ms: i64, event_ms: i64) -> f32 {
    recency_decay(now_ms, event_ms, 24.0 * HOUR_MS)
}

fn temporal_score_for_query(raw: &str, now_ms: i64, event_ms: i64) -> f32 {
    let half_life_ms = temporal_half_life_ms(raw, now_ms, event_ms);
    recency_decay(now_ms, event_ms, half_life_ms)
}

fn temporal_half_life_ms(raw: &str, now_ms: i64, event_ms: i64) -> f32 {
    let query = raw.to_ascii_lowercase();
    if query.contains("now") || query.contains("recent") {
        return 6.0 * HOUR_MS;
    }
    if query.contains("week") || (now_ms - event_ms).abs() > 2 * 86_400_000 {
        return 7.0 * 24.0 * HOUR_MS;
    }
    24.0 * HOUR_MS
}

fn recency_decay(now_ms: i64, event_ms: i64, half_life_ms: f32) -> f32 {
    let age_ms = (now_ms - event_ms).max(0) as f32;
    2.0_f32
        .powf(-(age_ms / half_life_ms.max(1.0)))
        .clamp(0.0, 1.0)
}

fn app_matches(app_name: &str, app_filter: Option<&str>) -> bool {
    let Some(filter) = app_filter.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    app_name.eq_ignore_ascii_case(filter)
}

fn insert_best(by_id: &mut HashMap<String, RouteHit>, hit: RouteHit) {
    by_id
        .entry(hit.memory_id.clone())
        .and_modify(|existing| {
            if hit.score > existing.score {
                *existing = hit.clone();
            }
        })
        .or_insert(hit);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SearchConfig, DEFAULT_IMAGE_EMBEDDING_DIM};
    use crate::embedding::EMBEDDING_DIM;
    use crate::storage::{MemoryRecord, Store};

    fn record(id: &str, timestamp: i64) -> MemoryRecord {
        MemoryRecord {
            id: id.to_string(),
            timestamp,
            app_name: "Terminal".to_string(),
            window_title: "Temporal Route".to_string(),
            session_id: "temporal-session".to_string(),
            text: "temporal route memory".to_string(),
            clean_text: "temporal route memory".to_string(),
            snippet: "temporal route memory".to_string(),
            summary_source: "llm".to_string(),
            embedding: vec![0.0; EMBEDDING_DIM],
            snippet_embedding: vec![0.0; EMBEDDING_DIM],
            support_embedding: vec![0.0; EMBEDDING_DIM],
            image_embedding: vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM],
            decay_score: 1.0,
            ..Default::default()
        }
    }

    #[test]
    fn recency_decay_scores_recent_events_higher() {
        let now = 1_000_000;
        assert!(apply_recency_decay(now, now) > apply_recency_decay(now, now - 86_400_000));
    }

    #[tokio::test]
    async fn temporal_route_returns_window_hits() {
        let now = chrono::Utc::now().timestamp_millis();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).expect("store"))
            .await
            .expect("store task");
        store
            .add_batch(&[record("temporal-1", now - 1_000)])
            .await
            .expect("add");
        let config = SearchConfig::default().normalized();
        let mut plan = crate::context_runtime::query_plan::plan(
            "today temporal route",
            &crate::context_runtime::query_plan::PlanHints {
                now_ms: Some(now),
                ..Default::default()
            },
        );
        plan.time_window = Some(crate::context_runtime::query_plan::TimeWindow {
            from_ms: now - 60_000,
            to_ms: now + 60_000,
        });
        let ctx = RouteCtx::new(&store, &config).with_now_ms(now);

        let hits = TemporalRoute.run(&plan, &ctx).await;
        assert_eq!(hits.route, Route::Temporal);
        assert!(!hits.hits.is_empty());
    }

    #[tokio::test]
    async fn temporal_route_is_empty_without_time_window() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).expect("store"))
            .await
            .expect("store task");
        let config = SearchConfig::default().normalized();
        let mut plan = crate::context_runtime::query_plan::plan(
            "temporal route",
            &crate::context_runtime::query_plan::PlanHints::default(),
        );
        plan.time_window = None;
        let ctx = RouteCtx::new(&store, &config);

        let hits = TemporalRoute.run(&plan, &ctx).await;
        assert!(hits.hits.is_empty());
    }
}
