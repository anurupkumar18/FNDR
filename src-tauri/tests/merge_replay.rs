//! Deterministic, synthetic replay coverage for the capture merge decision.
//!
//! The fixture corpus deliberately contains no user captures. Each frame is
//! labeled with the story a person would expect it to belong to; the Python
//! scorer consumes the JSONL emitted by this test.

use fndr_lib::capture::replay_memory_records;
use fndr_lib::config::{Config, DEFAULT_IMAGE_EMBEDDING_DIM};
use fndr_lib::embedding::EMBEDDING_DIM;
use fndr_lib::graph::GraphStore;
use fndr_lib::storage::{MemoryRecord, StateStore, Store};
use fndr_lib::AppState;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Deserialize)]
struct SessionFixture {
    session_id: String,
    frames: Vec<FrameFixture>,
}

#[derive(Debug, Deserialize)]
struct FrameFixture {
    frame_id: String,
    story_id: String,
    app: String,
    window_title: String,
    url: Option<String>,
    evidence: String,
}

#[derive(Serialize)]
struct ScoredFrame<'a> {
    frame_id: &'a str,
    story_id: &'a str,
    memory_id: &'a str,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sessions")
}

fn fixture_paths() -> Vec<PathBuf> {
    let mut paths = fs::read_dir(fixtures_dir())
        .expect("session fixtures directory")
        .map(|entry| entry.expect("fixture entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn load_session(path: &Path) -> SessionFixture {
    serde_json::from_str(&fs::read_to_string(path).expect("read session fixture"))
        .expect("parse synthetic session fixture")
}

fn record(frame: &FrameFixture, session_id: &str, sequence: i64) -> MemoryRecord {
    MemoryRecord {
        id: frame.frame_id.clone(),
        timestamp: 1_790_000_000_000 + sequence * 60_000,
        day_bucket: "2026-09-22".to_string(),
        app_name: frame.app.clone(),
        window_title: frame.window_title.clone(),
        session_id: session_id.to_string(),
        text: frame.evidence.clone(),
        clean_text: frame.evidence.clone(),
        ocr_confidence: 0.95,
        ocr_block_count: 4,
        snippet: frame.evidence.clone(),
        summary_source: "fixture".to_string(),
        noise_score: 0.0,
        session_key: format!("fixture:{session_id}"),
        lexical_shadow: frame.evidence.clone(),
        url: frame.url.clone(),
        embedding: vec![0.0; EMBEDDING_DIM],
        image_embedding: vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM],
        snippet_embedding: vec![0.0; EMBEDDING_DIM],
        support_embedding: vec![0.0; EMBEDDING_DIM],
        decay_score: 1.0,
        ..Default::default()
    }
}

fn state(temp_dir: &Path) -> AppState {
    let store = Arc::new(Store::new(temp_dir).expect("store"));
    let state_store = Arc::new(StateStore::new(temp_dir).expect("state store"));
    let graph = GraphStore::new(store.clone());
    AppState::new(
        temp_dir.to_path_buf(),
        Config::default(),
        store,
        state_store,
        graph,
        None,
        None,
    )
}

#[test]
fn replays_synthetic_sessions_and_emits_gold_scoring_input() {
    let fixture_paths = fixture_paths();
    assert_eq!(fixture_paths.len(), 6, "exactly six replay sessions");

    let output_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.eval-tmp");
    fs::create_dir_all(&output_dir).expect("create replay output directory");
    for path in fs::read_dir(&output_dir).expect("read replay output directory") {
        let path = path.expect("replay output entry").path();
        if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("merge-"))
        {
            fs::remove_file(path).expect("clear prior merge replay output");
        }
    }

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    for path in fixture_paths {
        let fixture = load_session(&path);
        assert_eq!(fixture.frames.len(), 8, "{}", fixture.session_id);
        let temp_dir = tempfile::tempdir().expect("temporary replay store");
        let records = fixture
            .frames
            .iter()
            .enumerate()
            .map(|(index, frame)| record(frame, &fixture.session_id, index as i64))
            .collect::<Vec<_>>();
        let outcomes = runtime
            .block_on(replay_memory_records(&state(temp_dir.path()), &records))
            .expect("replay merge session");

        assert_eq!(outcomes.len(), fixture.frames.len());
        assert!(outcomes.iter().all(|outcome| !outcome.id.is_empty()));

        let jsonl = fixture
            .frames
            .iter()
            .zip(&outcomes)
            .map(|(frame, outcome)| {
                serde_json::to_string(&ScoredFrame {
                    frame_id: &frame.frame_id,
                    story_id: &frame.story_id,
                    memory_id: &outcome.id,
                })
                .expect("serialize scored frame")
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(
            output_dir.join(format!("merge-{}.jsonl", fixture.session_id)),
            format!("{jsonl}\n"),
        )
        .expect("write replay output");
    }
}
