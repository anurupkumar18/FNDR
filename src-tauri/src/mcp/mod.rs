//! Robust MCP server for FNDR — HTTPS JSON-RPC 2.0 + SSE transport.
//!
//! Features:
//!  - Self-signed TLS (HTTPS) via `axum-server` + `rcgen`
//!  - Binds to `0.0.0.0:0` (OS-assigned port) for LAN accessibility
//!  - Writes `~/.fndr/mcp.json` for client discovery
//!  - Bearer-token authentication on all MCP endpoints
//!  - CORS layer so mobile/web clients can connect
//!  - SSE endpoint (`GET /mcp/sse`) for the official MCP streaming transport
//!  - `spawn_blocking` for SQLite + embedding calls
//!  - 30-second timeout on LLM inference

pub mod tls;
pub mod token;

use crate::context_runtime::{self, CodeContextRequest, ContextRequest, DecisionProposal};
use crate::embed::Embedder;
use crate::meeting;
use crate::search::HybridSearcher;
use crate::AppState;
use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    routing::{get, post},
    Json, Router,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::ReceiverStream;
use tower_http::cors::{Any, CorsLayer};

// ---------------------------------------------------------------------------
// Public status type (returned to Tauri frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerStatus {
    pub running: bool,
    pub host: String,
    pub port: u16,
    pub endpoint: String,
    pub token: String,
    pub last_error: Option<String>,
}

// ---------------------------------------------------------------------------
// Internal runtime state
// ---------------------------------------------------------------------------

struct McpRuntime {
    running: bool,
    host: String,
    port: u16,
    endpoint: String,
    token: String,
    shutdown: Option<oneshot::Sender<()>>,
    server_handle: Option<axum_server::Handle>,
    task: Option<JoinHandle<()>>,
    last_error: Option<String>,
}

impl Default for McpRuntime {
    fn default() -> Self {
        Self {
            running: false,
            host: "0.0.0.0".to_string(),
            port: 0,
            endpoint: String::new(),
            token: String::new(),
            shutdown: None,
            server_handle: None,
            task: None,
            last_error: None,
        }
    }
}

#[derive(Clone)]
struct HttpState {
    app_state: Arc<AppState>,
    token: String,
}

// ---------------------------------------------------------------------------
// JSON-RPC types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
    #[serde(default)]
    jsonrpc: Option<String>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct ToolCallParams {
    name: String,
    #[serde(default)]
    arguments: Value,
}

#[derive(Debug, Deserialize)]
struct SearchMemoriesArgs {
    query: String,
    #[serde(default)]
    time_filter: Option<String>,
    #[serde(default)]
    app_filter: Option<String>,
    #[serde(default = "default_search_limit")]
    limit: usize,
}

#[derive(Debug, Deserialize)]
struct AskFndrArgs {
    query: String,
}

#[derive(Debug, Deserialize)]
struct StartMeetingArgs {
    title: String,
    #[serde(default)]
    participants: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct GetMeetingTranscriptArgs {
    meeting_id: String,
}

#[derive(Debug, Deserialize)]
struct SearchMeetingTranscriptsArgs {
    query: String,
    #[serde(default = "default_search_limit")]
    limit: usize,
}

#[derive(Debug, Deserialize)]
struct GetAmbientContextArgs {
    #[serde(default = "default_ambient_limit")]
    limit: usize,
}

#[derive(Debug, Deserialize, Default)]
struct FndrDiffArgs {
    session_id: String,
    #[serde(default)]
    since_timestamp: Option<i64>,
}

fn default_ambient_limit() -> usize {
    5
}

fn default_search_limit() -> usize {
    10
}

// ---------------------------------------------------------------------------
// Global singleton runtime
// ---------------------------------------------------------------------------

static MCP_RUNTIME: OnceLock<Mutex<McpRuntime>> = OnceLock::new();

fn runtime() -> &'static Mutex<McpRuntime> {
    MCP_RUNTIME.get_or_init(|| Mutex::new(McpRuntime::default()))
}

