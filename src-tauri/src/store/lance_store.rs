//! LanceDB-backed storage for FNDR memory records.
//!
//! Replaces the JSON-based simple_store with a proper vector database.
//! All methods that touch LanceDB are async.

use super::schema::{
    AppCount, DayCount, DaypartCount, DomainCount, EdgeType, GraphEdge, GraphNode, HourCount,
    MeetingSegment, MeetingSession, MemoryRecord, NodeType, SearchResult, Stats, Task, TaskType,
    WeekdayCount,
};
use crate::memory_compaction::{build_lexical_shadow, compact_memory_record_payload};
use arrow_array::{
    builder::{Int64Builder, StringBuilder},
    Array, BooleanArray, FixedSizeListArray, Float32Array, Int64Array, RecordBatch,
    RecordBatchIterator, RecordBatchReader, StringArray, UInt32Array,
};
use arrow_schema::{ArrowError, DataType, Field, Schema};
use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase, Select};
use lancedb::table::{AddDataMode, NewColumnTransform};
use lancedb::{Connection, Table};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const MEMORIES_TABLE: &str = "memories";
pub const TASKS_TABLE: &str = "tasks";
pub const MEETINGS_TABLE: &str = "meetings";
pub const SEGMENTS_TABLE: &str = "segments";
pub const NODES_TABLE: &str = "knowledge_nodes";
pub const EDGES_TABLE: &str = "knowledge_edges";
const SEARCH_RESULT_COLUMNS: &[&str] = &[
    "id",
    "timestamp",
    "app_name",
    "bundle_id",
    "window_title",
    "session_id",
    "text",
    "clean_text",
    "ocr_confidence",
    "ocr_block_count",
    "snippet",
    "summary_source",
    "noise_score",
    "session_key",
    "lexical_shadow",
    "screenshot_path",
    "url",
    "decay_score",
];
const TEXT_EMBED_DIM: i32 = 384;
const IMAGE_EMBED_DIM: i32 = 512;
const VECTOR_QUERY_MULTIPLIER: usize = 3;
const KEYWORD_QUERY_MULTIPLIER: usize = 8;
const MAX_KEYWORD_SCAN: usize = 600;
const INDEX_NOISE_HOSTS: &[&str] = &[
    "accounts.google.com",
    "auth.openai.com",
    "idmsa.apple.com",
    "login.live.com",
    "login.microsoftonline.com",
];

/// LanceDB-backed store for memory records.
pub struct Store {
    data_dir: PathBuf,
    table: Table,
    tasks_table: Table,
    meetings_table: Table,
    segments_table: Table,
    nodes_table: Table,
    edges_table: Table,
}

impl Store {
    /// Open (or create) the LanceDB store at `data_dir`.
    ///
    /// This is synchronous — it spins up a temporary Tokio runtime for
    /// initialization so it can be called from non-async contexts (e.g.
    /// the Tauri `setup()` callback).
    pub fn new(data_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = data_dir.to_path_buf();
        let db_path = data_dir.join("lancedb");
        std::fs::create_dir_all(&db_path)?;

        // Temporary single-threaded runtime for initialization only.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        let (table, tasks_table, meetings_table, segments_table, nodes_table, edges_table) =
            rt.block_on(open_all_tables(&db_path))?;

        // Migrate legacy storages if present.
        let memories_json = data_dir.join("memories.json");
        if memories_json.exists() {
            rt.block_on(migrate_from_json(&table, &memories_json));
        }

        let tasks_json = data_dir.join("tasks.json");
        if tasks_json.exists() {
            rt.block_on(migrate_tasks_from_json(&tasks_table, &tasks_json));
        }

        let meetings_json = data_dir.join("meetings/meetings.json");
        if meetings_json.exists() {
            rt.block_on(migrate_meetings_from_json(&meetings_table, &meetings_json));
        }

        let segments_json = data_dir.join("meetings/segments.json");
        if segments_json.exists() {
            rt.block_on(migrate_segments_from_json(&segments_table, &segments_json));
        }

        let graph_json = data_dir.join("memory_graph.json");
        if graph_json.exists() {
            rt.block_on(migrate_graph_from_json(
                &nodes_table,
                &edges_table,
                &graph_json,
            ));
        }

        Ok(Self {
            data_dir,
            table,
            tasks_table,
            meetings_table,
            segments_table,
            nodes_table,
            edges_table,
        })
    }

    pub async fn upsert_tasks(&self, tasks: &[Task]) -> Result<(), Box<dyn std::error::Error>> {
        let batch = task_to_batch(tasks)?;
        let schema = Arc::new(task_schema());
        let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.tasks_table
            .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
            .mode(AddDataMode::Overwrite)
            .execute()
            .await?;
        Ok(())
    }

    pub async fn list_tasks(&self) -> Result<Vec<Task>, Box<dyn std::error::Error>> {
        let batches = self
            .tasks_table
            .query()
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        let mut results = Vec::new();
        for b in batches {
            results.extend(batch_to_tasks(&b));
        }
        Ok(results)
    }

    pub async fn upsert_meetings(
        &self,
        meetings: &[MeetingSession],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let batch = meeting_to_batch(meetings)?;
        let schema = Arc::new(meeting_schema());
        let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.meetings_table
            .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
            .mode(AddDataMode::Overwrite)
            .execute()
            .await?;
        Ok(())
    }

    pub async fn upsert_segments(
        &self,
        segments: &[MeetingSegment],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let batch = segment_to_batch(segments)?;
        let schema = Arc::new(segment_schema());
        let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.segments_table
            .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
            .mode(AddDataMode::Append)
            .execute()
            .await?;
        Ok(())
    }

    pub async fn get_all_nodes(&self) -> Result<Vec<GraphNode>, Box<dyn std::error::Error>> {
        let batches = self
            .nodes_table
            .query()
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        let mut results = Vec::new();
        for b in batches {
            results.extend(batch_to_nodes(&b));
        }
        Ok(results)
    }

    pub async fn get_all_edges(&self) -> Result<Vec<GraphEdge>, Box<dyn std::error::Error>> {
        let batches = self
            .edges_table
            .query()
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        let mut results = Vec::new();
        for b in batches {
            results.extend(batch_to_edges(&b));
        }
        Ok(results)
    }

    pub async fn upsert_nodes(
        &self,
        nodes: &[GraphNode],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Read existing nodes, merge by id (new wins), write back everything.
        let mut existing = self.get_all_nodes().await.unwrap_or_default();
        let mut by_id: std::collections::HashMap<String, GraphNode> =
            existing.drain(..).map(|n| (n.id.clone(), n)).collect();
        for n in nodes {
            by_id.insert(n.id.clone(), n.clone());
        }
        let all: Vec<GraphNode> = by_id.into_values().collect();
        let batch = nodes_to_batch(&all)?;
        let schema = Arc::new(node_schema());
        let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.nodes_table
            .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
            .mode(AddDataMode::Overwrite)
            .execute()
            .await?;
        Ok(())
    }

    pub async fn upsert_segments_full(
        &self,
        segments: &[MeetingSegment],
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.segments_table.delete("id IS NOT NULL").await?;
        if segments.is_empty() {
            return Ok(());
        }
        let batch = segment_to_batch(segments)?;
        let schema = Arc::new(segment_schema());
        let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.segments_table
            .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
            .mode(AddDataMode::Overwrite)
            .execute()
            .await?;
        Ok(())
    }

    pub async fn list_meetings(&self) -> Result<Vec<MeetingSession>, Box<dyn std::error::Error>> {
        let batches = self
            .meetings_table
            .query()
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        let mut meetings = Vec::new();
        for batch in batches {
            meetings.extend(batch_to_meetings(&batch));
        }
        Ok(meetings)
    }

    pub async fn list_segments(&self) -> Result<Vec<MeetingSegment>, Box<dyn std::error::Error>> {
        let batches = self
            .segments_table
            .query()
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        let mut segments = Vec::new();
        for batch in batches {
            segments.extend(batch_to_segments(&batch));
        }
        Ok(segments)
    }

    pub async fn upsert_edges(
        &self,
        edges: &[GraphEdge],
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Read existing edges, deduplicate by (source, target, edge_type) to avoid
        // accumulating redundant edges, then write back the full set.
        let mut existing = self.get_all_edges().await.unwrap_or_default();

        // Dedup key: (source, target, edge_type string). New edges win (they replace
        // an old edge with the same relationship).
        let mut by_rel: std::collections::HashMap<(String, String, String), GraphEdge> = existing
            .drain(..)
            .map(|e| {
                let key = (
                    e.source.clone(),
                    e.target.clone(),
                    format!("{:?}", e.edge_type),
                );
                (key, e)
            })
            .collect();

        for e in edges {
            let key = (
                e.source.clone(),
                e.target.clone(),
                format!("{:?}", e.edge_type),
            );
            by_rel.insert(key, e.clone());
        }

        let all: Vec<GraphEdge> = by_rel.into_values().collect();
        let batch = edges_to_batch(&all)?;
        let schema = Arc::new(edge_schema());
        let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.edges_table
            .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
            .mode(AddDataMode::Overwrite)
            .execute()
            .await?;
        Ok(())
    }

    /// Return the data directory (sync — no DB access).
    pub fn data_dir(&self) -> PathBuf {
        self.data_dir.clone()
    }

    /// Insert a batch of records into LanceDB.
    pub async fn add_batch(
        &self,
        records: &[MemoryRecord],
    ) -> Result<(), Box<dyn std::error::Error>> {
        if records.is_empty() {
            return Ok(());
        }
        let normalized = records
            .iter()
            .map(normalize_record_for_index)
            .collect::<Vec<_>>();
        let compacted = dedup_records_for_insert(&normalized);
        if compacted.is_empty() {
            return Ok(());
        }
        self.insert_memory_batch(&compacted).await
    }

    /// Insert a batch without content-based deduping, preserving caller-provided ids.
    pub async fn add_batch_preserving_ids(
        &self,
        records: &[MemoryRecord],
    ) -> Result<(), Box<dyn std::error::Error>> {
        if records.is_empty() {
            return Ok(());
        }
        let normalized = records
            .iter()
            .map(normalize_record_for_index)
            .collect::<Vec<_>>();
        self.insert_memory_batch(&normalized).await
    }

    /// Replace the entire memories table in one write, preserving caller ids.
    pub async fn replace_all_memories_preserving_ids(
        &self,
        records: &[MemoryRecord],
    ) -> Result<(), Box<dyn std::error::Error>> {
        if records.is_empty() {
            self.delete_all().await?;
            return Ok(());
        }

        let normalized = records
            .iter()
            .map(normalize_record_for_index)
            .collect::<Vec<_>>();
        let batch = records_to_batch(&normalized)?;
        let schema = Arc::new(memory_schema());
        let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.table
            .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
            .mode(AddDataMode::Overwrite)
            .execute()
            .await?;
        Ok(())
    }

    /// Approximate nearest-neighbour search over `embedding` column.
    pub async fn vector_search(
        &self,
        query_embedding: &[f32],
        limit: usize,
        time_filter: Option<&str>,
        app_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        let filter = build_filter(time_filter, app_filter);
        let query_vec: Vec<f32> = query_embedding.to_vec();
        let base_limit = limit.max(1);
        let retrieval_limit = if base_limit >= 300 {
            base_limit
        } else {
            base_limit.saturating_mul(VECTOR_QUERY_MULTIPLIER).min(300)
        };

        let mut vq = self
            .table
            .vector_search(query_vec)?
            .column("embedding")
            .limit(retrieval_limit);

        if let Some(f) = filter {
            vq = vq.only_if(f);
        }

        let batches: Vec<RecordBatch> = vq.execute().await?.try_collect().await?;
        let mut results = Vec::new();
        for batch in &batches {
            results.extend(batch_to_search_results(batch));
        }
        Ok(dedup_search_results(results, limit))
    }

    /// ANN search over the `snippet_embedding` column (second semantic tower).
    pub async fn snippet_vector_search(
        &self,
        query_embedding: &[f32],
        limit: usize,
        time_filter: Option<&str>,
        app_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        let filter = build_filter(time_filter, app_filter);
        let query_vec: Vec<f32> = query_embedding.to_vec();
        let base_limit = limit.max(1);
        let retrieval_limit = if base_limit >= 300 {
            base_limit
        } else {
            base_limit.saturating_mul(VECTOR_QUERY_MULTIPLIER).min(300)
        };

        let mut vq = self
            .table
            .vector_search(query_vec)?
            .column("snippet_embedding")
            .limit(retrieval_limit);

        if let Some(f) = filter {
            vq = vq.only_if(f);
        }

        let batches: Vec<RecordBatch> = vq.execute().await?.try_collect().await?;
        let mut results = Vec::new();
        for batch in &batches {
            results.extend(batch_to_search_results(batch));
        }
        Ok(dedup_search_results(results, limit))
    }

