//! Connect other AI tools to FNDR's MCP server: the exact config for Claude
//! Code, Claude Desktop and Cursor, written the way a careful person would.
//!
//! Rules the writer keeps, each held by a test below:
//! - Before it changes a file it copies the file to `<name>.fndr-backup-<time>`.
//! - It writes only the `fndr` entry under `mcpServers` and keeps every other
//!   key. An existing `fndr` entry is replaced only when FNDR wrote it, which
//!   it recognises by a loopback `/mcp` address; anything else is refused and
//!   the file is left as it was.
//! - It is idempotent: the same server and token change nothing and make no
//!   backup.
//! - A file that is not valid JSON, or whose `mcpServers` is not an object, is
//!   refused rather than rewritten.
//! - It can remove its own entry again, which is how a token is revoked.
//!
//! Paths are passed in, so tests run on synthetic files in a temp directory.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) const ENTRY_NAME: &str = "fndr";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Client {
    ClaudeCode,
    ClaudeDesktop,
    Cursor,
}

impl Client {
    pub(crate) const ALL: [Client; 3] = [Client::ClaudeCode, Client::ClaudeDesktop, Client::Cursor];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Client::ClaudeCode => "claude_code",
            Client::ClaudeDesktop => "claude_desktop",
            Client::Cursor => "cursor",
        }
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Client::ClaudeCode => "Claude Code",
            Client::ClaudeDesktop => "Claude Desktop",
            Client::Cursor => "Cursor",
        }
    }

    pub(crate) fn from_id(id: &str) -> Option<Client> {
        Client::ALL.into_iter().find(|client| client.id() == id)
    }

    /// The JSON file the client reads its servers from, under `home`.
    pub(crate) fn config_path(self, home: &Path) -> PathBuf {
        match self {
            Client::ClaudeCode => home.join(".claude.json"),
            Client::ClaudeDesktop => home
                .join("Library")
                .join("Application Support")
                .join("Claude")
                .join("claude_desktop_config.json"),
            Client::Cursor => home.join(".cursor").join("mcp.json"),
        }
    }

    /// Whether the client looks installed: its folder exists.
    pub(crate) fn detected(self, home: &Path) -> bool {
        let path = self.config_path(home);
        match self {
            Client::ClaudeCode => path.exists(),
            _ => path.parent().is_some_and(Path::exists),
        }
    }
}

/// What the client needs to reach the running server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Server {
    pub endpoint: String,
    pub token: String,
}

impl Server {
    fn bearer(&self) -> String {
        format!("Bearer {}", self.token)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConnectError {
    /// An entry named `fndr` exists that FNDR did not write.
    NotOurs {
        path: PathBuf,
    },
    /// The file cannot be read as the JSON shape the client uses.
    Unreadable {
        path: PathBuf,
        reason: String,
    },
    Io {
        path: PathBuf,
        reason: String,
    },
}

impl std::fmt::Display for ConnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectError::NotOurs { path } => write!(
                f,
                "{} already has a server named \"{ENTRY_NAME}\" that FNDR did not add. Nothing was changed. Rename or remove that entry, then connect again.",
                path.display()
            ),
            ConnectError::Unreadable { path, reason } => write!(
                f,
                "{} is not a config FNDR can edit safely ({reason}). Nothing was changed.",
                path.display()
            ),
            ConnectError::Io { path, reason } => {
                write!(f, "Could not write {}: {reason}", path.display())
            }
        }
    }
}

/// What a client's config says about FNDR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EntryState {
    /// No `fndr` entry (or no file).
    Absent,
    /// FNDR's entry, equal to what would be written now.
    Current,
    /// FNDR's entry, pointing at an old address or token.
    Stale,
    /// An entry named `fndr` that FNDR did not write.
    Foreign,
}

// ---------------------------------------------------------------------------
// The entries
// ---------------------------------------------------------------------------

/// Cursor: a remote server with a header.
pub(crate) fn cursor_entry(server: &Server) -> Value {
    json!({
        "url": server.endpoint,
        "headers": { "Authorization": server.bearer() }
    })
}

/// Claude Code's own record of an http server added with `claude mcp add`.
pub(crate) fn claude_code_entry(server: &Server) -> Value {
    json!({
        "type": "http",
        "url": server.endpoint,
        "headers": { "Authorization": server.bearer() }
    })
}

/// Claude Desktop reads command servers from its config file, so the bridge
/// is `mcp-remote`, which speaks stdio to the app and HTTP to FNDR. The
/// header value travels in `env`, which keeps the space in "Bearer <token>"
/// out of the argument list.
pub(crate) fn desktop_entry(server: &Server, npx: &str) -> Value {
    json!({
        "command": npx,
        "args": [
            "-y",
            "mcp-remote",
            server.endpoint,
            "--allow-http",
            "--header",
            "Authorization:${FNDR_MCP_AUTHORIZATION}"
        ],
        "env": { "FNDR_MCP_AUTHORIZATION": server.bearer() }
    })
}

/// The entry for a client that is configured by file. `None` for Claude Code,
/// which is configured through its CLI.
pub(crate) fn file_entry(client: Client, server: &Server, npx: &str) -> Option<Value> {
    match client {
        Client::ClaudeCode => None,
        Client::ClaudeDesktop => Some(desktop_entry(server, npx)),
        Client::Cursor => Some(cursor_entry(server)),
    }
}

/// The arguments for `claude mcp add`, one string each (no shell involved).
pub(crate) fn claude_code_add_args(server: &Server) -> Vec<String> {
    [
        "mcp",
        "add",
        "--scope",
        "user",
        "--transport",
        "http",
        ENTRY_NAME,
        &server.endpoint,
        "--header",
        &format!("Authorization: {}", server.bearer()),
    ]
    .map(str::to_string)
    .to_vec()
}

/// The same command as a line a person can paste into a terminal.
pub(crate) fn claude_code_command_line(server: &Server) -> String {
    format!(
        "claude mcp add --scope user --transport http {ENTRY_NAME} {} --header \"Authorization: {}\"",
        server.endpoint,
        server.bearer()
    )
}

fn is_loopback_mcp_url(value: &str) -> bool {
    let Some(rest) = value
        .strip_prefix("http://")
        .or_else(|| value.strip_prefix("https://"))
    else {
        return false;
    };
    let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = authority
        .rsplit_once(':')
        .map_or(authority, |(host, _)| host);
    matches!(host, "127.0.0.1" | "localhost" | "[::1]") && path.trim_end_matches('/') == "mcp"
}

