//! Seeds the FNDR alpha demo profile from scripts/demo/demo-week.json through the
//! real Store insert path (normalization + storage-outcome classification).
//! Usage: cargo run --example seed_demo -- --data-dir <dir> --corpus <json>
//! Refuses to write into the real profile (com.fndr.app).

use chrono::{Duration as ChronoDuration, Local, NaiveTime, TimeZone};
use fndr_lib::config::DEFAULT_IMAGE_EMBEDDING_DIM;
use fndr_lib::embedding::{Embedder, EMBEDDING_DIM};
use fndr_lib::memory_quality::partition_surfaceable;
use fndr_lib::storage::{MemoryRecord, Store};
use serde::Deserialize;
use std::path::PathBuf;

// Same literals the live capture path uses (capture/mod.rs).
const SOURCE_TYPE_URL: &str = "browser";
const SOURCE_TYPE_SCREEN: &str = "screen";

#[derive(Deserialize)]
struct SeedEntry {
    id: String,
    day_offset: i64,
    time: String,
    app_name: String,
    #[serde(default)]
    bundle_id: Option<String>,
    window_title: String,
    #[serde(default)]
    url: Option<String>,
    summary: String,
    ocr_text: String,
    session: String,
    #[serde(default)]
    low_signal: bool,
}

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn timestamp_ms(day_offset: i64, time: &str) -> i64 {
    let t = NaiveTime::parse_from_str(time, "%H:%M").expect("time HH:MM");
    let date = Local::now().date_naive() + ChronoDuration::days(day_offset);
    Local
        .from_local_datetime(&date.and_time(t))
        .single()
        .expect("unambiguous local time")
        .timestamp_millis()
}

fn record(entry: &SeedEntry, embedding: Vec<f32>) -> MemoryRecord {
    let ts = timestamp_ms(entry.day_offset, &entry.time);
    let day_bucket = Local
        .timestamp_millis_opt(ts)
        .single()
        .expect("ts")
        .format("%Y-%m-%d")
        .to_string();
    let lines = entry
        .ocr_text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count() as u32;
    let mut r = MemoryRecord {
        id: entry.id.clone(),
        timestamp: ts,
        timestamp_start: ts,
        timestamp_end: ts + 45_000,
        day_bucket,
        app_name: entry.app_name.clone(),
        bundle_id: entry.bundle_id.clone(),
        window_title: entry.window_title.clone(),
        session_id: entry.session.clone(),
        session_key: entry.session.clone(),
        text: entry.ocr_text.clone(),
        clean_text: entry.ocr_text.clone(),
        snippet: entry.summary.clone(),
        display_summary: entry.summary.clone(),
        memory_context: entry.summary.clone(),
        summary_source: "demo_seed".to_string(),
        synthesis_branch: "demo_seed".to_string(),
        source_type: if entry.url.is_some() {
            SOURCE_TYPE_URL
        } else {
            SOURCE_TYPE_SCREEN
        }
        .to_string(),
        url: entry.url.clone(),
        ocr_confidence: 0.93,
        ocr_block_count: lines.max(1),
        noise_score: 0.04,
        embedding: embedding.clone(),
        snippet_embedding: embedding.clone(),
        support_embedding: embedding,
        image_embedding: vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM],
        decay_score: 1.0,
        specificity_score: 0.78,
        intent_score: 0.68,
        entity_score: 0.62,
        agent_usefulness_score: 0.72,
        evidence_confidence: 0.84,
        retrieval_value_score: 0.74,
        ocr_noise_score: 0.05,
        enrichment_status: "pending".to_string(),
        raw_screenshot_stored: false,
        ..Default::default()
    };
    if entry.low_signal {
        r.snippet = String::new();
        r.display_summary = String::new();
        r.memory_context = String::new();
        let has_text = !entry.ocr_text.trim().is_empty();
        r.ocr_block_count = if has_text { 1 } else { 0 };
        r.ocr_confidence = if has_text { 0.4 } else { 0.0 };
        r.enrichment_status = "visual_metadata_fallback".to_string();
        r.synthesis_branch = "visual_metadata_fallback".to_string();
        r.embedding = vec![0.0; EMBEDDING_DIM];
        r.snippet_embedding = vec![0.0; EMBEDDING_DIM];
        r.support_embedding = vec![0.0; EMBEDDING_DIM];
    }
    r
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(arg("--data-dir").ok_or("--data-dir required")?);
    let corpus = PathBuf::from(arg("--corpus").ok_or("--corpus required")?);
    std::fs::create_dir_all(&data_dir)?;
    if let Some(real) = dirs::data_dir().map(|d| d.join("com.fndr.app")) {
        let real = real.canonicalize().unwrap_or(real);
        if data_dir.canonicalize()? == real {
            return Err("refusing to seed the real FNDR profile".into());
        }
    }

    let entries: Vec<SeedEntry> = serde_json::from_slice(&std::fs::read(&corpus)?)?;
    let expected_low = entries.iter().filter(|e| e.low_signal).count();
    let embedder = Embedder::new()?;

    let mut records = Vec::with_capacity(entries.len());
    for chunk in entries.chunks(16) {
        let texts: Vec<String> = chunk
            .iter()
            .map(|e| format!("{}\n{}\n{}", e.window_title, e.summary, e.ocr_text))
            .collect();
        let vectors = embedder.embed_batch(&texts)?;
        for (entry, vector) in chunk.iter().zip(vectors) {
            records.push(record(entry, vector));
        }
    }

    let store = Store::new(&data_dir)?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(store.add_batch_preserving_ids(&records))
        .map_err(|e| e.to_string())?;
    let listed = rt
        .block_on(store.list_recent_results(1_000, None))
        .map_err(|e| e.to_string())?;
    let stored = listed.len();
    let (surfaced, hidden) = partition_surfaceable(listed);
    println!(
        "seeded {} records into {} — stored {}, surfaced {}, needs-signal {}",
        records.len(),
        data_dir.display(),
        stored,
        surfaced.len(),
        hidden.len()
    );
    if hidden.len() != expected_low {
        return Err(format!(
            "quality gate mismatch: expected {expected_low} needs-signal records, got {}",
            hidden.len()
        )
        .into());
    }
    if stored != records.len() {
        eprintln!(
            "warning: insert-time dedup merged {} seeded entries",
            records.len() - stored
        );
    }
    Ok(())
}
