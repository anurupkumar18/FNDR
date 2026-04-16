//! Hybrid search combining semantic and keyword retrieval with query understanding.

use crate::capture::text_cleanup;
use crate::embed::Embedder;
use crate::store::{SearchResult, Store};
use std::collections::{HashMap, HashSet};

/// Legacy RRF constant kept for backwards-compatible helper usage.
const RRF_K: f32 = 60.0;

const CANDIDATE_MULTIPLIER: usize = 6;
const MAX_KEYWORD_VARIANTS: usize = 4;
const MAX_RERANK_POOL: usize = 28;
const SEMANTIC_WEIGHT: f32 = 0.50;
const LEXICAL_WEIGHT: f32 = 0.50;
const ABSOLUTE_RELEVANCE_FLOOR: f32 = 0.31;
const RELATIVE_RELEVANCE_FLOOR: f32 = 0.56;

/// Hybrid searcher combining semantic + lexical retrieval and sentence-aware reranking.
pub struct HybridSearcher;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryIntent {
    Definition,
    HowTo,
    Lookup,
    General,
}

#[derive(Debug, Clone)]
struct QueryProfile {
    raw: String,
    normalized: String,
    intent: QueryIntent,
    primary_terms: Vec<String>,
    expanded_terms: Vec<String>,
    number_terms: HashSet<String>,
    phrase: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct FusionSignals {
    semantic_score: Option<f32>,
    keyword_score: Option<f32>,
    lexical_score: f32,
    coverage: f32,
    phrase_score: f32,
}

impl QueryProfile {
    fn from_query(query: &str) -> Self {
        let normalized = normalize_text(query);
        let mut tokens = token_vec(&normalized);

        if tokens.is_empty() {
            return Self {
                raw: query.to_string(),
                normalized,
                intent: QueryIntent::General,
                primary_terms: Vec::new(),
                expanded_terms: Vec::new(),
                number_terms: HashSet::new(),
                phrase: None,
            };
        }

        let mut number_terms = HashSet::new();
        for token in &tokens {
            if token.chars().any(|ch| ch.is_ascii_digit()) {
                number_terms.insert(token.clone());
            }
        }

        let intent = detect_intent(&normalized);

        let mut primary_terms = tokens
            .iter()
            .filter(|token| !is_stop_word(token) || token.chars().any(|ch| ch.is_ascii_digit()))
            .cloned()
            .collect::<Vec<_>>();

        if primary_terms.is_empty() {
            primary_terms = tokens.clone();
        }

        // Keep sentence-level search usable by preserving high-signal content terms.
        primary_terms.truncate(8);

        let mut expanded_terms = Vec::new();
        for token in &primary_terms {
            push_unique(&mut expanded_terms, token);

            let stem = stem_token(token);
            if !stem.is_empty() {
                push_unique(&mut expanded_terms, &stem);
            }

            for alias in alias_terms(token) {
                push_unique(&mut expanded_terms, &alias);
            }

            if token.len() > 4 && token.ends_with('s') {
                push_unique(&mut expanded_terms, &token[..token.len() - 1]);
            }
            if token.len() > 3 && !token.ends_with('s') {
                let plural = format!("{}s", token);
                push_unique(&mut expanded_terms, &plural);
            }
        }

        tokens.clear();

        let phrase = if normalized.split_whitespace().count() >= 2 {
            Some(normalized.clone())
        } else {
            None
        };

        Self {
            raw: query.to_string(),
            normalized,
            intent,
            primary_terms,
            expanded_terms,
            number_terms,
            phrase,
        }
    }

    fn is_empty(&self) -> bool {
        self.normalized.is_empty()
    }

    fn keyword_variants(&self) -> Vec<String> {
        let mut variants = Vec::new();

        if let Some(phrase) = self.phrase.as_ref() {
            push_unique(&mut variants, phrase);
        }

        if !self.number_terms.is_empty() {
            for value in &self.number_terms {
                push_unique(&mut variants, value);
            }
        }

        if !self.primary_terms.is_empty() {
            let joined = self
                .primary_terms
                .iter()
                .take(4)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            if !joined.is_empty() {
                push_unique(&mut variants, &joined);
            }
        }

        for term in self.primary_terms.iter().take(6) {
            push_unique(&mut variants, term);
        }

        if variants.is_empty() && !self.raw.trim().is_empty() {
            variants.push(self.raw.trim().to_string());
        }

        variants.truncate(MAX_KEYWORD_VARIANTS);
        variants
    }