fn to_status(rt: &McpRuntime) -> McpServerStatus {
    McpServerStatus {
        running: rt.running,
        host: rt.host.clone(),
        port: rt.port,
        endpoint: rt.endpoint.clone(),
        token: rt.token.clone(),
        last_error: rt.last_error.clone(),
    }
}

// ---------------------------------------------------------------------------
// Discovery file
// ---------------------------------------------------------------------------

fn discovery_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".fndr")
        .join("mcp.json")
}

fn write_discovery(host: &str, port: u16, token: &str) {
    let path = discovery_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let connect_host = if host == "0.0.0.0" { "127.0.0.1" } else { host };
    let endpoint = format!("https://{}:{}/mcp", connect_host, port);
    let cert_pem = tls::get_cert_pem();
    let payload = json!({
        "host": connect_host,
        "bind_host": host,
        "port": port,
        "token": token,
        "endpoint": endpoint,
        "sse_endpoint": format!("https://{}:{}/mcp/sse", connect_host, port),
        "tls": true,
        "cert_pem": cert_pem
    });
    match std::fs::write(
        &path,
        serde_json::to_string_pretty(&payload).unwrap_or_default(),
    ) {
        Ok(_) => tracing::info!("MCP discovery file written to {:?}", path),
        Err(e) => tracing::warn!("Failed to write MCP discovery file: {}", e),
    }
}

fn remove_discovery() {
    let _ = std::fs::remove_file(discovery_path());
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn status() -> McpServerStatus {
    let mut rt = runtime().lock();

    if rt.running {
        if let Some(task) = rt.task.as_ref() {
            if task.is_finished() {
                rt.running = false;
                rt.shutdown = None;
                rt.task = None;
                if rt.last_error.is_none() {
                    rt.last_error = Some("MCP server exited unexpectedly".to_string());
                }
            }
        }
    }

    to_status(&rt)
}

pub async fn start(
    app_state: Arc<AppState>,
    host: Option<String>,
    port: Option<u16>,
) -> Result<McpServerStatus, String> {
    // Prefer LAN binding unless caller explicitly provides a host
    let host = host.unwrap_or_else(|| "0.0.0.0".to_string());
    // Port 0 = let OS pick a free port
    let port = port.unwrap_or(0);

    {
        let rt = runtime().lock();
        if rt.running {
            return Ok(to_status(&rt));
        }
    }

    // Load (or generate) the bearer token
    let tok = token::load_or_create();

    // Generate or load self-signed TLS certificate
    let tls_config = tls::load_or_create_rustls_config().await?;

    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|e| format!("Invalid MCP bind address: {e}"))?;

    // When port is 0, discover a free port via a temporary TCP bind.
    // axum-server::bind_rustls doesn't expose local_addr() before serving,
    // so we probe first, drop the socket, and immediately re-bind with TLS.
    let actual_addr = if port == 0 {
        let probe = std::net::TcpListener::bind(&addr)
            .map_err(|e| format!("Failed to probe for free port: {e}"))?;
        let resolved = probe
            .local_addr()
            .map_err(|e| format!("Failed to get local address: {e}"))?;
        drop(probe);
        resolved
    } else {
        addr
    };
    let actual_port = actual_addr.port();
    let connect_host = if host == "0.0.0.0" {
        "127.0.0.1".to_string()
    } else {
        host.clone()
    };
    let endpoint = format!("https://{}:{}/mcp", connect_host, actual_port);

    // Write discovery file so mobile / desktop clients can find us
    write_discovery(&host, actual_port, &tok);

    let server_state = Arc::new(HttpState {
        app_state,
        token: tok.clone(),
    });

    // CORS: allow any origin (LAN mode) — restrict in production as desired
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let router = Router::new()
        .route("/", get(root_handler))
        .route("/mcp", post(mcp_handler))
        .route("/mcp/sse", get(sse_handler))
        .route("/mcp/messages", post(mcp_handler))
        .with_state(server_state)
        .layer(cors);

    let (shutdown_tx, _shutdown_rx) = oneshot::channel();

    let handle = axum_server::Handle::new();
    let server_handle = handle.clone();

    let task = tokio::spawn(async move {
        if let Err(err) = axum_server::bind_rustls(actual_addr, tls_config)
            .handle(server_handle)
            .serve(router.into_make_service())
            .await
        {
            tracing::error!("MCP HTTPS server error: {}", err);
        }
    });

    let mut rt = runtime().lock();
    rt.running = true;
    rt.host = connect_host;
    rt.port = actual_port;
    rt.endpoint = endpoint;
    rt.token = tok;
    rt.shutdown = Some(shutdown_tx);
    rt.server_handle = Some(handle);
    rt.task = Some(task);
    rt.last_error = None;
    Ok(to_status(&rt))
}

