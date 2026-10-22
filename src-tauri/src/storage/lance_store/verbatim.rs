//! Verbatim page text kept beside a memory (ADR 036).
//!
//! One row per memory in `memory_verbatim_v1`: the cleaned text the capture
//! saw, capped, with no vector and no full-text index. Nothing here feeds
//! search or ranking. A consumer that needs an exact value asks for it by
//! memory id, through `context_runtime::verbatim_text_for_memory`.
//!
//! Every path that removes a memory removes its row (`delete_memory_by_id`,
//! `delete_memories_matching`, `delete_memories_by_domain`, `delete_older_than`,
//! `delete_all`), and erasure prunes this table's old versions too.

use super::*;
use arrow_schema::{DataType, Field, Schema};

pub const MEMORY_VERBATIM_TABLE: &str = "memory_verbatim_v1";

/// One memory's stored text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerbatimEntry {
    pub memory_id: String,
    pub text: String,
    /// The memory's own capture time in milliseconds; retention counts from it.
    pub captured_at: i64,
}

/// What the table holds, for the budget and the Settings screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VerbatimStats {
    pub rows: usize,
    pub bytes: u64,
}

fn verbatim_schema() -> Schema {
    Schema::new(vec![
        Field::new("memory_id", DataType::Utf8, false),
        Field::new("text", DataType::Utf8, false),
        Field::new("byte_len", DataType::Int64, false),
        Field::new("captured_at", DataType::Int64, false),
    ])
}

fn entries_to_batch(entries: &[VerbatimEntry]) -> Result<RecordBatch, arrow_schema::ArrowError> {
    let ids: Vec<&str> = entries.iter().map(|e| e.memory_id.as_str()).collect();
    let texts: Vec<&str> = entries.iter().map(|e| e.text.as_str()).collect();
    let lens: Vec<i64> = entries.iter().map(|e| e.text.len() as i64).collect();
    let captured: Vec<i64> = entries.iter().map(|e| e.captured_at).collect();
    RecordBatch::try_new(
        Arc::new(verbatim_schema()),
        vec![
            Arc::new(StringArray::from(ids)),
            Arc::new(StringArray::from(texts)),
            Arc::new(Int64Array::from(lens)),
            Arc::new(Int64Array::from(captured)),
        ],
    )
}

pub(super) async fn open_verbatim_table(db_path: &Path) -> Result<Table, lancedb::Error> {
    let uri = db_path.to_string_lossy();
    let conn = lancedb::connect(&uri).execute().await?;
    let names = conn.table_names().execute().await?;
    open_or_create_named_table(
        &conn,
        &names,
        MEMORY_VERBATIM_TABLE,
        Arc::new(verbatim_schema()),
    )
    .await
}

impl Store {
    /// Store or replace the verbatim text of each entry's memory. Callers cap
    /// the text and run the privacy checks first; this only writes.
    pub async fn put_verbatim(
        &self,
        entries: &[VerbatimEntry],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let entries: Vec<VerbatimEntry> = entries
            .iter()
            .filter(|e| !e.memory_id.trim().is_empty() && !e.text.trim().is_empty())
            .cloned()
            .collect();
        if entries.is_empty() {
            return Ok(());
        }
        let batch = entries_to_batch(&entries)?;
        let iter = RecordBatchIterator::new(vec![Ok(batch)], Arc::new(verbatim_schema()));
        let mut merge = self.verbatim_table.merge_insert(&["memory_id"]);
        merge
            .when_matched_update_all(None)
            .when_not_matched_insert_all();
        merge
            .execute(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
            .await?;
        Ok(())
    }

    /// The stored text for a memory id with no visibility check. Only the
    /// capture merge path and `context_runtime::verbatim_text_for_memory` call
    /// this; a surface that shows text goes through the latter.
    pub async fn verbatim_text_unchecked(
        &self,
        memory_id: &str,
    ) -> Result<Option<String>, Box<dyn std::error::Error>> {
        let batches: Vec<RecordBatch> = self
            .verbatim_table
            .query()
            .only_if(format!("memory_id = '{}'", sql_escape(memory_id)))
            .execute()
            .await?
            .try_collect()
            .await?;
        for batch in &batches {
            let texts = str_col(batch, "text");
            if batch.num_rows() > 0 {
                let text = get_str(&texts, 0);
                if !text.is_empty() {
                    return Ok(Some(text));
                }
            }
        }
        Ok(None)
    }

    /// Which of `ids` are rows of the memories table right now.
    pub async fn persisted_memory_ids(
        &self,
        ids: &[String],
    ) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
        let mut found = HashSet::new();
        for id_batch in ids.chunks(128) {
            if let Some(filter) = build_string_match_filter("id", id_batch) {
                found.extend(self.memory_ids_for_filter(&filter).await?);
            }
        }
        Ok(found)
    }

