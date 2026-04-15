use crate::inference::{InferenceEngine, MemoryCardDraft};
use crate::store::SearchResult;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tokio::time::{timeout, Duration};

const MAX_GROUP_SNIPPETS: usize = 6;
const GROUPING_TIMEOUT: Duration = Duration::from_millis(350);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCard {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub action: String,
    pub context: Vec<String>,
    pub timestamp: i64,
    pub app_name: String,
    pub window_title: String,
    pub url: Option<String>,
    pub score: f32,
    pub source_count: usize,
    pub raw_snippets: Vec<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub confidence: f32,
}

#[derive(Debug, Clone)]
struct SessionGroup {
    members: Vec<SearchResult>,
}

pub struct MemoryCardSynthesizer;

impl MemoryCardSynthesizer {
    pub async fn from_results(
        inference: Option<&InferenceEngine>,
        query: &str,
        results: &[SearchResult],
    ) -> Vec<MemoryCard> {
        Self::from_results_with_policy(inference, query, results, 6, 3, Duration::from_millis(1500))
            .await
    }

    pub async fn from_results_with_policy(
        inference: Option<&InferenceEngine>,
        query: &str,
        results: &[SearchResult],
        max_groups: usize,
        max_llm_groups: usize,
        llm_timeout: Duration,
    ) -> Vec<MemoryCard> {
        if results.is_empty() {
            return Vec::new();
        }

        tracing::info!("search_memory_cards:grouping:start");
        let groups = match timeout(
            GROUPING_TIMEOUT,
            tokio::task::spawn_blocking({
                let results = results.to_vec();
                move || group_results(&results, max_groups)
            }),
        )
        .await
        {
            Ok(Ok(groups)) => groups,
            Ok(Err(err)) => {
                tracing::warn!("search_memory_cards:grouping:join_error err={}", err);
                results
                    .iter()
                    .take(max_groups)
                    .cloned()
                    .map(|r| SessionGroup { members: vec![r] })
                    .collect()
            }
            Err(_) => {
                tracing::warn!(
                    timeout_ms = GROUPING_TIMEOUT.as_millis(),
                    "search_memory_cards:grouping:timeout"
                );
                results
                    .iter()
                    .take(max_groups)
                    .cloned()
                    .map(|r| SessionGroup { members: vec![r] })
                    .collect()
            }
        };
        tracing::info!(groups = groups.len(), "search_memory_cards:grouping:done");
        let mut cards = Vec::with_capacity(groups.len());

        for (index, group) in groups.into_iter().enumerate() {
            let snippets = collect_group_snippets(&group.members);
            let grounded_snippets = collect_grounded_snippets(&group.members);
            let anchor = select_anchor(&group.members);
            let evidence_ids = collect_evidence_ids(&group.members, 4);

            let mut draft = None;
            if index < max_llm_groups {
                if let Some(engine) = inference {
                    tracing::info!(group_idx = index, "search_memory_cards:synthesis_llm:start");
                    draft = match timeout(
                        llm_timeout,
                        engine.synthesize_memory_card(
                            query,
                            &anchor.app_name,
                            &anchor.window_title,
                            &grounded_snippets,
                        ),
                    )
                    .await
                    {
                        Ok(value) => value,
                        Err(_) => {
                            tracing::warn!(
                                group_idx = index,
                                timeout_ms = llm_timeout.as_millis(),
                                "search_memory_cards:synthesis_llm:timeout"
                            );
                            None
                        }
                    };
                    tracing::info!(
                        group_idx = index,
                        used_llm = draft.is_some(),
                        "search_memory_cards:synthesis_llm:done"
                    );
                }
            }

            let (title, mut summary, action, mut context) = match draft.as_ref().and_then(|d| {
                validate_draft(d, query, &snippets, &anchor.app_name, &anchor.window_title)
            }) {
                Some(valid) => valid,
                None => deterministic_fallback(query, &anchor, &snippets),
            };

            let score = aggregate_score(&group.members);
            let source_count = group.members.len();
            let confidence = grounding_confidence(query, &summary, score, &snippets);
            if confidence < 0.42 && !summary.to_lowercase().starts_with("low confidence:") {
                summary = format!("Low confidence: {}", summary);
            }
            append_source_context(&mut context, &evidence_ids);

            cards.push(MemoryCard {
                id: anchor.id.clone(),
                title,
                summary,
                action,
                context,
                timestamp: anchor.timestamp,
                app_name: anchor.app_name.clone(),
                window_title: anchor.window_title.clone(),
                url: anchor.url.clone(),
                score,
                source_count,
                raw_snippets: snippets,
                evidence_ids,
                confidence,
            });
        }

        cards.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        });
        apply_story_continuity(&mut cards);

        cards
    }

    pub fn deterministic_from_results(
        query: &str,
        results: &[SearchResult],
        limit: usize,
    ) -> Vec<MemoryCard> {
        let mut cards = Vec::new();
        let capped = limit.max(1);
        for result in results.iter().take(capped) {
            cards.push(fallback_card_for_result(query, result));
        }
        apply_story_continuity(&mut cards);
        cards
    }
}

