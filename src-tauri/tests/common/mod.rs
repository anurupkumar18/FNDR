//! Shared helpers for integration tests.

use fndr_lib::config::SearchConfig;

/// Search tuning for tests that check *what* search returns, not how fast.
///
/// Production budgets are tuned for a warm release build. A cold debug build
/// on a shared CI runner misses them, which silently drops whole retrieval
/// branches (semantic, snippet, keyword) and makes result-quality tests fail
/// for timing reasons. This keeps every budget at the largest value
/// `SearchConfig::normalized` allows.
#[allow(dead_code)]
pub fn ci_safe_search_config() -> SearchConfig {
    SearchConfig {
        semantic_timeout_ms: 10_000,
        snippet_timeout_ms: 10_000,
        keyword_timeout_ms: 10_000,
        keyword_variant_timeout_ms: 5_000,
        ..SearchConfig::default()
    }
    .normalized()
}
