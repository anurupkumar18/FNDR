//! Notch Do: a spoken request becomes a plan, then runs step by step on the
//! Mac (ADR-020, ADR-022 and ADR-018 amendments of 2026-10-06).
//!
//! One `codex app-server` per run, on the person's ChatGPT sign-in, with a
//! computer-use MCP server attached for this process only: OpenAI's bundled
//! Computer Use when it is installed, otherwise open-computer-use.
//!
//! - A planner turn returns the steps as JSON. Opening apps and links is done
//!   by FNDR natively; only `operate` steps hand an app's UI to Codex.
//! - Every computer-use call arrives as an approval request and is decided by
//!   `operator::policy` from the call itself and FNDR's own view of the UI:
//!   it runs, waits for the person's yes, or is refused.
//! - Every step is checked by FNDR (`operator::plan::verify`), retried once,
//!   and every action is written to the local journal.
//! - Stop aborts the run and kills Codex's whole process group, so an action
//!   in flight dies with it.

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::mpsc;

use super::codex_account::{
    configured_mcp_server_names, is_plain_config_key, parse_account, ready_executable, AppServer,
    account_model, read_only_feature_args, refresh_codex_features, with_model,
};
use crate::inference::prompts::{OPERATOR_PLANNER_SYSTEM, OPERATOR_STEP_SYSTEM};
use crate::operator::journal::{redact, Journal, JournalEntry};
use crate::operator::plan::{self, Plan, PlanStep, StepAction, StepCheck};
use crate::operator::policy::{classify, Decision, Observed, Risk};
use crate::operator::{memory, native};
use crate::workset::{ItemOutcome, Resolution, WorkItem, WorkSet};
use crate::AppState;

pub const COMPUTER_USE_EVENT: &str = "computer-use://event";

/// Name of the computer-use server inside FNDR's Codex session.
const COMPUTER_SERVER: &str = "fndr_computer";

/// Where Codex sends model requests on the ChatGPT plan.
const MODEL_HOST: &str = "chatgpt.com";

const PLAN_TIMEOUT: Duration = Duration::from_secs(90);
const STEP_TIMEOUT: Duration = Duration::from_secs(180);
const FRONTMOST_TIMEOUT: Duration = Duration::from_secs(6);
const PAGE_TIMEOUT: Duration = Duration::from_secs(10);
/// How often a run in progress rechecks the kill switch and Private Mode.
const HALT_CHECK_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StepView {
    pub label: String,
    pub action: StepAction,
    pub app: String,
    /// What a `reopen_memory` step opens, for the card and the narration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<WorkItem>,
}

fn step_views(plan: &Plan) -> Vec<StepView> {
    plan.steps
        .iter()
        .map(|step| StepView {
            label: step.label.clone(),
            action: step.action,
            app: step.app.clone(),
            item: step.item.clone(),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum ComputerUseEvent {
    Planning {
        run_id: String,
        used_memories: usize,
    },
    Planned {
        run_id: String,
        steps: Vec<StepView>,
        /// The plan may start by itself: no step in it can need a yes.
        auto_start: bool,
    },
    /// A work-set request matched more than one thread about equally; the
    /// person picks one (ADR 027). The run ends here.
    Choose {
        run_id: String,
        options: Vec<WorkSet>,
    },
    StepStarted {
        run_id: String,
        index: usize,
        attempt: u8,
    },
    Action {
        run_id: String,
        index: usize,
        item_id: String,
        tool: String,
        summary: String,
        risk: Risk,
    },
    ActionDone {
        run_id: String,
        item_id: String,
        ok: bool,
    },
    Approval {
        run_id: String,
        request_key: String,
        tool: String,
        summary: String,
    },
    ApprovalResolved {
        run_id: String,
        request_key: String,
    },
    Blocked {
        run_id: String,
        index: usize,
        tool: String,
        summary: String,
        reason: String,
    },
    StepDone {
        run_id: String,
        index: usize,
        ok: bool,
        detail: String,
        /// FNDR saw the result itself; false means it is the model's report.
        checked: bool,
    },
    Finished {
        run_id: String,
        ok: bool,
        summary: String,
    },
    Stopped {
        run_id: String,
    },
    /// `reconnect` is true when the ChatGPT sign-in has to be redone.
    Failed {
        run_id: String,
        error: String,
        reconnect: bool,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerUseStatus {
    pub enabled: bool,
    pub codex_ready: bool,
    pub backend: Option<String>,
    pub backend_path: Option<String>,
    pub active_run: Option<String>,
}

#[derive(Debug)]
enum RunCommand {
    Start,
    Respond { request_key: String, approve: bool },
}

#[derive(Debug, PartialEq)]
enum RunError {
    Stopped,
    Failed(String),
    /// The ChatGPT sign-in is missing or expired.
    SignedOut(String),
}

struct RunHandle {
    run_id: String,
    commands: mpsc::UnboundedSender<RunCommand>,
    task: tauri::async_runtime::JoinHandle<()>,
    /// Codex's process group, killed on Stop.
    codex_pid: Arc<Mutex<Option<u32>>>,
}

fn current_run() -> &'static Mutex<Option<RunHandle>> {
    static RUN: OnceLock<Mutex<Option<RunHandle>>> = OnceLock::new();
    RUN.get_or_init(|| Mutex::new(None))
}

// MARK: - Backends

/// Shown when FNDR's own executor cannot start and no other helper is
/// installed. The ChatGPT app's current Computer Use is one run-any-code tool,
/// which FNDR cannot decide action by action, so FNDR does not attach it (ADR 026).
const NO_HELPER: &str = "Notch Do could not start its own click-and-type tools and found no other helper. Reinstall FNDR, or install open-computer-use; the ChatGPT app's Computer Use is not supported.";

/// Which computer-use MCP server the run attaches.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Backend {
    /// FNDR's own binary, run as `fndr operator-mcp` (ADR 026).
    Native(PathBuf),
    /// OpenAI's Computer Use plugin, installed with the ChatGPT/Codex app.
    CodexBundled(PathBuf),
    OpenComputerUse(PathBuf),
}

impl Backend {
    pub(crate) fn label(&self) -> &'static str {
        match self {
            Backend::Native(_) => "fndr_native",
            Backend::CodexBundled(_) => "codex_computer_use",
            Backend::OpenComputerUse(_) => "open_computer_use",
        }
    }

    pub(crate) fn path(&self) -> &Path {
        match self {
            Backend::Native(path)
            | Backend::CodexBundled(path)
            | Backend::OpenComputerUse(path) => path,
        }
    }

    /// The argument that makes the program speak MCP on stdio.
    pub(crate) fn mcp_arg(&self) -> &'static str {
        match self {
            Backend::Native(_) => "operator-mcp",
            _ => "mcp",
        }
    }
}

/// FNDR's own executable, which serves the tools itself.
fn detect_native() -> Option<PathBuf> {
    std::env::current_exe().ok().filter(|path| path.is_file())
}

/// The newest installed `computer-use-client-launcher` under a Codex home.
fn bundled_computer_use(codex_home: &Path) -> Option<PathBuf> {
    let root = codex_home.join("plugins/cache/openai-bundled/computer-use");
    let mut versions: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .collect();
    versions.sort_by_key(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| {
                name.split('.')
                    .map(|part| part.parse::<u64>().unwrap_or(0))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    });
    versions
        .into_iter()
        .rev()
        .map(|version| version.join("bin/computer-use-client-launcher"))
        .find(|launcher| launcher.is_file())
}

pub(crate) fn detect_open_computer_use() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join("open-computer-use"))
                .collect()
        })
        .unwrap_or_default();
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join(".npm-global/bin/open-computer-use"));
        candidates.push(home.join(".local/bin/open-computer-use"));
    }
    candidates.push(PathBuf::from("/opt/homebrew/bin/open-computer-use"));
    candidates.push(PathBuf::from("/usr/local/bin/open-computer-use"));
    candidates.into_iter().find(|candidate| candidate.is_file())
}

/// Set when the bundled Computer Use was refused Automation access, so the
/// next run falls back to open-computer-use instead of failing the same way.
static BUNDLED_REFUSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const AUTOMATION_REFUSED: &str = "Computer Use was refused Automation access. Allow FNDR under System Settings > Privacy & Security > Automation, then try again; the next run uses open-computer-use if it is installed.";

/// Picks the computer-use server. FNDR's own comes first; the third-party
/// helpers stay as fallbacks until the twenty-task set passes (ADR 026, item 5).
/// `forced` is `FNDR_COMPUTER_USE`, a fixed choice for diagnosis.
fn choose_backend(
    forced: Option<&str>,
    native: Option<PathBuf>,
    bundled: impl FnOnce() -> Option<PathBuf>,
    open_computer_use: impl FnOnce() -> Option<PathBuf>,
) -> Option<Backend> {
    match forced {
        Some("fndr_native") => return native.map(Backend::Native),
        Some("open_computer_use") => return open_computer_use().map(Backend::OpenComputerUse),
        Some("codex_computer_use") => return bundled().map(Backend::CodexBundled),
        _ => {}
    }
    native
        .map(Backend::Native)
        .or_else(|| bundled().map(Backend::CodexBundled))
        .or_else(|| open_computer_use().map(Backend::OpenComputerUse))
}

pub(crate) fn detect_backend() -> Option<Backend> {
    choose_backend(
        std::env::var("FNDR_COMPUTER_USE").ok().as_deref(),
        detect_native(),
        || {
            (!BUNDLED_REFUSED.load(std::sync::atomic::Ordering::Relaxed))
                .then(|| bundled_computer_use(&super::codex_account::codex_home_dir()))
                .flatten()
        },
        detect_open_computer_use,
    )
}

fn session_args(user_mcp_servers: &[String], backend: &Backend) -> Result<Vec<String>, String> {
    let mut args = read_only_feature_args();
    for name in user_mcp_servers
        .iter()
        .filter(|name| name.as_str() != COMPUTER_SERVER)
    {
        if !is_plain_config_key(name) {
            return Err(format!(
                "FNDR can't isolate the Codex MCP server \"{name}\". Rename it in ~/.codex/config.toml to use Notch Do."
            ));
        }
        args.push("-c".to_string());
        args.push(format!("mcp_servers.{name}.enabled=false"));
    }
    let command =
        serde_json::to_string(&backend.path().display().to_string()).map_err(|e| e.to_string())?;
    args.extend([
        "-c".to_string(),
        format!("mcp_servers.{COMPUTER_SERVER}.command={command}"),
        "-c".to_string(),
        format!(
            "mcp_servers.{COMPUTER_SERVER}.args=[\"{}\"]",
            backend.mcp_arg()
        ),
        "-c".to_string(),
        format!("mcp_servers.{COMPUTER_SERVER}.default_tools_approval_mode=\"prompt\""),
    ]);
    Ok(args)
}

// MARK: - Describing actions

