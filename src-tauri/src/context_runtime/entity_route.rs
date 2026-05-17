use crate::context_runtime::query_plan::{EntityHintKind, QueryPlan, Route};
use crate::context_runtime::retrieval_routes::{
    finish_route, memory_record_to_search_result, RetrievalRoute, RouteBranch, RouteCtx, RouteHit,
    RouteHits, RouteSignals,
};
use crate::graph::schema::{GraphNode, GraphNodeType};
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::time::Instant;

pub struct EntityRoute;

impl RetrievalRoute for EntityRoute {
    fn route(&self) -> Route {
        Route::Entity
    }

    fn run<'a>(&'a self, plan: &'a QueryPlan, ctx: &'a RouteCtx<'a>) -> BoxFuture<'a, RouteHits> {
        Box::pin(async move {
            let started = Instant::now();
            if plan.target_entities.is_empty() && plan.target_project.is_none() {
                return finish_route(Route::Entity, started, Vec::new());
            }

            let mut by_id: HashMap<String, RouteHit> = HashMap::new();
            for node in ctx.graph_nodes {
                let Some((match_score, kind_weight)) = node_match_score(node, plan) else {
                    continue;
                };
                let score = (node.confidence * match_score * kind_weight).clamp(0.0, 1.0);
                for memory_id in &node.source_memory_ids {
                    if memory_id.trim().is_empty() {
                        continue;
                    }
                    let search_result = match ctx.store.get_memory_by_id(memory_id).await {
                        Ok(Some(record)) => Some(memory_record_to_search_result(&record, score)),
                        Ok(None) => None,
                        Err(err) => {
                            tracing::warn!(err = %err, memory_id = %memory_id, "retrieval_route:entity_memory_fetch_failed");
                            None
                        }
                    };
                    insert_best(
                        &mut by_id,
                        RouteHit {
                            memory_id: memory_id.clone(),
                            score,
                            signals: RouteSignals {
                                branch: RouteBranch::Entity,
                                confidence: score,
                                search_result,
                            },
                            graph_path: None,
                        },
                    );
                }
            }

            let mut hits = by_id.into_values().collect::<Vec<_>>();
            hits.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            hits.truncate(ctx.limit.max(1));
            finish_route(Route::Entity, started, hits)
        })
    }
}

fn node_match_score(node: &GraphNode, plan: &QueryPlan) -> Option<(f32, f32)> {
    if let Some(project) = plan.target_project.as_ref() {
        if matches!(
            node.node_type,
            GraphNodeType::Project | GraphNodeType::Concept
        ) && label_matches(&node.label, project)
        {
            return Some((
                label_match_score(&node.label, project),
                node_type_weight(node.node_type),
            ));
        }
    }

    for entity in &plan.target_entities {
        if !kind_matches(node.node_type, entity.kind) {
            continue;
        }
        if label_matches(&node.label, &entity.label) {
            return Some((
                label_match_score(&node.label, &entity.label),
                node_type_weight(node.node_type),
            ));
        }
    }

    None
}

fn kind_matches(node_type: GraphNodeType, kind: EntityHintKind) -> bool {
    match kind {
        EntityHintKind::Concept => {
            matches!(node_type, GraphNodeType::Concept | GraphNodeType::Project)
        }
        EntityHintKind::Person => node_type == GraphNodeType::Person,
        EntityHintKind::Tool => node_type == GraphNodeType::Tool,
        EntityHintKind::File => node_type == GraphNodeType::File,
        EntityHintKind::Url => node_type == GraphNodeType::Url,
        EntityHintKind::App => node_type == GraphNodeType::App,
        EntityHintKind::Command => node_type == GraphNodeType::Command,
    }
}

fn label_matches(label: &str, target: &str) -> bool {
    let label = label.trim().to_ascii_lowercase();
    let target = target.trim().to_ascii_lowercase();
    !label.is_empty()
        && !target.is_empty()
        && (label == target || label.starts_with(&target) || target.starts_with(&label))
}

fn label_match_score(label: &str, target: &str) -> f32 {
    if label.eq_ignore_ascii_case(target) {
        1.0
    } else {
        0.82
    }
}

fn node_type_weight(node_type: GraphNodeType) -> f32 {
    match node_type {
        GraphNodeType::Project => 1.0,
        GraphNodeType::File | GraphNodeType::Url | GraphNodeType::Command => 0.96,
        GraphNodeType::App | GraphNodeType::Tool => 0.92,
        GraphNodeType::Person => 0.90,
        GraphNodeType::Concept => 0.88,
        _ => 0.80,
    }
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
    use crate::config::SearchConfig;
    use crate::graph::schema::GraphNode;
    use crate::storage::Store;
    use chrono::Utc;
    use uuid::Uuid;

    fn node(label: &str, node_type: GraphNodeType, memory_id: &str) -> GraphNode {
        GraphNode {
            id: Uuid::new_v4(),
            node_type,
            label: label.to_string(),
            confidence: 0.9,
            source_memory_ids: vec![memory_id.to_string()],
            embedding: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            stale: false,
            metadata: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn entity_route_returns_matching_graph_node_memories() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).expect("store"))
            .await
            .expect("store task");
        let config = SearchConfig::default().normalized();
        let graph_nodes = vec![node("FNDR", GraphNodeType::Project, "entity-1")];
        let plan = crate::context_runtime::query_plan::plan(
            "FNDR",
            &crate::context_runtime::query_plan::PlanHints {
                entity_aliases: vec![crate::context_runtime::query_plan::EntityAliasHint {
                    alias: "fndr".to_string(),
                    canonical_name: "FNDR".to_string(),
                    entity_type: "project".to_string(),
                    project: Some("FNDR".to_string()),
                }],
                ..Default::default()
            },
        );
        let graph_index = crate::graph::graph_index::GraphIndex::build(&graph_nodes, &[]);
        let ctx = RouteCtx::new(&store, &config).with_graph(&graph_index, &graph_nodes, &[]);

        let hits = EntityRoute.run(&plan, &ctx).await;
        assert_eq!(hits.route, Route::Entity);
        assert_eq!(hits.hits[0].memory_id, "entity-1");
    }

    #[tokio::test]
    async fn entity_route_is_empty_without_entity_targets() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).expect("store"))
            .await
            .expect("store task");
        let config = SearchConfig::default().normalized();
        let mut plan = crate::context_runtime::query_plan::plan(
            "plain lookup",
            &crate::context_runtime::query_plan::PlanHints::default(),
        );
        plan.target_entities.clear();
        plan.target_project = None;
        let ctx = RouteCtx::new(&store, &config);

        let hits = EntityRoute.run(&plan, &ctx).await;
        assert!(hits.hits.is_empty());
    }
}
