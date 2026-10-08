//! Hermes bridge, gateway, and chat Tauri commands.

use crate::http_util::{llm_http_client, local_service_client, post_json_response};
use crate::search::MemoryCard;
use crate::AppState;
use parking_lot::Mutex as AgentMutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, OnceLock as AgentOnceLock};
use std::time::UNIX_EPOCH;
use tauri::State;
use tokio::time::{Duration, Instant};

use super::common::{strip_internal_fndr_results, truncate_chars};
use super::search::{memory_card_from_result, refine_memory_card_titles};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesAppContext {
    pub app_name: String,
    pub memory_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesMemoryDigest {
    pub title: String,
    pub app_name: String,
    pub summary: String,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesBridgeStatus {
    pub installed: bool,
    pub configured: bool,
    pub setup_complete: bool,
    pub gateway_running: bool,
    pub api_server_ready: bool,
    pub version: Option<String>,
    pub bundled_repo_available: bool,
    pub runtime_source: Option<String>,
    pub provider_kind: Option<String>,
    pub model_name: Option<String>,
    pub base_url: Option<String>,
    /// The saved provider answers on this Mac.
    pub provider_is_local: bool,
    /// FNDR adds memories it finds itself to each message for this provider.
    pub related_memories: bool,
    pub api_url: String,
    pub gateway_dir: String,
    pub home_dir: String,
    pub context_path: String,
    pub context_ready: bool,
    pub last_synced_at: Option<i64>,
    pub fndr_local_model_id: Option<String>,
    pub ollama_installed: bool,
    pub ollama_reachable: bool,
    pub ollama_models: Vec<String>,
    pub ollama_base_url: String,
    pub codex_cli_installed: bool,
    pub codex_logged_in: bool,
    pub codex_auth_path: String,
    pub profile_name: Option<String>,
    pub focus_task: Option<String>,
    pub recent_memory_count: u32,
    pub open_task_count: u32,
    /// True when Ollama is reachable and configured — chat works without Hermes CLI.
    pub direct_ollama_ready: bool,
    pub top_apps: Vec<HermesAppContext>,
    pub recent_memories: Vec<HermesMemoryDigest>,
    pub last_error: Option<String>,
    pub install_command: String,
    /// `stopped`, `starting`, `running`, `restarting` or `crashed`.
    pub gateway_state: String,
    pub gateway_restarts: u32,
    /// The ChatGPT sign-in could not be used or refreshed: show Reconnect.
    pub reconnect_chatgpt: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesSetupPayload {
    pub provider_kind: String,
    pub model_name: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    /// Also send memories FNDR finds on its own to a provider that is not
    /// on this Mac. Off unless the person turns it on.
    #[serde(default)]
    pub related_memories: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HermesChatReply {
    pub response_id: String,
    pub conversation_id: String,
    pub content: String,
    /// Memories FNDR added on its own to the message this answers.
    #[serde(default)]
    pub auto_memories: Vec<super::agent_chats::AttachedMemory>,
    /// What Hermes did on the way to this answer, in order, once each.
    #[serde(default)]
    pub tools_used: Vec<String>,
}

/// What a Hermes tool call was, in words for the person. The name of a tool
/// FNDR does not know is shown as it is, so nothing Hermes does is hidden.
fn tool_use_label(name: &str) -> String {
    let lower = name.to_lowercase();
    if lower.contains("fndr") || lower.contains("memory.") || lower.contains("memory_") {
        "searched FNDR memories".to_string()
    } else if lower == "todo" {
        "kept a planning list".to_string()
    } else {
        format!("used {name}")
    }
}

/// The tool calls in a gateway response: `function_call` items come before
/// the final message, each with the tool's name.
fn tools_used_in(response: &serde_json::Value) -> Vec<String> {
    let mut used: Vec<String> = Vec::new();
    let calls = response
        .get("output")
        .and_then(|value| value.as_array())
        .into_iter()
        .flatten()
        .filter(|item| item.get("type").and_then(|value| value.as_str()) == Some("function_call"))
        .filter_map(|item| item.get("name").and_then(|value| value.as_str()));
    for name in calls {
        let label = tool_use_label(name);
        if !used.contains(&label) {
            used.push(label);
        }
    }
    used
}

#[derive(Debug, Deserialize)]
struct HermesOnboardingProfile {
    display_name: Option<String>,
    model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HermesSetupRecord {
    provider_kind: String,
    model_name: String,
    #[serde(default)]
    base_url: Option<String>,
    #[serde(default)]
    related_memories: bool,
}

fn is_loopback_url(url: &str) -> bool {
    url.parse::<reqwest::Url>()
        .ok()
        .and_then(|url| url.host_str().map(str::to_lowercase))
        .is_some_and(|host| matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1" | "[::1]"))
}

/// Whether the saved provider answers on this Mac, so nothing leaves it.
fn provider_is_local(record: &HermesSetupRecord) -> bool {
    let base_url = record
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty());
    match record.provider_kind.as_str() {
        "ollama" => base_url.is_none_or(is_loopback_url),
        "custom" => base_url.is_some_and(is_loopback_url),
        _ => false,
    }
}

/// Whether FNDR may add memories it found itself, and let Hermes search
/// memory, for this provider. Always for a local one; elsewhere only when
/// the person turned it on (ADR 024).
fn sends_related_memories(record: &HermesSetupRecord) -> bool {
    provider_is_local(record) || record.related_memories
}

static HERMES_GATEWAY_PROCESS: AgentOnceLock<AgentMutex<Option<Child>>> = AgentOnceLock::new();
static HERMES_GATEWAY_ERROR: AgentOnceLock<AgentMutex<Option<String>>> = AgentOnceLock::new();

// Local service endpoints. Keep host/port/path declarations together so the
// Hermes gateway and Ollama probe URLs can be updated in one place rather than
// scattered as literal strings throughout this module.
const HERMES_API_HOST: &str = "127.0.0.1";
const HERMES_API_PORT: u16 = 8742;
const OLLAMA_HOME_URL: &str = "http://127.0.0.1:11434";
const OLLAMA_BASE_URL: &str = "http://127.0.0.1:11434/v1";
const OLLAMA_API_TAGS_URL: &str = "http://127.0.0.1:11434/api/tags";

fn hermes_gateway_dir(state: &AppState) -> PathBuf {
    state.app_data_dir.join("hermes-gateway")
}

fn hermes_home_dir(state: &AppState) -> PathBuf {
    state.app_data_dir.join("hermes-home")
}

/// The memory snapshot older builds wrote; removed on sync (memories now go
/// with each message, see `send_hermes_message`).
fn legacy_hermes_context_path(state: &AppState) -> PathBuf {
    hermes_gateway_dir(state).join("FNDR_CONTEXT.md")
}

fn hermes_project_context_path(state: &AppState) -> PathBuf {
    hermes_gateway_dir(state).join(".hermes.md")
}

fn hermes_gateway_readme_path(state: &AppState) -> PathBuf {
    hermes_gateway_dir(state).join("README.md")
}

fn hermes_env_path(state: &AppState) -> PathBuf {
    hermes_home_dir(state).join(".env")
}

fn hermes_config_path(state: &AppState) -> PathBuf {
    hermes_home_dir(state).join("config.yaml")
}

fn hermes_setup_record_path(state: &AppState) -> PathBuf {
    hermes_home_dir(state).join("fndr_setup.json")
}

fn hermes_soul_path(state: &AppState) -> PathBuf {
    hermes_home_dir(state).join("SOUL.md")
}

fn hermes_api_url() -> String {
    format!("http://{HERMES_API_HOST}:{HERMES_API_PORT}")
}

fn get_hermes_gateway_process() -> &'static AgentMutex<Option<Child>> {
    HERMES_GATEWAY_PROCESS.get_or_init(|| AgentMutex::new(None))
}

fn get_hermes_gateway_error_store() -> &'static AgentMutex<Option<String>> {
    HERMES_GATEWAY_ERROR.get_or_init(|| AgentMutex::new(None))
}

fn read_hermes_profile_name(state: &AppState) -> Option<String> {
    let path = state.app_data_dir.join("onboarding.json");
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<HermesOnboardingProfile>(&raw)
        .ok()?
        .display_name
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
}

fn read_fndr_local_model_id(state: &AppState) -> Option<String> {
    let path = state.app_data_dir.join("onboarding.json");
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<HermesOnboardingProfile>(&raw)
        .ok()?
        .model_id
        .map(|model_id| model_id.trim().to_string())
        .filter(|model_id| !model_id.is_empty())
}

#[derive(Debug, Clone)]
enum HermesLauncher {
    Bundled { python: PathBuf, script: PathBuf },
}

impl HermesLauncher {
    fn command(&self) -> Command {
        match self {
            Self::Bundled { python, script } => {
                let mut command = Command::new(python);
                command.arg(script);
                command
            }
        }
    }
}

#[derive(Debug, Clone)]
struct HermesRuntimeStatus {
    installed: bool,
    version: Option<String>,
    launcher: Option<HermesLauncher>,
    bundled_repo_path: Option<PathBuf>,
    runtime_source: Option<String>,
}

impl HermesRuntimeStatus {
    fn bundled_repo_available(&self) -> bool {
        self.bundled_repo_path.is_some()
    }
}

fn user_home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn hermes_runtime_root(state: &AppState) -> PathBuf {
    state.app_data_dir.join("hermes-runtime")
}

fn hermes_runtime_bin_dir(state: &AppState) -> PathBuf {
    hermes_runtime_root(state).join("bin")
}

fn hermes_runtime_python_dir(state: &AppState) -> PathBuf {
    hermes_runtime_root(state).join("python")
}

fn hermes_runtime_venv_dir(state: &AppState) -> PathBuf {
    hermes_runtime_root(state).join("venv")
}

fn hermes_runtime_python_path(state: &AppState) -> PathBuf {
    hermes_runtime_venv_dir(state).join("bin").join("python3")
}

fn hermes_uv_path(state: &AppState) -> PathBuf {
    hermes_runtime_bin_dir(state).join("uv")
}

fn is_hermes_repo(path: &Path) -> bool {
    path.join("pyproject.toml").exists() && path.join("hermes").exists()
}

/// The Hermes source FNDR runs: a `FNDR_HERMES_REPO` override, else the
/// pinned clone in FNDR's app data (`hermes_codex::HERMES_PINNED_COMMIT`).
fn resolve_bundled_hermes_repo(state: &AppState) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(value) = std::env::var_os("FNDR_HERMES_REPO") {
        candidates.push(PathBuf::from(value));
    }
    candidates.push(super::hermes_codex::pinned_hermes_dir(
        &hermes_runtime_root(state),
    ));
    candidates
        .into_iter()
        .find(|candidate| is_hermes_repo(candidate))
}

fn read_hermes_repo_version(repo_root: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(repo_root.join("pyproject.toml")).ok()?;
    let value = raw.parse::<toml::Value>().ok()?;
    value
        .get("project")
        .and_then(|project| project.get("version"))
        .and_then(|version| version.as_str())
        .map(str::to_string)
}

fn existing_executable_path(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn find_existing_executable(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn common_executable_candidates(name: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(home) = user_home_dir() {
        candidates.push(home.join(".local/bin").join(name));
        candidates.push(home.join(".cargo/bin").join(name));
        candidates.push(home.join(".npm-global/bin").join(name));
    }

    candidates.push(PathBuf::from("/opt/homebrew/bin").join(name));
    candidates.push(PathBuf::from("/usr/local/bin").join(name));
    candidates
}

fn detect_uv_executable(state: &AppState) -> Option<PathBuf> {
    let bundled_uv = hermes_uv_path(state);
    if bundled_uv.exists() {
        return Some(bundled_uv);
    }

    existing_executable_path("uv")
        .or_else(|| find_existing_executable(common_executable_candidates("uv")))
}

fn detect_ollama_executable() -> Option<PathBuf> {
    let mut candidates = common_executable_candidates("ollama");
    candidates.push(PathBuf::from(
        "/Applications/Ollama.app/Contents/Resources/ollama",
    ));
    existing_executable_path("ollama").or_else(|| find_existing_executable(candidates))
}

/// Codex binaries bundled inside OpenAI's desktop apps. They ship a working
/// native build even when a global npm install has lost its platform binary.
const BUNDLED_CODEX_CANDIDATES: &[&str] = &[
    "/Applications/ChatGPT.app/Contents/Resources/codex",
    "/Applications/Codex.app/Contents/Resources/codex",
];

/// First Codex that actually runs; otherwise the first one found, so callers
/// can report a broken install instead of a missing one.
pub(crate) fn detect_codex_executable() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = existing_executable_path("codex").into_iter().collect();
    candidates.extend(common_executable_candidates("codex"));
    candidates.extend(BUNDLED_CODEX_CANDIDATES.iter().map(PathBuf::from));
    candidates.retain(|candidate| candidate.is_file());
    candidates.dedup();

    candidates
        .iter()
        .find(|candidate| codex_runs(candidate))
        .or_else(|| candidates.first())
        .cloned()
}

fn codex_runs(executable: &Path) -> bool {
    Command::new(executable)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn command_failure_detail(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stderr.is_empty() {
        stderr
    } else if !stdout.is_empty() {
        stdout
    } else {
        "No diagnostic output was returned.".to_string()
    }
}

fn configure_uv_command(command: &mut Command, state: &AppState) -> Result<(), String> {
    let runtime_root = hermes_runtime_root(state);
    let xdg_cache_home = runtime_root.join("xdg-cache");
    let xdg_data_home = runtime_root.join("xdg-data");
    let xdg_config_home = runtime_root.join("xdg-config");
    let tool_dir = runtime_root.join("tools");
    let tool_bin_dir = hermes_runtime_bin_dir(state);
    let python_dir = hermes_runtime_python_dir(state);
    let project_env = hermes_runtime_venv_dir(state);

    for dir in [
        &runtime_root,
        &xdg_cache_home,
        &xdg_data_home,
        &xdg_config_home,
        &tool_dir,
        &tool_bin_dir,
        &python_dir,
    ] {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }

    command
        .env("XDG_CACHE_HOME", xdg_cache_home)
        .env("XDG_DATA_HOME", xdg_data_home)
        .env("XDG_CONFIG_HOME", xdg_config_home)
        .env("UV_TOOL_DIR", tool_dir)
        .env("UV_TOOL_BIN_DIR", tool_bin_dir)
        .env("UV_PYTHON_INSTALL_DIR", python_dir)
        .env("UV_PROJECT_ENVIRONMENT", project_env)
        .env("UV_NO_PROGRESS", "1");

    Ok(())
}

fn ensure_uv_available(state: &AppState) -> Result<PathBuf, String> {
    if let Some(path) = detect_uv_executable(state) {
        return Ok(path);
    }

    let install_dir = hermes_runtime_bin_dir(state);
    std::fs::create_dir_all(&install_dir).map_err(|e| e.to_string())?;

    let output = Command::new("sh")
        .arg("-lc")
        .arg("curl -LsSf https://astral.sh/uv/install.sh | sh")
        .env("UV_UNMANAGED_INSTALL", &install_dir)
        .env("UV_NO_MODIFY_PATH", "1")
        .output()
        .map_err(|e| format!("Failed to install uv for the bundled Hermes runtime: {e}"))?;

    if !output.status.success() {
        return Err(format!(
            "FNDR could not prepare its private Hermes runtime because uv failed to install. {}",
            command_failure_detail(&output)
        ));
    }

    detect_uv_executable(state).ok_or_else(|| {
        "uv installed successfully, but FNDR could not locate the resulting binary.".to_string()
    })
}

fn prepare_vendored_hermes_runtime(state: &AppState) -> Result<(), String> {
    let repo_root = match resolve_bundled_hermes_repo(state) {
        Some(repo_root) => repo_root,
        None => super::hermes_codex::clone_pinned_hermes(&hermes_runtime_root(state))?,
    };
    let uv = ensure_uv_available(state)?;
    let venv_dir = hermes_runtime_venv_dir(state);

    let mut venv_command = Command::new(&uv);
    venv_command
        .arg("venv")
        .arg(&venv_dir)
        .arg("--python")
        .arg("3.11");
    configure_uv_command(&mut venv_command, state)?;
    let venv_output = venv_command
        .current_dir(&repo_root)
        .output()
        .map_err(|e| format!("Failed to create the FNDR Hermes environment: {e}"))?;
    if !venv_output.status.success() {
        return Err(format!(
            "FNDR could not create the bundled Hermes environment. {}",
            command_failure_detail(&venv_output)
        ));
    }

    let mut sync_command = Command::new(&uv);
    sync_command.arg("sync").arg("--locked");
    for extra in ["messaging", "pty", "honcho", "mcp", "acp"] {
        sync_command.arg("--extra").arg(extra);
    }
    configure_uv_command(&mut sync_command, state)?;
    let sync_output = sync_command
        .current_dir(&repo_root)
        .output()
        .map_err(|e| format!("Failed to install Hermes dependencies for FNDR: {e}"))?;
    if !sync_output.status.success() {
        return Err(format!(
            "FNDR could not finish installing Hermes dependencies. {}",
            command_failure_detail(&sync_output)
        ));
    }

    if !hermes_runtime_python_path(state).exists() {
        return Err(
            "FNDR prepared the Hermes runtime, but the private Python interpreter is missing."
                .to_string(),
        );
    }

    Ok(())
}

/// Installs the pinned Hermes into FNDR's app data unless it is already there.
pub fn ensure_pinned_hermes(state: &AppState) -> Result<(), String> {
    if detect_hermes_runtime(state).runtime_source.as_deref() == Some("pinned") {
        return Ok(());
    }
    prepare_vendored_hermes_runtime(state)
}

fn detect_hermes_runtime(state: &AppState) -> HermesRuntimeStatus {
    let bundled_repo_path = resolve_bundled_hermes_repo(state);
    let bundled_version = bundled_repo_path
        .as_deref()
        .and_then(read_hermes_repo_version);

    if let Some(repo_root) = bundled_repo_path.clone() {
        let python = hermes_runtime_python_path(state);
        let script = repo_root.join("hermes");
        if python.exists() && script.exists() {
            return HermesRuntimeStatus {
                installed: true,
                version: bundled_version.clone(),
                launcher: Some(HermesLauncher::Bundled { python, script }),
                bundled_repo_path: Some(repo_root),
                runtime_source: Some("pinned".to_string()),
            };
        }
    }

    // Only the pinned Hermes runs. A Hermes found elsewhere on this Mac is a
    // different version with different default tools, and FNDR's limits on
    // it were checked against the pinned one.
    HermesRuntimeStatus {
        installed: false,
        version: bundled_version,
        launcher: None,
        bundled_repo_path,
        runtime_source: None,
    }
}

fn detect_ollama_installation() -> bool {
    detect_ollama_executable()
        .and_then(|executable| {
            Command::new(executable)
                .arg("--version")
                .output()
                .ok()
                .filter(|output| output.status.success())
        })
        .is_some()
}

fn parse_ollama_list_output(output: &str) -> Vec<String> {
    let mut models = output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            trimmed
                .split_whitespace()
                .next()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.eq_ignore_ascii_case("name"))
        })
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    models
}

async fn detect_ollama_state() -> (bool, bool, Vec<String>) {
    let installed = detect_ollama_installation();
    let mut reachable = false;
    let mut models: Vec<String> = Vec::new();

    if let Ok(client) = local_service_client() {
        if let Ok(parsed_url) = OLLAMA_API_TAGS_URL.parse::<reqwest::Url>() {
            if let Some(host) = parsed_url.host_str() {
                crate::privacy_proof::record_egress(host);
            }
        }
        if let Ok(response) = client.get(OLLAMA_API_TAGS_URL).send().await {
            if response.status().is_success() {
                reachable = true;
                if let Ok(json) = response.json::<serde_json::Value>().await {
                    models = json
                        .get("models")
                        .and_then(|value| value.as_array())
                        .into_iter()
                        .flatten()
                        .filter_map(|item| item.get("name").and_then(|value| value.as_str()))
                        .map(str::to_string)
                        .collect();
                }
            }
        }
    }

    if models.is_empty() && installed {
        if let Some(ollama) = detect_ollama_executable() {
            if let Ok(output) = Command::new(ollama).arg("list").output() {
                if output.status.success() {
                    reachable = true;
                    models = parse_ollama_list_output(&String::from_utf8_lossy(&output.stdout));
                }
            }
        }
    }

    models.sort();
    models.dedup();
    (installed, reachable, models)
}

use super::codex_account::codex_home_dir;

fn codex_auth_path() -> PathBuf {
    codex_home_dir().join("auth.json")
}

fn detect_codex_state() -> (bool, bool, PathBuf) {
    let auth_path = codex_auth_path();
    let cli_installed = detect_codex_executable()
        .and_then(|executable| {
            Command::new(executable)
                .arg("--help")
                .output()
                .ok()
                .filter(|output| output.status.success())
        })
        .is_some();

    let logged_in = std::fs::read_to_string(&auth_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .map(|json| {
            json.get("OPENAI_API_KEY")
                .and_then(|value| value.as_str())
                .is_some_and(|value| !value.trim().is_empty())
                || json
                    .get("tokens")
                    .and_then(|value| value.as_object())
                    .is_some_and(|tokens| !tokens.is_empty())
                || json
                    .get("tokens")
                    .and_then(|value| value.as_array())
                    .is_some_and(|tokens| !tokens.is_empty())
        })
        .unwrap_or(false);

    (cli_installed, logged_in, auth_path)
}

fn read_hermes_setup_record(state: &AppState) -> Option<HermesSetupRecord> {
    let raw = std::fs::read_to_string(hermes_setup_record_path(state)).ok()?;
    serde_json::from_str::<HermesSetupRecord>(&raw).ok()
}

/// A YAML double-quoted scalar. JSON string syntax is valid YAML, so this
/// quotes and escapes model names and URLs safely. (This used to go through
/// `toml::to_string`, which rejects a bare string with "unsupported rust type"
/// and made every provider save fail.)
fn yaml_scalar(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

/// Hermes's `config.yaml` model block for a saved provider choice.
fn hermes_config_yaml(record: &HermesSetupRecord) -> Result<String, String> {
    let model = yaml_scalar(&record.model_name);
    let base_url = record
        .base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    Ok(match record.provider_kind.as_str() {
        "ollama" => format!(
            "model:\n  provider: custom\n  default: {model}\n  base_url: {}\n  context_length: 32768\n",
            yaml_scalar(base_url.unwrap_or(OLLAMA_BASE_URL)),
        ),
        // Hermes's canonical id for ChatGPT-subscription inference; it reads
        // the tokens Codex keeps in $CODEX_HOME/auth.json (see codex_account.rs).
        "codex" => format!("model:\n  provider: openai-codex\n  default: {model}\n"),
        "custom" => {
            let base_url =
                base_url.ok_or_else(|| "A base URL is required for a custom endpoint.".to_string())?;
            format!(
                "model:\n  provider: custom\n  default: {model}\n  base_url: {}\n",
                yaml_scalar(base_url),
            )
        }
        provider => format!(
            "model:\n  provider: {}\n  default: {model}\n",
            yaml_scalar(provider),
        ),
    })
}

fn restrict_to_owner(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
    }
    #[cfg(not(unix))]
    let _ = (path, mode);
}

/// Writes a file only this account can read. These hold a provider API key,
/// the gateway's key, or FNDR's MCP token.
fn write_private(path: &Path, contents: &str) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| e.to_string())?;
    file.write_all(contents.as_bytes())
        .map_err(|e| e.to_string())?;
    // A file from an older build keeps its old mode unless it is reset.
    restrict_to_owner(path, 0o600);
    Ok(())
}