    pub async fn delete_verbatim_for_memories(
        &self,
        memory_ids: &[String],
    ) -> Result<(), Box<dyn std::error::Error>> {
        for id_batch in memory_ids.chunks(128) {
            if let Some(filter) = build_string_match_filter("memory_id", id_batch) {
                self.verbatim_table.delete(&filter).await?;
            }
        }
        Ok(())
    }

    /// Remove every stored text (the switch was turned off, or all data wiped).
    pub async fn delete_all_verbatim(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.verbatim_table.delete("memory_id IS NOT NULL").await?;
        Ok(())
    }

    /// Remove text of memories captured before `cutoff_ms`; the memories stay.
    pub async fn delete_verbatim_older_than(
        &self,
        cutoff_ms: i64,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let before = self.verbatim_table.count_rows(None).await?;
        self.verbatim_table
            .delete(&format!("captured_at < {cutoff_ms}"))
            .await?;
        let after = self.verbatim_table.count_rows(None).await?;
        Ok(before.saturating_sub(after))
    }

    pub async fn verbatim_stats(&self) -> Result<VerbatimStats, Box<dyn std::error::Error>> {
        let batches: Vec<RecordBatch> = self
            .verbatim_table
            .query()
            .select(Select::Columns(vec!["byte_len".to_string()]))
            .execute()
            .await?
            .try_collect()
            .await?;
        let mut stats = VerbatimStats::default();
        for batch in &batches {
            if let Some(lens) = i64_col(batch, "byte_len") {
                for row in 0..batch.num_rows() {
                    stats.rows += 1;
                    stats.bytes += lens.value(row).max(0) as u64;
                }
            }
        }
        Ok(stats)
    }

    /// Delete the oldest text until the table holds at most `max_bytes`.
    /// Returns the rows removed.
    pub async fn enforce_verbatim_budget(
        &self,
        max_bytes: u64,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let batches: Vec<RecordBatch> = self
            .verbatim_table
            .query()
            .select(Select::Columns(vec![
                "memory_id".to_string(),
                "byte_len".to_string(),
                "captured_at".to_string(),
            ]))
            .execute()
            .await?
            .try_collect()
            .await?;
        let mut rows: Vec<(i64, String, u64)> = Vec::new();
        for batch in &batches {
            let ids = str_col(batch, "memory_id");
            let lens = i64_col(batch, "byte_len");
            let times = i64_col(batch, "captured_at");
            let (Some(lens), Some(times)) = (lens, times) else {
                continue;
            };
            for row in 0..batch.num_rows() {
                rows.push((
                    times.value(row),
                    get_str(&ids, row),
                    lens.value(row).max(0) as u64,
                ));
            }
        }
        let mut total: u64 = rows.iter().map(|r| r.2).sum();
        if total <= max_bytes {
            return Ok(0);
        }
        rows.sort();
        let mut evict = Vec::new();
        for (_, id, len) in rows {
            if total <= max_bytes {
                break;
            }
            total = total.saturating_sub(len);
            evict.push(id);
        }
        self.delete_verbatim_for_memories(&evict).await?;
        Ok(evict.len())
    }

    /// Fold new rows into the table's files and drop deleted rows' old
    /// versions. Used by erasure, where a deleted row must not survive in a
    /// previous table version.
    pub(super) async fn prune_verbatim_versions(
        &self,
        older_than_days: i64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.verbatim_table
            .optimize(OptimizeAction::Compact {
                options: CompactionOptions::default(),
                remap_options: None,
            })
            .await?;
        self.verbatim_table
            .optimize(OptimizeAction::Prune {
                older_than: Some(chrono::Duration::try_days(older_than_days).unwrap_or_default()),
                delete_unverified: Some(true),
                error_if_tagged_old_versions: Some(false),
            })
            .await?;
        Ok(())
    }

    /// Compact and prune the verbatim table after a sweep removed rows.
    pub async fn reclaim_verbatim_space(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.prune_verbatim_versions(0).await
    }
}
