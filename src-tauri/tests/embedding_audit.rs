//! Synthetic corpus audit for the text that reaches FNDR's embedding and chunk paths.

use fndr_lib::config::{ChunkingConfig, DEFAULT_IMAGE_EMBEDDING_DIM};
use fndr_lib::embedding::EMBEDDING_DIM;
use fndr_lib::memory_embedding_document::compose_memory_embedding_document;
use fndr_lib::storage::MemoryRecord;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize)]
struct ScreenFixture {
    id: String,
    app_class: String,
    window_title: String,
    expected_text: String,
}

#[derive(Deserialize)]
struct SessionFixture {
    session_id: String,
    frames: Vec<SessionFrame>,
}

#[derive(Deserialize)]
struct SessionFrame {
    frame_id: String,
    app: String,
    window_title: String,
    evidence: String,
}

struct AuditInput {
    label: String,
    app: String,
    title: String,
    text: String,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn record(input: &AuditInput) -> MemoryRecord {
    MemoryRecord {
        id: input.label.clone(),
        timestamp: 1_790_000_000_000,
        day_bucket: "2026-09-22".to_string(),
        app_name: input.app.clone(),
        window_title: input.title.clone(),
        session_id: "embedding-audit".to_string(),
        text: input.text.clone(),
        clean_text: input.text.clone(),
        snippet: input.text.clone(),
        lexical_shadow: input.text.clone(),
        ocr_confidence: 0.95,
        ocr_block_count: 4,
        noise_score: 0.0,
        summary_source: "fixture".to_string(),
        session_key: input.label.clone(),
        embedding: vec![0.0; EMBEDDING_DIM],
        image_embedding: vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM],
        snippet_embedding: vec![0.0; EMBEDDING_DIM],
        support_embedding: vec![0.0; EMBEDDING_DIM],
        decay_score: 1.0,
        ..Default::default()
    }
}

fn normalized(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn inputs() -> Vec<AuditInput> {
    let root = root();
    let screens: Vec<ScreenFixture> = serde_json::from_str(
        &fs::read_to_string(root.join("src-tauri/tests/fixtures/screens/manifest.json"))
            .expect("read screen manifest"),
    )
    .expect("parse screen manifest");

    let mut inputs = screens
        .into_iter()
        .map(|screen| AuditInput {
            label: format!("screen:{}", screen.id),
            app: screen.app_class,
            title: screen.window_title,
            text: screen.expected_text,
        })
        .collect::<Vec<_>>();

    let sessions_dir = root.join("src-tauri/tests/fixtures/sessions");
    let mut session_paths = fs::read_dir(sessions_dir)
        .expect("read sessions")
        .map(|entry| entry.expect("session entry").path())
        .collect::<Vec<_>>();
    session_paths.sort();
    for path in session_paths {
        let session: SessionFixture =
            serde_json::from_str(&fs::read_to_string(path).expect("read session fixture"))
                .expect("parse session fixture");
        inputs.extend(session.frames.into_iter().map(|frame| AuditInput {
            label: format!("session:{}:{}", session.session_id, frame.frame_id),
            app: frame.app,
            title: frame.window_title,
            text: frame.evidence,
        }));
    }
    inputs
}

#[test]
fn audits_embedding_documents_for_screen_and_session_fixtures() {
    let inputs = inputs();
    assert_eq!(
        inputs.len(),
        78,
        "30 screen fixtures plus 48 session frames"
    );
    let config = ChunkingConfig::default();
    let mut rows = Vec::with_capacity(inputs.len());

    for input in &inputs {
        let document = compose_memory_embedding_document(&record(input), Some(&config));
        assert!(
            !document.primary_text.trim().is_empty(),
            "{} should retain embedding text",
            input.label
        );
        let chunks = document.support_texts;
        let chunk_count = chunks.len();
        let token_counts = chunks
            .iter()
            .map(|chunk| chunk.chars().count().div_ceil(4))
            .collect::<Vec<_>>();
        let mean_tokens = if token_counts.is_empty() {
            0
        } else {
            token_counts.iter().sum::<usize>() / token_counts.len()
        };
        let max_tokens = token_counts.iter().copied().max().unwrap_or(0);
        let unique_chunks = chunks
            .iter()
            .map(|chunk| normalized(chunk))
            .collect::<HashSet<_>>()
            .len();
        let duplicate_share = if chunk_count == 0 {
            0.0
        } else {
            (chunk_count - unique_chunks) as f32 / chunk_count as f32
        };
        let cleaned = fndr_lib::capture::text_cleanup::build_high_signal_text_for_app(
            &input.app,
            &input.text,
        );
        let noise_share = if cleaned.stats.total_lines == 0 {
            0.0
        } else {
            cleaned.stats.dropped_noise_lines as f32 / cleaned.stats.total_lines as f32
        };
        rows.push(format!(
            "| {} | {} | {} | {} | {:.0}% | {:.0}% |",
            input.label,
            chunk_count,
            mean_tokens,
            max_tokens,
            duplicate_share * 100.0,
            noise_share * 100.0,
        ));
    }

    let report = format!(
        "# Embedding and chunk audit\n\nSynthetic fixture baseline. Chunk size uses four characters per token. Noise share uses the capture text-cleanup rules.\n\n| Input | Chunks | Mean tokens | Max tokens | Duplicate chunks | Noise lines |\n| --- | ---: | ---: | ---: | ---: | ---: |\n{}\n",
        rows.join("\n"),
    );
    println!("{report}");
    fs::write(root().join("docs/evidence/W03/embedding-audit.md"), report)
        .expect("write embedding audit evidence");
}
