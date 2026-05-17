//! Typed insight graph records (distinct from legacy `storage::schema::GraphNode` rows).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

pub use crate::graph::edges::GraphEdgeType;
pub use crate::graph::entities::GraphNodeType;

pub const NODE_ID_FIELD: &str = "id";
pub const NODE_TYPE_FIELD: &str = "node_type";
pub const NODE_LABEL_FIELD: &str = "label";
pub const NODE_CONFIDENCE_FIELD: &str = "confidence";
pub const NODE_SOURCE_MEMORY_IDS_FIELD: &str = "source_memory_ids";
pub const NODE_EMBEDDING_FIELD: &str = "embedding";
pub const NODE_CREATED_AT_MS_FIELD: &str = "created_at_ms";
pub const NODE_UPDATED_AT_MS_FIELD: &str = "updated_at_ms";
pub const NODE_STALE_FIELD: &str = "stale";
pub const NODE_METADATA_FIELD: &str = "metadata";

pub const EDGE_ID_FIELD: &str = "id";
pub const EDGE_SOURCE_ID_FIELD: &str = "source_id";
pub const EDGE_TARGET_ID_FIELD: &str = "target_id";
pub const EDGE_TYPE_FIELD: &str = "edge_type";
pub const EDGE_CONFIDENCE_FIELD: &str = "confidence";
pub const EDGE_CONFLICT_FLAG_FIELD: &str = "conflict_flag";
pub const EDGE_CREATED_AT_MS_FIELD: &str = "created_at_ms";
pub const EDGE_METADATA_FIELD: &str = "metadata";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct GraphNode {
    pub id: Uuid,
    pub node_type: GraphNodeType,
    pub label: String,
    /// 0..1 salience of this extraction.
    pub confidence: f32,
    pub source_memory_ids: Vec<String>,
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct GraphEdge {
    pub id: Uuid,
    pub source_id: Uuid,
    pub target_id: Uuid,
    pub edge_type: GraphEdgeType,
    pub confidence: f32,
    /// When true, this edge participates in a Contradicts/Supports pair; both are kept.
    #[serde(default)]
    pub conflict_flag: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Type)]
pub struct GraphSubgraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    /// Louvain-style community id per graph node id (UI layout / grouping).
    #[serde(default)]
    pub louvain: HashMap<Uuid, usize>,
    /// Human-readable label for community 0 when non-empty.
    #[serde(default)]
    pub cluster_0_name: String,
}