    fn embedding_query(&self) -> String {
        let mut parts = Vec::new();

        if !self.raw.trim().is_empty() {
            parts.push(self.raw.trim().to_string());
        }

        let compact_terms = self
            .primary_terms
            .iter()
            .take(6)
            .cloned()
            .collect::<Vec<_>>();

        if !compact_terms.is_empty() {
            parts.push(compact_terms.join(" "));
        }

        let with_numbers = self
            .number_terms
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(" ");
        if !with_numbers.is_empty() {
            parts.push(with_numbers);
        }

        parts.join(" ").trim().to_string()
    }
}

impl HybridSearcher {
    /// Perform hybrid search with query understanding, weighted fusion, and reranking.
    pub async fn search(
        store: &Store,
        embedder: &Embedder,
        query: &str,
        limit: usize,
        time_filter: Option<&str>,
        app_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        let profile = QueryProfile::from_query(query);
        if profile.is_empty() {
            return Ok(Vec::new());
        }

        let branch_limit = limit.max(1) * CANDIDATE_MULTIPLIER;

        let embedding_query = profile.embedding_query();
        let query_embedding = embedder.embed_batch(&[embedding_query])?;
        let query_embedding = query_embedding.into_iter().next().unwrap_or_default();

        let semantic_results = store
            .vector_search(&query_embedding, branch_limit, time_filter, app_filter)
            .await?;

        let mut keyword_results = Vec::new();
        for (variant_idx, variant) in profile.keyword_variants().iter().enumerate() {
            let mut hits = store
                .keyword_search(variant, branch_limit, time_filter, app_filter)
                .await?;

            // Earlier variants are stronger rewrites; keep a light priority decay.
            let decay = (1.0 - (variant_idx as f32 * 0.08)).max(0.72);
            for hit in &mut hits {
                hit.score *= decay;
            }

            keyword_results.extend(hits);
        }
        let keyword_results = dedup_by_best_score(keyword_results);

        let fused = Self::hybrid_fusion(&profile, &semantic_results, &keyword_results);
        let reranked = Self::rerank_with_profile(&profile, fused, limit);

        Ok(reranked)
    }

    /// Merge semantic + keyword candidates, then rerank with the standard policy.
    pub fn fuse_and_rerank(
        query: &str,
        semantic: &[SearchResult],
        keyword: &[SearchResult],
        limit: usize,
    ) -> Vec<SearchResult> {
        let profile = QueryProfile::from_query(query);
        let fused = Self::hybrid_fusion(&profile, semantic, keyword);
        Self::rerank_with_profile(&profile, fused, limit)
    }

    /// Backwards-compatible legacy RRF fusion helper.
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
            let keyword_boost = 1.15;
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