fn group_results(results: &[SearchResult], max_groups: usize) -> Vec<SessionGroup> {
    let mut sorted = results.to_vec();
    sorted.sort_by_key(|r| std::cmp::Reverse(r.timestamp));

    let mut groups: Vec<SessionGroup> = Vec::new();
    let mut key_to_group_idx: HashMap<String, usize> = HashMap::new();

    for result in sorted {
        let key = grouping_key(&result);
        if let Some(group_idx) = key_to_group_idx.get(&key).copied() {
            let anchor = &groups[group_idx].members[0];
            if should_group(anchor, &result) {
                groups[group_idx].members.push(result);
                continue;
            }
        }

        if groups.len() >= max_groups {
            continue;
        }
        let next_idx = groups.len();
        groups.push(SessionGroup {
            members: vec![result],
        });
        key_to_group_idx.insert(key, next_idx);
    }

    groups
}

fn grouping_key(result: &SearchResult) -> String {
    if !result.session_key.trim().is_empty() {
        return result.session_key.clone();
    }

    let domain = extract_domain(result.url.as_deref()).unwrap_or_default();
    let title = normalize_for_dedup(&result.window_title);
    format!("{}:{}:{}", result.app_name.to_lowercase(), domain, title)
}

fn should_group(a: &SearchResult, b: &SearchResult) -> bool {
    if a.app_name != b.app_name {
        return false;
    }

    let within_time_window = (a.timestamp - b.timestamp).abs() <= 5 * 60 * 1000;
    if !within_time_window {
        return false;
    }

    if !a.session_key.is_empty() && a.session_key == b.session_key {
        return true;
    }

    let title_sim = token_overlap(&a.window_title, &b.window_title);
    let text_sim = token_overlap(&merged_text(a), &merged_text(b));
    let domain_match = extract_domain(a.url.as_deref()) == extract_domain(b.url.as_deref());

    domain_match || title_sim >= 0.55 || text_sim >= 0.40
}

fn merged_text(result: &SearchResult) -> String {
    if !result.clean_text.trim().is_empty() {
        result.clean_text.clone()
    } else {
        result.text.clone()
    }
}

fn collect_group_snippets(results: &[SearchResult]) -> Vec<String> {
    let mut snippets = Vec::new();
    let mut seen = HashSet::new();

    for result in results {
        let primary = if !result.snippet.trim().is_empty() {
            result.snippet.trim().to_string()
        } else {
            merged_text(result)
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .to_string()
        };

        if primary.is_empty() {
            continue;
        }

        let normalized = normalize_for_dedup(&primary);
        if seen.insert(normalized) {
            snippets.push(primary);
        }

        if snippets.len() >= MAX_GROUP_SNIPPETS {
            break;
        }
    }

    snippets
}