fn persist_hermes_setup_files(state: &AppState, setup: &HermesSetupPayload) -> Result<(), String> {
    let home_dir = hermes_home_dir(state);
    std::fs::create_dir_all(&home_dir).map_err(|e| e.to_string())?;
    let api_key = setup
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let record = HermesSetupRecord {
        provider_kind: setup.provider_kind.trim().to_string(),
        model_name: setup.model_name.trim().to_string(),
        base_url: setup
            .base_url
            .as_ref()
            .map(|value| value.trim().to_string()),
        related_memories: setup.related_memories,
    };

    let config_yaml = hermes_config_yaml(&record)? + super::hermes_codex::HERMES_TOOLS_YAML;

    let mut env_lines = vec![
        "API_SERVER_ENABLED=true".to_string(),
        format!("API_SERVER_HOST={HERMES_API_HOST}"),
        format!("API_SERVER_PORT={HERMES_API_PORT}"),
        format!("API_SERVER_KEY={}", uuid::Uuid::new_v4()),
        "API_SERVER_MODEL_NAME=hermes-agent".to_string(),
    ];

    match record.provider_kind.as_str() {
        "custom" | "ollama" => {
            if let Some(api_key) = api_key {
                env_lines.push(format!("OPENAI_API_KEY={api_key}"));
            }
        }
        "openrouter" => {
            if let Some(api_key) = api_key {
                env_lines.push(format!("OPENROUTER_API_KEY={api_key}"));
            }
        }
        _ => {}
    }

    let soul_md = crate::inference::prompts::HERMES_IDENTITY;

    let record_json = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?;
    restrict_to_owner(&home_dir, 0o700);
    write_private(&hermes_config_path(state), &config_yaml)?;
    write_private(&hermes_env_path(state), &(env_lines.join("\n") + "\n"))?;
    std::fs::write(hermes_soul_path(state), soul_md).map_err(|e| e.to_string())?;
    write_private(&hermes_setup_record_path(state), &record_json)?;
    Ok(())
}

