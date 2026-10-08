//! Phase 3 fusion stage: combine per-route hits into a single ranked list with
//! `FusionSignals` + `SurfacingReason` attached. Pure function, no I/O.

use crate::context_runtime::context_pack::{
    FusedHit, FusionSignals, FusionWeights, SurfacingReason,
};
use crate::context_runtime::query_plan::{QueryPlan, Route};
use crate::context_runtime::retrieval_routes::{PathStep, RouteHits};
use crate::telemetry::runtime_metrics;
use std::collections::HashMap;
use std::time::Instant;

const MAX_FUSED_HITS: usize = 50;

pub fn fuse(plan: &QueryPlan, hits: Vec<RouteHits>, weights: &FusionWeights) -> Vec<FusedHit> {
    let started = Instant::now();
    let mut agg: HashMap<String, Agg> = HashMap::new();

    for route_hits in &hits {
        let weight = weight_for(weights, route_hits.route);
        for hit in &route_hits.hits {
            let entry = agg.entry(hit.memory_id.clone()).or_default();
            let normalized = hit.score.clamp(0.0, 1.0);
            entry.add_route(route_hits.route, normalized, weight);
            if matches!(route_hits.route, Route::Graph) {
                if let Some(path) = &hit.graph_path {
                    if entry.graph_path.is_none() && !path.is_empty() {
                        entry.graph_path = Some(path.clone());
                    }
                }
            }
            if let Some(result) = hit.signals.search_result.as_ref() {
                entry.timestamp = entry.timestamp.max(result.timestamp);
                for label in &result.embedding_reason_labels {
                    if !entry.embedding_reason_labels.contains(label) {
                        entry.embedding_reason_labels.push(label.clone());
                    }
                }
            }
            entry.coverage = entry.coverage.max(coverage_from_hit(hit));
        }
    }

    let anchor_terms = plan_anchor_terms(plan);

    let mut fused: Vec<(FusedHit, i64)> = agg
        .into_iter()
        .map(|(memory_id, entry)| {
            let recency_boost = entry.signals.temporal * weights.recency;
            let surfacing_reason = build_surfacing_reason(
                &entry.contributing_routes,
                &entry.graph_path,
                anchor_terms.clone(),
                recency_boost,
                &entry.embedding_reason_labels,
            );
            let hit = FusedHit {
                memory_id,
                score: entry.score,
                signals: FusionSignals {
                    recency: recency_boost,
                    coverage: entry.coverage,
                    ..entry.signals
                },
                surfacing_reason,
                contributing_routes: entry.contributing_routes.clone(),
            };
            (hit, entry.timestamp)
        })
        .collect();

    // `agg` is a HashMap, so equal scores need a fixed order: the newer
    // memory, then the smaller id (VS-21).
    fused.sort_by(|(a, a_time), (b, b_time)| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b_time.cmp(a_time))
            .then_with(|| a.memory_id.cmp(&b.memory_id))
    });
    fused.truncate(MAX_FUSED_HITS);
    let fused = fused.into_iter().map(|(hit, _)| hit).collect::<Vec<_>>();

    runtime_metrics::record_ms(
        "fndr.retrieval.fusion.ms",
        started.elapsed().as_millis() as u64,
    );
    fused
}

struct Agg {
    signals: FusionSignals,
    score: f32,
    /// Newest timestamp any route reported, for breaking score ties.
    timestamp: i64,
    coverage: f32,
    graph_path: Option<Vec<PathStep>>,
    contributing_routes: Vec<Route>,
    embedding_reason_labels: Vec<String>,
}

impl Default for Agg {
    fn default() -> Self {
        Self {
            signals: FusionSignals::default(),
            score: 0.0,
            timestamp: i64::MIN,
            coverage: 0.0,
            graph_path: None,
            contributing_routes: Vec::new(),
            embedding_reason_labels: Vec::new(),
        }
    }
}

impl Agg {
    fn add_route(&mut self, route: Route, score: f32, weight: f32) {
        match route {
            Route::Chunk => self.signals.chunk = self.signals.chunk.max(score),
            Route::Vector => self.signals.vector = self.signals.vector.max(score),
            Route::Keyword => self.signals.keyword = self.signals.keyword.max(score),
            Route::Temporal => self.signals.temporal = self.signals.temporal.max(score),
            Route::Entity => self.signals.entity = self.signals.entity.max(score),
            Route::Graph => self.signals.graph = self.signals.graph.max(score),
        }
        self.score += score * weight;
        if !self.contributing_routes.contains(&route) {
            self.contributing_routes.push(route);
        }
    }
}

