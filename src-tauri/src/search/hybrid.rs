//! Hybrid search combining semantic and keyword search.

use crate::capture::text_cleanup;
use crate::embed::Embedder;
use crate::store::{SearchResult, Store};
use std::collections::{HashMap, HashSet};

/// Reciprocal Rank Fusion constant.
const RRF_K: f32 = 60.0;

/// Hybrid searcher combining semantic and keyword results.
pub struct HybridSearcher;

impl HybridSearcher {
    /// Perform hybrid search with RRF fusion + multi-factor reranking.
    pub async fn search(
        store: &Store,
        embedder: &Embedder,
        query: &str,
        limit: usize,
        time_filter: Option<&str>,
        app_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        let query_embedding = embedder.embed_batch(&[query.to_string()])?;
        let query_embedding = query_embedding.into_iter().next().unwrap_or_default();

        let semantic_results = store
            .vector_search(&query_embedding, limit * 3, time_filter, app_filter)
            .await?;

        let keyword_results = store
            .keyword_search(query, limit * 3, time_filter, app_filter)
            .await?;

        let fused = Self::rrf_fusion(&semantic_results, &keyword_results);
        let reranked = Self::rerank(query, fused, limit);

        Ok(reranked)
    }

    /// Merge semantic + keyword candidates, then rerank with the standard policy.
    pub fn fuse_and_rerank(
        query: &str,
        semantic: &[SearchResult],
        keyword: &[SearchResult],
        limit: usize,
    ) -> Vec<SearchResult> {
        let fused = Self::rrf_fusion(semantic, keyword);
        Self::rerank(query, fused, limit)
    }