    /// ANN search over the `support_embedding` column (representative chunk centroid tower).
    pub async fn support_vector_search(
        &self,
        query_embedding: &[f32],
        limit: usize,
        time_filter: Option<&str>,
        app_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        let filter = build_filter(time_filter, app_filter);
        let query_vec: Vec<f32> = query_embedding.to_vec();
        let base_limit = limit.max(1);
        let retrieval_limit = if base_limit >= 300 {
            base_limit
        } else {
            base_limit.saturating_mul(VECTOR_QUERY_MULTIPLIER).min(300)
        };

        let mut vq = self
            .table
            .vector_search(query_vec)?
            .column("support_embedding")
            .limit(retrieval_limit);

        if let Some(f) = filter {
            vq = vq.only_if(f);
        }

        let batches: Vec<RecordBatch> = vq.execute().await?.try_collect().await?;
        let mut results = Vec::new();
        for batch in &batches {
            results.extend(batch_to_search_results(batch));
        }
        Ok(dedup_search_results(results, limit))
    }

    async fn insert_memory_batch(
        &self,
        records: &[MemoryRecord],
    ) -> Result<(), Box<dyn std::error::Error>> {
        if records.is_empty() {
            return Ok(());
        }
        let batch = records_to_batch(records)?;
        let schema = Arc::new(memory_schema());
        let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.table
            .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
            .execute()
            .await?;
        Ok(())
    }

    /// Asynchronously update snippet + summary_source for a single record (post-LLM).
    pub async fn update_snippet(
        &self,
        id: &str,
        snippet: &str,
        source: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let escaped_id = sql_escape(id);
        let escaped_snippet = sql_escape(snippet);
        let escaped_source = sql_escape(source);
        self.table
            .update()
            .only_if(format!("id = '{escaped_id}'"))
            .column("snippet", format!("'{escaped_snippet}'"))
            .column("summary_source", format!("'{escaped_source}'"))
            .execute()
            .await?;
        Ok(())
    }

    /// Batch-apply Ebbinghaus decay scores. `updates` is a vec of (id, new_decay_score).
    pub async fn apply_decay_batch(
        &self,
        updates: &[(String, f32)],
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (id, new_decay) in updates {
            let escaped_id = sql_escape(id);
            self.table
                .update()
                .only_if(format!("id = '{escaped_id}'"))
                .column("decay_score", format!("{new_decay}"))
                .execute()
                .await?;
        }
        Ok(())
    }

