//! VS-09: one retrieval function every surface calls.

use fndr_lib::config::{Config, DEFAULT_IMAGE_EMBEDDING_DIM};
use fndr_lib::context_runtime::{
    retrieve, retrieve_search_results, run_query, ComposeMode, RetrieveRequest, STRONG_MATCH_SCORE,
};
use fndr_lib::embedding::{Embedder, EMBEDDING_DIM};
use fndr_lib::graph::GraphStore;
use fndr_lib::ipc::commands::search::search_ranked_results;
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

/// Route time budgets lifted to their maximums, as `retrieval_qa` does:
/// these tests check ranking, and with production budgets a loaded machine
/// (twelve tests in parallel on a debug build) drops keyword hits.
fn ranking_config() -> Config {
    let mut config = Config::default();
    config.search.semantic_timeout_ms = 10_000;
    config.search.snippet_timeout_ms = 10_000;
    config.search.keyword_timeout_ms = 10_000;
    config.search.keyword_variant_timeout_ms = 5_000;
    config
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
        ranking_config(),
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
fn public_retrieval_excludes_hidden_hits_and_nested_links_from_serialized_payloads() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = seeded_state(&runtime);
    let embedding = runtime
        .block_on(state.store.get_memory_by_id("vendor"))
        .unwrap()
        .unwrap()
        .embedding;
    // Distinct ages: keyword search treats rows with the same title, URL, and
    // timestamp as one capture, and keeps only one of them. Without a text
    // model (CI) keyword search is the only route, so a shared timestamp let
    // a hidden row stand in for the visible source.
    let make_record = |id: &str, age_ms: i64| {
        record(
            id,
            "Editor",
            "Cobalt release provenance",
            "Reviewed the Cobalt release provenance and recorded deployment verification evidence.",
            age_ms,
            embedding.clone(),
        )
    };
    let mut seed = make_record("cobalt-source", 1_000);
    seed.related_memory_ids = vec![
        "PRIVATE_BLOCKED_TARGET".into(),
        "PRIVATE_DELETED_TARGET".into(),
        "PRIVATE_MISSING_TARGET".into(),
        "earlier-cobalt-link".into(),
    ];
    seed.files_touched = vec!["/synthetic/visible-release.md".into()];
    let original_links = seed.related_memory_ids.clone();
    let mut linked = make_record("cobalt-visible-link", 2_000);
    linked.consolidated_from = vec!["earlier-cobalt-link".into()];
    let mut blocked = make_record("PRIVATE_BLOCKED_TARGET", 3_000);
    blocked.app_name = "PrivateWorkspace".into();
    blocked.files_touched = vec!["/synthetic/PRIVATE_BLOCKED_PATH.md".into()];
    blocked.decisions = vec!["PRIVATE_BLOCKED_DECISION".into()];
    let mut deleted = make_record("PRIVATE_DELETED_TARGET", 4_000);
    deleted.is_soft_deleted = true;
    deleted.files_touched = vec!["/synthetic/PRIVATE_DELETED_PATH.md".into()];
    deleted.decisions = vec!["PRIVATE_DELETED_DECISION".into()];
    runtime
        .block_on(
            state
                .store
                .add_batch_preserving_ids(&[seed, linked, blocked, deleted]),
        )
        .unwrap();
    // A rule added after capture must apply to the next read.
    state.config.write().blocklist = vec!["privateworkspace".into()];

    let (retrieved, rows) = runtime
        .block_on(retrieve_search_results(&state, &request("cobalt release")))
        .expect("public search results");
    let source = rows
        .iter()
        .find(|row| row.id == "cobalt-source")
        .expect("visible source");
    assert_eq!(source.related_memory_ids, ["cobalt-visible-link"]);
    assert!(retrieved.hits.iter().any(|hit| hit.memory_id == source.id));
    let search_payload = serde_json::to_string(&(retrieved, rows)).unwrap();
    assert!(!search_payload.contains("PRIVATE_"), "{search_payload}");
    assert!(!search_payload.contains("earlier-cobalt-link"));

    let answer = runtime
        .block_on(run_query(&state, "cobalt release", 10, ComposeMode::Cards))
        .expect("public Cards response");
    assert!(answer.cards.iter().any(|card| card.id == "cobalt-source"));
    assert!(answer.evidence.files.iter().any(|file| {
        file.path == "/synthetic/visible-release.md"
            && file.memory_ids.iter().any(|id| id == "cobalt-source")
    }));
    assert!(
        answer.debug_trace.is_some(),
        "debug trace must be exercised"
    );
    let cards_payload = serde_json::to_string(&answer).unwrap();
    assert!(!cards_payload.contains("PRIVATE_"), "{cards_payload}");
    assert!(!cards_payload.contains("earlier-cobalt-link"));

    let stored = runtime
        .block_on(state.store.get_memory_by_id("cobalt-source"))
        .unwrap()
        .unwrap();
    assert_eq!(stored.related_memory_ids, original_links);
}

