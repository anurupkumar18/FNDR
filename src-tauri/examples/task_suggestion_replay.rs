//! Replays task suggestion over the memories of a COPY of a profile, the way
//! capture now does it: skip AI chat and system screens, ask the local model,
//! keep only what the screen text supports, drop repeats by meaning. Nothing
//! is written. Prints counts only unless `--show` is given, which also prints
//! each kept suggestion with its quote for the owner to judge.
//! Usage: cargo run --example task_suggestion_replay -- --data-dir <profile copy> [--show] [--limit N]

use fndr_lib::embedding::Embedder;
use fndr_lib::inference::InferenceEngine;
use fndr_lib::storage::Store;
use fndr_lib::tasks::suggest::{
    drop_repeats, is_task_source, parse_suggestions, surface_of, Surface,
};
use std::collections::BTreeMap;
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
    let real = dirs::data_dir().ok_or("no data dir")?.join("com.fndr.app");
    if real
        .canonicalize()
        .is_ok_and(|real| data_dir.starts_with(&real))
    {
        return Err("refusing the real FNDR profile; pass a copy".into());
    }
    let show = std::env::args().any(|current| current == "--show");
    let limit: usize = arg("--limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(usize::MAX);

    let store = Store::new(&data_dir)?;
    let embedder = Embedder::new().ok();
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let engine = InferenceEngine::new(Some(real), None)
            .await
            .map_err(|e| e.to_string())?;
        let mut memories = store.list_all_memories().await.map_err(|e| e.to_string())?;
        memories.sort_by_key(|memory| memory.timestamp);

        let (mut skipped_source, mut skipped_thin, mut asked, mut model_lines) = (0, 0, 0, 0);
        let (mut supported, mut kept) = (0usize, 0usize);
        let mut memories_with_a_suggestion = 0;
        let mut by_surface: BTreeMap<&str, usize> = BTreeMap::new();
        let mut by_app: BTreeMap<String, usize> = BTreeMap::new();
        let mut known_titles: Vec<String> = Vec::new();
        for memory in memories.iter().filter(|memory| !memory.is_agent_note()) {
            if !is_task_source(&memory.app_name, memory.url.as_deref()) {
                skipped_source += 1;
                continue;
            }
            if memory.clean_text.trim().chars().count() < 40 {
                skipped_thin += 1;
                continue;
            }
            if asked == limit {
                break;
            }
            asked += 1;
            let evidence = format!(
                "{}\n{}",
                memory.window_title,
                memory.clean_text.chars().take(1500).collect::<String>()
            );
            let raw = engine.suggest_tasks(&evidence).await;
            model_lines += raw.lines().filter(|line| line.contains('|')).count();
            if show {
                for line in raw.lines().filter(|line| line.contains('|')) {
                    println!("MODEL [{}] {}", memory.app_name, line.trim());
                }
            }
            let surface = surface_of(&memory.app_name, memory.url.as_deref());
            let checked = parse_suggestions(&raw, &evidence, surface);
            supported += checked.len();
            let fresh = drop_repeats(checked, &known_titles, |texts| {
                embedder.as_ref().and_then(|e| e.embed_batch(texts).ok())
            });
            if !fresh.is_empty() {
                memories_with_a_suggestion += 1;
            }
            for suggestion in fresh {
                kept += 1;
                *by_surface
                    .entry(if surface == Surface::Personal {
                        "personal"
                    } else {
                        "public"
                    })
                    .or_insert(0) += 1;
                *by_app.entry(memory.app_name.clone()).or_insert(0) += 1;
                if show {
                    println!(
                        "KEPT [{}] {:?} | {} | \"{}\"",
                        memory.app_name, suggestion.task_type, suggestion.title, suggestion.quote
                    );
                }
                known_titles.push(suggestion.title);
            }
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "memories": memories.len(),
                "skipped_ai_chat_or_system_screen": skipped_source,
                "skipped_too_little_text": skipped_thin,
                "asked_the_model": asked,
                "model_task_lines": model_lines,
                "supported_by_the_screen": supported,
                "kept_after_dropping_repeats": kept,
                "memories_with_a_suggestion": memories_with_a_suggestion,
                "kept_by_surface": by_surface,
                "kept_by_app": by_app,
            }))
            .map_err(|e| e.to_string())?
        );
        Ok::<(), String>(())
    })?;
    Ok(())
}