pub async fn stop() -> McpServerStatus {
    let (shutdown, server_handle, task) = {
        let mut rt = runtime().lock();
        rt.running = false;
        (rt.shutdown.take(), rt.server_handle.take(), rt.task.take())
    };

    if let Some(h) = server_handle {
        h.shutdown();
    }
    if let Some(tx) = shutdown {
        let _ = tx.send(());
    }
    if let Some(task) = task {
        let _ = task.await;
    }

    remove_discovery();
    status()
}

// ---------------------------------------------------------------------------
// Authentication helper
// ---------------------------------------------------------------------------

/// Returns `None` if the request carries a valid bearer token, or
/// `Some(Response)` with a 401 if authentication fails.
fn check_auth(headers: &HeaderMap, expected_token: &str) -> Option<Response> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let valid = auth_header
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t == expected_token)
        .unwrap_or(false);

    if valid {
        None
    } else {
        Some(
            (
                StatusCode::UNAUTHORIZED,
                Json(json!({"error": "Unauthorized: valid Bearer token required"})),
            )
                .into_response(),
        )
    }
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

/// Unauthenticated probe — lets clients discover the server without a token.
async fn root_handler(State(_state): State<Arc<HttpState>>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "name": "FNDR MCP Server",
            "mcp_endpoint": "/mcp",
            "sse_endpoint": "/mcp/sse",
            "transport": ["http", "sse"]
        })),
    )
}

/// POST /mcp  and  POST /mcp/messages — authenticated JSON-RPC handler.
async fn mcp_handler(
    State(state): State<Arc<HttpState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    if let Some(err_resp) = check_auth(&headers, &state.token) {
        return err_resp;
    }

    let app_state = state.app_state.clone();
    let handled = tokio::task::spawn_blocking(move || {
        let handle = tokio::runtime::Handle::current();
        handle.block_on(handle_payload(payload, app_state))
    })
    .await;

    match handled {
        Ok(Some(response_payload)) => (StatusCode::OK, Json(response_payload)).into_response(),
        Ok(None) => StatusCode::NO_CONTENT.into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("MCP handler task failed: {err}") })),
        )
            .into_response(),
    }
}

/// GET /mcp/sse — SSE streaming transport (MCP spec 2024-11-05).
///
/// Sends an initial `endpoint` event pointing the client at POST /mcp/messages,
/// then keeps the stream alive with periodic pings.
async fn sse_handler(State(state): State<Arc<HttpState>>, headers: HeaderMap) -> Response {
    if let Some(err_resp) = check_auth(&headers, &state.token) {
        return err_resp;
    }

    let session_id = uuid::Uuid::new_v4().to_string();
    let messages_url = format!("/mcp/messages?session={}", session_id);

    // Channel for the endpoint event + keepalives
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(16);

    // Send the initial endpoint event
    let endpoint_event = Event::default()
        .event("endpoint")
        .data(messages_url.clone());
    let _ = tx.send(Ok(endpoint_event)).await;

    // Spawn a task that keeps the stream pinging so clients don't time out
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(15)).await;
            if tx.send(Ok(Event::default().comment("ping"))).await.is_err() {
                break;
            }
        }
    });

    let stream = ReceiverStream::new(rx);
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

