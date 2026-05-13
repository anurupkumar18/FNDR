//! Minimal wiki text composer for MCP / graph context (extend with real page storage later).

/// Short stub summary; production can merge persisted [`crate::storage::KnowledgePage`] rows.
pub fn synthesize_wiki_stub(project: Option<&str>) -> String {
    match project {
        Some(p) if !p.trim().is_empty() => format!(
            "Graph wiki (stub): project focus \"{p}\". High-confidence nodes feed append-only sections; contradictions are listed explicitly."
        ),
        _ => "Graph wiki (stub): no project filter. Append-only sections; conflicts preserved.".to_string(),
    }
}
