//! In-memory graph index used by retrieval expansion.

use crate::graph::schema::{GraphEdge, GraphEdgeType, GraphNode};
use crate::graph::traversal;
use std::collections::{HashMap, HashSet, VecDeque};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct GraphIndex {
    pub adjacency: HashMap<Uuid, Vec<(Uuid, GraphEdgeType, f32)>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NeighborHit {
    pub id: Uuid,
    pub via_edge: GraphEdgeType,
    pub confidence: f32,
    pub hops: usize,
}

impl GraphIndex {
    pub fn build(nodes: &[GraphNode], edges: &[GraphEdge]) -> Self {
        Self {
            adjacency: traversal::undirected_adjacency(nodes, edges),
        }
    }

    pub fn neighbors(
        &self,
        id: Uuid,
        allowed: &[GraphEdgeType],
        max_hops: usize,
    ) -> Vec<NeighborHit> {
        if max_hops == 0 {
            return Vec::new();
        }

        let allowed: HashSet<GraphEdgeType> = allowed.iter().copied().collect();
        let allow_all = allowed.is_empty();
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut queue = VecDeque::new();
        seen.insert(id);
        queue.push_back((id, 0usize, 1.0f32));

        while let Some((current, hops, path_confidence)) = queue.pop_front() {
            if hops >= max_hops {
                continue;
            }
            let Some(neighbors) = self.adjacency.get(&current) else {
                continue;
            };
            for &(next, edge_type, edge_confidence) in neighbors {
                if !allow_all && !allowed.contains(&edge_type) {
                    continue;
                }
                if !seen.insert(next) {
                    continue;
                }
                let confidence = path_confidence.min(edge_confidence);
                let next_hops = hops + 1;
                out.push(NeighborHit {
                    id: next,
                    via_edge: edge_type,
                    confidence,
                    hops: next_hops,
                });
                queue.push_back((next, next_hops, confidence));
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::schema::{GraphEdge, GraphNodeType};
    use chrono::Utc;

    fn node(id: u8) -> GraphNode {
        GraphNode {
            id: Uuid::from_u128(id as u128),
            node_type: GraphNodeType::Concept,
            label: format!("node {id}"),
            confidence: 0.9,
            source_memory_ids: vec!["m1".into()],
            embedding: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            stale: false,
            metadata: serde_json::json!({}),
        }
    }

    fn edge(source: u8, target: u8, edge_type: GraphEdgeType) -> GraphEdge {
        GraphEdge {
            id: Uuid::new_v4(),
            source_id: Uuid::from_u128(source as u128),
            target_id: Uuid::from_u128(target as u128),
            edge_type,
            confidence: 0.9,
            conflict_flag: false,
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        }
    }

    #[test]
    fn neighbors_respect_allowed_edges_and_hops() {
        let index = GraphIndex::build(
            &[node(1), node(2), node(3)],
            &[
                edge(1, 2, GraphEdgeType::BelongsToProject),
                edge(2, 3, GraphEdgeType::SimilarTo),
            ],
        );

        let hits = index.neighbors(Uuid::from_u128(1), &[GraphEdgeType::BelongsToProject], 2);

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, Uuid::from_u128(2));
    }
}