// ---------------------------------------------------------------------------
// JSON-RPC dispatch
// ---------------------------------------------------------------------------

async fn handle_payload(payload: Value, app_state: Arc<AppState>) -> Option<Value> {
    if let Value::Array(items) = payload {
        let mut responses = Vec::new();
        for item in items {
            if let Some(resp) = handle_single_request(item, app_state.clone()).await {
                responses.push(resp);
            }
        }
        if responses.is_empty() {
            None
        } else {
            Some(Value::Array(responses))
        }
    } else {
        handle_single_request(payload, app_state).await
    }
}

async fn handle_single_request(raw: Value, app_state: Arc<AppState>) -> Option<Value> {
    let req: JsonRpcRequest = match serde_json::from_value(raw) {
        Ok(req) => req,
        Err(err) => {
            return Some(error_response(
                Value::Null,
                -32600,
                format!("Invalid request: {err}"),
            ));
        }
    };

    let is_notification = req.id.is_none();
    let id = req.id.clone().unwrap_or(Value::Null);

    if req.jsonrpc.as_deref() != Some("2.0") {
        if is_notification {
            return None;
        }
        return Some(error_response(
            id,
            -32600,
            "Invalid JSON-RPC version; expected 2.0".to_string(),
        ));
    }

    let response = match req.method.as_str() {
        "initialize" => Ok(initialize_result(req.params)),
        "notifications/initialized" | "notifications.initialized" => {
            if is_notification {
                return None;
            }
            Ok(json!({}))
        }
        "ping" => Ok(json!({})),
        "tools/list" | "tools.list" => Ok(tools_list_result()),
        "tools/call" | "tools.call" => call_tool(req.params, app_state).await,
        _ => Err(JsonRpcError {
            code: -32601,
            message: format!("Method not found: {}", req.method),
        }),
    };

    if is_notification {
        return None;
    }

    Some(match response {
        Ok(result) => success_response(id, result),
        Err(err) => error_response(id, err.code, err.message),
    })
}

// ---------------------------------------------------------------------------
// MCP capability declarations
// ---------------------------------------------------------------------------

fn initialize_result(params: Option<Value>) -> Value {
    let protocol_version = params
        .as_ref()
        .and_then(|p| p.get("protocolVersion"))
        .and_then(Value::as_str)
        .unwrap_or("2024-11-05");

    json!({
        "protocolVersion": protocol_version,
        "capabilities": {
            "tools": { "listChanged": false }
        },
        "serverInfo": {
            "name": "FNDR",
            "version": env!("CARGO_PKG_VERSION")
        },
        "instructions": "FNDR exposes private local memory search and Q&A tools. All data lives on your machine."
    })
}

