//! Storage module

mod schema;
mod simple_store;

pub use schema::{MemoryRecord, SearchResult, Stats, AppCount};
pub use simple_store::Store;
