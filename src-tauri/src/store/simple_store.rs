//! Simple in-memory storage with JSON persistence
//! 
//! Replaces LanceDB for the prototype to avoid dependency conflicts.
//! Implements naive vector search (cosine similarity) and keyword search.

use super::schema::{AppCount, MemoryRecord, SearchResult, Stats};
// use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

const STORE_FILENAME: &str = "memories.json";

/// Simple in-memory store
pub struct Store {
    data_path: PathBuf,
    records: Arc<RwLock<Vec<MemoryRecord>>>,
}

impl Store {
    pub fn new(data_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let data_path = data_dir.join(STORE_FILENAME);
        let records = if data_path.exists() {
            let file = File::open(&data_path)?;
            let reader = BufReader::new(file);
            serde_json::from_reader(reader).unwrap_or_else(|_| Vec::new())
        } else {
            Vec::new()
        };

        Ok(Self {
            data_path,
            records: Arc::new(RwLock::new(records)),
        })
    }

    /// Add a batch of records
    pub fn add_batch(&self, new_records: &[MemoryRecord]) -> Result<(), Box<dyn std::error::Error>> {
        {
            let mut records = self.records.write().unwrap();
            records.extend_from_slice(new_records);
        }
        self.save()?;
        Ok(())
    }

    /// Save records to disk
    fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let records = self.records.read().unwrap();
        if let Some(parent) = self.data_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::create(&self.data_path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer(writer, &*records)?;
        Ok(())
    }

    /// Vector search using cosine similarity
    pub fn vector_search(
        &self,
        query_embedding: &[f32],
        limit: usize,
        time_filter: Option<&str>,
        app_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        let records = self.records.read().unwrap();
        
        // Filter candidates
        let candidates: Vec<&MemoryRecord> = records.iter().filter(|r| {
            if let Some(tf) = time_filter {
                let now = chrono::Utc::now();
                match tf {
                    "today" => {
                        let today = now.format("%Y-%m-%d").to_string();
                        if r.day_bucket != today { return false; }
                    }
                    "yesterday" => {
                        let yesterday = (now - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
                        if r.day_bucket != yesterday { return false; }
                    }
                    "week" => {
                        let week_ago = (now - chrono::Duration::days(7)).timestamp_millis();
                        if r.timestamp < week_ago { return false; }
                    }
                    _ => {}
                }
            }
            if let Some(app) = app_filter {
                if r.app_name != app { return false; }
            }
            true
        }).collect();

        // Calculate scores
        let mut scored: Vec<(f32, &MemoryRecord)> = candidates.iter().map(|&r| {
            let score = cosine_similarity(query_embedding, &r.embedding);
            (score, r)
        }).collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Create results
        let results = scored.into_iter().take(limit).map(|(score, r)| {
            SearchResult {
                id: r.id.clone(),
                timestamp: r.timestamp,
                app_name: r.app_name.clone(),
                window_title: r.window_title.clone(),
                text: r.text.clone(),
                snippet: r.snippet.clone(),
                score,
            }
        }).collect();

        Ok(results)
    }

    /// Keyword search (simple substring match) with optional time and app filters
    pub fn keyword_search(
        &self,
        query: &str,
        limit: usize,
        time_filter: Option<&str>,
        app_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>, Box<dyn std::error::Error>> {
        let records = self.records.read().unwrap();
        let query_lower = query.to_lowercase();

        let mut matched: Vec<&MemoryRecord> = records
            .iter()
            .filter(|r| {
                if let Some(tf) = time_filter {
                    let now = chrono::Utc::now();
                    match tf {
                        "today" => {
                            let today = now.format("%Y-%m-%d").to_string();
                            if r.day_bucket != today {
                                return false;
                            }
                        }
                        "yesterday" => {
                            let yesterday =
                                (now - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
                            if r.day_bucket != yesterday {
                                return false;
                            }
                        }
                        "week" => {
                            let week_ago = (now - chrono::Duration::days(7)).timestamp_millis();
                            if r.timestamp < week_ago {
                                return false;
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(app) = app_filter {
                    if r.app_name != app {
                        return false;
                    }
                }
                r.text.to_lowercase().contains(&query_lower)
                    || r.app_name.to_lowercase().contains(&query_lower)
                    || r.window_title.to_lowercase().contains(&query_lower)
            })
            .collect();

        // Sort by timestamp descending (newest first)
        matched.sort_by_key(|r| std::cmp::Reverse(r.timestamp));

        let results = matched.into_iter().take(limit).map(|r| {
            SearchResult {
                id: r.id.clone(),
                timestamp: r.timestamp,
                app_name: r.app_name.clone(),
                window_title: r.window_title.clone(),
                text: r.text.clone(),
                snippet: r.snippet.clone(),
                score: 1.0,
            }
        }).collect();

        Ok(results)
    }

    /// Get statistics
    pub fn get_stats(&self) -> Result<Stats, Box<dyn std::error::Error>> {
        let records = self.records.read().unwrap();
        
        let total_records = records.len();
        let mut days = std::collections::HashSet::new();
        let mut app_counts: HashMap<String, usize> = HashMap::new();
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let mut today_count = 0;

        for r in records.iter() {
            days.insert(&r.day_bucket);
            if r.day_bucket == today {
                today_count += 1;
            }
            *app_counts.entry(r.app_name.clone()).or_insert(0) += 1;
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

    /// Delete all data
    pub fn delete_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        {
            let mut records = self.records.write().unwrap();
            records.clear();
        }
        self.save()?;
        Ok(())
    }

    /// Get sorted list of unique app names (for filter dropdown)
    pub fn get_app_names(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let records = self.records.read().unwrap();
        let mut names: std::collections::HashSet<String> =
            records.iter().map(|r| r.app_name.clone()).collect();
        let mut list: Vec<String> = names.drain().collect();
        list.sort();
        Ok(list)
    }

    /// Delete records older than days
    pub fn delete_older_than(&self, days: u32) -> Result<usize, Box<dyn std::error::Error>> {
        let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);
        let cutoff_ms = cutoff.timestamp_millis();
        
        let mut records = self.records.write().unwrap();
        let initial_len = records.len();
        records.retain(|r| r.timestamp >= cutoff_ms);
        let deleted = initial_len - records.len();

        if deleted > 0 {
            drop(records); // Release lock before saving
            self.save()?;
        }
        
        Ok(deleted)
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot_product: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}
