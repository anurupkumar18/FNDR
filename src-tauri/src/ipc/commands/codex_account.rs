//! Sign in with ChatGPT through the official Codex app-server.
//!
//! FNDR never handles ChatGPT tokens. `codex app-server` owns the OAuth flow,
//! hosts the localhost callback, and persists tokens to `$CODEX_HOME/auth.json`
//! (forced to file storage so they don't land in the Keychain). Hermes's
//! `openai-codex` provider reads that same file, so a user's ChatGPT or Codex
//! subscription becomes the agent's model provider without an API key.

use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::oneshot;

use super::hermes_agent::detect_codex_executable;

pub const CODEX_LOGIN_COMPLETED_EVENT: &str = "codex-login-completed";

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a browser sign-in may stay open before FNDR gives up on it.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// Tokens must live in auth.json for Hermes to import them.
const FILE_CREDENTIAL_STORE: &str = "cli_auth_credentials_store=\"file\"";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum CodexCliState {
    /// No `codex` executable found on this Mac.
    Missing,
    /// Found, but it would not start or complete the app-server handshake.
    Broken,
    Ready,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexAccount {
    /// `chatgpt`, `apiKey`, `amazonBedrock`, …
    pub kind: String,
    pub email: Option<String>,
    pub plan_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsageWindow {
    pub used_percent: f64,
    pub window_minutes: Option<u64>,
    pub resets_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CodexModel {
    pub id: String,
    pub display_name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexAccountStatus {
    pub cli_state: CodexCliState,
    pub cli_path: Option<String>,
    pub cli_error: Option<String>,
    pub account: Option<CodexAccount>,
    /// Only ChatGPT sign-in can back Hermes's `openai-codex` provider.
    pub usable_for_hermes: bool,
    pub primary_window: Option<CodexUsageWindow>,
    pub secondary_window: Option<CodexUsageWindow>,
    pub models: Vec<CodexModel>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexLoginStarted {
    pub login_id: String,
    pub auth_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexLoginCompleted {
    pub login_id: String,
    pub success: bool,
    pub error: Option<String>,
}

/// The Codex CLI's home: `$CODEX_HOME`, else `~/.codex`.
pub(crate) fn codex_home_dir() -> PathBuf {
    if let Some(value) = std::env::var_os("CODEX_HOME") {
        return PathBuf::from(value);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".codex")
}

/// One JSON-RPC connection to a `codex app-server` child over stdio (JSONL).
pub(crate) struct AppServer {
    child: Child,
    stdin: ChildStdin,
    lines: Lines<BufReader<ChildStdout>>,
    next_id: u64,
    /// The last thing Codex wrote to stderr, so an exit can say why.
    last_error_line: std::sync::Arc<Mutex<String>>,
}

/// "Codex app-server exited", with Codex's last error line when it left one.
fn exit_message(last_error_line: &str) -> String {
    let reason = last_error_line.trim();
    if reason.is_empty() {
        return "Codex app-server exited".to_string();
    }
    let reason: String = reason.chars().take(240).collect();
    format!("Codex app-server exited: {reason}")
}

impl AppServer {
    async fn spawn(executable: &Path) -> Result<Self, String> {
        Self::spawn_with(executable, &[]).await
    }

    pub(crate) async fn spawn_with(
        executable: &Path,
        extra_args: &[String],
    ) -> Result<Self, String> {
        let mut child = Command::new(executable)
            .args(["app-server", "-c", FILE_CREDENTIAL_STORE])
            .args(extra_args)
            .env("PATH", child_path_env(executable))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            // Its own process group, so stopping Notch Do can kill the MCP
            // servers Codex started along with it.
            .process_group(0)
            .spawn()
            .map_err(|e| format!("Could not start Codex ({}): {e}", executable.display()))?;

        let stdin = child.stdin.take().ok_or("Codex app-server has no stdin")?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Codex app-server has no stdout")?;
        let last_error_line = std::sync::Arc::new(Mutex::new(String::new()));
        if let Some(stderr) = child.stderr.take() {
            let keep = last_error_line.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    // Codex logs warnings while it runs; only what it says
                    // as it gives up is worth keeping.
                    if line.trim_start().starts_with("Error") || line.contains("in `") {
                        if let Ok(mut kept) = keep.lock() {
                            if line.trim_start().starts_with("Error") {
                                *kept = line;
                            } else {
                                kept.push(' ');
                                kept.push_str(line.trim());
                            }
                        }
                    }
                }
            });
        }
        let mut server = Self {
            child,
            stdin,
            lines: BufReader::new(stdout).lines(),
            next_id: 0,
            last_error_line,
        };

        server
            .request(
                "initialize",
                json!({
                    "clientInfo": {
                        "name": "fndr",
                        "title": "FNDR",
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            )
            .await?;
        server.notify("initialized", json!({})).await?;
        Ok(server)
    }

    /// Process id, which is also the process group id (see `spawn_with`).
    pub(crate) fn pid(&self) -> Option<u32> {
        self.child.id()
    }

    pub(crate) async fn write(&mut self, message: Value) -> Result<(), String> {
        let mut line = message.to_string();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .map_err(|e| format!("Codex app-server closed its input: {e}"))
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        self.write(json!({ "method": method, "params": params }))
            .await
    }

    pub(crate) async fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.write(json!({ "method": method, "id": id, "params": params }))
            .await?;

        tokio::time::timeout(REQUEST_TIMEOUT, async {
            loop {
                let message = self.read_message().await?;
                if message.get("id").and_then(Value::as_u64) != Some(id) {
                    continue;
                }
                if let Some(error) = message.get("error") {
                    let detail = error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown error");
                    return Err(format!("Codex {method} failed: {detail}"));
                }
                return Ok(message.get("result").cloned().unwrap_or(Value::Null));
            }
        })
        .await
        .map_err(|_| format!("Codex {method} timed out"))?
    }

    /// Waits for a server notification, skipping everything else.
    async fn wait_for_notification(&mut self, method: &str) -> Result<Value, String> {
        loop {
            let message = self.read_message().await?;
            if message.get("id").is_none()
                && message.get("method").and_then(Value::as_str) == Some(method)
            {
                return Ok(message.get("params").cloned().unwrap_or(Value::Null));
            }
        }
    }

    /// Next JSON-RPC message of any kind, including server-initiated requests.
    pub(crate) async fn read_raw(&mut self) -> Result<Value, String> {
        loop {
            let next = self
                .lines
                .next_line()
                .await
                .map_err(|e| format!("Reading from Codex app-server failed: {e}"))?;
            let Some(line) = next else {
                // Give the stderr reader a moment to see Codex's last words.
                tokio::time::sleep(Duration::from_millis(80)).await;
                let reason = self.last_error_line.lock().map(|l| l.clone());
                return Err(exit_message(&reason.unwrap_or_default()));
            };
            if let Ok(message) = serde_json::from_str::<Value>(&line) {
                // Live tests only: `FNDR_CODEX_TRACE=1` prints what Codex sent.
                #[cfg(test)]
                if std::env::var_os("FNDR_CODEX_TRACE").is_some() {
                    eprintln!("codex> {}", line.chars().take(700).collect::<String>());
                }
                return Ok(message);
            }
        }
    }

    /// Next response or notification. Server-initiated requests (approvals,
    /// elicitations, token refresh) carry both a method and an id; callers
    /// that don't handle them explicitly never grant them.
    async fn read_message(&mut self) -> Result<Value, String> {
        loop {
            let message = self.read_raw().await?;
            if let (Some(id), Some(method)) = (message.get("id").cloned(), message.get("method")) {
                tracing::warn!(%method, "codex_app_server:declined_server_request");
                self.write(json!({
                    "id": id,
                    "error": { "code": -32000, "message": "FNDR does not grant this request." }
                }))
                .await?;
                continue;
            }
            return Ok(message);
        }
    }

    pub(crate) async fn shutdown(mut self) {
        let _ = self.child.kill().await;
    }
}

/// GUI apps on macOS don't inherit the shell PATH, and the npm-installed
/// `codex` is a Node script, so the child needs `node` findable.
pub(crate) fn child_path_env(executable: &Path) -> std::ffi::OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(parent) = executable.parent() {
        dirs.push(parent.to_path_buf());
    }
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    if let Some(existing) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

pub(crate) fn parse_account(result: &Value) -> Option<CodexAccount> {
    let account = result.get("account")?.as_object()?;
    Some(CodexAccount {
        kind: account.get("type")?.as_str()?.to_string(),
        email: account
            .get("email")
            .and_then(Value::as_str)
            .map(str::to_string),
        plan_type: account
            .get("planType")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn parse_window(window: Option<&Value>) -> Option<CodexUsageWindow> {
    let window = window?.as_object()?;
    Some(CodexUsageWindow {
        used_percent: window.get("usedPercent")?.as_f64()?,
        window_minutes: window.get("windowDurationMins").and_then(Value::as_u64),
        resets_at: window.get("resetsAt").and_then(Value::as_i64),
    })
}

/// The model to run a turn on: the one this account's Codex marks as its
/// default, else the first it offers. The person's own `config.toml` may name
/// a model their plan does not include, and every turn on it fails.
fn pick_model(models: &[CodexModel]) -> Option<String> {
    models
        .iter()
        .find(|model| model.is_default)
        .or_else(|| models.first())
        .map(|model| model.id.clone())
}

/// Asks Codex which model this account should use. `None` leaves the choice
/// to Codex's own configuration.
pub(crate) async fn account_model(server: &mut AppServer) -> Option<String> {
    let listed = server
        .request("model/list", json!({ "limit": 50 }))
        .await
        .ok()?;
    pick_model(&parse_models(&listed))
}

/// `thread/start` parameters with the account's model named, when known.
pub(crate) fn with_model(mut thread: Value, model: Option<&str>) -> Value {
    if let (Some(model), Some(params)) = (model, thread.as_object_mut()) {
        params.insert("model".to_string(), json!(model));
    }
    thread
}

fn parse_models(result: &Value) -> Vec<CodexModel> {
    result
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|model| {
            !model
                .get("hidden")
                .and_then(Value::as_bool)
                .unwrap_or(false)
        })
        .filter_map(|model| {
            let id = model
                .get("model")
                .or_else(|| model.get("id"))?
                .as_str()?
                .to_string();
            Some(CodexModel {
                display_name: model
                    .get("displayName")
                    .and_then(Value::as_str)
                    .unwrap_or(&id)
                    .to_string(),
                is_default: model
                    .get("isDefault")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                id,
            })
        })
        .collect()
}

pub(crate) fn ready_executable() -> Result<PathBuf, String> {
    detect_codex_executable().ok_or_else(|| {
        "Codex isn't installed. Install it with `brew install codex` or `npm install -g @openai/codex@0.151.0`, then try again."
            .to_string()
    })
}

async fn read_status() -> CodexAccountStatus {
    let mut status = CodexAccountStatus {
        cli_state: CodexCliState::Missing,
        cli_path: None,
        cli_error: None,
        account: None,
        usable_for_hermes: false,
        primary_window: None,
        secondary_window: None,
        models: Vec::new(),
    };

    let Some(executable) = detect_codex_executable() else {
        return status;
    };
    status.cli_path = Some(executable.display().to_string());

    let mut server = match AppServer::spawn(&executable).await {
        Ok(server) => server,
        Err(error) => {
            status.cli_state = CodexCliState::Broken;
            status.cli_error = Some(error);
            return status;
        }
    };
    status.cli_state = CodexCliState::Ready;

    if let Ok(result) = server
        .request("account/read", json!({ "refreshToken": false }))
        .await
    {
        status.account = parse_account(&result);
    }
    status.usable_for_hermes = status.account.as_ref().is_some_and(|a| a.kind == "chatgpt");

    if status.usable_for_hermes {
        if let Ok(result) = server.request("account/rateLimits/read", Value::Null).await {
            let limits = result.get("rateLimits");
            status.primary_window = parse_window(limits.and_then(|l| l.get("primary")));
            status.secondary_window = parse_window(limits.and_then(|l| l.get("secondary")));
        }
        if let Ok(result) = server.request("model/list", json!({ "limit": 50 })).await {
            status.models = parse_models(&result);
        }
    }

    server.shutdown().await;
    status
}

struct PendingLogin {
    login_id: String,
    cancel: oneshot::Sender<()>,
}

fn pending_login() -> &'static Mutex<Option<PendingLogin>> {
    static PENDING: OnceLock<Mutex<Option<PendingLogin>>> = OnceLock::new();
    PENDING.get_or_init(|| Mutex::new(None))
}

fn take_pending_login(login_id: Option<&str>) -> Option<PendingLogin> {
    let mut guard = pending_login().lock().ok()?;
    match (guard.as_ref(), login_id) {
        (Some(pending), Some(id)) if pending.login_id != id => None,
        _ => guard.take(),
    }
}

#[tauri::command]
pub async fn codex_account_status() -> Result<CodexAccountStatus, String> {
    Ok(read_status().await)
}

/// Starts the ChatGPT browser sign-in. The app-server keeps running to host
/// the localhost callback; completion arrives as `codex-login-completed`.
#[tauri::command]
pub async fn codex_login_start(app: AppHandle) -> Result<CodexLoginStarted, String> {
    let executable = ready_executable()?;
    if let Some(previous) = take_pending_login(None) {
        let _ = previous.cancel.send(());
    }

    let mut server = AppServer::spawn(&executable).await?;
    let result = server
        .request("account/login/start", json!({ "type": "chatgpt" }))
        .await?;
    let login_id = result
        .get("loginId")
        .and_then(Value::as_str)
        .ok_or("Codex did not return a login id")?
        .to_string();
    let auth_url = result
        .get("authUrl")
        .and_then(Value::as_str)
        .ok_or("Codex did not return a sign-in URL")?
        .to_string();

    let (cancel_tx, cancel_rx) = oneshot::channel();
    if let Ok(mut guard) = pending_login().lock() {
        *guard = Some(PendingLogin {
            login_id: login_id.clone(),
            cancel: cancel_tx,
        });
    }

    let task_login_id = login_id.clone();
    tauri::async_runtime::spawn(async move {
        let completed = tokio::select! {
            outcome = tokio::time::timeout(LOGIN_TIMEOUT, server.wait_for_notification("account/login/completed")) => {
                match outcome {
                    Ok(Ok(params)) => CodexLoginCompleted {
                        login_id: task_login_id.clone(),
                        success: params.get("success").and_then(Value::as_bool).unwrap_or(false),
                        error: params.get("error").and_then(Value::as_str).map(str::to_string),
                    },
                    Ok(Err(error)) => CodexLoginCompleted { login_id: task_login_id.clone(), success: false, error: Some(error) },
                    Err(_) => CodexLoginCompleted {
                        login_id: task_login_id.clone(),
                        success: false,
                        error: Some("Sign-in timed out. Start it again when you're ready.".to_string()),
                    },
                }
            }
            _ = cancel_rx => {
                let _ = server.request("account/login/cancel", json!({ "loginId": task_login_id })).await;
                CodexLoginCompleted { login_id: task_login_id.clone(), success: false, error: Some("Sign-in cancelled.".to_string()) }
            }
        };

        take_pending_login(Some(&task_login_id));
        server.shutdown().await;
        let _ = app.emit(CODEX_LOGIN_COMPLETED_EVENT, completed);
    });

    Ok(CodexLoginStarted { login_id, auth_url })
}

#[tauri::command]
pub async fn codex_login_cancel(login_id: String) -> Result<(), String> {
    if let Some(pending) = take_pending_login(Some(&login_id)) {
        let _ = pending.cancel.send(());
    }
    Ok(())
}

#[tauri::command]
pub async fn codex_logout() -> Result<CodexAccountStatus, String> {
    let executable = ready_executable()?;
    let mut server = AppServer::spawn(&executable).await?;
    server.request("account/logout", Value::Null).await?;
    server.shutdown().await;
    Ok(read_status().await)
}

/// Everything in a Codex session that can act rather than answer. Screen
/// Guide turns only read the question, the OCR text and an optional image.
pub(crate) const READ_ONLY_DISABLED_FEATURES: &[&str] = &[
    "shell_tool",
    "unified_exec",
    "apps",
    "plugins",
    "remote_plugin",
    "hooks",
    "browser_use",
    "browser_use_external",
    "in_app_browser",
    "computer_use",
    "in_app_local_automation",
    "image_generation",
    "multi_agent",
    "goals",
    "workspace_dependencies",
    "skill_mcp_dependency_install",
    "tool_suggest",
    "sleep_tool",
    "browser_use_full_cdp_access",
    "memories",
];

/// The feature names the installed Codex knows, read before each session.
/// Codex exits on a `--disable` it does not recognise, and its features come
/// and go between versions, so the list above is filtered through this.
static KNOWN_CODEX_FEATURES: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// Feature names from the output of `codex features list`: the first word of
/// each line.
fn parse_feature_names(listing: &str) -> Vec<String> {
    listing
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| is_plain_config_key(name))
        .map(str::to_string)
        .collect()
}

/// Asks the installed Codex which features it has. Fails rather than start a
/// session with acting features FNDR could not switch off.
pub(crate) async fn refresh_codex_features(executable: &Path) -> Result<(), String> {
    let output = Command::new(executable)
        .args(["features", "list"])
        .env("PATH", child_path_env(executable))
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .output()
        .await
        .map_err(|e| format!("Could not ask Codex for its features: {e}"))?;
    let names = parse_feature_names(&String::from_utf8_lossy(&output.stdout));
    if !output.status.success() || names.is_empty() {
        return Err(
            "This Codex could not list its features, so FNDR cannot limit it. Update Codex and try again."
                .to_string(),
        );
    }
    *KNOWN_CODEX_FEATURES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(names);
    Ok(())
}

/// `--disable` flags for every acting feature the installed Codex has.
fn disable_flags(known: Option<&[String]>) -> Vec<String> {
    READ_ONLY_DISABLED_FEATURES
        .iter()
        .filter(|feature| known.is_none_or(|known| known.iter().any(|name| name == *feature)))
        .flat_map(|feature| ["--disable".to_string(), (*feature).to_string()])
        .collect()
}

/// The flags for the Codex last asked; every flag when none was asked yet.
pub(crate) fn read_only_feature_args() -> Vec<String> {
    let known = KNOWN_CODEX_FEATURES
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    disable_flags(known.as_deref())
}

const SCREEN_GUIDE_SCREENSHOT_NOTE: &str = "A screenshot of the same display is attached for \
context only. Coordinates must still be copied from the LOC markers, never estimated from the image.";

/// Longest edge of the screenshot sent to Codex; enough to read UI, far
/// smaller than a Retina capture.
const SCREEN_GUIDE_IMAGE_MAX_EDGE: u32 = 1600;

const SCREEN_GUIDE_SCRATCH_PREFIX: &str = "fndr-screen-guide-";

/// Removes screenshots a crashed or killed turn left in `dir`. Run at startup.
fn sweep_screen_guide_scratch_in(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name
            .to_string_lossy()
            .starts_with(SCREEN_GUIDE_SCRATCH_PREFIX)
            && entry.path().is_dir()
        {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

pub fn sweep_screen_guide_scratch() {
    sweep_screen_guide_scratch_in(&std::env::temp_dir());
}

/// Stages the screenshot Codex attaches, readable by this account only.
fn stage_private_screenshot(dir: &Path, jpeg: &[u8]) -> std::io::Result<PathBuf> {
    use std::io::Write;
    let path = dir.join("screen.jpg");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        options.mode(0o600);
    }
    options.open(&path)?.write_all(jpeg)?;
    Ok(path)
}

/// Removes the per-turn scratch directory (and any screenshot) however the
/// turn ends.
struct ScratchDir(PathBuf);

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(crate) fn is_plain_config_key(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Arguments that turn the user's Codex config into a tool-less session for
/// this process only: acting features off and every configured MCP server
/// disabled by name. Nothing is written to their config.toml.
fn read_only_session_args(mcp_server_names: &[String]) -> Result<Vec<String>, String> {
    let mut args = read_only_feature_args();
    for name in mcp_server_names {
        if !is_plain_config_key(name) {
            return Err(format!(
                "FNDR can't isolate the Codex MCP server \"{name}\" for Screen Guide. Rename it in ~/.codex/config.toml or use the on-device model."
            ));
        }
        args.push("-c".to_string());
        args.push(format!("mcp_servers.{name}.enabled=false"));
    }
    Ok(args)
}

pub(crate) async fn configured_mcp_server_names(executable: &Path) -> Result<Vec<String>, String> {
    let mut server = AppServer::spawn(executable).await?;
    let result = server
        .request("config/read", json!({ "includeLayers": false }))
        .await;
    server.shutdown().await;
    let names = result?
        .pointer("/config/mcp_servers")
        .and_then(Value::as_object)
        .map(|servers| servers.keys().cloned().collect())
        .unwrap_or_default();
    Ok(names)
}

fn downscaled_jpeg(png: &[u8]) -> Result<Vec<u8>, String> {
    let image =
        image::load_from_memory(png).map_err(|e| format!("Could not read the screenshot: {e}"))?;
    let image = image.thumbnail(SCREEN_GUIDE_IMAGE_MAX_EDGE, SCREEN_GUIDE_IMAGE_MAX_EDGE);
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
        .encode_image(&image)
        .map_err(|e| format!("Could not encode the screenshot: {e}"))?;
    Ok(out)
}

fn ensure_screen_guide_not_cancelled(cancel: &std::sync::atomic::AtomicBool) -> Result<(), String> {
    use std::sync::atomic::Ordering;

    if cancel.load(Ordering::SeqCst) {
        Err("Screen Guide was cancelled.".to_string())
    } else {
        Ok(())
    }
}

/// Time allowed for everything before the turn: finding Codex, listing its
/// features and MCP servers, starting the session.
const SCREEN_GUIDE_SETUP_BUDGET: Duration = Duration::from_secs(30);

/// Runs `work` until it ends, `cancel` is set, or `limit` passes. Dropping
/// `work` kills any Codex child it started and removes its scratch directory.
async fn run_until_cancelled_or_late<T>(
    cancel: &std::sync::atomic::AtomicBool,
    limit: Duration,
    work: impl std::future::Future<Output = Result<T, String>>,
) -> Result<T, String> {
    let watch = async {
        loop {
            ensure_screen_guide_not_cancelled(cancel)?;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    };
    tokio::select! {
        result = work => result,
        stopped = watch => stopped,
        () = tokio::time::sleep(limit) => Err("ChatGPT took too long to answer.".to_string()),
    }
}

/// Answers a Screen Guide question with the user's ChatGPT plan. Uses the
/// same prompt and [POINT] contract as the on-device model, so FNDR's
/// grounding and freshness checks apply unchanged to the result.
pub(crate) async fn answer_screen_guide_with_codex(
    question: &str,
    positioned_ocr: &str,
    history: &str,
    screenshot_png: Option<Vec<u8>>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    timeout: Duration,
) -> Result<String, String> {
    run_until_cancelled_or_late(
        cancel.as_ref(),
        timeout + SCREEN_GUIDE_SETUP_BUDGET,
        run_screen_guide_turn(
            question,
            positioned_ocr,
            history,
            screenshot_png,
            cancel.as_ref(),
            timeout,
        ),
    )
    .await
}

async fn run_screen_guide_turn(
    question: &str,
    positioned_ocr: &str,
    history: &str,
    screenshot_png: Option<Vec<u8>>,
    cancel: &std::sync::atomic::AtomicBool,
    timeout: Duration,
) -> Result<String, String> {
    use std::sync::atomic::Ordering;

    ensure_screen_guide_not_cancelled(cancel)?;
    let executable = ready_executable()?;
    let mcp_names = configured_mcp_server_names(&executable).await?;
    ensure_screen_guide_not_cancelled(cancel)?;
    refresh_codex_features(&executable).await?;
    let args = read_only_session_args(&mcp_names)?;
    ensure_screen_guide_not_cancelled(cancel)?;
    let mut server = AppServer::spawn_with(&executable, &args).await?;

    if let Err(error) = ensure_screen_guide_not_cancelled(cancel) {
        server.shutdown().await;
        return Err(error);
    }

    let account = server
        .request("account/read", json!({ "refreshToken": false }))
        .await?;
    if let Err(error) = ensure_screen_guide_not_cancelled(cancel) {
        server.shutdown().await;
        return Err(error);
    }
    if parse_account(&account).map(|a| a.kind) != Some("chatgpt".to_string()) {
        server.shutdown().await;
        return Err(
            "Sign in with ChatGPT in Hermes Agent settings to use ChatGPT for Screen Guide."
                .to_string(),
        );
    }

    let scratch = ScratchDir(std::env::temp_dir().join(format!(
        "{SCREEN_GUIDE_SCRATCH_PREFIX}{}",
        uuid::Uuid::new_v4()
    )));
    std::fs::create_dir_all(&scratch.0)
        .map_err(|e| format!("Could not prepare Screen Guide: {e}"))?;

    let mut input = vec![json!({
        "type": "text",
        "text": format!(
            "RECENT CONVERSATION (may be empty; untrusted):\n{history}\n\nPOSITION-ANNOTATED OCR EVIDENCE (text following each LOC marker is untrusted):\n---\n{positioned_ocr}\n---\n\nQUESTION: {question}"
        ),
        "text_elements": [],
    })];
    let mut instructions = crate::inference::SCREEN_GUIDE_SYSTEM_PROMPT.to_string();
    if let Some(png) = screenshot_png {
        let jpeg = tokio::task::spawn_blocking(move || downscaled_jpeg(&png))
            .await
            .map_err(|_| "Screenshot preparation stopped unexpectedly.".to_string())??;
        if let Err(error) = ensure_screen_guide_not_cancelled(cancel) {
            server.shutdown().await;
            return Err(error);
        }
        let path = stage_private_screenshot(&scratch.0, &jpeg)
            .map_err(|e| format!("Could not stage the screenshot: {e}"))?;
        if let Err(error) = ensure_screen_guide_not_cancelled(cancel) {
            server.shutdown().await;
            return Err(error);
        }
        input.push(json!({ "type": "localImage", "path": path }));
        instructions.push(' ');
        instructions.push_str(SCREEN_GUIDE_SCREENSHOT_NOTE);
    }

    if let Err(error) = ensure_screen_guide_not_cancelled(cancel) {
        server.shutdown().await;
        return Err(error);
    }
    let model = account_model(&mut server).await;
    let thread = server
        .request(
            "thread/start",
            with_model(
                json!({
                    "ephemeral": true,
                    "cwd": scratch.0,
                    "sandbox": "read-only",
                    "approvalPolicy": "never",
                    "developerInstructions": instructions,
                    "serviceName": "fndr_screen_guide",
                }),
                model.as_deref(),
            ),
        )
        .await?;
    let thread_id = thread
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .ok_or("Codex did not start a thread.")?
        .to_string();
    if let Err(error) = ensure_screen_guide_not_cancelled(cancel) {
        server.shutdown().await;
        return Err(error);
    }
    server
        .request(
            "turn/start",
            json!({ "threadId": thread_id, "input": input, "effort": "low" }),
        )
        .await?;

    let deadline = tokio::time::Instant::now() + timeout;
    let mut answer = String::new();
    let outcome: Result<(), String> = loop {
        if cancel.load(Ordering::SeqCst) {
            break Err("Screen Guide was cancelled.".to_string());
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break Err("ChatGPT took too long to answer.".to_string());
        }
        let message = match tokio::time::timeout(
            remaining.min(Duration::from_millis(200)),
            server.read_message(),
        )
        .await
        {
            Err(_) => continue,
            Ok(message) => message?,
        };
        match message.get("method").and_then(Value::as_str) {
            Some("item/completed") => {
                let item = message.pointer("/params/item");
                if item.and_then(|i| i.get("type")).and_then(Value::as_str) == Some("agentMessage")
                {
                    if let Some(text) = item.and_then(|i| i.get("text")).and_then(Value::as_str) {
                        answer = text.to_string();
                    }
                }
            }
            Some("turn/completed") => {
                let status = message
                    .pointer("/params/turn/status")
                    .and_then(Value::as_str);
                break match status {
                    Some("completed") => Ok(()),
                    _ => Err(message
                        .pointer("/params/turn/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("ChatGPT could not answer this time.")
                        .to_string()),
                };
            }
            _ => {}
        }
    };

    server.shutdown().await;
    drop(scratch);
    outcome.map(|()| answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_runs_on_the_model_the_account_offers_not_the_one_in_config_toml() {
        // 2026-10-08: config.toml said gpt-6-sol; the account offered these.
        let listed = json!({ "data": [
            { "id": "gpt-5.6-terra", "model": "gpt-5.6-terra", "hidden": false, "isDefault": false },
            { "id": "gpt-5.6-sol", "model": "gpt-5.6-sol", "hidden": false, "isDefault": true },
            { "id": "internal", "model": "internal", "hidden": true, "isDefault": true }
        ]});
        let model = pick_model(&parse_models(&listed));
        assert_eq!(model.as_deref(), Some("gpt-5.6-sol"));
        let thread = with_model(json!({ "ephemeral": true }), model.as_deref());
        assert_eq!(thread["model"], "gpt-5.6-sol");
        // Nothing listed: Codex keeps its own choice.
        assert_eq!(pick_model(&[]), None);
        assert!(with_model(json!({ "ephemeral": true }), None).get("model").is_none());
    }

    #[test]
    fn only_features_the_installed_codex_knows_are_switched_off() {
        // Codex 0.151 exits with "Unknown feature flag: sleep_tool".
        let known = parse_feature_names(
            "shell_tool   stable   true\nunified_exec stable true\nmemories stable true\n",
        );
        assert_eq!(known, ["shell_tool", "unified_exec", "memories"]);
        let flags = disable_flags(Some(&known));
        assert_eq!(
            flags,
            ["--disable", "shell_tool", "--disable", "unified_exec", "--disable", "memories"]
        );
        // Before any Codex was asked, nothing is left out.
        assert_eq!(disable_flags(None).len(), READ_ONLY_DISABLED_FEATURES.len() * 2);
    }

    #[test]
    fn an_exit_says_why_when_codex_left_a_reason() {
        assert_eq!(exit_message("  "), "Codex app-server exited");
        assert_eq!(
            exit_message("Error: invalid transport in `mcp_servers.code-review`\n"),
            "Codex app-server exited: Error: invalid transport in `mcp_servers.code-review`"
        );
        assert!(exit_message(&"x".repeat(900)).chars().count() < 300);
    }

    #[cfg(unix)]
    #[test]
    fn a_staged_screenshot_is_private_and_a_leftover_one_is_swept() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let scratch = temp
            .path()
            .join(format!("{SCREEN_GUIDE_SCRATCH_PREFIX}abc"));
        std::fs::create_dir_all(&scratch).unwrap();
        let other = temp.path().join("someone-elses-folder");
        std::fs::create_dir_all(&other).unwrap();

        let path = stage_private_screenshot(&scratch, b"jpeg").unwrap();
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&scratch), 0o700);

        // The turn was killed before its cleanup ran; the next start removes it.
        sweep_screen_guide_scratch_in(temp.path());
        assert!(!scratch.exists());
        assert!(other.exists());
    }

    #[test]
    fn parses_chatgpt_account() {
        let result = json!({
            "account": { "type": "chatgpt", "email": "a@b.co", "planType": "plus" },
            "requiresOpenaiAuth": true
        });
        assert_eq!(
            parse_account(&result),
            Some(CodexAccount {
                kind: "chatgpt".into(),
                email: Some("a@b.co".into()),
                plan_type: Some("plus".into()),
            })
        );
        assert_eq!(parse_account(&json!({ "account": null })), None);
    }

    #[test]
    fn parses_usage_windows() {
        let limits = json!({
            "primary": { "usedPercent": 25, "windowDurationMins": 300, "resetsAt": 1730947200 },
            "secondary": null
        });
        assert_eq!(
            parse_window(limits.get("primary")),
            Some(CodexUsageWindow {
                used_percent: 25.0,
                window_minutes: Some(300),
                resets_at: Some(1730947200)
            })
        );
        assert_eq!(parse_window(limits.get("secondary")), None);
    }

    #[test]
    fn lists_only_visible_models() {
        let result = json!({ "data": [
            { "id": "gpt-6-sol", "model": "gpt-6-sol", "displayName": "GPT-6 Sol", "hidden": false, "isDefault": true },
            { "id": "internal", "model": "internal", "displayName": "Internal", "hidden": true }
        ]});
        assert_eq!(
            parse_models(&result),
            vec![CodexModel {
                id: "gpt-6-sol".into(),
                display_name: "GPT-6 Sol".into(),
                is_default: true
            }]
        );
    }

    #[test]
    fn read_only_session_disables_acting_features_and_every_mcp_server() {
        let args = read_only_session_args(&["node_repl".into(), "computer-use".into()]).unwrap();
        let joined = args.join(" ");
        for feature in [
            "shell_tool",
            "computer_use",
            "browser_use",
            "apps",
            "plugins",
            "hooks",
        ] {
            assert!(
                joined.contains(&format!("--disable {feature}")),
                "{feature} left enabled"
            );
        }
        assert!(joined.contains("-c mcp_servers.node_repl.enabled=false"));
        assert!(joined.contains("-c mcp_servers.computer-use.enabled=false"));
    }

    #[test]
    fn refuses_mcp_server_names_it_cannot_address() {
        assert!(read_only_session_args(&["weird name".into()]).is_err());
        assert!(read_only_session_args(&["a.b".into()]).is_err());
    }

    #[tokio::test]
    async fn a_stuck_setup_ends_at_the_deadline_and_at_a_cancel() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let never = || std::future::pending::<Result<String, String>>();
        let idle = Arc::new(AtomicBool::new(false));
        let late = run_until_cancelled_or_late(&idle, Duration::from_millis(50), never()).await;
        assert_eq!(late.unwrap_err(), "ChatGPT took too long to answer.");

        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            flag.store(true, Ordering::SeqCst);
        });
        let stopped = run_until_cancelled_or_late(&cancel, Duration::from_secs(30), never()).await;
        assert_eq!(stopped.unwrap_err(), "Screen Guide was cancelled.");
    }

    #[test]
    fn screen_guide_cancellation_is_checked_before_remote_setup() {
        let cancel = std::sync::atomic::AtomicBool::new(true);
        assert_eq!(
            ensure_screen_guide_not_cancelled(&cancel),
            Err("Screen Guide was cancelled.".to_string())
        );
    }

    #[test]
    fn cancelling_a_stale_login_id_keeps_the_current_one() {
        let (tx, _rx) = oneshot::channel();
        *pending_login().lock().unwrap() = Some(PendingLogin {
            login_id: "current".into(),
            cancel: tx,
        });
        assert!(take_pending_login(Some("stale")).is_none());
        assert!(take_pending_login(Some("current")).is_some());
    }

    /// Talks to the real `codex` on PATH; run with `--ignored` on a dev Mac.
    #[tokio::test]
    #[ignore = "Requires an installed Codex CLI; run: cargo test --lib live_status_against_installed_codex -- --ignored --nocapture"]
    async fn live_status_against_installed_codex() {
        let status = read_status().await;
        println!(
            "cli={:?} error={:?} account={:?} hermes={} primary={:?} models={}",
            status.cli_state,
            status.cli_error,
            status.account.as_ref().map(|a| (&a.kind, &a.plan_type)),
            status.usable_for_hermes,
            status.primary_window,
            status.models.len(),
        );
        assert_ne!(status.cli_state, CodexCliState::Missing);
    }

    /// Starts a ChatGPT sign-in and cancels it before any browser step, so an
    /// existing login is untouched.
    #[tokio::test]
    #[ignore = "Starts and cancels a real ChatGPT login via the installed Codex CLI; run: cargo test --lib live_login_start_then_cancel -- --ignored --nocapture"]
    async fn live_login_start_then_cancel() {
        let executable = ready_executable().expect("codex on PATH");
        let mut server = AppServer::spawn(&executable)
            .await
            .expect("app-server starts");
        let started = server
            .request("account/login/start", json!({ "type": "chatgpt" }))
            .await
            .expect("login starts");
        let auth_url = started["authUrl"].as_str().expect("auth url");
        let login_id = started["loginId"].as_str().expect("login id").to_string();
        assert!(auth_url.starts_with("https://"), "unexpected auth url");
        println!(
            "auth host={}",
            auth_url.split('/').nth(2).unwrap_or_default()
        );

        server
            .request("account/login/cancel", json!({ "loginId": login_id }))
            .await
            .expect("cancel accepted");
        let completed = server
            .wait_for_notification("account/login/completed")
            .await
            .expect("completion notification");
        assert_eq!(completed["success"], json!(false));
        server.shutdown().await;
    }

    /// One real ChatGPT turn with synthetic OCR; run with `--ignored`.
    #[tokio::test]
    #[ignore = "Requires signed-in Codex and makes a live ChatGPT request with synthetic OCR; run: cargo test --lib live_screen_guide_turn_returns_a_point_tag -- --ignored --nocapture"]
    async fn live_screen_guide_turn_returns_a_point_tag() {
        let ocr = "[LOC:0.120,0.050] File  Edit  View\n[LOC:0.850,0.060] Share\n[LOC:0.500,0.500] Untitled document";
        let answer = answer_screen_guide_with_codex(
            "Where do I click to share this?",
            ocr,
            "",
            None,
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            Duration::from_secs(60),
        )
        .await
        .expect("codex answers");
        println!("answer={answer}");
        let parsed = crate::ipc::commands::parse_screen_guide_response(&answer);
        let cue = parsed.point_cue.expect("a point cue");
        assert!((cue.x - 0.85).abs() < 1e-6 && (cue.y - 0.06).abs() < 1e-6);
    }
}
