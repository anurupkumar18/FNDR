//! VS-09: one retrieval function every surface calls.

use fndr_lib::config::{Config, DEFAULT_IMAGE_EMBEDDING_DIM};
use fndr_lib::context_runtime::{retrieve, run_query, ComposeMode, RetrieveRequest};
use fndr_lib::embedding::{Embedder, EMBEDDING_DIM};
use fndr_lib::graph::GraphStore;
use fndr_lib::storage::{MemoryRecord, StateStore, Store};
use fndr_lib::AppState;
use std::collections::HashSet;
use std::sync::Arc;

fn record(
    id: &str,
    app: &str,
    title: &str,
    text: &str,
    age_ms: i64,
    embedding: Vec<f32>,
) -> MemoryRecord {
    let now = chrono::Utc::now().timestamp_millis();
    MemoryRecord {
        id: id.to_string(),
        timestamp: now - age_ms,
        app_name: app.to_string(),
        window_title: title.to_string(),
        session_id: format!("session-{id}"),
        text: text.to_string(),
        clean_text: text.to_string(),
        snippet: text.to_string(),
        summary_source: "llm".to_string(),
        embedding: embedding.clone(),
        snippet_embedding: embedding,
        support_embedding: vec![0.0; EMBEDDING_DIM],
        image_embedding: vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM],
        decay_score: 1.0,
        ..Default::default()
    }
}

fn seeded_state(runtime: &tokio::runtime::Runtime) -> (tempfile::TempDir, AppState) {
    std::env::set_var("FNDR_ALLOW_MOCK_EMBEDDER", "1");
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(Store::new(dir.path()).expect("store"));
    let state_store = Arc::new(StateStore::new(dir.path()).expect("state store"));
    let embedder = Embedder::new().expect("embedder");
    let rows = [
        (
            "vendor",
            "Slack",
            "Vendor thread",
            "The Zephyr vendor contract renews next quarter at the same price",
        ),
        (
            "budget",
            "Sheets",
            "Budget review",
            "Monthly budget review with budget lines for the design team",
        ),
        (
            "standup",
            "Zoom",
            "Daily standup",
            "Standup notes: deploy blocked on the staging database migration",
        ),
        (
            "lunch",
            "Slack",
            "Lunch",
            "Ordered sandwiches for the team lunch on Friday",
        ),
    ];
    let texts = rows.iter().map(|row| row.3.to_string()).collect::<Vec<_>>();
    let embeddings = embedder.embed_batch(&texts).expect("embeddings");
    let records = rows
        .iter()
        .zip(embeddings)
        .enumerate()
        .map(|(index, ((id, app, title, text), embedding))| {
            record(id, app, title, text, (index as i64 + 1) * 60_000, embedding)
        })
        .collect::<Vec<_>>();
    runtime
        .block_on(store.add_batch(&records))
        .expect("add records");
    let graph = GraphStore::new(store.clone());
    let state = AppState::new(
        dir.path().to_path_buf(),
        Config::default(),
        store,
        state_store,
        graph,
        None,
        None,
    );
    (dir, state)
}

fn request(query: &str) -> RetrieveRequest {
    RetrieveRequest {
        query: query.to_string(),
        limit: 10,
        ..Default::default()
    }
}

#[test]
fn retrieve_explains_which_routes_and_words_found_each_hit() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = seeded_state(&runtime);

    let result = runtime
        .block_on(retrieve(&state, &request("zephyr contract")))
        .expect("retrieve");

    let top = result.hits.first().expect("a hit");
    assert_eq!(top.memory_id, "vendor");
    assert!(top.why.routes.iter().any(|route| route == "keyword"));
    assert_eq!(top.why.matched_terms, vec!["zephyr", "contract"]);
    assert!(top.score > 0.0);
    assert!(top.chunk_id.is_none());
    for hit in &result.hits {
        if !hit.why.routes.iter().any(|route| route == "keyword") {
            assert!(
                hit.why.matched_terms.is_empty(),
                "{} has words but no keyword route",
                hit.memory_id
            );
        }
    }
}

#[test]
fn retrieve_and_ask_rank_the_same_memories_in_the_same_order() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = seeded_state(&runtime);

    for query in ["zephyr contract", "staging database", "team lunch"] {
        let hits = runtime
            .block_on(retrieve(&state, &request(query)))
            .expect("retrieve")
            .hits
            .into_iter()
            .map(|hit| hit.memory_id)
            .collect::<Vec<_>>();
        let cards = runtime
            .block_on(run_query(&state, query, 10, ComposeMode::Cards))
            .expect("run_query")
            .cards
            .into_iter()
            .map(|card| card.id)
            .collect::<Vec<_>>();
        assert_eq!(hits, cards, "{query}");
    }
}

#[test]
fn retrieve_keeps_the_app_filter_and_the_limit() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = seeded_state(&runtime);

    let in_slack = runtime
        .block_on(retrieve(
            &state,
            &RetrieveRequest {
                query: "team".to_string(),
                app: Some("Slack".to_string()),
                limit: 10,
                ..Default::default()
            },
        ))
        .expect("retrieve");
    let ids = in_slack
        .hits
        .iter()
        .map(|hit| hit.memory_id.as_str())
        .collect::<HashSet<_>>();
    assert!(!ids.is_empty());
    assert!(ids.is_subset(&["vendor", "lunch"].into()), "{ids:?}");

    let one = runtime
        .block_on(retrieve(
            &state,
            &RetrieveRequest {
                query: "team".to_string(),
                limit: 1,
                ..Default::default()
            },
        ))
        .expect("retrieve");
    assert_eq!(one.hits.len(), 1);
}

