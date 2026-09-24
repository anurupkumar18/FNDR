//! Voice-driven computer use from the notch.
//!
//! One long-lived Codex app-server session per conversation, on the user's
//! ChatGPT plan, with open-computer-use (github.com/iFurySt/open-codex-computer-use)
//! attached as an MCP server for this process only. The user talks; Codex
//! narrates what it is about to do, then acts through accessibility tools.
//!
//! Safety model:
//! - Off unless Screen Guide's `operate_computer` setting is on.
//! - Codex's own shell, browser, computer-use, apps, plugins and hooks are
//!   disabled, and every MCP server from the user's config is switched off by
//!   name, so open-computer-use is the only way to touch the Mac.
//! - Every open-computer-use call arrives as an approval request. Read-only
//!   look-ups (listing apps, reading an app's UI tree) are granted
//!   automatically; anything that clicks, types, scrolls, drags or presses a
//!   key is surfaced to the notch and waits for the user's yes or no.
//! - Any other server-initiated request is declined.

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use super::codex_account::{
    configured_mcp_server_names, is_plain_config_key, parse_account, ready_executable, AppServer,
    READ_ONLY_DISABLED_FEATURES,
};
use crate::AppState;

pub const COMPUTER_USE_EVENT: &str = "computer-use://event";

/// Name of the open-computer-use server inside FNDR's Codex session.
const COMPUTER_SERVER: &str = "fndr_computer";

/// Tools that only observe. Granted without asking so a conversation doesn't
/// stall on "may I look at the screen" for every step.
const READ_ONLY_TOOLS: &[&str] = &["list_apps", "get_app_state"];

const OPERATOR_INSTRUCTIONS: &str = "\
You are FNDR's operator on the user's Mac. The user talks to you through the notch and \
everything you write is read aloud, so write the way you would speak: short sentences, no \
markdown, no lists, never more than 25 words per message. Before each action, say in one \
sentence what you are about to do. Operate the Mac only through the fndr_computer tools; \
look at an app with get_app_state before clicking in it. Never type passwords, payment \
details or one-time codes, and never change security or privacy settings; ask the user to \
do those themselves. Before anything irreversible, such as sending, deleting, buying or \
submitting, stop and ask the user to confirm in words. If the user redirects you mid-task, \
follow the new instruction. When finished, say what you did in one sentence.";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "kind")]
pub enum ComputerUseEvent {
    /// Session is ready for the first instruction.
    Ready,
    /// Something Codex said: `final` is true for the turn's answer.
    Message { text: String, r#final: bool },
    /// A tool call started.
    Action { item_id: String, tool: String, summary: String },
    /// A tool call finished.
    ActionDone { item_id: String, tool: String, ok: bool },
    /// An action needs the user's yes or no.
    Approval { request_key: String, tool: String, summary: String },
    /// The approval was settled (answered, or cleared by an interrupt).
    ApprovalResolved { request_key: String },
    /// A turn ended: `completed`, `interrupted` or `failed`.
    TurnDone { status: String, error: Option<String> },
    /// The session ended; `error` is set when it failed.
    Ended { error: Option<String> },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUseStatus {
    pub enabled: bool,
    pub codex_ready: bool,
    pub open_computer_use_path: Option<String>,
    pub active: bool,
}

enum Command {
    Say(String),
    Interrupt,
    Respond { request_key: String, approve: bool },
    Stop,
}

fn session() -> &'static Mutex<Option<mpsc::UnboundedSender<Command>>> {
    static SESSION: OnceLock<Mutex<Option<mpsc::UnboundedSender<Command>>>> = OnceLock::new();
    SESSION.get_or_init(|| Mutex::new(None))
}

fn send_command(command: Command) -> Result<(), String> {
    let guard = session().lock().map_err(|_| "Computer use is unavailable.".to_string())?;
    guard
        .as_ref()
        .ok_or_else(|| "Start a computer-use conversation first.".to_string())?
        .send(command)
        .map_err(|_| "The computer-use session has ended.".to_string())
}

pub(crate) fn detect_open_computer_use() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).map(|dir| dir.join("open-computer-use")).collect())
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join(".npm-global/bin/open-computer-use"));
        candidates.push(home.join(".local/bin/open-computer-use"));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/open-computer-use"));
    candidates.push(PathBuf::from("/usr/local/bin/open-computer-use"));
    candidates.into_iter().find(|candidate| candidate.is_file())
}

