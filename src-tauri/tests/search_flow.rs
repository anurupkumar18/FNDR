//! Integration smoke: LanceDB + demo seed + keyword search.

use fndr_lib::demo;
use fndr_lib::embed::Embedder;
use fndr_lib::store::Store;

#[tokio::test]
async fn demo_seed_keyword_search_finds_content() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("store");
    let embedder = Embedder::new().expect("embedder");
    let records = demo::build_demo_records(&embedder).expect("demo records");
    store.add_batch(&records[..2]).await.expect("add_batch");

    let hits = store
        .keyword_search("OAuth", 10, None, None)
        .await
        .expect("keyword_search");
    assert!(
        !hits.is_empty(),
        "expected at least one keyword hit for OAuth in seeded rows"
    );
}
