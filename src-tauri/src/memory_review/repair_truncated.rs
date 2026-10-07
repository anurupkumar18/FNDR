//! One-time repair for summaries that were cut inside a token.
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
use crate::memory_embedding_document::compose_memory_embedding_document;
use crate::storage::{MemoryRecord, Store};
use crate::summariser::narration_filter::clean_or_fallback_display_summary;
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
    /// Rows with at least one field cut inside a token.
    pub repairable: usize,
    /// Rows rewritten (always 0 in a dry run).
    pub repaired: usize,
    /// Repaired rows whose vectors were refreshed from the new text.
    pub reembedded: usize,
    /// Repaired rows that kept their old vectors because no embedder was
    /// available or it returned a different dimension.
    pub vectors_kept: usize,
    pub examples: Vec<RepairExample>,
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
            let cut_inside_token =
                after.next() == Some('.') && after.next().is_some_and(|next| !next.is_whitespace());
            cut_inside_token.then(|| first_sentence(&source[start..]).to_string())
        })
}

/// Repairs `record` in place. Returns the before and after of its summary
/// when anything changed.
pub fn repair_record(record: &mut MemoryRecord) -> Option<RepairExample> {
    if record.is_agent_note() {
        return None;
    }
    let before = record.display_summary.clone();
    let sentence = uncut_sentence(&record.display_summary, record)
        .or_else(|| uncut_sentence(&record.snippet, record))?;
    let (repaired, fell_back) = clean_or_fallback_display_summary(
        &sentence,
        &record.window_title,
        record.url.as_deref(),
        record.timestamp,
    );
    // A fallback line ("Viewed X at 3:04 PM") is not the original sentence.
    let stem = stem_of(&before);
    if fell_back || repaired.len() <= before.trim().len() || !repaired.starts_with(stem) {
        return None;
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

/// Recompose the embedding text and refresh the primary and snippet vectors
/// the way capture writes them: with the app and window as chunking context.
/// Returns false, leaving the vectors as they were, when that is not possible.
fn reembed(record: &mut MemoryRecord, embedder: Option<&Embedder>) -> bool {
    let document = compose_memory_embedding_document(record, None);
    record.embedding_text = document.primary_text.clone();
    let Some(embedder) = embedder else {
        return false;
    };
    let texts = [document.primary_text, document.snippet_text]
        .map(|text| (record.app_name.clone(), record.window_title.clone(), text));
    match embedder.embed_batch_with_context(&texts) {
        Ok(vectors)
            if vectors.len() == 2
                && vectors[0].len() == record.embedding.len()
                && vectors[1].len() == record.snippet_embedding.len() =>
        {
            let mut vectors = vectors.into_iter();
            record.embedding = vectors.next().unwrap_or_default();
            record.snippet_embedding = vectors.next().unwrap_or_default();
            true
        }
        _ => false,
    }
}

/// Scan every memory and repair summaries cut inside a token. With `dry_run`
/// nothing is written and the summary reports what would change.
pub async fn repair_truncated_summaries(
    store: &Store,
    embedder: Option<&Embedder>,
    dry_run: bool,
) -> Result<RepairSummary, String> {
    let records = store.list_all_memories().await.map_err(|err| err.to_string())?;
    let mut summary = RepairSummary {
        dry_run,
        scanned: records.len(),
        ..Default::default()
    };
    for mut record in records {
        let Some(example) = repair_record(&mut record) else {
            continue;
        };
        summary.repairable += 1;
        if summary.examples.len() < MAX_EXAMPLES {
            summary.examples.push(example);
        }
        if dry_run {
            continue;
        }
        if reembed(&mut record, embedder) {
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

    #[test]
    fn repairs_a_summary_cut_inside_a_file_name() {
        let mut row = cut_row("m1", "Reviewed search/hybrid.rs and the reranker weights. Then ran the gate.");
        assert_eq!(row.display_summary, "Reviewed search/hybrid.");
        let change = repair_record(&mut row).expect("repair");
        assert_eq!(change.before, "Reviewed search/hybrid.");
        assert_eq!(change.after, "Reviewed search/hybrid.rs and the reranker weights.");
        assert_eq!(row.snippet, change.after);
        assert_eq!(row.insight_what_happened, change.after);
        assert!(repair_record(&mut row).is_none(), "second pass changes nothing");
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
    fn repairs_a_summary_cut_at_an_ellipsis() {
        let mut row = cut_row("m6", "Read about Postgres queues with SELECT ... FOR UPDATE SKIP LOCKED.");
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
        let mut whole = cut_row("m3", "Drafted the essay to 1,450 words. The quote is not in yet.");
        let untouched = whole.clone();
        assert!(repair_record(&mut whole).is_none());
        assert_eq!(whole.display_summary, untouched.display_summary);

        // The summary does not appear in the longer text at all.
        let mut unrelated = cut_row("m4", "Reviewed search/hybrid.rs today.");
        unrelated.display_summary = "Compared two rerankers.".into();
        unrelated.snippet = unrelated.display_summary.clone();
        assert!(repair_record(&mut unrelated).is_none());

        let mut note = cut_row("m5", "Reviewed search/hybrid.rs today.");
        note.source_type = "agent".into();
        assert!(repair_record(&mut note).is_none());
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
        store.add_batch_preserving_ids(&[cut, whole.clone()]).await.unwrap();

        let dry = repair_truncated_summaries(&store, None, true).await.unwrap();
        assert_eq!((dry.scanned, dry.repairable, dry.repaired), (2, 1, 0));
        let stored = store.get_memory_by_id("cut").await.unwrap().unwrap();
        assert_eq!(stored.display_summary, "Reviewed search/hybrid.");

        let before_whole = store.get_memory_by_id("whole").await.unwrap().unwrap();
        let applied = repair_truncated_summaries(&store, None, false).await.unwrap();
        assert_eq!((applied.repaired, applied.vectors_kept), (1, 1));
        let stored = store.get_memory_by_id("cut").await.unwrap().unwrap();
        assert_eq!(stored.display_summary, "Reviewed search/hybrid.rs and the reranker weights.");
        assert!(stored.embedding_text.contains("hybrid.rs"));
        let after_whole = store.get_memory_by_id("whole").await.unwrap().unwrap();
        assert_eq!(
            serde_json::to_string(&after_whole).unwrap(),
            serde_json::to_string(&before_whole).unwrap(),
            "a row without evidence must not change at all"
        );

        let again = repair_truncated_summaries(&store, None, false).await.unwrap();
        assert_eq!(again.repairable, 0, "the repair is idempotent");
    }
}
