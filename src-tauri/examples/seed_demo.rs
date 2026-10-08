//! Seeds an FNDR demo profile from a JSON corpus through the
//! real Store insert path (normalization + storage-outcome classification).
//! Usage: cargo run --example seed_demo -- --data-dir <dir> --corpus <json>
//! Refuses to write into the real profile (com.fndr.app).

use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveTime, TimeZone};
use fndr_lib::config::DEFAULT_IMAGE_EMBEDDING_DIM;
use fndr_lib::embedding::{Embedder, EMBEDDING_DIM};
use fndr_lib::memory_quality::partition_surfaceable;
use fndr_lib::storage::{MemoryRecord, Store};
use serde::Deserialize;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

// Same literals the live capture path uses (capture/mod.rs).
const SOURCE_TYPE_URL: &str = "browser";
const SOURCE_TYPE_SCREEN: &str = "screen";

#[derive(Deserialize)]
struct SeedEntry {
    id: String,
    #[serde(default)]
    day_offset: i64,
    #[serde(default)]
    time: String,
    /// Places the entry relative to seeding time, so a Resume window always has recent threads.
    #[serde(default)]
    minutes_ago: Option<i64>,
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
    // Fields the local model normally fills at capture time. Seeding them lets
    // QA judge a screen separately from extraction quality.
    #[serde(default)]
    project: String,
    #[serde(default)]
    topic: String,
    #[serde(default)]
    outcome: String,
    #[serde(default)]
    next_steps: Vec<String>,
    #[serde(default)]
    decisions: Vec<String>,
    #[serde(default)]
    errors: Vec<String>,
}

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn resolved_path_for_safety(path: &Path) -> Result<PathBuf, String> {
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("refusing a profile path containing parent traversal".to_string());
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("could not resolve profile path: {error}"))?
            .join(path)
    };
    let mut existing = absolute.as_path();
    let mut missing = Vec::<OsString>::new();
    while !existing.exists() {
        let name = existing
            .file_name()
            .ok_or_else(|| "could not resolve profile path".to_string())?;
        missing.push(name.to_os_string());
        existing = existing
            .parent()
            .ok_or_else(|| "could not resolve profile path".to_string())?;
    }
    let mut resolved = existing
        .canonicalize()
        .map_err(|error| format!("could not resolve profile path: {error}"))?;
    for component in missing.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn validate_seed_target(data_dir: &Path, real_profile: &Path) -> Result<PathBuf, String> {
    let requested = resolved_path_for_safety(data_dir)?;
    let real = resolved_path_for_safety(real_profile)?;
    if requested.starts_with(&real) || real.starts_with(&requested) {
        return Err("refusing to seed or reset the real FNDR profile".to_string());
    }
    Ok(requested)
}

fn timestamp_ms(entry: &SeedEntry, now: DateTime<Local>) -> i64 {
    if let Some(minutes) = entry.minutes_ago {
        return now.timestamp_millis() - minutes * 60_000;
    }
    let t = NaiveTime::parse_from_str(&entry.time, "%H:%M").expect("time HH:MM");
    let date = now.date_naive() + ChronoDuration::days(entry.day_offset);
    Local
        .from_local_datetime(&date.and_time(t))
        .single()
        .expect("unambiguous local time")
        .timestamp_millis()
}