    /// Touch accessed records: reset decay to 1.0 and update last_accessed_at.
    pub async fn touch_accessed(&self, ids: &[String]) -> Result<(), Box<dyn std::error::Error>> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        for id in ids {
            let escaped_id = sql_escape(id);
            self.table
                .update()
                .only_if(format!("id = '{escaped_id}'"))
                .column("decay_score", "1.0".to_string())
                .column("last_accessed_at", format!("{now_ms}"))
                .execute()
                .await?;
        }
        Ok(())
    }

    /// Retroactively delete all memories whose URL or window title matches the blocklist domain
    pub async fn delete_memories_by_domain(
        &self,
        domain: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let escaped = sql_escape(&domain.to_lowercase());
        let filter = format!(
            "LOWER(window_title) LIKE '%{}%' OR LOWER(url) LIKE '%{}%'",
            escaped, escaped
        );
        self.table.delete(&filter).await?;
        Ok(())
    }

    /// Return the path to the frames directory (for screenshot eviction).
    pub fn frames_dir(&self) -> PathBuf {
        self.data_dir.join("frames")
    }

    /// Return all memory records whose timestamp falls within [start_ms, end_ms].
    pub async fn get_memories_in_range(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<MemoryRecord>, Box<dyn std::error::Error>> {
        let filter = format!("timestamp >= {start_ms} AND timestamp <= {end_ms}");
        let batches: Vec<RecordBatch> = self
            .table
            .query()
            .only_if(filter)
            .execute()
            .await?
            .try_collect()
            .await?;
        let mut records = Vec::new();
        for batch in &batches {
            records.extend(batch_to_memory_records(batch));
        }
        Ok(records)
    }

    /// Return lightweight search-style rows whose timestamp falls within [start_ms, end_ms].
    pub async fn get_search_results_in_range(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        let filter = format!("timestamp >= {start_ms} AND timestamp <= {end_ms}");
        let mut results = self.query_search_results(Some(filter)).await?;
        results.sort_by_key(|result| result.timestamp);
        Ok(results)
    }

    /// Full-scan keyword search using SQL LIKE predicates.
    pub async fn keyword_search(
        &self,
        query: &str,
        limit: usize,
        time_filter: Option<&str>,
        app_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        let terms = keyword_terms(query);
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let base_limit = limit.max(1);
        let retrieval_limit = if base_limit >= MAX_KEYWORD_SCAN {
            base_limit
        } else {
            base_limit
                .saturating_mul(KEYWORD_QUERY_MULTIPLIER)
                .min(MAX_KEYWORD_SCAN)
        };

        let mut clauses = Vec::new();
        for term in &terms {
            let escaped = sql_escape(&term.to_lowercase());
            clauses.push(format!("LOWER(text) LIKE '%{escaped}%'"));
            clauses.push(format!("LOWER(clean_text) LIKE '%{escaped}%'"));
            clauses.push(format!("LOWER(snippet) LIKE '%{escaped}%'"));
            clauses.push(format!("LOWER(lexical_shadow) LIKE '%{escaped}%'"));
            clauses.push(format!("LOWER(window_title) LIKE '%{escaped}%'"));
            clauses.push(format!("LOWER(app_name) LIKE '%{escaped}%'"));
            clauses.push(format!("LOWER(url) LIKE '%{escaped}%'"));
        }
        let keyword_pred = format!("({})", clauses.join(" OR "));

        let filter = match build_filter(time_filter, app_filter) {
            Some(f) => format!("{keyword_pred} AND {f}"),
            None => keyword_pred,
        };

        let batches: Vec<RecordBatch> = self
            .table
            .query()
            .only_if(filter)
            .limit(retrieval_limit)
            .execute()
            .await?
            .try_collect()
            .await?;

        let mut results = Vec::new();
        let now_ms = chrono::Utc::now().timestamp_millis();
        for batch in &batches {
            let mut batch_results = batch_to_search_results(batch);
            // Keyword branch gets a lexical relevance score before hybrid fusion.
            for r in &mut batch_results {
                let lexical = lexical_keyword_score(&terms, r);
                let recency = recency_score(now_ms, r.timestamp);
                r.score = (lexical * 0.86 + recency * 0.14).clamp(0.0, 1.0);
            }
            results.extend(batch_results);
        }
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        });
        Ok(dedup_search_results(results, limit))
    }

    /// Return comprehensive statistics and usage insights about stored data.
    pub async fn get_stats(&self) -> Result<Stats, Box<dyn std::error::Error>> {
        let batches: Vec<RecordBatch> = self.table.query().execute().await?.try_collect().await?;

        let total_records: usize = batches.iter().map(|b| b.num_rows()).sum();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let one_hour_ago = now_ms - chrono::Duration::hours(1).num_milliseconds();
        let one_day_ago = now_ms - chrono::Duration::hours(24).num_milliseconds();
        let seven_days_ago = now_ms - chrono::Duration::days(7).num_milliseconds();
        let today = local_day_bucket_now();
        let mut days = std::collections::HashSet::new();
        let mut app_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut day_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut domain_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut unique_apps = std::collections::HashSet::new();
        let mut unique_sessions = std::collections::HashSet::new();
        let mut unique_window_titles = std::collections::HashSet::new();
        let mut unique_urls = std::collections::HashSet::new();
        let mut unique_domains = std::collections::HashSet::new();
        let mut hourly_counts = [0usize; 24];
        let mut weekday_counts = [0usize; 7];
        let mut daypart_counts = [0usize; 4]; // Night, Morning, Afternoon, Evening
        let mut first_capture_ts: Option<i64> = None;
        let mut last_capture_ts: Option<i64> = None;
        let mut records_with_url: usize = 0;
        let mut records_with_screenshot: usize = 0;
        let mut records_with_clean_text: usize = 0;
        let mut records_last_hour: usize = 0;
        let mut records_last_24h: usize = 0;
        let mut records_last_7d: usize = 0;
        let mut llm_count: usize = 0;
        let mut vlm_count: usize = 0;
        let mut fallback_count: usize = 0;
        let mut other_summary_count: usize = 0;
        let mut ocr_confidence_sum = 0.0_f64;
        let mut noise_score_sum = 0.0_f64;
        let mut ocr_block_sum = 0.0_f64;
        let mut low_confidence_records: usize = 0;
        let mut high_noise_records: usize = 0;
        let mut timeline_points: Vec<(i64, String)> = Vec::with_capacity(total_records);
        let mut today_count: usize = 0;

        for batch in &batches {
            let timestamp_col = batch
                .column_by_name("timestamp")
                .and_then(|c| c.as_any().downcast_ref::<Int64Array>().cloned());
            let app_col = batch
                .column_by_name("app_name")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned());
            let session_key_col = batch
                .column_by_name("session_key")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned());
            let session_id_col = batch
                .column_by_name("session_id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned());
            let title_col = batch
                .column_by_name("window_title")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned());
            let url_col = batch
                .column_by_name("url")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned());
            let screenshot_col = batch
                .column_by_name("screenshot_path")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned());
            let clean_text_col = batch
                .column_by_name("clean_text")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned());
            let summary_source_col = batch
                .column_by_name("summary_source")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned());
            let ocr_confidence_col = batch
                .column_by_name("ocr_confidence")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>().cloned());
            let noise_score_col = batch
                .column_by_name("noise_score")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>().cloned());
            let ocr_block_col = batch
                .column_by_name("ocr_block_count")
                .and_then(|c| c.as_any().downcast_ref::<Int64Array>().cloned());

            for i in 0..batch.num_rows() {
                let timestamp = timestamp_col.as_ref().map(|c| c.value(i)).unwrap_or(0);

                if timestamp >= one_hour_ago {
                    records_last_hour += 1;
                }
                if timestamp >= one_day_ago {
                    records_last_24h += 1;
                }
                if timestamp >= seven_days_ago {
                    records_last_7d += 1;
                }

                first_capture_ts = Some(first_capture_ts.map_or(timestamp, |v| v.min(timestamp)));
                last_capture_ts = Some(last_capture_ts.map_or(timestamp, |v| v.max(timestamp)));

                let day = local_day_bucket_from_timestamp(timestamp);
                if day == today {
                    today_count += 1;
                }
                days.insert(day.clone());
                *day_counts.entry(day).or_insert(0) += 1;

                if let Some(dt) = Local.timestamp_millis_opt(timestamp).single() {
                    let hour_idx = dt.hour() as usize;
                    hourly_counts[hour_idx] += 1;
                    weekday_counts[dt.weekday().num_days_from_monday() as usize] += 1;

                    let daypart_idx = match dt.hour() {
                        4..=11 => 1,
                        12..=15 => 2,
                        16..=19 => 3,
                        _ => 0,
                    };
                    daypart_counts[daypart_idx] += 1;
                }

                let app_name =
                    get_non_empty_str(&app_col, i).unwrap_or_else(|| "Unknown".to_string());
                *app_counts.entry(app_name.clone()).or_insert(0) += 1;
                unique_apps.insert(app_name.clone());
                timeline_points.push((timestamp, app_name));

                if let Some(title) = get_non_empty_str(&title_col, i) {
                    unique_window_titles.insert(title);
                }

                let session = get_non_empty_str(&session_key_col, i)
                    .or_else(|| get_non_empty_str(&session_id_col, i));
                if let Some(session_id) = session {
                    unique_sessions.insert(session_id);
                }

                if let Some(url) = get_non_empty_str(&url_col, i) {
                    records_with_url += 1;
                    unique_urls.insert(url.clone());
                    if let Some(domain) = extract_domain(&url) {
                        unique_domains.insert(domain.clone());
                        *domain_counts.entry(domain).or_insert(0) += 1;
                    }
                }

                if get_non_empty_str(&screenshot_col, i).is_some() {
                    records_with_screenshot += 1;
                }
                if get_non_empty_str(&clean_text_col, i).is_some() {
                    records_with_clean_text += 1;
                }

                let source = get_non_empty_str(&summary_source_col, i)
                    .unwrap_or_else(|| "fallback".to_string())
                    .to_ascii_lowercase();
                match source.as_str() {
                    "llm" => llm_count += 1,
                    "vlm" => vlm_count += 1,
                    "fallback" => fallback_count += 1,
                    _ => other_summary_count += 1,
                }

                let confidence = ocr_confidence_col
                    .as_ref()
                    .map(|c| c.value(i) as f64)
                    .unwrap_or(0.0);
                ocr_confidence_sum += confidence;
                if confidence > 0.0 && confidence < 0.55 {
                    low_confidence_records += 1;
                }

                let noise = noise_score_col
                    .as_ref()
                    .map(|c| c.value(i) as f64)
                    .unwrap_or(0.0);
                noise_score_sum += noise;
                if noise >= 0.40 {
                    high_noise_records += 1;
                }

                let ocr_blocks = ocr_block_col
                    .as_ref()
                    .map(|c| c.value(i).max(0) as f64)
                    .unwrap_or(0.0);
                ocr_block_sum += ocr_blocks;
            }
        }

        let mut apps: Vec<AppCount> = app_counts
            .into_iter()
            .map(|(name, count)| AppCount { name, count })
            .collect();
        apps.sort_by(|a, b| b.count.cmp(&a.count));
        let focus_app_share_pct = if total_records > 0 {
            apps.first()
                .map(|a| (a.count as f64 / total_records as f64) * 100.0)
                .unwrap_or(0.0)
        } else {
            0.0
        };
        apps.truncate(10);

        let mut top_domains: Vec<DomainCount> = domain_counts
            .into_iter()
            .map(|(domain, count)| DomainCount { domain, count })
            .collect();
        top_domains.sort_by(|a, b| b.count.cmp(&a.count));
        top_domains.truncate(10);

        let busiest_day = day_counts
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(day, count)| DayCount {
                day: day.clone(),
                count: *count,
            });

        let quietest_day = day_counts
            .iter()
            .min_by_key(|(_, count)| *count)
            .map(|(day, count)| DayCount {
                day: day.clone(),
                count: *count,
            });

        let hourly_distribution: Vec<HourCount> = hourly_counts
            .iter()
            .enumerate()
            .map(|(hour, count)| HourCount {
                hour: hour as u8,
                count: *count,
            })
            .collect();

        let busiest_hour = hourly_counts
            .iter()
            .enumerate()
            .max_by_key(|(_, count)| *count)
            .and_then(|(hour, count)| {
                if *count == 0 {
                    None
                } else {
                    Some(HourCount {
                        hour: hour as u8,
                        count: *count,
                    })
                }
            });

        let weekday_labels = [
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
            "Sunday",
        ];
        let weekday_distribution: Vec<WeekdayCount> = weekday_counts
            .iter()
            .enumerate()
            .map(|(idx, count)| WeekdayCount {
                weekday: weekday_labels[idx].to_string(),
                count: *count,
            })
            .collect();

        let daypart_labels = ["Night", "Morning", "Afternoon", "Evening"];
        let daypart_distribution: Vec<DaypartCount> = daypart_counts
            .iter()
            .enumerate()
            .map(|(idx, count)| DaypartCount {
                daypart: daypart_labels[idx].to_string(),
                count: *count,
            })
            .collect();

        timeline_points.sort_by_key(|(timestamp, _)| *timestamp);

        let mut app_switches = 0usize;
        let mut total_gap_ms = 0_i64;
        let mut gap_count = 0usize;
        let mut longest_gap_ms = 0_i64;

        for pair in timeline_points.windows(2) {
            let (prev_ts, prev_app) = (&pair[0].0, &pair[0].1);
            let (next_ts, next_app) = (&pair[1].0, &pair[1].1);

            if prev_app != next_app {
                app_switches += 1;
            }

            let gap = (*next_ts - *prev_ts).max(0);
            if gap > 0 {
                total_gap_ms += gap;
                gap_count += 1;
                longest_gap_ms = longest_gap_ms.max(gap);
            }
        }

        let capture_span_hours = match (first_capture_ts, last_capture_ts) {
            (Some(first), Some(last)) if last >= first => (last - first) as f64 / 3_600_000.0,
            _ => 0.0,
        };

        let avg_gap_minutes = if gap_count > 0 {
            total_gap_ms as f64 / gap_count as f64 / 60_000.0
        } else {
            0.0
        };

        let app_switch_rate_per_hour = if capture_span_hours > 0.0 {
            app_switches as f64 / capture_span_hours
        } else {
            0.0
        };

        let avg_records_per_active_day = if !days.is_empty() {
            total_records as f64 / days.len() as f64
        } else {
            0.0
        };

        let avg_records_per_hour = if capture_span_hours > 0.0 {
            total_records as f64 / capture_span_hours
        } else {
            0.0
        };

        let avg_ocr_confidence = if total_records > 0 {
            ocr_confidence_sum / total_records as f64
        } else {
            0.0
        };

        let avg_noise_score = if total_records > 0 {
            noise_score_sum / total_records as f64
        } else {
            0.0
        };

        let avg_ocr_blocks = if total_records > 0 {
            ocr_block_sum / total_records as f64
        } else {
            0.0
        };

        let (current_streak_days, longest_streak_days) = compute_activity_streaks(&day_counts);

        Ok(Stats {
            total_records,
            total_days: days.len(),
            apps,
            today_count,
            unique_apps: unique_apps.len(),
            unique_sessions: unique_sessions.len(),
            unique_window_titles: unique_window_titles.len(),
            unique_urls: unique_urls.len(),
            unique_domains: unique_domains.len(),
            records_with_url,
            records_with_screenshot,
            records_with_clean_text,
            records_last_hour,
            records_last_24h,
            records_last_7d,
            avg_records_per_active_day,
            avg_records_per_hour,
            focus_app_share_pct,
            app_switches,
            app_switch_rate_per_hour,
            avg_gap_minutes,
            longest_gap_minutes: (longest_gap_ms / 60_000).max(0) as u64,
            first_capture_ts,
            last_capture_ts,
            capture_span_hours,
            current_streak_days,
            longest_streak_days,
            avg_ocr_confidence,
            low_confidence_records,
            avg_noise_score,
            high_noise_records,
            avg_ocr_blocks,
            llm_count,
            vlm_count,
            fallback_count,
            other_summary_count,
            top_domains,
            busiest_day,
            quietest_day,
            busiest_hour,
            hourly_distribution,
            weekday_distribution,
            daypart_distribution,
        })
    }

    /// Delete all records.
    pub async fn delete_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.table.delete("id IS NOT NULL").await?;
        Ok(())
    }

    /// Return sorted list of unique app names.
    pub async fn get_app_names(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let batches: Vec<RecordBatch> = self.table.query().execute().await?.try_collect().await?;

        let mut names = std::collections::HashSet::new();
        for batch in &batches {
            let app_col = batch
                .column_by_name("app_name")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned());

            if let Some(col) = app_col {
                for i in 0..batch.num_rows() {
                    let name = col.value(i);
                    if !name.is_empty() {
                        names.insert(name.to_string());
                    }
                }
            }
        }
        let mut list: Vec<String> = names.into_iter().collect();
        list.sort();
        Ok(list)
    }

    /// Delete records older than `days` days; returns count of deleted rows.
    pub async fn delete_older_than(&self, days: u32) -> Result<usize, Box<dyn std::error::Error>> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
        let cutoff_ms = cutoff.timestamp_millis();

        // Count before deletion.
        let before = self.table.count_rows(None).await?;
        self.table
            .delete(&format!("timestamp < {cutoff_ms}"))
            .await?;
        let after = self.table.count_rows(None).await?;

        Ok(before.saturating_sub(after))
    }

    /// Delete rows whose id starts with `prefix` (SQL LIKE `prefix%`).
    pub async fn delete_id_prefix(
        &self,
        prefix: &str,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let before = self.table.count_rows(None).await?;
        let p = sql_escape(prefix);
        self.table.delete(&format!("id LIKE '{p}%'")).await?;
        let after = self.table.count_rows(None).await?;
        Ok(before.saturating_sub(after))
    }

    /// Delete a specific memory row by exact id.
    pub async fn delete_memory_by_id(
        &self,
        memory_id: &str,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let before = self.table.count_rows(None).await?;
        let id = sql_escape(memory_id);
        self.table.delete(&format!("id = '{id}'")).await?;
        let after = self.table.count_rows(None).await?;
        Ok(before.saturating_sub(after))
    }

    /// Return recent memory records (last `hours` hours).
    pub async fn get_recent_memories(
        &self,
        hours: u32,
    ) -> Result<Vec<MemoryRecord>, Box<dyn std::error::Error>> {
        let cutoff = chrono::Utc::now() - chrono::Duration::hours(hours as i64);
        let cutoff_ms = cutoff.timestamp_millis();

        let batches: Vec<RecordBatch> = self
            .table
            .query()
            .only_if(format!("timestamp >= {cutoff_ms}"))
            .execute()
            .await?
            .try_collect()
            .await?;

        let mut records = Vec::new();
        for batch in &batches {
            records.extend(batch_to_memory_records(batch));
        }
        Ok(records)
    }

    /// Return all stored memory records ordered oldest -> newest.
    pub async fn list_all_memories(&self) -> Result<Vec<MemoryRecord>, Box<dyn std::error::Error>> {
        let batches: Vec<RecordBatch> = self.table.query().execute().await?.try_collect().await?;

        let mut records = Vec::new();
        for batch in &batches {
            records.extend(batch_to_memory_records(batch));
        }
        records.sort_by_key(|record| record.timestamp);
        Ok(records)
    }

    /// Fetch a single record by id.
    pub async fn get_memory_by_id(
        &self,
        memory_id: &str,
    ) -> Result<Option<MemoryRecord>, Box<dyn std::error::Error>> {
        let id = sql_escape(memory_id);
        let batches: Vec<RecordBatch> = self
            .table
            .query()
            .only_if(format!("id = '{id}'"))
            .limit(1)
            .execute()
            .await?
            .try_collect()
            .await?;

        for batch in &batches {
            let records = batch_to_memory_records(batch);
            if let Some(r) = records.into_iter().next() {
                return Ok(Some(r));
            }
        }
        Ok(None)
    }

    /// Return recently captured URLs (newest first, deduplicated).
    pub async fn get_recent_urls(
        &self,
        limit: usize,
    ) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let batches: Vec<RecordBatch> = self
            .table
            .query()
            .only_if("url IS NOT NULL")
            .execute()
            .await?
            .try_collect()
            .await?;

        let mut pairs: Vec<(i64, String)> = Vec::new();
        for batch in &batches {
            let ts_col = batch
                .column_by_name("timestamp")
                .and_then(|c| c.as_any().downcast_ref::<Int64Array>().cloned());
            let url_col = batch
                .column_by_name("url")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned());
            if let (Some(ts), Some(url)) = (ts_col, url_col) {
                for i in 0..batch.num_rows() {
                    if !url.is_null(i) {
                        pairs.push((ts.value(i), url.value(i).to_string()));
                    }
                }
            }
        }

        pairs.sort_by_key(|(ts, _)| std::cmp::Reverse(*ts));

        let mut unique = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (_, url) in pairs {
            if seen.insert(url.clone()) {
                unique.push(url);
            }
            if unique.len() >= limit {
                break;
            }
        }
        Ok(unique)
    }

    /// List newest memories as raw search-style rows (optionally filtered by app).
    pub async fn list_recent_results(
        &self,
        limit: usize,
        app_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        let base_limit = limit.max(1);
        // Lance's plain table query does not guarantee timestamp ordering, so
        // we need to sort after scanning matching rows rather than limit first.
        let mut query = self
            .table
            .query()
            .select(Select::columns(SEARCH_RESULT_COLUMNS));
        if let Some(filter) = build_filter(None, app_filter) {
            query = query.only_if(filter);
        }

        let batches: Vec<RecordBatch> = query.execute().await?.try_collect().await?;
        let mut results = Vec::new();
        for batch in &batches {
            let mut batch_results = batch_to_search_results(batch);
            for result in &mut batch_results {
                result.score = 1.0;
            }
            results.extend(batch_results);
        }
        results.sort_by_key(|result| std::cmp::Reverse(result.timestamp));
        results.truncate(base_limit);
        Ok(results)
    }

    async fn query_search_results(
        &self,
        filter: Option<String>,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        let mut query = self
            .table
            .query()
            .select(Select::columns(SEARCH_RESULT_COLUMNS));
        if let Some(filter) = filter {
            query = query.only_if(filter);
        }

        let batches: Vec<RecordBatch> = query.execute().await?.try_collect().await?;
        let mut results = Vec::new();
        for batch in &batches {
            let mut batch_results = batch_to_search_results(batch);
            for result in &mut batch_results {
                result.score = 1.0;
            }
            results.extend(batch_results);
        }
        Ok(results)
    }
}