/// A spoken-length description of a tool call, e.g. `click "Send" in Mail`.
fn describe_tool_call(tool: &str, params: &Value) -> String {
    let text = |key: &str| params.get(key).and_then(Value::as_str).filter(|s| !s.is_empty());
    let app = text("app").or_else(|| text("bundle_id")).or_else(|| text("app_name"));
    let target = text("element").or_else(|| text("label")).or_else(|| text("title"));
    let in_app = app.map(|a| format!(" in {a}")).unwrap_or_default();
    match tool {
        "click" => match target {
            Some(t) => format!("click \"{t}\"{in_app}"),
            None => format!("click{in_app}"),
        },
        "type_text" => match text("text") {
            Some(t) => format!("type \"{}\"{in_app}", truncate(t, 60)),
            None => format!("type{in_app}"),
        },
        "press_key" => format!("press {}{in_app}", text("key").unwrap_or("a key")),
        "scroll" => format!("scroll{in_app}"),
        "drag" => format!("drag{in_app}"),
        "set_value" => format!("change a value{in_app}"),
        "perform_secondary_action" => format!("use a menu action{in_app}"),
        "get_app_state" => format!("look at{}", app.map(|a| format!(" {a}")).unwrap_or_else(|| " the app".into())),
        "list_apps" => "check which apps are open".to_string(),
        other => format!("run {other}{in_app}"),
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(max).collect::<String>())
    }
}

/// The tool an approval asks about: `Allow the … to run tool "click"?`.
fn tool_from_approval_message(message: &str) -> Option<String> {
    let start = message.find("run tool \"")? + "run tool \"".len();
    let end = message[start..].find('"')? + start;
    Some(message[start..end].to_string())
}

fn session_args(user_mcp_servers: &[String], open_computer_use: &std::path::Path) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    for feature in READ_ONLY_DISABLED_FEATURES {
        args.push("--disable".to_string());
        args.push((*feature).to_string());
    }
    for name in user_mcp_servers.iter().filter(|name| name.as_str() != COMPUTER_SERVER) {
        if !is_plain_config_key(name) {
            return Err(format!(
                "FNDR can't isolate the Codex MCP server \"{name}\". Rename it in ~/.codex/config.toml to use computer use."
            ));
        }
        args.push("-c".to_string());
        args.push(format!("mcp_servers.{name}.enabled=false"));
    }
    let command = serde_json::to_string(&open_computer_use.display().to_string()).map_err(|e| e.to_string())?;
    args.extend([
        "-c".to_string(),
        format!("mcp_servers.{COMPUTER_SERVER}.command={command}"),
        "-c".to_string(),
        format!("mcp_servers.{COMPUTER_SERVER}.args=[\"mcp\"]"),
        "-c".to_string(),
        format!("mcp_servers.{COMPUTER_SERVER}.default_tools_approval_mode=\"prompt\""),
    ]);
    Ok(args)
}

struct Session {
    app: AppHandle,
    server: AppServer,
    thread_id: String,
    active_turn: Option<String>,
    next_id: u64,
    /// Our outgoing request ids for turn/start, so we learn the turn id.
    pending_turn_starts: Vec<u64>,
    /// Approval request ids waiting on the user, keyed for the frontend.
    approvals: HashMap<String, Value>,
    /// Tool name + arguments of in-flight calls, by item id.
    tool_calls: HashMap<String, (String, Value)>,
}

impl Session {
    fn emit(&self, event: ComputerUseEvent) {
        let _ = self.app.emit(COMPUTER_USE_EVENT, event);
    }

    async fn send_request(&mut self, method: &str, params: Value) -> Result<u64, String> {
        self.next_id += 1;
        let id = self.next_id;
        self.server.write(json!({ "method": method, "id": id, "params": params })).await?;
        Ok(id)
    }

    async fn say(&mut self, text: String) -> Result<(), String> {
        let input = json!([{ "type": "text", "text": text, "text_elements": [] }]);
        match self.active_turn.clone() {
            Some(turn_id) => {
                self.send_request(
                    "turn/steer",
                    json!({ "threadId": self.thread_id, "input": input, "expectedTurnId": turn_id }),
                )
                .await?;
            }
            None => {
                let id = self
                    .send_request("turn/start", json!({ "threadId": self.thread_id, "input": input, "effort": "low" }))
                    .await?;
                self.pending_turn_starts.push(id);
            }
        }
        Ok(())
    }