fn collect_grounded_snippets(results: &[SearchResult]) -> Vec<String> {
    let mut snippets = Vec::new();
    for result in results.iter().take(MAX_GROUP_SNIPPETS) {
        let snippet = if !result.snippet.trim().is_empty() {
            result.snippet.trim().to_string()
        } else {
            merged_text(result)
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .to_string()
        };

        if snippet.is_empty() {
            continue;
        }

        snippets.push(format!("[{}] {}", short_id(&result.id), snippet));
    }
    snippets
}

fn select_anchor(results: &[SearchResult]) -> SearchResult {
    results
        .iter()
        .max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.timestamp.cmp(&b.timestamp))
        })
        .cloned()
        .unwrap_or_else(|| results[0].clone())
}

fn aggregate_score(results: &[SearchResult]) -> f32 {
    let mut weighted = 0.0f32;
    let mut total_w = 0.0f32;
    for (idx, result) in results.iter().enumerate() {
        let weight = 1.0 / (idx as f32 + 1.0);
        weighted += result.score * weight;
        total_w += weight;
    }

    let avg = if total_w > 0.0 {
        weighted / total_w
    } else {
        0.0
    };
    (avg + (results.len() as f32 * 0.04)).min(1.0)
}

fn collect_evidence_ids(results: &[SearchResult], max_ids: usize) -> Vec<String> {
    let mut ranked = results.to_vec();
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.timestamp.cmp(&a.timestamp))
    });

    ranked
        .iter()
        .take(max_ids.max(1))
        .map(|result| result.id.clone())
        .collect()
}

fn short_id(value: &str) -> String {
    value.chars().take(8).collect::<String>()
}

fn append_source_context(context: &mut Vec<String>, evidence_ids: &[String]) {
    if evidence_ids.is_empty() {
        return;
    }

    let source_line = format!(
        "Sources: {}",
        evidence_ids
            .iter()
            .map(|id| short_id(id))
            .collect::<Vec<_>>()
            .join(", ")
    );

    if !context.iter().any(|entry| entry.starts_with("Sources:")) {
        context.push(source_line);
    }

    context.truncate(5);
}

fn grounding_confidence(query: &str, summary: &str, base_score: f32, snippets: &[String]) -> f32 {
    if snippets.is_empty() {
        return 0.0;
    }

    let summary_terms = tokenize(summary)
        .into_iter()
        .filter(|term| !grounding_stop_words().contains(term.as_str()))
        .collect::<HashSet<_>>();
    if summary_terms.is_empty() {
        return 0.0;
    }

    let snippet_blob = normalize_for_dedup(&snippets.join(" "));
    let supported = summary_terms
        .iter()
        .filter(|term| snippet_blob.contains(term.as_str()))
        .count();
    let support_ratio = supported as f32 / summary_terms.len().max(1) as f32;

    let query_terms = tokenize(query)
        .into_iter()
        .filter(|term| !grounding_stop_words().contains(term.as_str()))
        .collect::<HashSet<_>>();
    let query_coverage = if query_terms.is_empty() {
        0.5
    } else {
        query_terms
            .iter()
            .filter(|term| snippet_blob.contains(term.as_str()))
            .count() as f32
            / query_terms.len() as f32
    };

    (base_score.clamp(0.0, 1.0) * 0.45 + support_ratio * 0.35 + query_coverage * 0.20)
        .clamp(0.0, 1.0)
}

fn grounding_stop_words() -> HashSet<&'static str> {
    [
        "the", "and", "for", "with", "that", "from", "this", "have", "into", "while", "open",
        "page", "about", "using", "user", "you", "your", "their", "then", "was", "what", "is",
        "are",
    ]
    .into_iter()
    .collect()
}