// ── Schema ────────────────────────────────────────────────────────────────────

fn memory_schema() -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("timestamp", DataType::Int64, false),
        Field::new("day_bucket", DataType::Utf8, false),
        Field::new("app_name", DataType::Utf8, false),
        Field::new("bundle_id", DataType::Utf8, true),
        Field::new("window_title", DataType::Utf8, false),
        Field::new("session_id", DataType::Utf8, false),
        Field::new("text", DataType::Utf8, false),
        Field::new("clean_text", DataType::Utf8, false),
        Field::new("ocr_confidence", DataType::Float32, false),
        Field::new("ocr_block_count", DataType::Int64, false),
        Field::new("snippet", DataType::Utf8, false),
        Field::new("summary_source", DataType::Utf8, false),
        Field::new("noise_score", DataType::Float32, false),
        Field::new("session_key", DataType::Utf8, false),
        Field::new("lexical_shadow", DataType::Utf8, false),
        Field::new(
            "embedding",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                TEXT_EMBED_DIM,
            ),
            false,
        ),
        Field::new(
            "image_embedding",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                IMAGE_EMBED_DIM,
            ),
            false,
        ),
        Field::new("screenshot_path", DataType::Utf8, true),
        Field::new("url", DataType::Utf8, true),
        Field::new(
            "snippet_embedding",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                TEXT_EMBED_DIM,
            ),
            false,
        ),
        Field::new(
            "support_embedding",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                TEXT_EMBED_DIM,
            ),
            false,
        ),
        Field::new("decay_score", DataType::Float32, false),
        Field::new("last_accessed_at", DataType::Int64, false),
    ])
}

fn task_schema() -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("title", DataType::Utf8, false),
        Field::new("description", DataType::Utf8, false),
        Field::new("source_app", DataType::Utf8, false),
        Field::new("source_memory_id", DataType::Utf8, true),
        Field::new("created_at", DataType::Int64, false),
        Field::new("due_date", DataType::Int64, true),
        Field::new("is_completed", DataType::Boolean, false),
        Field::new("is_dismissed", DataType::Boolean, false),
        Field::new("task_type", DataType::Utf8, false),
        Field::new(
            "linked_urls",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            false,
        ),
        Field::new(
            "linked_memory_ids",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            false,
        ),
    ])
}

fn meeting_schema() -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("title", DataType::Utf8, false),
        Field::new(
            "participants",
            DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            false,
        ),
        Field::new("model", DataType::Utf8, false),
        Field::new("status", DataType::Utf8, false),
        Field::new("start_timestamp", DataType::Int64, false),
        Field::new("end_timestamp", DataType::Int64, true),
        Field::new("created_at", DataType::Int64, false),
        Field::new("updated_at", DataType::Int64, false),
        Field::new("segment_count", DataType::Int64, false),
        Field::new("duration_seconds", DataType::Int64, false),
        Field::new("meeting_dir", DataType::Utf8, false),
        Field::new("audio_dir", DataType::Utf8, false),
        Field::new("transcript_path", DataType::Utf8, true),
        Field::new("breakdown_json", DataType::Utf8, true),
    ])
}

fn segment_schema() -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("meeting_id", DataType::Utf8, false),
        Field::new("index", DataType::UInt32, false),
        Field::new("start_timestamp", DataType::Int64, false),
        Field::new("end_timestamp", DataType::Int64, false),
        Field::new("text", DataType::Utf8, false),
        Field::new("audio_chunk_path", DataType::Utf8, false),
        Field::new("model", DataType::Utf8, false),
        Field::new("created_at", DataType::Int64, false),
    ])
}

fn node_schema() -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("node_type", DataType::Utf8, false),
        Field::new("label", DataType::Utf8, false),
        Field::new("created_at", DataType::Int64, false),
        Field::new("metadata_json", DataType::Utf8, false),
    ])
}

fn edge_schema() -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("source", DataType::Utf8, false),
        Field::new("target", DataType::Utf8, false),
        Field::new("edge_type", DataType::Utf8, false),
        Field::new("timestamp", DataType::Int64, false),
        Field::new("metadata_json", DataType::Utf8, false),
    ])
}

// ── Arrow ↔ MemoryRecord conversion ─────────────────────────────────────────

fn records_to_batch(records: &[MemoryRecord]) -> Result<RecordBatch, ArrowError> {
    let schema = Arc::new(memory_schema());

    // Scalar string columns
    let ids: Vec<&str> = records.iter().map(|r| r.id.as_str()).collect();
    let timestamps: Vec<i64> = records.iter().map(|r| r.timestamp).collect();
    let day_buckets: Vec<&str> = records.iter().map(|r| r.day_bucket.as_str()).collect();
    let app_names: Vec<&str> = records.iter().map(|r| r.app_name.as_str()).collect();
    let bundle_ids: Vec<Option<&str>> = records.iter().map(|r| r.bundle_id.as_deref()).collect();
    let window_titles: Vec<&str> = records.iter().map(|r| r.window_title.as_str()).collect();
    let session_ids: Vec<&str> = records.iter().map(|r| r.session_id.as_str()).collect();
    let texts: Vec<&str> = records.iter().map(|r| r.text.as_str()).collect();
    let clean_texts: Vec<&str> = records.iter().map(|r| r.clean_text.as_str()).collect();
    let ocr_confidences: Vec<f32> = records.iter().map(|r| r.ocr_confidence).collect();
    let ocr_block_counts: Vec<i64> = records.iter().map(|r| r.ocr_block_count as i64).collect();
    let snippets: Vec<&str> = records.iter().map(|r| r.snippet.as_str()).collect();
    let summary_sources: Vec<&str> = records.iter().map(|r| r.summary_source.as_str()).collect();
    let noise_scores: Vec<f32> = records.iter().map(|r| r.noise_score).collect();
    let session_keys: Vec<&str> = records.iter().map(|r| r.session_key.as_str()).collect();
    let lexical_shadows: Vec<&str> = records.iter().map(|r| r.lexical_shadow.as_str()).collect();
    let screenshot_paths: Vec<Option<&str>> = records
        .iter()
        .map(|r| r.screenshot_path.as_deref())
        .collect();
    let urls: Vec<Option<&str>> = records.iter().map(|r| r.url.as_deref()).collect();

    // Text embeddings — flatten all embeddings into one Float32Array.
    let flat_text: Vec<f32> = records
        .iter()
        .flat_map(|r| r.embedding.iter().copied())
        .collect();
    let text_values = Arc::new(Float32Array::from(flat_text)) as Arc<dyn Array>;
    let embedding_array = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        TEXT_EMBED_DIM,
        text_values,
        None,
    )?;

    // Image embeddings
    let flat_img: Vec<f32> = records
        .iter()
        .flat_map(|r| r.image_embedding.iter().copied())
        .collect();
    let img_values = Arc::new(Float32Array::from(flat_img)) as Arc<dyn Array>;
    let image_embedding_array = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        IMAGE_EMBED_DIM,
        img_values,
        None,
    )?;

    // Snippet embeddings (second semantic tower)
    let flat_snip: Vec<f32> = records
        .iter()
        .flat_map(|r| r.snippet_embedding.iter().copied())
        .collect();
    let snip_values = Arc::new(Float32Array::from(flat_snip)) as Arc<dyn Array>;
    let snippet_embedding_array = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        TEXT_EMBED_DIM,
        snip_values,
        None,
    )?;

    let flat_support: Vec<f32> = records
        .iter()
        .flat_map(|r| r.support_embedding.iter().copied())
        .collect();
    let support_values = Arc::new(Float32Array::from(flat_support)) as Arc<dyn Array>;
    let support_embedding_array = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        TEXT_EMBED_DIM,
        support_values,
        None,
    )?;

    let decay_scores: Vec<f32> = records.iter().map(|r| r.decay_score).collect();
    let last_accessed: Vec<i64> = records.iter().map(|r| r.last_accessed_at).collect();

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(ids)),
            Arc::new(Int64Array::from(timestamps)),
            Arc::new(StringArray::from(day_buckets)),
            Arc::new(StringArray::from(app_names)),
            Arc::new(StringArray::from(bundle_ids)),
            Arc::new(StringArray::from(window_titles)),
            Arc::new(StringArray::from(session_ids)),
            Arc::new(StringArray::from(texts)),
            Arc::new(StringArray::from(clean_texts)),
            Arc::new(Float32Array::from(ocr_confidences)),
            Arc::new(Int64Array::from(ocr_block_counts)),
            Arc::new(StringArray::from(snippets)),
            Arc::new(StringArray::from(summary_sources)),
            Arc::new(Float32Array::from(noise_scores)),
            Arc::new(StringArray::from(session_keys)),
            Arc::new(StringArray::from(lexical_shadows)),
            Arc::new(embedding_array),
            Arc::new(image_embedding_array),
            Arc::new(StringArray::from(screenshot_paths)),
            Arc::new(StringArray::from(urls)),
            Arc::new(snippet_embedding_array),
            Arc::new(support_embedding_array),
            Arc::new(Float32Array::from(decay_scores)),
            Arc::new(Int64Array::from(last_accessed)),
        ],
    )
}

fn batch_to_memory_records(batch: &RecordBatch) -> Vec<MemoryRecord> {
    let n = batch.num_rows();
    let ids = str_col(batch, "id");
    let timestamps = i64_col(batch, "timestamp");
    let day_buckets = str_col(batch, "day_bucket");
    let app_names = str_col(batch, "app_name");
    let bundle_ids = str_col(batch, "bundle_id");
    let window_titles = str_col(batch, "window_title");
    let session_ids = str_col(batch, "session_id");
    let texts = str_col(batch, "text");
    let clean_texts = str_col(batch, "clean_text");
    let ocr_confidences = f32_col(batch, "ocr_confidence");
    let ocr_block_counts = i64_col(batch, "ocr_block_count");
    let snippets = str_col(batch, "snippet");
    let summary_sources = str_col(batch, "summary_source");
    let noise_scores = f32_col(batch, "noise_score");
    let session_keys = str_col(batch, "session_key");
    let lexical_shadows = str_col(batch, "lexical_shadow");
    let screenshot_paths = str_col(batch, "screenshot_path");
    let urls = str_col(batch, "url");

    let embed_col = batch
        .column_by_name("embedding")
        .and_then(|c| c.as_any().downcast_ref::<FixedSizeListArray>().cloned());
    let img_col = batch
        .column_by_name("image_embedding")
        .and_then(|c| c.as_any().downcast_ref::<FixedSizeListArray>().cloned());
    let snip_embed_col = batch
        .column_by_name("snippet_embedding")
        .and_then(|c| c.as_any().downcast_ref::<FixedSizeListArray>().cloned());
    let support_embed_col = batch
        .column_by_name("support_embedding")
        .and_then(|c| c.as_any().downcast_ref::<FixedSizeListArray>().cloned());
    let decay_scores = f32_col(batch, "decay_score");
    let last_accessed = i64_col(batch, "last_accessed_at");

    (0..n)
        .map(|i| {
            let embedding = extract_f32_list(&embed_col, i, TEXT_EMBED_DIM as usize);
            let image_embedding = extract_f32_list(&img_col, i, IMAGE_EMBED_DIM as usize);
            let snippet_embedding = extract_f32_list(&snip_embed_col, i, TEXT_EMBED_DIM as usize);
            let support_embedding =
                extract_f32_list(&support_embed_col, i, TEXT_EMBED_DIM as usize);
            MemoryRecord {
                id: get_str(&ids, i),
                timestamp: timestamps.as_ref().map(|c| c.value(i)).unwrap_or(0),
                day_bucket: get_str(&day_buckets, i),
                app_name: get_str(&app_names, i),
                bundle_id: get_opt_str(&bundle_ids, i),
                window_title: get_str(&window_titles, i),
                session_id: get_str(&session_ids, i),
                text: get_str(&texts, i),
                clean_text: get_str(&clean_texts, i),
                ocr_confidence: get_f32(&ocr_confidences, i),
                ocr_block_count: get_i64(&ocr_block_counts, i).max(0) as u32,
                snippet: get_str(&snippets, i),
                summary_source: get_str(&summary_sources, i),
                noise_score: get_f32(&noise_scores, i),
                session_key: get_str(&session_keys, i),
                lexical_shadow: get_str(&lexical_shadows, i),
                embedding,
                image_embedding,
                screenshot_path: get_opt_str(&screenshot_paths, i),
                url: get_opt_str(&urls, i),
                snippet_embedding,
                support_embedding,
                decay_score: get_f32(&decay_scores, i).max(0.01),
                last_accessed_at: get_i64(&last_accessed, i),
            }
        })
        .collect()
}