    async fn interrupt(&mut self) -> Result<(), String> {
        if let Some(turn_id) = self.active_turn.clone() {
            self.send_request("turn/interrupt", json!({ "threadId": self.thread_id, "turnId": turn_id }))
                .await?;
        }
        for key in self.approvals.keys().cloned().collect::<Vec<_>>() {
            self.answer_approval(&key, false).await?;
        }
        Ok(())
    }

    async fn answer_approval(&mut self, request_key: &str, approve: bool) -> Result<(), String> {
        let Some(id) = self.approvals.remove(request_key) else {
            return Ok(());
        };
        let result = if approve {
            json!({ "action": "accept", "content": {} })
        } else {
            json!({ "action": "decline", "content": null })
        };
        self.server.write(json!({ "id": id, "result": result })).await?;
        self.emit(ComputerUseEvent::ApprovalResolved { request_key: request_key.to_string() });
        Ok(())
    }

    async fn handle_server_request(&mut self, id: Value, method: &str, params: &Value) -> Result<(), String> {
        let is_tool_approval = method == "mcpServer/elicitation/request"
            && params.get("serverName").and_then(Value::as_str) == Some(COMPUTER_SERVER)
            && params.pointer("/_meta/codex_approval_kind").and_then(Value::as_str) == Some("mcp_tool_call");
        if !is_tool_approval {
            tracing::warn!(%method, "computer_use:declined_server_request");
            return self
                .server
                .write(json!({ "id": id, "error": { "code": -32000, "message": "FNDR does not grant this request." } }))
                .await;
        }

        let tool = params
            .get("message")
            .and_then(Value::as_str)
            .and_then(tool_from_approval_message)
            .unwrap_or_else(|| "tool".to_string());
        let tool_params = params.pointer("/_meta/tool_params").cloned().unwrap_or(Value::Null);
        let request_key = uuid::Uuid::new_v4().to_string();
        self.approvals.insert(request_key.clone(), id);

        if READ_ONLY_TOOLS.contains(&tool.as_str()) {
            return self.answer_approval(&request_key, true).await;
        }
        self.emit(ComputerUseEvent::Approval {
            request_key,
            summary: describe_tool_call(&tool, &tool_params),
            tool,
        });
        Ok(())
    }

    fn handle_response(&mut self, id: u64, message: &Value) {
        if let Some(position) = self.pending_turn_starts.iter().position(|pending| *pending == id) {
            self.pending_turn_starts.remove(position);
            if let Some(turn_id) = message.pointer("/result/turn/id").and_then(Value::as_str) {
                self.active_turn = Some(turn_id.to_string());
            }
        }
        if let Some(error) = message.get("error") {
            let detail = error.get("message").and_then(Value::as_str).unwrap_or("Codex refused that request.");
            tracing::warn!(detail, "computer_use:request_failed");
        }
    }