/// A short description of a tool call for the notch, e.g. `click "Play" in Spotify`.
fn describe_tool_call(tool: &str, params: &Value, observed: &Observed) -> String {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };
    let app = text("app");
    let in_app = app.map(|a| format!(" in {a}")).unwrap_or_default();
    let target = app
        .zip(text("element_index"))
        .and_then(|(app, index)| observed.describe(app, index))
        .map(|label| format!(" \"{}\"", truncate(&label, 40)))
        .unwrap_or_default();
    match tool {
        "click" => format!("click{target}{in_app}"),
        "type_text" => format!(
            "type \"{}\"{in_app}",
            truncate(text("text").unwrap_or_default(), 40)
        ),
        "set_value" => format!("set{target}{in_app}"),
        "press_key" => format!("press {}{in_app}", text("key").unwrap_or("a key")),
        "scroll" => format!("scroll{in_app}"),
        "drag" => format!("drag{in_app}"),
        "perform_secondary_action" => format!(
            "{}{target}{in_app}",
            text("action").unwrap_or("menu action")
        ),
        "get_app_state" => format!("look at {}", app.unwrap_or("the app")),
        "list_apps" => "check which apps are open".to_string(),
        "select_text" => format!("select text{in_app}"),
        other => format!("{other}{in_app}"),
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

fn looks_signed_out(message: &str) -> bool {
    let lower = message.to_lowercase();
    [
        "401",
        "unauthorized",
        "not logged in",
        "log in",
        "login",
        "sign in",
        "refresh token",
        "expired",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

// MARK: - One Codex session

/// Limits on a whole run that do not depend on one action's risk.
#[derive(Clone)]
struct Guards {
    /// Why nothing may run right now: actions are switched off, or FNDR is private.
    halt: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    /// Whether an app may not be read or operated: the person's blocklist and FNDR itself.
    off_limits: Arc<dyn Fn(&str) -> bool + Send + Sync>,
}

const ACTIONS_OFF: &str = "Actions are turned off in Settings.";
const PRIVATE_MODE: &str = "Notch Do is paused while FNDR is private.";
const CODEX_UNREADABLE: &str = "Notch Do could not read an action from this version of Codex, so nothing was run. Update FNDR or Codex, then try again.";

impl Guards {
    fn for_state(state: &Arc<AppState>) -> Self {
        let halt_state = state.clone();
        let limits_state = state.clone();
        Self {
            halt: Arc::new(move || {
                if halt_state.config.read().actions_kill_switch {
                    Some(ACTIONS_OFF.to_string())
                } else if halt_state
                    .is_incognito
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    Some(PRIVATE_MODE.to_string())
                } else {
                    None
                }
            }),
            off_limits: Arc::new(move |app| {
                let app = app.trim();
                // An empty name matches every blocklist entry.
                !app.is_empty()
                    && (crate::privacy::Blocklist::is_internal_app(app, Some(app))
                        || crate::privacy::Blocklist::is_blocked(
                            app,
                            &limits_state.config.read().blocklist,
                        ))
            }),
        }
    }

    #[cfg(test)]
    fn open() -> Self {
        Self {
            halt: Arc::new(|| None),
            off_limits: Arc::new(|_| false),
        }
    }
}

struct TurnContext<'a> {
    run_id: &'a str,
    step: Option<usize>,
    guards: &'a Guards,
    observed: &'a mut Observed,
    journal: &'a Journal,
    emit: &'a (dyn Fn(ComputerUseEvent) + Send + Sync),
    commands: &'a mut mpsc::UnboundedReceiver<RunCommand>,
}

struct PendingApproval {
    request_id: Value,
    tool: String,
    args: Value,
    risk: Risk,
}

pub(crate) struct Session {
    server: AppServer,
    next_id: u64,
    planner_thread: String,
    operator_thread: String,
    /// The person said no to an action during the turn in progress.
    declined: bool,
    /// What the operator said about the last operate step it finished.
    last_report: Option<String>,
}

impl Session {
    async fn open(codex: &Path, args: &[String]) -> Result<Self, RunError> {
        let mut server = AppServer::spawn_with(codex, args)
            .await
            .map_err(RunError::Failed)?;
        let account = server
            .request("account/read", json!({ "refreshToken": false }))
            .await
            .map_err(RunError::Failed)?;
        if parse_account(&account).map(|a| a.kind) != Some("chatgpt".to_string()) {
            server.shutdown().await;
            return Err(RunError::SignedOut(
                "Sign in with ChatGPT to use Notch Do.".to_string(),
            ));
        }
        let cwd = std::env::temp_dir().join("fndr-computer-use");
        std::fs::create_dir_all(&cwd)
            .map_err(|e| RunError::Failed(format!("Could not prepare Notch Do: {e}")))?;
        let model = account_model(&mut server).await;
        let thread = |instructions: &'static str, service: &'static str| {
            with_model(
                json!({
                    "ephemeral": true,
                    "cwd": cwd,
                    "sandbox": "read-only",
                    "approvalPolicy": "on-request",
                    "developerInstructions": instructions,
                    "serviceName": service,
                }),
                model.as_deref(),
            )
        };
        let planner = server
            .request(
                "thread/start",
                thread(OPERATOR_PLANNER_SYSTEM, "fndr_notch_do_plan"),
            )
            .await;
        let operator = server
            .request(
                "thread/start",
                thread(OPERATOR_STEP_SYSTEM, "fndr_notch_do"),
            )
            .await;
        let id = |result: Result<Value, String>| -> Result<String, RunError> {
            result
                .map_err(RunError::Failed)?
                .pointer("/thread/id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| RunError::Failed("Codex did not start a conversation.".to_string()))
        };
        Ok(Session {
            planner_thread: id(planner)?,
            operator_thread: id(operator)?,
            server,
            next_id: 1_000,
            declined: false,
            last_report: None,
        })
    }

    async fn send_request(&mut self, method: &str, params: Value) -> Result<u64, RunError> {
        self.next_id += 1;
        let id = self.next_id;
        self.server
            .write(json!({ "method": method, "id": id, "params": params }))
            .await
            .map_err(RunError::Failed)?;
        Ok(id)
    }

    async fn answer(&mut self, request_id: Value, approve: bool) -> Result<(), RunError> {
        let result = if approve {
            json!({ "action": "accept", "content": {} })
        } else {
            json!({ "action": "decline", "content": null })
        };
        self.server
            .write(json!({ "id": request_id, "result": result }))
            .await
            .map_err(RunError::Failed)
    }

    /// Runs one turn on `thread_id` and returns the final answer's text.
    /// Computer-use approval requests are decided by the policy as they arrive.
    async fn run_turn(
        &mut self,
        ctx: &mut TurnContext<'_>,
        thread_id: &str,
        text: &str,
        schema: Value,
    ) -> Result<String, RunError> {
        let input = json!([{ "type": "text", "text": text, "text_elements": [] }]);
        let turn_request = self
            .send_request(
                "turn/start",
                json!({ "threadId": thread_id, "input": input, "effort": "low", "outputSchema": schema }),
            )
            .await?;
        let mut final_text = String::new();
        let mut pending: HashMap<String, PendingApproval> = HashMap::new();
        let mut calls: HashMap<String, (String, Value)> = HashMap::new();
        let index = ctx.step.unwrap_or(0);

        loop {
            tokio::select! {
                command = ctx.commands.recv() => match command {
                    Some(RunCommand::Respond { request_key, approve }) => {
                        if let Some(approval) = pending.remove(&request_key) {
                            self.declined |= !approve;
                            self.answer(approval.request_id, approve).await?;
                            journal(ctx, &approval.tool, &approval.args, Some(approval.risk), if approve { "approved" } else { "declined" }, None);
                            (ctx.emit)(ComputerUseEvent::ApprovalResolved { run_id: ctx.run_id.to_string(), request_key });
                        }
                    }
                    Some(RunCommand::Start) => {}
                    None => return Err(RunError::Stopped),
                },
                message = self.server.read_raw() => {
                    let message = message.map_err(RunError::Failed)?;
                    let id = message.get("id").cloned();
                    let method = message.get("method").and_then(Value::as_str).map(str::to_string);
                    let params = message.get("params").cloned().unwrap_or(Value::Null);
                    match (id, method) {
                        (Some(request_id), Some(method)) => {
                            let is_tool_approval = method == "mcpServer/elicitation/request"
                                && params.get("serverName").and_then(Value::as_str) == Some(COMPUTER_SERVER)
                                && params.pointer("/_meta/codex_approval_kind").and_then(Value::as_str) == Some("mcp_tool_call");
                            if !is_tool_approval {
                                tracing::warn!(%method, "computer_use:declined_server_request");
                                self.server
                                    .write(json!({ "id": request_id, "error": { "code": -32000, "message": "FNDR does not grant this request." } }))
                                    .await
                                    .map_err(RunError::Failed)?;
                                continue;
                            }
                            let tool = params.get("message").and_then(Value::as_str).and_then(tool_from_approval_message).unwrap_or_default();
                            let args = params.pointer("/_meta/tool_params").cloned().unwrap_or(Value::Null);
                            if let Some(reason) = (ctx.guards.halt)() {
                                self.answer(request_id, false).await?;
                                return Err(RunError::Failed(reason));
                            }
                            if tool.is_empty() {
                                self.answer(request_id, false).await?;
                                return Err(RunError::Failed(CODEX_UNREADABLE.to_string()));
                            }
                            let app = args.get("app").and_then(Value::as_str).unwrap_or_default();
                            // A planning turn only plans: nothing is read or done before
                            // the plan is on screen and started.
                            let refusal = if ctx.step.is_none() {
                                Some("planning does not act")
                            } else if (ctx.guards.off_limits)(app) {
                                Some("this app is off limits")
                            } else {
                                None
                            };
                            if let Some(reason) = refusal {
                                self.answer(request_id, false).await?;
                                journal(ctx, &tool, &args, Some(Risk::Never), "blocked", Some(reason.to_string()));
                                if ctx.step.is_some() {
                                    let summary = describe_tool_call(&tool, &args, ctx.observed);
                                    (ctx.emit)(ComputerUseEvent::Blocked { run_id: ctx.run_id.to_string(), index, tool, summary, reason: reason.to_string() });
                                }
                                continue;
                            }
                            let Decision { risk, reason } = classify(&tool, &args, ctx.observed);
                            let summary = describe_tool_call(&tool, &args, ctx.observed);
                            match risk {
                                Risk::Runs => self.answer(request_id, true).await?,
                                Risk::Never => {
                                    self.answer(request_id, false).await?;
                                    journal(ctx, &tool, &args, Some(risk), "blocked", Some(reason.clone()));
                                    (ctx.emit)(ComputerUseEvent::Blocked { run_id: ctx.run_id.to_string(), index, tool, summary, reason });
                                }
                                Risk::Confirm if asks_no_more(self.declined) => {
                                    // After a no the model tries other routes to the same
                                    // end (letter keys, set_value). One no covers them all.
                                    self.answer(request_id, false).await?;
                                    journal(ctx, &tool, &args, Some(risk), "declined", Some(NO_COVERS_THE_STEP.to_string()));
                                }
                                Risk::Confirm => {
                                    let request_key = uuid::Uuid::new_v4().to_string();
                                    (ctx.emit)(ComputerUseEvent::Approval {
                                        run_id: ctx.run_id.to_string(),
                                        request_key: request_key.clone(),
                                        tool: tool.clone(),
                                        summary,
                                    });
                                    pending.insert(request_key, PendingApproval { request_id, tool, args, risk });
                                }
                            }
                        }
                        (Some(response_id), None) => {
                            if response_id.as_u64() == Some(turn_request) {
                                if let Some(error) = message.pointer("/error/message").and_then(Value::as_str) {
                                    return Err(if looks_signed_out(error) { RunError::SignedOut(error.to_string()) } else { RunError::Failed(error.to_string()) });
                                }
                            }
                        }
                        (None, Some(method)) => match method.as_str() {
                            "item/started" if params.pointer("/item/type").and_then(Value::as_str) == Some("mcpToolCall") => {
                                let item = &params["item"];
                                let item_id = item.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                                let tool = item.get("tool").and_then(Value::as_str).unwrap_or_default().to_string();
                                let args = item.get("arguments").cloned().unwrap_or(Value::Null);
                                let risk = classify(&tool, &args, ctx.observed).risk;
                                (ctx.emit)(ComputerUseEvent::Action {
                                    run_id: ctx.run_id.to_string(),
                                    index,
                                    item_id: item_id.clone(),
                                    summary: describe_tool_call(&tool, &args, ctx.observed),
                                    tool: tool.clone(),
                                    risk,
                                });
                                calls.insert(item_id, (tool, args));
                            }
                            "item/completed" => {
                                let item = &params["item"];
                                match item.get("type").and_then(Value::as_str) {
                                    Some("mcpToolCall") => {
                                        let item_id = item.get("id").and_then(Value::as_str).unwrap_or_default().to_string();
                                        let (tool, args) = calls.remove(&item_id).unwrap_or_default();
                                        let result_text = tool_result_text(item);
                                        let ok = item.get("status").and_then(Value::as_str) == Some("completed")
                                            && item.get("error").is_none_or(Value::is_null)
                                            && item.pointer("/result/isError").and_then(Value::as_bool) != Some(true);
                                        record_tool_result(ctx, &tool, &args, ok, &result_text, tool_result_image_bytes(item));
                                        if result_text.contains("-1743") {
                                            // Apple Events refused: every retry would wait
                                            // minutes for the same answer.
                                            return Err(RunError::Failed(AUTOMATION_REFUSED.to_string()));
                                        }
                                        (ctx.emit)(ComputerUseEvent::ActionDone { run_id: ctx.run_id.to_string(), item_id, ok });
                                    }
                                    Some("agentMessage") if item.get("phase").and_then(Value::as_str) == Some("final_answer") => {
                                        final_text = item.get("text").and_then(Value::as_str).unwrap_or_default().to_string();
                                    }
                                    _ => {}
                                }
                            }
                            "serverRequest/resolved" => {
                                if let Some(resolved) = params.get("requestId") {
                                    pending.retain(|key, approval| {
                                        let settled = approval.request_id == *resolved;
                                        if settled {
                                            (ctx.emit)(ComputerUseEvent::ApprovalResolved { run_id: ctx.run_id.to_string(), request_key: key.clone() });
                                        }
                                        !settled
                                    });
                                }
                            }
                            "turn/completed" => {
                                let status = params.pointer("/turn/status").and_then(Value::as_str).unwrap_or("completed");
                                if status == "completed" {
                                    return Ok(final_text);
                                }
                                let error = params
                                    .pointer("/turn/error/message")
                                    .and_then(Value::as_str)
                                    .unwrap_or("Codex stopped before finishing.")
                                    .to_string();
                                return Err(if looks_signed_out(&error) { RunError::SignedOut(error) } else { RunError::Failed(error) });
                            }
                            _ => {}
                        },
                        (None, None) => {}
                    }
                }
            }
        }
    }
}

/// Bytes of pictures in a tool result. A helper with Screen Recording access
/// attaches a picture of the app's window, and Codex passes it to the model.
fn tool_result_image_bytes(item: &Value) -> usize {
    item.pointer("/result/content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("image"))
        .map(|part| {
            ["data", "image_url", "imageUrl"]
                .iter()
                .find_map(|key| part.get(*key).and_then(Value::as_str))
                .map_or(1, str::len)
        })
        .sum()
}

fn tool_result_text(item: &Value) -> String {
    item.pointer("/result/content")
        .and_then(Value::as_array)
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn journal(
    ctx: &TurnContext<'_>,
    tool: &str,
    args: &Value,
    risk: Option<Risk>,
    outcome: &str,
    detail: Option<String>,
) {
    let entry = JournalEntry {
        at: chrono::Utc::now().to_rfc3339(),
        run_id: ctx.run_id.to_string(),
        step: ctx.step,
        tool: tool.to_string(),
        args: redact(args),
        risk,
        outcome: outcome.to_string(),
        detail,
    };
    if let Err(error) = ctx.journal.append(&entry) {
        tracing::warn!(%error, "computer_use:journal_write_failed");
    }
}

/// What a tool result carried to the model, for Privacy Activity.
fn sent_kinds(result_text: &str, image_bytes: usize) -> &'static [&'static str] {
    match (!result_text.is_empty(), image_bytes > 0) {
        (true, true) => &["screen_text", "screenshot"],
        (false, true) => &["screenshot"],
        _ => &["screen_text"],
    }
}

/// Bookkeeping after a computer-use call: what FNDR learned about the UI,
/// the journal line, and the screen text that went to the model.
fn record_tool_result(
    ctx: &mut TurnContext<'_>,
    tool: &str,
    args: &Value,
    ok: bool,
    result_text: &str,
    image_bytes: usize,
) {
    let app = args.get("app").and_then(Value::as_str).unwrap_or_default();
    // The risk the call was decided at, before this result changes what
    // FNDR knows about focus.
    let risk = classify(tool, args, ctx.observed).risk;
    if ok && tool == "get_app_state" && !app.is_empty() {
        ctx.observed.observe_tree(app, result_text);
    }
    if ok && matches!(tool, "click" | "set_value") {
        if let Some(index) = args.get("element_index").and_then(Value::as_str) {
            ctx.observed.note_target(app, index);
        } else {
            // A click by position lands on something FNDR did not identify.
            ctx.observed.forget_focus(app);
        }
    }
    if ok && tool == "press_key" {
        // A key can move focus (Tab, Return, a shortcut) without FNDR seeing where.
        ctx.observed.forget_focus(app);
    }
    if result_text.contains("-1743") {
        BUNDLED_REFUSED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    if !result_text.is_empty() || image_bytes > 0 {
        crate::privacy_proof::record_model_request_including(
            crate::privacy_proof::Feature::NotchDoScreenText,
            MODEL_HOST,
            result_text.len() + image_bytes,
            sent_kinds(result_text, image_bytes),
        );
    }
    let detail = (!ok).then(|| truncate(result_text, 200));
    journal(
        ctx,
        tool,
        args,
        Some(risk),
        if ok { "ok" } else { "failed" },
        detail,
    );
}

// MARK: - A run

fn plan_request_text(
    transcript: &str,
    snippets: &[memory::MemorySnippet],
    in_front: Option<&str>,
) -> String {
    let mut text = format!("Request: {transcript}");
    if let Some(app) = in_front {
        // The name only, so "this page" has something to mean.
        text.push_str(&format!("\nIn front: {app}"));
    }
    if !snippets.is_empty() {
        text.push_str(&format!(
            "\n\nMemory snippets:\n{}",
            memory::format_block(snippets)
        ));
    }
    text
}

/// The app the person is looking at, for the planner. Never an app FNDR may
/// not read, and never FNDR itself.
fn app_in_front(seen: &plan::Observation, guards: &Guards) -> Option<String> {
    let app = seen.frontmost_app.trim();
    (!app.is_empty() && !(guards.off_limits)(app)).then(|| app.to_string())
}

/// What a run adds to "Done" when its last operate step reported something:
/// the answer to a request that only reads, in the model's words and marked so.
fn with_report(summary: String, report: Option<&str>) -> String {
    match report.map(str::trim).filter(|report| !report.is_empty()) {
        Some(report) => format!("{summary} Reported: {}", truncate(report, 400)),
        None => summary,
    }
}

fn step_request_text(
    index: usize,
    total: usize,
    step: &PlanStep,
    retry_note: Option<&str>,
) -> String {
    let mut text = format!(
        "Step {} of {}. App: {}. Goal: {}.",
        index + 1,
        total,
        step.app,
        step.goal
    );
    if let Some(note) = retry_note {
        text.push_str(&format!(
            " The previous attempt did not land: {note}. Try once more."
        ));
    }
    text
}

struct RunContext {
    run_id: String,
    emit: Arc<dyn Fn(ComputerUseEvent) + Send + Sync>,
    journal: Journal,
    guards: Guards,
    /// The person's own words, to tell a link they asked for from one they did not.
    request: String,
}

/// A work set of more than this many reopens waits for Start (ADR 027).
const MAX_AUTO_REOPENS: usize = 3;

/// Whether every step is one that cannot need the person's yes: opening an
/// app, opening a link their words account for, playback in a media app, or
/// a plan of at most three reopens FNDR resolved itself.
fn plan_starts_by_itself(plan: &Plan, request: &str, guards: &Guards) -> bool {
    plan.steps.iter().all(|step| match step.action {
        StepAction::OpenApp => {
            !(guards.off_limits)(&step.app)
                && classify(
                    "open_app",
                    &json!({ "name": step.app }),
                    &Observed::default(),
                )
                .risk
                    == Risk::Runs
        }
        StepAction::OpenUrl => plan::link_was_asked_for(&step.url, request),
        StepAction::Operate => {
            !(guards.off_limits)(&step.app) && crate::operator::policy::is_media_app(&step.app)
        }
        StepAction::ReopenMemory => {
            plan.steps.len() <= MAX_AUTO_REOPENS
                && plan
                    .steps
                    .iter()
                    .all(|step| step.action == StepAction::ReopenMemory)
                && step.item.is_some()
                && !(guards.off_limits)(&step.app)
        }
    })
}

/// Asks the person about one action FNDR itself is about to take, and waits.
async fn ask_person(
    ctx: &RunContext,
    commands: &mut mpsc::UnboundedReceiver<RunCommand>,
    tool: &str,
    summary: String,
) -> Result<bool, RunError> {
    let request_key = uuid::Uuid::new_v4().to_string();
    (ctx.emit)(ComputerUseEvent::Approval {
        run_id: ctx.run_id.clone(),
        request_key: request_key.clone(),
        tool: tool.to_string(),
        summary,
    });
    loop {
        match commands.recv().await {
            Some(RunCommand::Respond {
                request_key: answered,
                approve,
            }) if answered == request_key => {
                (ctx.emit)(ComputerUseEvent::ApprovalResolved {
                    run_id: ctx.run_id.clone(),
                    request_key,
                });
                return Ok(approve);
            }
            Some(_) => {}
            None => return Err(RunError::Stopped),
        }
    }
}

async fn wait_for_start(
    commands: &mut mpsc::UnboundedReceiver<RunCommand>,
) -> Result<(), RunError> {
    loop {
        match commands.recv().await {
            Some(RunCommand::Start) => return Ok(()),
            Some(RunCommand::Respond { .. }) => {}
            None => return Err(RunError::Stopped),
        }
    }
}

const NO_COVERS_THE_STEP: &str = "the person already said no in this step";

/// Whether to stop asking for the rest of a step: once the person has said
/// no, nothing else in it that needs a yes is put to them.
fn asks_no_more(declined_in_this_step: bool) -> bool {
    declined_in_this_step
}

/// Whether a failed step gets its second attempt. Asking again after the
/// person declined would put the same question to them twice.
fn worth_retrying(verdict: &plan::Verdict) -> bool {
    !verdict.ok && ![plan::LINK_DECLINED, plan::ACTION_DECLINED].contains(&verdict.detail.as_str())
}

/// One attempt at one step; returns what FNDR saw afterward.
async fn attempt_step(
    session: &mut Session,
    ctx: &RunContext,
    observed: &mut Observed,
    commands: &mut mpsc::UnboundedReceiver<RunCommand>,
    plan: &Plan,
    index: usize,
    retry_note: Option<&str>,
) -> Result<plan::Verdict, RunError> {
    let step = &plan.steps[index];
    let native_entry = |tool: &str, args: Value, outcome: &str, detail: Option<String>| {
        let _ = ctx.journal.append(&JournalEntry {
            at: chrono::Utc::now().to_rfc3339(),
            run_id: ctx.run_id.clone(),
            step: Some(index),
            tool: tool.to_string(),
            args: redact(&args),
            risk: Some(classify(tool, &args, observed).risk),
            outcome: outcome.to_string(),
            detail,
        });
    };
    if step.action != StepAction::OpenUrl && (ctx.guards.off_limits)(&step.app) {
        native_entry(
            "open_app",
            json!({ "name": step.app }),
            "blocked",
            Some("this app is off limits".to_string()),
        );
        return Ok(plan::Verdict {
            ok: false,
            detail: format!("{} is off limits", step.app),
        });
    }
    match step.action {
        StepAction::OpenApp => {
            let args = json!({ "name": step.app });
            if classify("open_app", &args, observed).risk == Risk::Never {
                native_entry("open_app", args, "blocked", None);
                return Ok(plan::Verdict {
                    ok: false,
                    detail: format!("{} is off limits", step.app),
                });
            }
            if let Err(error) = native::open_app(&step.app) {
                native_entry("open_app", args, "failed", Some(error.clone()));
                return Ok(plan::Verdict {
                    ok: false,
                    detail: error,
                });
            }
            let seen = native::wait_until_frontmost(&step.app, FRONTMOST_TIMEOUT).await;
            let verdict = plan::verify(step, &seen);
            native_entry(
                "open_app",
                args,
                if verdict.ok { "ok" } else { "failed" },
                Some(verdict.detail.clone()),
            );
            Ok(verdict)
        }
        StepAction::OpenUrl => {
            let args = json!({ "url": step.url });
            if !plan::link_was_asked_for(&step.url, &ctx.request) {
                let summary = format!("Open {}", truncate(&step.url, 120));
                let approved = ask_person(ctx, commands, "open_url", summary).await?;
                native_entry(
                    "open_url",
                    args.clone(),
                    if approved { "approved" } else { "declined" },
                    None,
                );
                if !approved {
                    return Ok(plan::Verdict {
                        ok: false,
                        detail: plan::LINK_DECLINED.to_string(),
                    });
                }
            }
            if let Err(error) = native::open_url(&step.url) {
                native_entry("open_url", args, "failed", Some(error.clone()));
                return Ok(plan::Verdict {
                    ok: false,
                    detail: error,
                });
            }
            let started = std::time::Instant::now();
            let verdict = loop {
                tokio::time::sleep(Duration::from_millis(400)).await;
                let verdict = plan::verify(step, &native::observe(None).await);
                if verdict.ok || started.elapsed() >= PAGE_TIMEOUT {
                    break verdict;
                }
            };
            native_entry(
                "open_url",
                args,
                if verdict.ok { "ok" } else { "failed" },
                Some(verdict.detail.clone()),
            );
            Ok(verdict)
        }
        StepAction::ReopenMemory => Ok(plan::Verdict {
            ok: false,
            detail: "A reopen runs only in a work set FNDR planned".to_string(),
        }),
        StepAction::Operate => {
            let media_app = (step.check == StepCheck::MediaPlaying).then_some(step.app.as_str());
            let track_before = match media_app {
                Some(app) => native::media_state(app).await.1,
                None => None,
            };
            if classify("open_app", &json!({ "name": step.app }), observed).risk != Risk::Never {
                let _ = native::open_app(&step.app);
                native::wait_until_frontmost(&step.app, FRONTMOST_TIMEOUT).await;
            }
            let text = step_request_text(index, plan.steps.len(), step, retry_note);
            crate::privacy_proof::record_model_request(
                crate::privacy_proof::Feature::NotchDoStep,
                MODEL_HOST,
                text.len(),
            );
            let operator_thread = session.operator_thread.clone();
            session.declined = false;
            let mut turn = TurnContext {
                run_id: &ctx.run_id,
                step: Some(index),
                guards: &ctx.guards,
                observed,
                journal: &ctx.journal,
                emit: ctx.emit.as_ref(),
                commands,
            };
            let report = tokio::time::timeout(
                STEP_TIMEOUT,
                session.run_turn(
                    &mut turn,
                    &operator_thread,
                    &text,
                    plan::step_report_schema(),
                ),
            )
            .await
            .map_err(|_| RunError::Failed(format!("\"{}\" took too long.", step.label)))??;
            let report = serde_json::from_str::<Value>(&report).ok();
            let reported_done = report
                .as_ref()
                .and_then(|value| value.get("done").and_then(Value::as_bool));
            session.last_report = report
                .as_ref()
                .filter(|_| reported_done == Some(true))
                .and_then(|value| value.get("detail").and_then(Value::as_str))
                .map(str::to_string);
            let mut seen = native::observe(media_app).await;
            // Poll only a player that answered: an unreadable one stays unreadable.
            if media_app.is_some() && seen.media_playing == Some(false) {
                // Playback can start a moment after the click lands.
                for _ in 0..8 {
                    tokio::time::sleep(Duration::from_millis(500)).await;
                    seen = native::observe(media_app).await;
                    if seen.media_playing == Some(true) {
                        break;
                    }
                }
            }
            seen.media_track_before = track_before;
            seen.reported_done = reported_done;
            let verdict = plan::verify(step, &seen);
            if !verdict.ok && session.declined {
                return Ok(plan::Verdict {
                    ok: false,
                    detail: plan::ACTION_DECLINED.to_string(),
                });
            }
            Ok(verdict)
        }
    }
}

async fn run(
    state: Arc<AppState>,
    ctx: RunContext,
    transcript: String,
    commands: mpsc::UnboundedReceiver<RunCommand>,
    codex_pid: Arc<Mutex<Option<u32>>>,
) -> Result<(), RunError> {
    if plan::asks_for_work_set(&transcript) {
        (ctx.emit)(ComputerUseEvent::Planning {
            run_id: ctx.run_id.clone(),
            used_memories: 0,
        });
        let resolution = crate::workset::resolve(&state, &transcript).await;
        return run_work_set(ctx, resolution, commands, move |item| {
            let state = state.clone();
            async move {
                let id = item.memory_id.clone();
                crate::workset::open_items(&state, &[item])
                    .await
                    .pop()
                    .unwrap_or(ItemOutcome {
                        memory_id: id,
                        label: String::new(),
                        kind: None,
                        ok: false,
                        detail: "Nothing was opened".to_string(),
                        outcome: None,
                    })
            }
        })
        .await;
    }
    let snippets = if plan::refers_to_past(&transcript) {
        memory::snippets(&state, &transcript).await
    } else {
        Vec::new()
    };
    run_with_snippets(ctx, transcript, snippets, commands, codex_pid).await
}

/// A work-set request (ADR 027): resolved on this Mac, planned by code from
/// what FNDR resolved, and opened through the reopen core. No Codex session
/// and no cloud call. A failed item does not stop the others; Stop and a
/// halt do.
async fn run_work_set<F, Fut>(
    ctx: RunContext,
    resolution: Resolution,
    mut commands: mpsc::UnboundedReceiver<RunCommand>,
    mut open: F,
) -> Result<(), RunError>
where
    F: FnMut(WorkItem) -> Fut,
    Fut: std::future::Future<Output = ItemOutcome>,
{
    let set = match resolution {
        Resolution::Best(set) => set,
        Resolution::Ambiguous(options) => {
            (ctx.emit)(ComputerUseEvent::Choose {
                run_id: ctx.run_id.clone(),
                options,
            });
            return Ok(());
        }
        Resolution::None { why } => return Err(RunError::Failed(why)),
    };
    let plan = plan::from_work_set(&set);
    (ctx.emit)(ComputerUseEvent::Planned {
        run_id: ctx.run_id.clone(),
        auto_start: plan_starts_by_itself(&plan, &ctx.request, &ctx.guards),
        steps: step_views(&plan),
    });
    wait_for_start(&mut commands).await?;

    let mut opened = Vec::new();
    let mut missed = Vec::new();
    for (index, step) in plan.steps.iter().enumerate() {
        if let Some(reason) = (ctx.guards.halt)() {
            return Err(RunError::Failed(reason));
        }
        let Some(item) = step.item.clone() else {
            continue;
        };
        (ctx.emit)(ComputerUseEvent::StepStarted {
            run_id: ctx.run_id.clone(),
            index,
            attempt: 1,
        });
        let blocked = (ctx.guards.off_limits)(&step.app);
        let outcome = if blocked {
            ItemOutcome {
                memory_id: item.memory_id.clone(),
                label: item.label.clone(),
                kind: Some(item.kind),
                ok: false,
                detail: format!("{} is off limits", step.app),
                outcome: None,
            }
        } else {
            open(item.clone()).await
        };
        let _ = ctx.journal.append(&JournalEntry {
            at: chrono::Utc::now().to_rfc3339(),
            run_id: ctx.run_id.clone(),
            step: Some(index),
            tool: "reopen_memory".to_string(),
            args: redact(&json!({ "memory_id": item.memory_id })),
            risk: Some(Risk::Runs),
            outcome: if blocked {
                "blocked"
            } else if outcome.ok {
                "ok"
            } else {
                "failed"
            }
            .to_string(),
            detail: Some(outcome.detail.clone()),
        });
        let verdict = plan::Verdict {
            ok: outcome.ok,
            detail: outcome.detail.clone(),
        };
        (ctx.emit)(ComputerUseEvent::StepDone {
            run_id: ctx.run_id.clone(),
            index,
            ok: verdict.ok,
            checked: plan::checked_by_fndr(step, &verdict),
            detail: verdict.detail,
        });
        if outcome.ok {
            opened.push(item.label);
        } else {
            missed.push(format!("{} ({})", item.label, outcome.detail));
        }
    }
    let summary = match (opened.is_empty(), missed.is_empty()) {
        (_, true) => format!("Opened: {}.", opened.join(", ")),
        (true, false) => format!("Nothing opened: {}.", missed.join("; ")),
        (false, false) => format!(
            "Opened: {}. Not opened: {}.",
            opened.join(", "),
            missed.join("; ")
        ),
    };
    (ctx.emit)(ComputerUseEvent::Finished {
        run_id: ctx.run_id.clone(),
        ok: missed.is_empty(),
        summary,
    });
    Ok(())
}

/// Plans, waits for Start, then runs and checks every step. Everything after
/// memory retrieval, so it runs without the app's state.
async fn run_with_snippets(
    ctx: RunContext,
    transcript: String,
    snippets: Vec<memory::MemorySnippet>,
    mut commands: mpsc::UnboundedReceiver<RunCommand>,
    codex_pid: Arc<Mutex<Option<u32>>>,
) -> Result<(), RunError> {
    let codex = ready_executable().map_err(RunError::Failed)?;
    let backend = detect_backend().ok_or_else(|| {
        RunError::Failed(NO_HELPER.to_string())
    })?;
    let user_servers = configured_mcp_server_names(&codex)
        .await
        .map_err(RunError::Failed)?;
    refresh_codex_features(&codex)
        .await
        .map_err(RunError::Failed)?;
    let args = session_args(&user_servers, &backend).map_err(RunError::Failed)?;

    (ctx.emit)(ComputerUseEvent::Planning {
        run_id: ctx.run_id.clone(),
        used_memories: snippets.len(),
    });

    let mut session = Session::open(&codex, &args).await?;
    if let Ok(mut slot) = codex_pid.lock() {
        *slot = session.server.pid();
    }
    let mut observed = Observed::default();

    let in_front = app_in_front(&native::observe(None).await, &ctx.guards);
    let request = plan_request_text(&transcript, &snippets, in_front.as_deref());
    crate::privacy_proof::record_model_request_including(
        crate::privacy_proof::Feature::NotchDoPlan,
        MODEL_HOST,
        request.len(),
        if snippets.is_empty() {
            &[]
        } else {
            &["memories"]
        },
    );
    let planner_thread = session.planner_thread.clone();
    let mut turn = TurnContext {
        run_id: &ctx.run_id,
        step: None,
        guards: &ctx.guards,
        observed: &mut observed,
        journal: &ctx.journal,
        emit: ctx.emit.as_ref(),
        commands: &mut commands,
    };
    let plan_text = tokio::time::timeout(
        PLAN_TIMEOUT,
        session.run_turn(
            &mut turn,
            &planner_thread,
            &request,
            plan::plan_output_schema(),
        ),
    )
    .await
    .map_err(|_| RunError::Failed("Planning took too long.".to_string()))??;
    let plan = plan::parse_plan(&plan_text)
        .map_err(|error| RunError::Failed(plan::explain_empty_plan(error, &ctx.request)))?;
    (ctx.emit)(ComputerUseEvent::Planned {
        run_id: ctx.run_id.clone(),
        auto_start: plan_starts_by_itself(&plan, &ctx.request, &ctx.guards),
        steps: step_views(&plan),
    });

    wait_for_start(&mut commands).await?;

    let mut done = Vec::new();
    for index in 0..plan.steps.len() {
        if let Some(reason) = (ctx.guards.halt)() {
            return Err(RunError::Failed(reason));
        }
        let mut retry_note: Option<String> = None;
        let mut verdict = plan::Verdict {
            ok: false,
            detail: String::new(),
        };
        for attempt in 1..=2u8 {
            (ctx.emit)(ComputerUseEvent::StepStarted {
                run_id: ctx.run_id.clone(),
                index,
                attempt,
            });
            verdict = attempt_step(
                &mut session,
                &ctx,
                &mut observed,
                &mut commands,
                &plan,
                index,
                retry_note.as_deref(),
            )
            .await?;
            // A no from the person is an answer, not a failure to retry.
            if verdict.ok || !worth_retrying(&verdict) {
                break;
            }
            retry_note = Some(verdict.detail.clone());
        }
        (ctx.emit)(ComputerUseEvent::StepDone {
            run_id: ctx.run_id.clone(),
            index,
            ok: verdict.ok,
            detail: verdict.detail.clone(),
            checked: plan::checked_by_fndr(&plan.steps[index], &verdict),
        });
        if !verdict.ok {
            (ctx.emit)(ComputerUseEvent::Finished {
                run_id: ctx.run_id.clone(),
                ok: false,
                summary: format!(
                    "Stopped at \"{}\": {}",
                    plan.steps[index].label, verdict.detail
                ),
            });
            session.server.shutdown().await;
            return Ok(());
        }
        done.push(plan.steps[index].label.clone());
    }
    (ctx.emit)(ComputerUseEvent::Finished {
        run_id: ctx.run_id.clone(),
        ok: true,
        summary: plan::with_left_out_note(
            with_report(format!("Done: {}.", done.join(", ")), session.last_report.as_deref()),
            &ctx.request,
        ),
    });
    session.server.shutdown().await;
    Ok(())
}

fn kill_process_group(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::killpg(pid as i32, libc::SIGKILL);
    }
}

/// Ends the active run at once: aborts its task and kills Codex's process
/// group, including the computer-use server and any action it is running.
fn stop_active_run(app: &AppHandle) {
    let handle = current_run().lock().ok().and_then(|mut slot| slot.take());
    if let Some(handle) = handle {
        handle.task.abort();
        if let Some(pid) = handle.codex_pid.lock().ok().and_then(|slot| *slot) {
            kill_process_group(pid);
        }
        let _ = app.emit(
            COMPUTER_USE_EVENT,
            ComputerUseEvent::Stopped {
                run_id: handle.run_id,
            },
        );
    }
}

/// Ends any run when FNDR quits, so no Codex or computer-use process outlives it.
pub fn shutdown_computer_use() {
    let handle = current_run().lock().ok().and_then(|mut slot| slot.take());
    if let Some(handle) = handle {
        handle.task.abort();
        if let Some(pid) = handle.codex_pid.lock().ok().and_then(|slot| *slot) {
            kill_process_group(pid);
        }
    }
}

fn send_to_run(command: RunCommand) -> Result<(), String> {
    let guard = current_run()
        .lock()
        .map_err(|_| "Notch Do is unavailable.".to_string())?;
    guard
        .as_ref()
        .ok_or_else(|| "Nothing is running.".to_string())?
        .commands
        .send(command)
        .map_err(|_| "That run has ended.".to_string())
}

// MARK: - IPC

#[tauri::command]
pub async fn computer_use_status(
    state: State<'_, Arc<AppState>>,
) -> Result<ComputerUseStatus, String> {
    let backend = detect_backend();
    Ok(ComputerUseStatus {
        enabled: state.inner().config.read().operator.enabled,
        codex_ready: ready_executable().is_ok(),
        backend: backend.as_ref().map(|b| b.label().to_string()),
        backend_path: backend.as_ref().map(|b| b.path().display().to_string()),
        active_run: current_run()
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(|run| run.run_id.clone())),
    })
}

/// Changes the opt-in and saves it. If the save fails the setting stays as
/// it was, so the switch never shows a state that is not on disk.
fn save_operator_enabled(
    config: &mut crate::config::Config,
    enabled: bool,
    save: impl FnOnce(&crate::config::Config) -> Result<(), String>,
) -> Result<(), String> {
    let previous = config.operator.enabled;
    config.operator.enabled = enabled;
    save(config).map_err(|error| {
        config.operator.enabled = previous;
        format!("Could not save that setting: {error}")
    })
}

/// Turns "Operate my Mac" on or off. Turning it off ends any run.
#[tauri::command]
pub async fn set_computer_use_enabled(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<bool, String> {
    {
        let mut config = state.inner().config.write();
        save_operator_enabled(&mut config, enabled, |config| {
            config.save().map_err(|error| error.to_string())
        })?;
    }
    if !enabled {
        stop_active_run(&app);
    }
    Ok(enabled)
}

/// Drives a run, ending it with the reason as soon as `halt` gives one.
/// Dropping the run is what stops it: its Codex session dies with it.
async fn until_halted(
    run: impl std::future::Future<Output = Result<(), RunError>>,
    halt: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    every: Duration,
) -> Result<(), RunError> {
    let halted = async move {
        loop {
            tokio::time::sleep(every).await;
            if let Some(reason) = halt() {
                return reason;
            }
        }
    };
    tokio::select! {
        result = run => result,
        reason = halted => Err(RunError::Failed(reason)),
    }
}

/// Plans a spoken request. Any run in progress is stopped first, so speaking
/// again mid-run redirects. Returns the new run's id; the run waits for
/// `computer_use_start` once its plan is on screen.
#[tauri::command]
pub async fn computer_use_plan(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    text: String,
) -> Result<String, String> {
    let transcript = text.trim().to_string();
    if transcript.is_empty() {
        return Err("Nothing was heard.".to_string());
    }
    if !state.inner().config.read().operator.enabled {
        return Err("Turn on \"Operate my Mac\" first.".to_string());
    }
    let guards = Guards::for_state(state.inner());
    if let Some(reason) = (guards.halt)() {
        return Err(reason);
    }
    stop_active_run(&app);

    let run_id = uuid::Uuid::new_v4().to_string();
    let (commands, receiver) = mpsc::unbounded_channel();
    let codex_pid = Arc::new(Mutex::new(None));
    let emit_app = app.clone();
    let ctx = RunContext {
        run_id: run_id.clone(),
        emit: Arc::new(move |event| {
            let _ = emit_app.emit(COMPUTER_USE_EVENT, event);
        }),
        journal: Journal::new(
            state
                .inner()
                .app_data_dir
                .join("operator")
                .join("journal.jsonl"),
        ),
        guards,
        request: transcript.clone(),
    };
    let task_state = state.inner().clone();
    let task_app = app.clone();
    let task_run_id = run_id.clone();
    let task_pid = codex_pid.clone();
    let task = tauri::async_runtime::spawn(async move {
        // Actions switched off or Private Mode ends a run at once, even in
        // the middle of an action; the guards inside the run cover the gaps.
        let halt = ctx.guards.halt.clone();
        let result = until_halted(
            run(task_state, ctx, transcript, receiver, task_pid.clone()),
            halt,
            HALT_CHECK_INTERVAL,
        )
        .await;
        if result.is_err() {
            // A run that ended early leaves nothing behind that could still act.
            if let Some(pid) = task_pid.lock().ok().and_then(|slot| *slot) {
                kill_process_group(pid);
            }
        }
        let failure = match result {
            Ok(()) | Err(RunError::Stopped) => None,
            Err(RunError::Failed(error)) => Some((error, false)),
            Err(RunError::SignedOut(error)) => Some((error, true)),
        };
        if let Some((error, reconnect)) = failure {
            tracing::warn!(%error, "computer_use:run_failed");
            let _ = task_app.emit(
                COMPUTER_USE_EVENT,
                ComputerUseEvent::Failed {
                    run_id: task_run_id.clone(),
                    error,
                    reconnect,
                },
            );
        }
        if let Ok(mut slot) = current_run().lock() {
            if slot.as_ref().is_some_and(|run| run.run_id == task_run_id) {
                *slot = None;
            }
        }
    });
    if let Ok(mut slot) = current_run().lock() {
        *slot = Some(RunHandle {
            run_id: run_id.clone(),
            commands,
            task,
            codex_pid,
        });
    }
    Ok(run_id)
}

/// Starts the planned run (the plan card's countdown, "go", or a tap).
#[tauri::command]
pub async fn computer_use_start(run_id: String) -> Result<(), String> {
    let matches = current_run()
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|run| run.run_id == run_id))
        .unwrap_or(false);
    if !matches {
        return Err("That plan is no longer current.".to_string());
    }
    send_to_run(RunCommand::Start)
}

#[tauri::command]
pub async fn computer_use_respond(request_key: String, approve: bool) -> Result<(), String> {
    send_to_run(RunCommand::Respond {
        request_key,
        approve,
    })
}

#[tauri::command]
pub async fn computer_use_stop(app: AppHandle) -> Result<(), String> {
    stop_active_run(&app);
    Ok(())
}

// MARK: - Permissions

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatorPermissions {
    /// FNDR may read other apps' windows (step checks, browser URL).
    pub accessibility: bool,
    pub screen_recording: bool,
    /// FNDR may ask Spotify or Music what is playing; `None` when neither runs.
    pub automation_media: Option<bool>,
    pub backend: Option<String>,
    /// The computer-use server answered `list_apps`; `None` until probed.
    pub backend_ready: Option<bool>,
    pub backend_detail: Option<String>,
}

/// Starts the computer-use server as FNDR's child and asks it for the app
/// list, which is what triggers and then proves its macOS permissions.
async fn probe_backend(backend: &Backend) -> (bool, String) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    if matches!(backend, Backend::Native(_))
        && !crate::accessibility::has_accessibility_permission()
    {
        return (
            false,
            "Allow FNDR under System Settings > Privacy & Security > Accessibility, then check again."
                .to_string(),
        );
    }
    let spawned = tokio::process::Command::new(backend.path())
        .arg(backend.mcp_arg())
        .env("CODEX_HOME", super::codex_account::codex_home_dir())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => return (false, format!("Could not start Computer Use: {error}")),
    };
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        return (false, "Computer Use did not open its pipes.".to_string());
    };
    let requests = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "fndr", "version": env!("CARGO_PKG_VERSION") } } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "list_apps", "arguments": {} } }),
    ];
    for request in requests {
        if stdin
            .write_all(format!("{request}\n").as_bytes())
            .await
            .is_err()
        {
            return (false, "Computer Use closed before answering.".to_string());
        }
    }
    let mut lines = BufReader::new(stdout).lines();
    let answer = tokio::time::timeout(Duration::from_secs(30), async {
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if message.get("id") == Some(&json!(2)) {
                return Some(message);
            }
        }
        None
    })
    .await
    .ok()
    .flatten();
    let _ = child.kill().await;
    let Some(answer) = answer else {
        return (
            false,
            "Computer Use did not answer. Finish any macOS permission prompt, then check again."
                .to_string(),
        );
    };
    let text = answer
        .pointer("/result/content/0/text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let failed = answer.pointer("/result/isError").and_then(Value::as_bool) == Some(true)
        || answer.get("error").is_some();
    if !failed {
        return (true, "Computer Use can see your apps.".to_string());
    }
    if text.contains("-1743") {
        BUNDLED_REFUSED.store(true, std::sync::atomic::Ordering::Relaxed);
        return (
            false,
            "Allow FNDR to control Codex Computer Use in Privacy & Security > Automation."
                .to_string(),
        );
    }
    (false, truncate(&text, 200))
}

