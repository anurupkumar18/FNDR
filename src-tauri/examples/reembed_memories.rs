//! Recomposes each memory's embedding text and refreshes its primary and
//! snippet vectors (`memory_embedding_document::refresh_text_vectors`). Use
//! after the embedding document changes. Dry run unless `--apply` is given;
//! refuses the real profile unless `--allow-real-profile` is also given.
//! Usage: cargo run --example reembed_memories -- --data-dir <profile> [--apply]

use fndr_lib::embedding::Embedder;
use fndr_lib::memory_embedding_document::{compose_memory_embedding_document, refresh_text_vectors};
use fndr_lib::storage::Store;
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(arg("--data-dir").ok_or("--data-dir required")?).canonicalize()?;
    let apply = std::env::args().any(|current| current == "--apply");
    let real = dirs::data_dir().ok_or("no data dir")?.join("com.fndr.app");
    let is_real = real.canonicalize().is_ok_and(|real| data_dir.starts_with(&real));
    if is_real && !std::env::args().any(|current| current == "--allow-real-profile") {
        return Err("refusing the real FNDR profile without --allow-real-profile".into());
    }

    let store = Store::new(&data_dir)?;
    let embedder = Embedder::new()?;
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let rows = store.list_all_memories().await.map_err(|e| e.to_string())?;
        let (mut text_changed, mut rewritten, mut failed) = (0usize, 0usize, 0usize);
        for mut row in rows.clone() {
            if row.is_agent_note() {
                continue;
            }
            text_changed += usize::from(compose_memory_embedding_document(&row, None).primary_text != row.embedding_text);
            if !apply {
                continue;
            }
            if refresh_text_vectors(&mut row, Some(&embedder)) {
                store.replace_memory_preserving_chunks(&row).await.map_err(|e| e.to_string())?;
                rewritten += 1;
            } else {
                failed += 1;
            }
        }
        println!(
            "{}",
            serde_json::json!({ "dry_run": !apply, "scanned": rows.len(), "embedding_text_would_change": text_changed, "rewritten": rewritten, "failed": failed })
        );
        Ok::<(), String>(())
    })?;
    Ok(())
}