/// Whether an entry is one FNDR wrote: a loopback `/mcp` address in `url`, or
/// in the arguments of the `mcp-remote` bridge.
pub(crate) fn is_ours(entry: &Value) -> bool {
    if let Some(url) = entry.get("url").and_then(Value::as_str) {
        return is_loopback_mcp_url(url);
    }
    let Some(args) = entry.get("args").and_then(Value::as_array) else {
        return false;
    };
    let strings = args.iter().filter_map(Value::as_str).collect::<Vec<_>>();
    strings.contains(&"mcp-remote") && strings.iter().any(|arg| is_loopback_mcp_url(arg))
}

// ---------------------------------------------------------------------------
// Reading and writing a config file
// ---------------------------------------------------------------------------

fn read_document(path: &Path) -> Result<Value, ConnectError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
        Err(err) => {
            return Err(ConnectError::Unreadable {
                path: path.to_path_buf(),
                reason: err.to_string(),
            })
        }
    };
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    let document: Value = serde_json::from_str(&text).map_err(|err| ConnectError::Unreadable {
        path: path.to_path_buf(),
        reason: format!("invalid JSON: {err}"),
    })?;
    if !document.is_object() {
        return Err(ConnectError::Unreadable {
            path: path.to_path_buf(),
            reason: "the top level is not an object".into(),
        });
    }
    if document
        .get("mcpServers")
        .is_some_and(|servers| !servers.is_object())
    {
        return Err(ConnectError::Unreadable {
            path: path.to_path_buf(),
            reason: "\"mcpServers\" is not an object".into(),
        });
    }
    Ok(document)
}

fn entry_of(document: &Value) -> Option<&Value> {
    document.get("mcpServers")?.get(ENTRY_NAME)
}

/// What the file at `path` says about FNDR, compared with `desired`.
pub(crate) fn entry_state(
    path: &Path,
    desired: Option<&Value>,
) -> Result<EntryState, ConnectError> {
    let document = read_document(path)?;
    Ok(match entry_of(&document) {
        None => EntryState::Absent,
        Some(entry) if !is_ours(entry) => EntryState::Foreign,
        Some(entry) if Some(entry) == desired => EntryState::Current,
        Some(_) => EntryState::Stale,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Written {
    pub changed: bool,
    pub backup: Option<PathBuf>,
}

fn timestamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

/// Copies `path` to a sibling that does not exist yet, readable by the owner
/// only, since the file may hold a token.
pub(crate) fn backup_file(path: &Path) -> Result<PathBuf, ConnectError> {
    let io = |err: std::io::Error| ConnectError::Io {
        path: path.to_path_buf(),
        reason: err.to_string(),
    };
    let stem = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let stamp = timestamp();
    let mut target = path.with_file_name(format!("{stem}.fndr-backup-{stamp}"));
    let mut counter = 1;
    while target.exists() {
        target = path.with_file_name(format!("{stem}.fndr-backup-{stamp}-{counter}"));
        counter += 1;
    }
    fs::copy(path, &target).map_err(io)?;
    set_owner_only(&target);
    Ok(target)
}

fn set_owner_only(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Writes `document` next to `path` and renames it into place, so a crash
/// leaves the old file or the new one. An existing file keeps its mode; a new
/// file is owner-only because it holds a token.
fn write_document(path: &Path, document: &Value) -> Result<(), ConnectError> {
    let io = |err: std::io::Error| ConnectError::Io {
        path: path.to_path_buf(),
        reason: err.to_string(),
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io)?;
    }
    let mut text = serde_json::to_string_pretty(document).map_err(|err| ConnectError::Io {
        path: path.to_path_buf(),
        reason: err.to_string(),
    })?;
    text.push('\n');
    let temp = path.with_file_name(format!(
        ".{}.fndr-tmp",
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    ));
    {
        let mut file = fs::File::create(&temp).map_err(io)?;
        file.write_all(text.as_bytes()).map_err(io)?;
        file.sync_all().map_err(io)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)
            .map(|meta| meta.permissions().mode() & 0o777)
            .unwrap_or(0o600);
        let _ = fs::set_permissions(&temp, fs::Permissions::from_mode(mode));
    }
    fs::rename(&temp, path).map_err(|err| {
        let _ = fs::remove_file(&temp);
        io(err)
    })
}

/// Adds or updates the `fndr` entry in the config at `path`.
pub(crate) fn apply_file_entry(path: &Path, entry: &Value) -> Result<Written, ConnectError> {
    let mut document = read_document(path)?;
    match entry_of(&document) {
        Some(existing) if !is_ours(existing) => {
            return Err(ConnectError::NotOurs {
                path: path.to_path_buf(),
            })
        }
        Some(existing) if existing == entry => {
            return Ok(Written {
                changed: false,
                backup: None,
            })
        }
        _ => {}
    }
    let backup = if path.exists() {
        Some(backup_file(path)?)
    } else {
        None
    };
    let root = document
        .as_object_mut()
        .expect("read_document returns an object");
    let servers = root
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()));
    servers
        .as_object_mut()
        .expect("read_document checked mcpServers")
        .insert(ENTRY_NAME.to_string(), entry.clone());
    write_document(path, &document)?;
    Ok(Written {
        changed: true,
        backup,
    })
}

/// Removes the `fndr` entry if FNDR wrote it. A foreign entry is refused, and
/// a config without one is left alone.
pub(crate) fn remove_file_entry(path: &Path) -> Result<Written, ConnectError> {
    let mut document = read_document(path)?;
    match entry_of(&document) {
        None => {
            return Ok(Written {
                changed: false,
                backup: None,
            })
        }
        Some(existing) if !is_ours(existing) => {
            return Err(ConnectError::NotOurs {
                path: path.to_path_buf(),
            })
        }
        Some(_) => {}
    }
    let backup = Some(backup_file(path)?);
    if let Some(servers) = document
        .get_mut("mcpServers")
        .and_then(Value::as_object_mut)
    {
        servers.remove(ENTRY_NAME);
    }
    write_document(path, &document)?;
    Ok(Written {
        changed: true,
        backup,
    })
}