fn batch_to_search_results(batch: &RecordBatch) -> Vec<SearchResult> {
    let n = batch.num_rows();
    let ids = str_col(batch, "id");
    let timestamps = i64_col(batch, "timestamp");
    let app_names = str_col(batch, "app_name");
    let bundle_ids = str_col(batch, "bundle_id");
    let window_titles = str_col(batch, "window_title");
    let session_ids = str_col(batch, "session_id");
    let texts = str_col(batch, "text");
    let clean_texts = str_col(batch, "clean_text");
    let ocr_confidences = f32_col(batch, "ocr_confidence");
    let ocr_block_counts = i64_col(batch, "ocr_block_count");
    let snippets = str_col(batch, "snippet");
    let summary_sources = str_col(batch, "summary_source");
    let noise_scores = f32_col(batch, "noise_score");
    let session_keys = str_col(batch, "session_key");
    let lexical_shadows = str_col(batch, "lexical_shadow");
    let screenshot_paths = str_col(batch, "screenshot_path");
    let urls = str_col(batch, "url");

    // LanceDB appends _distance column to vector search results.
    let dist_col = batch
        .column_by_name("_distance")
        .and_then(|c| c.as_any().downcast_ref::<Float32Array>().cloned());
    let decay_scores = f32_col(batch, "decay_score");

    (0..n)
        .map(|i| {
            let score = dist_col
                .as_ref()
                .map(|c| {
                    let d = c.value(i);
                    // Standard L2 distance → similarity mapping.
                    // Using a gentle decay handles both normalized and un-normalized distance scales.
                    1.0 / (1.0 + d * 0.01)
                })
                .unwrap_or(1.0);
            SearchResult {
                id: get_str(&ids, i),
                timestamp: timestamps.as_ref().map(|c| c.value(i)).unwrap_or(0),
                app_name: get_str(&app_names, i),
                bundle_id: get_opt_str(&bundle_ids, i),
                window_title: get_str(&window_titles, i),
                session_id: get_str(&session_ids, i),
                text: get_str(&texts, i),
                clean_text: get_str(&clean_texts, i),
                ocr_confidence: get_f32(&ocr_confidences, i),
                ocr_block_count: get_i64(&ocr_block_counts, i).max(0) as u32,
                snippet: get_str(&snippets, i),
                summary_source: get_str(&summary_sources, i),
                noise_score: get_f32(&noise_scores, i),
                session_key: get_str(&session_keys, i),
                lexical_shadow: get_str(&lexical_shadows, i),
                score,
                screenshot_path: get_opt_str(&screenshot_paths, i),
                url: get_opt_str(&urls, i),
                decay_score: get_f32(&decay_scores, i).max(0.15),
            }
        })
        .collect()
}

// ── Arrow column helpers ─────────────────────────────────────────────────────

fn str_col(batch: &RecordBatch, name: &str) -> Option<StringArray> {
    batch
        .column_by_name(name)?
        .as_any()
        .downcast_ref::<StringArray>()
        .cloned()
}

fn i64_col(batch: &RecordBatch, name: &str) -> Option<Int64Array> {
    batch
        .column_by_name(name)?
        .as_any()
        .downcast_ref::<Int64Array>()
        .cloned()
}

fn f32_col(batch: &RecordBatch, name: &str) -> Option<Float32Array> {
    batch
        .column_by_name(name)?
        .as_any()
        .downcast_ref::<Float32Array>()
        .cloned()
}

fn bool_col(batch: &RecordBatch, name: &str) -> Option<BooleanArray> {
    batch
        .column_by_name(name)?
        .as_any()
        .downcast_ref::<BooleanArray>()
        .cloned()
}

fn u32_col(batch: &RecordBatch, name: &str) -> Option<UInt32Array> {
    batch
        .column_by_name(name)?
        .as_any()
        .downcast_ref::<UInt32Array>()
        .cloned()
}

fn get_str(col: &Option<StringArray>, i: usize) -> String {
    col.as_ref()
        .map(|c| c.value(i).to_string())
        .unwrap_or_default()
}

fn get_opt_str(col: &Option<StringArray>, i: usize) -> Option<String> {
    col.as_ref().and_then(|c| {
        if c.is_null(i) {
            None
        } else {
            Some(c.value(i).to_string())
        }
    })
}

fn get_non_empty_str(col: &Option<StringArray>, i: usize) -> Option<String> {
    get_opt_str(col, i).and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn get_i64(col: &Option<Int64Array>, i: usize) -> i64 {
    col.as_ref().map(|c| c.value(i)).unwrap_or(0)
}

fn get_f32(col: &Option<Float32Array>, i: usize) -> f32 {
    col.as_ref().map(|c| c.value(i)).unwrap_or(0.0)
}

fn get_u32(col: &Option<UInt32Array>, i: usize) -> u32 {
    col.as_ref().map(|c| c.value(i)).unwrap_or(0)
}

fn extract_domain(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }

    let without_scheme = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);

    let host_and_path = without_scheme.split('/').next().unwrap_or("");
    let without_credentials = host_and_path.rsplit('@').next().unwrap_or(host_and_path);
    let host = without_credentials.split(':').next().unwrap_or("").trim();
    if host.is_empty() {
        return None;
    }

    let host = host.to_ascii_lowercase();
    let normalized = host
        .strip_prefix("www.")
        .map(|h| h.to_string())
        .unwrap_or(host);

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn compute_activity_streaks(
    day_counts: &std::collections::HashMap<String, usize>,
) -> (usize, usize) {
    let mut days: Vec<NaiveDate> = day_counts
        .keys()
        .filter_map(|day| NaiveDate::parse_from_str(day, "%Y-%m-%d").ok())
        .collect();

    if days.is_empty() {
        return (0, 0);
    }

    days.sort_unstable();
    days.dedup();

    let mut longest_streak = 1usize;
    let mut run = 1usize;
    for i in 1..days.len() {
        if days[i] == days[i - 1] + chrono::Duration::days(1) {
            run += 1;
        } else {
            run = 1;
        }
        longest_streak = longest_streak.max(run);
    }

    let mut current_streak = 1usize;
    for i in (1..days.len()).rev() {
        if days[i] == days[i - 1] + chrono::Duration::days(1) {
            current_streak += 1;
        } else {
            break;
        }
    }

    (current_streak, longest_streak)
}

fn get_bool(col: &Option<BooleanArray>, i: usize) -> bool {
    col.as_ref().map(|c| c.value(i)).unwrap_or(false)
}

fn get_opt_i64(col: &Option<Int64Array>, i: usize) -> Option<i64> {
    col.as_ref()
        .and_then(|c| if c.is_null(i) { None } else { Some(c.value(i)) })
}

fn extract_str_list(col: &Option<arrow_array::ListArray>, i: usize) -> Vec<String> {
    if let Some(list) = col {
        if let Some(values) = list
            .value(i)
            .as_any()
            .downcast_ref::<StringArray>()
            .cloned()
        {
            return (0..values.len())
                .map(|j| values.value(j).to_string())
                .collect();
        }
    }
    Vec::new()
}

fn extract_f32_list(col: &Option<FixedSizeListArray>, i: usize, dim: usize) -> Vec<f32> {
    if let Some(list) = col {
        if let Some(values) = list
            .value(i)
            .as_any()
            .downcast_ref::<Float32Array>()
            .cloned()
        {
            return (0..values.len()).map(|j| values.value(j)).collect();
        }
    }
    vec![0.0; dim]
}

// ── Arrow ↔ Task conversion ──────────────────────────────────────────────────

fn task_to_batch(tasks: &[Task]) -> Result<RecordBatch, ArrowError> {
    let schema = Arc::new(task_schema());
    let ids: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
    let titles: Vec<&str> = tasks.iter().map(|t| t.title.as_str()).collect();
    let descriptions: Vec<&str> = tasks.iter().map(|t| t.description.as_str()).collect();
    let source_apps: Vec<&str> = tasks.iter().map(|t| t.source_app.as_str()).collect();
    let source_memory_ids: Vec<Option<&str>> = tasks
        .iter()
        .map(|t| t.source_memory_id.as_deref())
        .collect();
    let created_at: Vec<i64> = tasks.iter().map(|t| t.created_at).collect();
    let due_date: Vec<Option<i64>> = tasks.iter().map(|t| t.due_date).collect();
    let is_completed: Vec<bool> = tasks.iter().map(|t| t.is_completed).collect();
    let is_dismissed: Vec<bool> = tasks.iter().map(|t| t.is_dismissed).collect();
    let task_types: Vec<String> = tasks.iter().map(|t| format!("{:?}", t.task_type)).collect();

    // List columns
    let mut url_builder =
        arrow_array::builder::ListBuilder::new(arrow_array::builder::StringBuilder::new());
    let mut mem_id_builder =
        arrow_array::builder::ListBuilder::new(arrow_array::builder::StringBuilder::new());

    for t in tasks {
        for url in &t.linked_urls {
            url_builder.values().append_value(url);
        }
        url_builder.append(true);

        for mid in &t.linked_memory_ids {
            mem_id_builder.values().append_value(mid);
        }
        mem_id_builder.append(true);
    }

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(ids)),
            Arc::new(StringArray::from(titles)),
            Arc::new(StringArray::from(descriptions)),
            Arc::new(StringArray::from(source_apps)),
            Arc::new(StringArray::from(source_memory_ids)),
            Arc::new(Int64Array::from(created_at)),
            Arc::new(Int64Array::from(due_date)),
            Arc::new(arrow_array::BooleanArray::from(is_completed)),
            Arc::new(arrow_array::BooleanArray::from(is_dismissed)),
            Arc::new(StringArray::from(task_types)),
            Arc::new(url_builder.finish()),
            Arc::new(mem_id_builder.finish()),
        ],
    )
}

fn nodes_to_batch(nodes: &[GraphNode]) -> Result<RecordBatch, Box<dyn std::error::Error>> {
    let mut ids = StringBuilder::new();
    let mut types = StringBuilder::new();
    let mut labels = StringBuilder::new();
    let mut created = Int64Builder::new();
    let mut metadata = StringBuilder::new();

    for n in nodes {
        ids.append_value(&n.id);
        types.append_value(match n.node_type {
            NodeType::Entity => "Entity",
            NodeType::Task => "Task",
            NodeType::Url => "Url",
            NodeType::MemoryChunk => "MemoryChunk",
            NodeType::Clipboard => "Clipboard",
            NodeType::AudioSegment => "AudioSegment",
        });
        labels.append_value(&n.label);
        created.append_value(n.created_at);
        metadata.append_value(serde_json::to_string(&n.metadata).unwrap_or_default());
    }

    RecordBatch::try_new(
        Arc::new(node_schema()),
        vec![
            Arc::new(ids.finish()),
            Arc::new(types.finish()),
            Arc::new(labels.finish()),
            Arc::new(created.finish()),
            Arc::new(metadata.finish()),
        ],
    )
    .map_err(|e| e.into())
}

fn edges_to_batch(edges: &[GraphEdge]) -> Result<RecordBatch, Box<dyn std::error::Error>> {
    let mut ids = StringBuilder::new();
    let mut sources = StringBuilder::new();
    let mut targets = StringBuilder::new();
    let mut types = StringBuilder::new();
    let mut timestamps = Int64Builder::new();
    let mut metadata = StringBuilder::new();

    for e in edges {
        ids.append_value(&e.id);
        sources.append_value(&e.source);
        targets.append_value(&e.target);
        types.append_value(match e.edge_type {
            EdgeType::PartOfSession => "PART_OF_SESSION",
            EdgeType::ReferenceForTask => "REFERENCE_FOR_TASK",
            EdgeType::OccurredAt => "OCCURRED_AT",
            EdgeType::ClipboardCopied => "CLIPBOARD_COPIED",
            EdgeType::OccurredDuringAudio => "OCCURRED_DURING_AUDIO",
        });
        timestamps.append_value(e.timestamp);
        metadata.append_value(serde_json::to_string(&e.metadata).unwrap_or_default());
    }

    RecordBatch::try_new(
        Arc::new(edge_schema()),
        vec![
            Arc::new(ids.finish()),
            Arc::new(sources.finish()),
            Arc::new(targets.finish()),
            Arc::new(types.finish()),
            Arc::new(timestamps.finish()),
            Arc::new(metadata.finish()),
        ],
    )
    .map_err(|e| e.into())
}

