//! Graph-aware reranking skeleton for the Agentic Graph RAG rollout.

use crate::graph::graph_index::GraphIndex;
use crate::graph::schema::GraphEdgeType;

#[derive(Debug, Clone)]
pub struct FusedHit {
    pub memory_id: String,
    pub score: f32,
}

#[derive(Debug, Clone, Default)]
pub struct GraphExpansion {
    pub max_hops: u8,
    pub allowed_edges: Vec<GraphEdgeType>,
}

pub fn rerank_with_graph_signals(
    _hits: &mut [FusedHit],
    _index: &GraphIndex,
    _plan: &GraphExpansion,
) {
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn rerank_boosts_hits_with_graph_support() {
        let _ = rerank_with_graph_signals;
    }
}
