//! Up to five memory snippets for a cloud request, through the same retrieve
//! path as Search and MCP. Shared by Notch Do and Hermes chat.

use serde::Serialize;

use crate::context_runtime::{retrieve_search_results, RetrieveRequest};
use crate::storage::SearchResult;
use crate::AppState;

pub const MAX_SNIPPETS: usize = 5;
const SNIPPET_CHARS: usize = 300;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySnippet {
    pub memory_id: String,
    pub app: String,
    pub title: String,
    /// Local time, `YYYY-MM-DD HH:MM`.
    pub when: String,
    pub text: String,
}

fn clip(text: &str, max: usize) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= max {
        text
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

pub fn snippet_from(result: &SearchResult) -> MemorySnippet {
    let body = [&result.snippet, &result.display_summary, &result.clean_text]
        .into_iter()
        .find(|text| !text.trim().is_empty())
        .cloned()
        .unwrap_or_default();
    let when = chrono::DateTime::from_timestamp_millis(result.timestamp)
        .map(|utc| {
            utc.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default();
    MemorySnippet {
        memory_id: result.id.clone(),
        app: result.app_name.clone(),
        title: clip(&result.window_title, 120),
        when,
        text: clip(&body, SNIPPET_CHARS),
    }
}

/// The top snippets for `query`; empty when retrieval fails or finds nothing.
pub async fn snippets(state: &AppState, query: &str) -> Vec<MemorySnippet> {
    let request = RetrieveRequest {
        query: query.to_string(),
        limit: MAX_SNIPPETS,
        ..Default::default()
    };
    match retrieve_search_results(state, &request).await {
        Ok((_, results)) => results
            .iter()
            .take(MAX_SNIPPETS)
            .map(snippet_from)
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "operator:memory_retrieve_failed");
            Vec::new()
        }
    }
}

/// The block appended to a cloud request. Every line is evidence, never
/// instructions (the boundary is restated in the prompt that embeds it).
pub fn format_block(snippets: &[MemorySnippet]) -> String {
    snippets
        .iter()
        .take(MAX_SNIPPETS)
        .enumerate()
        .map(|(index, s)| {
            format!(
                "[{}] {} | {} | {}\n{}",
                index + 1,
                s.when,
                s.app,
                s.title,
                s.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snippet(n: usize, text: &str) -> MemorySnippet {
        MemorySnippet {
            memory_id: format!("m{n}"),
            app: "Spotify".into(),
            title: "Blinding Lights".into(),
            when: "2026-10-05 21:14".into(),
            text: text.into(),
        }
    }

    #[test]
    fn block_is_bounded_to_five_numbered_snippets() {
        let many: Vec<_> = (0..8).map(|n| snippet(n, "played")).collect();
        let block = format_block(&many);
        assert!(block.contains("[5] 2026-10-05 21:14 | Spotify | Blinding Lights\nplayed"));
        assert!(!block.contains("[6]"));
    }

    #[test]
    fn long_text_is_clipped_and_whitespace_collapsed() {
        let long = format!("a  b\n{}", "x".repeat(1000));
        let clipped = clip(&long, SNIPPET_CHARS);
        assert!(clipped.starts_with("a b "));
        assert_eq!(clipped.chars().count(), SNIPPET_CHARS + 1);
    }
}