#[tauri::command]
pub async fn computer_use_permissions(probe: bool) -> Result<OperatorPermissions, String> {
    let backend = detect_backend();
    let (backend_ready, backend_detail) = match (&backend, probe) {
        (Some(backend), true) => {
            let (ready, detail) = probe_backend(backend).await;
            (Some(ready), Some(detail))
        }
        _ => (None, None),
    };
    let automation_media = match native::automation_allowed("Spotify").await {
        Some(allowed) => Some(allowed),
        None => native::automation_allowed("Music").await,
    };
    Ok(OperatorPermissions {
        accessibility: crate::accessibility::has_accessibility_permission(),
        screen_recording: crate::ipc::onboarding::check_screen_recording_permission(),
        automation_media,
        backend: detect_backend().map(|b| b.label().to_string()),
        backend_ready,
        backend_detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_attaches_only_the_computer_use_server_and_prompts_for_it() {
        let backend =
            Backend::OpenComputerUse(PathBuf::from("/opt/homebrew/bin/open-computer-use"));
        let args = session_args(&["node_repl".into(), "computer-use".into()], &backend)
            .unwrap()
            .join(" ");
        assert!(
            args.contains("--disable computer_use"),
            "Codex's own computer use stays off"
        );
        assert!(args.contains("--disable shell_tool"));
        assert!(args.contains("mcp_servers.node_repl.enabled=false"));
        assert!(args.contains("mcp_servers.computer-use.enabled=false"));
        assert!(args
            .contains("mcp_servers.fndr_computer.command=\"/opt/homebrew/bin/open-computer-use\""));
        assert!(args.contains("mcp_servers.fndr_computer.default_tools_approval_mode=\"prompt\""));
    }

    #[test]
    fn fndrs_own_executor_is_attached_with_its_subcommand() {
        let backend = Backend::Native(PathBuf::from("/Applications/FNDR.app/Contents/MacOS/fndr"));
        assert_eq!(backend.label(), "fndr_native");
        let args = session_args(&["computer-use".into()], &backend)
            .unwrap()
            .join(" ");
        assert!(args.contains(
            "mcp_servers.fndr_computer.command=\"/Applications/FNDR.app/Contents/MacOS/fndr\""
        ));
        assert!(args.contains("mcp_servers.fndr_computer.args=[\"operator-mcp\"]"));
        assert!(args.contains("mcp_servers.fndr_computer.default_tools_approval_mode=\"prompt\""));
    }

    #[test]
    fn the_native_backend_wins_and_the_old_helpers_are_fallbacks() {
        let at = |p: &str| Some(PathBuf::from(p));
        let pick = |forced: Option<&str>, native: Option<PathBuf>, bundled, open| {
            choose_backend(forced, native, move || bundled, move || open)
        };
        let all = |forced| pick(forced, at("/fndr"), at("/launcher"), at("/occ"));
        assert_eq!(all(None), Some(Backend::Native("/fndr".into())));
        assert_eq!(
            all(Some("open_computer_use")),
            Some(Backend::OpenComputerUse("/occ".into()))
        );
        assert_eq!(
            all(Some("codex_computer_use")),
            Some(Backend::CodexBundled("/launcher".into()))
        );
        assert_eq!(
            pick(None, None, at("/launcher"), at("/occ")),
            Some(Backend::CodexBundled("/launcher".into()))
        );
        assert_eq!(
            pick(None, None, None, at("/occ")),
            Some(Backend::OpenComputerUse("/occ".into()))
        );
        assert_eq!(pick(None, None, None, None), None);
        assert_eq!(
            pick(Some("fndr_native"), None, at("/launcher"), at("/occ")),
            None
        );
    }

    #[test]
    fn finds_the_newest_bundled_computer_use() {
        let home = tempfile::tempdir().unwrap();
        for version in ["1.0.900", "1.0.1000926", "1.0.99"] {
            let bin = home.path().join(format!(
                "plugins/cache/openai-bundled/computer-use/{version}/bin"
            ));
            std::fs::create_dir_all(&bin).unwrap();
            std::fs::write(bin.join("computer-use-client-launcher"), "").unwrap();
        }
        let found = bundled_computer_use(home.path()).unwrap();
        assert!(
            found.to_string_lossy().contains("/1.0.1000926/"),
            "{}",
            found.display()
        );
        assert!(bundled_computer_use(&home.path().join("missing")).is_none());
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
    fn describes_actions_with_labels_fndr_has_seen() {
        let mut observed = Observed::default();
        observed.observe_tree(
            "Spotify",
            "App=com.spotify.client (pid 1)\n0 window\n\t2 button Play\n",
        );
        assert_eq!(
            describe_tool_call(
                "click",
                &json!({ "app": "Spotify", "element_index": "2" }),
                &observed
            ),
            "click \"button play\" in Spotify"
        );
        assert_eq!(
            describe_tool_call(
                "press_key",
                &json!({ "key": "Return", "app": "Spotify" }),
                &observed
            ),
            "press Return in Spotify"
        );
        let long = "x".repeat(200);
        assert!(
            describe_tool_call("type_text", &json!({ "text": long }), &observed)
                .chars()
                .count()
                < 60
        );
    }

    #[test]
    fn memories_reach_the_planner_only_when_retrieved() {
        assert_eq!(
            plan_request_text("open Spotify", &[], None),
            "Request: open Spotify"
        );
        let snippet = memory::MemorySnippet {
            memory_id: "m1".into(),
            app: "Spotify".into(),
            title: "Blinding Lights".into(),
            when: "2026-10-05 21:00".into(),
            timestamp: 1_790_000_000_000,
            text: "played".into(),
        };
        let text = plan_request_text("play the song from yesterday", &[snippet], Some("Music"));
        assert!(text.contains("Memory snippets:\n[1] 2026-10-05 21:00 | Spotify | Blinding Lights"));
        assert!(text.starts_with("Request: play the song from yesterday\nIn front: Music\n"));
    }

    #[test]
    fn the_planner_learns_the_app_in_front_unless_it_is_off_limits() {
        let seen = |app: &str| plan::Observation {
            frontmost_app: app.to_string(),
            ..Default::default()
        };
        let mut guards = Guards::open();
        assert_eq!(app_in_front(&seen("Google Chrome"), &guards).as_deref(), Some("Google Chrome"));
        assert_eq!(app_in_front(&seen(""), &guards), None);
        guards.off_limits = Arc::new(|app| app == "1Password");
        assert_eq!(app_in_front(&seen("1Password"), &guards), None);
    }

    #[test]
    fn a_run_that_only_read_shows_what_was_reported() {
        // Live run, 2026-10-08: "summarize the article" ended in a bare "Done".
        assert_eq!(
            with_report("Done: Summarize article.".into(), Some(" It argues X. ")),
            "Done: Summarize article. Reported: It argues X."
        );
        assert_eq!(with_report("Done: Open Chrome.".into(), None), "Done: Open Chrome.");
        assert_eq!(with_report("Done.".into(), Some("  ")), "Done.");
    }

    #[test]
    fn a_picture_in_a_tool_result_is_counted_and_named() {
        // The shape open-computer-use 0.3.6 returned with Screen Recording on.
        let item = json!({ "result": { "content": [
            { "type": "text", "text": "App=com.google.Chrome" },
            { "type": "image", "mimeType": "image/png", "data": "AAAABBBB" }
        ]}});
        assert_eq!(tool_result_image_bytes(&item), 8);
        assert_eq!(sent_kinds("tree", 8), ["screen_text", "screenshot"]);
        let text_only = json!({ "result": { "content": [{ "type": "text", "text": "tree" }] }});
        assert_eq!(tool_result_image_bytes(&text_only), 0);
        assert_eq!(sent_kinds("tree", 0), ["screen_text"]);
    }

    #[test]
    fn a_declined_link_is_not_asked_about_twice() {
        let verdict = |ok: bool, detail: &str| plan::Verdict {
            ok,
            detail: detail.to_string(),
        };
        assert!(!worth_retrying(&verdict(false, plan::LINK_DECLINED)));
        assert!(!worth_retrying(&verdict(false, plan::ACTION_DECLINED)));
        assert!(worth_retrying(&verdict(false, "Spotify is not in front")));
        assert!(!worth_retrying(&verdict(true, "")));
    }

    #[test]
    fn retry_requests_say_why_the_first_attempt_failed() {
        let step = PlanStep {
            action: StepAction::Operate,
            label: "Play".into(),
            app: "Spotify".into(),
            url: String::new(),
            goal: "play Blinding Lights".into(),
            check: StepCheck::MediaPlaying,
            item: None,
        };
        assert_eq!(
            step_request_text(1, 3, &step, None),
            "Step 2 of 3. App: Spotify. Goal: play Blinding Lights."
        );
        assert!(step_request_text(1, 3, &step, Some("Nothing is playing"))
            .contains("did not land: Nothing is playing"));
    }

    /// Drives a planner turn and an operator turn through a scripted
    /// app-server and checks what the policy did with each tool call.
    #[tokio::test]
    async fn turns_dispatch_tool_calls_through_the_policy() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/operator/fake_codex_app_server.py");
        let dir = tempfile::tempdir().unwrap();
        let answers = dir.path().join("answers.jsonl");
        std::env::set_var("FAKE_CODEX_LOG", &answers);

        let mut session = Session::open(&fixture, &[])
            .await
            .expect("fake app-server opens");
        let events: Arc<Mutex<Vec<ComputerUseEvent>>> = Arc::default();
        let (commands, mut receiver) = mpsc::unbounded_channel();
        let recorded = events.clone();
        let emit = move |event: ComputerUseEvent| {
            if let ComputerUseEvent::Approval { request_key, .. } = &event {
                // The person says yes to whatever asks.
                commands
                    .send(RunCommand::Respond {
                        request_key: request_key.clone(),
                        approve: true,
                    })
                    .unwrap();
            }
            recorded.lock().unwrap().push(event);
        };
        let journal = Journal::new(dir.path().join("journal.jsonl"));
        let mut observed = Observed::default();
        let guards = Guards::open();

        let planner = session.planner_thread.clone();
        let mut turn = TurnContext {
            run_id: "r1",
            step: None,
            guards: &guards,
            observed: &mut observed,
            journal: &journal,
            emit: &emit,
            commands: &mut receiver,
        };
        let plan_text = session
            .run_turn(
                &mut turn,
                &planner,
                "Request: open Spotify",
                plan::plan_output_schema(),
            )
            .await
            .unwrap();
        let plan = plan::parse_plan(&plan_text).unwrap();
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.steps[1].check, StepCheck::MediaPlaying);

        let operator = session.operator_thread.clone();
        let mut turn = TurnContext {
            run_id: "r1",
            step: Some(1),
            guards: &guards,
            observed: &mut observed,
            journal: &journal,
            emit: &emit,
            commands: &mut receiver,
        };
        let step_text = step_request_text(1, 3, &plan.steps[1], None);
        let report = session
            .run_turn(&mut turn, &operator, &step_text, plan::step_report_schema())
            .await
            .unwrap();
        assert!(
            report.contains("\"done\": true") || report.contains("\"done\":true"),
            "{report}"
        );

        let answers: Vec<Value> = std::fs::read_to_string(&answers)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let by_id = |id: u64| {
            answers
                .iter()
                .find(|a| a["id"] == id)
                .map(|a| a["answer"].as_str().unwrap_or_default().to_string())
        };
        assert_eq!(
            by_id(900).as_deref(),
            Some("accept"),
            "reading the screen runs"
        );
        assert_eq!(
            by_id(901).as_deref(),
            Some("decline"),
            "a purchase button is refused"
        );
        assert_eq!(
            by_id(902).as_deref(),
            Some("accept"),
            "typing outside a search field waits for a yes"
        );

        {
            let events = events.lock().unwrap();
            assert!(events
                .iter()
                .any(|e| matches!(e, ComputerUseEvent::Blocked { tool, .. } if tool == "click")));
            assert!(events.iter().any(
                |e| matches!(e, ComputerUseEvent::Approval { tool, .. } if tool == "type_text")
            ));
            assert!(!events.iter().any(
                |e| matches!(e, ComputerUseEvent::Approval { tool, .. } if tool == "get_app_state")
            ));
        }

        let journal: Vec<Value> = std::fs::read_to_string(dir.path().join("journal.jsonl"))
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let outcomes: Vec<(String, String)> = journal
            .iter()
            .map(|e| {
                (
                    e["tool"].as_str().unwrap().to_string(),
                    e["outcome"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        assert!(outcomes.contains(&("get_app_state".into(), "ok".into())));
        assert!(outcomes.contains(&("click".into(), "blocked".into())));
        assert!(outcomes.contains(&("type_text".into(), "approved".into())));
        let typed = journal.iter().find(|e| e["tool"] == "type_text").unwrap();
        assert!(
            !typed["args"]["text"].as_str().unwrap().contains("hello"),
            "typed text is hashed"
        );
        session.server.shutdown().await;
    }

    #[tokio::test]
    async fn stop_ends_a_turn_waiting_for_approval() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/operator/fake_codex_app_server.py");
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::open(&fixture, &[]).await.unwrap();
        let (commands, mut receiver) = mpsc::unbounded_channel::<RunCommand>();
        let journal = Journal::new(dir.path().join("journal.jsonl"));
        let mut observed = Observed::default();
        let guards = Guards::open();
        // Dropping the sender is what aborting the run does to the turn.
        let sender = std::sync::Mutex::new(Some(commands));
        let emit = move |event: ComputerUseEvent| {
            if matches!(event, ComputerUseEvent::Approval { .. }) {
                sender.lock().unwrap().take();
            }
        };
        let operator = session.operator_thread.clone();
        let mut turn = TurnContext {
            run_id: "r2",
            step: Some(0),
            guards: &guards,
            observed: &mut observed,
            journal: &journal,
            emit: &emit,
            commands: &mut receiver,
        };
        let result = session
            .run_turn(
                &mut turn,
                &operator,
                "Step 1 of 1. App: Notes. Goal: x.",
                plan::step_report_schema(),
            )
            .await;
        assert_eq!(result, Err(RunError::Stopped));
        session.server.shutdown().await;
    }

    fn fake_codex() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/operator/fake_codex_app_server.py")
    }

    fn journal_lines(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }

    #[test]
    fn a_plan_starts_by_itself_only_when_no_step_can_need_a_yes() {
        let step = |action: &str, app: &str, url: &str| {
            format!(
                r#"{{"action":"{action}","label":"x","app":"{app}","url":"{url}","goal":"g","check":"none"}}"#
            )
        };
        let plan_of = |steps: &[String]| {
            plan::parse_plan(&format!(r#"{{"steps":[{}]}}"#, steps.join(","))).unwrap()
        };
        let request = "open Spotify, play Blinding Lights, then look up looped transformers";
        let open = Guards::open();

        let safe = plan_of(&[
            step("open_app", "Spotify", ""),
            step("operate", "Spotify", ""),
            step(
                "open_url",
                "",
                "https://www.google.com/search?q=looped+transformers",
            ),
        ]);
        assert!(plan_starts_by_itself(&safe, request, &open));

        let types_in_notes = plan_of(&[step("operate", "Notes", "")]);
        assert!(!plan_starts_by_itself(&types_in_notes, request, &open));

        let unasked_link = plan_of(&[step("open_url", "", "https://evil.example/?d=looped")]);
        assert!(!plan_starts_by_itself(&unasked_link, request, &open));

        let blocked = Guards {
            halt: Arc::new(|| None),
            off_limits: Arc::new(|app| app == "Spotify"),
        };
        assert!(!plan_starts_by_itself(&safe, request, &blocked));
    }

    /// Actions switched off mid-action ends the run without waiting for its next step.
    #[tokio::test]
    async fn a_run_in_the_middle_of_an_action_ends_when_halted() {
        let switched_off = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = switched_off.clone();
        let halt: Arc<dyn Fn() -> Option<String> + Send + Sync> = Arc::new(move || {
            flag.load(std::sync::atomic::Ordering::SeqCst)
                .then(|| ACTIONS_OFF.to_string())
        });
        let every = Duration::from_millis(10);

        // A run that never finishes on its own, like an action stuck in flight.
        let stuck = std::future::pending::<Result<(), RunError>>();
        let flip = switched_off.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(40)).await;
            flip.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        let ended = tokio::time::timeout(
            Duration::from_secs(2),
            until_halted(stuck, halt.clone(), every),
        )
        .await
        .expect("the run ends");
        assert_eq!(ended, Err(RunError::Failed(ACTIONS_OFF.to_string())));

        // A run that finishes first keeps its own result.
        switched_off.store(false, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(until_halted(async { Ok(()) }, halt, every).await, Ok(()));
    }

    #[test]
    fn the_operate_opt_in_keeps_its_old_value_when_saving_fails() {
        let mut config = crate::config::Config::default();
        assert!(!config.operator.enabled);

        let failed = save_operator_enabled(&mut config, true, |_| Err("disk full".to_string()));
        assert_eq!(
            failed,
            Err("Could not save that setting: disk full".to_string())
        );
        assert!(!config.operator.enabled);

        let saved = std::cell::Cell::new(false);
        save_operator_enabled(&mut config, true, |config| {
            saved.set(config.operator.enabled);
            Ok(())
        })
        .unwrap();
        assert!(
            config.operator.enabled && saved.get(),
            "the new value is what gets saved"
        );
    }

    /// A planner that asks to read the screen is refused: nothing is read or
    /// done before the plan is shown and started.
    #[tokio::test]
    async fn a_planning_turn_cannot_use_tools() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::open(&fake_codex(), &[]).await.unwrap();
        let (_commands, mut receiver) = mpsc::unbounded_channel::<RunCommand>();
        let events: Arc<Mutex<Vec<ComputerUseEvent>>> = Arc::default();
        let recorded = events.clone();
        let emit = move |event: ComputerUseEvent| recorded.lock().unwrap().push(event);
        let journal_path = dir.path().join("journal.jsonl");
        let journal = Journal::new(journal_path.clone());
        let mut observed = Observed::default();
        let guards = Guards::open();
        let planner = session.planner_thread.clone();
        let mut turn = TurnContext {
            run_id: "r4",
            step: None,
            guards: &guards,
            observed: &mut observed,
            journal: &journal,
            emit: &emit,
            commands: &mut receiver,
        };
        let plan_text = session
            .run_turn(
                &mut turn,
                &planner,
                "Request: PROBE open Spotify",
                plan::plan_output_schema(),
            )
            .await
            .unwrap();
        assert!(plan::parse_plan(&plan_text).is_ok());
        let lines = journal_lines(&journal_path);
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0]["tool"], "get_app_state");
        assert_eq!(lines[0]["outcome"], "blocked");
        assert!(events.lock().unwrap().is_empty());
        session.server.shutdown().await;
    }

    /// An app on the person's blocklist is never read or operated.
    #[tokio::test]
    async fn an_off_limits_app_is_never_read_or_operated() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::open(&fake_codex(), &[]).await.unwrap();
        let (commands, mut receiver) = mpsc::unbounded_channel();
        let events: Arc<Mutex<Vec<ComputerUseEvent>>> = Arc::default();
        let recorded = events.clone();
        let emit = move |event: ComputerUseEvent| {
            if let ComputerUseEvent::Approval { request_key, .. } = &event {
                commands
                    .send(RunCommand::Respond {
                        request_key: request_key.clone(),
                        approve: true,
                    })
                    .unwrap();
            }
            recorded.lock().unwrap().push(event);
        };
        let journal_path = dir.path().join("journal.jsonl");
        let journal = Journal::new(journal_path.clone());
        let mut observed = Observed::default();
        let guards = Guards {
            halt: Arc::new(|| None),
            off_limits: Arc::new(|app| app == "Spotify"),
        };
        let operator = session.operator_thread.clone();
        let mut turn = TurnContext {
            run_id: "r5",
            step: Some(0),
            guards: &guards,
            observed: &mut observed,
            journal: &journal,
            emit: &emit,
            commands: &mut receiver,
        };
        session
            .run_turn(
                &mut turn,
                &operator,
                "Step 1 of 1. App: Spotify. Goal: x.",
                plan::step_report_schema(),
            )
            .await
            .unwrap();
        let spotify: Vec<(String, String)> = journal_lines(&journal_path)
            .iter()
            .filter(|line| line["args"]["app"] == "Spotify")
            .map(|line| {
                (
                    line["tool"].as_str().unwrap().to_string(),
                    line["outcome"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        assert_eq!(
            spotify,
            vec![
                ("get_app_state".to_string(), "blocked".to_string()),
                ("click".to_string(), "blocked".to_string()),
            ]
        );
        assert!(!events.lock().unwrap().iter().any(|event| match event {
            ComputerUseEvent::Action { tool, .. } | ComputerUseEvent::Approval { tool, .. } => {
                tool != "type_text"
            }
            _ => false,
        }));
        session.server.shutdown().await;
    }

    /// Actions switched off, or Private Mode, ends the turn at the next action.
    #[tokio::test]
    async fn a_halted_run_refuses_the_next_action() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::open(&fake_codex(), &[]).await.unwrap();
        let (_commands, mut receiver) = mpsc::unbounded_channel::<RunCommand>();
        let emit = |_event: ComputerUseEvent| {};
        let journal_path = dir.path().join("journal.jsonl");
        let journal = Journal::new(journal_path.clone());
        let mut observed = Observed::default();
        let guards = Guards {
            halt: Arc::new(|| Some(ACTIONS_OFF.to_string())),
            off_limits: Arc::new(|_| false),
        };
        let operator = session.operator_thread.clone();
        let mut turn = TurnContext {
            run_id: "r6",
            step: Some(0),
            guards: &guards,
            observed: &mut observed,
            journal: &journal,
            emit: &emit,
            commands: &mut receiver,
        };
        let result = session
            .run_turn(
                &mut turn,
                &operator,
                "Step 1 of 1. App: Spotify. Goal: x.",
                plan::step_report_schema(),
            )
            .await;
        assert_eq!(result, Err(RunError::Failed(ACTIONS_OFF.to_string())));
        assert!(journal_lines(&journal_path).is_empty());
        session.server.shutdown().await;
    }

    /// One no covers the step: the routes the model tries next are declined
    /// without putting each to the person.
    #[tokio::test]
    async fn after_a_no_nothing_else_in_the_step_is_put_to_the_person() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::open(&fake_codex(), &[]).await.unwrap();
        let (commands, mut receiver) = mpsc::unbounded_channel::<RunCommand>();
        let asked: Arc<Mutex<Vec<String>>> = Arc::default();
        let recorded = asked.clone();
        let emit = move |event: ComputerUseEvent| {
            if let ComputerUseEvent::Approval { request_key, tool, .. } = &event {
                recorded.lock().unwrap().push(tool.clone());
                commands
                    .send(RunCommand::Respond {
                        request_key: request_key.clone(),
                        approve: false,
                    })
                    .unwrap();
            }
        };
        let journal_path = dir.path().join("journal.jsonl");
        let journal = Journal::new(journal_path.clone());
        let mut observed = Observed::default();
        let guards = Guards::open();
        let operator = session.operator_thread.clone();
        let mut turn = TurnContext {
            run_id: "r8",
            step: Some(0),
            guards: &guards,
            observed: &mut observed,
            journal: &journal,
            emit: &emit,
            commands: &mut receiver,
        };
        session
            .run_turn(
                &mut turn,
                &operator,
                "Step 1 of 1. App: Notes. Goal: PERSISTS_AFTER_NO.",
                plan::step_report_schema(),
            )
            .await
            .unwrap();
        assert_eq!(*asked.lock().unwrap(), ["type_text"], "asked once");
        assert!(session.declined);
        let declined = journal_lines(&journal_path)
            .iter()
            .filter(|line| line["outcome"] == "declined")
            .count();
        assert_eq!(declined, 3, "all three were declined and journaled");
        session.server.shutdown().await;
    }

    /// A Codex whose approval wording FNDR cannot read fails with the cause.
    #[tokio::test]
    async fn an_unreadable_action_ends_the_turn_with_the_cause() {
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::open(&fake_codex(), &[]).await.unwrap();
        let (_commands, mut receiver) = mpsc::unbounded_channel::<RunCommand>();
        let emit = |_event: ComputerUseEvent| {};
        let journal = Journal::new(dir.path().join("journal.jsonl"));
        let mut observed = Observed::default();
        let guards = Guards::open();
        let operator = session.operator_thread.clone();
        let mut turn = TurnContext {
            run_id: "r7",
            step: Some(0),
            guards: &guards,
            observed: &mut observed,
            journal: &journal,
            emit: &emit,
            commands: &mut receiver,
        };
        let result = session
            .run_turn(
                &mut turn,
                &operator,
                "Step 1 of 1. App: Spotify. Goal: REWORDED_APPROVAL.",
                plan::step_report_schema(),
            )
            .await;
        assert_eq!(result, Err(RunError::Failed(CODEX_UNREADABLE.to_string())));
        session.server.shutdown().await;
    }

    /// The example request, end to end on this Mac: real Codex, real Computer
    /// Use, real apps. Steps that need a yes are declined, so nothing beyond
    /// the policy's "runs" tier happens. Events print as they arrive.
    /// `FNDR_LIVE_REQUEST="..." cargo test --lib live_notch_do -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "live: operates apps on this Mac with the real ChatGPT login"]
    async fn live_notch_do_runs_the_example_request() {
        let transcript = std::env::var("FNDR_LIVE_REQUEST").unwrap_or_else(|_| {
            "open Spotify, play Blinding Lights, then open the browser and look up looped transformers".to_string()
        });
        let journal_dir = std::env::temp_dir().join("fndr-live-notch-do");
        std::fs::create_dir_all(&journal_dir).unwrap();
        let (commands, receiver) = mpsc::unbounded_channel();
        let finished: Arc<Mutex<Option<(bool, String)>>> = Arc::default();
        let recorded = finished.clone();
        let started = std::time::Instant::now();
        let ctx = RunContext {
            run_id: "live".to_string(),
            journal: Journal::new(journal_dir.join("journal.jsonl")),
            guards: Guards::open(),
            request: transcript.clone(),
            emit: Arc::new(move |event| {
                println!(
                    "[{:>5.1}s] {}",
                    started.elapsed().as_secs_f32(),
                    serde_json::to_string(&event).unwrap()
                );
                match &event {
                    ComputerUseEvent::Planned { .. } => commands.send(RunCommand::Start).unwrap(),
                    ComputerUseEvent::Approval { request_key, .. } => commands
                        .send(RunCommand::Respond {
                            request_key: request_key.clone(),
                            approve: false,
                        })
                        .unwrap(),
                    ComputerUseEvent::Finished { ok, summary, .. } => {
                        *recorded.lock().unwrap() = Some((*ok, summary.clone()));
                    }
                    _ => {}
                }
            }),
        };
        let result = run_with_snippets(ctx, transcript, Vec::new(), receiver, Arc::default()).await;
        println!(
            "run result: {result:?}; journal: {}",
            journal_dir.join("journal.jsonl").display()
        );
        let finished = finished.lock().unwrap().clone();
        assert_eq!(result, Ok(()));
        let (ok, summary) = finished.expect("the run finished");
        assert!(ok, "{summary}");
    }

    #[tokio::test]
    async fn a_refused_automation_grant_ends_the_turn_with_the_fix() {
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/operator/fake_codex_app_server.py");
        let dir = tempfile::tempdir().unwrap();
        let mut session = Session::open(&fixture, &[]).await.unwrap();
        let (_commands, mut receiver) = mpsc::unbounded_channel::<RunCommand>();
        let journal = Journal::new(dir.path().join("journal.jsonl"));
        let mut observed = Observed::default();
        let guards = Guards::open();
        let emit = |_event: ComputerUseEvent| {};
        let operator = session.operator_thread.clone();
        let mut turn = TurnContext {
            run_id: "r3",
            step: Some(1),
            guards: &guards,
            observed: &mut observed,
            journal: &journal,
            emit: &emit,
            commands: &mut receiver,
        };
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            session.run_turn(
                &mut turn,
                &operator,
                "AUTOMATION_DENIED",
                plan::step_report_schema(),
            ),
        )
        .await
        .expect("the turn ends at once");
        match result {
            Err(RunError::Failed(message)) => assert!(message.contains("Automation"), "{message}"),
            other => panic!("expected a permission failure, got {other:?}"),
        }
        session.server.shutdown().await;
    }

    #[test]
    fn signed_out_errors_ask_for_reconnect() {
        assert!(looks_signed_out("unexpected status 401 Unauthorized"));
        assert!(looks_signed_out(
            "Your refresh token has expired; please log in again"
        ));
        assert!(!looks_signed_out("Rate limit reached"));
    }

    #[test]
    fn events_serialize_in_the_shape_the_notch_reads() {
        let done = serde_json::to_value(ComputerUseEvent::StepDone {
            run_id: "r".into(),
            index: 1,
            ok: true,
            detail: "Playing".into(),
            checked: true,
        })
        .unwrap();
        assert_eq!(
            done,
            json!({ "kind": "stepDone", "runId": "r", "index": 1, "ok": true, "detail": "Playing", "checked": true })
        );
        let failed = serde_json::to_value(ComputerUseEvent::Failed {
            run_id: "r".into(),
            error: "x".into(),
            reconnect: true,
        })
        .unwrap();
        assert_eq!(failed["reconnect"], true);
    }
}
