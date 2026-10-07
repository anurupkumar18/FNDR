//! Compose `embedding_text` from structured / insight fields only (no OCR blob).

use crate::storage::schema::MemoryRecord;

fn trim_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

const MAX_COMMAND_CHARS: usize = 120;
const MAX_COMMANDS: usize = 5;

fn push_segment(out: &mut Vec<String>, value: &str, label: &str) {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("unknown") {
        return;
    }
    out.push(format!("{label}: {trimmed}"));
}

/// Build retrieval text for embedding from insight and structured fields only.
/// Does **not** append `clean_text`, compressed OCR, or `raw_evidence`.
pub fn compose_insight_embedding_text(record: &MemoryRecord) -> String {
    let mut segments = Vec::new();

    // The line a person reads on the card goes first, so the vector answers
    // a search for what they remember reading. Placeholders say nothing.
    let summary = record.display_summary.trim();
    let has_summary =
        !crate::summariser::narration_filter::is_placeholder_summary(summary);
    if has_summary {
        push_segment(&mut segments, summary, "summary");
    }

    // The window title is how people often remember a page or file. The app
    // name alone is not a title.
    let title = record.window_title.trim();
    if !title.eq_ignore_ascii_case(record.app_name.trim()) {
        push_segment(&mut segments, title, "title");
    }

    push_segment(&mut segments, &record.user_intent, "intent");
    push_segment(&mut segments, &record.project, "project");
    push_segment(&mut segments, &record.topic, "topic");
    push_segment(&mut segments, &record.workflow, "workflow");
    push_segment(&mut segments, &record.memory_context, "context");

    // Insight layers (when populated) anchor semantics for cards + retrieval.
    if !(has_summary && record.insight_what_happened.trim() == summary) {
        push_segment(
            &mut segments,
            &record.insight_what_happened,
            "what_happened",
        );
    }
    push_segment(&mut segments, &record.insight_why_mattered, "why_mattered");
    push_segment(&mut segments, &record.insight_what_changed, "what_changed");
    push_segment(
        &mut segments,
        &record.insight_context_thread,
        "context_thread",
    );

    let entity_blob = record.entities.join(", ");
    push_segment(&mut segments, &entity_blob, "entities");
    let alias_blob = record.search_aliases.join(", ");
    push_segment(&mut segments, &alias_blob, "aliases");
    let decisions = record.decisions.join("; ");
    push_segment(&mut segments, &decisions, "decisions");
    let errors = record.errors.join("; ");
    push_segment(&mut segments, &errors, "errors");
    let blockers = record.blockers.join("; ");
    push_segment(&mut segments, &blockers, "blockers");
    let todos = if !record.todos.is_empty() {
        record.todos.join("; ")
    } else {
        record.next_steps.join("; ")
    };
    push_segment(&mut segments, &todos, "todos");
    let results = record.results.join("; ");
    push_segment(&mut segments, &results, "results");
    push_segment(&mut segments, &record.files_touched.join(", "), "files");
    if let Some(url) = record.url.as_deref() {
        push_segment(&mut segments, url, "urls");
    }
    // A command is short. A long entry is prose that extraction misfiled,
    // and one of those used to fill a third of the embedded text.
    let commands = record
        .commands
        .iter()
        .filter(|command| command.chars().count() <= MAX_COMMAND_CHARS)
        .take(MAX_COMMANDS)
        .cloned()
        .collect::<Vec<_>>();
    push_segment(&mut segments, &commands.join("; "), "commands");

    trim_chars(&segments.join("\n"), 2_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::schema::MemoryRecord;

    #[test]
    fn compose_never_includes_clean_text_when_structured_present() {
        let mut r = MemoryRecord::default();
        r.user_intent = "Ship the fix".to_string();
        r.project = "fndr".to_string();
        r.topic = "memory".to_string();
        r.clean_text = "SECRET_OCR_BLOB_THAT_MUST_NOT_LEAK_INTO_EMBEDDING".to_string();
        r.insight_what_happened = "Worked on memory indexing.".to_string();
        let out = compose_insight_embedding_text(&r);
        assert!(out.contains("what_happened"));
        assert!(!out.contains("SECRET_OCR"));
    }

    #[test]
    fn the_card_summary_leads_and_is_not_repeated() {
        let mut r = MemoryRecord::default();
        r.display_summary = "Reviewed the retrieval gate results.".to_string();
        r.insight_what_happened = r.display_summary.clone();
        r.topic = "retrieval".to_string();
        let out = compose_insight_embedding_text(&r);
        assert!(out.starts_with("summary: Reviewed the retrieval gate results."), "{out}");
        assert!(!out.contains("what_happened"), "{out}");

        r.app_name = "Google Chrome".to_string();
        r.window_title = "Retrieval gate - GitLab".to_string();
        assert!(compose_insight_embedding_text(&r).contains("title: Retrieval gate - GitLab"));
        r.window_title = "Google Chrome".to_string();
        assert!(!compose_insight_embedding_text(&r).contains("title:"));

        r.display_summary = "Screen capture visual : ChatGPT_1789709739566.".to_string();
        assert!(!compose_insight_embedding_text(&r).contains("summary:"));
    }

    #[test]
    fn prose_filed_as_a_command_is_left_out() {
        let mut r = MemoryRecord::default();
        r.topic = "build".to_string();
        r.commands = vec!["cargo test --lib".to_string(), "please ".repeat(40)];
        let out = compose_insight_embedding_text(&r);
        assert!(out.contains("commands: cargo test --lib"), "{out}");
        assert!(!out.contains("please please"), "{out}");
    }
}