fn read_hermes_api_key(state: &AppState) -> Option<String> {
    let env_contents = std::fs::read_to_string(hermes_env_path(state)).ok()?;
    env_contents.lines().find_map(|line| {
        let value = line.strip_prefix("API_SERVER_KEY=")?;
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn update_hermes_gateway_runtime() -> (bool, Option<String>) {
    let mut process_guard = get_hermes_gateway_process().lock();
    if let Some(child) = process_guard.as_mut() {
        match child.try_wait() {
            Ok(Some(status)) => {
                let last_output = gateway_stderr().lock().back().cloned();
                let message = if status.success() {
                    "Hermes gateway exited.".to_string()
                } else {
                    gateway_exit_message(&format!("with status {status}"), last_output.as_deref())
                };
                *get_hermes_gateway_error_store().lock() = Some(message.clone());
                *process_guard = None;
                (false, Some(message))
            }
            Ok(None) => (true, get_hermes_gateway_error_store().lock().clone()),
            Err(err) => {
                let message = format!("Failed to inspect Hermes gateway: {err}");
                *get_hermes_gateway_error_store().lock() = Some(message.clone());
                *process_guard = None;
                (false, Some(message))
            }
        }
    } else {
        (false, get_hermes_gateway_error_store().lock().clone())
    }
}

async fn hermes_api_ready() -> bool {
    let Ok(client) = local_service_client() else {
        return false;
    };
    crate::privacy_proof::record_egress(HERMES_API_HOST);
    match client
        .get(format!("{}/health", hermes_api_url()))
        .send()
        .await
    {
        Ok(response) => response.status().is_success(),
        Err(_) => false,
    }
}

fn file_modified_at_ms(path: &PathBuf) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let duration = modified.duration_since(UNIX_EPOCH).ok()?;
    Some(duration.as_millis().min(i64::MAX as u128) as i64)
}

async fn build_hermes_bridge_status(state: &AppState) -> Result<HermesBridgeStatus, String> {
    let context_path = hermes_project_context_path(state);
    let home_dir = hermes_home_dir(state);
    // Memory and task counts are decoration; a vault that can't be read
    // (locked, migrating) must not hide Hermes setup.
    let recent_results = state
        .store
        .list_recent_results(18, None)
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "hermes:status_recent_memories_unavailable");
            Vec::new()
        });
    let mut recent_memories: Vec<MemoryCard> = strip_internal_fndr_results(recent_results)
        .into_iter()
        .map(memory_card_from_result)
        .collect();
    refine_memory_card_titles(&mut recent_memories);

    let mut app_counts: HashMap<String, usize> = HashMap::new();
    for memory in &recent_memories {
        *app_counts.entry(memory.app_name.clone()).or_insert(0) += 1;
    }

    let mut top_apps: Vec<HermesAppContext> = app_counts
        .into_iter()
        .map(|(app_name, memory_count)| HermesAppContext {
            app_name,
            memory_count: memory_count as u32,
        })
        .collect();
    top_apps.sort_by(|left, right| {
        right
            .memory_count
            .cmp(&left.memory_count)
            .then_with(|| left.app_name.cmp(&right.app_name))
    });
    top_apps.truncate(6);

    let recent_memories = recent_memories
        .into_iter()
        .take(6)
        .map(|memory| HermesMemoryDigest {
            title: memory.title,
            app_name: memory.app_name,
            summary: truncate_chars(&memory.summary, 180),
            timestamp: memory.timestamp,
        })
        .collect::<Vec<_>>();

    let open_task_count = state
        .store
        .list_tasks()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|task| !task.is_completed && !task.is_dismissed)
        .count() as u32;

    let runtime = detect_hermes_runtime(state);
    let (ollama_installed, ollama_reachable, ollama_models) = detect_ollama_state().await;
    let (codex_cli_installed, codex_logged_in, codex_auth_path) = detect_codex_state();
    let setup = read_hermes_setup_record(state);
    let configured = setup.is_some();
    let (gateway_running, last_error) = update_hermes_gateway_runtime();
    let api_server_ready = if gateway_running {
        hermes_api_ready().await
    } else {
        false
    };

    // Direct Ollama mode: provider configured as ollama + Ollama reachable + has models.
    // This works without the Hermes CLI being installed at all.
    let direct_ollama_ready = setup
        .as_ref()
        .map(|s| s.provider_kind == "ollama")
        .unwrap_or(false)
        && ollama_reachable
        && !ollama_models.is_empty();

    let bundled_repo_available = runtime.bundled_repo_available();
    let runtime_source = runtime.runtime_source.clone();
    let version = runtime.version.clone();

    let (gateway_state, gateway_restarts, reconnect_chatgpt) = gateway_snapshot();
    Ok(HermesBridgeStatus {
        installed: runtime.installed,
        configured,
        setup_complete: configured && (runtime.installed || direct_ollama_ready),
        gateway_running,
        api_server_ready,
        direct_ollama_ready,
        version,
        bundled_repo_available,
        runtime_source,
        provider_kind: setup.as_ref().map(|value| value.provider_kind.clone()),
        model_name: setup.as_ref().map(|value| value.model_name.clone()),
        provider_is_local: setup.as_ref().is_some_and(provider_is_local),
        related_memories: setup.as_ref().is_some_and(sends_related_memories),
        base_url: setup.as_ref().and_then(|value| {
            if value.provider_kind == "ollama" {
                Some(
                    value
                        .base_url
                        .clone()
                        .unwrap_or_else(|| OLLAMA_BASE_URL.to_string()),
                )
            } else {
                value.base_url.clone()
            }
        }),
        api_url: hermes_api_url(),
        gateway_dir: hermes_gateway_dir(state).display().to_string(),
        home_dir: home_dir.display().to_string(),
        context_path: context_path.display().to_string(),
        context_ready: context_path.exists(),
        last_synced_at: file_modified_at_ms(&context_path),
        fndr_local_model_id: read_fndr_local_model_id(state),
        ollama_installed,
        ollama_reachable,
        ollama_models,
        ollama_base_url: OLLAMA_BASE_URL.to_string(),
        codex_cli_installed,
        codex_logged_in,
        codex_auth_path: codex_auth_path.display().to_string(),
        profile_name: read_hermes_profile_name(state),
        focus_task: state.focus_task.read().clone(),
        recent_memory_count: recent_memories.len() as u32,
        open_task_count,
        top_apps,
        recent_memories,
        last_error,
        install_command: "FNDR installs a pinned Hermes into its own app data.".to_string(),
        gateway_state,
        gateway_restarts,
        reconnect_chatgpt,
    })
}

