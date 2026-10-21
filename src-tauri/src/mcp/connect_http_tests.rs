//! The Setup flow and recall tools through the real local HTTP router.
//! Every row and credential in these tests is synthetic.

use super::*;
use crate::config::Config;
use crate::graph::GraphStore;
use crate::storage::{MemoryRecord, StateStore, Store};

const TEST_TOKEN: &str = "synthetic-mcp-connect-token";

struct Server {
    endpoint: String,
    app_state: Arc<AppState>,
    client: reqwest::Client,
    task: tokio::task::JoinHandle<()>,
    _dir: tempfile::TempDir,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn serve() -> Server {
    let dir = tempfile::tempdir().expect("synthetic store directory");
    let data_dir = dir.path().to_path_buf();
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
    let mut config = Config::default();
    config.blocklist = vec!["blocked-app".into()];
    let app_state = Arc::new(AppState::new(
        data_dir,
        config,
        store,
        state_store,
        graph,
        None,
    ));
    let state = Arc::new(HttpState {
        app_state: app_state.clone(),
        app_handle: None,
        approvals: Arc::new(McpApprovalBroker::default()),
        token: TEST_TOKEN.into(),
        hermes_token: "synthetic-hermes-token".into(),
        mode: McpDeploymentMode::Local,
        require_auth: true,
        allow_loopback_auth_bypass: true,
        allowed_origins: Vec::new(),
        public_endpoint: None,
        public_sse_endpoint: None,
        sessions: Arc::new(Mutex::new(ClientSessions::default())),
        note_limiter: Arc::new(remember::RememberLimiter::new()),
        note_embedder: remember::NoteEmbedder::Given(None),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("loopback listener");
    let address = listener.local_addr().expect("listener address");
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            mcp_router(state).into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .expect("server");
    });
    Server {
        endpoint: format!("http://{address}/mcp"),
        app_state,
        client: reqwest::Client::new(),
        task,
        _dir: dir,
    }
}

impl Server {
    async fn call(&self, name: &str, arguments: Value) -> Value {
        let response = self
            .client
            .post(&self.endpoint)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("Authorization", format!("Bearer {TEST_TOKEN}"))
            .json(&json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": { "name": name, "arguments": arguments }
            }))
            .send()
            .await
            .expect("HTTP request");
        let status = response.status().as_u16();
        let body = response.json::<Value>().await.expect("JSON response");
        assert_eq!(status, 200, "{body}");
        assert_ne!(body["result"]["isError"], true, "{body}");
        body["result"]["structuredContent"].clone()
    }
}