    /// Reciprocal rank fusion to merge retrieval channels.
    pub fn rrf_fusion(semantic: &[SearchResult], keyword: &[SearchResult]) -> Vec<SearchResult> {
        let mut scores: HashMap<String, (f32, SearchResult)> = HashMap::new();

        for (rank, result) in semantic.iter().enumerate() {
            let rrf_score = 1.0 / (RRF_K + rank as f32 + 1.0);
            scores
                .entry(result.id.clone())
                .and_modify(|(s, existing)| {
                    *s += rrf_score;
                    if result.score > existing.score {
                        *existing = result.clone();
                    }
                })
                .or_insert((rrf_score, result.clone()));
        }

        for (rank, result) in keyword.iter().enumerate() {
            let rrf_score = 1.0 / (RRF_K + rank as f32 + 1.0);
            let keyword_boost = 1.18;
            scores
                .entry(result.id.clone())
                .and_modify(|(s, existing)| {
                    *s += rrf_score * keyword_boost;
                    if result.score > existing.score {
                        *existing = result.clone();
                    }
                })
                .or_insert((rrf_score * keyword_boost, result.clone()));
        }

        let mut results: Vec<SearchResult> = scores
            .into_iter()
            .map(|(_, (score, mut result))| {
                result.score = score;
                result
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results
    }

    pub fn rerank(query: &str, mut candidates: Vec<SearchResult>, limit: usize) -> Vec<SearchResult> {
        if candidates.is_empty() {
            return Vec::new();
        }

        let query_tokens = tokenize(query);
        let query_lower = query.to_lowercase();
        let code_query = is_code_query(query);

        let mut session_counts: HashMap<String, usize> = HashMap::new();
        for candidate in &candidates {
            let key = session_key(candidate);
            *session_counts.entry(key).or_insert(0) += 1;
        }

        let now = chrono::Utc::now().timestamp_millis();

        for candidate in &mut candidates {
            let mut score = candidate.score;
            let text = candidate_text(candidate);
            let text_norm = normalize_text(&text);
            let title_norm = normalize_text(&candidate.window_title);
            let snippet_norm = normalize_text(&candidate.snippet);

            // Time decay (gentle).
            let age_hours = (now - candidate.timestamp).max(0) as f32 / 3_600_000.0;
            score *= 1.0 / (1.0 + age_hours * 0.0012);

            // Penalties.
            if is_generic_title(&candidate.window_title) {
                score *= 0.72;
            }
            if candidate.ocr_confidence > 0.0 {
                score *= 0.75 + candidate.ocr_confidence.min(1.0) * 0.35;
            }
            if candidate.noise_score > 0.0 {
                score *= (1.0 - (candidate.noise_score * 0.35)).max(0.45);
            }
            if text_cleanup::symbol_ratio(&text_norm) > 0.46 {
                score *= 0.7;
            }
            if snippet_norm.split_whitespace().count() < 4 {
                score *= 0.8;
            }
            if looks_like_browser_chrome(&text_norm, &title_norm) {
                score *= 0.62;
            }
            if !code_query
                && (text_cleanup::looks_like_file_inventory(&text_norm)
                    || looks_like_json_dump(&text_norm))
            {
                score *= 0.55;
            }

            // Boosts.
            let exact_overlap = query_tokens
                .iter()
                .filter(|token| {
                    text_norm.contains(token.as_str()) || title_norm.contains(token.as_str())
                })
                .count();
            if exact_overlap > 0 {
                score *= 1.0 + (exact_overlap as f32 * 0.08).min(0.35);
            }

            let entity_overlap = named_entity_overlap(query, &text);
            if entity_overlap > 0 {
                score *= 1.0 + (entity_overlap as f32 * 0.06).min(0.18);
            }

            if let Some(url) = &candidate.url {
                let domain = extract_domain(url);
                if !domain.is_empty() && query_lower.contains(&domain) {
                    score *= 1.18;
                }
                if !domain.is_empty() && query_tokens.iter().any(|t| domain.contains(t)) {
                    score *= 1.08;
                }
            }

            if candidate.summary_source.eq_ignore_ascii_case("llm") {
                score *= 1.06;
            }

            let coherence = session_counts
                .get(&session_key(candidate))
                .copied()
                .unwrap_or(1);
            if coherence > 1 {
                score *= 1.0 + (coherence as f32 * 0.04).min(0.16);
            }

            candidate.score = score;
        }

        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        });

        // Session-level fuzzy dedup.
        let mut deduped = Vec::new();
        let mut seen_texts: Vec<String> = Vec::new();
        let mut seen_sessions: HashMap<String, usize> = HashMap::new();

        for candidate in candidates {
            let norm = normalize_text(&candidate_text(&candidate));
            if norm.is_empty() {
                continue;
            }

            let duplicate = seen_texts
                .iter()
                .any(|existing| fuzzy_sim(existing, &norm) >= 0.92);
            if duplicate {
                continue;
            }

            let sess = session_key(&candidate);
            let count = seen_sessions.entry(sess).or_insert(0);
            if *count >= 2 {
                continue;
            }

            *count += 1;
            seen_texts.push(norm);
            deduped.push(candidate);

            if deduped.len() >= limit.min(30) {
                break;
            }
        }

        deduped
    }
}

fn candidate_text(result: &SearchResult) -> String {
    if !result.clean_text.trim().is_empty() {
        result.clean_text.clone()
    } else {
        result.text.clone()
    }
}

fn session_key(result: &SearchResult) -> String {
    if !result.session_key.trim().is_empty() {
        return result.session_key.clone();
    }

    let domain = result
        .url
        .as_ref()
        .map(|url| extract_domain(url))
        .unwrap_or_default();
    format!(
        "{}:{}:{}",
        result.app_name.to_lowercase(),
        domain,
        normalize_text(&result.window_title)
    )
}

fn is_generic_title(title: &str) -> bool {
    matches!(
        normalize_text(title).as_str(),
        "new tab" | "home" | "untitled" | "dashboard" | "settings" | "preferences"
    )
}