fn weight_for(weights: &FusionWeights, route: Route) -> f32 {
    match route {
        Route::Chunk => weights.chunk,
        Route::Vector => weights.vector,
        Route::Keyword => weights.keyword,
        Route::Temporal => weights.temporal,
        Route::Entity => weights.entity,
        Route::Graph => weights.graph,
    }
}

fn coverage_from_hit(hit: &crate::context_runtime::retrieval_routes::RouteHit) -> f32 {
    hit.signals
        .search_result
        .as_ref()
        .map(|sr| sr.anchor_coverage_score)
        .unwrap_or(0.0)
}

fn plan_anchor_terms(plan: &QueryPlan) -> Vec<String> {
    let mut terms = Vec::new();
    for hint in &plan.target_entities {
        if !hint.label.trim().is_empty() {
            terms.push(hint.label.clone());
        }
    }
    for topic in &plan.target_topics {
        if !topic.trim().is_empty() {
            terms.push(topic.clone());
        }
    }
    terms.sort();
    terms.dedup();
    terms
}

fn build_surfacing_reason(
    contributing_routes: &[Route],
    graph_path: &Option<Vec<PathStep>>,
    anchor_terms_hit: Vec<String>,
    recency_boost: f32,
    embedding_reason_labels: &[String],
) -> SurfacingReason {
    let mut route_strings: Vec<String> = contributing_routes
        .iter()
        .map(|route| route_label(*route, graph_path))
        .collect();
    for label in embedding_reason_labels {
        if !route_strings.contains(label) {
            route_strings.push(label.clone());
        }
    }

    let headline = if let Some(path) = graph_path.as_ref().filter(|p| !p.is_empty()) {
        let last = path.last().unwrap();
        let first = path.first().unwrap();
        format!(
            "Reached via {} from {}",
            edge_label(&last.edge),
            first.from_label
        )
    } else if contributing_routes.contains(&Route::Temporal) && contributing_routes.len() >= 2 {
        format!("Most recent of {} this session", contributing_routes.len())
    } else {
        format!("Matched in {} routes", contributing_routes.len().max(1))
    };

    SurfacingReason {
        headline,
        routes: route_strings,
        graph_path: graph_path.clone(),
        anchor_terms_hit,
        recency_boost,
    }
}

fn route_label(route: Route, graph_path: &Option<Vec<PathStep>>) -> String {
    match route {
        Route::Chunk => "Chunk".to_string(),
        Route::Vector => "vector".to_string(),
        Route::Keyword => "keyword".to_string(),
        Route::Temporal => "temporal".to_string(),
        Route::Entity => "entity".to_string(),
        Route::Graph => match graph_path.as_ref().filter(|p| !p.is_empty()) {
            Some(path) => format!(
                "graph({}-hop via {}:{})",
                path.len(),
                edge_label(&path.last().unwrap().edge),
                path.last().unwrap().to_label
            ),
            None => "graph".to_string(),
        },
    }
}

