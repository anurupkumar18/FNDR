//! VS-68: `fndr.remember` over real HTTP, and the injected-note corpus
//! (`tests/fixtures/agent_notes/injected-notes.json`, spec section 13).
//!
//! Each test serves `mcp_router` on an ephemeral loopback port with its own
//! token, a fake limiter clock, and a mock embedder, so nothing depends on
//! the model files or on `~/.fndr`.

use super::remember::{NoteEmbedder, RememberLimiter};
use super::*;
use crate::config::Config;
use crate::graph::GraphStore;
use crate::privacy::safety_gate::{self, SafetyDecision};
use crate::storage::{DecisionLedgerEntry, MemoryRecord, StateStore, Store};
use std::sync::atomic::{AtomicI64, Ordering};

const TOKEN: &str = "vs68-test-token-0123456789";
const DAY_MS: i64 = 86_400_000;

struct Server {
    endpoint: String,
    app_state: Arc<AppState>,
    clock: Arc<AtomicI64>,
    client: reqwest::Client,
}

async fn app_state_with(config: Config) -> Arc<AppState> {
    let dir = tempfile::tempdir().expect("tempdir");
    let data_dir = dir.path().to_path_buf();
    std::mem::forget(dir);
    let store_dir = data_dir.clone();
    let store =
        tokio::task::spawn_blocking(move || Arc::new(Store::new(&store_dir).expect("store")))
            .await
            .expect("store task");
    let state_dir = data_dir.clone();
    let state_store =
        tokio::task::spawn_blocking(move || Arc::new(StateStore::new(&state_dir).expect("state")))
            .await
            .expect("state task");
    let graph = GraphStore::new(store.clone());
    Arc::new(AppState::new(
        data_dir,
        config,
        store,
        state_store,
        graph,
        None,
    ))
}

fn notes_on() -> Config {
    let mut config = Config::default();
    config.agent_notes_enabled = true;
    config.blocklist = Vec::new();
    config
}

async fn serve(app_state: Arc<AppState>, require_auth: bool, embedder: NoteEmbedder) -> Server {
    let clock = Arc::new(AtomicI64::new(1_800_000_000_000));
    let limiter_clock = clock.clone();
    let state = Arc::new(HttpState {
        app_state: app_state.clone(),
        app_handle: None,
        approvals: Arc::new(McpApprovalBroker::default()),
        token: TOKEN.to_string(),
        hermes_token: "test-hermes-read-token".to_string(),
        mode: McpDeploymentMode::Local,
        require_auth,
        allow_loopback_auth_bypass: true,
        allowed_origins: Vec::new(),
        public_endpoint: None,
        public_sse_endpoint: None,
        sessions: Arc::new(Mutex::new(ClientSessions::default())),
        note_limiter: Arc::new(RememberLimiter::with_clock(move || {
            limiter_clock.load(Ordering::SeqCst)
        })),
        note_embedder: embedder,
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let address = listener.local_addr().expect("address");
    tokio::spawn(async move {
        axum::serve(
            listener,
            mcp_router(state).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .expect("serve");
    });
    Server {
        endpoint: format!("http://{address}/mcp"),
        app_state,
        clock,
        client: reqwest::Client::new(),
    }
}

fn mock_embedder() -> NoteEmbedder {
    NoteEmbedder::Given(Some(Arc::new(crate::embedding::Embedder::mock_for_tests())))
}

impl Server {
    async fn post(
        &self,
        token: Option<&str>,
        session: Option<&str>,
        body: Value,
    ) -> (u16, Value, Option<String>) {
        let mut request = self
            .client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(token) = token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }
        if let Some(session) = session {
            request = request.header(MCP_SESSION_HEADER, session);
        }
        let response = request.send().await.expect("request");
        let status = response.status().as_u16();
        let session = response
            .headers()
            .get(MCP_SESSION_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = response.json::<Value>().await.unwrap_or(Value::Null);
        (status, body, session)
    }

    async fn initialize(&self, client_name: Option<&str>) -> String {
        let mut params = json!({ "protocolVersion": "2025-03-26", "capabilities": {} });
        if let Some(name) = client_name {
            params["clientInfo"] = json!({ "name": name, "version": "1.0" });
        }
        let (status, _, session) = self
            .post(
                None,
                None,
                json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": params }),
            )
            .await;
        assert_eq!(status, 200);
        session.expect("initialize returns Mcp-Session-Id")
    }

    async fn remember(
        &self,
        token: Option<&str>,
        session: Option<&str>,
        args: Value,
    ) -> (u16, Value) {
        let (status, body, _) = self.post(token, session, remember_call(2, args)).await;
        (status, body)
    }

    async fn rows(&self) -> Vec<MemoryRecord> {
        let mut rows = self
            .app_state
            .store
            .get_memories_in_range(0, i64::MAX)
            .await
            .expect("rows");
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        rows
    }

    async fn decisions(&self) -> Vec<DecisionLedgerEntry> {
        self.app_state
            .store
            .list_decision_ledger_entries(100, None)
            .await
            .expect("decision ledger")
    }
}

fn remember_call(id: i64, args: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": "fndr.remember", "arguments": args }
    })
}

fn decision_call(id: i64, args: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": { "name": "fndr_remember_decision", "arguments": args }
    })
}