    fn handle_notification(&mut self, method: &str, params: &Value) {
        match method {
            "turn/started" => {
                if let Some(turn_id) = params.pointer("/turn/id").and_then(Value::as_str) {
                    self.active_turn = Some(turn_id.to_string());
                }
            }
            "item/started" => {
                let item = &params["item"];
                if item.get("type").and_then(Value::as_str) == Some("mcpToolCall") {
                    let item_id = item.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                    let tool = item.get("tool").and_then(Value::as_str).unwrap_or("tool").to_string();
                    let arguments = item.get("arguments").cloned().unwrap_or(Value::Null);
                    self.emit(ComputerUseEvent::Action {
                        item_id: item_id.clone(),
                        summary: describe_tool_call(&tool, &arguments),
                        tool: tool.clone(),
                    });
                    self.tool_calls.insert(item_id, (tool, arguments));
                }
            }
            "item/completed" => {
                let item = &params["item"];
                match item.get("type").and_then(Value::as_str) {
                    Some("agentMessage") => {
                        let text = item.get("text").and_then(Value::as_str).unwrap_or_default().trim().to_string();
                        if !text.is_empty() {
                            let r#final = item.get("phase").and_then(Value::as_str) == Some("final_answer");
                            self.emit(ComputerUseEvent::Message { text, r#final });
                        }
                    }
                    Some("mcpToolCall") => {
                        let item_id = item.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                        let (tool, _) = self.tool_calls.remove(&item_id).unwrap_or_default();
                        let ok = item.get("status").and_then(Value::as_str) == Some("completed")
                            && item.get("error").map_or(true, Value::is_null);
                        self.emit(ComputerUseEvent::ActionDone { item_id, tool, ok });
                    }
                    _ => {}
                }
            }
            "turn/completed" => {
                self.active_turn = None;
                self.tool_calls.clear();
                let status = params.pointer("/turn/status").and_then(Value::as_str).unwrap_or("completed").to_string();
                let error = params
                    .pointer("/turn/error/message")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                self.emit(ComputerUseEvent::TurnDone { status, error });
            }
            "serverRequest/resolved" => {
                // An interrupt can clear a pending approval server-side.
                let resolved = params.get("requestId").cloned();
                if let Some(resolved) = resolved {
                    let keys: Vec<String> = self
                        .approvals
                        .iter()
                        .filter(|(_, id)| **id == resolved)
                        .map(|(key, _)| key.clone())
                        .collect();
                    for key in keys {
                        self.approvals.remove(&key);
                        self.emit(ComputerUseEvent::ApprovalResolved { request_key: key });
                    }
                }
            }
            _ => {}
        }
    }

    async fn run(mut self, mut commands: mpsc::UnboundedReceiver<Command>) -> Result<(), String> {
        loop {
            tokio::select! {
                command = commands.recv() => match command {
                    Some(Command::Say(text)) => self.say(text).await?,
                    Some(Command::Interrupt) => self.interrupt().await?,
                    Some(Command::Respond { request_key, approve }) => self.answer_approval(&request_key, approve).await?,
                    Some(Command::Stop) | None => {
                        let _ = self.interrupt().await;
                        return Ok(());
                    }
                },
                message = self.server.read_raw() => {
                    let message = message?;
                    match (message.get("id").cloned(), message.get("method").and_then(Value::as_str).map(str::to_string)) {
                        (Some(id), Some(method)) => {
                            let params = message.get("params").cloned().unwrap_or(Value::Null);
                            self.handle_server_request(id, &method, &params).await?;
                        }
                        (Some(id), None) => {
                            if let Some(id) = id.as_u64() {
                                self.handle_response(id, &message);
                            }
                        }
                        (None, Some(method)) => {
                            let params = message.get("params").cloned().unwrap_or(Value::Null);
                            self.handle_notification(&method, &params);
                        }
                        (None, None) => {}
                    }
                }
            }
        }
    }
}

async fn open_session(app: AppHandle) -> Result<Session, String> {
    let codex = ready_executable()?;
    let computer = detect_open_computer_use().ok_or_else(|| {
        "Computer use needs open-computer-use. Install it with `npm install -g open-computer-use`, then run `open-computer-use doctor` once to grant Accessibility and Screen Recording.".to_string()
    })?;
    let user_servers = configured_mcp_server_names(&codex).await?;
    let args = session_args(&user_servers, &computer)?;
    let mut server = AppServer::spawn_with(&codex, &args).await?;

    let account = server.request("account/read", json!({ "refreshToken": false })).await?;
    if parse_account(&account).map(|a| a.kind) != Some("chatgpt".to_string()) {
        server.shutdown().await;
        return Err("Sign in with ChatGPT in Hermes Agent settings to use computer use.".to_string());
    }

    let cwd = std::env::temp_dir().join("fndr-computer-use");
    std::fs::create_dir_all(&cwd).map_err(|e| format!("Could not prepare computer use: {e}"))?;
    let thread = server
        .request(
            "thread/start",
            json!({
                "ephemeral": true,
                "cwd": cwd,
                "sandbox": "read-only",
                "approvalPolicy": "on-request",
                "developerInstructions": OPERATOR_INSTRUCTIONS,
                "serviceName": "fndr_computer_use",
            }),
        )
        .await?;
    let thread_id = thread
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .ok_or("Codex did not start a conversation.")?
        .to_string();

    Ok(Session {
        app,
        server,
        thread_id,
        active_turn: None,
        next_id: 1_000,
        pending_turn_starts: Vec::new(),
        approvals: HashMap::new(),
        tool_calls: HashMap::new(),
    })
}

#[tauri::command]
pub async fn computer_use_status(state: State<'_, std::sync::Arc<AppState>>) -> Result<ComputerUseStatus, String> {
    let enabled = state.inner().config.read().screen_guide.operate_computer;
    let active = session().lock().map(|guard| guard.is_some()).unwrap_or(false);
    Ok(ComputerUseStatus {
        enabled,
        codex_ready: ready_executable().is_ok(),
        open_computer_use_path: detect_open_computer_use().map(|p| p.display().to_string()),
        active,
    })
}

/// Opens a conversation (if none is open) and sends the user's words.
#[tauri::command]
pub async fn computer_use_say(
    app: AppHandle,
    state: State<'_, std::sync::Arc<AppState>>,
    text: String,
) -> Result<(), String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Ok(());
    }
    if !state.inner().config.read().screen_guide.operate_computer {
        return Err("Turn on \"Operate my Mac\" in Screen Guide settings first.".to_string());
    }
    if state.inner().is_incognito.load(std::sync::atomic::Ordering::SeqCst) {
        return Err("Computer use is paused while FNDR is private.".to_string());
    }

