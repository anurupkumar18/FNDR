use crate::memory::types::{EmbeddingDocument, ValidatedMemory};

fn looks_like_bad_alias(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return true;
    }
    if trimmed.len() > 120 {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("www.") {
        return true;
    }
    let has_many_symbols = trimmed
        .chars()
        .filter(|ch| !ch.is_ascii_alphanumeric() && !ch.is_whitespace())
        .count()
        > (trimmed.len() / 3);
    has_many_symbols || lower.contains("reopen") || lower.contains("find similar")
}

pub fn build_embedding_document(memory: &ValidatedMemory) -> EmbeddingDocument {
    let mut lines = Vec::new();
    if !memory.title.trim().is_empty() {
        lines.push(format!("title: {}", memory.title.trim()));
    }
    if !memory.topic.trim().is_empty() {
        lines.push(format!("topic: {}", memory.topic.trim()));
    }
    if !memory.summary_short.trim().is_empty() {
        lines.push(format!("summary: {}", memory.summary_short.trim()));
    }
    if !memory.memory_context.trim().is_empty() {
        lines.push(format!("context: {}", memory.memory_context.trim()));
    }
    if !memory.workflow.trim().is_empty() {
        lines.push(format!("workflow: {}", memory.workflow.trim()));
    }
    if !memory.project.trim().is_empty() {
        lines.push(format!("project: {}", memory.project.trim()));
    }
    if !memory.user_intent.trim().is_empty() {
        lines.push(format!("intent: {}", memory.user_intent.trim()));
    }
    if !memory.entities.is_empty() {
        lines.push(format!("entities: {}", memory.entities.join(", ")));
    }
    if !memory.actions.is_empty() {
        lines.push(format!("actions: {}", memory.actions.join("; ")));
    }

    let aliases = memory
        .entities
        .iter()
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !looks_like_bad_alias(v))
        .collect::<Vec<_>>();

    EmbeddingDocument {
        text: lines.join("\n"),
        aliases,
    }
}
