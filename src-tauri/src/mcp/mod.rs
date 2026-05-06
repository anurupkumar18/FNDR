//! Localhost-only MCP server for FNDR — HTTP JSON-RPC 2.0 + SSE transport.
//!
//! Features:
//!  - Binds to `127.0.0.1:0` (OS-assigned port) for localhost-only access
//!  - Writes `~/.fndr/mcp.json` for client discovery
//!  - Optional bearer-token authentication, disabled by default for localhost
//!  - CORS layer permissive for local editor / tool connections
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
    extract::{ConnectInfo, OriginalUri, State},
    http::{header, HeaderMap, StatusCode, Uri},
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
use std::net::{IpAddr, SocketAddr};
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
    pub use_tls: bool,
    pub require_auth: bool,
    pub auth_mode: String,
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
    use_tls: bool,
    require_auth: bool,
    shutdown: Option<oneshot::Sender<()>>,
    server_handle: Option<axum_server::Handle>,
    task: Option<JoinHandle<()>>,
    last_error: Option<String>,
}

impl Default for McpRuntime {
    fn default() -> Self {
        Self {
            running: false,
            host: "127.0.0.1".to_string(),
            port: 0,
            endpoint: String::new(),
            token: String::new(),
            use_tls: false,
            require_auth: false,
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
    require_auth: bool,
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
const LOOPBACK_HOST: &str = "127.0.0.1";

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
        use_tls: rt.use_tls,
        require_auth: rt.require_auth,
        auth_mode: auth_mode_label(rt.require_auth),
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

fn write_discovery(host: &str, port: u16, token: &str, use_tls: bool, require_auth: bool) {
    let path = discovery_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let scheme = if use_tls { "https" } else { "http" };
    let endpoint = format!("{}://{}:{}/mcp", scheme, host, port);
    let cert_pem = if use_tls { tls::get_cert_pem() } else { None };
    let payload = json!({
        "host": host,
        "bind_host": host,
        "port": port,
        "token": token,
        "endpoint": endpoint,
        "sse_endpoint": format!("{}://{}:{}/mcp/sse", scheme, host, port),
        "tls": use_tls,
        "cert_pem": cert_pem,
        "auth_required": require_auth,
        "auth_mode": auth_mode_label(require_auth),
        "local_only": true
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
    let requested_host = host.unwrap_or_else(|| LOOPBACK_HOST.to_string());
    if !is_loopback_host(&requested_host) {
        return Err(format!(
            "FNDR MCP only supports localhost transport. Refusing to bind to {requested_host}."
        ));
    }
    let host = LOOPBACK_HOST.to_string();
    let port = port.unwrap_or(0);
    let require_auth = mcp_require_auth();

    {
        let rt = runtime().lock();
        if rt.running {
            return Ok(to_status(&rt));
        }
    }

    let use_tls = false;

    // Load (or generate) the bearer token
    let tok = token::load_or_create();

    let addr: SocketAddr = format!("{host}:{port}")
        .parse()
        .map_err(|e| format!("Invalid MCP bind address: {e}"))?;

    // axum-server::bind doesn't expose local_addr() before serving,
    // so we probe first, drop the socket, and immediately re-bind.
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
    let endpoint = format!("http://{}:{}/mcp", host, actual_port);

    tracing::info!(
        requested_host = %requested_host,
        bind_host = %host,
        port = actual_port,
        require_auth,
        "Starting FNDR MCP server on localhost"
    );

    write_discovery(&host, actual_port, &tok, use_tls, require_auth);

    let server_state = Arc::new(HttpState {
        app_state,
        token: tok.clone(),
        require_auth,
    });

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

    let task = if use_tls {
        let tls_config = tls::load_or_create_rustls_config().await?;
        tokio::spawn(async move {
            if let Err(err) = axum_server::bind_rustls(actual_addr, tls_config)
                .handle(server_handle)
                .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                .await
            {
                tracing::error!("MCP HTTPS server error: {}", err);
            }
        })
    } else {
        tokio::spawn(async move {
            if let Err(err) = axum_server::bind(actual_addr)
                .handle(server_handle)
                .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                .await
            {
                tracing::error!("MCP HTTP server error: {}", err);
            }
        })
    };

    let mut rt = runtime().lock();
    rt.running = true;
    rt.host = host;
    rt.port = actual_port;
    rt.endpoint = endpoint;
    rt.token = tok;
    rt.use_tls = use_tls;
    rt.require_auth = require_auth;
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

fn check_auth(headers: &HeaderMap, expected_token: &str) -> bool {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    auth_header
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t == expected_token)
        .unwrap_or(false)
}

fn mcp_require_auth() -> bool {
    std::env::var("FNDR_MCP_REQUIRE_AUTH")
        .ok()
        .and_then(|value| parse_bool_env(&value))
        .unwrap_or(false)
}

fn parse_bool_env(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn auth_mode_label(require_auth: bool) -> String {
    if require_auth {
        "required".to_string()
    } else {
        "disabled for localhost".to_string()
    }
}

fn is_loopback_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|ip| ip.is_loopback())
            .unwrap_or(false)
}

fn is_local_peer(peer_addr: SocketAddr) -> bool {
    peer_addr.ip().is_loopback()
}

fn is_local_handshake_method(rpc_method: Option<&str>) -> bool {
    matches!(
        rpc_method,
        Some("initialize" | "tools/list" | "tools.list")
    )
}

fn should_bypass_http_auth(
    peer_addr: SocketAddr,
    require_auth: bool,
    rpc_method: Option<&str>,
) -> bool {
    if !is_local_peer(peer_addr) {
        return false;
    }
    if !require_auth {
        return true;
    }
    is_local_handshake_method(rpc_method)
}

fn log_auth_bypass(peer_addr: SocketAddr, uri: &Uri, rpc_method: Option<&str>, reason: &str) {
    tracing::info!(
        peer = %peer_addr,
        path = %uri.path(),
        rpc_method = rpc_method.unwrap_or("unknown"),
        reason,
        "MCP auth bypassed for localhost request"
    );
}

fn jsonrpc_method_hint(payload: &Value) -> Option<&str> {
    match payload {
        Value::Object(map) => map.get("method").and_then(Value::as_str),
        Value::Array(items) => items.iter().find_map(jsonrpc_method_hint),
        _ => None,
    }
}

fn unauthorized_jsonrpc_response(payload: &Value) -> Response {
    let response_payload = unauthorized_jsonrpc_payload(payload);
    (StatusCode::UNAUTHORIZED, Json(response_payload)).into_response()
}

fn unauthorized_jsonrpc_payload(payload: &Value) -> Value {
    match payload {
        Value::Array(items) => {
            let responses = items
                .iter()
                .filter_map(unauthorized_jsonrpc_item)
                .collect::<Vec<_>>();
            if responses.is_empty() {
                error_response(
                    Value::Null,
                    -32001,
                    "Unauthorized: valid Bearer token required".to_string(),
                )
            } else {
                Value::Array(responses)
            }
        }
        _ => unauthorized_jsonrpc_item(payload).unwrap_or_else(|| {
            error_response(
                Value::Null,
                -32001,
                "Unauthorized: valid Bearer token required".to_string(),
            )
        }),
    }
}

fn unauthorized_jsonrpc_item(payload: &Value) -> Option<Value> {
    payload.as_object().map(|object| {
        error_response(
            object.get("id").cloned().unwrap_or(Value::Null),
            -32001,
            "Unauthorized: valid Bearer token required".to_string(),
        )
    })
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

/// Unauthenticated probe — lets clients discover the server without a token.
async fn root_handler(State(state): State<Arc<HttpState>>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(json!({
            "name": "FNDR MCP Server",
            "mcp_endpoint": "/mcp",
            "sse_endpoint": "/mcp/sse",
            "transport": ["http", "sse"],
            "auth_required": state.require_auth,
            "auth_mode": auth_mode_label(state.require_auth),
            "local_only": true
        })),
    )
}

/// POST /mcp  and  POST /mcp/messages — localhost JSON-RPC handler.
async fn mcp_handler(
    State(state): State<Arc<HttpState>>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let rpc_method = jsonrpc_method_hint(&payload);
    if should_bypass_http_auth(peer_addr, state.require_auth, rpc_method) {
        let reason = if state.require_auth {
            "local initialize/tools/list exemption"
        } else {
            "localhost auth disabled"
        };
        log_auth_bypass(peer_addr, &uri, rpc_method, reason);
    } else if !check_auth(&headers, &state.token) {
        return unauthorized_jsonrpc_response(&payload);
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
async fn sse_handler(
    State(state): State<Arc<HttpState>>,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Response {
    if should_bypass_http_auth(peer_addr, state.require_auth, None) {
        log_auth_bypass(peer_addr, &uri, Some("sse"), "localhost auth disabled");
    } else if !check_auth(&headers, &state.token) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error": "Unauthorized: valid Bearer token required"})),
        )
            .into_response();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::graph::GraphStore;
    use crate::store::{StateStore, Store};
    use tempfile::tempdir;

    fn build_test_app_state() -> Arc<AppState> {
        let temp_dir = tempdir().expect("tempdir");
        let data_dir = temp_dir.path().to_path_buf();
        std::mem::forget(temp_dir);
        let store = Arc::new(Store::new(&data_dir).expect("store"));
        let state_store = Arc::new(StateStore::new(&data_dir).expect("state store"));
        let graph = GraphStore::new(store.clone());
        Arc::new(AppState::new(
            data_dir,
            Config::default(),
            store,
            state_store,
            graph,
            None,
            None,
        ))
    }

    async fn wait_for_server(base_url: &str) {
        let client = reqwest::Client::new();
        for _ in 0..40 {
            if client.get(base_url).send().await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("MCP server did not become ready at {base_url}");
    }

    #[test]
    fn local_handshake_methods_bypass_auth_even_when_enabled() {
        let peer = SocketAddr::from(([127, 0, 0, 1], 8080));
        assert!(should_bypass_http_auth(peer, true, Some("initialize")));
        assert!(should_bypass_http_auth(peer, true, Some("tools/list")));
        assert!(!should_bypass_http_auth(peer, true, Some("tools/call")));
    }

    #[test]
    fn localhost_initialize_tools_list_and_call_work_without_auth() {
        std::env::remove_var("FNDR_MCP_REQUIRE_AUTH");
        let app_state = build_test_app_state();
        let runtime = tokio::runtime::Runtime::new().expect("tokio runtime");
        runtime.block_on(async move {
            let _ = stop().await;
            let status = start(app_state, None, Some(0)).await.expect("start mcp");
            let base_url = format!("http://{}:{}/", status.host, status.port);
            wait_for_server(&base_url).await;

            let client = reqwest::Client::new();

            let initialize = client
                .post(&status.endpoint)
                .header("Content-Type", "application/json")
                .json(&json!({
                    "jsonrpc": "2.0",
                    "id": 1,
                    "method": "initialize",
                    "params": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {},
                        "clientInfo": { "name": "reqwest-test", "version": "0.1.0" }
                    }
                }))
                .send()
                .await
                .expect("initialize request");
            assert_eq!(initialize.status(), reqwest::StatusCode::OK);
            let initialize_body: Value = initialize.json().await.expect("initialize json");
            assert_eq!(initialize_body["jsonrpc"], "2.0");
            assert_eq!(initialize_body["result"]["serverInfo"]["name"], "FNDR");

            let tools_list = client
                .post(&status.endpoint)
                .header("Content-Type", "application/json")
                .json(&json!({
                    "jsonrpc": "2.0",
                    "id": 2,
                    "method": "tools/list"
                }))
                .send()
                .await
                .expect("tools/list request");
            assert_eq!(tools_list.status(), reqwest::StatusCode::OK);
            let tools_list_body: Value = tools_list.json().await.expect("tools/list json");
            assert_eq!(tools_list_body["jsonrpc"], "2.0");
            assert!(tools_list_body["result"]["tools"].is_array());

            let tool_call = client
                .post(&status.endpoint)
                .header("Content-Type", "application/json")
                .json(&json!({
                    "jsonrpc": "2.0",
                    "id": 3,
                    "method": "tools/call",
                    "params": {
                        "name": "fndr_health_check",
                        "arguments": {}
                    }
                }))
                .send()
                .await
                .expect("tools/call request");
            assert_eq!(tool_call.status(), reqwest::StatusCode::OK);
            let tool_call_body: Value = tool_call.json().await.expect("tools/call json");
            assert_eq!(tool_call_body["jsonrpc"], "2.0");
            assert!(tool_call_body["result"]["structuredContent"]["health"].is_object());

            let _ = stop().await;
        });
    }
}