    fn hybrid_fusion(
        profile: &QueryProfile,
        semantic: &[SearchResult],
        keyword: &[SearchResult],
    ) -> Vec<SearchResult> {
        let mut signals: HashMap<String, FusionSignals> = HashMap::new();
        let mut candidates: HashMap<String, SearchResult> = HashMap::new();

        for result in semantic {
            candidates
                .entry(result.id.clone())
                .and_modify(|existing| {
                    if result.score > existing.score {
                        *existing = result.clone();
                    }
                })
                .or_insert_with(|| result.clone());

            signals
                .entry(result.id.clone())
                .and_modify(|signal| {
                    signal.semantic_score = Some(
                        signal
                            .semantic_score
                            .map(|current| current.max(result.score))
                            .unwrap_or(result.score),
                    );
                })
                .or_insert_with(|| FusionSignals {
                    semantic_score: Some(result.score),
                    ..FusionSignals::default()
                });
        }

        for result in keyword {
            candidates
                .entry(result.id.clone())
                .and_modify(|existing| {
                    if result.score > existing.score {
                        *existing = result.clone();
                    }
                })
                .or_insert_with(|| result.clone());

            signals
                .entry(result.id.clone())
                .and_modify(|signal| {
                    signal.keyword_score = Some(
                        signal
                            .keyword_score
                            .map(|current| current.max(result.score))
                            .unwrap_or(result.score),
                    );
                })
                .or_insert_with(|| FusionSignals {
                    keyword_score: Some(result.score),
                    ..FusionSignals::default()
                });
        }

        let mut docs = Vec::new();
        for result in candidates.values() {
            let merged = merged_candidate_text(result);
            let doc_tokens = token_vec(&merged);
            docs.push((result.id.clone(), merged, doc_tokens));
        }

        let avg_len = if docs.is_empty() {
            1.0
        } else {
            docs.iter()
                .map(|(_, _, tokens)| tokens.len() as f32)
                .sum::<f32>()
                / docs.len() as f32
        }
        .max(1.0);

        let doc_freq = build_doc_frequency(profile, &docs);

        for (id, text, tokens) in &docs {
            let lexical = bm25_like_score(profile, text, tokens, &doc_freq, docs.len(), avg_len)
                + signals.get(id).and_then(|s| s.keyword_score).unwrap_or(0.0) * 0.55;

            let coverage = term_coverage(profile, text);
            let phrase = phrase_alignment(profile, text);

            if let Some(signal) = signals.get_mut(id) {
                signal.lexical_score = lexical;
                signal.coverage = coverage;
                signal.phrase_score = phrase;
            }
        }

        let semantic_values = signals
            .values()
            .map(|s| s.semantic_score.unwrap_or(0.0))
            .collect::<Vec<_>>();
        let lexical_values = signals
            .values()
            .map(|s| s.lexical_score)
            .collect::<Vec<_>>();

        let semantic_range = value_range(&semantic_values);
        let lexical_range = value_range(&lexical_values);

        let mut fused = Vec::new();
        for (id, mut result) in candidates {
            let signal = signals.get(&id).cloned().unwrap_or_default();

            let semantic_norm =
                normalize_range(signal.semantic_score.unwrap_or(0.0), semantic_range);
            let lexical_norm = normalize_range(signal.lexical_score, lexical_range);

            let mut score = semantic_norm * SEMANTIC_WEIGHT + lexical_norm * LEXICAL_WEIGHT;
            score += signal.coverage * 0.12;
            score += signal.phrase_score * 0.08;

            if signal.semantic_score.is_some() && signal.keyword_score.is_some() {
                score += 0.05;
            }

            if profile.intent == QueryIntent::Definition
                && mentions_query_entities(profile, &merged_candidate_text(&result))
            {
                score += 0.04;
            }

            result.score = score.max(0.0);
            fused.push(result);
        }

        fused.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        });

        fused
    }

    pub fn rerank(query: &str, candidates: Vec<SearchResult>, limit: usize) -> Vec<SearchResult> {
        let profile = QueryProfile::from_query(query);
        Self::rerank_with_profile(&profile, candidates, limit)
    }

    fn rerank_with_profile(
        profile: &QueryProfile,
        mut candidates: Vec<SearchResult>,
        limit: usize,
    ) -> Vec<SearchResult> {
        if candidates.is_empty() {
            return Vec::new();
        }

        candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        });
        candidates.truncate(MAX_RERANK_POOL.max(limit * 3));

        let query_lower = profile.normalized.clone();
        let code_query = is_code_query(&profile.raw);

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
            let merged_norm = format!("{} {} {}", title_norm, text_norm, snippet_norm);

            // Query-aware sentence reranker feature.
            let sentence_relevance = sentence_level_relevance(profile, &merged_norm);
            score *= 0.70 + sentence_relevance * 0.58;

            // Time decay (gentle).
            let age_hours =
                ((now - candidate.timestamp).max(0) as f32 / 3_600_000.0).min(24.0 * 30.0);
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
                score *= 0.82;
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
            let coverage = term_coverage(profile, &merged_norm);
            if coverage > 0.0 {
                score *= 1.0 + coverage.min(0.85) * 0.28;
            } else if !profile.primary_terms.is_empty() {
                score *= 0.68;
            }

            let entity_overlap = named_entity_overlap(&profile.raw, &text);
            if entity_overlap > 0 {
                score *= 1.0 + (entity_overlap as f32 * 0.06).min(0.18);
            }

            if !profile.number_terms.is_empty() && !mentions_query_entities(profile, &merged_norm) {
                score *= 0.5;
            }

            if let Some(url) = &candidate.url {
                let domain = extract_domain(url);
                if !domain.is_empty() && query_lower.contains(&domain) {
                    score *= 1.18;
                }
                if !domain.is_empty() && profile.primary_terms.iter().any(|t| domain.contains(t)) {
                    score *= 1.08;
                }
            }

            if candidate.summary_source.eq_ignore_ascii_case("llm") {
                score *= 1.05;
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

            if deduped.len() >= limit.max(6).min(30) {
                break;
            }
        }

        let mut gated = apply_relevance_gate(profile, deduped);
        gated.truncate(limit.min(30));
        gated
    }
}