fn record_at(entry: &SeedEntry, embedding: Vec<f32>, now: DateTime<Local>) -> MemoryRecord {
    let ts = timestamp_ms(entry, now);
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
        project: entry.project.clone(),
        topic: entry.topic.clone(),
        outcome: entry.outcome.clone(),
        next_steps: entry.next_steps.clone(),
        decisions: entry.decisions.clone(),
        errors: entry.errors.clone(),
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
    let requested_data_dir = PathBuf::from(arg("--data-dir").ok_or("--data-dir required")?);
    let corpus = PathBuf::from(arg("--corpus").ok_or("--corpus required")?);
    let real = dirs::data_dir()
        .map(|dir| dir.join("com.fndr.app"))
        .ok_or("could not locate the FNDR application-data directory")?;
    let data_dir = validate_seed_target(&requested_data_dir, &real)?;
    std::fs::create_dir_all(&data_dir)?;

    let entries: Vec<SeedEntry> = serde_json::from_slice(&std::fs::read(&corpus)?)?;
    let expected_low = entries.iter().filter(|e| e.low_signal).count();
    let embedder = Embedder::new()?;
    let now = Local::now();

    let mut records = Vec::with_capacity(entries.len());
    for chunk in entries.chunks(16) {
        let texts: Vec<String> = chunk
            .iter()
            .map(|e| format!("{}\n{}\n{}", e.window_title, e.summary, e.ocr_text))
            .collect();
        let vectors = embedder.embed_batch(&texts)?;
        for (entry, vector) in chunk.iter().zip(vectors) {
            records.push(record_at(entry, vector, now));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> SeedEntry {
        serde_json::from_str(json).expect("seed entry")
    }

    fn fixed_now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 9, 24, 15, 0, 0).unwrap()
    }

    fn knowledge_worker_corpus() -> Vec<SeedEntry> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../scripts/demo/knowledge-worker-week.json"
        );
        serde_json::from_slice(&std::fs::read(path).expect("corpus file")).expect("valid corpus")
    }

    #[test]
    fn legacy_entries_without_structured_fields_still_parse() {
        let entry = parse(
            r#"{"id":"a","day_offset":-1,"time":"09:00","app_name":"Notion",
                "window_title":"W","summary":"S","ocr_text":"T","session":"s1"}"#,
        );
        assert!(entry.project.is_empty());
        assert!(entry.next_steps.is_empty());
        assert_eq!(entry.minutes_ago, None);
    }

    #[test]
    fn minutes_ago_overrides_day_offset_and_time() {
        let entry = parse(
            r#"{"id":"a","minutes_ago":90,"app_name":"Google Chrome",
                "window_title":"W","summary":"S","ocr_text":"T","session":"s1"}"#,
        );
        assert_eq!(
            timestamp_ms(&entry, fixed_now()),
            fixed_now().timestamp_millis() - 90 * 60_000
        );
    }

    #[test]
    fn structured_fields_reach_the_memory_record() {
        let entry = parse(
            r#"{"id":"a","minutes_ago":30,"app_name":"Google Chrome","window_title":"W",
                "summary":"S","ocr_text":"T","session":"s1",
                "project":"HIST 2100 essay: Reconstruction",
                "topic":"Drafting the counterargument section",
                "outcome":"Draft at 1,450 words.",
                "next_steps":["Finish the counterargument paragraph"],
                "decisions":["Argue economic causes first"],
                "errors":[]}"#,
        );
        let record = record_at(&entry, vec![0.0; EMBEDDING_DIM], fixed_now());
        assert_eq!(record.project, "HIST 2100 essay: Reconstruction");
        assert_eq!(record.topic, "Drafting the counterargument section");
        assert_eq!(record.outcome, "Draft at 1,450 words.");
        assert_eq!(
            record.next_steps,
            vec!["Finish the counterargument paragraph".to_string()]
        );
        assert_eq!(
            record.decisions,
            vec!["Argue economic causes first".to_string()]
        );
        assert!(record.errors.is_empty());
    }

    #[test]
    fn knowledge_worker_corpus_has_unique_ids_and_resumable_threads() {
        let entries = knowledge_worker_corpus();
        assert_eq!(entries.len(), 20, "corpus must contain exactly 20 records");
        let ids: std::collections::HashSet<&str> =
            entries.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(ids.len(), entries.len(), "ids must be unique");
        let recent_projects: std::collections::HashSet<&str> = entries
            .iter()
            .filter(|entry| {
                entry.minutes_ago.is_some_and(|minutes| minutes <= 48 * 60)
                    && !entry.project.is_empty()
            })
            .map(|entry| entry.project.as_str())
            .collect();
        assert!(
            recent_projects.len() >= 3,
            "need at least three projects inside a 48 hour window, got {recent_projects:?}"
        );
        assert_eq!(entries.iter().filter(|entry| entry.low_signal).count(), 1);
    }

    #[test]
    fn knowledge_worker_corpus_repeats_one_error_days_apart() {
        let entries = knowledge_worker_corpus();
        let with_error: Vec<&SeedEntry> = entries
            .iter()
            .filter(|entry| {
                entry
                    .errors
                    .iter()
                    .any(|error| error == "ModuleNotFoundError: No module named 'pandas'")
            })
            .collect();
        assert!(
            with_error.iter().any(|entry| entry.minutes_ago.is_some()),
            "the error recurs today"
        );
        assert!(
            with_error.iter().any(|entry| {
                entry.minutes_ago.is_none() && entry.day_offset <= -2 && !entry.decisions.is_empty()
            }),
            "an older memory holds the fix"
        );
    }

    #[test]
    fn profile_guard_rejects_real_profile_ancestors_and_symlinked_descendants() {
        let root = tempfile::tempdir().expect("temp root");
        let real = root.path().join("profiles/com.fndr.app");
        std::fs::create_dir_all(&real).expect("real profile");

        assert!(validate_seed_target(root.path(), &real).is_err());
        assert!(validate_seed_target(&real.join("nested"), &real).is_err());

        let alias = root.path().join("real-alias");
        std::os::unix::fs::symlink(&real, &alias).expect("profile alias");
        assert!(validate_seed_target(&alias.join("not-created-yet"), &real).is_err());

        assert!(resolved_path_for_safety(Path::new("../profile")).is_err());
    }
}
