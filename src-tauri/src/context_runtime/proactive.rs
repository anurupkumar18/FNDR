//! The "From your memory" toast: an older memory that looks like what is on
//! screen now. It goes through the same visibility rule as every other
//! surface and shows the line a card would show.

use crate::storage::{MemoryRecord, SearchResult};
use crate::summariser::narration_filter::{
    clean_or_fallback_display_summary, is_placeholder_summary,
};
use crate::ProactiveSuggestion;
use std::collections::HashMap;

/// A memory newer than this is the work in front of the person, not a recall.
/// It matches the Vault's session gap.
pub const MIN_RECALL_AGE_MS: i64 = 30 * 60 * 1000;

/// The first hit worth a toast, or none. A hit qualifies when it is similar
/// enough, was not shown before, is still visible under the current blocklist
/// and quality rules, is older than the current session, and has a real
/// sentence to show. Raw screen text is never shown in its place.
pub fn pick_suggestion(
    hits: &[SearchResult],
    records: &HashMap<String, MemoryRecord>,
    blocklist: &[String],
    similarity_threshold: f32,
    already_shown: impl Fn(&str) -> bool,
    now_ms: i64,
) -> Option<ProactiveSuggestion> {
    hits.iter().find_map(|hit| {
        if hit.score <= similarity_threshold || already_shown(&hit.id) {
            return None;
        }
        let record = records.get(&hit.id)?;
        if !super::retrieve::memory_is_visible(record, blocklist)
            || now_ms - record.timestamp < MIN_RECALL_AGE_MS
            || is_placeholder_summary(&record.display_summary)
        {
            return None;
        }
        let (line, fell_back) = clean_or_fallback_display_summary(
            &record.display_summary,
            &record.window_title,
            record.url.as_deref(),
            record.timestamp,
        );
        if fell_back || is_placeholder_summary(&line) {
            return None;
        }
        Some(ProactiveSuggestion {
            memory_id: hit.id.clone(),
            snippet: line,
            similarity: hit.score,
            task_title: None,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000_000;
    const HOUR: i64 = 60 * 60 * 1000;

    fn hit(id: &str, score: f32) -> SearchResult {
        SearchResult {
            id: id.to_string(),
            score,
            ..Default::default()
        }
    }

    fn record(id: &str, age_ms: i64, summary: &str) -> MemoryRecord {
        MemoryRecord {
            id: id.to_string(),
            timestamp: NOW - age_ms,
            app_name: "Pages".to_string(),
            window_title: "Lab 5 report".to_string(),
            display_summary: summary.to_string(),
            clean_text: "Parallel loops. Timing table with four threads and a speedup of 3.1 on the lab machine. \
                         Conclusion paragraph drafted with the measured results and the limits of the method."
                .to_string(),
            ..Default::default()
        }
    }

    fn pick(
        hits: &[SearchResult],
        rows: Vec<MemoryRecord>,
        blocklist: &[String],
    ) -> Option<ProactiveSuggestion> {
        let records = rows.into_iter().map(|row| (row.id.clone(), row)).collect();
        pick_suggestion(hits, &records, blocklist, 0.82, |id| id == "shown", NOW)
    }

    const SENTENCE: &str =
        "Measured the parallel loop with four threads and recorded a speedup of 3.1.";

    #[test]
    fn an_older_visible_memory_with_a_real_sentence_is_suggested() {
        let suggestion =
            pick(&[hit("a", 0.9)], vec![record("a", 3 * HOUR, SENTENCE)], &[]).unwrap();
        assert_eq!(suggestion.memory_id, "a");
        assert_eq!(suggestion.snippet, SENTENCE);
    }

    #[test]
    fn the_capture_from_this_session_is_not_a_recall() {
        assert!(pick(&[hit("a", 0.99)], vec![record("a", 60_000, SENTENCE)], &[]).is_none());
    }

    #[test]
    fn a_memory_from_an_app_blocked_since_is_never_shown() {
        let blocked = ["Pages".to_string()];
        assert!(pick(
            &[hit("a", 0.9)],
            vec![record("a", 3 * HOUR, SENTENCE)],
            &blocked
        )
        .is_none());
    }

    #[test]
    fn a_deleted_or_missing_memory_is_never_shown() {
        let mut deleted = record("a", 3 * HOUR, SENTENCE);
        deleted.is_soft_deleted = true;
        assert!(pick(&[hit("a", 0.9)], vec![deleted], &[]).is_none());
        assert!(pick(&[hit("gone", 0.9)], vec![], &[]).is_none());
    }

    #[test]
    fn a_placeholder_summary_is_skipped_for_the_next_hit() {
        let rows = vec![
            record("a", 3 * HOUR, "Viewed Lab 5 report at 03:14 PM."),
            record("b", 4 * HOUR, SENTENCE),
        ];
        let suggestion = pick(&[hit("a", 0.95), hit("b", 0.9)], rows, &[]).unwrap();
        assert_eq!(suggestion.memory_id, "b");
    }

    #[test]
    fn narration_is_reworded_before_it_is_shown() {
        let rows = vec![record(
            "a",
            3 * HOUR,
            "The user measured the parallel loop with four threads.",
        )];
        let suggestion = pick(&[hit("a", 0.9)], rows, &[]).unwrap();
        assert!(
            !suggestion.snippet.to_lowercase().contains("the user"),
            "{}",
            suggestion.snippet
        );
    }

    #[test]
    fn a_weak_or_already_shown_hit_is_not_suggested() {
        let rows = || {
            vec![
                record("a", 3 * HOUR, SENTENCE),
                record("shown", 3 * HOUR, SENTENCE),
            ]
        };
        assert!(pick(&[hit("a", 0.5)], rows(), &[]).is_none());
        assert!(pick(&[hit("shown", 0.9)], rows(), &[]).is_none());
    }
}