// ---------------------------------------------------------------------------
// Claude Code, through its CLI
// ---------------------------------------------------------------------------

/// A runnable `name` found in `dirs`, then the usual install places.
pub(crate) fn find_executable(
    name: &str,
    path_var: Option<&std::ffi::OsStr>,
    home: &Path,
) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = path_var
        .map(|path| {
            std::env::split_paths(path)
                .map(|dir| dir.join(name))
                .collect()
        })
        .unwrap_or_default();
    candidates.extend([
        home.join(".local/bin").join(name),
        home.join(".claude/local").join(name),
        PathBuf::from("/opt/homebrew/bin").join(name),
        PathBuf::from("/usr/local/bin").join(name),
    ]);
    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn run_claude(claude: &Path, home: &Path, args: &[String]) -> Result<(), String> {
    let output = std::process::Command::new(claude)
        .args(args)
        .env("HOME", home)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|err| format!("could not run {}: {err}", claude.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    Err(detail.trim().chars().take(400).collect())
}

/// Adds FNDR to Claude Code at user scope through `claude mcp add`. The CLI
/// owns `~/.claude.json`, so FNDR reads it to learn whether the entry is its
/// own, backs it up, and lets the CLI do the edit.
pub(crate) fn apply_claude_code(
    home: &Path,
    claude: &Path,
    server: &Server,
) -> Result<Written, ConnectError> {
    let path = Client::ClaudeCode.config_path(home);
    let desired = claude_code_entry(server);
    match user_scope_entry(&path)? {
        Some(existing) if !is_ours(&existing) => return Err(ConnectError::NotOurs { path }),
        Some(existing) if existing == desired => {
            return Ok(Written {
                changed: false,
                backup: None,
            })
        }
        _ => {}
    }
    let backup = if path.exists() {
        Some(backup_file(&path)?)
    } else {
        None
    };
    let io = |reason: String| ConnectError::Io {
        path: path.clone(),
        reason,
    };
    if user_scope_entry(&path)?.is_some() {
        run_claude(
            claude,
            home,
            &["mcp", "remove", "--scope", "user", ENTRY_NAME].map(str::to_string),
        )
        .map_err(io)?;
    }
    run_claude(claude, home, &claude_code_add_args(server)).map_err(io)?;
    Ok(Written {
        changed: true,
        backup,
    })
}

/// Removes FNDR from Claude Code if FNDR added it.
pub(crate) fn remove_claude_code(home: &Path, claude: &Path) -> Result<Written, ConnectError> {
    let path = Client::ClaudeCode.config_path(home);
    match user_scope_entry(&path)? {
        None => {
            return Ok(Written {
                changed: false,
                backup: None,
            })
        }
        Some(existing) if !is_ours(&existing) => return Err(ConnectError::NotOurs { path }),
        Some(_) => {}
    }
    let backup = Some(backup_file(&path)?);
    run_claude(
        claude,
        home,
        &["mcp", "remove", "--scope", "user", ENTRY_NAME].map(str::to_string),
    )
    .map_err(|reason| ConnectError::Io { path, reason })?;
    Ok(Written {
        changed: true,
        backup,
    })
}

/// The user-scope `fndr` entry in `~/.claude.json`, read only. That file also
/// holds per-project data FNDR must not parse beyond this key.
pub(crate) fn user_scope_entry(path: &Path) -> Result<Option<Value>, ConnectError> {
    let document = read_document(path)?;
    Ok(entry_of(&document).cloned())
}

pub(crate) fn claude_code_state(home: &Path, server: &Server) -> Result<EntryState, ConnectError> {
    let path = Client::ClaudeCode.config_path(home);
    entry_state(&path, Some(&claude_code_entry(server)))
}

// ---------------------------------------------------------------------------
// What the app remembers between launches
// ---------------------------------------------------------------------------

/// Kept in `~/.fndr/mcp_connect.json`: the port the server last used, so a
/// client's saved address keeps working, and the clients FNDR connected, so
/// the server comes back at launch for them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ConnectState {
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub clients: Vec<String>,
    #[serde(default = "default_keep_running")]
    pub keep_running: bool,
}

fn default_keep_running() -> bool {
    true
}

impl Default for ConnectState {
    fn default() -> Self {
        Self {
            port: None,
            clients: Vec::new(),
            keep_running: true,
        }
    }
}

impl ConnectState {
    pub(crate) fn path(dir: &Path) -> PathBuf {
        dir.join("mcp_connect.json")
    }

    pub(crate) fn load(dir: &Path) -> ConnectState {
        fs::read_to_string(Self::path(dir))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub(crate) fn save(&self, dir: &Path) -> std::io::Result<()> {
        fs::create_dir_all(dir)?;
        let path = Self::path(dir);
        fs::write(
            &path,
            serde_json::to_string_pretty(self).unwrap_or_default(),
        )?;
        set_owner_only(&path);
        Ok(())
    }

    pub(crate) fn add(&mut self, client: Client) {
        if !self.clients.iter().any(|id| id == client.id()) {
            self.clients.push(client.id().to_string());
        }
    }

    pub(crate) fn remove(&mut self, client: Client) {
        self.clients.retain(|id| id != client.id());
    }
}

// ---------------------------------------------------------------------------
// Test connection: the calls a client makes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct TestStep {
    pub name: &'static str,
    pub ok: bool,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestReport {
    pub ok: bool,
    pub steps: Vec<TestStep>,
    pub tool_count: usize,
}

async fn rpc(
    client: &reqwest::Client,
    endpoint: &str,
    token: Option<&str>,
    body: Value,
) -> Result<(u16, Value), String> {
    let mut request = client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/event-stream")
        .json(&body);
    if let Some(token) = token {
        request = request.header("Authorization", format!("Bearer {token}"));
    }
    let response = request.send().await.map_err(|err| err.to_string())?;
    let status = response.status().as_u16();
    let body = response.json::<Value>().await.unwrap_or(Value::Null);
    Ok((status, body))
}

fn call_body(id: u64, method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params })
}

fn first_error(body: &Value) -> String {
    let item = if body.is_array() { &body[0] } else { body };
    item["error"]["message"]
        .as_str()
        .or_else(|| item["error"].as_str())
        .unwrap_or("no detail")
        .to_string()
}

fn step(name: &'static str, ok: bool, detail: impl Into<String>) -> TestStep {
    TestStep {
        name,
        ok,
        detail: detail.into(),
    }
}