use crate::inference::prompts::HERMES_OPERATING_NOTES;

fn render_hermes_gateway_readme(status: &HermesBridgeStatus) -> String {
    format!(
        "# FNDR Hermes Gateway\n\n\
FNDR generated this workspace for Hermes.\n\n\
Files:\n\
- `.hermes.md` holds FNDR's operating notes. Memories are not stored here; each message carries its own.\n\n\
Gateway directory: {}\n",
        status.gateway_dir
    )
}

/// Writes the gateway's operating instructions. No memories go in these
/// files: each message carries its own retrieved snippets instead.
async fn sync_hermes_bridge_files(state: &AppState) -> Result<HermesBridgeStatus, String> {
    let mut status = build_hermes_bridge_status(state).await?;
    let gateway_dir = hermes_gateway_dir(state);
    std::fs::create_dir_all(&gateway_dir).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(legacy_hermes_context_path(state));
    std::fs::write(hermes_project_context_path(state), HERMES_OPERATING_NOTES)
        .map_err(|e| e.to_string())?;
    std::fs::write(
        hermes_gateway_readme_path(state),
        render_hermes_gateway_readme(&status),
    )
    .map_err(|e| e.to_string())?;
    status.context_ready = true;
    status.last_synced_at = file_modified_at_ms(&hermes_project_context_path(state));
    Ok(status)
}