fn looks_like_browser_chrome(text: &str, title: &str) -> bool {
    let lower = text.to_lowercase();
    let title_lower = title.to_lowercase();

    title_lower == "new tab"
        || lower.contains("new tab")
        || lower.contains("tab strip")
        || lower.contains("back forward")
        || lower.contains("home trending")
        || lower.contains("notifications")
}

fn looks_like_json_dump(text: &str) -> bool {
    (text.contains('{') && text.contains('}') && text.contains(':') && text.len() > 80)
        || text.contains("\"items\"")
        || text.contains("\"files\"")
}

fn named_entity_overlap(query: &str, text: &str) -> usize {
    let query_entities = extract_named_entities(query);
    if query_entities.is_empty() {
        return 0;
    }

    let text_lower = text.to_lowercase();
    query_entities
        .iter()
        .filter(|entity| text_lower.contains(entity.as_str()))
        .count()
}

fn extract_named_entities(text: &str) -> HashSet<String> {
    text.split_whitespace()
        .filter(|token| token.len() > 2)
        .filter_map(|token| {
            let clean = token
                .trim_matches(|ch: char| !ch.is_alphanumeric())
                .to_string();
            if clean.len() <= 2 {
                return None;
            }
            if clean.chars().next().is_some_and(|ch| ch.is_uppercase()) {
                Some(clean.to_lowercase())
            } else {
                None
            }
        })
        .collect()
}

fn normalize_text(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch.is_whitespace() {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn tokenize(value: &str) -> HashSet<String> {
    normalize_text(value)
        .split_whitespace()
        .filter(|token| token.len() > 2)
        .map(|token| token.to_string())
        .collect()
}

fn extract_domain(url: &str) -> String {
    url.split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or_default()
        .to_lowercase()
}

fn fuzzy_sim(a: &str, b: &str) -> f32 {
    let left = tokenize(a);
    let right = tokenize(b);
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }

    let inter = left.intersection(&right).count() as f32;
    let union = left.union(&right).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        inter / union
    }
}

fn is_code_query(query: &str) -> bool {
    let lower = query.to_lowercase();
    lower.contains("code")
        || lower.contains("json")
        || lower.contains("stack trace")
        || lower.contains("rust")
        || lower.contains("typescript")
        || lower.contains("error")
        || lower.contains("file")
        || lower.contains("terminal")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sr(id: &str, title: &str, text: &str, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            timestamp: 2_000_000,
            app_name: "Chrome".to_string(),
            bundle_id: None,
            window_title: title.to_string(),
            session_id: "s1".to_string(),
            text: text.to_string(),
            clean_text: text.to_string(),
            ocr_confidence: 0.9,
            ocr_block_count: 7,
            snippet: text.to_string(),
            summary_source: "llm".to_string(),
            noise_score: 0.1,
            session_key: "chrome:test".to_string(),
            score,
            screenshot_path: None,
            url: Some("https://example.com".to_string()),
        }
    }

    #[test]
    fn rerank_penalizes_generic_chrome_noise() {
        let mut noisy = sr("1", "New Tab", "New Tab Home Trending Notifications", 0.8);
        noisy.noise_score = 0.9;
        noisy.ocr_confidence = 0.4;

        let useful = sr(
            "2",
            "IPL 2026 Highlights",
            "Reviewed IPL highlights and match stats on YouTube",
            0.75,
        );

        let ranked = HybridSearcher::rerank("ipl highlights", vec![noisy, useful.clone()], 10);
        assert_eq!(ranked.first().map(|r| r.id.as_str()), Some("2"));
    }

    #[test]
    fn rerank_fuzzy_dedups_near_identical_snippets() {
        let a = sr(
            "1",
            "Title",
            "Reviewed onboarding checklist for FNDR launch",
            0.8,
        );
        let mut b = a.clone();
        b.id = "2".to_string();
        b.score = 0.79;

        let ranked = HybridSearcher::rerank("onboarding checklist", vec![a, b], 10);
        assert_eq!(ranked.len(), 1);
    }
}