fn apply_relevance_gate(
    profile: &QueryProfile,
    candidates: Vec<SearchResult>,
) -> Vec<SearchResult> {
    if candidates.is_empty() {
        return Vec::new();
    }

    let top_score = candidates[0].score;
    let absolute_floor = ABSOLUTE_RELEVANCE_FLOOR
        + if profile.primary_terms.len() >= 4 {
            0.04
        } else {
            0.0
        }
        + if profile.intent == QueryIntent::Definition {
            0.02
        } else {
            0.0
        };
    let effective_absolute_floor = absolute_floor.min((top_score * 0.90).max(0.01));
    let relative_floor = top_score * RELATIVE_RELEVANCE_FLOOR;
    let min_coverage = if profile.primary_terms.len() >= 4 {
        0.34
    } else if profile.primary_terms.len() >= 3 {
        0.28
    } else if profile.primary_terms.len() >= 2 {
        0.20
    } else {
        0.0
    };

    let mut filtered = Vec::new();
    for candidate in candidates {
        let merged = merged_candidate_text(&candidate);
        let coverage = term_coverage(profile, &merged);
        let has_entity = mentions_query_entities(profile, &merged);

        if candidate.score < effective_absolute_floor {
            continue;
        }

        if candidate.score < relative_floor && !filtered.is_empty() {
            continue;
        }

        if !profile.primary_terms.is_empty() && coverage < min_coverage && !has_entity {
            continue;
        }

        if !profile.number_terms.is_empty() && !has_entity {
            continue;
        }

        filtered.push(candidate);
    }

    if filtered.is_empty() {
        return Vec::new();
    }

    filtered
}

fn candidate_text(result: &SearchResult) -> String {
    if !result.clean_text.trim().is_empty() {
        result.clean_text.clone()
    } else {
        result.text.clone()
    }
}

fn merged_candidate_text(result: &SearchResult) -> String {
    format!(
        "{} {} {} {}",
        result.window_title,
        candidate_text(result),
        result.snippet,
        result.url.clone().unwrap_or_default()
    )
}

fn dedup_by_best_score(results: Vec<SearchResult>) -> Vec<SearchResult> {
    let mut by_id: HashMap<String, SearchResult> = HashMap::new();

    for result in results {
        by_id
            .entry(result.id.clone())
            .and_modify(|existing| {
                if result.score > existing.score
                    || (result.score == existing.score && result.timestamp > existing.timestamp)
                {
                    *existing = result.clone();
                }
            })
            .or_insert(result);
    }

    by_id.into_values().collect()
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

fn token_vec(value: &str) -> Vec<String> {
    normalize_text(value)
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
        .collect()
}

fn token_set_with_stems(value: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for token in token_vec(value) {
        set.insert(token.clone());
        let stem = stem_token(&token);
        if !stem.is_empty() {
            set.insert(stem);
        }
    }
    set
}

fn stem_token(token: &str) -> String {
    let lower = token.trim().to_lowercase();
    if lower.len() <= 2 {
        return lower;
    }

    if lower.len() > 4 && lower.ends_with("ies") {
        return format!("{}y", &lower[..lower.len() - 3]);
    }
    if lower.len() > 5 && lower.ends_with("ing") {
        return lower[..lower.len() - 3].to_string();
    }
    if lower.len() > 4 && lower.ends_with("ed") {
        return lower[..lower.len() - 2].to_string();
    }
    if lower.len() > 4 && lower.ends_with("es") {
        return lower[..lower.len() - 2].to_string();
    }
    if lower.len() > 3 && lower.ends_with('s') {
        return lower[..lower.len() - 1].to_string();
    }

    lower
}

fn alias_terms(token: &str) -> Vec<String> {
    match token {
        "canva" => vec!["canvas".to_string(), "design".to_string()],
        "canvas" => vec!["canva".to_string(), "design".to_string()],
        "cricket" => vec!["ipl".to_string(), "match".to_string()],
        _ => Vec::new(),
    }
}

fn is_stop_word(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "by"
            | "for"
            | "from"
            | "how"
            | "i"
            | "in"
            | "is"
            | "it"
            | "me"
            | "my"
            | "of"
            | "on"
            | "or"
            | "that"
            | "the"
            | "this"
            | "to"
            | "was"
            | "what"
            | "when"
            | "where"
            | "who"
            | "why"
            | "with"
    )
}

