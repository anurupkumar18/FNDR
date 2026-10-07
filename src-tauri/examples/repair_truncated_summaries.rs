//! Repairs summaries that were cut inside a token (see
//! `memory_review::repair_truncated`). Dry run unless `--apply` is given.
//! Refuses the real profile unless `--allow-real-profile` is also given.
//! Prints a JSON report with before and after examples, plus a fingerprint
//! of every row before and after so an unchanged store can be confirmed.
//! Usage: cargo run --example repair_truncated_summaries -- --data-dir <profile> [--apply]

use fndr_lib::embedding::Embedder;
use fndr_lib::memory_review::repair_truncated_summaries;
use fndr_lib::storage::Store;
use sha2::{Digest, Sha256};
use std::path::PathBuf;

fn arg(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(current) = args.next() {
        if current == name {
            return args.next();
        }
    }
    None
}

fn flag(name: &str) -> bool {
    std::env::args().any(|current| current == name)
}

async fn fingerprint(store: &Store) -> Result<String, Box<dyn std::error::Error>> {
    let mut rows: Vec<String> = store
        .list_all_memories()
        .await?
        .iter()
        .map(|record| serde_json::to_string(record).unwrap_or_default())
        .collect();
    rows.sort();
    let mut hasher = Sha256::new();
    for row in &rows {
        hasher.update(row.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(arg("--data-dir").ok_or("--data-dir required")?);
    let apply = flag("--apply");
    let canonical = data_dir.canonicalize()?;
    if !canonical.join("lancedb").is_dir() {
        return Err("profile is missing its lancedb store".into());
    }
    let real = dirs::data_dir().map(|dir| dir.join("com.fndr.app"));
    let is_real = real
        .and_then(|path| path.canonicalize().ok())
        .is_some_and(|real| canonical == real || canonical.starts_with(&real));
    if is_real && !flag("--allow-real-profile") {
        return Err("refusing the real FNDR profile without --allow-real-profile".into());
    }

    // Store builds its own runtime, so construct it before ours.
    let store = Store::new(&canonical)?;
    let embedder = if apply { Embedder::new().ok() } else { None };
    let runtime = tokio::runtime::Runtime::new()?;
    let before = runtime.block_on(fingerprint(&store))?;
    let summary = runtime
        .block_on(repair_truncated_summaries(
            &store,
            embedder.as_ref(),
            !apply,
        ))
        .map_err(std::io::Error::other)?;
    let after = runtime.block_on(fingerprint(&store))?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "summary": summary,
            "rows_fingerprint_before": before,
            "rows_fingerprint_after": after,
            "store_changed": before != after,
        }))?
    );
    Ok(())
}
