//! Integration smoke: LanceDB + keyword search over seeded rows.

use fndr_lib::embed::Embedder;
use fndr_lib::store::{MemoryRecord, Store};

#[test]
fn keyword_search_finds_seeded_content() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("store");
    let embedder = Embedder::new().expect("embedder");

    let text = "Investigated oauth callback handling and redirect uri mismatch in auth flow";
    let embedding = embedder
        .embed_batch(&[text.to_string()])
        .expect("embedding")
        .into_iter()
        .next()
        .expect("embedding vector");

    let record = MemoryRecord {
        id: "seed-1".to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        day_bucket: "2026-04-15".to_string(),
        app_name: "Terminal".to_string(),
        bundle_id: None,
        window_title: "Auth Debug".to_string(),
        session_id: "seed-session".to_string(),
        text: text.to_string(),
        clean_text: text.to_string(),
        ocr_confidence: 0.95,
        ocr_block_count: 6,
        snippet: "Debugged oauth callback mismatch".to_string(),
        summary_source: "llm".to_string(),
        noise_score: 0.05,
        session_key: "terminal:oauth".to_string(),
        embedding,
        image_embedding: vec![0.0; 512],
        screenshot_path: None,
        url: None,
    };

    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(async {
        store.add_batch(&[record]).await.expect("add_batch");
    });

    let hits = rt
        .block_on(async { store.keyword_search("oauth", 10, None, None).await })
        .expect("keyword_search");
    assert!(
        !hits.is_empty(),
        "expected at least one keyword hit for OAuth in seeded rows"
    );
}