fn batch_to_nodes(batch: &RecordBatch) -> Vec<GraphNode> {
    let n = batch.num_rows();
    let ids = str_col(batch, "id");
    let types = str_col(batch, "node_type");
    let labels = str_col(batch, "label");
    let created = i64_col(batch, "created_at");
    let meta = str_col(batch, "metadata_json");

    let mut nodes = Vec::with_capacity(n);
    for i in 0..n {
        let node_type = match get_str(&types, i).as_str() {
            "Entity" => NodeType::Entity,
            "Task" => NodeType::Task,
            "Url" => NodeType::Url,
            "Clipboard" => NodeType::Clipboard,
            "AudioSegment" => NodeType::AudioSegment,
            _ => NodeType::MemoryChunk,
        };
        nodes.push(GraphNode {
            id: get_str(&ids, i),
            node_type,
            label: get_str(&labels, i),
            created_at: get_i64(&created, i),
            metadata: serde_json::from_str(&get_str(&meta, i)).unwrap_or_default(),
        });
    }
    nodes
}

fn batch_to_edges(batch: &RecordBatch) -> Vec<GraphEdge> {
    let n = batch.num_rows();
    let ids = str_col(batch, "id");
    let sources = str_col(batch, "source");
    let targets = str_col(batch, "target");
    let types = str_col(batch, "edge_type");
    let ts = i64_col(batch, "timestamp");
    let meta = str_col(batch, "metadata_json");

    let mut edges = Vec::with_capacity(n);
    for i in 0..n {
        let edge_type = match get_str(&types, i).as_str() {
            "PART_OF_SESSION" | "PartOfSession" | "MentionedIn" => EdgeType::PartOfSession,
            "REFERENCE_FOR_TASK" | "ReferenceForTask" | "References" => EdgeType::ReferenceForTask,
            "CLIPBOARD_COPIED" | "ClipboardCopied" => EdgeType::ClipboardCopied,
            "OCCURRED_DURING_AUDIO" | "OccurredDuringAudio" => EdgeType::OccurredDuringAudio,
            "OCCURRED_AT" | "OccurredAt" | "LinkedTo" => EdgeType::OccurredAt,
            _ => EdgeType::OccurredAt,
        };
        edges.push(GraphEdge {
            id: get_str(&ids, i),
            source: get_str(&sources, i),
            target: get_str(&targets, i),
            edge_type,
            timestamp: get_i64(&ts, i),
            metadata: serde_json::from_str(&get_str(&meta, i)).unwrap_or_default(),
        });
    }
    edges
}

fn batch_to_meetings(batch: &RecordBatch) -> Vec<MeetingSession> {
    let n = batch.num_rows();
    let id = str_col(batch, "id");
    let title = str_col(batch, "title");
    let participants = batch
        .column_by_name("participants")
        .and_then(|c| c.as_any().downcast_ref::<arrow_array::ListArray>().cloned());
    let model = str_col(batch, "model");
    let status = str_col(batch, "status");
    let start = i64_col(batch, "start_timestamp");
    let end = i64_col(batch, "end_timestamp");
    let created = i64_col(batch, "created_at");
    let updated = i64_col(batch, "updated_at");
    let segment_count = i64_col(batch, "segment_count");
    let duration = i64_col(batch, "duration_seconds");
    let mdir = str_col(batch, "meeting_dir");
    let adir = str_col(batch, "audio_dir");
    let tpath = str_col(batch, "transcript_path");
    let breakdown = str_col(batch, "breakdown_json");

    let mut results = Vec::with_capacity(n);
    for i in 0..n {
        results.push(MeetingSession {
            id: get_str(&id, i),
            title: get_str(&title, i),
            participants: extract_str_list(&participants, i),
            model: get_str(&model, i),
            status: get_str(&status, i),
            start_timestamp: get_i64(&start, i),
            end_timestamp: Some(get_i64(&end, i)).filter(|t| *t > 0),
            created_at: get_i64(&created, i),
            updated_at: get_i64(&updated, i),
            segment_count: get_i64(&segment_count, i) as usize,
            duration_seconds: get_i64(&duration, i) as u64,
            meeting_dir: get_str(&mdir, i),
            audio_dir: get_str(&adir, i),
            transcript_path: Some(get_str(&tpath, i)).filter(|s| !s.is_empty()),
            breakdown: serde_json::from_str(&get_str(&breakdown, i)).ok(),
        });
    }
    results
}

fn batch_to_segments(batch: &RecordBatch) -> Vec<MeetingSegment> {
    let n = batch.num_rows();
    let id = str_col(batch, "id");
    let mid = str_col(batch, "meeting_id");
    let index = u32_col(batch, "index");
    let start = i64_col(batch, "start_timestamp");
    let end = i64_col(batch, "end_timestamp");
    let text = str_col(batch, "text");
    let audio = str_col(batch, "audio_chunk_path");
    let model = str_col(batch, "model");
    let created = i64_col(batch, "created_at");

    let mut results = Vec::with_capacity(n);
    for i in 0..n {
        results.push(MeetingSegment {
            id: get_str(&id, i),
            meeting_id: get_str(&mid, i),
            index: get_u32(&index, i),
            start_timestamp: get_i64(&start, i),
            end_timestamp: get_i64(&end, i),
            text: get_str(&text, i),
            audio_chunk_path: get_str(&audio, i),
            model: get_str(&model, i),
            created_at: get_i64(&created, i),
        });
    }
    results
}

fn batch_to_tasks(batch: &RecordBatch) -> Vec<Task> {
    let n = batch.num_rows();
    let ids = str_col(batch, "id");
    let titles = str_col(batch, "title");
    let descriptions = str_col(batch, "description");
    let source_apps = str_col(batch, "source_app");
    let source_memory_ids = str_col(batch, "source_memory_id");
    let created_at = i64_col(batch, "created_at");
    let due_date = i64_col(batch, "due_date");
    let is_completed = bool_col(batch, "is_completed");
    let is_dismissed = bool_col(batch, "is_dismissed");
    let task_types = str_col(batch, "task_type");

    let url_col = batch
        .column_by_name("linked_urls")
        .and_then(|c| c.as_any().downcast_ref::<arrow_array::ListArray>().cloned());
    let mem_id_col = batch
        .column_by_name("linked_memory_ids")
        .and_then(|c| c.as_any().downcast_ref::<arrow_array::ListArray>().cloned());

    (0..n)
        .map(|i| {
            let t_type = match get_str(&task_types, i).as_str() {
                "Reminder" => TaskType::Reminder,
                "Followup" => TaskType::Followup,
                _ => TaskType::Todo,
            };

            Task {
                id: get_str(&ids, i),
                title: get_str(&titles, i),
                description: get_str(&descriptions, i),
                source_app: get_str(&source_apps, i),
                source_memory_id: get_opt_str(&source_memory_ids, i),
                created_at: get_i64(&created_at, i),
                due_date: get_opt_i64(&due_date, i),
                is_completed: get_bool(&is_completed, i),
                is_dismissed: get_bool(&is_dismissed, i),
                task_type: t_type,
                linked_urls: extract_str_list(&url_col, i),
                linked_memory_ids: extract_str_list(&mem_id_col, i),
            }
        })
        .collect()
}

// ── Arrow ↔ Meeting conversion ───────────────────────────────────────────────

fn meeting_to_batch(meetings: &[MeetingSession]) -> Result<RecordBatch, ArrowError> {
    let schema = Arc::new(meeting_schema());
    let ids: Vec<&str> = meetings.iter().map(|m| m.id.as_str()).collect();
    let titles: Vec<&str> = meetings.iter().map(|m| m.title.as_str()).collect();
    let models: Vec<&str> = meetings.iter().map(|m| m.model.as_str()).collect();
    let statuses: Vec<&str> = meetings.iter().map(|m| m.status.as_str()).collect();
    let starts: Vec<i64> = meetings.iter().map(|m| m.start_timestamp).collect();
    let ends: Vec<Option<i64>> = meetings.iter().map(|m| m.end_timestamp).collect();
    let created: Vec<i64> = meetings.iter().map(|m| m.created_at).collect();
    let updated: Vec<i64> = meetings.iter().map(|m| m.updated_at).collect();
    let counts: Vec<i64> = meetings.iter().map(|m| m.segment_count as i64).collect();
    let durations: Vec<i64> = meetings.iter().map(|m| m.duration_seconds as i64).collect();
    let meeting_dirs: Vec<&str> = meetings.iter().map(|m| m.meeting_dir.as_str()).collect();
    let audio_dirs: Vec<&str> = meetings.iter().map(|m| m.audio_dir.as_str()).collect();
    let transcript_paths: Vec<Option<&str>> = meetings
        .iter()
        .map(|m| m.transcript_path.as_deref())
        .collect();
    let breakdowns: Vec<Option<String>> = meetings
        .iter()
        .map(|m| {
            m.breakdown
                .as_ref()
                .and_then(|b| serde_json::to_string(b).ok())
        })
        .collect();

    let mut participants_builder =
        arrow_array::builder::ListBuilder::new(arrow_array::builder::StringBuilder::new());
    for m in meetings {
        for p in &m.participants {
            participants_builder.values().append_value(p);
        }
        participants_builder.append(true);
    }

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(ids)),
            Arc::new(StringArray::from(titles)),
            Arc::new(participants_builder.finish()),
            Arc::new(StringArray::from(models)),
            Arc::new(StringArray::from(statuses)),
            Arc::new(Int64Array::from(starts)),
            Arc::new(Int64Array::from(ends)),
            Arc::new(Int64Array::from(created)),
            Arc::new(Int64Array::from(updated)),
            Arc::new(Int64Array::from(counts)),
            Arc::new(Int64Array::from(durations)),
            Arc::new(StringArray::from(meeting_dirs)),
            Arc::new(StringArray::from(audio_dirs)),
            Arc::new(StringArray::from(transcript_paths)),
            Arc::new(StringArray::from(breakdowns)),
        ],
    )
}

fn segment_to_batch(segments: &[MeetingSegment]) -> Result<RecordBatch, ArrowError> {
    let schema = Arc::new(segment_schema());
    let ids: Vec<&str> = segments.iter().map(|s| s.id.as_str()).collect();
    let m_ids: Vec<&str> = segments.iter().map(|s| s.meeting_id.as_str()).collect();
    let indices: Vec<u32> = segments.iter().map(|s| s.index).collect();
    let starts: Vec<i64> = segments.iter().map(|s| s.start_timestamp).collect();
    let ends: Vec<i64> = segments.iter().map(|s| s.end_timestamp).collect();
    let texts: Vec<&str> = segments.iter().map(|s| s.text.as_str()).collect();
    let paths: Vec<&str> = segments
        .iter()
        .map(|s| s.audio_chunk_path.as_str())
        .collect();
    let models: Vec<&str> = segments.iter().map(|s| s.model.as_str()).collect();
    let created: Vec<i64> = segments.iter().map(|s| s.created_at).collect();

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(ids)),
            Arc::new(StringArray::from(m_ids)),
            Arc::new(arrow_array::UInt32Array::from(indices)),
            Arc::new(Int64Array::from(starts)),
            Arc::new(Int64Array::from(ends)),
            Arc::new(StringArray::from(texts)),
            Arc::new(StringArray::from(paths)),
            Arc::new(StringArray::from(models)),
            Arc::new(Int64Array::from(created)),
        ],
    )
}

// ── Arrow ↔ Graph conversion ─────────────────────────────────────────────────

fn node_to_batch(nodes: &[GraphNode]) -> Result<RecordBatch, ArrowError> {
    let schema = Arc::new(node_schema());
    let ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let types: Vec<String> = nodes.iter().map(|n| format!("{:?}", n.node_type)).collect();
    let labels: Vec<&str> = nodes.iter().map(|n| n.label.as_str()).collect();
    let created: Vec<i64> = nodes.iter().map(|n| n.created_at).collect();
    let metadata: Vec<String> = nodes.iter().map(|n| n.metadata.to_string()).collect();

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(ids)),
            Arc::new(StringArray::from(types)),
            Arc::new(StringArray::from(labels)),
            Arc::new(Int64Array::from(created)),
            Arc::new(StringArray::from(metadata)),
        ],
    )
}

fn edge_to_batch(edges: &[GraphEdge]) -> Result<RecordBatch, ArrowError> {
    let schema = Arc::new(edge_schema());
    let ids: Vec<&str> = edges.iter().map(|e| e.id.as_str()).collect();
    let sources: Vec<&str> = edges.iter().map(|e| e.source.as_str()).collect();
    let targets: Vec<&str> = edges.iter().map(|e| e.target.as_str()).collect();
    let types: Vec<String> = edges.iter().map(|e| format!("{:?}", e.edge_type)).collect();
    let timestamps: Vec<i64> = edges.iter().map(|e| e.timestamp).collect();
    let metadata: Vec<String> = edges.iter().map(|e| e.metadata.to_string()).collect();

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(ids)),
            Arc::new(StringArray::from(sources)),
            Arc::new(StringArray::from(targets)),
            Arc::new(StringArray::from(types)),
            Arc::new(Int64Array::from(timestamps)),
            Arc::new(StringArray::from(metadata)),
        ],
    )
}

