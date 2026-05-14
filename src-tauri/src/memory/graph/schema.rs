//! Typed insight graph model (distinct from legacy `storage::schema::GraphNode` rows).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;
use uuid::Uuid;

/// High-level entity kinds extracted from finalized memory / insight fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "PascalCase")]
pub enum GraphNodeType {
    Project,
    Memory,
    Concept,
    Decision,
    File,
    Error,
    Tool,
    Person,
    Url,
    Session,
    Task,
}

/// Semantic edge kinds for the insight graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "PascalCase")]
pub enum GraphEdgeType {
    DependsOn,
    Contains,
    Imports,
    Extends,
    Implements,
    PartOf,
    Supports,
    Contradicts,
    Supersedes,
    Refines,
    Questions,
    Resolves,
    Causes,
    Prevents,
    TriggeredBy,
    FixedBy,
    BrokeBy,
    PrecededBy,
    FollowedBy,
    SimilarTo,
    MentionedIn,
    UsedIn,
    CreatedBy,
    AppliesTo,
}

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
