//! Setup center: every component FNDR depends on, its state, and the one
//! action that fixes it, so a downloaded FNDR can install or update the rest
//! from inside the app.

use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::State;

use super::codex_account::{codex_home_dir, ready_executable};
use super::computer_use::detect_backend;
use crate::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentState {
    Ready,
    Missing,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentAction {
    /// FNDR installs it (`install_component`).
    Install,
    /// The person signs in with ChatGPT.
    SignIn,
    /// The person installs it from a web page (`url`).
    OpenUrl,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupComponent {
    pub id: &'static str,
    pub name: &'static str,
    /// What it is for, in one line.
    pub purpose: &'static str,
    pub required: bool,
    pub state: ComponentState,
    pub version: Option<String>,
    pub detail: Option<String>,
    pub action: Option<ComponentAction>,
    pub url: Option<&'static str>,
}

const NODE_URL: &str = "https://nodejs.org/en/download";

/// An `npm` FNDR can run, for components published there.
fn detect_npm() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join("npm"))
                .collect()
        })
        .unwrap_or_default();
    candidates.extend(["/opt/homebrew/bin/npm", "/usr/local/bin/npm"].map(PathBuf::from));
    candidates.into_iter().find(|candidate| candidate.is_file())
}

/// How FNDR installs a component. Nothing outside this list is installable.
#[derive(Debug, PartialEq, Eq)]
enum Installer {
    /// An npm package, installed globally.
    Npm(&'static str),
    /// OpenAI's official Codex installer: no Node.js, installs under
    /// `~/.codex/packages/standalone` and links `~/.local/bin/codex`.
    CodexScript,
    /// FNDR's own pinned Hermes.
    Hermes,
}

const CODEX_INSTALL_SCRIPT_URL: &str = "https://chatgpt.com/codex/install.sh";

fn installer_for(id: &str) -> Option<Installer> {
    match id {
        "computer_use" => Some(Installer::Npm("open-computer-use@0.3.6")),
        "codex_cli" => Some(Installer::CodexScript),
        "hermes" => Some(Installer::Hermes),
        _ => None,
    }
}

fn component(
    id: &'static str,
    name: &'static str,
    purpose: &'static str,
    required: bool,
) -> SetupComponent {
    SetupComponent {
        id,
        name,
        purpose,
        required,
        state: ComponentState::Missing,
        version: None,
        detail: None,
        action: None,
        url: None,
    }
}

/// What to offer for something FNDR installs through npm.
fn npm_action(mut item: SetupComponent, npm_available: bool) -> SetupComponent {
    if npm_available {
        item.action = Some(ComponentAction::Install);
    } else {
        item.action = Some(ComponentAction::OpenUrl);
        item.url = Some(NODE_URL);
        item.detail = Some("Install Node.js first; FNDR then installs this for you.".to_string());
    }
    item
}

fn codex_signed_in() -> bool {
    std::fs::read_to_string(codex_home_dir().join("auth.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .is_some_and(|auth| auth.get("tokens").is_some_and(|t| t.is_object()))
}

#[tauri::command]
pub async fn setup_components(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<SetupComponent>, String> {
    let npm = detect_npm().is_some();
    let mut items = Vec::new();

    let mut codex = component(
        "codex_cli",
        "Codex",
        "Signs in with ChatGPT and runs Notch Do and Hermes on your plan.",
        true,
    );
    match ready_executable() {
        Ok(path) => {
            codex.state = ComponentState::Ready;
            codex.version = std::process::Command::new(&path)
                .arg("--version")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        }
        Err(error) => {
            codex.action = Some(ComponentAction::Install);
            codex.detail = Some(error);
        }
    }
    items.push(codex);

    let mut sign_in = component(
        "chatgpt",
        "ChatGPT sign-in",
        "One sign-in for Notch Do and Hermes. No API key.",
        true,
    );
    if codex_signed_in() {
        sign_in.state = ComponentState::Ready;
    } else {
        sign_in.action = Some(ComponentAction::SignIn);
    }
    items.push(sign_in);

    let mut computer = component(
        "computer_use",
        "Computer Use",
        "Lets Notch Do click and type in apps.",
        false,
    );
    match detect_backend() {
        Some(backend) => {
            computer.state = ComponentState::Ready;
            computer.detail = Some(backend.path().display().to_string());
        }
        None => computer = npm_action(computer, npm),
    }
    items.push(computer);

    let mut hermes = component(
        "hermes",
        "Hermes agent",
        "The agent in the Agent tab, on your ChatGPT plan.",
        false,
    );
    let runtime_root = state.inner().app_data_dir.join("hermes-runtime");
    match super::hermes_codex::installed_hermes_version(&runtime_root) {
        Some(version) => {
            hermes.state = ComponentState::Ready;
            hermes.version = Some(version);
        }
        None => hermes.action = Some(ComponentAction::Install),
    }
    items.push(hermes);

    Ok(items)
}

/// Installs a component FNDR knows how to install. Unknown ids are refused.
#[tauri::command]
pub async fn install_component(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    let installer = installer_for(&id).ok_or_else(|| format!("FNDR can't install \"{id}\"."))?;
    match installer {
        Installer::Hermes => {
            let app_state = state.inner().clone();
            tokio::task::spawn_blocking(move || {
                super::hermes_agent::ensure_pinned_hermes(&app_state)
            })
            .await
            .map_err(|e| e.to_string())?
        }
        Installer::CodexScript => {
            crate::privacy_proof::record_egress("chatgpt.com");
            run_install(
                tokio::process::Command::new("/bin/sh")
                    .args(["-c", &format!("curl -fsSL {CODEX_INSTALL_SCRIPT_URL} | sh")]),
                "the Codex installer",
            )
            .await
        }
        Installer::Npm(package) => {
            let npm =
                detect_npm().ok_or_else(|| "Install Node.js first, then try again.".to_string())?;
            crate::privacy_proof::record_egress("registry.npmjs.org");
            run_install(
                tokio::process::Command::new(&npm).args(["install", "-g", package]),
                package,
            )
            .await
        }
    }
}

async fn run_install(command: &mut tokio::process::Command, what: &str) -> Result<(), String> {
    let output = command
        .output()
        .await
        .map_err(|e| format!("Could not start {what}: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Installing {what} failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .last()
                .unwrap_or("unknown error")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_components_are_installable_and_codex_needs_no_node() {
        assert_eq!(installer_for("codex_cli"), Some(Installer::CodexScript));
        assert_eq!(
            installer_for("computer_use"),
            Some(Installer::Npm("open-computer-use@0.3.6"))
        );
        assert_eq!(installer_for("hermes"), Some(Installer::Hermes));
        assert_eq!(installer_for("anything-else"), None);
        assert!(CODEX_INSTALL_SCRIPT_URL.starts_with("https://chatgpt.com/"));
    }

    #[test]
    fn without_node_the_action_points_at_its_download() {
        let item = npm_action(component("computer_use", "Computer Use", "x", false), false);
        assert_eq!(item.action, Some(ComponentAction::OpenUrl));
        assert_eq!(item.url, Some(NODE_URL));
        let item = npm_action(component("computer_use", "Computer Use", "x", false), true);
        assert_eq!(item.action, Some(ComponentAction::Install));
    }
}