/// Makes the calls a client makes, in the same order: `initialize`,
/// `tools/list`, then a `tools/call` with the bearer token, and checks that a
/// call without the token is refused. Never returns the token.
pub(crate) async fn test_connection(endpoint: &str, token: &str) -> TestReport {
    const REACH: &str = "Reach FNDR";
    const LIST: &str = "List tools";
    const CALL: &str = "Call a tool with the token";
    const REFUSE: &str = "Refuse calls without the token";

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .unwrap_or_default();
    let mut steps = Vec::new();
    let mut tool_count = 0;

    let initialize = call_body(
        1,
        "initialize",
        json!({
            "protocolVersion": "2025-03-26",
            "capabilities": {},
            "clientInfo": { "name": "FNDR connection test", "version": "1" }
        }),
    );
    match rpc(&client, endpoint, Some(token), initialize).await {
        Ok((200, body)) if body["result"]["serverInfo"]["name"] == "FNDR" => {
            steps.push(step(REACH, true, "FNDR answered the handshake."));
        }
        Ok((status, body)) => {
            steps.push(step(
                REACH,
                false,
                format!("Unexpected answer (HTTP {status}): {}", first_error(&body)),
            ));
            return TestReport {
                ok: false,
                steps,
                tool_count,
            };
        }
        Err(err) => {
            steps.push(step(
                REACH,
                false,
                format!("Could not connect to {endpoint}: {err}"),
            ));
            return TestReport {
                ok: false,
                steps,
                tool_count,
            };
        }
    }

    steps.push(
        match rpc(
            &client,
            endpoint,
            Some(token),
            call_body(2, "tools/list", json!({})),
        )
        .await
        {
            Ok((200, body)) => {
                let tools = body["result"]["tools"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                tool_count = tools.len();
                if tools.iter().any(|tool| tool["name"] == "fndr.search") {
                    step(LIST, true, format!("{tool_count} tools available."))
                } else {
                    step(
                        LIST,
                        false,
                        format!("{tool_count} tools listed, but fndr.search is missing."),
                    )
                }
            }
            Ok((status, body)) => step(
                LIST,
                false,
                format!("HTTP {status}: {}", first_error(&body)),
            ),
            Err(err) => step(LIST, false, err),
        },
    );

    let call = call_body(
        3,
        "tools/call",
        json!({ "name": "fndr.privacy_status", "arguments": {} }),
    );
    steps.push(
        match rpc(&client, endpoint, Some(token), call.clone()).await {
            Ok((200, body))
                if body["result"]["isError"] != true
                    && body["result"]["structuredContent"].is_object() =>
            {
                step(CALL, true, "fndr.privacy_status answered.")
            }
            Ok((200, body)) => step(
                CALL,
                false,
                format!("The tool reported an error: {}", first_error(&body)),
            ),
            Ok((status, body)) => step(
                CALL,
                false,
                format!("HTTP {status}: {}", first_error(&body)),
            ),
            Err(err) => step(CALL, false, err),
        },
    );

    steps.push(match rpc(&client, endpoint, None, call).await {
        Ok((401, _)) => step(REFUSE, true, "A call with no token was refused."),
        Ok((200, body)) if body["error"].is_object() || body["result"]["isError"] == true => {
            step(REFUSE, true, "A call with no token was refused.")
        }
        Ok((status, _)) => step(
            REFUSE,
            false,
            format!("A call with no token was answered (HTTP {status}). Token checks are off."),
        ),
        Err(err) => step(REFUSE, false, err),
    });

    TestReport {
        ok: steps.iter().all(|step| step.ok),
        steps,
        tool_count,
    }
}

// ---------------------------------------------------------------------------
// The flow the Setup screen drives
// ---------------------------------------------------------------------------

/// Where things live on this Mac. Tests build one over a temp directory.
#[derive(Debug, Clone)]
pub(crate) struct Env {
    pub home: PathBuf,
    pub fndr_dir: PathBuf,
    pub claude: Option<PathBuf>,
    pub npx: Option<PathBuf>,
}

impl Env {
    pub(crate) fn detect() -> Env {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let path_var = std::env::var_os("PATH");
        Env {
            claude: find_executable("claude", path_var.as_deref(), &home),
            npx: find_executable("npx", path_var.as_deref(), &home),
            fndr_dir: super::fndr_home(),
            home,
        }
    }

    fn npx_command(&self) -> String {
        self.npx
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| "npx".to_string())
    }
}

/// What the Setup card for one client shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClientCard {
    pub id: &'static str,
    pub name: &'static str,
    /// not_connected, connected, server_off, needs_update, conflict or unreadable.
    pub state: &'static str,
    pub detected: bool,
    pub config_path: String,
    pub detail: Option<String>,
    /// Claude Desktop needs Node for its bridge; Claude Code needs its CLI.
    pub missing_tool: Option<&'static str>,
}

