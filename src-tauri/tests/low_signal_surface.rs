//! Store-backed regression for the read-side surface policy: fixture records go
//! through `Store::add_batch` normalization, come back via `list_recent_results`,
//! and are partitioned exactly as Search/Vault/Home partition them.

use fndr_lib::config::DEFAULT_IMAGE_EMBEDDING_DIM;
use fndr_lib::embedding::EMBEDDING_DIM;
use fndr_lib::memory_quality::{partition_surfaceable, LowSignalReason};
use fndr_lib::storage::{MemoryRecord, Store};

fn base(id: &str, ts: i64, app: &str, title: &str, text: &str) -> MemoryRecord {
    MemoryRecord {
        id: id.to_string(),
        timestamp: ts,
        day_bucket: chrono::Local::now().format("%Y-%m-%d").to_string(),
        app_name: app.to_string(),
        window_title: title.to_string(),
        session_id: format!("session-{id}"),
        text: text.to_string(),
        clean_text: text.to_string(),
        ocr_confidence: 0.93,
        ocr_block_count: 6,
        snippet: text.chars().take(160).collect(),
        summary_source: "fixture".to_string(),
        noise_score: 0.02,
        session_key: format!("fixture-{id}"),
        embedding: vec![0.0; EMBEDDING_DIM],
        image_embedding: vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM],
        snippet_embedding: vec![0.0; EMBEDDING_DIM],
        support_embedding: vec![0.0; EMBEDDING_DIM],
        decay_score: 1.0,
        specificity_score: 0.8,
        intent_score: 0.7,
        entity_score: 0.6,
        agent_usefulness_score: 0.75,
        evidence_confidence: 0.85,
        retrieval_value_score: 0.7,
        ocr_noise_score: 0.05,
        ..Default::default()
    }
}

#[test]
fn surface_policy_over_real_store_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("store");
    let now = chrono::Utc::now().timestamp_millis();

    let high_signal = base(
        "fx-high-signal",
        now - 1_000,
        "Cursor",
        "capture/mod.rs — fndr",
        "error[E0502]: cannot borrow `self.frame_buffer` as mutable because it is also borrowed as immutable --> src/capture/mod.rs:412:9",
    );

    let mut reviewed = base(
        "fx-reviewed",
        now - 2_000,
        "Google Chrome",
        "CS 4500 Alpha Release — Canvas",
        "Alpha Release Expectations: most rank 1 features complete, framework for rank 2, components talk to each other",
    );
    reviewed.enrichment_status = "reviewed_local".to_string();
    reviewed.reviewed_at_ms = now - 500;
    reviewed.insight_what_happened = "Read the CS 4500 Alpha rubric.".to_string();

    let mut review_failed = base(
        "fx-review-failed",
        now - 3_000,
        "Slack",
        "#fndr-team — Slack",
        "Jordan Lee: can we move the alpha dry run to Thursday at 4pm in the MEB lab? I booked room 3147",
    );
    review_failed.enrichment_status = "review_failed".to_string();

    let mut filename_only = base(
        "fx-filename-only",
        now - 4_000,
        "ChatGPT",
        "ChatGPT",
        "ChatGPT:",
    );
    filename_only.ocr_block_count = 1;
    filename_only.synthesis_branch = "llm_ocr_grounded_visual_fallback".to_string();
    filename_only.display_summary =
        "Screen capture (visual) ChatGPT_1789709739566.png. ChatGPT".to_string();

    let mut visual_metadata = base(
        "fx-visual-metadata",
        now - 5_000,
        "Preview",
        "IMG_4471.HEIC",
        "",
    );
    visual_metadata.ocr_block_count = 0;
    visual_metadata.ocr_confidence = 0.0;
    visual_metadata.enrichment_status = "visual_metadata_fallback".to_string();
    visual_metadata.synthesis_branch = "visual_metadata_fallback".to_string();

    // Two near-identical captures of the same screen in one batch.
    let dup_a = base(
        "fx-dup-a",
        now - 6_000,
        "Cursor",
        "reranker.rs — fndr",
        "fn rerank_results(query: &QueryContext, results: Vec<SearchResult>) -> (Vec<SearchResult>, RerankStats)",
    );
    let mut dup_b = dup_a.clone();
    dup_b.id = "fx-dup-b".to_string();
    dup_b.timestamp = now - 5_500;

    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(async {
        store
            .add_batch(&[
                high_signal,
                reviewed,
                review_failed,
                filename_only,
                visual_metadata,
                dup_a,
                dup_b,
            ])
            .await
            .expect("add_batch");
    });

    let listed = rt
        .block_on(async { store.list_recent_results(100, None).await })
        .expect("list");
    let dup_count = listed
        .iter()
        .filter(|r| r.id.starts_with("fx-dup-"))
        .count();
    assert_eq!(
        dup_count, 1,
        "near-identical captures in one batch must collapse to one record"
    );

    let (kept, hidden) = partition_surfaceable(listed);
    let kept_ids: Vec<&str> = kept.iter().map(|r| r.id.as_str()).collect();
    assert!(kept_ids.contains(&"fx-high-signal"), "kept: {kept_ids:?}");
    assert!(kept_ids.contains(&"fx-reviewed"), "kept: {kept_ids:?}");
    assert!(kept_ids.contains(&"fx-review-failed"), "kept: {kept_ids:?}");
    assert!(!kept_ids.contains(&"fx-filename-only"));
    assert!(!kept_ids.contains(&"fx-visual-metadata"));

    let hidden_ids: Vec<(&str, LowSignalReason)> = hidden
        .iter()
        .map(|(r, why)| (r.id.as_str(), *why))
        .collect();
    assert!(
        hidden_ids.contains(&("fx-filename-only", LowSignalReason::ImageOnly)),
        "hidden: {hidden_ids:?}"
    );
    assert!(
        hidden_ids.contains(&("fx-visual-metadata", LowSignalReason::ImageOnly)),
        "hidden: {hidden_ids:?}"
    );
}