fn memory(id: &str, source_type: &str, app: &str, title: &str, text: &str) -> MemoryRecord {
    MemoryRecord {
        id: id.into(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        source_type: source_type.into(),
        app_name: app.into(),
        window_title: title.into(),
        snippet: text.into(),
        clean_text: text.into(),
        text: text.into(),
        ..MemoryRecord::default()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn setup_connection_test_completes_a_real_http_handshake_and_refuses_missing_auth() {
    let server = serve().await;
    let report = connect::test_connection(&server.endpoint, TEST_TOKEN).await;
    assert!(report.ok, "{report:?}");
    assert_eq!(report.steps.len(), 4);
    assert!(report.tool_count >= 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recall_over_http_reports_origin_and_hides_blocked_memory() {
    let server = serve().await;
    let mut screen = memory(
        "screen-1",
        "screen",
        "Xcode",
        "Parser work",
        "Fixed the parser state machine.",
    );
    screen.files_touched = vec!["/tmp/synthetic/parser.rs".into()];
    let imported = memory(
        "mail-1",
        "import:mail",
        "Mail",
        "Release update",
        "Drafted the release update.",
    );
    let blocked = memory(
        "blocked-1",
        "screen",
        "blocked-app",
        "Private notes",
        "This must not leave FNDR.",
    );
    server
        .app_state
        .store
        .add_batch_preserving_ids(&[screen, imported, blocked])
        .await
        .expect("seed synthetic rows");

    let recent = server
        .call("fndr.recent_activity", json!({ "time_window": "24h" }))
        .await;
    let items = recent["items"].as_array().expect("recent items");
    assert_eq!(items.len(), 2, "{recent}");
    let screen = items
        .iter()
        .find(|item| item["memory_id"] == "screen-1")
        .expect("screen row");
    assert_eq!(screen["source_type"], "screen");
    assert_eq!(screen["app"], "Xcode");
    assert!(screen["timestamp_ms"].is_number());
    assert_eq!(screen["reopen"]["target"], "/tmp/synthetic/parser.rs");
    let imported = items
        .iter()
        .find(|item| item["memory_id"] == "mail-1")
        .expect("import row");
    assert_eq!(imported["source_type"], "import:mail");
    assert_eq!(imported["source_label"], "Imported mail");
    assert!(!recent.to_string().contains("blocked-1"));

    let sources = server
        .call("fndr.list_sources", json!({ "time_window": "24h" }))
        .await;
    assert_eq!(sources["total"], 2, "{sources}");
    assert!(!sources.to_string().contains("blocked-app"));

    let standup = server
        .call("fndr.standup", json!({ "period": "today" }))
        .await;
    let cited = standup["markdown"].as_str().expect("standup text");
    assert!(cited.contains("memory:screen-1"), "{standup}");
    assert!(cited.contains("memory:mail-1"), "{standup}");
    assert!(!cited.contains("blocked-1"));

    let search = server
        .call("fndr.search_memories", json!({ "query": "parser" }))
        .await;
    assert_eq!(search["items"][0]["memory_id"], "screen-1", "{search}");
    assert_eq!(search["items"][0]["source_type"], "screen");
    assert!(!search.to_string().contains("blocked-1"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hide_more_setting_changes_what_the_http_tools_return() {
    let server = serve().await;
    let ordinary = memory(
        "development-1",
        "screen",
        "Xcode",
        "Token budget",
        "Reviewed password manager tests and token budget.",
    );
    let secret = memory(
        "secret-1",
        "screen",
        "Xcode",
        "Build settings",
        "access_token: synthetic-credential-value",
    );
    server
        .app_state
        .store
        .add_batch_preserving_ids(&[ordinary, secret])
        .await
        .expect("seed synthetic rows");

    let default = server
        .call("fndr.recent_activity", json!({ "time_window": "24h" }))
        .await;
    assert!(default.to_string().contains("development-1"), "{default}");
    assert!(!default.to_string().contains("secret-1"), "{default}");

    server.app_state.config.write().mcp_strict_privacy = true;
    let strict = server
        .call("fndr.recent_activity", json!({ "time_window": "24h" }))
        .await;
    assert!(!strict.to_string().contains("development-1"), "{strict}");
    assert!(!strict.to_string().contains("secret-1"), "{strict}");
    assert_eq!(strict["withheld_for_privacy"], 2, "{strict}");
}

/// Run explicitly at integration with the official MCP client. Its stdio
/// transport launches the same mcp-remote bridge Claude Desktop uses.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "installs the real MCP client in a synthetic temporary home"]
async fn real_mcp_remote_client_calls_a_recall_tool() {
    use tokio::process::Command;
    use tokio::time::{timeout, Duration};

    let server = serve().await;
    server
        .app_state
        .store
        .add_batch_preserving_ids(&[memory(
            "bridge-1",
            "screen",
            "Xcode",
            "Parser work",
            "Fixed the parser state machine.",
        )])
        .await
        .expect("seed synthetic row");
    let home = tempfile::tempdir().expect("isolated client home");
    let path = format!(
        "/usr/local/bin:{}",
        std::env::var("PATH").unwrap_or_default()
    );
    let install = timeout(
        Duration::from_secs(120),
        Command::new("/usr/local/bin/npm")
            .args([
                "install",
                "--silent",
                "--no-audit",
                "--no-fund",
                "@modelcontextprotocol/client@2.3.1",
            ])
            .current_dir(home.path())
            .env("HOME", home.path())
            .env("npm_config_cache", home.path().join("npm-cache"))
            .env("PATH", &path)
            .output(),
    )
    .await
    .expect("official MCP client install within two minutes")
    .expect("install official MCP client");
    assert!(
        install.status.success(),
        "{}",
        String::from_utf8_lossy(&install.stderr)
    );

    let script = home.path().join("call.mjs");
    std::fs::write(
        &script,
        r#"import { Client } from '@modelcontextprotocol/client';
import { StdioClientTransport } from '@modelcontextprotocol/client/stdio';
const transport = new StdioClientTransport({
  command: '/usr/local/bin/npx',
  args: ['-y', 'mcp-remote', process.env.FNDR_TEST_ENDPOINT, '--allow-http', '--header', `Authorization:Bearer ${process.env.FNDR_TEST_TOKEN}`],
  env: { ...process.env, HOME: process.env.FNDR_TEST_HOME },
});
const client = new Client({ name: 'fndr-synthetic-client', version: '1.0.0' });
try {
  await client.connect(transport);
  const result = await client.callTool({ name: 'fndr.recent_activity', arguments: { time_window: '24h' } });
  console.log(JSON.stringify({ server: client.getServerVersion()?.name, result }));
} finally {
  await client.close();
}
"#,
    )
    .expect("write synthetic client script");
    let called = timeout(
        Duration::from_secs(60),
        Command::new("/usr/local/bin/node")
            .arg(&script)
            .current_dir(home.path())
            .env("HOME", home.path())
            .env("npm_config_cache", home.path().join("npm-cache"))
            .env("PATH", &path)
            .env("FNDR_TEST_HOME", home.path())
            .env("FNDR_TEST_ENDPOINT", &server.endpoint)
            .env("FNDR_TEST_TOKEN", TEST_TOKEN)
            .output(),
    )
    .await
    .expect("real client call within one minute")
    .expect("run real client");
    assert!(
        called.status.success(),
        "{}",
        String::from_utf8_lossy(&called.stderr)
    );
    let answer: Value = serde_json::from_slice(&called.stdout).expect("client JSON response");
    assert_eq!(answer["server"], "FNDR", "{answer}");
    assert_eq!(
        answer["result"]["structuredContent"]["items"][0]["memory_id"], "bridge-1",
        "{answer}"
    );
}