async fn wait_for_hermes_api(timeout_ms: u64) -> bool {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    while Instant::now() < deadline {
        if hermes_api_ready().await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    false
}

fn validate_hermes_gateway_prerequisites(status: &HermesBridgeStatus) -> Result<(), String> {
    if !status.installed {
        return Err(if status.bundled_repo_available {
            "FNDR has a bundled Hermes clone, but the private runtime is not prepared yet. Click Enable Agent in the FNDR Agent panel first."
                .to_string()
        } else {
            "Hermes is not installed yet.".to_string()
        });
    }
    if !status.configured {
        return Err("Finish FNDR Agent setup before starting the runtime.".to_string());
    }
    if status.provider_kind.as_deref() == Some("ollama") {
        if !status.ollama_installed {
            return Err(
                "Install Ollama on this Mac before starting the FNDR agent in Ollama mode."
                    .to_string(),
            );
        }
        if !status.ollama_reachable {
            return Err(format!(
                "FNDR could not reach Ollama at {OLLAMA_HOME_URL}. Open Ollama or run `ollama serve`, then try again."
            ));
        }
    }
    if status.provider_kind.as_deref() == Some("codex") && !status.codex_logged_in {
        return Err(
            "FNDR could not find an active Codex login for the agent runtime. Sign in to Codex on this Mac first."
                .to_string(),
        );
    }
    Ok(())
}

async fn ensure_hermes_gateway_ready(
    state: &AppState,
    timeout_ms: u64,
) -> Result<HermesBridgeStatus, String> {
    let status = sync_hermes_bridge_files(state).await?;
    validate_hermes_gateway_prerequisites(&status)?;

    if status.api_server_ready {
        return Ok(status);
    }

    let (running, _) = update_hermes_gateway_runtime();
    if !running {
        let uses_chatgpt = status.provider_kind.as_deref() == Some("codex");
        if uses_chatgpt {
            sync_chatgpt_login(state).await?;
        }
        write_hermes_mcp_config(state);
        spawn_hermes_gateway(state, uses_chatgpt)?;
        start_hermes_supervisor(state, uses_chatgpt);
    }

    if !wait_for_hermes_api(timeout_ms).await {
        let message =
            "Hermes gateway started, but the local API server did not come online in time."
                .to_string();
        *get_hermes_gateway_error_store().lock() = Some(message.clone());
        return Err(message);
    }

    let ready_status = build_hermes_bridge_status(state).await?;
    if ready_status.api_server_ready {
        Ok(ready_status)
    } else {
        Err(ready_status
            .last_error
            .clone()
            .unwrap_or_else(|| "Hermes gateway is still unavailable.".to_string()))
    }
}

// MARK: - Gateway lifecycle

#[derive(Debug, Default)]
struct Supervisor {
    state: &'static str,
    restarts: u32,
    started_at: Option<Instant>,
    /// Set by Stop and by a provider change, so their exit is not a crash.
    stop_requested: bool,
    running: bool,
    reconnect_chatgpt: bool,
}

/// `(state, restarts, reconnect_chatgpt)` under one lock.
fn gateway_snapshot() -> (String, u32, bool) {
    let supervisor = supervisor().lock();
    (
        supervisor.state.to_string(),
        supervisor.restarts,
        supervisor.reconnect_chatgpt,
    )
}

fn supervisor() -> &'static AgentMutex<Supervisor> {
    static SUPERVISOR: AgentOnceLock<AgentMutex<Supervisor>> = AgentOnceLock::new();
    SUPERVISOR.get_or_init(|| {
        AgentMutex::new(Supervisor {
            state: "stopped",
            ..Default::default()
        })
    })
}

/// Refreshes the ChatGPT login through the Codex app-server and lets Hermes
/// import it. A missing or unusable login fails loudly with Reconnect.
async fn sync_chatgpt_login(state: &AppState) -> Result<(), String> {
    match super::hermes_codex::sync_hermes_codex_login(&hermes_home_dir(state)).await {
        super::hermes_codex::LoginState::SignedIn => {
            supervisor().lock().reconnect_chatgpt = false;
            Ok(())
        }
        super::hermes_codex::LoginState::SignedOut(reason) => {
            supervisor().lock().reconnect_chatgpt = true;
            Err(format!("Reconnect ChatGPT to use Hermes. {reason}"))
        }
    }
}

/// Adds FNDR's MCP server to Hermes's config when it is running, so Hermes can
/// search FNDR itself. The port changes per launch, so this runs every start.
fn write_hermes_mcp_config(state: &AppState) {
    let Some(record) = read_hermes_setup_record(state) else {
        return;
    };
    let Ok(mut config) = hermes_config_yaml(&record) else {
        return;
    };
    config.push_str(super::hermes_codex::HERMES_TOOLS_YAML);
    let mcp = crate::mcp::status();
    // Searching memory is another way for memories to reach the provider.
    if sends_related_memories(&record) && mcp.running && !mcp.endpoint.is_empty() {
        config.push_str(&super::hermes_codex::hermes_mcp_yaml(
            &mcp.endpoint,
            &mcp.token,
        ));
    }
    if let Err(error) = write_private(&hermes_config_path(state), &config) {
        tracing::warn!(%error, "hermes:mcp_config_write_failed");
    }
}

const GATEWAY_STDERR_LINES: usize = 40;
const GATEWAY_STDERR_LINE_CHARS: usize = 300;
const GATEWAY_PID_FILE: &str = "gateway.pid";

/// The gateway's last error output, held in memory only, so a start that
/// fails can say why. Never written to disk or sent anywhere.
fn gateway_stderr() -> &'static AgentMutex<std::collections::VecDeque<String>> {
    static TAIL: AgentOnceLock<AgentMutex<std::collections::VecDeque<String>>> =
        AgentOnceLock::new();
    TAIL.get_or_init(|| AgentMutex::new(std::collections::VecDeque::new()))
}

fn remember_gateway_stderr(tail: &mut std::collections::VecDeque<String>, line: &str) {
    let line = line.trim();
    if line.is_empty() {
        return;
    }
    tail.push_back(line.chars().take(GATEWAY_STDERR_LINE_CHARS).collect());
    while tail.len() > GATEWAY_STDERR_LINES {
        tail.pop_front();
    }
}

fn gateway_exit_message(status: &str, last_output: Option<&str>) -> String {
    match last_output {
        Some(line) => format!("Hermes gateway exited {status}. Last output: {line}"),
        None => format!("Hermes gateway exited {status}."),
    }
}

/// Whether a running process's command line is a Hermes gateway.
fn is_hermes_gateway_command(command: &str) -> bool {
    command.to_lowercase().contains("hermes")
        && command.split_whitespace().any(|word| word == "gateway")
}

/// Ends a gateway that an earlier FNDR left running (a crash or force quit).
pub fn reap_stale_hermes_gateway(app_data_dir: &Path) {
    let gateway_dir = app_data_dir.join("hermes-gateway");
    let command_of = |pid: i32| {
        Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "command="])
            .output()
            .map(|output| String::from_utf8_lossy(&output.stdout).to_string())
            .unwrap_or_default()
    };
    if let Some(pid) = take_stale_gateway_pid(&gateway_dir, command_of) {
        tracing::warn!(pid, "hermes:stale_gateway_stopped");
        #[cfg(unix)]
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
    }
}

/// Reads and removes the saved gateway process id, and returns it only when
/// that process is still a Hermes gateway. The id may belong to something
/// else by now, and that something must never be stopped.
fn take_stale_gateway_pid(gateway_dir: &Path, command_of: impl Fn(i32) -> String) -> Option<i32> {
    let pid_path = gateway_dir.join(GATEWAY_PID_FILE);
    let pid = std::fs::read_to_string(&pid_path)
        .ok()?
        .trim()
        .parse::<i32>()
        .ok();
    let _ = std::fs::remove_file(&pid_path);
    pid.filter(|pid| *pid > 1 && is_hermes_gateway_command(&command_of(*pid)))
}

