//! One-time repair for summaries that were cut inside a token, and for
//! stored summaries that narrate the person (see `reword_narration`).
//!
//! Until 2026-10-06 several call sites ended a sentence at the first period
//! anywhere, so "Reviewed search/hybrid.rs and the reranker" was stored as
//! "Reviewed search/hybrid." (see `summariser::sentences`). New captures are
//! correct; this repairs rows written before the fix.
//!
//! A field is repaired only with evidence from the same row: its longer text
//! (`memory_context`, then `clean_text`) must contain the stored value
//! followed directly by a period and a non-space character. The replacement
//! is that text's full sentence, passed through the cleanup capture uses. No
//! model is called and nothing is invented. Running it twice changes nothing
//! the second time.

use crate::embedding::Embedder;
use crate::memory_embedding_document::refresh_text_vectors;
use crate::storage::{MemoryRecord, Store};
use crate::summariser::narration_filter::{
    clean_or_fallback_display_summary, is_placeholder_summary, narration_filter_hits, neutral_voice,
};
use crate::summariser::sentences::first_sentence;
use serde::Serialize;

const MAX_EXAMPLES: usize = 20;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RepairExample {
    pub memory_id: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct RepairSummary {
    pub dry_run: bool,
    pub scanned: usize,
    /// Rows with a field cut inside a token, a narrated summary or a retired label.
    pub repairable: usize,
    /// Of those, rows whose narrated summary was reworded.
    pub reworded: usize,
    /// Rows whose activity label was brought onto the current list.
    pub relabelled: usize,
    /// Rows rewritten (always 0 in a dry run).
    pub repaired: usize,
    /// Repaired rows whose vectors were refreshed from the new text.
    pub reembedded: usize,
    /// Repaired rows that kept their old vectors because no embedder was
    /// available or it returned a different dimension.
    pub vectors_kept: usize,
    pub examples: Vec<RepairExample>,
}

#[derive(Clone, Copy)]
enum InsightField {
    WhatHappened,
    WhyMattered,
}

impl InsightField {
    fn get(self, record: &MemoryRecord) -> &str {
        match self {
            Self::WhatHappened => &record.insight_what_happened,
            Self::WhyMattered => &record.insight_why_mattered,
        }
    }

    fn get_mut(self, record: &mut MemoryRecord) -> &mut String {
        match self {
            Self::WhatHappened => &mut record.insight_what_happened,
            Self::WhyMattered => &mut record.insight_why_mattered,
        }
    }
}

/// A stored summary without the period capture appended to it.
fn stem_of(stored: &str) -> &str {
    stored.trim().trim_end_matches('.').trim_end()
}

/// The full sentence a cut-off `stored` value came from, or `None` when the
/// row holds no evidence that it was cut inside a token.
fn uncut_sentence(stored: &str, record: &MemoryRecord) -> Option<String> {
    let stem = stem_of(stored);
    if stem.split_whitespace().count() < 2 {
        return None;
    }
    [record.memory_context.as_str(), record.clean_text.as_str()]
        .into_iter()
        .find_map(|source| {
            let start = source.find(stem)?;
            // Spaces may sit between the stored text and the period it was
            // cut at ("SELECT ... FOR" was stored as "SELECT.").
            let mut after = source[start + stem.len()..].trim_start_matches(' ').chars();
            // A letter or digit after the period means a file name, decimal
            // or version; another period means an ellipsis. Punctuation such
            // as ".;" is a real sentence end inside a joined list.
            let cut_inside_token = after.next() == Some('.')
                && after
                    .next()
                    .is_some_and(|next| next.is_alphanumeric() || next == '.');
            cut_inside_token.then(|| first_sentence(&source[start..]).to_string())
        })
}

/// Repairs `record` in place. Returns the before and after of its summary
/// when anything changed.
pub fn repair_record(record: &mut MemoryRecord) -> Option<RepairExample> {
    if record.is_agent_note() {
        return None;
    }
    // The insight rows are derived separately and can be cut even when the
    // summary is whole; the card shows them first.
    let mut insight_change = None;
    for field in [InsightField::WhatHappened, InsightField::WhyMattered] {
        let stored = field.get(record).to_string();
        if let Some(sentence) = uncut_sentence(&stored, record) {
            let repaired = format!("{}.", sentence.trim_end_matches('.'));
            if repaired.len() > stored.trim().len() && repaired.starts_with(stem_of(&stored)) {
                insight_change.get_or_insert(RepairExample {
                    memory_id: record.id.clone(),
                    before: stored,
                    after: repaired.clone(),
                });
                *field.get_mut(record) = repaired;
            }
        }
    }

    let before = record.display_summary.clone();
    let Some(sentence) = uncut_sentence(&record.display_summary, record)
        .or_else(|| uncut_sentence(&record.snippet, record))
    else {
        return insight_change;
    };
    let (repaired, fell_back) = clean_or_fallback_display_summary(
        &sentence,
        &record.window_title,
        record.url.as_deref(),
        record.timestamp,
    );
    // A fallback line ("Viewed X at 3:04 PM") is not the original sentence.
    let stem = stem_of(&before);
    if fell_back || repaired.len() <= before.trim().len() || !repaired.starts_with(stem) {
        return insight_change;
    }

    let same_as_summary = |field: &str| stem_of(field) == stem;
    if same_as_summary(&record.snippet) {
        record.snippet = repaired.clone();
    }
    if same_as_summary(&record.insight_what_happened) {
        record.insight_what_happened = repaired.clone();
    }
    record.display_summary = repaired.clone();
    Some(RepairExample {
        memory_id: record.id.clone(),
        before,
        after: repaired,
    })
}

/// Brings a stored activity label onto the current list. Rows written before
/// a label was retired keep it until something rewrites them. Returns whether
/// the label changed.
pub fn relabel_activity(record: &mut MemoryRecord) -> bool {
    let current = crate::inference::normalize_activity_type(&record.activity_type);
    let changed = current != record.activity_type;
    record.activity_type = current;
    changed
}

const MIN_REWORDED_WORDS: usize = 3;

fn narrates(text: &str) -> bool {
    let lower = text.trim().to_lowercase();
    narration_filter_hits(text) || lower.starts_with("the user") || lower.starts_with("you ")
}

/// Rewords a stored summary that narrates the person ("The user is viewing
/// ...") with the cleanup the display path already applies. Cards showed the
/// cleaned line while the stored text, and so its vector, kept the narration.
/// Returns the before and after when the summary changed.
pub fn reword_narration(record: &mut MemoryRecord) -> Option<RepairExample> {
    if record.is_agent_note() || !narrates(&record.display_summary) {
        return None;
    }
    let before = record.display_summary.clone();
    let after = neutral_voice(&before).trim().to_string();
    if after == before.trim()
        || after.split_whitespace().count() < MIN_REWORDED_WORDS
        || narrates(&after)
        || is_placeholder_summary(&after)
    {
        return None;
    }
    for field in [&mut record.snippet, &mut record.insight_what_happened] {
        if field.trim() == before.trim() {
            *field = after.clone();
        }
    }
    record.display_summary = after.clone();
    Some(RepairExample {
        memory_id: record.id.clone(),
        before,
        after,
    })
}

/// Scan every memory, reword stored narration and repair summaries cut inside
/// a token. With `dry_run` nothing is written and the summary reports what
/// would change.
pub async fn repair_truncated_summaries(
    store: &Store,
    embedder: Option<&Embedder>,
    dry_run: bool,
) -> Result<RepairSummary, String> {
    let records = store
        .list_all_memories()
        .await
        .map_err(|err| err.to_string())?;
    let mut summary = RepairSummary {
        dry_run,
        scanned: records.len(),
        ..Default::default()
    };
    for mut record in records {
        let relabelled = relabel_activity(&mut record);
        summary.relabelled += usize::from(relabelled);
        let reworded = reword_narration(&mut record);
        summary.reworded += usize::from(reworded.is_some());
        let text_change = repair_record(&mut record).or(reworded);
        if text_change.is_none() && !relabelled {
            continue;
        }
        summary.repairable += 1;
        if let Some(example) = text_change.clone() {
            if summary.examples.len() < MAX_EXAMPLES {
                summary.examples.push(example);
            }
        }
        if dry_run {
            continue;
        }
        // The activity label is not part of the embedded text.
        if text_change.is_none() {
            summary.vectors_kept += 1;
        } else if refresh_text_vectors(&mut record, embedder) {
            summary.reembedded += 1;
        } else {
            summary.vectors_kept += 1;
        }
        store
            .replace_memory_preserving_chunks(&record)
            .await
            .map_err(|err| err.to_string())?;
        summary.repaired += 1;
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A row as the old capture path stored it: summary cut at the first period.
    fn cut_row(id: &str, context: &str) -> MemoryRecord {
        let cut = format!("{}.", context.split('.').next().unwrap_or_default());
        MemoryRecord {
            id: id.into(),
            timestamp: 1_790_000_000_000,
            app_name: "Visual Studio Code".into(),
            window_title: "hybrid.rs - fndr".into(),
            memory_context: context.into(),
            snippet: cut.clone(),
            display_summary: cut.clone(),
            insight_what_happened: cut,
            ..Default::default()
        }
    }

    fn narrated_row(summary: &str) -> MemoryRecord {
        MemoryRecord {
            id: "n1".into(),
            timestamp: 1_790_000_000_000,
            app_name: "Numbers".into(),
            window_title: "Q3 forecast".into(),
            snippet: summary.into(),
            display_summary: summary.into(),
            insight_what_happened: summary.into(),
            ..Default::default()
        }
    }

    #[test]
    fn rewords_a_stored_summary_that_narrates_the_person() {
        let mut row = narrated_row("The user is viewing the Q3 forecast with margins by region.");
        let change = reword_narration(&mut row).expect("reworded");
        assert_eq!(
            change.before,
            "The user is viewing the Q3 forecast with margins by region."
        );
        assert!(
            !row.display_summary.to_lowercase().contains("the user"),
            "{}",
            row.display_summary
        );
        assert!(row
            .display_summary
            .contains("Q3 forecast with margins by region"));
        assert_eq!(row.snippet, row.display_summary);
        assert_eq!(row.insight_what_happened, row.display_summary);
        assert!(
            reword_narration(&mut row).is_none(),
            "second pass changes nothing"
        );
    }

    #[test]
    fn rewording_leaves_neutral_rows_and_assistant_notes_alone() {
        let mut neutral = narrated_row("Reviewed the Q3 forecast with margins by region.");
        assert!(reword_narration(&mut neutral).is_none());

        let mut note = narrated_row("The user is viewing the Q3 forecast with margins by region.");
        note.source_type = crate::storage::schema::AGENT_NOTE_SOURCE_TYPE.into();
        assert!(reword_narration(&mut note).is_none());
    }

    #[test]
    fn rewording_never_leaves_a_summary_too_short_to_mean_anything() {
        let mut row = narrated_row("The user is viewing.");
        assert!(reword_narration(&mut row).is_none());
        assert_eq!(row.display_summary, "The user is viewing.");
    }

    #[test]
    fn repairs_a_summary_cut_inside_a_file_name() {
        let mut row = cut_row(
            "m1",
            "Reviewed search/hybrid.rs and the reranker weights. Then ran the gate.",
        );
        assert_eq!(row.display_summary, "Reviewed search/hybrid.");
        let change = repair_record(&mut row).expect("repair");
        assert_eq!(change.before, "Reviewed search/hybrid.");
        assert_eq!(
            change.after,
            "Reviewed search/hybrid.rs and the reranker weights."
        );
        assert_eq!(row.snippet, change.after);
        assert_eq!(row.insight_what_happened, change.after);
        assert!(
            repair_record(&mut row).is_none(),
            "second pass changes nothing"
        );
    }

    #[test]
    fn repairs_a_summary_cut_inside_a_decimal() {
        let mut row = cut_row("m2", "Margin rose from 0.120 to 0.432 over the period.");
        assert_eq!(row.display_summary, "Margin rose from 0.");
        assert_eq!(
            repair_record(&mut row).expect("repair").after,
            "Margin rose from 0.120 to 0.432 over the period."
        );
    }

    #[test]
    fn repairs_insight_rows_cut_inside_a_token_when_the_summary_is_whole() {
        let mut row = cut_row(
            "m7",
            "The current state is FNDR 1.0 at the repo root. The index maps the files.",
        );
        row.display_summary = "Reviewed the FNDR review index.".into();
        row.snippet = row.display_summary.clone();
        row.insight_what_happened = "The current state is FNDR 1.".into();
        row.insight_why_mattered = "The current state is FNDR 1".into();
        let change = repair_record(&mut row).expect("repair");
        assert_eq!(
            change.after,
            "The current state is FNDR 1.0 at the repo root."
        );
        assert_eq!(row.insight_what_happened, change.after);
        assert_eq!(row.insight_why_mattered, change.after);
        assert_eq!(row.display_summary, "Reviewed the FNDR review index.");
        assert!(repair_record(&mut row).is_none());
    }

    #[test]
    fn repairs_a_summary_cut_at_an_ellipsis() {
        let mut row = cut_row(
            "m6",
            "Read about Postgres queues with SELECT ... FOR UPDATE SKIP LOCKED.",
        );
        row.display_summary = "Read about Postgres queues with SELECT .".into();
        row.snippet = "Read about Postgres queues with SELECT".into();
        row.insight_what_happened = row.display_summary.clone();
        assert_eq!(
            repair_record(&mut row).expect("repair").after,
            "Read about Postgres queues with SELECT ... FOR UPDATE SKIP LOCKED."
        );
        assert_eq!(row.snippet, row.display_summary);
    }

    #[test]
    fn leaves_rows_without_evidence_alone() {
        // A real sentence end: the period is followed by a space.
        let mut whole = cut_row(
            "m3",
            "Drafted the essay to 1,450 words. The quote is not in yet.",
        );
        let untouched = whole.clone();
        assert!(repair_record(&mut whole).is_none());
        assert_eq!(whole.display_summary, untouched.display_summary);

        // The summary does not appear in the longer text at all.
        let mut unrelated = cut_row("m4", "Reviewed search/hybrid.rs today.");
        unrelated.display_summary = "Compared two rerankers.".into();
        unrelated.snippet = unrelated.display_summary.clone();
        unrelated.insight_what_happened = unrelated.display_summary.clone();
        assert!(repair_record(&mut unrelated).is_none());

        // A whole sentence followed by ".;" in a joined list is not a cut.
        let mut joined = cut_row("m8", "Kept MiniLM for now.; Considering a later test.");
        joined.display_summary = "Kept MiniLM for now.".into();
        joined.snippet = joined.display_summary.clone();
        joined.insight_what_happened = joined.display_summary.clone();
        assert!(repair_record(&mut joined).is_none());

        let mut note = cut_row("m5", "Reviewed search/hybrid.rs today.");
        note.source_type = "agent".into();
        assert!(repair_record(&mut note).is_none());
    }

    #[test]
    fn a_retired_activity_label_becomes_unknown_and_a_current_one_stays() {
        let mut row = narrated_row("Reviewed the Q3 forecast with margins by region.");
        row.activity_type = "screen_review".into();
        assert!(relabel_activity(&mut row));
        assert_eq!(row.activity_type, "unknown");
        assert!(!relabel_activity(&mut row), "second pass changes nothing");

        row.activity_type = "debugging".into();
        assert!(!relabel_activity(&mut row));
        assert_eq!(row.activity_type, "debugging");
    }

    #[tokio::test]
    async fn dry_run_writes_nothing_and_apply_rewrites_only_cut_rows() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).unwrap())
            .await
            .unwrap();
        let cut = cut_row("cut", "Reviewed search/hybrid.rs and the reranker weights.");
        let whole = cut_row("whole", "Drafted the essay to 1,450 words. More later.");
        store
            .add_batch_preserving_ids(&[cut, whole.clone()])
            .await
            .unwrap();

        let dry = repair_truncated_summaries(&store, None, true)
            .await
            .unwrap();
        assert_eq!((dry.scanned, dry.repairable, dry.repaired), (2, 1, 0));
        let stored = store.get_memory_by_id("cut").await.unwrap().unwrap();
        assert_eq!(stored.display_summary, "Reviewed search/hybrid.");

        let before_whole = store.get_memory_by_id("whole").await.unwrap().unwrap();
        let applied = repair_truncated_summaries(&store, None, false)
            .await
            .unwrap();
        assert_eq!((applied.repaired, applied.vectors_kept), (1, 1));
        let stored = store.get_memory_by_id("cut").await.unwrap().unwrap();
        assert_eq!(
            stored.display_summary,
            "Reviewed search/hybrid.rs and the reranker weights."
        );
        assert!(stored.embedding_text.contains("hybrid.rs"));
        let after_whole = store.get_memory_by_id("whole").await.unwrap().unwrap();
        assert_eq!(
            serde_json::to_string(&after_whole).unwrap(),
            serde_json::to_string(&before_whole).unwrap(),
            "a row without evidence must not change at all"
        );

        let again = repair_truncated_summaries(&store, None, false)
            .await
            .unwrap();
        assert_eq!(again.repairable, 0, "the repair is idempotent");
    }
}