/// The tool-level outcome: `Ok(structuredContent)` when stored, `Err(code)`
/// when refused (a transport refusal reads as "unauthorized").
fn outcome(status: u16, body: &Value) -> Result<Value, String> {
    if status == 401 {
        let error = if body.is_array() { &body[0] } else { body };
        assert_eq!(error["error"]["code"], -32001, "{body}");
        return Err("unauthorized".to_string());
    }
    assert_eq!(status, 200, "{body}");
    let result = &body["result"];
    if result["isError"] == true {
        return Err(result["structuredContent"]["error"]
            .as_str()
            .unwrap_or("untyped_error")
            .to_string());
    }
    Ok(result["structuredContent"].clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remember_rejects_without_token() {
    let server = serve(app_state_with(notes_on()).await, true, mock_embedder()).await;
    let session = server.initialize(Some("Claude Code")).await;
    let (status, body) = server
        .remember(
            None,
            Some(&session),
            json!({ "kind": "note", "text": "hello" }),
        )
        .await;
    assert_eq!(outcome(status, &body), Err("unauthorized".to_string()));
    let (status, body) = server
        .remember(
            Some("wrong-token-wrong-token"),
            Some(&session),
            json!({ "kind": "note", "text": "hello" }),
        )
        .await;
    assert_eq!(outcome(status, &body), Err("unauthorized".to_string()));
    assert!(server.rows().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remember_in_batch_after_handshake_requires_token() {
    let server = serve(app_state_with(notes_on()).await, true, mock_embedder()).await;
    let batch = json!([
        { "jsonrpc": "2.0", "id": 1, "method": "initialize",
          "params": { "protocolVersion": "2025-03-26", "clientInfo": { "name": "Claude Code" } } },
        remember_call(2, json!({ "kind": "note", "text": "behind a handshake" }))
    ]);
    let (status, body, _) = server.post(None, None, batch).await;
    assert_eq!(status, 401, "{body}");
    assert!(server.rows().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remember_refused_when_auth_disabled() {
    let server = serve(app_state_with(notes_on()).await, false, mock_embedder()).await;
    let session = server.initialize(Some("Claude Code")).await;
    for token in [None, Some(TOKEN)] {
        let (status, body) = server
            .remember(
                token,
                Some(&session),
                json!({ "kind": "note", "text": "hello" }),
            )
            .await;
        assert_eq!(
            outcome(status, &body),
            Err("auth_required_for_writes".to_string())
        );
    }
    assert!(server.rows().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remember_refused_when_notes_disabled_or_kill_switch_on() {
    let mut config = notes_on();
    config.agent_notes_enabled = false;
    let server = serve(app_state_with(config).await, true, mock_embedder()).await;
    let session = server.initialize(Some("Claude Code")).await;
    let args = json!({ "kind": "note", "text": "hello", "not_a_field": true });
    let (status, body) = server
        .remember(Some(TOKEN), Some(&session), args.clone())
        .await;
    // The gate answers before any argument is read.
    assert_eq!(outcome(status, &body), Err("notes_disabled".to_string()));
    server.app_state.config.write().actions_kill_switch = true;
    server.app_state.config.write().agent_notes_enabled = true;
    let (status, body) = server.remember(Some(TOKEN), Some(&session), args).await;
    assert_eq!(outcome(status, &body), Err("actions_off".to_string()));
    assert!(server.rows().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remember_decision_rejects_missing_or_wrong_token() {
    let server = serve(app_state_with(notes_on()).await, true, mock_embedder()).await;
    let session = server.initialize(Some("Claude Code")).await;
    for token in [None, Some("wrong-token-wrong-token")] {
        let (status, body, _) = server
            .post(
                token,
                Some(&session),
                decision_call(2, json!({ "title": "Keep the parser local" })),
            )
            .await;
        assert_eq!(outcome(status, &body), Err("unauthorized".to_string()));
        assert!(server.decisions().await.is_empty());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remember_decision_in_batch_after_handshake_requires_token() {
    let server = serve(app_state_with(notes_on()).await, true, mock_embedder()).await;
    let batch = json!([
        { "jsonrpc": "2.0", "id": 1, "method": "initialize",
          "params": { "protocolVersion": "2025-03-26", "clientInfo": { "name": "Claude Code" } } },
        decision_call(2, json!({ "title": "Keep the parser local" }))
    ]);
    let (status, body, _) = server.post(None, None, batch).await;
    assert_eq!(outcome(status, &body), Err("unauthorized".to_string()));
    assert!(server.decisions().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remember_decision_refused_when_auth_disabled() {
    let server = serve(app_state_with(notes_on()).await, false, mock_embedder()).await;
    for token in [None, Some("wrong-token-wrong-token"), Some(TOKEN)] {
        let (status, body, _) = server
            .post(
                token,
                None,
                decision_call(2, json!({ "title": "Keep the parser local" })),
            )
            .await;
        assert_eq!(
            outcome(status, &body),
            Err("auth_required_for_writes".to_string())
        );
        assert!(server.decisions().await.is_empty());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remember_decision_checks_settings_before_parsing_arguments() {
    let server = serve(app_state_with(notes_on()).await, true, mock_embedder()).await;
    for (kill_switch, notes_enabled, expected) in [
        (false, false, "notes_disabled"),
        (true, false, "actions_off"),
        (true, true, "actions_off"),
    ] {
        {
            let mut config = server.app_state.config.write();
            config.actions_kill_switch = kill_switch;
            config.agent_notes_enabled = notes_enabled;
        }
        for args in [json!({ "title": "Keep the parser local" }), Value::Null] {
            let (status, body, _) = server.post(Some(TOKEN), None, decision_call(2, args)).await;
            assert_eq!(outcome(status, &body), Err(expected.to_string()));
            assert!(server.decisions().await.is_empty());
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remember_decision_with_token_and_notes_enabled_persists_to_ledger() {
    let server = serve(app_state_with(notes_on()).await, true, mock_embedder()).await;
    let (status, body, _) = server
        .post(
            Some(TOKEN),
            None,
            decision_call(
                2,
                json!({
                    "title": "Keep the parser local",
                    "summary": "Use local parsing for predictable offline behavior."
                }),
            ),
        )
        .await;
    let result = outcome(status, &body).expect("decision stored");
    let decisions = server.decisions().await;
    assert_eq!(decisions.len(), 1);
    assert_eq!(result["decision"]["id"], decisions[0].id);
    assert_eq!(decisions[0].title, "Keep the parser local");
    assert_eq!(
        decisions[0].summary,
        "Use local parsing for predictable offline behavior."
    );
    assert_eq!(decisions[0].status, "proposed");
    assert!(server.rows().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remember_fails_closed_when_embedder_unavailable() {
    let server = serve(
        app_state_with(notes_on()).await,
        true,
        NoteEmbedder::Given(None),
    )
    .await;
    let session = server.initialize(Some("Claude Code")).await;
    let (status, body) = server
        .remember(
            Some(TOKEN),
            Some(&session),
            json!({ "kind": "note", "text": "hello" }),
        )
        .await;
    assert_eq!(
        outcome(status, &body),
        Err("embedder_unavailable".to_string())
    );
    assert!(server.rows().await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remember_refuses_sensitive_project_without_storing_or_echoing_it() {
    let server = serve(app_state_with(notes_on()).await, true, mock_embedder()).await;
    let project = "password: synthetic-project-secret";
    let (status, body) = server
        .remember(
            Some(TOKEN),
            None,
            json!({"kind":"note", "text":"Reviewed the parser tests.", "project":project}),
        )
        .await;
    assert_eq!(outcome(status, &body), Err("sensitive_content".to_string()));
    assert!(server.rows().await.is_empty());
    assert!(!body.to_string().contains(project));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remember_refuses_blocklisted_project_without_storing_or_echoing_it() {
    let mut config = notes_on();
    config.blocklist = vec!["blocked-project.example".to_string()];
    let server = serve(app_state_with(config).await, true, mock_embedder()).await;
    let project = "Work on blocked-project.example";
    let (status, body) = server
        .remember(
            Some(TOKEN),
            None,
            json!({"kind":"note", "text":"Reviewed the parser tests.", "project":project}),
        )
        .await;
    assert_eq!(
        outcome(status, &body),
        Err("blocklisted_content".to_string())
    );
    assert!(server.rows().await.is_empty());
    assert!(!body.to_string().contains(project));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remember_stores_agent_provenance_and_the_note_is_findable() {
    // Route budgets lifted: under the parallel lib tests a production
    // keyword budget can drop a hit (see build_seeded_search_state).
    let mut config = notes_on();
    config.search.semantic_timeout_ms = 10_000;
    config.search.snippet_timeout_ms = 10_000;
    config.search.keyword_timeout_ms = 10_000;
    config.search.keyword_variant_timeout_ms = 5_000;
    let server = serve(app_state_with(config).await, true, mock_embedder()).await;
    let session = server.initialize(Some("Claude Code")).await;
    let text =
        "Decided to keep the quillfeather parser behind its flag. Revisit after the gate run.";
    let (status, body) = server
        .remember(
            Some(TOKEN),
            Some(&session),
            json!({ "kind": "decision", "text": format!("  {text}\n"), "project": "FNDR" }),
        )
        .await;
    let stored = outcome(status, &body).expect("stored");
    assert_eq!(stored["status"], "stored");
    assert_eq!(stored["source_type"], "agent");
    assert_eq!(stored["added_by"], "Claude Code");
    assert_eq!(stored["kind"], "decision");
    assert_eq!(stored["remaining"]["this_minute"], 9);
    let id = stored["memory_id"].as_str().expect("id").to_string();

    let row = server
        .app_state
        .store
        .get_memory_by_id(&id)
        .await
        .expect("read")
        .expect("row");
    assert_eq!(row.source_type, "agent");
    assert_eq!(row.app_name, "Agent note");
    assert_eq!(row.window_title, "Decision from Claude Code");
    assert_eq!(row.text, text);
    assert_eq!(row.clean_text, text);
    assert_eq!(
        row.snippet,
        "Decided to keep the quillfeather parser behind its flag."
    );
    assert_eq!(row.related_agents, ["Claude Code"]);
    assert_eq!(row.related_tools, ["fndr.remember"]);
    assert_eq!(row.storage_outcome, "agent_note");
    assert_eq!(row.enrichment_status, "agent_note");
    assert_eq!(row.project, "FNDR");
    assert_eq!(row.session_key, format!("agent_note:{id}"));
    assert_eq!(row.url, None);
    assert_eq!(row.reopen_kind, crate::memory::reopen::ReopenKind::Unknown);
    assert!(row.tags.contains(&"agent_note".to_string()));
    let evidence: Value = serde_json::from_str(&row.raw_evidence).expect("raw evidence json");
    assert_eq!(evidence["agent_provenance"]["client"], "Claude Code");
    assert_eq!(
        evidence["agent_provenance"]["client_name_source"],
        "mcp_initialize"
    );
    assert!(evidence.get("embedding_manifest").is_some(), "{evidence}");
    assert!(!crate::memory_compaction::is_low_signal_embedding(
        &row.embedding
    ));
    // The handler never loads a model.
    assert!(server.app_state.inference_engine().is_none());

    let found = crate::ipc::commands::search::search_ranked_results(
        &server.app_state,
        "quillfeather parser",
        None,
        None,
        10,
    )
    .await
    .expect("search");
    assert!(found.iter().any(|hit| hit.id == id), "the note is findable");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn remember_client_name_comes_from_session_not_arguments() {
    let server = serve(app_state_with(notes_on()).await, true, mock_embedder()).await;
    // No session: the name is unknown, whatever the arguments claim.
    let (status, body) = server
        .remember(
            Some(TOKEN),
            None,
            json!({ "kind": "note", "text": "from nowhere" }),
        )
        .await;
    assert_eq!(
        outcome(status, &body).expect("stored")["added_by"],
        "Unknown client"
    );
    let (status, body) = server
        .remember(
            Some(TOKEN),
            None,
            json!({ "kind": "note", "text": "x", "client": "Claude Code" }),
        )
        .await;
    assert_eq!(outcome(status, &body), Err("unknown_argument".to_string()));
    // A session whose initialize named no client.
    let session = server.initialize(None).await;
    let (status, body) = server
        .remember(
            Some(TOKEN),
            Some(&session),
            json!({ "kind": "note", "text": "nameless" }),
        )
        .await;
    assert_eq!(
        outcome(status, &body).expect("stored")["added_by"],
        "Unknown client"
    );
}

/// Every invariant the corpus names that this slice can change.
#[derive(Debug, PartialEq)]
struct Invariants {
    config: Value,
    tools: Value,
    risk: Vec<String>,
    derived: [usize; 7],
}

async fn invariants(app_state: &AppState) -> Invariants {
    use crate::agent::risk_policy::{self, Caller};
    let registry = crate::agent::tools::october_registry();
    let mut risk = Vec::new();
    for kill_switch in [false, true] {
        for spec in registry.specs() {
            for caller in [Caller::CommandBar, Caller::Voice, Caller::Mcp] {
                risk.push(format!(
                    "{} {caller:?} {kill_switch} {:?}",
                    spec.name,
                    risk_policy::decide_for_tool(&registry, spec.name, caller, kill_switch)
                ));
            }
        }
    }
    risk.push(format!("{:?}", risk_policy::mcp_side_effect_tools()));
    risk.push(format!("{:?}", risk_policy::mcp_write_tools()));
    let store = &app_state.store;
    let graph = crate::graph::graph_store::GraphStore::new(store.clone());
    Invariants {
        config: serde_json::to_value(&*app_state.config.read()).expect("config"),
        tools: tools_list_result(),
        risk,
        derived: [
            graph.all_nodes().await.expect("nodes").len(),
            graph.all_edges().await.expect("edges").len(),
            store.count_activity_events().await.expect("events"),
            store.count_decision_entries().await.expect("ledger"),
            store.count_context_packs().await.expect("packs"),
            store
                .list_knowledge_pages(1_000, None, None)
                .await
                .expect("pages")
                .len(),
            store.list_tasks().await.expect("tasks").len(),
        ],
    }
}

fn corpus() -> Value {
    serde_json::from_str(include_str!(
        "../../tests/fixtures/agent_notes/injected-notes.json"
    ))
    .expect("corpus json")
}

async fn seed(server: &Server, fixture: &Value) {
    let seeds = fixture["seed_memories"]
        .as_array()
        .expect("seeds")
        .iter()
        .map(|seed| {
            let text = seed["text"].as_str().unwrap_or_default().to_string();
            MemoryRecord {
                id: seed["id"].as_str().unwrap_or_default().to_string(),
                timestamp: 1_790_000_000_000,
                source_type: seed["source_type"].as_str().unwrap_or("screen").to_string(),
                app_name: seed["app_name"].as_str().unwrap_or_default().to_string(),
                window_title: seed["window_title"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                url: seed["url"].as_str().map(str::to_string),
                snippet: text.clone(),
                clean_text: text.clone(),
                text,
                ..MemoryRecord::default()
            }
        })
        .collect::<Vec<_>>();
    server
        .app_state
        .store
        .add_batch_preserving_ids(&seeds)
        .await
        .expect("seed");
}

fn detector_flags(title: &str, text: &str) -> bool {
    safety_gate::evaluate(
        None,
        None,
        None,
        None,
        Some(&format!("{title}\n{text}")),
        &[],
    ) != SafetyDecision::Allow
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn corpus_preserves_all_invariants() {
    let corpus = corpus();
    let fixture = &corpus["fixture"];
    let limits = &fixture["limits"];
    assert_eq!(limits["per_client_per_minute"], 10);
    assert_eq!(limits["global_per_day"], 500);
    assert_eq!(fixture["settings"]["agent_notes_enabled"], true);
    assert_eq!(fixture["settings"]["blocklist"], json!([]));

    let server = serve(app_state_with(notes_on()).await, true, mock_embedder()).await;
    seed(&server, fixture).await;
    let seeds_before = server.rows().await;
    let before = invariants(&server.app_state).await;
    let mut expected_new_rows = 0;

    for case in corpus["cases"].as_array().expect("cases") {
        let id = case["id"].as_str().expect("id");
        let expected = &case["expected"];
        let rows_before = server.rows().await.len();
        let mut args = json!({ "kind": case["kind"], "text": case["text"] });
        if let Some(extra) = case["args"].as_object() {
            for (key, value) in extra {
                args[key] = value.clone();
            }
        }
        let token = (case["auth"].as_str() != Some("none")).then_some(TOKEN);

        if let Some(burst) = case.get("burst") {
            // A fresh day, so earlier cases do not count against the limits.
            let start = server.clock.load(Ordering::SeqCst) + 2 * DAY_MS;
            let calls = burst["calls"].as_i64().expect("calls");
            let window_ms = burst["window_seconds"].as_i64().expect("window") * 1_000;
            let clients = burst["clients"].as_array().expect("clients");
            let mut sessions = HashMap::new();
            for client in clients {
                let name = client.as_str().expect("client");
                sessions.insert(name, server.initialize(Some(name)).await);
            }
            let mut stored = 0;
            let mut refused = None;
            for call in 0..calls {
                server
                    .clock
                    .store(start + call * window_ms / calls, Ordering::SeqCst);
                let client = clients[call as usize % clients.len()].as_str().unwrap();
                let mut call_args = args.clone();
                if burst["vary_text"] == true {
                    call_args["text"] =
                        json!(format!("{} #{}", case["text"].as_str().unwrap(), call + 1));
                }
                let (status, body) = server
                    .remember(token, Some(&sessions[client]), call_args)
                    .await;
                match outcome(status, &body) {
                    Ok(_) => stored += 1,
                    Err(code) => {
                        refused = Some((code, body["result"]["structuredContent"].clone()));
                        break;
                    }
                }
            }
            let (code, details) = refused.unwrap_or_else(|| panic!("{id}: nothing refused"));
            assert_eq!(code, expected["error"].as_str().unwrap(), "{id}");
            assert_eq!(
                stored,
                expected["accepted_before_limit"].as_i64().unwrap(),
                "{id}"
            );
            assert!(
                details["retry_after_seconds"].as_u64().unwrap_or(0)
                    >= expected["retry_after_seconds_min"].as_u64().unwrap(),
                "{id}: {details}"
            );
            expected_new_rows += stored as usize;
            assert_eq!(
                server.rows().await.len(),
                rows_before + stored as usize,
                "{id}"
            );
            continue;
        }

        // Single calls 61 seconds apart, so no minute window fills.
        server.clock.store(
            server.clock.load(Ordering::SeqCst).max(1_800_000_000_000) + 61_000,
            Ordering::SeqCst,
        );
        let client = case["client"].as_str().expect("client");
        let (status, body) = if case["transport"].as_str() == Some("batch_after_initialize") {
            let batch = json!([
                { "jsonrpc": "2.0", "id": 1, "method": "initialize",
                  "params": { "protocolVersion": "2025-03-26", "clientInfo": { "name": client } } },
                remember_call(2, args.clone())
            ]);
            let (status, body, _) = server.post(token, None, batch).await;
            (status, body)
        } else {
            let session = server.initialize(Some(client)).await;
            server.remember(token, Some(&session), args.clone()).await
        };
        let result = outcome(status, &body);
        let title = case["args"]["title"].as_str().unwrap_or_default();
        let text = case["text"].as_str().unwrap();
        if expected["accepted"] == true {
            let stored = result.unwrap_or_else(|code| panic!("{id} refused: {code}"));
            assert_eq!(stored["source_type"], "agent", "{id}");
            if let Some(client) = expected["stored_client"].as_str() {
                assert_eq!(stored["added_by"], client, "{id}");
            }
            let row = server
                .app_state
                .store
                .get_memory_by_id(stored["memory_id"].as_str().unwrap())
                .await
                .expect("read")
                .expect("row");
            // Stored inert: byte-identical, labeled, no open target.
            assert_eq!(row.text, text, "{id}");
            assert_eq!(row.source_type, "agent", "{id}");
            assert_eq!(row.app_name, "Agent note", "{id}");
            assert_eq!(row.url, None, "{id}");
            assert!(
                !detector_flags(title, text),
                "{id}: stored but the detector flags it"
            );
            expected_new_rows += 1;
            assert_eq!(server.rows().await.len(), rows_before + 1, "{id}");
        } else {
            let code = result.err().unwrap_or_else(|| panic!("{id} was stored"));
            assert_eq!(code, expected["error"].as_str().unwrap(), "{id}");
            if code == "sensitive_content" {
                assert!(detector_flags(title, text), "{id}");
            }
            assert!(
                !body.to_string().contains(text),
                "{id}: the refusal echoes the note"
            );
            assert_eq!(server.rows().await.len(), rows_before, "{id}");
        }
    }

    let after = server.rows().await;
    assert_eq!(after.len(), seeds_before.len() + expected_new_rows);
    for seed_row in &seeds_before {
        let now = after
            .iter()
            .find(|row| row.id == seed_row.id)
            .expect("seed kept");
        assert_eq!(
            serde_json::to_value(now).unwrap(),
            serde_json::to_value(seed_row).unwrap(),
            "seed {} changed",
            seed_row.id
        );
    }
    assert_eq!(invariants(&server.app_state).await, before);
}