fn spawn_gateway_child(
    launcher: &HermesLauncher,
    hermes_home: &Path,
    gateway_dir: &Path,
    uses_chatgpt: bool,
) -> std::io::Result<Child> {
    use std::io::BufRead;
    let mut command = launcher.command();
    command
        .arg("gateway")
        .env("HERMES_HOME", hermes_home)
        .current_dir(gateway_dir)
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if uses_chatgpt {
        command.env("CODEX_HOME", codex_home_dir());
    }
    let mut child = command.spawn()?;
    gateway_stderr().lock().clear();
    if let Some(stderr) = child.stderr.take() {
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stderr)
                .lines()
                .map_while(Result::ok)
            {
                remember_gateway_stderr(&mut gateway_stderr().lock(), &line);
            }
        });
    }
    let _ = std::fs::write(gateway_dir.join(GATEWAY_PID_FILE), child.id().to_string());
    Ok(child)
}

fn spawn_hermes_gateway(state: &AppState, uses_chatgpt: bool) -> Result<(), String> {
    let launcher = detect_hermes_runtime(state)
        .launcher
        .ok_or_else(|| "FNDR could not resolve a Hermes runtime to launch.".to_string())?;
    let child = spawn_gateway_child(
        &launcher,
        &hermes_home_dir(state),
        &hermes_gateway_dir(state),
        uses_chatgpt,
    )
    .map_err(|e| format!("Failed to start Hermes gateway: {e}"))?;
    *get_hermes_gateway_process().lock() = Some(child);
    *get_hermes_gateway_error_store().lock() = None;
    let mut supervisor = supervisor().lock();
    supervisor.state = "starting";
    supervisor.started_at = Some(Instant::now());
    supervisor.stop_requested = false;
    Ok(())
}

/// Ends the gateway on purpose (Stop, or a provider change).
/// Stops the gateway when FNDR quits, so it does not keep running without it.
pub fn shutdown_hermes_gateway() {
    stop_hermes_gateway_process();
}

fn stop_hermes_gateway_process() {
    supervisor().lock().stop_requested = true;
    let mut process_guard = get_hermes_gateway_process().lock();
    if let Some(child) = process_guard.as_mut() {
        let _ = child.kill();
    }
    *process_guard = None;
    supervisor().lock().state = "stopped";
}

/// Watches the gateway: restarts it once after a crash (with backoff), and
/// keeps the ChatGPT login fresh through the app-server while it runs.
fn start_hermes_supervisor(state: &AppState, uses_chatgpt: bool) {
    {
        let mut supervisor = supervisor().lock();
        if supervisor.running {
            return;
        }
        supervisor.running = true;
    }
    let app_data_dir = state.app_data_dir.clone();
    let hermes_home = hermes_home_dir(state);
    let gateway_dir = hermes_gateway_dir(state);
    let launcher = detect_hermes_runtime(state).launcher;
    tauri::async_runtime::spawn(async move {
        let mut last_refresh = Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            if supervisor().lock().stop_requested {
                break;
            }
            let (running, last_error) = update_hermes_gateway_runtime();
            if running {
                let starting = supervisor().lock().state == "starting";
                if starting && hermes_api_ready().await {
                    supervisor().lock().state = "running";
                }
                if uses_chatgpt && last_refresh.elapsed() >= super::hermes_codex::REFRESH_INTERVAL {
                    last_refresh = Instant::now();
                    let signed_in = matches!(
                        super::hermes_codex::sync_hermes_codex_login(&hermes_home).await,
                        super::hermes_codex::LoginState::SignedIn
                    );
                    supervisor().lock().reconnect_chatgpt = !signed_in;
                }
                continue;
            }
            let (restarts, uptime) = {
                let supervisor = supervisor().lock();
                (
                    supervisor.restarts,
                    supervisor
                        .started_at
                        .map(|t| t.elapsed())
                        .unwrap_or_default(),
                )
            };
            match super::hermes_codex::on_crash(restarts, uptime) {
                super::hermes_codex::CrashResponse::Restart { after } => {
                    tracing::warn!(?last_error, "hermes:gateway_crashed_restarting");
                    supervisor().lock().state = "restarting";
                    tokio::time::sleep(after).await;
                    let Some(launcher) = launcher.as_ref() else {
                        break;
                    };
                    match spawn_gateway_child(launcher, &hermes_home, &gateway_dir, uses_chatgpt) {
                        Ok(child) => {
                            *get_hermes_gateway_process().lock() = Some(child);
                            let mut supervisor = supervisor().lock();
                            supervisor.restarts = if uptime >= Duration::from_secs(300) {
                                1
                            } else {
                                restarts + 1
                            };
                            supervisor.started_at = Some(Instant::now());
                            supervisor.state = "starting";
                        }
                        Err(error) => {
                            *get_hermes_gateway_error_store().lock() =
                                Some(format!("Hermes could not restart: {error}"));
                            supervisor().lock().state = "crashed";
                            break;
                        }
                    }
                }
                super::hermes_codex::CrashResponse::GiveUp => {
                    tracing::warn!(?last_error, "hermes:gateway_crashed_again");
                    supervisor().lock().state = "crashed";
                    break;
                }
            }
        }
        let _ = app_data_dir;
        supervisor().lock().running = false;
    });
}

#[tauri::command]
pub async fn get_hermes_bridge_status(
    state: State<'_, Arc<AppState>>,
) -> Result<HermesBridgeStatus, String> {
    build_hermes_bridge_status(state.inner()).await
}

#[tauri::command]
pub async fn install_hermes_bridge(
    state: State<'_, Arc<AppState>>,
) -> Result<HermesBridgeStatus, String> {
    ensure_pinned_hermes(state.inner())?;
    build_hermes_bridge_status(state.inner()).await
}

#[tauri::command]
pub async fn save_hermes_setup(
    state: State<'_, Arc<AppState>>,
    payload: HermesSetupPayload,
) -> Result<HermesBridgeStatus, String> {
    let provider_kind = payload.provider_kind.trim();
    let model_name = payload.model_name.trim();
    if model_name.is_empty() {
        return Err("Choose a model name for the FNDR agent.".to_string());
    }

    let api_key = payload
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());

    match provider_kind {
        "openrouter" => {
            if api_key.is_none() {
                return Err(
                    "An OpenRouter API key is required to finish FNDR Agent setup.".to_string(),
                );
            }
        }
        "custom" => {
            let has_base_url = payload
                .base_url
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty());
            if !has_base_url {
                return Err("A base URL is required for a custom endpoint.".to_string());
            }
        }
        "ollama" => {
            let (ollama_installed, _, _) = detect_ollama_state().await;
            if !ollama_installed {
                return Err(
                    "FNDR could not find Ollama on this Mac. Install Ollama first, then return to the Agent page."
                        .to_string(),
                );
            }
        }
        "codex" => {
            let (_, codex_logged_in, _) = detect_codex_state();
            if !codex_logged_in {
                return Err(
                    "FNDR could not find a local Codex login yet. Sign in to Codex on this Mac first, then choose Codex again."
                        .to_string(),
                );
            }
        }
        _ => {
            return Err(
                "FNDR currently supports agent setup via Ollama, Codex OAuth, OpenRouter, or a custom endpoint."
                    .to_string(),
            );
        }
    }

    persist_hermes_setup_files(state.inner(), &payload)?;
    stop_hermes_gateway_process();
    *get_hermes_gateway_error_store().lock() = None;
    sync_hermes_bridge_files(state.inner()).await
}

/// The memory block for one chat message: the evidence preamble and up to
/// five snippets, or nothing when retrieval found none.
fn memory_context(snippets: &[crate::operator::memory::MemorySnippet]) -> String {
    if snippets.is_empty() {
        return String::new();
    }
    format!(
        "\n\n{}\n\n{}\n\n",
        crate::inference::prompts::HERMES_MEMORY_PREAMBLE,
        crate::operator::memory::format_block(snippets)
    )
}

const HERMES_STOPPED: &str = "Stopped.";

/// One stop signal per chat with a message in flight.
fn hermes_cancels() -> &'static AgentMutex<HashMap<String, Arc<tokio::sync::Notify>>> {
    static CANCELS: AgentOnceLock<AgentMutex<HashMap<String, Arc<tokio::sync::Notify>>>> =
        AgentOnceLock::new();
    CANCELS.get_or_init(|| AgentMutex::new(HashMap::new()))
}