fn validate_draft(
    draft: &MemoryCardDraft,
    query: &str,
    snippets: &[String],
    app_name: &str,
    window_title: &str,
) -> Option<(String, String, String, Vec<String>)> {
    let title = sanitize_title(&draft.title, app_name, window_title);
    let summary = sanitize_summary(&draft.summary)?;
    if grounding_confidence(query, &summary, 0.6, snippets) < 0.38 {
        return None;
    }
    let action = sanitize_action(&draft.action);

    let mut context = draft
        .context
        .iter()
        .map(|value| normalize_sentence(value))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    context.retain(|value| !is_ui_chrome_phrase(value));
    context.dedup();
    context.truncate(4);

    if context.is_empty() {
        let fallback = truncate_words(window_title, 6);
        if fallback.is_empty() {
            context.push(app_name.to_string());
        } else {
            context.push(fallback);
        }
    }

    Some((title, summary, action, context))
}

fn deterministic_fallback(
    _query: &str,
    anchor: &SearchResult,
    snippets: &[String],
) -> (String, String, String, Vec<String>) {
    let title = sanitize_title("", &anchor.app_name, &anchor.window_title);
    let summary = build_story_summary(anchor, snippets);
    let action = build_action_summary(anchor, snippets);
    let context = build_context(anchor, snippets);

    (
        title,
        sanitize_summary(&summary).unwrap_or(summary),
        action,
        context,
    )
}

fn fallback_card_for_result(query: &str, result: &SearchResult) -> MemoryCard {
    let snippets = collect_group_snippets(std::slice::from_ref(result));
    let (title, mut summary, action, mut context) =
        deterministic_fallback(query, result, &snippets);
    let evidence_ids = vec![result.id.clone()];
    append_source_context(&mut context, &evidence_ids);
    let confidence = grounding_confidence(query, &summary, result.score, &snippets);
    if confidence < 0.42 && !summary.to_lowercase().starts_with("low confidence:") {
        summary = format!("Low confidence: {}", summary);
    }
    MemoryCard {
        id: result.id.clone(),
        title,
        summary,
        action,
        context,
        timestamp: result.timestamp,
        app_name: result.app_name.clone(),
        window_title: result.window_title.clone(),
        url: result.url.clone(),
        score: result.score,
        source_count: 1,
        raw_snippets: snippets,
        evidence_ids,
        confidence,
    }
}

fn sanitize_title(raw: &str, app_name: &str, window_title: &str) -> String {
    let candidate = normalize_sentence(raw);
    if !candidate.is_empty() && !is_generic_title(&candidate) {
        return truncate_words(&candidate, 8);
    }

    let clean_window = normalize_sentence(window_title);
    if !clean_window.is_empty() && !is_generic_title(&clean_window) {
        return truncate_words(&clean_window, 8);
    }

    format!("{} activity", app_name)
}

fn sanitize_action(raw: &str) -> String {
    let cleaned = normalize_sentence(raw);
    if cleaned.is_empty() || is_ui_chrome_phrase(&cleaned) {
        "Reviewed key details".to_string()
    } else {
        truncate_words(&cleaned, 10)
    }
}

fn sanitize_summary(raw: &str) -> Option<String> {
    let summary = normalize_sentence(raw);
    if summary.is_empty() {
        return None;
    }

    if summary.contains('\n')
        || summary.contains('*')
        || summary.contains('#')
        || summary.contains('`')
    {
        return None;
    }

    let lower = summary.to_lowercase();
    if lower.starts_with("the screen shows") || lower.starts_with("i see") {
        return None;
    }
    if is_ui_chrome_phrase(&summary) {
        return None;
    }

    let mut sentences = summary
        .replace('!', ".")
        .replace('?', ".")
        .split('.')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(normalize_sentence)
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    if sentences.is_empty() {
        return None;
    }
    sentences.truncate(2);

    let total_words = sentences
        .iter()
        .map(|sentence| sentence.split_whitespace().count())
        .sum::<usize>();
    if !(8..=36).contains(&total_words) {
        return None;
    }

    for sentence in &sentences {
        let words = sentence.split_whitespace().count();
        if !(4..=22).contains(&words) {
            return None;
        }
    }

    let rendered = sentences
        .iter()
        .map(|s| ensure_sentence_period(s))
        .collect::<Vec<_>>()
        .join(" ");

    Some(rendered)
}

