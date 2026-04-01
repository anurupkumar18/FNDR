//! Storage module

mod lance_store;
mod schema;
mod simple_store; // kept for reference, no longer the active store

pub use lance_store::Store;
pub use schema::{AppCount, MemoryRecord, SearchResult, Stats};