fn tools_list_result() -> Value {
    json!({
        "tools": [
            {
                "name": "search_memories",
                "description": "Search FNDR memory records by semantic + keyword relevance.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query":       { "type": "string", "description": "Search query text" },
                        "time_filter": { "type": "string", "enum": ["1h","24h","7d","today","yesterday"] },
                        "app_filter":  { "type": "string", "description": "Filter by app name" },
                        "limit":       { "type": "integer", "minimum": 1, "maximum": 50 }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "ask_fndr",
                "description": "Ask FNDR a question and get an answer grounded in captured memories. Times out after 30 seconds.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Question about captured activity" }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "get_fndr_stats",
                "description": "Return current capture/storage stats.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "start_meeting",
                "description": "Start a meeting recording session (Whisper large-v3 turbo GGUF on demand).",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "participants": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["title"]
                }
            },
            {
                "name": "stop_meeting",
                "description": "Stop the active meeting session.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            },
            {
                "name": "get_meeting_transcript",
                "description": "Fetch transcript data for a meeting id.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "meeting_id": { "type": "string" }
                    },
                    "required": ["meeting_id"]
                }
            },
            {
                "name": "search_meeting_transcripts",
                "description": "Search across meeting transcripts stored locally.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 100 }
                    },
                    "required": ["query"]
                }
            },
            {
                "name": "get_ambient_context",
                "description": "Return what the user is actively working on right now: frontmost app, recent memory snippets, and window context. Use this to give code editors, AI assistants, or other clients real-time awareness of the user's current task — the 'Time Machine for IDEs' feature.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "maximum": 20,
                            "description": "Number of recent memory snippets to include (default: 5)"
                        }
                    }
                }
            },
            {
                "name": "fndr_context",
                "description": "Build a source-backed FNDR context pack for an agent session.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "agent_type": { "type": "string" },
                        "budget_tokens": { "type": "integer", "minimum": 200, "maximum": 12000 },
                        "session_id": { "type": "string" },
                        "active_files": { "type": "array", "items": { "type": "string" } },
                        "project": { "type": "string" }
                    }
                }
            },
            {
                "name": "fndr_search_code_context",
                "description": "Return coding-oriented context for the active repo and files.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "repo": { "type": "string" },
                        "files": { "type": "array", "items": { "type": "string" } },
                        "budget_tokens": { "type": "integer", "minimum": 200, "maximum": 12000 }
                    }
                }
            },
            {
                "name": "fndr_diff",
                "description": "Return only new or changed FNDR context for a session since the last injection or explicit timestamp.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string" },
                        "since_timestamp": { "type": "integer" }
                    },
                    "required": ["session_id"]
                }
            },
            {
                "name": "fndr_get_recent_working_state",
                "description": "Return FNDR's best current understanding of what the user was just doing.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": { "type": "string" }
                    }
                }
            },
            {
                "name": "fndr_remember_decision",
                "description": "Append a proposed project decision to FNDR's decision ledger.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "project": { "type": "string" },
                        "title": { "type": "string" },
                        "summary": { "type": "string" },
                        "proposed_by": { "type": "string" },
                        "evidence_ids": { "type": "array", "items": { "type": "string" } }
                    },
                    "required": ["title"]
                }
            },
            {
                "name": "fndr_health_check",
                "description": "Return FNDR context runtime health, embedding contract status, and storage health.",
                "inputSchema": {
                    "type": "object",
                    "properties": {}
                }
            }
        ]
    })
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