fn is_ui_chrome_phrase(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.contains("new tab")
        || lower.contains("toolbar")
        || lower.contains("tab strip")
        || lower == "home"
        || lower == "trending"
}

fn is_generic_title(value: &str) -> bool {
    matches!(
        value.to_lowercase().as_str(),
        "new tab" | "home" | "untitled" | "dashboard" | "settings"
    )
}

fn ensure_sentence_period(value: &str) -> String {
    let mut out = value.trim().to_string();
    if !out.ends_with('.') {
        out.push('.');
    }
    out
}

fn build_story_summary(anchor: &SearchResult, snippets: &[String]) -> String {
    let facts = extract_story_facts(snippets);

    if facts.is_empty() {
        let domain = extract_domain(anchor.url.as_deref());
        return if let Some(dom) = domain {
            format!(
                "Reviewed {} updates on {}.",
                truncate_words(&anchor.window_title, 6),
                dom
            )
        } else {
            format!("Reviewed {}.", truncate_words(&anchor.window_title, 8))
        };
    }

    let mut summary = ensure_sentence_period(&facts[0]);
    if let Some(second) = facts.get(1) {
        summary.push_str(" Then ");
        summary.push_str(&ensure_sentence_period(second));
    }

    summary
}

fn build_action_summary(anchor: &SearchResult, snippets: &[String]) -> String {
    if let Some(first) = extract_story_facts(snippets).first() {
        return sanitize_action(&truncate_words(first, 10));
    }

    if let Some(domain) = extract_domain(anchor.url.as_deref()) {
        return format!("Followed updates on {}", domain);
    }

    format!("Reviewed {}", truncate_words(&anchor.window_title, 5))
}

fn build_context(anchor: &SearchResult, snippets: &[String]) -> Vec<String> {
    let mut context = Vec::new();
    let mut seen = HashSet::new();

    if let Some(domain) = extract_domain(anchor.url.as_deref()) {
        seen.insert(domain.to_lowercase());
        context.push(domain);
    }

    for snippet in snippets {
        for entity in extract_entities(snippet) {
            let key = entity.to_lowercase();
            if key.len() < 3 || seen.contains(&key) {
                continue;
            }
            seen.insert(key);
            context.push(entity);
            if context.len() >= 4 {
                break;
            }
        }
        if context.len() >= 4 {
            break;
        }
    }

    if context.is_empty() {
        context.push(truncate_words(&anchor.window_title, 6));
    }

    context
}

fn extract_story_facts(snippets: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut facts = Vec::new();

    for snippet in snippets {
        let cleaned = normalize_sentence(snippet);
        if cleaned.is_empty() {
            continue;
        }
        let lower = cleaned.to_lowercase();
        if lower.starts_with("worked in ")
            || lower == "google chrome"
            || lower.contains("new tab")
            || is_ui_chrome_phrase(&cleaned)
        {
            continue;
        }

        let key = normalize_for_dedup(&cleaned);
        if key.is_empty() || !seen.insert(key) {
            continue;
        }

        let clipped = truncate_words(&cleaned, 18);
        if clipped.split_whitespace().count() >= 4 {
            facts.push(clipped);
        }
        if facts.len() >= 2 {
            break;
        }
    }

    facts
}

fn apply_story_continuity(cards: &mut [MemoryCard]) {
    if cards.len() <= 1 {
        return;
    }

    for idx in 1..cards.len() {
        let previous = cards[idx - 1].timestamp;
        let current = cards[idx].timestamp;
        if previous >= current && previous - current <= 20 * 60 * 1000 {
            let lower = cards[idx].summary.to_lowercase();
            if !lower.starts_with("then ") && !lower.starts_with("after that") {
                cards[idx].summary = format!("Then, {}", cards[idx].summary);
            }
        }
    }
}

