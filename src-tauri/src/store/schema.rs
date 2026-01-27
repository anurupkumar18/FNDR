//! Database schema for memory records

use serde::{Deserialize, Serialize};

/// A single memory record stored in the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    /// Unique identifier
    pub id: String,
    /// Unix timestamp in milliseconds
    pub timestamp: i64,
    /// Day bucket for grouping (YYYY-MM-DD)
    pub day_bucket: String,
    /// Application name
    pub app_name: String,
    /// Window title
    pub window_title: String,
    /// Extracted text content
    pub text: String,
    /// Concise summary
    pub snippet: String,
    /// Embedding vector
    pub embedding: Vec<f32>,
}

/// Search result returned to the UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub timestamp: i64,
    pub app_name: String,
    pub window_title: String,
    pub text: String,
    pub snippet: String,
    pub score: f32,
}

/// Statistics about stored data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub total_records: usize,
    pub total_days: usize,
    pub apps: Vec<AppCount>,
    pub today_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppCount {
    pub name: String,
    pub count: usize,
}
