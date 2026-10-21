//! Reads the store for a date range and hands the builders only what a
//! surface may show: the one visibility rule (blocklist, soft delete, quality
//! gate), and none of the text that is not the person's own work. Imported
//! rows (`import:*`) and assistant notes are untrusted text; a standup line
//! must never be one of them.

use super::{Evidence, MemoryEvidence, TaskEvidence};
use crate::storage::{ActivityEvent, AGENT_NOTE_SOURCE_TYPE};
use crate::AppState;
use std::collections::HashSet;

const MAX_EVENTS: usize = 5_000;

/// True for a memory that came from the person's own screen.
pub fn is_own_work(source_type: &str) -> bool {
    source_type != AGENT_NOTE_SOURCE_TYPE && !source_type.starts_with("import:")
}

/// Events that overlap the range and were built only from memories in
/// `eligible`.
pub fn eligible_events(
    events: Vec<ActivityEvent>,
    eligible: &HashSet<String>,
    start_ms: i64,
    end_ms: i64,
) -> Vec<ActivityEvent> {
    events
        .into_iter()
        .filter(|event| event.end_time >= start_ms && event.start_time <= end_ms)
        .filter(|event| eligible.contains(&event.memory_id))
        .filter(|event| {
            event
                .source_memory_ids
                .iter()
                .all(|id| id.trim().is_empty() || eligible.contains(id))
        })
        .collect()
}

pub async fn gather(state: &AppState, start_ms: i64, end_ms: i64) -> Result<Evidence, String> {
    let results = state
        .store
        .get_search_results_in_range(start_ms, end_ms)
        .await
        .map_err(|e| e.to_string())?;
    let (shown, _hidden) = crate::memory_quality::partition_surfaceable(results);
    let blocklist = state.config.read().blocklist.clone();
    let memories: Vec<MemoryEvidence> = shown
        .into_iter()
        .filter(|result| crate::context_runtime::retrieve::result_is_permitted(result, &blocklist))
        .filter(|result| is_own_work(&result.source_type))
        .map(|result| MemoryEvidence {
            summary: if result.display_summary.trim().is_empty() {
                result.snippet.clone()
            } else {
                result.display_summary.clone()
            },
            id: result.id,
            timestamp_ms: result.timestamp,
            app: result.app_name,
            window_title: result.window_title,
        })
        .collect();
    let eligible: HashSet<String> = memories.iter().map(|memory| memory.id.clone()).collect();

    let events = state
        .store
        .list_activity_events(MAX_EVENTS, None)
        .await
        .map_err(|e| e.to_string())?;
    let events = crate::context_runtime::retain_context_events(state, events).await?;
    let events = eligible_events(events, &eligible, start_ms, end_ms);

    let open = crate::tasks::suggest::open_commitments(
        state.store.list_tasks().await.map_err(|e| e.to_string())?,
    );
    let wanted: Vec<String> = open
        .iter()
        .flat_map(|task| {
            task.source_memory_id
                .iter()
                .chain(task.linked_memory_ids.iter())
                .cloned()
        })
        .collect();
    let visible = crate::context_runtime::context_source_memories(state, &wanted).await?;
    let tasks = open
        .into_iter()
        .map(|task| {
            let memory_ids = task
                .source_memory_id
                .iter()
                .chain(task.linked_memory_ids.iter())
                .filter(|id| {
                    visible
                        .get(*id)
                        .is_some_and(|m| is_own_work(&m.source_type))
                })
                .cloned()
                .collect();
            TaskEvidence {
                title: task.title,
                due_ms: task.due_date,
                created_at_ms: task.created_at,
                memory_ids,
            }
        })
        .collect();

    Ok(Evidence {
        events,
        memories,
        tasks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: &str, memory: &str, sources: &[&str], start: i64, end: i64) -> ActivityEvent {
        ActivityEvent {
            id: id.to_string(),
            memory_id: memory.to_string(),
            source_memory_ids: sources.iter().map(|s| s.to_string()).collect(),
            start_time: start,
            end_time: end,
            ..Default::default()
        }
    }

    #[test]
    fn imported_rows_and_assistant_notes_are_not_the_persons_work() {
        assert!(is_own_work("screen"));
        assert!(is_own_work(""));
        assert!(!is_own_work("import:mail"));
        assert!(!is_own_work("import:slack"));
        assert!(!is_own_work(AGENT_NOTE_SOURCE_TYPE));
    }

    #[test]
    fn an_event_needs_every_source_memory_eligible_and_overlap_the_range() {
        let eligible: HashSet<String> = ["m1", "m2"].iter().map(|s| s.to_string()).collect();
        let events = vec![
            event("ok", "m1", &["m2"], 10, 20),
            event("imported-source", "m1", &["m-import"], 10, 20),
            event("hidden-memory", "m3", &[], 10, 20),
            event("too-early", "m1", &[], 0, 5),
            event("too-late", "m1", &[], 100, 200),
            event("straddles", "m2", &[], 0, 12),
        ];
        let kept: Vec<String> = eligible_events(events, &eligible, 10, 50)
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(kept, ["ok", "straddles"]);
    }
}
