use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct QueryContext {
    pub raw_query: String,
    pub normalized_query: String,
    pub anchor_terms: Vec<String>,
}

impl QueryContext {
    pub fn from_query(query: &str) -> Self {
        let normalized_query = normalize_text(query);
        let mut anchors = extract_base_anchor_terms(&normalized_query);

        // Lightweight synonym expansion for high-impact browsing intents.
        let mut expanded = Vec::new();
        for term in &anchors {
            expanded.push(term.clone());
            match term.as_str() {
                "ipl" => expanded.push("cricket".to_string()),
                "cricket" => expanded.push("ipl".to_string()),
                "football" => expanded.push("soccer".to_string()),
                "soccer" => expanded.push("football".to_string()),
                "auth" => expanded.push("authentication".to_string()),
                "authentication" => expanded.push("auth".to_string()),
                _ => {}
            }
        }

        if expanded.is_empty() && !normalized_query.is_empty() {
            expanded.push(normalized_query.clone());
        }

        let mut seen = HashSet::new();
        anchors = expanded
            .into_iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .filter(|value| seen.insert(value.clone()))
            .take(8)
            .collect();

        Self {
            raw_query: query.to_string(),
            normalized_query,
            anchor_terms: anchors,
        }
    }
}

pub fn normalize_text(input: &str) -> String {
    input
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

fn extract_base_anchor_terms(normalized_query: &str) -> Vec<String> {
    if normalized_query.is_empty() {
        return Vec::new();
    }

    let mut anchors = Vec::new();
    anchors.push(normalized_query.to_string());

    for token in normalized_query.split_whitespace() {
        if token.len() <= 1 {
            continue;
        }
        if is_anchor_stop_word(token) && !token.chars().any(|ch| ch.is_ascii_digit()) {
            continue;
        }
        if !anchors.iter().any(|existing| existing == token) {
            anchors.push(token.to_string());
        }
    }

    anchors
}

fn is_anchor_stop_word(token: &str) -> bool {
    matches!(
        token,
        "a"
            | "an"
            | "and"
            | "are"
            | "as"
            | "at"
            | "be"
            | "for"
            | "from"
            | "in"
            | "is"
            | "it"
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
            | "open"
            | "go"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_anchor_terms_with_synonyms() {
        let context = QueryContext::from_query("IPL highlights");
        assert!(context.anchor_terms.iter().any(|term| term == "ipl"));
        assert!(context.anchor_terms.iter().any(|term| term == "cricket"));
    }
}
