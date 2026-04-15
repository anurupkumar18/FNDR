//! LanceDB-backed storage for FNDR memory records.
//!
//! Replaces the JSON-based simple_store with a proper vector database.
//! All methods that touch LanceDB are async.

use super::schema::{
    AppCount, DayCount, DaypartCount, DomainCount, HourCount, MemoryRecord, SearchResult, Stats,
    WeekdayCount,
};
use arrow_array::{
    Array, FixedSizeListArray, Float32Array, Int64Array, RecordBatch, RecordBatchIterator,
    RecordBatchReader, StringArray,
};
use arrow_schema::{ArrowError, DataType, Field, Schema};
use chrono::{Datelike, Local, NaiveDate, TimeZone, Timelike};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::table::NewColumnTransform;
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
        self.table
            .add(Box::new(iter) as Box<dyn RecordBatchReader + Send>)
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

        let mut vq = self
            .table
            .vector_search(query_vec)?
            .column("embedding")
            .limit(limit);

        if let Some(f) = filter {
            vq = vq.only_if(f);
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
        let terms = keyword_terms(query);
        if terms.is_empty() {
            return Ok(Vec::new());
        }

        let mut clauses = Vec::new();
        for term in &terms {
            let escaped = sql_escape(term);
            clauses.push(format!("text LIKE '%{escaped}%'"));
            clauses.push(format!("clean_text LIKE '%{escaped}%'"));
            clauses.push(format!("snippet LIKE '%{escaped}%'"));
            clauses.push(format!("window_title LIKE '%{escaped}%'"));
            clauses.push(format!("app_name LIKE '%{escaped}%'"));
            clauses.push(format!("url LIKE '%{escaped}%'"));
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
            .limit(limit)
            .execute()
            .await?
            .try_collect()
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            let mut batch_results = batch_to_search_results(batch);
            // Keyword branch gets a lexical relevance score before hybrid fusion.
            for r in &mut batch_results {
                r.score = lexical_keyword_score(&terms, r);
            }
            results.extend(batch_results);
        }
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        });
        Ok(results)
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
                        0..=5 => 0,
                        6..=11 => 1,
                        12..=17 => 2,
                        _ => 3,
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
            let summary_col = batch
                .column_by_name("summary_source")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>().cloned());

            if let Some(col) = app_col {
                for i in 0..batch.num_rows() {
                    let summary_source = summary_col
                        .as_ref()
                        .map(|s| s.value(i))
                        .unwrap_or("fallback");
                    if summary_source.eq_ignore_ascii_case("fallback") {
                        continue;
                    }
                    names.insert(col.value(i).to_string());
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
        let mut query = self.table.query().limit(limit);
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
    let clean_texts: Vec<&str> = records.iter().map(|r| r.clean_text.as_str()).collect();
    let ocr_confidences: Vec<f32> = records.iter().map(|r| r.ocr_confidence).collect();
    let ocr_block_counts: Vec<i64> = records.iter().map(|r| r.ocr_block_count as i64).collect();
    let snippets: Vec<&str> = records.iter().map(|r| r.snippet.as_str()).collect();
    let summary_sources: Vec<&str> = records.iter().map(|r| r.summary_source.as_str()).collect();
    let noise_scores: Vec<f32> = records.iter().map(|r| r.noise_score).collect();
    let session_keys: Vec<&str> = records.iter().map(|r| r.session_key.as_str()).collect();
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
    let clean_texts = str_col(batch, "clean_text");
    let ocr_confidences = f32_col(batch, "ocr_confidence");
    let ocr_block_counts = i64_col(batch, "ocr_block_count");
    let snippets = str_col(batch, "snippet");
    let summary_sources = str_col(batch, "summary_source");
    let noise_scores = f32_col(batch, "noise_score");
    let session_keys = str_col(batch, "session_key");
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
                clean_text: get_str(&clean_texts, i),
                ocr_confidence: get_f32(&ocr_confidences, i),
                ocr_block_count: get_i64(&ocr_block_counts, i).max(0) as u32,
                snippet: get_str(&snippets, i),
                summary_source: get_str(&summary_sources, i),
                noise_score: get_f32(&noise_scores, i),
                session_key: get_str(&session_keys, i),
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
    let clean_texts = str_col(batch, "clean_text");
    let ocr_confidences = f32_col(batch, "ocr_confidence");
    let ocr_block_counts = i64_col(batch, "ocr_block_count");
    let snippets = str_col(batch, "snippet");
    let summary_sources = str_col(batch, "summary_source");
    let noise_scores = f32_col(batch, "noise_score");
    let session_keys = str_col(batch, "session_key");
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
                .map(|c| 1.0 / (1.0 + c.value(i))) // distance → similarity
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

fn f32_col(batch: &RecordBatch, name: &str) -> Option<Float32Array> {
    batch
        .column_by_name(name)?
        .as_any()
        .downcast_ref::<Float32Array>()
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
    let merged = format!("{} {} {} {}", title, snippet, clean, url);

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
        let table = conn.open_table(TABLE_NAME).execute().await?;
        ensure_memory_schema_columns(&table).await?;
        Ok(table)
    } else {
        let schema = Arc::new(memory_schema());
        let empty = RecordBatchIterator::new(
            std::iter::empty::<Result<RecordBatch, ArrowError>>(),
            schema,
        );
        conn.create_table(
            TABLE_NAME,
            Box::new(empty) as Box<dyn RecordBatchReader + Send>,
        )
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