    let has_session = session().lock().map(|guard| guard.is_some()).unwrap_or(false);
    if !has_session {
        let opened = open_session(app.clone()).await?;
        let (tx, rx) = mpsc::unbounded_channel();
        if let Ok(mut guard) = session().lock() {
            *guard = Some(tx);
        }
        let _ = app.emit(COMPUTER_USE_EVENT, ComputerUseEvent::Ready);
        let task_app = app.clone();
        tauri::async_runtime::spawn(async move {
            let result = opened.run(rx).await;
            if let Ok(mut guard) = session().lock() {
                *guard = None;
            }
            let _ = task_app.emit(COMPUTER_USE_EVENT, ComputerUseEvent::Ended { error: result.err() });
        });
    }
    send_command(Command::Say(text))
}

#[tauri::command]
pub async fn computer_use_interrupt() -> Result<(), String> {
    send_command(Command::Interrupt)
}

#[tauri::command]
pub async fn computer_use_respond(request_key: String, approve: bool) -> Result<(), String> {
    send_command(Command::Respond { request_key, approve })
}

#[tauri::command]
pub async fn computer_use_stop() -> Result<(), String> {
    let sender = session().lock().ok().and_then(|mut guard| guard.take());
    if let Some(sender) = sender {
        let _ = sender.send(Command::Stop);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_actions_in_spoken_length() {
        assert_eq!(
            describe_tool_call("click", &json!({ "app": "Mail", "element": "Send" })),
            "click \"Send\" in Mail"
        );
        assert_eq!(describe_tool_call("press_key", &json!({ "key": "cmd+s" })), "press cmd+s");
        assert_eq!(describe_tool_call("list_apps", &json!({})), "check which apps are open");
        let long = "x".repeat(200);
        assert!(describe_tool_call("type_text", &json!({ "text": long })).chars().count() < 80);
    }

    #[test]
    fn reads_the_tool_out_of_an_approval_message() {
        assert_eq!(
            tool_from_approval_message("Allow the fndr_computer MCP server to run tool \"click\"?"),
            Some("click".to_string())
        );
        assert_eq!(tool_from_approval_message("Something else"), None);
    }

    #[test]
    fn session_attaches_only_open_computer_use_and_prompts_for_it() {
        let args = session_args(
            &["node_repl".into(), "computer-use".into()],
            std::path::Path::new("/opt/homebrew/bin/open-computer-use"),
        )
        .unwrap()
        .join(" ");
        assert!(args.contains("--disable computer_use"), "Codex's own computer use stays off");
        assert!(args.contains("--disable shell_tool"));
        assert!(args.contains("mcp_servers.node_repl.enabled=false"));
        assert!(args.contains("mcp_servers.computer-use.enabled=false"));
        assert!(args.contains("mcp_servers.fndr_computer.command=\"/opt/homebrew/bin/open-computer-use\""));
        assert!(args.contains("mcp_servers.fndr_computer.default_tools_approval_mode=\"prompt\""));
    }

    #[test]
    fn events_serialize_in_the_shape_the_notch_reads() {
        let action = serde_json::to_value(ComputerUseEvent::ActionDone {
            item_id: "i1".into(),
            tool: "click".into(),
            ok: true,
        })
        .unwrap();
        assert_eq!(action, json!({ "kind": "actionDone", "itemId": "i1", "tool": "click", "ok": true }));
        let message = serde_json::to_value(ComputerUseEvent::Message { text: "Done.".into(), r#final: true }).unwrap();
        assert_eq!(message, json!({ "kind": "message", "text": "Done.", "final": true }));
    }

    #[test]
    fn only_observation_tools_skip_the_approval_prompt() {
        for tool in ["click", "type_text", "press_key", "scroll", "drag", "set_value", "perform_secondary_action"] {
            assert!(!READ_ONLY_TOOLS.contains(&tool), "{tool} must ask first");
        }
    }
}