pub(crate) fn client_card(env: &Env, client: Client, server: Option<&Server>) -> ClientCard {
    let path = client.config_path(&env.home);
    let desired = server.and_then(|server| match client {
        Client::ClaudeCode => Some(claude_code_entry(server)),
        other => file_entry(other, server, &env.npx_command()),
    });
    let (state, detail) = match entry_state(&path, desired.as_ref()) {
        Ok(EntryState::Absent) => ("not_connected", None),
        Ok(EntryState::Foreign) => (
            "conflict",
            Some(format!("A server named \"{ENTRY_NAME}\" is already there and FNDR did not add it. FNDR will not change it.")),
        ),
        Ok(EntryState::Current) => ("connected", None),
        Ok(EntryState::Stale) if server.is_none() => ("server_off", None),
        Ok(EntryState::Stale) => ("needs_update", Some("The saved address or token is out of date.".to_string())),
        Err(err) => ("unreadable", Some(err.to_string())),
    };
    let missing_tool = match client {
        Client::ClaudeCode if env.claude.is_none() => Some("the claude command"),
        Client::ClaudeDesktop if env.npx.is_none() => Some("Node.js (npx)"),
        _ => None,
    };
    ClientCard {
        id: client.id(),
        name: client.name(),
        state,
        detected: client.detected(&env.home),
        config_path: path.to_string_lossy().to_string(),
        detail,
        missing_tool,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub message: String,
    pub config_path: String,
    pub backup_path: Option<String>,
    pub changed: bool,
}

fn port_of(endpoint: &str) -> Option<u16> {
    endpoint
        .rsplit_once(':')
        .and_then(|(_, rest)| rest.split('/').next())
        .and_then(|port| port.parse().ok())
}

pub(crate) const KILL_SWITCH_MESSAGE: &str =
    "Actions are turned off, so FNDR will not give another tool access. Turn actions back on in Settings, then connect.";

/// Connects one client. Granting access is an action: the kill switch
/// refuses it before anything is read or written.
pub(crate) fn apply(
    env: &Env,
    kill_switch: bool,
    client: Client,
    server: &Server,
) -> Result<Outcome, String> {
    if kill_switch {
        return Err(KILL_SWITCH_MESSAGE.to_string());
    }
    let path = client.config_path(&env.home);
    let written = match client {
        Client::ClaudeCode => {
            let claude = env.claude.as_deref().ok_or_else(|| {
                format!(
                    "FNDR could not find the claude command. Install Claude Code, or run this yourself:\n{}",
                    claude_code_command_line(server)
                )
            })?;
            apply_claude_code(&env.home, claude, server)
        }
        other => {
            let entry = file_entry(other, server, &env.npx_command()).expect("file clients have an entry");
            apply_file_entry(&path, &entry)
        }
    }
    .map_err(|err| err.to_string())?;

    let mut state = ConnectState::load(&env.fndr_dir);
    state.port = port_of(&server.endpoint).or(state.port);
    state.add(client);
    state
        .save(&env.fndr_dir)
        .map_err(|err| format!("Connected, but FNDR could not save its own settings: {err}"))?;

    let restart = match client {
        Client::ClaudeCode => "Start a new Claude Code session to use it.",
        Client::ClaudeDesktop | Client::Cursor => "Restart the app to use it.",
    };
    Ok(Outcome {
        message: if written.changed {
            format!("{} is connected. {restart}", client.name())
        } else {
            format!("{} was already connected.", client.name())
        },
        config_path: path.to_string_lossy().to_string(),
        backup_path: written.backup.map(|p| p.to_string_lossy().to_string()),
        changed: written.changed,
    })
}

/// Removes FNDR's entry from one client. Never refused by the kill switch:
/// taking access away is always allowed.
pub(crate) fn disconnect(env: &Env, client: Client) -> Result<Outcome, String> {
    let path = client.config_path(&env.home);
    let written = match client {
        Client::ClaudeCode => match env.claude.as_deref() {
            Some(claude) => remove_claude_code(&env.home, claude),
            None => match user_scope_entry(&path) {
                Ok(None) => Ok(Written { changed: false, backup: None }),
                Ok(Some(_)) => return Err("FNDR could not find the claude command to remove its entry. Run: claude mcp remove --scope user fndr".to_string()),
                Err(err) => Err(err),
            },
        },
        _ => remove_file_entry(&path),
    }
    .map_err(|err| err.to_string())?;
    let mut state = ConnectState::load(&env.fndr_dir);
    state.remove(client);
    let _ = state.save(&env.fndr_dir);
    Ok(Outcome {
        message: if written.changed {
            format!("{} is disconnected.", client.name())
        } else {
            format!("{} had no FNDR entry.", client.name())
        },
        config_path: path.to_string_lossy().to_string(),
        backup_path: written.backup.map(|p| p.to_string_lossy().to_string()),
        changed: written.changed,
    })
}

/// The exact config for a client, as text a person can read or paste.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub title: String,
    pub path: String,
    pub text: String,
    /// The same text with the token hidden, for the screen.
    pub masked_text: String,
    pub command: Option<String>,
}

