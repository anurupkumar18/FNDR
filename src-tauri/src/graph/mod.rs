//! Top-level insight graph module.

pub mod community;
pub mod edges;
pub mod entities;
pub mod graph_index;
pub mod graph_rerank;
pub mod graph_store;
pub mod pathfinding;
pub mod schema;
pub mod traversal;

mod legacy;

pub use legacy::{compress_node_label, GraphStore, MemoryCard, MemoryReconstruction};