async fn call_tool(params: Option<Value>, app_state: Arc<AppState>) -> Result<Value, JsonRpcError> {
    let params: ToolCallParams = serde_json::from_value(params.unwrap_or_else(|| json!({})))
        .map_err(|err| JsonRpcError {
            code: -32602,
            message: format!("Invalid tools/call params: {err}"),
        })?;

    match params.name.as_str() {
        "search_memories" => {
            let args: SearchMemoriesArgs =
                serde_json::from_value(params.arguments).map_err(|err| JsonRpcError {
                    code: -32602,
                    message: format!("Invalid search_memories args: {err}"),
                })?;
            run_search_memories(app_state, args).await
        }
        "ask_fndr" => {
            let args: AskFndrArgs =
                serde_json::from_value(params.arguments).map_err(|err| JsonRpcError {
                    code: -32602,
                    message: format!("Invalid ask_fndr args: {err}"),
                })?;
            run_ask_fndr(app_state, args).await
        }
        "get_fndr_stats" => run_get_stats(app_state).await,
        "start_meeting" => {
            let args: StartMeetingArgs =
                serde_json::from_value(params.arguments).map_err(|err| JsonRpcError {
                    code: -32602,
                    message: format!("Invalid start_meeting args: {err}"),
                })?;
            run_start_meeting(args).await
        }
        "stop_meeting" => run_stop_meeting().await,
        "get_meeting_transcript" => {
            let args: GetMeetingTranscriptArgs =
                serde_json::from_value(params.arguments).map_err(|err| JsonRpcError {
                    code: -32602,
                    message: format!("Invalid get_meeting_transcript args: {err}"),
                })?;
            run_get_meeting_transcript(args).await
        }
        "search_meeting_transcripts" => {
            let args: SearchMeetingTranscriptsArgs = serde_json::from_value(params.arguments)
                .map_err(|err| JsonRpcError {
                    code: -32602,
                    message: format!("Invalid search_meeting_transcripts args: {err}"),
                })?;
            run_search_meeting_transcripts(args).await
        }
        "get_ambient_context" => {
            let args: GetAmbientContextArgs = serde_json::from_value(params.arguments)
                .unwrap_or_else(|_| GetAmbientContextArgs {
                    limit: default_ambient_limit(),
                });
            run_get_ambient_context(app_state, args).await
        }
        "fndr_context" => {
            let args: ContextRequest =
                serde_json::from_value(params.arguments).map_err(|err| JsonRpcError {
                    code: -32602,
                    message: format!("Invalid fndr_context args: {err}"),
                })?;
            run_fndr_context(app_state, args).await
        }
        "fndr_search_code_context" => {
            let args: CodeContextRequest =
                serde_json::from_value(params.arguments).map_err(|err| JsonRpcError {
                    code: -32602,
                    message: format!("Invalid fndr_search_code_context args: {err}"),
                })?;
            run_fndr_search_code_context(app_state, args).await
        }
        "fndr_diff" => {
            let args: FndrDiffArgs =
                serde_json::from_value(params.arguments).map_err(|err| JsonRpcError {
                    code: -32602,
                    message: format!("Invalid fndr_diff args: {err}"),
                })?;
            run_fndr_diff(app_state, args).await
        }
        "fndr_get_recent_working_state" => {
            let args: ContextRequest = serde_json::from_value(params.arguments)
                .unwrap_or_else(|_| ContextRequest::default());
            run_fndr_get_recent_working_state(app_state, args).await
        }
        "fndr_remember_decision" => {
            let args: DecisionProposal =
                serde_json::from_value(params.arguments).map_err(|err| JsonRpcError {
                    code: -32602,
                    message: format!("Invalid fndr_remember_decision args: {err}"),
                })?;
            run_fndr_remember_decision(app_state, args).await
        }
        "fndr_health_check" => run_fndr_health_check(app_state).await,
        unknown => Ok(tool_error(format!("Unknown tool: {unknown}"))),
    }
}

async fn run_search_memories(
    app_state: Arc<AppState>,
    args: SearchMemoriesArgs,
) -> Result<Value, JsonRpcError> {
    let limit = args.limit.clamp(1, 50);
    let context_pack = context_runtime::build_context_pack(
        &app_state,
        ContextRequest {
            query: args.query.clone(),
            agent_type: "chat_agent".to_string(),
            budget_tokens: 1200,
            session_id: None,
            active_files: Vec::new(),
            project: None,
        },
    )
    .await
    .map_err(internal_tool_error)?;
    let embedder = Embedder::new().map_err(internal_tool_error)?;
    let results = HybridSearcher::search(
        &app_state.store,
        &embedder,
        &args.query,
        limit,
        args.time_filter.as_deref(),
        args.app_filter.as_deref(),
    )
    .await
    .map_err(internal_tool_error)?;

    Ok(tool_success(json!({
        "query": args.query,
        "count": results.len(),
        "results": results,
        "context_pack": context_pack
    })))
}

