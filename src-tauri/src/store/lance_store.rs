//! LanceDB-backed storage for FNDR memory records.
//!
//! Replaces the JSON-based simple_store with a proper vector database.
//! All methods that touch LanceDB are async.

use super::schema::{AppCount, MemoryRecord, SearchResult, Stats};
use arrow_array::{
    Array, FixedSizeListArray, Float32Array, Int64Array, RecordBatch, RecordBatchIterator,
    StringArray,
};
use arrow_schema::{ArrowError, DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::{Connection, Table};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TABLE_NAME: &str = "memories";
const TEXT_EMBED_DIM: i32 = 384;
const IMAGE_EMBED_DIM: i32 = 512;

/// LanceDB-backed store for memory records.
pub struct Store {
    data_dir: PathBuf,
    table: Table,
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

        let table = rt.block_on(open_or_create_table(&db_path))?;

        // Migrate from legacy memories.json if present.
        let json_path = data_dir.join("memories.json");
        if json_path.exists() {
            rt.block_on(migrate_from_json(&table, &json_path));
        }

        Ok(Self { data_dir, table })
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
        let batch = records_to_batch(records)?;
        let schema = Arc::new(memory_schema());
        let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.table.add(Box::new(iter)).execute().await?;
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

        let mut vq = self
            .table
            .vector_search(query_vec)?
            .column("embedding")
            .limit(limit);

        if let Some(f) = filter {
            vq = vq.filter(f);
        }

        let batches: Vec<RecordBatch> = vq.execute().await?.try_collect().await?;
        let mut results = Vec::new();
        for batch in &batches {
            results.extend(batch_to_search_results(batch));
        }
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
        let q = sql_escape(query);
        let keyword_pred = format!(
            "(text LIKE '%{q}%' OR app_name LIKE '%{q}%' OR window_title LIKE '%{q}%')"
        );

        let filter = match build_filter(time_filter, app_filter) {
            Some(f) => format!("{keyword_pred} AND {f}"),
            None => keyword_pred,
        };

        let batches: Vec<RecordBatch> = self
            .table
            .query()
            .filter(filter)
            .limit(limit)
            .execute()
            .await?
            .try_collect()
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            let mut batch_results = batch_to_search_results(batch);
            // Keyword results get a fixed relevance score of 1.0.
            for r in &mut batch_results {
                r.score = 1.0;
            }
            results.extend(batch_results);
        }
        // Sort newest-first.
        results.sort_by_key(|r| std::cmp::Reverse(r.timestamp));
        Ok(results)
    }

    /// Return basic statistics about stored data.
    pub async fn get_stats(&self) -> Result<Stats, Box<dyn std::error::Error>> {
        let batches: Vec<RecordBatch> = self
            .table
            .query()
            .execute()
            .await?
            .try_collect()
            .await?;

        let total_records: usize = batches.iter().map(|b| b.num_rows()).sum();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let mut days = std::collections::HashSet::new();
        let mut app_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut today_count: usize = 0;

        for batch in &batches {
            let day_col = batch
                .column_by_name("day_bucket")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned());
            let app_col = batch
                .column_by_name("app_name")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned());

            for i in 0..batch.num_rows() {
                if let Some(ref dc) = day_col {
                    let day = dc.value(i).to_string();
                    if day == today {
                        today_count += 1;
                    }
                    days.insert(day);
                }
                if let Some(ref ac) = app_col {
                    let app = ac.value(i).to_string();
                    *app_counts.entry(app).or_insert(0) += 1;
                }
            }
        }

        let mut apps: Vec<AppCount> = app_counts
            .into_iter()
            .map(|(name, count)| AppCount { name, count })
            .collect();
        apps.sort_by(|a, b| b.count.cmp(&a.count));
        apps.truncate(10);

        Ok(Stats {
            total_records,
            total_days: days.len(),
            apps,
            today_count,
        })
    }

    /// Delete all records.
    pub async fn delete_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.table.delete("id IS NOT NULL").await?;
        Ok(())
    }

    /// Return sorted list of unique app names.
    pub async fn get_app_names(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let batches: Vec<RecordBatch> = self
            .table
            .query()
            .execute()
            .await?
            .try_collect()
            .await?;

        let mut names = std::collections::HashSet::new();
        for batch in &batches {
            if let Some(col) = batch
                .column_by_name("app_name")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned())
            {
                for i in 0..batch.num_rows() {
                    names.insert(col.value(i).to_string());
                }
            }
        }
        let mut list: Vec<String> = names.into_iter().collect();
        list.sort();
        Ok(list)
    }

    /// Delete records older than `days` days; returns count of deleted rows.
    pub async fn delete_older_than(
        &self,
        days: u32,
    ) -> Result<usize, Box<dyn std::error::Error>> {
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
            .filter(format!("timestamp >= {cutoff_ms}"))
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

    /// Fetch a single record by id.
    pub async fn get_memory_by_id(
        &self,
        memory_id: &str,
    ) -> Result<Option<MemoryRecord>, Box<dyn std::error::Error>> {
        let id = sql_escape(memory_id);
        let batches: Vec<RecordBatch> = self
            .table
            .query()
            .filter(format!("id = '{id}'"))
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
            .filter("url IS NOT NULL".to_string())
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
        Field::new("snippet", DataType::Utf8, false),
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
    let snippets: Vec<&str> = records.iter().map(|r| r.snippet.as_str()).collect();
    let screenshot_paths: Vec<Option<&str>> =
        records.iter().map(|r| r.screenshot_path.as_deref()).collect();
    let urls: Vec<Option<&str>> = records.iter().map(|r| r.url.as_deref()).collect();

    // Text embeddings — flatten all embeddings into one Float32Array.
    let flat_text: Vec<f32> = records.iter().flat_map(|r| r.embedding.iter().copied()).collect();
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
            Arc::new(StringArray::from(snippets)),
            Arc::new(embedding_array),
            Arc::new(image_embedding_array),
            Arc::new(StringArray::from(screenshot_paths)),
            Arc::new(StringArray::from(urls)),
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
    let snippets = str_col(batch, "snippet");
    let screenshot_paths = str_col(batch, "screenshot_path");
    let urls = str_col(batch, "url");

    let embed_col = batch
        .column_by_name("embedding")
        .and_then(|c| c.as_any().downcast_ref::<FixedSizeListArray>().cloned());
    let img_col = batch
        .column_by_name("image_embedding")
        .and_then(|c| c.as_any().downcast_ref::<FixedSizeListArray>().cloned());

    (0..n)
        .map(|i| {
            let embedding = extract_f32_list(&embed_col, i, TEXT_EMBED_DIM as usize);
            let image_embedding = extract_f32_list(&img_col, i, IMAGE_EMBED_DIM as usize);
            MemoryRecord {
                id: get_str(&ids, i),
                timestamp: timestamps.as_ref().map(|c| c.value(i)).unwrap_or(0),
                day_bucket: get_str(&day_buckets, i),
                app_name: get_str(&app_names, i),
                bundle_id: get_opt_str(&bundle_ids, i),
                window_title: get_str(&window_titles, i),
                session_id: get_str(&session_ids, i),
                text: get_str(&texts, i),
                snippet: get_str(&snippets, i),
                embedding,
                image_embedding,
                screenshot_path: get_opt_str(&screenshot_paths, i),
                url: get_opt_str(&urls, i),
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
    let snippets = str_col(batch, "snippet");
    let screenshot_paths = str_col(batch, "screenshot_path");
    let urls = str_col(batch, "url");

    // LanceDB appends _distance column to vector search results.
    let dist_col = batch
        .column_by_name("_distance")
        .and_then(|c| c.as_any().downcast_ref::<Float32Array>().cloned());

    (0..n)
        .map(|i| {
            let score = dist_col
                .as_ref()
                .map(|c| 1.0 / (1.0 + c.value(i)))  // distance → similarity
                .unwrap_or(1.0);
            SearchResult {
                id: get_str(&ids, i),
                timestamp: timestamps.as_ref().map(|c| c.value(i)).unwrap_or(0),
                app_name: get_str(&app_names, i),
                bundle_id: get_opt_str(&bundle_ids, i),
                window_title: get_str(&window_titles, i),
                session_id: get_str(&session_ids, i),
                text: get_str(&texts, i),
                snippet: get_str(&snippets, i),
                score,
                screenshot_path: get_opt_str(&screenshot_paths, i),
                url: get_opt_str(&urls, i),
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

// ── Filter helpers ────────────────────────────────────────────────────────────

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
        "today" => Some(format!(
            "day_bucket = '{}'",
            now.format("%Y-%m-%d")
        )),
        "yesterday" => Some(format!(
            "day_bucket = '{}'",
            (now - Duration::days(1)).format("%Y-%m-%d")
        )),
        _ => None,
    }
}

/// Escape single quotes for SQL string literals.
fn sql_escape(s: &str) -> String {
    s.replace('\'', "''")
}

// ── DB initialization ─────────────────────────────────────────────────────────

async fn open_or_create_table(db_path: &Path) -> Result<Table, lancedb::Error> {
    let uri = db_path.to_string_lossy();
    let conn: Connection = lancedb::connect(&uri).execute().await?;

    let names = conn.table_names().execute().await?;
    if names.contains(&TABLE_NAME.to_string()) {
        conn.open_table(TABLE_NAME).execute().await
    } else {
        let schema = Arc::new(memory_schema());
        let empty = RecordBatchIterator::new(
            std::iter::empty::<Result<RecordBatch, ArrowError>>(),
            schema,
        );
        conn.create_table(TABLE_NAME, Box::new(empty))
            .execute()
            .await
    }
}

// ── Migration from legacy JSON store ─────────────────────────────────────────

async fn migrate_from_json(table: &Table, json_path: &Path) {
    let result: Result<(), Box<dyn std::error::Error>> = (async {
        let data = std::fs::read(json_path)?;
        let records: Vec<MemoryRecord> = serde_json::from_slice(&data)?;
        if records.is_empty() {
            return Ok(());
        }

        tracing::info!(
            "Migrating {} records from memories.json to LanceDB",
            records.len()
        );

        // Insert in chunks to avoid huge Arrow batches.
        for chunk in records.chunks(500) {
            let batch = records_to_batch(chunk)?;
            let schema = Arc::new(memory_schema());
            let iter = RecordBatchIterator::new(vec![Ok(batch)], schema);
            table.add(Box::new(iter)).execute().await?;
        }

        // Rename the JSON file so we don't migrate again on next start.
        let backup = json_path.with_extension("json.migrated");
        std::fs::rename(json_path, backup)?;

        tracing::info!("Migration complete");
        Ok(())
    })
    .await;

    if let Err(e) = result {
        tracing::warn!("JSON migration failed (data not lost): {}", e);
    }
}