#[test]
fn related_memories_use_surviving_text_only_when_no_links_are_stored() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = seeded_state(&runtime);
    let vendor = runtime
        .block_on(state.store.get_memory_by_id("vendor"))
        .unwrap()
        .unwrap();
    let source = record(
        "related-source",
        "Editor",
        "Zephyr vendor renewal",
        "The Zephyr vendor contract renews next quarter at the same price",
        1_000,
        vendor.embedding,
    );
    runtime
        .block_on(state.store.add_batch_preserving_ids(&[source]))
        .unwrap();
    let stored = runtime
        .block_on(state.store.get_memory_by_id("related-source"))
        .unwrap()
        .unwrap();
    assert!(stored.text.is_empty());
    assert!(stored.related_memory_ids.is_empty());
    let cards = runtime
        .block_on(fndr_lib::context_runtime::related_memories(
            &state, &stored.id, 1,
        ))
        .unwrap();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].id, "vendor");
    let reason = cards[0].surfacing_reason.as_ref().unwrap();
    assert_eq!(reason.headline, "Similar context");
    assert!(!reason.routes.is_empty());
    assert!(!reason.routes.iter().any(|route| route == "stored_link"));
    // A newly excluded result must not return through the similarity fallback.
    state.config.write().blocklist.push("Slack".into());
    let cards = runtime
        .block_on(fndr_lib::context_runtime::related_memories(
            &state, &stored.id, 12,
        ))
        .unwrap();
    assert!(cards
        .iter()
        .all(|card| card.app_name != "Slack" && card.id != stored.id));
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
        matched_text: None,
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
        ["memory_id", "chunk_id", "matched_text", "score", "why"]
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

    let result = serde_json::to_value(fndr_lib::context_runtime::RetrieveResult::default())
        .expect("result json");
    let keys = result
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    assert_eq!(
        keys,
        ["hits", "filters", "strong_match"].map(String::from).into()
    );
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
        ranking_config(),
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

#[test]
fn search_lists_what_retrieve_found_in_the_same_order() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = seeded_state(&runtime);

    for query in [
        "zephyr contract",
        "staging database",
        "team lunch",
        "budget",
    ] {
        let hits = runtime
            .block_on(retrieve(&state, &request(query)))
            .expect("retrieve")
            .hits;
        let results = runtime
            .block_on(search_ranked_results(&state, query, None, None, 10))
            .expect("search");
        assert_eq!(
            results
                .iter()
                .map(|result| result.id.as_str())
                .collect::<Vec<_>>(),
            hits.iter()
                .map(|hit| hit.memory_id.as_str())
                .collect::<Vec<_>>(),
            "{query}"
        );
        // Same evidence strength; recency counts whole minutes, so two calls
        // that straddle a minute differ by about 2e-5 here.
        for (result, hit) in results.iter().zip(&hits) {
            assert!((result.score - hit.score).abs() < 1e-3, "{query}");
        }
    }

    // Cards read the routes and the share of query words each row covers.
    let results = runtime
        .block_on(search_ranked_results(
            &state,
            "zephyr contract",
            None,
            None,
            10,
        ))
        .expect("search");
    let top = results.first().expect("a result");
    assert_eq!(top.id, "vendor");
    assert!(top.matched_routes.iter().any(|route| route == "keyword"));
    // Two of three anchors ("zephyr", "contract", not the phrase): computed
    // for this query, not the stored value.
    assert!(top.anchor_coverage_score > 0.5);
}