fn build_filter(time_filter: Option<&str>, app_filter: Option<&str>) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    if let Some(tf) = time_filter.and_then(time_filter_to_sql) {
        parts.push(tf);
    }
    if let Some(app) = app_filter {
        parts.push(format!("app_name = '{}'", sql_escape(app)));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" AND "))
    }
}

fn time_filter_to_sql(tf: &str) -> Option<String> {
    use chrono::Duration;
    let now = chrono::Utc::now();
    match tf {
        "1h" => Some(format!(
            "timestamp >= {}",
            (now - Duration::hours(1)).timestamp_millis()
        )),
        "24h" => Some(format!(
            "timestamp >= {}",
            (now - Duration::hours(24)).timestamp_millis()
        )),
        "7d" | "week" => Some(format!(
            "timestamp >= {}",
            (now - Duration::days(7)).timestamp_millis()
        )),
        "today" => local_day_range_filter(0),
        "yesterday" => local_day_range_filter(1),
        _ => None,
    }
}

fn local_day_bucket_now() -> String {
    Local::now().format("%Y-%m-%d").to_string()
}

fn local_day_bucket_from_timestamp(timestamp: i64) -> String {
    Local
        .timestamp_millis_opt(timestamp)
        .single()
        .unwrap_or_else(Local::now)
        .format("%Y-%m-%d")
        .to_string()
}

fn local_day_range_filter(days_ago: i64) -> Option<String> {
    let target_day = Local::now().date_naive() - chrono::Duration::days(days_ago);
    let start = target_day.and_hms_opt(0, 0, 0)?;
    let end = (target_day + chrono::Duration::days(1)).and_hms_opt(0, 0, 0)?;

    let start_ms = Local
        .from_local_datetime(&start)
        .earliest()
        .or_else(|| Local.from_local_datetime(&start).latest())?
        .timestamp_millis();
    let end_ms = Local
        .from_local_datetime(&end)
        .earliest()
        .or_else(|| Local.from_local_datetime(&end).latest())?
        .timestamp_millis();

    Some(format!(
        "timestamp >= {} AND timestamp < {}",
        start_ms, end_ms
    ))
}

fn normalize_keyword_text(input: &str) -> String {
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

fn is_keyword_stop_word(token: &str) -> bool {
    matches!(
        token,
        "a" | "an"
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
    )
}

fn keyword_terms(query: &str) -> Vec<String> {
    let normalized = normalize_keyword_text(query);
    if normalized.is_empty() {
        return Vec::new();
    }

    let mut terms = Vec::new();
    // Keep the normalized query as a phrase candidate first.
    terms.push(normalized.clone());

    for token in normalized.split_whitespace() {
        if token.len() <= 1 {
            continue;
        }
        if is_keyword_stop_word(token) && !token.chars().any(|ch| ch.is_ascii_digit()) {
            continue;
        }
        if !terms.iter().any(|existing| existing == token) {
            terms.push(token.to_string());
        }
    }

    terms.truncate(10);
    terms
}

fn lexical_keyword_score(terms: &[String], result: &SearchResult) -> f32 {
    if terms.is_empty() {
        return 0.0;
    }

    let title = normalize_keyword_text(&result.window_title);
    let snippet = normalize_keyword_text(&result.snippet);
    let lexical_shadow = normalize_keyword_text(&result.lexical_shadow);
    let clean = normalize_keyword_text(if !result.clean_text.trim().is_empty() {
        &result.clean_text
    } else {
        &result.text
    });
    let app = normalize_keyword_text(&result.app_name);
    let url = result
        .url
        .as_ref()
        .map(|value| normalize_keyword_text(value))
        .unwrap_or_default();
    let merged = format!("{} {} {} {} {}", title, snippet, clean, lexical_shadow, url);

    let mut matched_terms = 0usize;
    let mut weighted = 0.0f32;

    for (idx, term) in terms.iter().enumerate() {
        let mut matched = false;
        if title.contains(term) {
            weighted += 1.8;
            matched = true;
        }
        if snippet.contains(term) {
            weighted += 1.35;
            matched = true;
        }
        if clean.contains(term) {
            weighted += 1.1;
            matched = true;
        }
        if lexical_shadow.contains(term) {
            weighted += 1.05;
            matched = true;
        }
        if app.contains(term) {
            weighted += 0.75;
            matched = true;
        }
        if !url.is_empty() && url.contains(term) {
            weighted += 0.95;
            matched = true;
        }

        // Reward full sentence/phrase hits for sentence queries.
        if idx == 0 && term.split_whitespace().count() >= 2 && merged.contains(term) {
            weighted += 1.1;
            matched = true;
        }

        if matched {
            matched_terms += 1;
        }
    }

    let coverage = matched_terms as f32 / terms.len() as f32;
    let normalized = (weighted / (terms.len() as f32 * 2.8)).min(1.0);
    (normalized * 0.7 + coverage * 0.3).clamp(0.0, 1.0)
}

fn recency_score(now_ms: i64, timestamp_ms: i64) -> f32 {
    let age_hours = ((now_ms - timestamp_ms).max(0) as f32 / 3_600_000.0).min(24.0 * 30.0);
    (1.0 / (1.0 + age_hours * 0.03)).clamp(0.0, 1.0)
}

fn estimate_signal_strength(
    summary_source: &str,
    ocr_confidence: f32,
    noise_score: f32,
    snippet: &str,
    clean_text: &str,
) -> f32 {
    let summary_weight = match summary_source.trim().to_ascii_lowercase().as_str() {
        "llm" => 1.0,
        "vlm" => 0.9,
        "fallback" => 0.66,
        _ => 0.58,
    };
    let snippet_density = (normalize_keyword_text(snippet)
        .split_whitespace()
        .count()
        .min(24) as f32
        / 24.0)
        .clamp(0.0, 1.0);
    let text_density = (normalize_keyword_text(clean_text)
        .split_whitespace()
        .count()
        .min(80) as f32
        / 80.0)
        .clamp(0.0, 1.0);

    (ocr_confidence.clamp(0.0, 1.0) * 0.24
        + (1.0 - noise_score.clamp(0.0, 1.0)) * 0.28
        + summary_weight * 0.18
        + snippet_density * 0.16
        + text_density * 0.14)
        .clamp(0.0, 1.0)
}

fn estimate_record_signal_strength(record: &MemoryRecord) -> f32 {
    estimate_signal_strength(
        &record.summary_source,
        record.ocr_confidence,
        record.noise_score,
        &record.snippet,
        &record.clean_text,
    )
}

fn normalize_record_for_index(record: &MemoryRecord) -> MemoryRecord {
    let lexical_shadow = if record.lexical_shadow.trim().is_empty() {
        build_lexical_shadow(
            &record.window_title,
            &record.snippet,
            &record.clean_text,
            record.url.as_deref(),
        )
    } else {
        record.lexical_shadow.clone()
    };
    let mut normalized = compact_memory_record_payload(record);
    normalized.url = sanitize_index_url(
        normalized.url.as_deref(),
        &normalized.window_title,
        &normalized.snippet,
    );
    normalized.session_key = build_index_session_key(&normalized);
    normalized.lexical_shadow = lexical_shadow;
    normalized
}

fn sanitize_index_url(url: Option<&str>, title: &str, snippet: &str) -> Option<String> {
    let raw = url?.trim();
    if raw.is_empty() {
        return None;
    }

    let normalized = canonicalize_index_url(raw);
    let domain = extract_domain(&normalized)?;
    let context = normalize_keyword_text(&format!("{title} {snippet}"));
    let path = extract_path_segments(&normalized, 3).unwrap_or_default();

    if INDEX_NOISE_HOSTS.iter().any(|host| domain == *host) {
        return None;
    }
    if looks_like_auth_or_error_context(&context) {
        return None;
    }
    if !path.is_empty() && is_low_entropy_path(&path) && context.split_whitespace().count() < 6 {
        return None;
    }

    Some(normalized)
}

fn canonicalize_index_url(url: &str) -> String {
    let no_fragment = url.split('#').next().unwrap_or(url);
    let no_query = no_fragment.split('?').next().unwrap_or(no_fragment);
    no_query.trim_end_matches('/').to_string()
}

fn build_index_session_key(record: &MemoryRecord) -> String {
    if record.session_key.starts_with("meeting:") {
        return record.session_key.clone();
    }

    let app = normalize_app_key(&record.app_name);
    if let Some(url) = record.url.as_deref() {
        if let Some(domain) = extract_domain(url) {
            if let Some(path) = extract_path_segments(url, 2) {
                if !path.is_empty() {
                    return format!("{app}:{domain}:{path}");
                }
            }
            return format!("{app}:{domain}");
        }
    }

    let title_key = normalize_anchor_key(&record.window_title);
    if !title_key.is_empty() {
        return format!("{app}:title:{title_key}");
    }

    let snippet_key = normalize_anchor_key(&record.snippet);
    if !snippet_key.is_empty() {
        return format!("{app}:snippet:{snippet_key}");
    }

    if !record.session_key.trim().is_empty() {
        return record.session_key.clone();
    }

    app
}

fn normalize_app_key(app_name: &str) -> String {
    let normalized = app_name
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    let compact = normalized
        .split('_')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    if compact.is_empty() {
        "unknown".to_string()
    } else {
        compact
    }
}

fn normalize_anchor_key(text: &str) -> String {
    normalize_keyword_text(text)
        .split_whitespace()
        .filter(|token| token.len() > 2)
        .take(8)
        .collect::<Vec<_>>()
        .join("_")
}

fn extract_path_segments(url: &str, count: usize) -> Option<String> {
    let without_scheme = url.split("://").nth(1).unwrap_or(url);
    let mut parts = without_scheme.split('/');
    let _host = parts.next()?;
    let segments = parts
        .filter(|segment| !segment.trim().is_empty())
        .map(|segment| {
            normalize_keyword_text(segment)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join("_")
        })
        .filter(|segment| !segment.is_empty())
        .take(count)
        .collect::<Vec<_>>();

    if segments.is_empty() {
        None
    } else {
        Some(segments.join("/"))
    }
}

fn is_low_entropy_path(path: &str) -> bool {
    let normalized = normalize_keyword_text(path);
    let tokens = normalized
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return true;
    }

    let unique = tokens
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>()
        .len();
    unique <= 2
        && tokens.iter().all(|token| {
            matches!(
                *token,
                "404" | "500" | "account" | "auth" | "error" | "login" | "signin"
            )
        })
}

fn looks_like_auth_or_error_context(context: &str) -> bool {
    context.contains("sign in")
        || context.contains("log in")
        || context.contains("authenticate")
        || context.contains("authorization")
        || context.contains("404")
        || context.contains("500")
        || context.contains("not found")
        || context.starts_with("error ")
}

fn dedup_records_for_insert(records: &[MemoryRecord]) -> Vec<MemoryRecord> {
    let mut by_key: HashMap<String, MemoryRecord> = HashMap::new();

    for record in records {
        let key = record_insert_dedup_key(record);
        by_key
            .entry(key)
            .and_modify(|existing| {
                let existing_rank = estimate_record_signal_strength(existing);
                let incoming_rank = estimate_record_signal_strength(record);
                if incoming_rank > existing_rank
                    || (incoming_rank == existing_rank && record.timestamp > existing.timestamp)
                {
                    *existing = record.clone();
                }
            })
            .or_insert_with(|| record.clone());
    }

    by_key.into_values().collect()
}

fn dedup_search_results(mut results: Vec<SearchResult>, limit: usize) -> Vec<SearchResult> {
    if results.is_empty() {
        return results;
    }

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.timestamp.cmp(&a.timestamp))
    });

    let mut by_key: HashMap<String, SearchResult> = HashMap::new();
    for result in results {
        let key = search_result_dedup_key(&result);
        by_key
            .entry(key)
            .and_modify(|existing| {
                if result.score > existing.score
                    || (result.score == existing.score && result.timestamp > existing.timestamp)
                {
                    *existing = result.clone();
                }
            })
            .or_insert(result);
    }

    let mut out: Vec<SearchResult> = by_key.into_values().collect();
    out.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.timestamp.cmp(&a.timestamp))
    });
    out.truncate(limit.max(1));
    out
}

