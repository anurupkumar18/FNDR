use crate::storage::SearchResult;

use super::query_processor::{normalize_text, QueryContext};

/// Share of the query's anchor terms found in a result's text, plus a small
/// bonus when the whole query appears. Search cards group by it; it no longer
/// reorders anything (VS-25 removed the coverage rerank, VS-10 moved Search
/// onto `retrieve`).
pub fn anchor_coverage_score(query_context: &QueryContext, result: &SearchResult) -> f32 {
    if query_context.anchor_terms.is_empty() {
        return 1.0;
    }

    let summary = if result.display_summary.trim().is_empty() {
        &result.snippet
    } else {
        &result.display_summary
    };
    let merged_text = normalize_text(&format!(
        "{} {} {} {} {} {}",
        result.window_title,
        summary,
        result.snippet,
        result.clean_text,
        result.extracted_entities.join(" "),
        result.url.clone().unwrap_or_default(),
    ));

    if merged_text.is_empty() {
        return 0.0;
    }

    let mut matched = 0usize;
    for term in &query_context.anchor_terms {
        let term_norm = normalize_text(term);
        if term_norm.is_empty() {
            continue;
        }
        if merged_text.contains(&term_norm) {
            matched += 1;
        }
    }

    let mut score = matched as f32 / query_context.anchor_terms.len() as f32;
    if !query_context.normalized_query.is_empty()
        && merged_text.contains(&query_context.normalized_query)
    {
        score = (score + 0.12).min(1.0);
    }
    score.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(title: &str, summary: &str) -> SearchResult {
        SearchResult {
            id: "1".to_string(),
            window_title: title.to_string(),
            snippet: summary.to_string(),
            display_summary: summary.to_string(),
            clean_text: summary.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn coverage_counts_the_query_words_a_result_contains() {
        let query = QueryContext::from_query("cricket highlights");
        let full = anchor_coverage_score(&query, &result("IPL", "Watched cricket highlights"));
        let none = anchor_coverage_score(&query, &result("Rust", "Debugged compiler issues"));
        assert!(full > 0.99);
        assert_eq!(none, 0.0);
    }
}