#[test]
fn retrieve_never_returns_fndr_own_windows() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = seeded_state(&runtime);
    let text = "Search results for the Zephyr vendor contract";
    let embedding = Embedder::new()
        .expect("embedder")
        .embed_batch(&[text.to_string()])
        .expect("embedding")
        .remove(0);
    let mut own = record(
        "own-window",
        "FNDR",
        "Search: zephyr contract",
        text,
        30_000,
        embedding,
    );
    own.bundle_id = Some("com.fndr.app".to_string());
    runtime
        .block_on(state.store.add_batch(&[own]))
        .expect("add own window");
    let stored = runtime
        .block_on(state.store.keyword_search("zephyr", 10, None, None))
        .expect("keyword search");
    assert!(stored.iter().any(|result| result.id == "own-window"));

    let hits = runtime
        .block_on(retrieve(&state, &request("zephyr contract")))
        .expect("retrieve")
        .hits;
    assert!(hits.iter().any(|hit| hit.memory_id == "vendor"));
    assert!(hits.iter().all(|hit| hit.memory_id != "own-window"));
    let results = runtime
        .block_on(search_ranked_results(
            &state,
            "zephyr contract",
            None,
            None,
            10,
        ))
        .expect("search");
    assert!(results.iter().all(|result| result.id != "own-window"));
}

/// Fourteen similar notes from the last six days: more memories fall in a
/// "recent" window than any route keeps, so ties decide what is shown.
fn busy_week_state(runtime: &tokio::runtime::Runtime) -> (tempfile::TempDir, AppState) {
    std::env::set_var("FNDR_ALLOW_MOCK_EMBEDDER", "1");
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Arc::new(Store::new(dir.path()).expect("store"));
    let state_store = Arc::new(StateStore::new(dir.path()).expect("state store"));
    let embedder = Embedder::new().expect("embedder");
    let topics = [
        "pricing page copy",
        "beta signup form",
        "release notes draft",
        "support macros",
        "onboarding email",
        "press kit",
        "partner webinar",
        "status page",
        "billing migration",
        "app store screenshots",
        "help center articles",
        "referral program",
        "launch retro agenda",
        "social posts",
    ];
    let texts = topics
        .iter()
        .map(|topic| format!("Launch notes: reviewed the {topic} for the spring launch"))
        .collect::<Vec<_>>();
    let embeddings = embedder.embed_batch(&texts).expect("embeddings");
    let records = topics
        .iter()
        .zip(texts.iter().zip(embeddings))
        .enumerate()
        .map(|(index, (topic, (text, embedding)))| {
            record(
                &format!("note-{index:02}"),
                "Notion",
                &format!("Launch notes: {topic}"),
                text,
                (index as i64 * 10 + 1) * 60 * 60 * 1000,
                embedding,
            )
        })
        .collect::<Vec<_>>();
    runtime
        .block_on(store.add_batch(&records))
        .expect("add records");
    let graph = GraphStore::new(store.clone());
    let state = AppState::new(
        dir.path().to_path_buf(),
        ranking_config(),
        store,
        state_store,
        graph,
        None,
        None,
    );
    (dir, state)
}