/// Sends one message. A send that fails is still kept in the chat, marked
/// as not sent; a send the person stopped is dropped.
#[tauri::command]
pub async fn send_hermes_message(
    state: State<'_, Arc<AppState>>,
    conversation_id: String,
    input: String,
    memory_ids: Option<Vec<String>>,
) -> Result<HermesChatReply, String> {
    let memory_ids = memory_ids.unwrap_or_default();
    let sent_at = chrono::Utc::now().timestamp_millis();
    let cancel = Arc::new(tokio::sync::Notify::new());
    hermes_cancels()
        .lock()
        .insert(conversation_id.clone(), cancel.clone());
    let result = unless_stopped(
        deliver_hermes_message(
            state.inner(),
            conversation_id.clone(),
            input.clone(),
            memory_ids.clone(),
        ),
        &cancel,
    )
    .await;
    {
        let mut cancels = hermes_cancels().lock();
        if cancels
            .get(&conversation_id)
            .is_some_and(|current| Arc::ptr_eq(current, &cancel))
        {
            cancels.remove(&conversation_id);
        }
    }
    if let Err(error) = &result {
        if error != HERMES_STOPPED && !input.trim().is_empty() {
            let memories = super::agent_chats::load_attached_memories(state.inner(), &memory_ids)
                .await
                .unwrap_or_default()
                .iter()
                .map(super::agent_chats::attached_memory)
                .collect();
            let failed = super::agent_chats::record_failed_message(
                state.inner(),
                &conversation_id,
                super::agent_chats::AgentChatMessage {
                    role: "user".to_string(),
                    content: input.trim().to_string(),
                    at: sent_at,
                    memories,
                    failed: true,
                    auto_memories: Vec::new(),
                    tools_used: Vec::new(),
                },
            );
            if let Err(err) = failed {
                tracing::warn!(%err, "agent_chats:record_failed");
            }
        }
    }
    result
}

/// Waits for a reply unless the person stops it first. Dropping the send
/// closes its connection, so nothing arrives or is recorded afterward.
async fn unless_stopped<T>(
    send: impl std::future::Future<Output = Result<T, String>>,
    cancel: &tokio::sync::Notify,
) -> Result<T, String> {
    tokio::select! {
        result = send => result,
        _ = cancel.notified() => Err(HERMES_STOPPED.to_string()),
    }
}

/// Stops waiting for the reply to a chat's message in flight.
#[tauri::command]
pub async fn cancel_hermes_message(conversation_id: String) -> Result<(), String> {
    if let Some(cancel) = hermes_cancels().lock().get(&conversation_id) {
        cancel.notify_one();
    }
    Ok(())
}

async fn deliver_hermes_message(
    state: &Arc<AppState>,
    conversation_id: String,
    input: String,
    memory_ids: Vec<String>,
) -> Result<HermesChatReply, String> {
    let attached = super::agent_chats::load_attached_memories(state, &memory_ids).await?;
    let status = ensure_hermes_gateway_ready(state, 12_000).await?;

    let api_key = read_hermes_api_key(state)
        .ok_or_else(|| "FNDR could not read the Hermes API server key.".to_string())?;
    let user_text = input.trim().to_string();
    if user_text.is_empty() {
        return Err("Message cannot be empty.".to_string());
    }
    let sent_at = chrono::Utc::now().timestamp_millis();
    let related = read_hermes_setup_record(state)
        .as_ref()
        .is_some_and(sends_related_memories);
    let snippets = if related {
        crate::operator::memory::snippets(state, &user_text).await
    } else {
        Vec::new()
    };
    let auto_memories: Vec<super::agent_chats::AttachedMemory> = snippets
        .iter()
        .map(|snippet| super::agent_chats::AttachedMemory {
            id: snippet.memory_id.clone(),
            title: if snippet.title.trim().is_empty() {
                snippet.app.clone()
            } else {
                snippet.title.clone()
            },
            app_name: snippet.app.clone(),
            timestamp: snippet.timestamp,
        })
        .collect();
    let input = format!(
        "{}{}{}",
        memory_context(&snippets).trim_start(),
        super::agent_chats::memory_context_block(&attached, snippets.len()),
        user_text
    );

    let instructions = crate::inference::prompts::HERMES_CHAT_INSTRUCTIONS;
    let request_body = serde_json::json!({
        "model": "hermes-agent",
        "input": input,
        "conversation": conversation_id,
        "store": true,
        "instructions": instructions
    });

    let model_host = match status.provider_kind.as_deref() {
        Some("codex") => "chatgpt.com".to_string(),
        Some("openrouter") => "openrouter.ai".to_string(),
        _ => status
            .base_url
            .as_deref()
            .and_then(|url| url.parse::<reqwest::Url>().ok())
            .and_then(|url| url.host_str().map(str::to_string))
            .unwrap_or_else(|| "unknown".to_string()),
    };
    crate::privacy_proof::record_model_request_including(
        crate::privacy_proof::Feature::HermesChat,
        &model_host,
        input.len() + instructions.len(),
        if snippets.is_empty() && attached.is_empty() {
            &[]
        } else {
            &["memories"]
        },
    );

    let client = llm_http_client().map_err(|e| format!("HTTP client: {e}"))?;
    let url = format!("{}/v1/responses", status.api_url.trim_end_matches('/'));
    let (status_code, json) =
        post_json_response(&client, &url, &request_body, Some(api_key.as_str()))
            .await
            .map_err(|e| format!("Failed to reach the Hermes API server: {e}"))?;

    if !status_code.is_success() {
        return Err(json
            .get("error")
            .and_then(|value| value.get("message"))
            .and_then(|value| value.as_str())
            .or_else(|| json.get("detail").and_then(|value| value.as_str()))
            .unwrap_or("Hermes API request failed.")
            .to_string());
    }

    let response_id = json
        .get("id")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();

    let tools_used = tools_used_in(&json);
    let content = json
        .get("output")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    if item.get("type").and_then(|value| value.as_str()) == Some("message") {
                        item.get("content")
                            .and_then(|value| value.as_array())
                            .map(|parts| {
                                parts
                                    .iter()
                                    .filter_map(|part| {
                                        part.get("text").and_then(|value| value.as_str())
                                    })
                                    .collect::<Vec<_>>()
                                    .join("\n")
                            })
                    } else {
                        None
                    }
                })
                .filter(|text| !text.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n\n")
        })
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| {
            "Hermes completed the turn, but no assistant text was returned.".to_string()
        });

    let memories = attached
        .iter()
        .map(super::agent_chats::attached_memory)
        .collect();
    let history = super::agent_chats::record_exchange(
        state,
        &conversation_id,
        super::agent_chats::AgentChatMessage {
            role: "user".to_string(),
            content: user_text,
            at: sent_at,
            memories,
            failed: false,
            auto_memories: auto_memories.clone(),
            tools_used: Vec::new(),
        },
        super::agent_chats::AgentChatMessage {
            role: "assistant".to_string(),
            content: content.clone(),
            at: chrono::Utc::now().timestamp_millis(),
            memories: Vec::new(),
            failed: false,
            auto_memories: Vec::new(),
            tools_used: tools_used.clone(),
        },
    );
    if let Err(err) = history {
        // The reply still reaches the user; only the history entry is lost.
        tracing::warn!(%err, "agent_chats:record_failed");
    }

    Ok(HermesChatReply {
        response_id,
        conversation_id,
        content,
        auto_memories,
        tools_used,
    })
}

#[cfg(test)]
mod tests {