fn detect_intent(query: &str) -> QueryIntent {
    if query.starts_with("what is ")
        || query.starts_with("who is ")
        || query.starts_with("define ")
        || query.starts_with("explain ")
    {
        QueryIntent::Definition
    } else if query.starts_with("how to ") || query.starts_with("how do ") {
        QueryIntent::HowTo
    } else if query.starts_with("where ") || query.starts_with("when ") {
        QueryIntent::Lookup
    } else {
        QueryIntent::General
    }
}

fn push_unique(target: &mut Vec<String>, value: &str) {
    let candidate = value.trim();
    if candidate.is_empty() {
        return;
    }
    if !target.iter().any(|existing| existing == candidate) {
        target.push(candidate.to_string());
    }
}

fn build_doc_frequency(
    profile: &QueryProfile,
    docs: &[(String, String, Vec<String>)],
) -> HashMap<String, usize> {
    let mut df: HashMap<String, usize> = HashMap::new();
    for (_, text, _) in docs {
        let token_set = token_set_with_stems(text);
        for term in &profile.expanded_terms {
            if token_set.contains(term) {
                *df.entry(term.clone()).or_insert(0) += 1;
            }
        }
    }
    df
}

fn bm25_like_score(
    profile: &QueryProfile,
    text: &str,
    tokens: &[String],
    doc_freq: &HashMap<String, usize>,
    doc_count: usize,
    avg_len: f32,
) -> f32 {
    if profile.expanded_terms.is_empty() || doc_count == 0 {
        return 0.0;
    }

    let mut tf: HashMap<String, usize> = HashMap::new();
    for token in tokens {
        *tf.entry(token.clone()).or_insert(0) += 1;
        let stem = stem_token(token);
        if !stem.is_empty() {
            *tf.entry(stem).or_insert(0) += 1;
        }
    }

    let k1 = 1.2;
    let b = 0.75;
    let doc_len = tokens.len().max(1) as f32;

    let mut score = 0.0;
    for term in &profile.expanded_terms {
        let freq = tf.get(term).copied().unwrap_or(0) as f32;
        if freq <= 0.0 {
            continue;
        }

        let df = doc_freq.get(term).copied().unwrap_or(1) as f32;
        let idf = (((doc_count as f32 - df + 0.5) / (df + 0.5)) + 1.0).ln();
        let denom = freq + k1 * (1.0 - b + b * (doc_len / avg_len));
        score += idf * ((freq * (k1 + 1.0)) / denom.max(1e-6));
    }

    score += phrase_alignment(profile, text) * 0.9;
    score
}

fn term_coverage(profile: &QueryProfile, text: &str) -> f32 {
    if profile.primary_terms.is_empty() {
        return 0.0;
    }

    let token_set = token_set_with_stems(text);
    let mut matched = 0usize;

    for term in &profile.primary_terms {
        let stem = stem_token(term);
        if token_set.contains(term)
            || (!stem.is_empty() && token_set.contains(&stem))
            || profile
                .number_terms
                .iter()
                .any(|number| !number.is_empty() && token_set.contains(number))
        {
            matched += 1;
        }
    }

    matched as f32 / profile.primary_terms.len().max(1) as f32
}