async fn run_ask_fndr(app_state: Arc<AppState>, args: AskFndrArgs) -> Result<Value, JsonRpcError> {
    let pack = context_runtime::build_context_pack(
        &app_state,
        ContextRequest {
            query: args.query.clone(),
            agent_type: "chat_agent".to_string(),
            budget_tokens: 1600,
            session_id: None,
            active_files: Vec::new(),
            project: None,
        },
    )
    .await
    .map_err(internal_tool_error)?;

    let embedder = Embedder::new().map_err(internal_tool_error)?;
    let results = HybridSearcher::search(&app_state.store, &embedder, &args.query, 8, None, None)
        .await
        .map_err(internal_tool_error)?;

    if results.is_empty() && pack.evidence.is_empty() && pack.relevant_files.is_empty() {
        return Ok(tool_success(json!({
            "answer": "I couldn't find relevant memories for that question yet.",
            "sources": [],
            "context_pack": pack
        })));
    }

    let mut context_sections = Vec::new();
    context_sections.push(context_runtime::render_pack_markdown(&pack));
    if !results.is_empty() {
        context_sections.push(
            results
                .iter()
                .take(8)
                .map(|r| {
                    format!(
                        "[{}] App: {} | Window: {} | Snippet: {} | URL: {}",
                        r.timestamp,
                        r.app_name,
                        r.window_title,
                        r.snippet,
                        r.url.clone().unwrap_or_else(|| "n/a".to_string())
                    )
                })
                .collect::<Vec<_>>()
                .join("\n"),
        );
    }
    let context = context_sections.join("\n\n");

    let answer_future = async {
        match app_state.ensure_inference_engine().await {
            Ok(Some(engine)) => engine.answer(&args.query, &context).await,
            Ok(None) => pack.summary.clone(),
            Err(err) => format!("AI intelligence is temporarily unavailable: {}", err),
        }
    };

    // 30-second timeout on LLM inference so slow models don't block forever
    let answer = tokio::time::timeout(Duration::from_secs(30), answer_future)
        .await
        .unwrap_or_else(|_| "Inference timed out after 30 seconds.".to_string());

    let sources: Vec<Value> = results
        .iter()
        .take(5)
        .map(|r| {
            json!({
                "id": r.id,
                "timestamp": r.timestamp,
                "app_name": r.app_name,
                "window_title": r.window_title,
                "snippet": r.snippet,
                "url": r.url
            })
        })
        .collect();

    Ok(tool_success(json!({
        "answer": answer,
        "sources": sources,
        "context_pack": pack
    })))
}

async fn run_get_stats(app_state: Arc<AppState>) -> Result<Value, JsonRpcError> {
    let stats = app_state
        .store
        .get_stats()
        .await
        .map_err(internal_tool_error)?;

    Ok(tool_success(json!({
        "stats": stats,
        "capture": {
            "is_capturing": app_state.is_capturing(),
            "is_paused": app_state.is_paused.load(std::sync::atomic::Ordering::SeqCst),
            "frames_captured": app_state.frames_captured.load(std::sync::atomic::Ordering::Relaxed),
            "frames_dropped": app_state.frames_dropped.load(std::sync::atomic::Ordering::Relaxed)
        }
    })))
}

async fn run_start_meeting(args: StartMeetingArgs) -> Result<Value, JsonRpcError> {
    let status = meeting::start_recording(
        None,
        args.title,
        args.participants.unwrap_or_default(),
        None,
    )
    .await
    .map_err(internal_tool_error)?;

    Ok(tool_success(json!({ "status": status })))
}

async fn run_stop_meeting() -> Result<Value, JsonRpcError> {
    let status = meeting::stop_recording()
        .await
        .map_err(internal_tool_error)?;
    Ok(tool_success(json!({ "status": status })))
}

async fn run_get_meeting_transcript(args: GetMeetingTranscriptArgs) -> Result<Value, JsonRpcError> {
    let transcript = meeting::get_meeting_transcript(&args.meeting_id)
        .await
        .map_err(internal_tool_error)?;
    Ok(tool_success(json!({ "transcript": transcript })))
}

async fn run_search_meeting_transcripts(
    args: SearchMeetingTranscriptsArgs,
) -> Result<Value, JsonRpcError> {
    let results = meeting::search_meeting_transcripts(&args.query, args.limit)
        .await
        .map_err(internal_tool_error)?;
    Ok(tool_success(json!({
        "query": args.query,
        "count": results.len(),
        "results": results
    })))
}

