//! Scores Files search (ADR 033) on the synthetic testbed that
//! `scripts/files/make-testbed.sh` builds, with the labeled queries in
//! `evals/files/queries.json`: recall at 1 and 5, and mean reciprocal rank.
//! Indexes with the real model helper and macOS's readers, in a temporary
//! index; nothing touches FNDR's own. Exits 1 below the thresholds.
//!
//! Usage: make files-eval, or
//! cargo run --example files_eval -- <testbed> <models dir> [queries.json]

use fndr_lib::files::helper::{self, FilesHelper};
use fndr_lib::files::indexer::{self, Tools};
use fndr_lib::files::reader::{self, FilesReader, Reader};
use fndr_lib::files::searcher::{SearchOptions, Searcher};
use fndr_lib::files::store::FilesStore;
use fndr_lib::files::{model, IndexOptions};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Deserialize)]
struct Eval {
    thresholds: Thresholds,
    queries: Vec<Query>,
}

#[derive(Deserialize)]
struct Thresholds {
    recall_at_1: f64,
    recall_at_5: f64,
}

#[derive(Deserialize)]
struct Query {
    query: String,
    expect: String,
    page: Option<usize>,
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(testbed), Some(models)) = (args.next(), args.next()) else {
        eprintln!("usage: files_eval <testbed> <models dir> [queries.json]");
        std::process::exit(2);
    };
    let queries = args.next().unwrap_or_else(|| {
        format!("{}/../evals/files/queries.json", env!("CARGO_MANIFEST_DIR"))
    });
    let eval: Eval = serde_json::from_str(&std::fs::read_to_string(&queries).expect("read queries"))
        .expect("parse queries");
    let models = PathBuf::from(models);
    let helper_program = helper::locate().expect("no fndr-files-helper: run make files-helper");
    let reader_program = reader::locate().expect("no fndr-files-reader: build the app crate on a Mac");

    let work = tempfile::tempdir().expect("temporary folder");
    let store = FilesStore::open(&work.path().join("index.sqlite")).expect("index");
    let options = IndexOptions::default();
    let started = Instant::now();
    let report = indexer::sync(&store, &[PathBuf::from(&testbed)], &options, Path::new("/nonexistent-home"))
        .expect("sync");
    let mut embedder = FilesHelper::start(
        &helper_program,
        &models.join(model::TEXT.name),
        Some(&models.join(model::MEDIA.name)),
    )
    .expect("start the helper");
    let mut files_reader = FilesReader::start(&reader_program).expect("start the reader");
    let scratch = work.path().join("scratch");
    let mut tools = Tools {
        embedder: &mut embedder,
        reader: Some(&mut files_reader as &mut dyn Reader),
        scratch: &scratch,
    };
    loop {
        let run = indexer::index_pending(&store, &mut tools, &options, 16).expect("index");
        let later = indexer::continue_long_files(&store, &mut tools, &options, 4).expect("index");
        if run.done + run.partial + run.skipped + run.failed + later.done + later.partial == 0 {
            break;
        }
    }
    let counts = store.counts().expect("counts");
    println!(
        "indexed {} files ({} failed, {} vectors) from {} found in {:.1} s",
        counts.done,
        counts.failed,
        counts.vectors,
        report.candidates.len(),
        started.elapsed().as_secs_f64()
    );
    drop(tools);

    let mut query_helper = FilesHelper::start(&helper_program, &models.join(model::TEXT.name), None)
        .expect("start the query helper");
    let searcher = Searcher::load(&store).expect("load vectors");
    let (mut at_1, mut at_5, mut reciprocal) = (0, 0, 0.0);
    for case in &eval.queries {
        let vector = query_helper.embed_query(&case.query, false).ok();
        let hits = searcher
            .search(&store, &case.query, vector.as_deref(), &SearchOptions::default())
            .expect("search");
        let rank = hits.iter().position(|hit| {
            Path::new(&hit.path).file_name().is_some_and(|name| name == case.expect.as_str())
        });
        let place_ok = match (case.page, rank) {
            (Some(page), Some(index)) => hits[index].loc.map(|loc| loc as usize) == Some(page),
            _ => true,
        };
        if let Some(index) = rank {
            reciprocal += 1.0 / (index + 1) as f64;
            at_1 += usize::from(index == 0);
            at_5 += usize::from(index < 5);
        }
        println!(
            "{:<4} {:<45} {}{}",
            rank.map_or("-".to_string(), |index| (index + 1).to_string()),
            case.query,
            case.expect,
            if place_ok { "" } else { "  (wrong page)" }
        );
    }
    let total = eval.queries.len() as f64;
    let (recall_1, recall_5) = (at_1 as f64 / total, at_5 as f64 / total);
    println!(
        "R@1 {recall_1:.2} (at least {:.2})  R@5 {recall_5:.2} (at least {:.2})  MRR {:.3}  over {} queries",
        eval.thresholds.recall_at_1,
        eval.thresholds.recall_at_5,
        reciprocal / total,
        eval.queries.len()
    );
    let pass = recall_1 >= eval.thresholds.recall_at_1 && recall_5 >= eval.thresholds.recall_at_5;
    std::process::exit(i32::from(!pass));
}