    /// Regression: status read the supervisor three times in one struct
    /// literal; the first guard was still held, so the second lock deadlocked
    /// and the Agent tab never left "Not set up".
    #[test]
    fn gateway_snapshot_reads_the_supervisor_without_deadlocking() {
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            done.send(gateway_snapshot()).unwrap();
        });
        let (state, restarts, reconnect) = finished
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("status must not deadlock on the supervisor lock");
        assert_eq!((state.as_str(), restarts, reconnect), ("stopped", 0, false));
    }

    use super::*;

    #[test]
    fn an_answer_reports_what_hermes_did_on_the_way_once_each_in_order() {
        // The shape the pinned gateway returned in the 2026-10-07 live check.
        let response = serde_json::json!({ "output": [
            { "type": "function_call", "name": "mcp_fndr_memory_search_full_context", "arguments": "{}", "call_id": "a" },
            { "type": "function_call_output", "call_id": "a", "output": "..." },
            { "type": "function_call", "name": "todo", "arguments": "{}", "call_id": "b" },
            { "type": "function_call_output", "call_id": "b", "output": "..." },
            { "type": "function_call", "name": "mcp_fndr_memory_timeline", "arguments": "{}", "call_id": "c" },
            { "type": "function_call", "name": "terminal", "arguments": "{}", "call_id": "d" },
            { "type": "message", "content": [{ "type": "output_text", "text": "Done." }] }
        ]});
        assert_eq!(
            tools_used_in(&response),
            [
                "searched FNDR memories",
                "kept a planning list",
                "used terminal"
            ]
        );
        assert!(
            tools_used_in(&serde_json::json!({ "output": [{ "type": "message" }] })).is_empty()
        );
        assert!(tools_used_in(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn a_leftover_gateway_is_stopped_only_when_its_id_is_still_a_gateway() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join(GATEWAY_PID_FILE);
        let gateway = |_: i32| "/x/venv/bin/python /x/src/hermes gateway".to_string();
        let something_else = |_: i32| "/Applications/Safari.app/Contents/MacOS/Safari".to_string();

        std::fs::write(&pid_file, "4242\n").unwrap();
        assert_eq!(take_stale_gateway_pid(dir.path(), gateway), Some(4242));
        assert!(!pid_file.exists(), "the saved id is used once");
        assert_eq!(take_stale_gateway_pid(dir.path(), gateway), None);

        // The id was reused by another app after FNDR crashed.
        std::fs::write(&pid_file, "4242").unwrap();
        assert_eq!(take_stale_gateway_pid(dir.path(), something_else), None);
        assert!(!pid_file.exists());

        for unusable in ["", "not a number", "0", "1", "-7"] {
            std::fs::write(&pid_file, unusable).unwrap();
            assert_eq!(
                take_stale_gateway_pid(dir.path(), gateway),
                None,
                "{unusable:?}"
            );
        }
    }

    #[tokio::test]
    async fn stopping_ends_the_wait_for_a_reply_and_a_finished_reply_is_kept() {
        let cancel = Arc::new(tokio::sync::Notify::new());
        let stop = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            stop.notify_one();
        });
        let never = std::future::pending::<Result<String, String>>();
        let stopped = tokio::time::timeout(Duration::from_secs(2), unless_stopped(never, &cancel))
            .await
            .expect("the wait ends");
        assert_eq!(stopped, Err(HERMES_STOPPED.to_string()));

        let fresh = tokio::sync::Notify::new();
        let reply = unless_stopped(async { Ok("done".to_string()) }, &fresh).await;
        assert_eq!(reply, Ok("done".to_string()));
    }

    fn saved(provider: &str, base_url: Option<&str>, related_memories: bool) -> HermesSetupRecord {
        HermesSetupRecord {
            provider_kind: provider.to_string(),
            model_name: "m".to_string(),
            base_url: base_url.map(str::to_string),
            related_memories,
        }
    }

    #[test]
    fn memories_fndr_finds_itself_go_to_a_cloud_provider_only_when_turned_on() {
        for cloud in [
            saved("codex", None, false),
            saved("openrouter", None, false),
            saved("custom", Some("https://api.example.com/v1"), false),
            saved("ollama", Some("http://192.168.1.20:11434/v1"), false),
            saved("custom", None, false),
        ] {
            assert!(!provider_is_local(&cloud), "{cloud:?}");
            assert!(!sends_related_memories(&cloud), "{cloud:?}");
        }
        assert!(sends_related_memories(&saved("codex", None, true)));
        for local in [
            saved("ollama", None, false),
            saved("ollama", Some("http://127.0.0.1:11434/v1"), false),
            saved("custom", Some("http://localhost:8000/v1"), false),
        ] {
            assert!(provider_is_local(&local), "{local:?}");
            assert!(sends_related_memories(&local), "{local:?}");
        }
    }

    #[test]
    fn a_setup_saved_before_the_switch_existed_keeps_related_memories_off() {
        let record: HermesSetupRecord =
            serde_json::from_str(r#"{"provider_kind":"codex","model_name":"gpt-6-sol"}"#).unwrap();
        assert!(!sends_related_memories(&record));
    }

    #[test]
    fn hermes_is_given_a_planning_list_and_nothing_that_acts() {
        let tools = crate::ipc::commands::hermes_codex::HERMES_TOOLS_YAML;
        assert_eq!(tools, "platform_toolsets:\n  api_server: [todo]\n");
        for acting in ["terminal", "file", "browser", "code", "cron"] {
            assert!(!tools.contains(acting), "{acting}");
        }
    }

    #[test]
    fn a_failed_gateway_start_reports_its_last_output_bounded() {
        let mut tail = std::collections::VecDeque::new();
        for n in 0..(GATEWAY_STDERR_LINES + 5) {
            remember_gateway_stderr(&mut tail, &format!("line {n}"));
        }
        remember_gateway_stderr(&mut tail, "   ");
        remember_gateway_stderr(&mut tail, &"x".repeat(GATEWAY_STDERR_LINE_CHARS * 2));
        assert_eq!(tail.len(), GATEWAY_STDERR_LINES);
        assert_eq!(
            tail.back().unwrap().chars().count(),
            GATEWAY_STDERR_LINE_CHARS
        );
        assert_eq!(
            gateway_exit_message("with status 1", Some("ModuleNotFoundError: yaml")),
            "Hermes gateway exited with status 1. Last output: ModuleNotFoundError: yaml"
        );
        assert_eq!(
            gateway_exit_message("with status 1", None),
            "Hermes gateway exited with status 1."
        );
    }

    #[test]
    fn only_a_hermes_gateway_process_counts_as_a_stale_gateway() {
        assert!(is_hermes_gateway_command(
            "/Users/a/Library/Application Support/x/hermes-runtime/venv/bin/python /x/src/hermes gateway"
        ));
        assert!(is_hermes_gateway_command(
            "/Users/a/.local/bin/hermes gateway"
        ));
        assert!(!is_hermes_gateway_command(
            "/usr/bin/python3 server.py gateway"
        ));
        assert!(!is_hermes_gateway_command(
            "/Users/a/.local/bin/hermes chat"
        ));
        assert!(!is_hermes_gateway_command(""));
    }

    #[cfg(unix)]
    #[test]
    fn secret_files_are_readable_by_the_owner_only_even_when_they_already_exist() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".env");
        std::fs::write(&path, "OLD=1\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

        write_private(&path, "OPENROUTER_API_KEY=k\n").unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "OPENROUTER_API_KEY=k\n"
        );
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    fn record(provider: &str, model: &str, base_url: Option<&str>) -> HermesSetupRecord {
        HermesSetupRecord {
            provider_kind: provider.to_string(),
            model_name: model.to_string(),
            base_url: base_url.map(str::to_string),
            related_memories: false,
        }
    }

    #[test]
    fn toml_cannot_quote_a_bare_string_which_is_why_saves_failed() {
        let err = toml::to_string(&"gpt-6-sol").unwrap_err().to_string();
        assert!(err.contains("unsupported rust type"), "{err}");
    }

    #[test]
    fn writes_the_codex_provider_hermes_expects() {
        assert_eq!(
            hermes_config_yaml(&record("codex", "gpt-6-sol", None)).unwrap(),
            "model:\n  provider: openai-codex\n  default: \"gpt-6-sol\"\n"
        );
    }

    #[test]
    fn quotes_values_yaml_would_otherwise_misread() {
        let yaml = hermes_config_yaml(&record("ollama", "llama3.2:latest", None)).unwrap();
        assert!(yaml.contains("default: \"llama3.2:latest\""));
        assert!(yaml.contains(&format!("base_url: \"{OLLAMA_BASE_URL}\"")));

        let yaml = hermes_config_yaml(&record(
            "custom",
            "my \"best\" model",
            Some(" http://localhost:8000/v1 "),
        ))
        .unwrap();
        assert!(yaml.contains(r#"default: "my \"best\" model""#));
        assert!(yaml.contains("base_url: \"http://localhost:8000/v1\""));
    }

    #[test]
    fn a_custom_endpoint_needs_a_base_url() {
        assert!(hermes_config_yaml(&record("custom", "m", None)).is_err());
        assert!(hermes_config_yaml(&record("custom", "m", Some("  "))).is_err());
    }

    #[test]
    fn other_providers_are_written_through_as_quoted_ids() {
        assert_eq!(
            hermes_config_yaml(&record("openrouter", "openai/gpt-5-mini", None)).unwrap(),
            "model:\n  provider: \"openrouter\"\n  default: \"openai/gpt-5-mini\"\n"
        );
    }
}
