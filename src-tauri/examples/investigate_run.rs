//! Runs one investigation (ADR 039) on a seeded profile, through the same
//! `investigate::run` the app calls, and prints the steps and the case file.
//! Reads a disposable copy of the profile's store and refuses the real one.
//! Usage: cargo run --example investigate_run -- --data-dir <profile> --goal "<request>" [--json] [--hits]

use fndr_lib::config::Config;
use fndr_lib::graph::GraphStore;
use fndr_lib::investigate;
use fndr_lib::storage::{StateStore, Store};
use fndr_lib::AppState;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn arg(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(value) = args.next() {
        if value == name {
            return args.next();
        }
    }
    None
}

fn copy_directory(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn day(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|at| {
            at.with_timezone(&chrono::Local)
                .format("%a %b %-d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(arg("--data-dir").ok_or("--data-dir required")?);
    let goal = arg("--goal").ok_or("--goal required")?;
    let as_json = std::env::args().any(|value| value == "--json");

    let canonical = data_dir.canonicalize()?;
    if let Some(real) = dirs::data_dir().map(|dir| dir.join("com.fndr.app")) {
        if real.canonicalize().is_ok_and(|real| real == canonical) {
            return Err("Refusing the real FNDR profile; pass a seeded one.".into());
        }
    }
    if !canonical.join("lancedb").is_dir() {
        return Err(format!("No store at {}", canonical.display()).into());
    }

    let scratch = tempfile::tempdir()?;
    copy_directory(&canonical.join("lancedb"), &scratch.path().join("lancedb"))?;
    let dir = scratch.path().to_path_buf();
    // Store and StateStore each start a runtime of their own, so they are
    // built before this one.
    let store = Arc::new(Store::new(&dir)?);
    let state_store = Arc::new(StateStore::new(&dir)?);
    let graph = GraphStore::new(store.clone());
    let state = AppState::new(dir, Config::default(), store, state_store, graph, None);

    let runtime = tokio::runtime::Runtime::new()?;
    // `--hits`: what the first search returns, before the loop keeps any of it.
    if std::env::args().any(|value| value == "--hits") {
        if let Ok(plan) = investigate::plan::plan(&goal) {
            let request = fndr_lib::context_runtime::RetrieveRequest {
                query: plan.query(),
                time: plan.time.clone(),
                limit: 12,
                ..Default::default()
            };
            let result = runtime.block_on(fndr_lib::context_runtime::retrieve(&state, &request))?;
            eprintln!("query {:?} strong {}", plan.query(), result.strong_match);
            for hit in &result.hits {
                eprintln!(
                    "hit {:.3} {} routes {:?} terms {:?}",
                    hit.score, hit.memory_id, hit.why.routes, hit.why.matched_terms
                );
            }
        }
    }
    let stop = AtomicBool::new(false);
    let file = runtime.block_on(investigate::run(&state, &goal, &stop, |event| {
        if event.status != investigate::StepStatus::Running && !as_json {
            eprintln!("step {}", serde_json::to_string(event).unwrap_or_default());
        }
    }));

    if as_json {
        println!("{}", serde_json::to_string_pretty(&file)?);
        return Ok(());
    }

    println!("\n# {} ({:?})", file.title, file.outcome);
    println!(
        "{} memories, apps: {}, {} to {}, {} steps in {} ms",
        file.stats.memories,
        file.stats.apps.join(", "),
        day(file.stats.first_ms),
        day(file.stats.last_ms),
        file.steps.len(),
        file.stats.ms
    );
    println!("\n## What happened");
    for entry in &file.timeline {
        println!(
            "- {} [{}] {} ({})",
            day(entry.at_ms),
            entry.app,
            entry.line,
            entry.memory_id
        );
    }
    println!("\n## Decisions");
    for finding in &file.decisions {
        println!(
            "- {} ({}, {})",
            finding.text,
            day(finding.at_ms),
            finding.memory_id
        );
    }
    println!("\n## Still open");
    for item in &file.open {
        println!(
            "- {} ({}{})",
            item.text,
            item.memory_id.as_deref().unwrap_or("to-do"),
            if item.todo_id.is_some() {
                ", to-do"
            } else {
                ""
            }
        );
    }
    println!("\n## Recorded as done");
    for item in &file.done {
        println!(
            "- {} ({})\n    later: \"{}\" ({}, {})",
            item.text,
            item.memory_id,
            item.clause,
            day(item.done_at_ms),
            item.done_memory_id
        );
    }
    println!("\n## Problems seen");
    for finding in &file.problems {
        println!("- {} ({})", finding.text, finding.memory_id);
    }
    println!("\n## Places");
    for place in &file.places {
        println!(
            "- {} [{}] ({})",
            place.label, place.app_name, place.memory_id
        );
    }
    Ok(())
}