#[test]
fn the_same_query_returns_the_same_ids_every_time() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = busy_week_state(&runtime);
    let query = "recent launch notes";
    let run = || {
        let retrieved = runtime
            .block_on(retrieve(
                &state,
                &RetrieveRequest {
                    query: query.to_string(),
                    limit: 5,
                    ..Default::default()
                },
            ))
            .expect("retrieve")
            .hits
            .into_iter()
            .map(|hit| hit.memory_id)
            .collect::<Vec<_>>();
        let searched = runtime
            .block_on(search_ranked_results(&state, query, None, None, 5))
            .expect("search")
            .into_iter()
            .map(|result| result.id)
            .collect::<Vec<_>>();
        let asked = runtime
            .block_on(run_query(&state, query, 5, ComposeMode::Cards))
            .expect("run_query")
            .cards
            .into_iter()
            .map(|card| card.id)
            .collect::<Vec<_>>();
        (retrieved, searched, asked)
    };

    let first = run();
    assert_eq!(first.0.len(), 5);
    for attempt in 1..5 {
        assert_eq!(run(), first, "run {attempt}");
    }
}

#[test]
fn a_short_page_is_the_start_of_a_long_page() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let ids = |state: &AppState, query: &str, limit: usize| {
        runtime
            .block_on(retrieve(
                state,
                &RetrieveRequest {
                    query: query.to_string(),
                    limit,
                    ..Default::default()
                },
            ))
            .expect("retrieve")
            .hits
            .into_iter()
            .map(|hit| hit.memory_id)
            .collect::<Vec<_>>()
    };

    let (_week_dir, week) = busy_week_state(&runtime);
    let (_dir, seeded) = seeded_state(&runtime);
    for (state, query) in [
        (&week, "recent launch notes"),
        (&week, "launch notes from last week"),
        (&week, "the billing migration"),
        (&seeded, "zephyr contract"),
        (&seeded, "team lunch"),
    ] {
        let long = ids(state, query, 20);
        for limit in 1..=5 {
            let short = ids(state, query, limit);
            let expected = long.iter().take(limit).cloned().collect::<Vec<_>>();
            assert_eq!(short, expected, "{query}, limit {limit}");
        }
    }
}

#[test]
fn retrieve_says_when_nothing_matches_well() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = seeded_state(&runtime);

    let unrelated = runtime
        .block_on(retrieve(&state, &request("dentist appointment reminder")))
        .expect("retrieve");
    assert!(!unrelated.strong_match, "{:?}", unrelated.hits.first());

    let related = runtime
        .block_on(retrieve(&state, &request("zephyr vendor contract")))
        .expect("retrieve");
    let top = related.hits.first().expect("a hit");
    assert_eq!(top.memory_id, "vendor");
    // Strong by score with a model loaded; without one only the keyword
    // route scores, and containing every query word is what makes it strong.
    assert!(
        top.score >= STRONG_MATCH_SCORE || top.why.matched_terms.len() >= 3,
        "{top:?}"
    );
    assert!(related.strong_match);
}