#[test]
fn retrieve_types_carry_the_documented_fields() {
    let request = serde_json::to_value(RetrieveRequest {
        query: "q".to_string(),
        time: Some("today".to_string()),
        app: Some("Slack".to_string()),
        limit: 5,
    })
    .expect("request json");
    let keys = request
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    assert_eq!(
        keys,
        ["query", "time", "app", "limit"].map(String::from).into()
    );

    let hit = serde_json::to_value(fndr_lib::context_runtime::RetrieveHit {
        memory_id: "m".to_string(),
        chunk_id: None,
        score: 0.5,
        why: fndr_lib::context_runtime::RetrieveWhy {
            routes: vec!["vector".to_string()],
            matched_terms: Vec::new(),
        },
    })
    .expect("hit json");
    let keys = hit
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    assert_eq!(
        keys,
        ["memory_id", "chunk_id", "score", "why"]
            .map(String::from)
            .into()
    );
    let why = hit["why"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    assert_eq!(why, ["routes", "matched_terms"].map(String::from).into());
}

fn day_state(runtime: &tokio::runtime::Runtime) -> (tempfile::TempDir, AppState) {
    std::env::set_var("FNDR_ALLOW_MOCK_EMBEDDER", "1");
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(Store::new(dir.path()).expect("store"));
    let state_store = Arc::new(StateStore::new(dir.path()).expect("state store"));
    let embedder = Embedder::new().expect("embedder");
    let day = 24 * 60 * 60 * 1000;
    let rows = [
        (
            "standup-today",
            "Zoom",
            "Standup",
            "Standup notes about the launch checklist",
            60_000,
        ),
        (
            "standup-old",
            "Zoom",
            "Standup",
            "Standup notes about the launch checklist and pricing",
            3 * day,
        ),
        (
            "checklist-slack",
            "Slack",
            "Launch",
            "Launch checklist thread in the channel",
            2 * day,
        ),
    ];
    let texts = rows.iter().map(|row| row.3.to_string()).collect::<Vec<_>>();
    let embeddings = embedder.embed_batch(&texts).expect("embeddings");
    let records = rows
        .iter()
        .zip(embeddings)
        .map(|((id, app, title, text, age), embedding)| {
            record(id, app, title, text, *age, embedding)
        })
        .collect::<Vec<_>>();
    runtime
        .block_on(store.add_batch(&records))
        .expect("add records");
    let graph = GraphStore::new(store.clone());
    let state = AppState::new(
        dir.path().to_path_buf(),
        Config::default(),
        store,
        state_store,
        graph,
        None,
        None,
    );
    (dir, state)
}

#[test]
fn retrieve_turns_time_and_app_phrases_into_filters() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = day_state(&runtime);

    let three_days_ago = runtime
        .block_on(retrieve(
            &state,
            &request("the standup notes from three days ago"),
        ))
        .expect("retrieve");
    // The text keeps its words; the phrase becomes a filter.
    assert_eq!(
        three_days_ago.filters.query,
        "the standup notes from three days ago"
    );
    assert!(three_days_ago
        .filters
        .time
        .as_deref()
        .is_some_and(|time| time.starts_with("range:")));
    assert_eq!(
        three_days_ago
            .hits
            .iter()
            .map(|hit| hit.memory_id.as_str())
            .collect::<Vec<_>>(),
        vec!["standup-old"]
    );

    let in_slack = runtime
        .block_on(retrieve(&state, &request("the launch checklist in Slack")))
        .expect("retrieve");
    assert_eq!(in_slack.filters.app.as_deref(), Some("Slack"));
    let top = in_slack.hits.first().expect("a hit");
    assert!(!top.why.matched_terms.iter().any(|term| term == "slack"));
    assert_eq!(
        in_slack
            .hits
            .iter()
            .map(|hit| hit.memory_id.as_str())
            .collect::<Vec<_>>(),
        vec!["checklist-slack"]
    );
}

#[test]
fn retrieve_falls_back_to_the_whole_vault_when_a_parsed_filter_finds_nothing() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = day_state(&runtime);

    let result = runtime
        .block_on(retrieve(
            &state,
            &request("launch checklist from 10 days ago on Notion"),
        ))
        .expect("retrieve");

    // "on Notion" names no stored app, so it stays text; nothing is ten days
    // old here, so the search widens to everything with the words as typed.
    assert!(!result.hits.is_empty());
    assert_eq!(result.filters.time, None);
    assert_eq!(
        result.filters.query,
        "launch checklist from 10 days ago on Notion"
    );
}

#[test]
fn explicit_request_filters_win_over_phrases() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = day_state(&runtime);

    let result = runtime
        .block_on(retrieve(
            &state,
            &RetrieveRequest {
                query: "launch checklist in Slack".to_string(),
                app: Some("Zoom".to_string()),
                limit: 10,
                ..Default::default()
            },
        ))
        .expect("retrieve");

    assert_eq!(result.filters.app.as_deref(), Some("Zoom"));
    assert!(result
        .hits
        .iter()
        .all(|hit| hit.memory_id.starts_with("standup")));
}
