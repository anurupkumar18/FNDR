//! Investigate (ADR 039) through its real tools on a real store: the search
//! every surface uses, Resume's threads, and the one visibility rule.

use fndr_lib::config::{Config, DEFAULT_IMAGE_EMBEDDING_DIM};
use fndr_lib::embedding::{Embedder, EMBEDDING_DIM};
use fndr_lib::graph::GraphStore;
use fndr_lib::investigate::{self, CaseFile, Outcome, StepEvent, StepKind, StepStatus};
use fndr_lib::storage::{MemoryRecord, StateStore, Store};
use fndr_lib::AppState;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const HOUR_MS: i64 = 60 * 60 * 1000;

struct Row {
    id: &'static str,
    app: &'static str,
    title: &'static str,
    text: &'static str,
    project: &'static str,
    hours_ago: i64,
    decisions: &'static [&'static str],
    next_steps: &'static [&'static str],
}

const ROWS: &[Row] = &[
    Row {
        id: "vendor-thread",
        app: "Slack",
        title: "#procurement - Slack",
        text: "Dana asked whether the Zephyr vendor contract should renew at the same price.",
        project: "Vendor renewal",
        hours_ago: 72,
        decisions: &["Renew Zephyr for one year, not three"],
        next_steps: &[
            "Sign the Zephyr renewal",
            "Ask finance about the budget line",
        ],
    },
    Row {
        id: "vendor-contract",
        app: "Google Chrome",
        title: "Zephyr vendor contract - Google Docs",
        text: "Reviewed the Zephyr vendor contract and marked the auto-renew clause for legal.",
        project: "Vendor renewal",
        hours_ago: 48,
        decisions: &[],
        next_steps: &[],
    },
    // Names neither word of the request: only the thread reaches it.
    Row {
        id: "vendor-quotes",
        app: "Microsoft Excel",
        title: "Renewal quotes.xlsx",
        text: "Compared three renewal quotes and the middle one came in eight percent lower.",
        project: "Vendor renewal",
        hours_ago: 30,
        decisions: &["Use the middle quote as the ceiling"],
        next_steps: &[],
    },
    Row {
        id: "vendor-signed",
        app: "Mail",
        title: "Re: Zephyr renewal",
        text: "Legal approved the clause, and Dana signed the Zephyr renewal this morning.",
        project: "Vendor renewal",
        hours_ago: 5,
        decisions: &[],
        next_steps: &[],
    },
    // The same work, in an app blocked since it was captured.
    Row {
        id: "vendor-blocked",
        app: "Signal",
        title: "Dana",
        text: "The Zephyr vendor contract price is confidential until the board meets.",
        project: "Vendor renewal",
        hours_ago: 20,
        decisions: &["Keep the price confidential"],
        next_steps: &[],
    },
    Row {
        id: "offsite-agenda",
        app: "Notion",
        title: "Offsite agenda",
        text: "Drafted the team offsite agenda with a contract review session on day two.",
        project: "Team offsite",
        hours_ago: 26,
        decisions: &["Hold the offsite in March"],
        next_steps: &["Book the venue"],
    },
    Row {
        id: "offsite-venue",
        app: "Google Chrome",
        title: "Venue shortlist",
        text: "Shortlisted two venues for the team offsite near the lake.",
        project: "Team offsite",
        hours_ago: 25,
        decisions: &[],
        next_steps: &[],
    },
];

fn lines(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn seeded_state(runtime: &tokio::runtime::Runtime) -> (tempfile::TempDir, AppState) {
    std::env::set_var("FNDR_ALLOW_MOCK_EMBEDDER", "1");
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(Store::new(dir.path()).expect("store"));
    let state_store = Arc::new(StateStore::new(dir.path()).expect("state store"));
    let embedder = Embedder::new().expect("embedder");
    let texts: Vec<String> = ROWS.iter().map(|row| row.text.to_string()).collect();
    let embeddings = embedder.embed_batch(&texts).expect("embeddings");
    let now = chrono::Utc::now().timestamp_millis();
    let records: Vec<MemoryRecord> = ROWS
        .iter()
        .zip(embeddings)
        .map(|(row, embedding)| MemoryRecord {
            id: row.id.to_string(),
            timestamp: now - row.hours_ago * HOUR_MS,
            app_name: row.app.to_string(),
            window_title: row.title.to_string(),
            session_id: format!("session-{}", row.id),
            text: row.text.to_string(),
            clean_text: row.text.to_string(),
            snippet: row.text.to_string(),
            display_summary: row.text.to_string(),
            summary_source: "llm".to_string(),
            project: row.project.to_string(),
            decisions: lines(row.decisions),
            next_steps: lines(row.next_steps),
            ocr_block_count: 5,
            ocr_confidence: 0.9,
            embedding: embedding.clone(),
            snippet_embedding: embedding,
            support_embedding: vec![0.0; EMBEDDING_DIM],
            image_embedding: vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM],
            decay_score: 1.0,
            ..Default::default()
        })
        .collect();
    runtime
        .block_on(store.add_batch(&records))
        .expect("add records");

    // Ranking is under test, not the route time budgets: a loaded machine
    // on a debug build drops keyword hits at production budgets.
    let mut config = Config::default();
    config.search.semantic_timeout_ms = 10_000;
    config.search.snippet_timeout_ms = 10_000;
    config.search.keyword_timeout_ms = 10_000;
    config.search.keyword_variant_timeout_ms = 5_000;
    config.blocklist = vec!["Signal".to_string()];

    let graph = GraphStore::new(store.clone());
    let state = AppState::new(
        dir.path().to_path_buf(),
        config,
        store,
        state_store,
        graph,
        None,
    );
    (dir, state)
}