#[test]
fn retrieve_names_the_chunk_that_matched() {
    use fndr_lib::inference::model_config::BGE_V5_DIMENSIONS;
    use fndr_lib::storage::MemoryChunkRecord;

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (dir, state) = seeded_state(&runtime);
    // The chunk route reads parents from the BGE table; write them with
    // placeholder vectors, as the v5 reindex would with real ones.
    let parents = ["vendor", "lunch"]
        .iter()
        .map(|id| {
            let mut parent = runtime
                .block_on(state.store.get_memory_by_id(id))
                .expect("lookup")
                .expect("stored");
            parent.embedding = vec![0.01; BGE_V5_DIMENSIONS];
            parent.snippet_embedding = vec![0.01; BGE_V5_DIMENSIONS];
            parent.support_embedding = vec![0.01; BGE_V5_DIMENSIONS];
            parent
        })
        .collect::<Vec<_>>();
    runtime
        .block_on(state.store.add_v5_batch_preserving_ids(&parents))
        .expect("v5 parents");
    let chunk = |id: &str, memory_id: &str, index: u32, text: &str| MemoryChunkRecord {
        id: id.to_string(),
        memory_id: memory_id.to_string(),
        chunk_index: index,
        line_kind: "plain".to_string(),
        text: text.to_string(),
        embedding: vec![0.01; BGE_V5_DIMENSIONS],
        created_at: 1_000,
        app_name: "Slack".to_string(),
        window_title: "Vendor thread".to_string(),
        day_bucket: "2026-10-01".to_string(),
        content_hash: format!("hash-{id}"),
    };
    runtime
        .block_on(state.store.upsert_memory_chunks(&[
            chunk("vendor-0", "vendor", 0, "Thread opened by procurement"),
            chunk(
                "vendor-1",
                "vendor",
                1,
                "The Zephyr vendor contract renews next quarter at the same price",
            ),
            chunk(
                "lunch-0",
                "lunch",
                0,
                "Ordered sandwiches for the team lunch",
            ),
        ]))
        .expect("chunks");
    let mut config = ranking_config();
    config.search.use_chunk_first_retrieval = true;
    let state = AppState::new(
        dir.path().to_path_buf(),
        config,
        state.store.clone(),
        state.state_store.clone(),
        GraphStore::new(state.store.clone()),
        None,
        None,
    );

    let hits = runtime
        .block_on(retrieve(&state, &request("zephyr contract")))
        .expect("retrieve")
        .hits;

    let top = hits.first().expect("a hit");
    assert_eq!(top.memory_id, "vendor");
    assert!(top.why.routes.iter().any(|route| route == "chunk"));
    // With the BGE model installed (the M1) the vector route also scores the
    // placeholder vectors, so either vendor chunk may win; without it BM25
    // picks vendor-1 (Linux cloud).
    let model_installed = fndr_lib::embedding::shared_bge_v5_query_embedder().is_ok();
    if !model_installed {
        assert_eq!(top.chunk_id.as_deref(), Some("vendor-1"));
        assert!(top
            .matched_text
            .as_deref()
            .is_some_and(|text| text.contains("Zephyr vendor contract")));
    }
    // Hits the chunk route did not find carry no chunk.
    for hit in hits
        .iter()
        .filter(|hit| !hit.why.routes.iter().any(|r| r == "chunk"))
    {
        assert!(hit.chunk_id.is_none() && hit.matched_text.is_none());
    }
    // Search rows carry the same chunk, so its cards can show the sentence.
    let rows = runtime
        .block_on(search_ranked_results(
            &state,
            "zephyr contract",
            None,
            None,
            10,
        ))
        .expect("search");
    if model_installed {
        assert!(rows[0].matched_chunk_ids[0].starts_with("vendor-"));
    } else {
        assert_eq!(rows[0].matched_chunk_ids, vec!["vendor-1"]);
    }
}

#[test]
fn retrieval_runs_no_graph_route_until_it_loads_the_graph() {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_dir, state) = seeded_state(&runtime);

    // A lookup query: the planner used to add the graph route for it, and
    // that route searched an empty in-memory graph (VS-33).
    let trace = runtime
        .block_on(run_query(&state, "zephyr contract", 10, ComposeMode::Cards))
        .expect("run_query")
        .debug_trace
        .expect("trace");
    let routes = trace["routes"]
        .as_array()
        .expect("routes")
        .iter()
        .map(|route| {
            (
                route["route"].as_str().unwrap_or_default().to_string(),
                route["top_candidates"].as_array().map_or(0, Vec::len),
            )
        })
        .collect::<Vec<_>>();
    assert!(
        routes.iter().any(|(name, _)| name == "Keyword"),
        "{routes:?}"
    );
    assert!(routes.iter().all(|(name, _)| name != "Graph"), "{routes:?}");
}