async fn run_get_ambient_context(
    app_state: Arc<AppState>,
    args: GetAmbientContextArgs,
) -> Result<Value, JsonRpcError> {
    let _limit = args.limit.clamp(1, 20);
    let frontmost_app =
        crate::capture::macos_frontmost_app_name().unwrap_or_else(|| "Unknown".to_string());
    let focus_task = app_state.focus_task.read().clone();
    let focus_drift_count = app_state
        .focus_drift_count
        .load(std::sync::atomic::Ordering::Relaxed);
    let working_state = context_runtime::get_recent_working_state(&app_state, None)
        .await
        .map_err(internal_tool_error)?;

    Ok(tool_success(json!({
        "frontmost_app": frontmost_app,
        "focus_task": focus_task,
        "focus_drift_count": focus_drift_count,
        "summary": working_state.summary,
        "working_state": working_state
    })))
}

async fn run_fndr_context(
    app_state: Arc<AppState>,
    args: ContextRequest,
) -> Result<Value, JsonRpcError> {
    let pack = context_runtime::build_context_pack(&app_state, args)
        .await
        .map_err(internal_tool_error)?;
    Ok(tool_success(json!({ "context_pack": pack })))
}

async fn run_fndr_search_code_context(
    app_state: Arc<AppState>,
    args: CodeContextRequest,
) -> Result<Value, JsonRpcError> {
    let code_context = context_runtime::build_code_context(&app_state, args)
        .await
        .map_err(internal_tool_error)?;
    Ok(tool_success(json!({ "code_context": code_context })))
}

async fn run_fndr_diff(
    app_state: Arc<AppState>,
    args: FndrDiffArgs,
) -> Result<Value, JsonRpcError> {
    let delta =
        context_runtime::build_context_delta(&app_state, &args.session_id, args.since_timestamp)
            .await
            .map_err(internal_tool_error)?;
    Ok(tool_success(json!({ "context_delta": delta })))
}

async fn run_fndr_get_recent_working_state(
    app_state: Arc<AppState>,
    args: ContextRequest,
) -> Result<Value, JsonRpcError> {
    let working_state = context_runtime::get_recent_working_state(&app_state, args.project)
        .await
        .map_err(internal_tool_error)?;
    Ok(tool_success(json!({ "working_state": working_state })))
}

async fn run_fndr_remember_decision(
    app_state: Arc<AppState>,
    args: DecisionProposal,
) -> Result<Value, JsonRpcError> {
    let decision = context_runtime::remember_decision(&app_state, args)
        .await
        .map_err(internal_tool_error)?;
    Ok(tool_success(json!({ "decision": decision })))
}

async fn run_fndr_health_check(app_state: Arc<AppState>) -> Result<Value, JsonRpcError> {
    let health = context_runtime::health_check(&app_state)
        .await
        .map_err(internal_tool_error)?;
    Ok(tool_success(json!({ "health": health })))
}

// ---------------------------------------------------------------------------
// Response helpers
// ---------------------------------------------------------------------------

fn tool_success(payload: Value) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
            }
        ],
        "structuredContent": payload
    })
}

fn tool_error(message: String) -> Value {
    json!({
        "isError": true,
        "content": [{ "type": "text", "text": message }]
    })
}

fn success_response(id: Value, result: Value) -> Value {
    serde_json::to_value(JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    })
    .unwrap_or_else(|_| {
        json!({"jsonrpc":"2.0","id":Value::Null,"error":{"code":-32603,"message":"Internal serialization error"}})
    })
}

fn error_response(id: Value, code: i64, message: String) -> Value {
    serde_json::to_value(JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError { code, message }),
    })
    .unwrap_or_else(|_| {
        json!({"jsonrpc":"2.0","id":Value::Null,"error":{"code":-32603,"message":"Internal serialization error"}})
    })
}

fn internal_tool_error<E: std::fmt::Display>(err: E) -> JsonRpcError {
    JsonRpcError {
        code: -32000,
        message: format!("Tool execution failed: {err}"),
    }
}