fn run(
    runtime: &tokio::runtime::Runtime,
    state: &AppState,
    goal: &str,
) -> (CaseFile, Vec<StepEvent>) {
    let stop = AtomicBool::new(false);
    let mut heard = Vec::new();
    let file = runtime.block_on(investigate::run(state, goal, &stop, |event| {
        heard.push(event.clone())
    }));
    (file, heard)
}

fn ids(file: &CaseFile) -> Vec<&str> {
    file.timeline
        .iter()
        .map(|entry| entry.memory_id.as_str())
        .collect()
}

#[test]
fn a_case_file_holds_one_piece_of_work_across_apps_and_nothing_hidden() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = seeded_state(&runtime);

    let (file, heard) = run(
        &runtime,
        &state,
        "Catch me up on the Zephyr vendor contract",
    );

    assert_eq!(file.outcome, Outcome::Found, "steps: {:?}", file.steps);
    assert_eq!(file.title, "Vendor renewal");
    // In the order things happened, including the sheet the request's words
    // never find, and without the blocked app or the other project.
    assert_eq!(
        ids(&file),
        vec![
            "vendor-thread",
            "vendor-contract",
            "vendor-quotes",
            "vendor-signed"
        ]
    );
    assert_eq!(file.stats.memories, 4);
    assert_eq!(file.stats.apps.len(), 4);

    let kinds: Vec<StepKind> = file.steps.iter().map(|step| step.kind).collect();
    assert_eq!(kinds[..2], [StepKind::Search, StepKind::Thread]);
    assert_eq!(*kinds.last().unwrap(), StepKind::Check);
    assert!(file.steps.len() <= investigate::agent::MAX_STEPS);

    let decisions: Vec<&str> = file
        .decisions
        .iter()
        .map(|item| item.text.as_str())
        .collect();
    assert_eq!(
        decisions,
        vec![
            "Renew Zephyr for one year, not three",
            "Use the middle quote as the ceiling"
        ]
    );
    // A later memory records one step as done; the other stays on record.
    assert_eq!(file.done.len(), 1, "{:?}", file.done);
    assert_eq!(file.done[0].text, "Sign the Zephyr renewal");
    assert_eq!(file.done[0].done_memory_id, "vendor-signed");
    assert!(file.done[0].clause.contains("signed the Zephyr renewal"));
    let open: Vec<&str> = file.open.iter().map(|item| item.text.as_str()).collect();
    assert_eq!(open, vec!["Ask finance about the budget line"]);

    // Every line names a memory of this case.
    let case_ids = ids(&file);
    assert!(file
        .decisions
        .iter()
        .all(|item| case_ids.contains(&item.memory_id.as_str())));

    // What the panel hears is the file's steps, each started then ended,
    // and no event carries a word of any memory.
    assert_eq!(heard.len(), file.steps.len() * 2);
    assert!(heard
        .iter()
        .step_by(2)
        .all(|event| event.status == StepStatus::Running));
    let wire = serde_json::to_string(&heard).expect("json").to_lowercase();
    for word in ["zephyr", "vendor", "dana", "renew", "quote", "slack"] {
        assert!(!wire.contains(word), "an event carried {word:?}: {wire}");
    }
}

#[test]
fn something_absent_is_nothing_found_after_a_wider_search() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = seeded_state(&runtime);

    let (file, _) = run(&runtime, &state, "the Mars lander budget spreadsheet");

    assert_eq!(file.outcome, Outcome::NothingFound, "{:?}", file.steps);
    let kinds: Vec<StepKind> = file.steps.iter().map(|step| step.kind).collect();
    assert_eq!(kinds, vec![StepKind::Search, StepKind::Widen]);
    assert_eq!(file.line_count(), 0);
    assert!(file.places.is_empty());
}

#[test]
fn a_request_with_no_topic_and_private_mode_never_read_memory() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = seeded_state(&runtime);

    let (file, heard) = run(&runtime, &state, "what did I do yesterday");
    assert_eq!(file.outcome, Outcome::NeedsTopic);
    assert!(heard.is_empty());

    state.is_incognito.store(true, Ordering::SeqCst);
    let (file, heard) = run(&runtime, &state, "the Zephyr vendor contract");
    assert_eq!(file.outcome, Outcome::Private);
    assert!(heard.is_empty());
    assert_eq!(file.line_count(), 0);
}