fn phrase_alignment(profile: &QueryProfile, text: &str) -> f32 {
    let Some(phrase) = profile.phrase.as_ref() else {
        return 0.0;
    };

    let normalized = normalize_text(text);
    if normalized.contains(phrase) {
        return 1.0;
    }

    let phrase_bigrams = bigrams(phrase);
    if phrase_bigrams.is_empty() {
        return 0.0;
    }

    let text_bigrams = bigrams(&normalized);
    if text_bigrams.is_empty() {
        return 0.0;
    }

    let overlap = phrase_bigrams
        .iter()
        .filter(|bigram| text_bigrams.contains(*bigram))
        .count();

    overlap as f32 / phrase_bigrams.len() as f32
}

fn bigrams(text: &str) -> HashSet<String> {
    let tokens = token_vec(text);
    if tokens.len() < 2 {
        return HashSet::new();
    }

    let mut out = HashSet::new();
    for pair in tokens.windows(2) {
        out.insert(format!("{} {}", pair[0], pair[1]));
    }
    out
}

fn sentence_level_relevance(profile: &QueryProfile, text: &str) -> f32 {
    let coverage = term_coverage(profile, text);
    let phrase = phrase_alignment(profile, text);
    let entity = if mentions_query_entities(profile, text) {
        1.0
    } else {
        0.0
    };

    (coverage * 0.58 + phrase * 0.28 + entity * 0.14).clamp(0.0, 1.0)
}

fn mentions_query_entities(profile: &QueryProfile, text: &str) -> bool {
    let normalized = normalize_text(text);

    if !profile.number_terms.is_empty()
        && profile
            .number_terms
            .iter()
            .any(|number| normalized.contains(number))
    {
        return true;
    }

    profile
        .primary_terms
        .iter()
        .any(|term| normalized.contains(term) || normalized.contains(&stem_token(term)))
}

fn value_range(values: &[f32]) -> (f32, f32) {
    if values.is_empty() {
        return (0.0, 0.0);
    }
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    for value in values {
        min = min.min(*value);
        max = max.max(*value);
    }
    (min, max)
}

fn normalize_range(value: f32, range: (f32, f32)) -> f32 {
    let (min, max) = range;
    if (max - min).abs() < 1e-6 {
        if value > 0.0 {
            1.0
        } else {
            0.0
        }
    } else {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    }
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
    let left = token_set_with_stems(a);
    let right = token_set_with_stems(b);
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
    fn query_profile_extracts_number_focus_for_definition_queries() {
        let profile = QueryProfile::from_query("what is 4000");
        assert_eq!(profile.intent, QueryIntent::Definition);
        assert!(profile.number_terms.contains("4000"));
        assert!(profile.expanded_terms.iter().any(|term| term == "4000"));
    }

    #[test]
    fn query_profile_adds_canva_canvas_aliases() {
        let profile = QueryProfile::from_query("canvas design");
        assert!(profile.expanded_terms.iter().any(|term| term == "canva"));
        assert!(profile.expanded_terms.iter().any(|term| term == "canvas"));
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

    #[test]
    fn relevance_gate_drops_irrelevant_results() {
        let random = sr("1", "Weather", "Forecast and humidity in Herriman", 0.34);
        let noisy = sr(
            "2",
            "Activity Monitor",
            "Checked battery and CPU usage",
            0.33,
        );

        let ranked = HybridSearcher::rerank("what is cricket", vec![random, noisy], 10);
        assert!(ranked.is_empty());
    }

    #[test]
    fn relevance_gate_keeps_exact_entity_matches() {
        let relevant = sr(
            "1",
            "ChatGPT - 4000",
            "User asked what is 4000 in ChatGPT",
            0.28,
        );
        let other = sr("2", "Weather", "Freeze watch in Herriman", 0.45);

        let ranked = HybridSearcher::rerank("what is 4000", vec![other, relevant], 10);
        assert_eq!(ranked.first().map(|r| r.id.as_str()), Some("1"));
    }
}