pub(crate) fn preview(env: &Env, client: Client, server: &Server) -> Preview {
    let path = client.config_path(&env.home);
    let (title, text, command) = match client {
        Client::ClaudeCode => (
            "Run in a terminal".to_string(),
            claude_code_command_line(server),
            Some(claude_code_command_line(server)),
        ),
        other => {
            let entry =
                file_entry(other, server, &env.npx_command()).expect("file clients have an entry");
            (
                format!("Added to {}", path.display()),
                serde_json::to_string_pretty(&json!({ "mcpServers": { ENTRY_NAME: entry } }))
                    .unwrap_or_default(),
                None,
            )
        }
    };
    let masked = |value: &str| {
        value.replace(
            &server.token,
            "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}",
        )
    };
    Preview {
        title,
        path: path.to_string_lossy().to_string(),
        masked_text: masked(&text),
        command: command.map(|command| masked(&command)),
        text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_in(dir: &Path, with_claude: bool) -> Env {
        let home = dir.join("home");
        fs::create_dir_all(&home).unwrap();
        Env {
            claude: with_claude.then(|| fake_claude(&home)),
            npx: Some(PathBuf::from("/opt/homebrew/bin/npx")),
            fndr_dir: dir.join("fndr"),
            home,
        }
    }

    #[test]
    fn the_kill_switch_refuses_a_connect_before_any_file_is_touched() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_in(dir.path(), true);
        for client in Client::ALL {
            let error = apply(&env, true, client, &server()).unwrap_err();
            assert_eq!(error, KILL_SWITCH_MESSAGE);
            assert!(!client.config_path(&env.home).exists());
        }
        assert!(calls(&env.home).is_empty());
        assert_eq!(ConnectState::load(&env.fndr_dir), ConnectState::default());
    }

    #[test]
    fn the_kill_switch_never_blocks_taking_access_away() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_in(dir.path(), true);
        apply(&env, false, Client::Cursor, &server()).unwrap();
        let removed = disconnect(&env, Client::Cursor).unwrap();
        assert!(removed.changed);
        assert!(ConnectState::load(&env.fndr_dir).clients.is_empty());
    }

    #[test]
    fn connecting_records_the_port_and_client_and_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_in(dir.path(), true);
        let first = apply(&env, false, Client::ClaudeDesktop, &server()).unwrap();
        assert!(first.changed && first.message.contains("Restart the app"));
        let second = apply(&env, false, Client::ClaudeDesktop, &server()).unwrap();
        assert!(!second.changed);
        assert_eq!(second.message, "Claude Desktop was already connected.");
        let state = ConnectState::load(&env.fndr_dir);
        assert_eq!(state.port, Some(47821));
        assert_eq!(state.clients, ["claude_desktop"]);
        let written = read(&Client::ClaudeDesktop.config_path(&env.home));
        assert_eq!(
            written["mcpServers"]["fndr"]["command"],
            "/opt/homebrew/bin/npx"
        );
    }

    #[test]
    fn a_card_reports_each_state_a_person_can_be_in() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_in(dir.path(), true);
        let card = |server: Option<&Server>| client_card(&env, Client::Cursor, server).state;
        assert_eq!(card(Some(&server())), "not_connected");
        apply(&env, false, Client::Cursor, &server()).unwrap();
        assert_eq!(card(Some(&server())), "connected");
        assert_eq!(card(Some(&other_server())), "needs_update");
        assert_eq!(card(None), "server_off");

        let path = Client::Cursor.config_path(&env.home);
        fs::write(&path, r#"{"mcpServers":{"fndr":{"command":"mine"}}}"#).unwrap();
        assert_eq!(card(Some(&server())), "conflict");
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(card(Some(&server())), "unreadable");
    }

    #[test]
    fn a_card_names_the_tool_a_client_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let mut env = env_in(dir.path(), false);
        env.npx = None;
        assert_eq!(
            client_card(&env, Client::ClaudeCode, None).missing_tool,
            Some("the claude command")
        );
        assert_eq!(
            client_card(&env, Client::ClaudeDesktop, None).missing_tool,
            Some("Node.js (npx)")
        );
        assert_eq!(client_card(&env, Client::Cursor, None).missing_tool, None);
        let error = apply(&env, false, Client::ClaudeCode, &server()).unwrap_err();
        assert!(error.contains("claude mcp add"));
    }

    #[test]
    fn the_preview_shows_the_exact_config_and_a_copy_with_the_token_hidden() {
        let dir = tempfile::tempdir().unwrap();
        let env = env_in(dir.path(), true);
        for client in Client::ALL {
            let shown = preview(&env, client, &server());
            assert!(shown.text.contains("tok-AAAA"), "{client:?}");
            assert!(!shown.masked_text.contains("tok-AAAA"), "{client:?}");
            assert!(shown
                .command
                .as_deref()
                .map_or(true, |c| !c.contains("tok-AAAA")));
        }
        let cursor = preview(&env, Client::Cursor, &server());
        let parsed: Value = serde_json::from_str(&cursor.text).unwrap();
        assert_eq!(parsed["mcpServers"]["fndr"], cursor_entry(&server()));
    }

    #[test]
    fn the_port_is_read_from_the_endpoint() {
        assert_eq!(port_of("http://127.0.0.1:47821/mcp"), Some(47821));
        assert_eq!(port_of("http://127.0.0.1/mcp"), None);
    }

    #[test]
    fn the_state_file_round_trips_and_a_bad_one_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(ConnectState::load(dir.path()), ConnectState::default());
        let mut state = ConnectState {
            port: Some(47821),
            clients: Vec::new(),
            ..Default::default()
        };
        state.add(Client::Cursor);
        state.add(Client::Cursor);
        state.add(Client::ClaudeCode);
        state.save(dir.path()).unwrap();
        assert_eq!(ConnectState::load(dir.path()), state);
        assert_eq!(state.clients, ["cursor", "claude_code"]);
        state.remove(Client::Cursor);
        assert_eq!(state.clients, ["claude_code"]);
        fs::write(ConnectState::path(dir.path()), "not json").unwrap();
        assert_eq!(ConnectState::load(dir.path()), ConnectState::default());
    }

    #[test]
    fn the_server_resume_choice_defaults_on_and_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut state = ConnectState::default();
        assert!(state.keep_running);
        state.add(Client::Cursor);
        state.keep_running = false;
        state.save(dir.path()).unwrap();
        let loaded = ConnectState::load(dir.path());
        assert_eq!(loaded.clients, vec!["cursor"]);
        assert!(!loaded.keep_running);
        let old = r#"{"port":47821,"clients":["cursor"]}"#;
        fs::write(ConnectState::path(dir.path()), old).unwrap();
        assert!(ConnectState::load(dir.path()).keep_running);
    }

    fn server() -> Server {
        Server {
            endpoint: "http://127.0.0.1:47821/mcp".into(),
            token: "tok-AAAA".into(),
        }
    }

    fn other_server() -> Server {
        Server {
            endpoint: "http://127.0.0.1:50000/mcp".into(),
            token: "tok-BBBB".into(),
        }
    }

    fn read(path: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    fn backups(dir: &Path) -> Vec<PathBuf> {
        let mut found = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.to_string_lossy().contains(".fndr-backup-"))
            .collect::<Vec<_>>();
        found.sort();
        found
    }

    #[test]
    fn each_client_gets_the_config_shape_it_reads() {
        let cursor = cursor_entry(&server());
        assert_eq!(cursor["url"], "http://127.0.0.1:47821/mcp");
        assert_eq!(cursor["headers"]["Authorization"], "Bearer tok-AAAA");

        let desktop = desktop_entry(&server(), "/opt/homebrew/bin/npx");
        assert_eq!(desktop["command"], "/opt/homebrew/bin/npx");
        let args = desktop["args"].as_array().unwrap();
        assert!(args.iter().any(|a| a == "mcp-remote"));
        assert!(args.iter().any(|a| a == "http://127.0.0.1:47821/mcp"));
        assert!(!desktop["args"].to_string().contains("tok-AAAA"));
        assert_eq!(desktop["env"]["FNDR_MCP_AUTHORIZATION"], "Bearer tok-AAAA");

        let args = claude_code_add_args(&server());
        assert_eq!(&args[..2], ["mcp", "add"]);
        assert!(args.contains(&"http://127.0.0.1:47821/mcp".to_string()));
        assert!(args.contains(&"Authorization: Bearer tok-AAAA".to_string()));
        assert!(claude_code_command_line(&server()).starts_with("claude mcp add "));
    }

    #[test]
    fn config_paths_sit_where_each_client_looks() {
        let home = Path::new("/h");
        assert_eq!(
            Client::ClaudeCode.config_path(home),
            Path::new("/h/.claude.json")
        );
        assert_eq!(
            Client::Cursor.config_path(home),
            Path::new("/h/.cursor/mcp.json")
        );
        assert_eq!(
            Client::ClaudeDesktop.config_path(home),
            Path::new("/h/Library/Application Support/Claude/claude_desktop_config.json")
        );
        assert_eq!(Client::from_id("cursor"), Some(Client::Cursor));
        assert_eq!(Client::from_id("emacs"), None);
    }

    #[test]
    fn only_a_loopback_mcp_address_counts_as_an_entry_fndr_wrote() {
        assert!(is_ours(&cursor_entry(&server())));
        assert!(is_ours(&claude_code_entry(&server())));
        assert!(is_ours(&desktop_entry(&server(), "npx")));
        assert!(is_ours(&json!({ "url": "http://localhost:9/mcp/" })));
        assert!(!is_ours(&json!({ "url": "https://mcp.example.com/mcp" })));
        assert!(!is_ours(&json!({ "url": "http://127.0.0.1:9/other" })));
        assert!(!is_ours(
            &json!({ "command": "npx", "args": ["-y", "some-other-server"] })
        ));
        assert!(!is_ours(
            &json!({ "command": "npx", "args": ["mcp-remote", "https://x.example/mcp"] })
        ));
    }

    #[test]
    fn a_missing_file_is_created_with_only_the_fndr_entry_and_owner_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".cursor").join("mcp.json");
        let written = apply_file_entry(&path, &cursor_entry(&server())).unwrap();
        assert!(written.changed);
        assert_eq!(written.backup, None);
        assert_eq!(
            read(&path),
            json!({ "mcpServers": { "fndr": cursor_entry(&server()) } })
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn applying_keeps_every_other_key_and_server_and_backs_up_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("claude_desktop_config.json");
        let original = json!({
            "globalShortcut": "Ctrl+Space",
            "mcpServers": {
                "github": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"], "env": { "GITHUB_TOKEN": "ghp_x" } },
                "notes": { "url": "https://notes.example.com/mcp" }
            }
        });
        fs::write(&path, serde_json::to_string_pretty(&original).unwrap()).unwrap();
        let before = fs::read_to_string(&path).unwrap();

        let written = apply_file_entry(&path, &desktop_entry(&server(), "npx")).unwrap();
        assert!(written.changed);
        let backup = written.backup.unwrap();
        assert_eq!(fs::read_to_string(&backup).unwrap(), before);

        let after = read(&path);
        assert_eq!(after["globalShortcut"], "Ctrl+Space");
        assert_eq!(
            after["mcpServers"]["github"],
            original["mcpServers"]["github"]
        );
        assert_eq!(
            after["mcpServers"]["notes"],
            original["mcpServers"]["notes"]
        );
        assert_eq!(after["mcpServers"]["fndr"], desktop_entry(&server(), "npx"));
    }

    #[test]
    fn applying_twice_changes_nothing_and_makes_one_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        fs::write(
            &path,
            r#"{ "mcpServers": { "other": { "url": "https://o.example/mcp" } } }"#,
        )
        .unwrap();

        let first = apply_file_entry(&path, &cursor_entry(&server())).unwrap();
        assert!(first.changed);
        let bytes = fs::read(&path).unwrap();

        let second = apply_file_entry(&path, &cursor_entry(&server())).unwrap();
        assert_eq!(
            second,
            Written {
                changed: false,
                backup: None
            }
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(backups(dir.path()).len(), 1);
    }

    #[test]
    fn a_new_token_replaces_our_own_entry_and_keeps_a_backup_of_the_old_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        apply_file_entry(&path, &cursor_entry(&server())).unwrap();
        assert_eq!(
            entry_state(&path, Some(&cursor_entry(&other_server()))).unwrap(),
            EntryState::Stale
        );
        let written = apply_file_entry(&path, &cursor_entry(&other_server())).unwrap();
        assert!(written.changed);
        let backup = written.backup.unwrap();
        assert!(fs::read_to_string(backup).unwrap().contains("tok-AAAA"));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("tok-BBBB") && !text.contains("tok-AAAA"));
        assert_eq!(
            entry_state(&path, Some(&cursor_entry(&other_server()))).unwrap(),
            EntryState::Current
        );
    }

    #[test]
    fn an_entry_fndr_did_not_write_is_never_overwritten_or_removed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        let theirs =
            r#"{"mcpServers":{"fndr":{"command":"my-own-fndr-bridge","args":["--stdio"]}}}"#;
        fs::write(&path, theirs).unwrap();

        let error = apply_file_entry(&path, &cursor_entry(&server())).unwrap_err();
        assert!(matches!(error, ConnectError::NotOurs { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), theirs);
        assert_eq!(entry_state(&path, None).unwrap(), EntryState::Foreign);

        let error = remove_file_entry(&path).unwrap_err();
        assert!(matches!(error, ConnectError::NotOurs { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), theirs);
        assert!(backups(dir.path()).is_empty());
    }

    #[test]
    fn a_file_that_is_not_the_expected_json_is_refused_untouched() {
        let dir = tempfile::tempdir().unwrap();
        for (name, body) in [
            ("jsonc.json", "{ // a comment\n \"mcpServers\": {} }"),
            ("array.json", "[1, 2]"),
            ("servers.json", r#"{ "mcpServers": ["x"] }"#),
        ] {
            let path = dir.path().join(name);
            fs::write(&path, body).unwrap();
            let error = apply_file_entry(&path, &cursor_entry(&server())).unwrap_err();
            assert!(matches!(error, ConnectError::Unreadable { .. }), "{name}");
            assert_eq!(fs::read_to_string(&path).unwrap(), body, "{name}");
        }
        assert!(backups(dir.path()).is_empty());
    }

    #[test]
    fn an_empty_file_is_treated_as_an_empty_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        fs::write(&path, "  \n").unwrap();
        apply_file_entry(&path, &cursor_entry(&server())).unwrap();
        assert_eq!(read(&path)["mcpServers"]["fndr"], cursor_entry(&server()));
    }

    #[test]
    fn removing_takes_only_our_entry_and_backs_up_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp.json");
        fs::write(
            &path,
            r#"{ "mcpServers": { "keep": { "url": "https://k.example/mcp" } } }"#,
        )
        .unwrap();
        apply_file_entry(&path, &cursor_entry(&server())).unwrap();

        let removed = remove_file_entry(&path).unwrap();
        assert!(removed.changed);
        assert!(fs::read_to_string(removed.backup.unwrap())
            .unwrap()
            .contains("tok-AAAA"));
        let after = read(&path);
        assert!(after["mcpServers"].get("fndr").is_none());
        assert_eq!(after["mcpServers"]["keep"]["url"], "https://k.example/mcp");
        assert!(!fs::read_to_string(&path).unwrap().contains("tok-AAAA"));

        assert_eq!(
            remove_file_entry(&path).unwrap(),
            Written {
                changed: false,
                backup: None
            }
        );
        let missing = dir.path().join("none.json");
        assert_eq!(
            remove_file_entry(&missing).unwrap(),
            Written {
                changed: false,
                backup: None
            }
        );
        assert!(!missing.exists());
    }

    /// A stand-in for the `claude` CLI: `add` writes the user-scope entry the
    /// real one writes, `remove` deletes it, and every call is logged.
    fn fake_claude(dir: &Path) -> PathBuf {
        let script = dir.join("claude");
        fs::write(
            &script,
            r#"#!/bin/sh
echo "$@" >> "$HOME/claude-calls.log"
f="$HOME/.claude.json"
case "$2" in
  add)
    url="$8"; auth="${10}"
    printf '{"projects":{"/p":{"history":[1]}},"mcpServers":{"fndr":{"type":"http","url":"%s","headers":{"Authorization":"%s"}}}}' "$url" "${auth#Authorization: }" > "$f"
    ;;
  remove)
    printf '{"projects":{"/p":{"history":[1]}},"mcpServers":{}}' > "$f"
    ;;