fn edge_label(edge: &crate::graph::schema::GraphEdgeType) -> String {
    format!("{edge:?}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_runtime::query_plan::PlannerIntent;
    use crate::context_runtime::retrieval_routes::{RouteBranch, RouteHit, RouteSignals};

    fn hit(memory_id: &str, score: f32, branch: RouteBranch) -> RouteHit {
        RouteHit {
            memory_id: memory_id.to_string(),
            score,
            signals: RouteSignals {
                branch,
                confidence: score,
                search_result: None,
            },
            graph_path: None,
        }
    }

    fn hit_with_embedding_reason(
        memory_id: &str,
        score: f32,
        branch: RouteBranch,
        label: &str,
    ) -> RouteHit {
        let mut search_result = crate::storage::SearchResult {
            id: memory_id.to_string(),
            score,
            ..Default::default()
        };
        search_result.embedding_reason_labels = vec![label.to_string()];
        RouteHit {
            memory_id: memory_id.to_string(),
            score,
            signals: RouteSignals {
                branch,
                confidence: score,
                search_result: Some(search_result),
            },
            graph_path: None,
        }
    }

    fn dummy_plan() -> QueryPlan {
        QueryPlan {
            raw: "test".to_string(),
            intent: PlannerIntent::Lookup,
            target_project: None,
            target_topics: Vec::new(),
            target_entities: Vec::new(),
            time_window: None,
            needed_context: Default::default(),
            retrieval_routes: vec![Route::Vector, Route::Keyword],
            graph_expansion: crate::context_runtime::query_plan::GraphExpansion {
                max_hops: 1,
                seed_kinds: Vec::new(),
                allowed_edges: Vec::new(),
            },
            budget_tokens: 1200,
        }
    }

    #[test]
    fn fuse_combines_weighted_route_scores() {
        let plan = dummy_plan();
        let weights = FusionWeights::default();
        let hits = vec![
            RouteHits {
                route: Route::Vector,
                hits: vec![
                    hit("a", 0.9, RouteBranch::Semantic),
                    hit("b", 0.6, RouteBranch::Semantic),
                ],
                elapsed_ms: 1,
            },
            RouteHits {
                route: Route::Keyword,
                hits: vec![hit("a", 0.4, RouteBranch::Keyword)],
                elapsed_ms: 1,
            },
        ];
        let fused = fuse(&plan, hits, &weights);
        assert_eq!(fused[0].memory_id, "a");
        assert!(fused[0].score > fused[1].score);
        assert_eq!(fused[0].contributing_routes.len(), 2);
        assert!(fused[0]
            .surfacing_reason
            .routes
            .contains(&"vector".to_string()));
        assert!(fused[0]
            .surfacing_reason
            .routes
            .contains(&"keyword".to_string()));
    }

    #[test]
    fn fuse_orders_equal_scores_by_memory_id() {
        let plan = dummy_plan();
        let ids = ["e", "c", "a", "d", "b", "f", "h", "g"];
        let hits = vec![RouteHits {
            route: Route::Vector,
            hits: ids
                .iter()
                .map(|id| hit(id, 0.5, RouteBranch::Semantic))
                .collect(),
            elapsed_ms: 1,
        }];
        let fused = fuse(&plan, hits, &FusionWeights::default());
        assert_eq!(
            fused
                .iter()
                .map(|hit| hit.memory_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c", "d", "e", "f", "g", "h"]
        );
    }

    #[test]
    fn fuse_orders_equal_scores_newest_first_then_by_id() {
        let plan = dummy_plan();
        let dated = |id: &str, timestamp: i64| {
            let mut hit = hit_with_embedding_reason(id, 0.5, RouteBranch::Semantic, "x");
            if let Some(result) = hit.signals.search_result.as_mut() {
                result.timestamp = timestamp;
            }
            hit
        };
        let hits = vec![RouteHits {
            route: Route::Vector,
            hits: vec![dated("b", 100), dated("c", 300), dated("a", 100)],
            elapsed_ms: 1,
        }];
        let fused = fuse(&plan, hits, &FusionWeights::default());
        assert_eq!(
            fused
                .iter()
                .map(|hit| hit.memory_id.as_str())
                .collect::<Vec<_>>(),
            vec!["c", "a", "b"]
        );
    }

    #[test]
    fn fuse_empty_input_returns_empty() {
        let plan = dummy_plan();
        let fused = fuse(&plan, Vec::new(), &FusionWeights::default());
        assert!(fused.is_empty());
    }

    #[test]
    fn fuse_carries_embedding_provenance_labels_into_surfacing_reason() {
        let plan = dummy_plan();
        let weights = FusionWeights::default();
        let hits = vec![RouteHits {
            route: Route::Vector,
            hits: vec![hit_with_embedding_reason(
                "stale",
                0.9,
                RouteBranch::Semantic,
                "embedding:primary:stale_source_text",
            )],
            elapsed_ms: 1,
        }];

        let fused = fuse(&plan, hits, &weights);

        assert!(fused[0]
            .surfacing_reason
            .routes
            .contains(&"embedding:primary:stale_source_text".to_string()));
    }

    #[test]
    fn for_intent_debug_boosts_graph_and_drops_vector() {
        let w = FusionWeights::for_intent(PlannerIntent::Debug);
        assert!((w.graph - 0.20).abs() < f32::EPSILON);
        assert!((w.vector - 0.35).abs() < f32::EPSILON);
    }

    /// Small seeded generator, so the property tests need no extra crate
    /// and every failure reproduces from its case number.
    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 33
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    /// Route hits shaped like the real routes: each route lists a memory at
    /// most once per branch, the vector route has two branches (semantic and
    /// snippet) that can both find the same memory, scores come from a few
    /// values so ties are common, and a few fall outside 0..1.
    fn random_route_hits(rng: &mut Lcg) -> Vec<RouteHits> {
        const SCORES: [f32; 7] = [-0.1, 0.2, 0.4, 0.4, 0.8, 1.0, 1.3];
        let mut groups = Vec::new();
        for (route, branches) in [
            (
                Route::Vector,
                &[RouteBranch::Semantic, RouteBranch::Snippet][..],
            ),
            (Route::Keyword, &[RouteBranch::Keyword][..]),
            (Route::Temporal, &[RouteBranch::Temporal][..]),
            (Route::Chunk, &[RouteBranch::Chunk][..]),
        ] {
            let mut hits = Vec::new();
            for branch in branches {
                for id in 0..12 {
                    if rng.below(3) == 0 {
                        let score = SCORES[rng.below(SCORES.len() as u64) as usize];
                        hits.push(hit(&format!("m{id:02}"), score, *branch));
                    }
                }
            }
            groups.push(RouteHits {
                route,
                hits,
                elapsed_ms: 1,
            });
        }
        groups
    }

    /// The most a memory can score from the routes that found it. The vector
    /// route counts once per branch (semantic and snippet), so it can count
    /// twice; see `both_vector_branches_add_to_a_memorys_score`.
    fn score_bound(weights: &FusionWeights, routes: &[Route]) -> f32 {
        routes
            .iter()
            .map(|route| {
                let branches = if *route == Route::Vector { 2.0 } else { 1.0 };
                branches * weight_for(weights, *route)
            })
            .sum()
    }

    #[test]
    fn fusion_holds_its_invariants_on_random_route_hits() {
        let plan = dummy_plan();
        let weights = FusionWeights::default();
        for case in 0..500u64 {
            let mut rng = Lcg(case);
            let groups = random_route_hits(&mut rng);
            let input_ids = groups
                .iter()
                .flat_map(|group| group.hits.iter().map(|hit| hit.memory_id.clone()))
                .collect::<std::collections::BTreeSet<_>>();

            let fused = fuse(&plan, groups.clone(), &weights);
            let again = fuse(&plan, groups.clone(), &weights);
            let ids = fused
                .iter()
                .map(|hit| hit.memory_id.clone())
                .collect::<Vec<_>>();

            // Same input, same output.
            assert_eq!(
                ids,
                again
                    .iter()
                    .map(|hit| hit.memory_id.clone())
                    .collect::<Vec<_>>(),
                "case {case}"
            );
            // Every input memory once, best score first.
            assert_eq!(
                ids.len(),
                input_ids.len().min(MAX_FUSED_HITS),
                "case {case}"
            );
            assert_eq!(
                ids.iter()
                    .cloned()
                    .collect::<std::collections::BTreeSet<_>>(),
                input_ids,
                "case {case}"
            );
            assert!(
                fused.windows(2).all(|pair| pair[0].score >= pair[1].score),
                "case {case}"
            );
            // Each route counts once per branch: no score above the weights
            // of the routes that found the memory.
            for hit in &fused {
                assert!(
                    hit.score <= score_bound(&weights, &hit.contributing_routes) + 1e-6,
                    "case {case}: {} scored {} from {:?}",
                    hit.memory_id,
                    hit.score,
                    hit.contributing_routes
                );
            }

            // The order of hits inside a route does not matter.
            let mut shuffled = groups.clone();
            for group in &mut shuffled {
                for i in (1..group.hits.len()).rev() {
                    let j = rng.below(i as u64 + 1) as usize;
                    group.hits.swap(i, j);
                }
            }
            let reordered = fuse(&plan, shuffled, &weights);
            assert_eq!(
                ids,
                reordered
                    .iter()
                    .map(|hit| hit.memory_id.clone())
                    .collect::<Vec<_>>(),
                "case {case}"
            );
        }
    }

    #[test]
    fn both_vector_branches_add_to_a_memorys_score() {
        // Deliberate, and measured: counting the vector route once per memory
        // lost Recall@5 on two of three personas (1.000 to 0.955, 0.900 to
        // 0.850) and put real queries under the no-match bar
        // (docs/evidence/W03/property-tests-cloud.md). Change it only with
        // the gate and the score bars recalibrated together.
        let plan = dummy_plan();
        let weights = FusionWeights::default();
        let fused = fuse(
            &plan,
            vec![RouteHits {
                route: Route::Vector,
                hits: vec![
                    hit("both", 0.8, RouteBranch::Semantic),
                    hit("both", 0.6, RouteBranch::Snippet),
                    hit("one", 0.8, RouteBranch::Semantic),
                ],
                elapsed_ms: 1,
            }],
            &weights,
        );
        let score = |id: &str| fused.iter().find(|hit| hit.memory_id == id).unwrap().score;
        assert!((score("both") - (0.8 + 0.6) * weights.vector).abs() < 1e-6);
        assert!((score("one") - 0.8 * weights.vector).abs() < 1e-6);
    }
}
