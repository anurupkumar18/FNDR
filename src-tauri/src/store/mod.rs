//! Storage module

mod schema;
mod simple_store;

pub use schema::{AppCount, MemoryRecord, SearchResult, Stats};
pub use simple_store::Store;
