//! Re-reviews weak summaries on a COPY of a profile with the local model and
//! prints before and after. Refuses the real profile. The copy is rewritten;
//! the source profile is never opened.
//! Usage: cargo run --example review_preview -- --data-dir <profile copy> [--limit N]

use fndr_lib::embedding::Embedder;
use fndr_lib::inference::InferenceEngine;
use fndr_lib::memory_review::{
    review_one_memory, InferenceReviewProvider, MemoryReviewJob, MemoryReviewOutcome,
};
use fndr_lib::storage::Store;
use fndr_lib::summariser::narration_filter::{is_placeholder_summary, narration_filter_hits};
use std::path::PathBuf;
use std::sync::Arc;

fn arg(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(current) = args.next() {
        if current == name {
            return args.next();
        }
    }
    None
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(arg("--data-dir").ok_or("--data-dir required")?).canonicalize()?;
    let limit: usize = arg("--limit").and_then(|value| value.parse().ok()).unwrap_or(12);
    let real = dirs::data_dir().ok_or("no data dir")?.join("com.fndr.app");
    if real.canonicalize().is_ok_and(|real| data_dir.starts_with(&real)) {
        return Err("refusing the real FNDR profile; pass a copy".into());
    }

    let store = Store::new(&data_dir)?;
    let embedder = Embedder::new().ok();
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let engine = Arc::new(InferenceEngine::new(Some(real), None).await.map_err(|e| e.to_string())?);
        let provider = InferenceReviewProvider::new(engine);
        let mut rows = store.list_all_memories().await.map_err(|e| e.to_string())?;
        rows.sort_by_key(|row| std::cmp::Reverse(row.timestamp));
        let weak: Vec<_> = rows
            .into_iter()
            .filter(|row| !row.is_agent_note() && !row.clean_text.trim().is_empty())
            .filter(|row| {
                is_placeholder_summary(&row.display_summary)
                    || narration_filter_hits(&row.display_summary)
                    || row.display_summary.to_lowercase().starts_with("the user")
            })
            .take(limit)
            .collect();
        println!("weak rows selected: {}", weak.len());
        let now = chrono::Utc::now().timestamp_millis();
        for row in weak {
            let job = MemoryReviewJob {
                memory_id: row.id.clone(),
                day_bucket: row.day_bucket.clone(),
                enqueued_at_ms: now,
            };
            let outcome = review_one_memory(&store, &provider, embedder.as_ref(), &job, now).await?;
            let after = store.get_memory_by_id(&row.id).await.map_err(|e| e.to_string())?;
            let label = match &outcome {
                MemoryReviewOutcome::Reviewed { .. } => "reviewed".to_string(),
                MemoryReviewOutcome::Failed { .. } => format!("{outcome:?}").chars().take(90).collect(),
                MemoryReviewOutcome::Skipped { reason, .. } => format!("skipped: {reason}"),
            };
            println!("--- {} | {} | {label}", row.app_name, row.window_title.chars().take(60).collect::<String>());
            println!("  BEFORE {}", row.display_summary);
            if let Some(after) = after {
                println!("  AFTER  {}", after.display_summary);
                println!("  WHAT   {}", after.insight_what_happened);
                println!("  META   topic={:?} activity={:?}", after.topic, after.activity_type);
            }
        }
        Ok::<(), String>(())
    })?;
    Ok(())
}