fn extract_entities(text: &str) -> Vec<String> {
    let stop = stop_words();
    let mut entities = Vec::new();

    for token in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|tok| tok.len() > 2)
    {
        let lower = token.to_lowercase();
        if stop.contains(lower.as_str()) {
            continue;
        }
        if token.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        entities.push(token.to_string());
        if entities.len() >= 3 {
            break;
        }
    }

    entities
}

fn stop_words() -> HashSet<&'static str> {
    [
        "the", "and", "for", "with", "that", "from", "this", "have", "into", "while", "open",
        "page", "about", "using", "user", "you", "your", "their",
    ]
    .into_iter()
    .collect()
}

fn truncate_words(text: &str, max_words: usize) -> String {
    text.split_whitespace()
        .take(max_words)
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_sentence(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|ch: char| ch == '"' || ch == '\'' || ch == '`')
        .to_string()
}

fn token_overlap(a: &str, b: &str) -> f32 {
    let left = tokenize(a);
    let right = tokenize(b);
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }

    let intersection = left.intersection(&right).count() as f32;
    let union = left.union(&right).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn tokenize(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|tok| tok.len() > 2)
        .map(|tok| tok.to_string())
        .collect()
}

fn normalize_for_dedup(text: &str) -> String {
    text.to_lowercase()
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

fn extract_domain(url: Option<&str>) -> Option<String> {
    let url = url?.trim();
    if url.is_empty() {
        return None;
    }

    let host = url
        .split("://")
        .nth(1)
        .unwrap_or(url)
        .split('/')
        .next()
        .unwrap_or_default()
        .trim();

    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_nearby_same_session_hits() {
        let base = SearchResult {
            id: "1".to_string(),
            timestamp: 1_000_000,
            app_name: "Chrome".to_string(),
            bundle_id: None,
            window_title: "IPL 2026 highlights - YouTube".to_string(),
            session_id: "s1".to_string(),
            text: "IPL highlights and score recap".to_string(),
            clean_text: "IPL highlights and score recap".to_string(),
            ocr_confidence: 0.91,
            ocr_block_count: 8,
            snippet: "Watching IPL highlights on YouTube".to_string(),
            summary_source: "llm".to_string(),
            noise_score: 0.1,
            session_key: "chrome:youtube:ipl".to_string(),
            score: 0.8,
            screenshot_path: None,
            url: Some("https://www.youtube.com/watch?v=123".to_string()),
        };

        let mut second = base.clone();
        second.id = "2".to_string();
        second.timestamp -= 60_000;
        second.snippet = "Searching for cricket highlights".to_string();

        let groups = group_results(&[base, second], 6);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members.len(), 2);
    }

    #[test]
    fn rejects_bad_summary_patterns() {
        assert!(sanitize_summary("The screen shows New Tab and toolbar labels.").is_none());
        assert!(sanitize_summary(
            "Reviewed IPL highlights on YouTube while comparing match statistics."
        )
        .is_some());
    }

    #[test]
    fn fallback_produces_contextual_summary() {
        let anchor = SearchResult {
            id: "1".to_string(),
            timestamp: 1,
            app_name: "Chrome".to_string(),
            bundle_id: None,
            window_title: "YouTube - Cricket".to_string(),
            session_id: "s".to_string(),
            text: "".to_string(),
            clean_text: "".to_string(),
            ocr_confidence: 0.8,
            ocr_block_count: 4,
            snippet: "".to_string(),
            summary_source: "fallback".to_string(),
            noise_score: 0.2,
            session_key: "chrome:youtube:cricket".to_string(),
            score: 0.4,
            screenshot_path: None,
            url: Some("https://www.youtube.com/results?search_query=cricket".to_string()),
        };

        let (_, summary, _, _) = deterministic_fallback(
            "cricket",
            &anchor,
            &["IPL highlights and score table".to_string()],
        );

        assert!(summary.matches('.').count() <= 2);
        assert!(!summary.to_lowercase().contains("new tab"));
        assert!(!summary.to_lowercase().contains("worked in google chrome"));
        assert!(
            summary.to_lowercase().contains("ipl") || summary.to_lowercase().contains("cricket")
        );
    }
}
