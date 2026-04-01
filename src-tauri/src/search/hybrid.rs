//! Hybrid search combining semantic and keyword search

use crate::embed::Embedder;
use crate::store::{SearchResult, Store};
use std::collections::HashMap;

/// Reciprocal Rank Fusion constant
const RRF_K: f32 = 60.0;

/// Hybrid searcher combining semantic and keyword results
pub struct HybridSearcher;

impl HybridSearcher {
    /// Perform hybrid search with RRF fusion
    pub async fn search(
        store: &Store,
        embedder: &Embedder,
        query: &str,
        limit: usize,
        time_filter: Option<&str>,
        app_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        // Get semantic results
        let query_embedding = embedder.embed_batch(&[query.to_string()])?;
        let query_embedding = query_embedding.into_iter().next().unwrap_or_default();

        let semantic_results =
            store.vector_search(&query_embedding, limit * 2, time_filter, app_filter).await?;

        // Get keyword results (with same filters for consistency)
        let keyword_results = store.keyword_search(query, limit * 2, time_filter, app_filter).await?;

        // RRF Fusion
        let fused = Self::rrf_fusion(&semantic_results, &keyword_results, limit);

        Ok(fused)
    }

    /// Reciprocal Rank Fusion
    fn rrf_fusion(
        semantic: &[SearchResult],
        keyword: &[SearchResult],
        limit: usize,
    ) -> Vec<SearchResult> {
        let mut scores: HashMap<String, (f32, Option<SearchResult>)> = HashMap::new();

        // Score semantic results
        for (rank, result) in semantic.iter().enumerate() {
            let rrf_score = 1.0 / (RRF_K + rank as f32 + 1.0);
            scores
                .entry(result.id.clone())
                .and_modify(|(s, _)| *s += rrf_score)
                .or_insert((rrf_score, Some(result.clone())));
        }

        // Score keyword results (with slight boost for exact matches)
        for (rank, result) in keyword.iter().enumerate() {
            let rrf_score = 1.0 / (RRF_K + rank as f32 + 1.0);
            let boost = 1.2; // Boost keyword matches
            scores
                .entry(result.id.clone())
                .and_modify(|(s, _)| *s += rrf_score * boost)
                .or_insert((rrf_score * boost, Some(result.clone())));
        }

        // Sort by fused score
        let mut results: Vec<(f32, SearchResult)> = scores
            .into_iter()
            .filter_map(|(_, (score, result))| result.map(|r| (score, r)))
            .collect();

        results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Apply time decay
        let now = chrono::Utc::now().timestamp_millis();
        for (score, result) in &mut results {
            let age_hours = (now - result.timestamp) as f32 / 3600000.0;
            let decay = 1.0 / (1.0 + age_hours * 0.001);
            *score *= decay;
            result.score = *score;
        }

        // Re-sort after decay
        results.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Apply relevance threshold: only return results with score >= 50% of top result
        if let Some((max_score, _)) = results.first() {
            let threshold = max_score * 0.5;
            results.retain(|(score, _)| *score >= threshold);
        }

        // Cap at reasonable limit (20 max) to prevent UI overload
        let max_results = limit.min(20);

        // Deduplicate similar results
        let mut seen_texts: Vec<String> = Vec::new();
        let mut deduped = Vec::new();

        for (_, result) in results {
            let text_preview: String = result.text.chars().take(100).collect();
            let text_hash = text_preview.as_str();
            if !seen_texts.iter().any(|t| t == text_hash) {
                seen_texts.push(text_hash.to_string());
                deduped.push(result);
            }

            if deduped.len() >= max_results {
                break;
            }
        }

        deduped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_fusion_combines_results() {
        let semantic = vec![
            SearchResult {
                id: "1".to_string(),
                timestamp: 1000,
                app_name: "App".to_string(),
                bundle_id: None,
                window_title: "Title".to_string(),
                session_id: "session".to_string(),
                text: "Hello world".to_string(),
                snippet: "Hello...".to_string(),
                score: 0.9,
                screenshot_path: None,
                url: None,
            },
            SearchResult {
                id: "2".to_string(),
                timestamp: 2000,
                app_name: "App".to_string(),
                bundle_id: None,
                window_title: "Title".to_string(),
                session_id: "session".to_string(),
                text: "Goodbye world".to_string(),
                snippet: "Goodbye...".to_string(),
                score: 0.8,
                screenshot_path: None,
                url: None,
            },
        ];

        let keyword = vec![SearchResult {
            id: "2".to_string(),
            timestamp: 2000,
            app_name: "App".to_string(),
            bundle_id: None,
            window_title: "Title".to_string(),
            session_id: "session".to_string(),
            text: "Goodbye world".to_string(),
            snippet: "Goodbye...".to_string(),
            score: 1.0,
            screenshot_path: None,
            url: None,
        }];

        let fused = HybridSearcher::rrf_fusion(&semantic, &keyword, 1000);

        // Result "2" should be ranked higher due to appearing in both
        assert!(!fused.is_empty());
        // ID "2" appears in both, should have higher combined score
    }
}