esac
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        }
        script
    }

    fn calls(home: &Path) -> Vec<String> {
        fs::read_to_string(home.join("claude-calls.log"))
            .map(|log| log.lines().map(str::to_string).collect())
            .unwrap_or_default()
    }

    #[test]
    fn claude_code_is_added_through_the_cli_after_a_backup_and_only_once() {
        let home = tempfile::tempdir().unwrap();
        let claude = fake_claude(home.path());
        let config = Client::ClaudeCode.config_path(home.path());
        fs::write(
            &config,
            r#"{"projects":{"/p":{"history":[1]}},"mcpServers":{}}"#,
        )
        .unwrap();

        let first = apply_claude_code(home.path(), &claude, &server()).unwrap();
        assert!(first.changed);
        assert!(first.backup.is_some());
        assert_eq!(calls(home.path()).len(), 1);
        assert!(calls(home.path())[0]
            .starts_with("mcp add --scope user --transport http fndr http://127.0.0.1:47821/mcp"));
        assert_eq!(
            claude_code_state(home.path(), &server()).unwrap(),
            EntryState::Current
        );

        let again = apply_claude_code(home.path(), &claude, &server()).unwrap();
        assert_eq!(
            again,
            Written {
                changed: false,
                backup: None
            }
        );
        assert_eq!(
            calls(home.path()).len(),
            1,
            "an unchanged entry must not run the CLI again"
        );
    }

    #[test]
    fn a_stale_claude_code_entry_is_removed_then_added_again() {
        let home = tempfile::tempdir().unwrap();
        let claude = fake_claude(home.path());
        apply_claude_code(home.path(), &claude, &server()).unwrap();
        assert_eq!(
            claude_code_state(home.path(), &other_server()).unwrap(),
            EntryState::Stale
        );

        let written = apply_claude_code(home.path(), &claude, &other_server()).unwrap();
        assert!(written.changed);
        let log = calls(home.path());
        assert!(log[1].starts_with("mcp remove --scope user fndr"));
        assert!(log[2].starts_with("mcp add "));
        assert_eq!(
            claude_code_state(home.path(), &other_server()).unwrap(),
            EntryState::Current
        );
    }

    #[test]
    fn a_claude_code_server_named_fndr_that_fndr_did_not_add_is_left_alone() {
        let home = tempfile::tempdir().unwrap();
        let claude = fake_claude(home.path());
        let config = Client::ClaudeCode.config_path(home.path());
        let theirs =
            r#"{"mcpServers":{"fndr":{"type":"http","url":"https://corp.example.com/mcp"}}}"#;
        fs::write(&config, theirs).unwrap();

        let error = apply_claude_code(home.path(), &claude, &server()).unwrap_err();
        assert!(matches!(error, ConnectError::NotOurs { .. }));
        assert!(calls(home.path()).is_empty(), "the CLI must not run");
        assert_eq!(fs::read_to_string(&config).unwrap(), theirs);
        let error = remove_claude_code(home.path(), &claude).unwrap_err();
        assert!(matches!(error, ConnectError::NotOurs { .. }));
        assert!(calls(home.path()).is_empty());
    }

    #[test]
    fn removing_claude_code_runs_the_cli_only_when_fndr_added_the_entry() {
        let home = tempfile::tempdir().unwrap();
        let claude = fake_claude(home.path());
        assert_eq!(
            remove_claude_code(home.path(), &claude).unwrap(),
            Written {
                changed: false,
                backup: None
            }
        );
        assert!(calls(home.path()).is_empty());

        apply_claude_code(home.path(), &claude, &server()).unwrap();
        let removed = remove_claude_code(home.path(), &claude).unwrap();
        assert!(removed.changed && removed.backup.is_some());
        assert_eq!(
            claude_code_state(home.path(), &server()).unwrap(),
            EntryState::Absent
        );
    }

    #[test]
    fn a_failing_cli_reports_its_message() {
        let home = tempfile::tempdir().unwrap();
        let script = home.path().join("claude");
        fs::write(&script, "#!/bin/sh\necho 'boom: nope' >&2\nexit 3\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let error = apply_claude_code(home.path(), &script, &server()).unwrap_err();
        assert!(error.to_string().contains("boom: nope"), "{error}");
    }

    #[test]
    fn executables_are_found_on_path_or_in_the_usual_places() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("npx"), "").unwrap();
        let path = std::env::join_paths([&bin]).unwrap();
        assert_eq!(
            find_executable("npx", Some(&path), dir.path()),
            Some(bin.join("npx"))
        );
        assert_eq!(
            find_executable("no-such-tool-xyz", Some(&path), dir.path()),
            None
        );
        let local = dir.path().join(".local/bin");
        fs::create_dir_all(&local).unwrap();
        fs::write(local.join("claude"), "").unwrap();
        assert_eq!(
            find_executable("claude", None, dir.path()),
            Some(local.join("claude"))
        );
    }
}