fn record_insert_dedup_key(record: &MemoryRecord) -> String {
    format!(
        "{}:{}:{}:{}:{}",
        record.app_name.to_lowercase(),
        normalize_keyword_text(&record.session_key),
        normalize_keyword_text(&record.window_title),
        normalize_keyword_text(&record.snippet),
        record.timestamp / 15_000
    )
}

fn search_result_dedup_key(result: &SearchResult) -> String {
    let domain = result
        .url
        .as_deref()
        .and_then(extract_domain)
        .unwrap_or_default();
    let session = if result.session_key.trim().is_empty() {
        result.session_id.to_lowercase()
    } else {
        result.session_key.to_lowercase()
    };
    format!(
        "{}:{}:{}:{}:{}",
        result.app_name.to_lowercase(),
        session,
        domain,
        normalize_keyword_text(&result.window_title),
        normalize_keyword_text(&result.snippet)
    )
}

/// Escape single quotes for SQL string literals.
fn sql_escape(s: &str) -> String {
    s.replace('\'', "''")
}

// ── DB initialization ─────────────────────────────────────────────────────────

async fn open_all_tables(
    db_path: &Path,
) -> Result<(Table, Table, Table, Table, Table, Table), lancedb::Error> {
    let uri = db_path.to_string_lossy();
    let conn: Connection = lancedb::connect(&uri).execute().await?;
    let names = conn.table_names().execute().await?;

    let table =
        open_or_create_named_table(&conn, &names, MEMORIES_TABLE, Arc::new(memory_schema()))
            .await?;
    ensure_memory_schema_columns(&table).await?;

    let tasks =
        open_or_create_named_table(&conn, &names, TASKS_TABLE, Arc::new(task_schema())).await?;
    let meetings =
        open_or_create_named_table(&conn, &names, MEETINGS_TABLE, Arc::new(meeting_schema()))
            .await?;
    let segments =
        open_or_create_named_table(&conn, &names, SEGMENTS_TABLE, Arc::new(segment_schema()))
            .await?;
    let nodes =
        open_or_create_named_table(&conn, &names, NODES_TABLE, Arc::new(node_schema())).await?;
    let edges =
        open_or_create_named_table(&conn, &names, EDGES_TABLE, Arc::new(edge_schema())).await?;

    Ok((table, tasks, meetings, segments, nodes, edges))
}

async fn open_or_create_named_table(
    conn: &Connection,
    existing_tables: &[String],
    name: &str,
    schema: Arc<Schema>,
) -> Result<Table, lancedb::Error> {
    if existing_tables.contains(&name.to_string()) {
        conn.open_table(name).execute().await
    } else {
        let empty = RecordBatchIterator::new(
            std::iter::empty::<Result<RecordBatch, ArrowError>>(),
            schema,
        );
        conn.create_table(name, Box::new(empty) as Box<dyn RecordBatchReader + Send>)
            .execute()
            .await
    }
}

async fn ensure_memory_schema_columns(table: &Table) -> Result<(), lancedb::Error> {
    let schema = table.schema().await?;
    let existing: std::collections::HashSet<String> = schema
        .fields()
        .iter()
        .map(|field| field.name().to_string())
        .collect();

    let mut transforms: Vec<(String, String)> = Vec::new();
    if !existing.contains("clean_text") {
        transforms.push(("clean_text".to_string(), "text".to_string()));
    }
    if !existing.contains("ocr_confidence") {
        transforms.push((
            "ocr_confidence".to_string(),
            "CAST(0.0 AS FLOAT)".to_string(),
        ));
    }
    if !existing.contains("ocr_block_count") {
        transforms.push((
            "ocr_block_count".to_string(),
            "CAST(0 AS BIGINT)".to_string(),
        ));
    }
    if !existing.contains("summary_source") {
        transforms.push(("summary_source".to_string(), "'fallback'".to_string()));
    }
    if !existing.contains("noise_score") {
        transforms.push(("noise_score".to_string(), "CAST(0.0 AS FLOAT)".to_string()));
    }
    if !existing.contains("session_key") {
        transforms.push(("session_key".to_string(), "''".to_string()));
    }
    if !existing.contains("lexical_shadow") {
        transforms.push(("lexical_shadow".to_string(), "''".to_string()));
    }
    if !existing.contains("snippet_embedding") {
        // Placeholder zeros — will be computed properly for new captures.
        transforms.push(("snippet_embedding".to_string(), "embedding".to_string()));
    }
    if !existing.contains("support_embedding") {
        transforms.push(("support_embedding".to_string(), "embedding".to_string()));
    }
    if !existing.contains("decay_score") {
        transforms.push(("decay_score".to_string(), "CAST(1.0 AS FLOAT)".to_string()));
    }
    if !existing.contains("last_accessed_at") {
        transforms.push(("last_accessed_at".to_string(), "timestamp".to_string()));
    }

    if !transforms.is_empty() {
        tracing::info!(
            "Migrating LanceDB memories table schema with {} new columns",
            transforms.len()
        );
        table
            .add_columns(NewColumnTransform::SqlExpressions(transforms), None)
            .await?;
    }

    Ok(())
}

// ── Migration from legacy JSON store ─────────────────────────────────────────

async fn migrate_from_json(table: &Table, json_path: &Path) {
    let result: Result<(), Box<dyn std::error::Error>> = (async {
        let data = std::fs::read(json_path)?;
        let mut records: Vec<MemoryRecord> = serde_json::from_slice(&data)?;
        if records.is_empty() {
            return Ok(());
        }

        // Backfill day_bucket for legacy records that predate the field.
        for r in &mut records {
            if r.day_bucket.is_empty() {
                r.day_bucket = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(r.timestamp)
                    .unwrap_or_else(chrono::Utc::now)
                    .format("%Y-%m-%d")
                    .to_string();
            }
        }
        records = records.iter().map(normalize_record_for_index).collect();
        records = dedup_records_for_insert(&records);

        tracing::info!(
            "Migrating {} records from memories.json to LanceDB",
            records.len()
        );

        // Insert in chunks to avoid huge Arrow batches.
        for chunk in records.chunks(500) {
            let batch = records_to_batch(chunk)?;
            let schema = Arc::new(memory_schema());
            let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
            table
                .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
                .execute()
                .await?;
        }

        // Remove the legacy JSON source once migration has completed successfully.
        std::fs::remove_file(json_path)?;

        tracing::info!("Migration complete");
        Ok(())
    })
    .await;

    if let Err(e) = result {
        tracing::warn!("JSON migration failed (data not lost): {}", e);
    }
}
async fn migrate_tasks_from_json(table: &Table, json_path: &Path) {
    let result: Result<(), Box<dyn std::error::Error>> = (async {
        let data = std::fs::read(json_path)?;
        let tasks: Vec<Task> = serde_json::from_slice(&data)?;
        if tasks.is_empty() {
            return Ok(());
        }
        tracing::info!("Migrating {} tasks to LanceDB", tasks.len());
        let batch = task_to_batch(&tasks)?;
        let schema = Arc::new(task_schema());
        let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
        table
            .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
            .mode(AddDataMode::Overwrite)
            .execute()
            .await?;
        std::fs::remove_file(json_path)?;
        Ok(())
    })
    .await;
    if let Err(e) = result {
        tracing::warn!("Task migration failed: {}", e);
    }
}

async fn migrate_meetings_from_json(table: &Table, json_path: &Path) {
    let result: Result<(), Box<dyn std::error::Error>> = (async {
        let data = std::fs::read(json_path)?;
        let meetings: Vec<MeetingSession> = serde_json::from_slice(&data)?;
        if meetings.is_empty() {
            return Ok(());
        }
        tracing::info!("Migrating {} meetings to LanceDB", meetings.len());
        let batch = meeting_to_batch(&meetings)?;
        let schema = Arc::new(meeting_schema());
        let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
        table
            .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
            .mode(AddDataMode::Overwrite)
            .execute()
            .await?;
        std::fs::remove_file(json_path)?;
        Ok(())
    })
    .await;
    if let Err(e) = result {
        tracing::warn!("Meeting migration failed: {}", e);
    }
}

async fn migrate_segments_from_json(table: &Table, json_path: &Path) {
    let result: Result<(), Box<dyn std::error::Error>> = (async {
        let data = std::fs::read(json_path)?;
        let segments: Vec<MeetingSegment> = serde_json::from_slice(&data)?;
        if segments.is_empty() {
            return Ok(());
        }
        tracing::info!("Migrating {} segments to LanceDB", segments.len());
        let batch = segment_to_batch(&segments)?;
        let schema = Arc::new(segment_schema());
        let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
        table
            .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
            .mode(AddDataMode::Overwrite)
            .execute()
            .await?;
        std::fs::remove_file(json_path)?;
        Ok(())
    })
    .await;
    if let Err(e) = result {
        tracing::warn!("Segment migration failed: {}", e);
    }
}

async fn migrate_graph_from_json(nodes_table: &Table, edges_table: &Table, json_path: &Path) {
    #[derive(serde::Deserialize)]
    struct LegacyGraph {
        nodes: Vec<GraphNode>,
        edges: Vec<GraphEdge>,
    }

    let result: Result<(), Box<dyn std::error::Error>> = (async {
        let data = std::fs::read(json_path)?;
        let graph: LegacyGraph = serde_json::from_slice(&data)?;
        if !graph.nodes.is_empty() {
            tracing::info!("Migrating {} graph nodes to LanceDB", graph.nodes.len());
            let batch = node_to_batch(&graph.nodes)?;
            let schema = Arc::new(node_schema());
            let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
            nodes_table
                .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
                .mode(AddDataMode::Overwrite)
                .execute()
                .await?;
        }
        if !graph.edges.is_empty() {
            tracing::info!("Migrating {} graph edges to LanceDB", graph.edges.len());
            let batch = edge_to_batch(&graph.edges)?;
            let schema = Arc::new(edge_schema());
            let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
            edges_table
                .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
                .mode(AddDataMode::Overwrite)
                .execute()
                .await?;
        }
        std::fs::remove_file(json_path)?;
        Ok(())
    })
    .await;
    if let Err(e) = result {
        tracing::warn!("Graph migration failed: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(url: Option<&str>, title: &str, snippet: &str) -> MemoryRecord {
        MemoryRecord {
            id: "memory-1".to_string(),
            timestamp: 1_000,
            day_bucket: "2026-04-17".to_string(),
            app_name: "Chrome".to_string(),
            bundle_id: None,
            window_title: title.to_string(),
            session_id: "session-1".to_string(),
            text: snippet.to_string(),
            clean_text: snippet.to_string(),
            ocr_confidence: 0.9,
            ocr_block_count: 4,
            snippet: snippet.to_string(),
            summary_source: "llm".to_string(),
            noise_score: 0.1,
            session_key: String::new(),
            lexical_shadow: String::new(),
            embedding: vec![0.0; 384],
            image_embedding: vec![0.0; 512],
            screenshot_path: None,
            url: url.map(|value| value.to_string()),
            snippet_embedding: vec![0.0; 384],
            support_embedding: vec![0.0; 384],
            decay_score: 1.0,
            last_accessed_at: 0,
        }
    }

    #[test]
    fn normalize_record_for_index_suppresses_auth_urls() {
        let normalized = normalize_record_for_index(&record(
            Some("https://accounts.google.com/signin/v2/challenge?foo=bar"),
            "Sign in",
            "Sign in to continue",
        ));
        assert!(normalized.url.is_none());
        assert_eq!(normalized.session_key, "chrome:title:sign");
    }

    #[test]
    fn normalize_record_for_index_keeps_specific_paths() {
        let normalized = normalize_record_for_index(&record(
            Some("https://docs.example.com/projects/fndr/pipeline?view=full"),
            "Pipeline design",
            "Reviewed the FNDR pipeline design and search notes",
        ));
        assert_eq!(
            normalized.url.as_deref(),
            Some("https://docs.example.com/projects/fndr/pipeline")
        );
        assert_eq!(
            normalized.session_key,
            "chrome:docs.example.com:projects/fndr"
        );
    }

    #[test]
    fn normalize_record_for_index_compacts_payload_fields() {
        let mut source = record(
            Some("https://example.com/research"),
            "Research notes",
            "Summarized the research notes for memory card storage.",
        );
        source.text = "raw noisy ocr block".to_string();
        source.clean_text = "raw noisy ocr block with repeated lines".to_string();
        source.screenshot_path = Some("/tmp/frame.png".to_string());

        let normalized = normalize_record_for_index(&source);
        assert!(normalized.text.is_empty());
        assert!(normalized.screenshot_path.is_none());
        assert_eq!(normalized.clean_text, source.snippet);
    }
}
