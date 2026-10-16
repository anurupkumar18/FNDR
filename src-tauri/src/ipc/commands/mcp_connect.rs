//! Setup commands for local MCP clients. A status response never includes the
//! bearer token; only an explicit config preview contains it for copying.

use crate::mcp::{self, connect};
use crate::AppState;
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConnectStatus {
    pub running: bool,
    pub endpoint: Option<String>,
    pub actions_blocked: bool,
    pub keep_running: bool,
    pub hide_more: bool,
    pub clients: Vec<connect::ClientCard>,
}

fn status_payload(
    env: &connect::Env,
    server_status: &mcp::McpServerStatus,
    actions_blocked: bool,
    hide_more: bool,
) -> McpConnectStatus {
    let server = server_status.running.then(|| connect::Server {
        endpoint: server_status.endpoint.clone(),
        token: server_status.token.clone(),
    });
    McpConnectStatus {
        running: server_status.running,
        endpoint: server.as_ref().map(|server| server.endpoint.clone()),
        actions_blocked,
        keep_running: connect::ConnectState::load(&env.fndr_dir).keep_running,
        hide_more,
        clients: connect::Client::ALL
            .into_iter()
            .map(|client| connect::client_card(env, client, server.as_ref()))
            .collect(),
    }
}

fn client(id: &str) -> Result<connect::Client, String> {
    connect::Client::from_id(id).ok_or_else(|| format!("Unknown MCP client: {id}"))
}

fn is_port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

async fn ensure_server(
    app: AppHandle,
    state: Arc<AppState>,
    env: &connect::Env,
) -> Result<mcp::McpServerStatus, String> {
    let current = mcp::status();
    if current.running {
        return Ok(current);
    }
    let saved = connect::ConnectState::load(&env.fndr_dir);
    let port = saved.port.filter(|port| is_port_free(*port));
    let started = mcp::start(Some(app.clone()), state.clone(), None, port)
        .await
        .or_else(|error| {
            if port.is_some() {
                Ok(mcp::status())
            } else {
                Err(error)
            }
        })?;
    let started = if started.running {
        started
    } else {
        mcp::start(Some(app), state, None, None).await?
    };
    let mut saved = saved;
    saved.port = Some(started.port);
    saved
        .save(&env.fndr_dir)
        .map_err(|error| format!("MCP started, but its port could not be saved: {error}"))?;
    Ok(started)
}

fn server(status: &mcp::McpServerStatus) -> connect::Server {
    connect::Server {
        endpoint: status.endpoint.clone(),
        token: status.token.clone(),
    }
}

#[tauri::command]
pub async fn mcp_connect_status(
    state: State<'_, Arc<AppState>>,
) -> Result<McpConnectStatus, String> {
    let env = connect::Env::detect();
    let config = state.config.read();
    Ok(status_payload(
        &env,
        &mcp::status(),
        config.actions_kill_switch,
        config.mcp_strict_privacy,
    ))
}

#[tauri::command]
pub async fn mcp_connect_preview(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    client_id: String,
) -> Result<connect::Preview, String> {
    let client = client(&client_id)?;
    let env = connect::Env::detect();
    let status = ensure_server(app, state.inner().clone(), &env).await?;
    Ok(connect::preview(&env, client, &server(&status)))
}

#[tauri::command]
pub async fn mcp_connect_apply(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    client_id: String,
) -> Result<connect::Outcome, String> {
    let client = client(&client_id)?;
    let actions_blocked = state.config.read().actions_kill_switch;
    if actions_blocked {
        return Err(connect::KILL_SWITCH_MESSAGE.to_string());
    }
    let env = connect::Env::detect();
    let status = ensure_server(app, state.inner().clone(), &env).await?;
    connect::apply(&env, false, client, &server(&status))
}

#[tauri::command]
pub async fn mcp_connect_test(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<connect::TestReport, String> {
    let env = connect::Env::detect();
    let status = ensure_server(app, state.inner().clone(), &env).await?;
    Ok(connect::test_connection(&status.endpoint, &status.token).await)
}

#[tauri::command]
pub async fn mcp_connect_rotate_token(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<McpConnectStatus, String> {
    if state.config.read().actions_kill_switch {
        return Err(connect::KILL_SWITCH_MESSAGE.to_string());
    }
    let env = connect::Env::detect();
    let saved = connect::ConnectState::load(&env.fndr_dir);
    mcp::token::rotate().map_err(|error| format!("Could not rotate MCP token: {error}"))?;
    mcp::stop().await;
    let status = ensure_server(app, state.inner().clone(), &env).await?;
    for id in &saved.clients {
        if let Some(client) = connect::Client::from_id(id) {
            connect::apply(&env, false, client, &server(&status))?;
        }
    }
    Ok(status_payload(
        &env,
        &status,
        false,
        state.config.read().mcp_strict_privacy,
    ))
}

#[tauri::command]
pub async fn mcp_connect_disconnect(client_id: String) -> Result<connect::Outcome, String> {
    let client = client(&client_id)?;
    let env = connect::Env::detect();
    let result = connect::disconnect(&env, client)?;
    if connect::ConnectState::load(&env.fndr_dir)
        .clients
        .is_empty()
    {
        mcp::stop().await;
    }
    Ok(result)
}

#[tauri::command]
pub async fn mcp_connect_set_keep_running(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    keep_running: bool,
) -> Result<McpConnectStatus, String> {
    let env = connect::Env::detect();
    let mut saved = connect::ConnectState::load(&env.fndr_dir);
    saved.keep_running = keep_running;
    saved
        .save(&env.fndr_dir)
        .map_err(|error| error.to_string())?;
    if keep_running && !saved.clients.is_empty() {
        ensure_server(app, state.inner().clone(), &env).await?;
    } else if !keep_running {
        mcp::stop().await;
    }
    let config = state.config.read();
    Ok(status_payload(
        &env,
        &mcp::status(),
        config.actions_kill_switch,
        config.mcp_strict_privacy,
    ))
}

#[tauri::command]
pub async fn mcp_connect_set_hide_more(
    state: State<'_, Arc<AppState>>,
    hide_more: bool,
) -> Result<(), String> {
    let mut current = state.config.write();
    let mut next = current.clone();
    next.mcp_strict_privacy = hide_more;
    next.save().map_err(|error| error.to_string())?;
    *current = next;
    Ok(())
}

#[tauri::command]
pub async fn mcp_connect_revoke() -> Result<String, String> {
    let env = connect::Env::detect();
    let saved = connect::ConnectState::load(&env.fndr_dir);
    let mut errors = Vec::new();
    for id in &saved.clients {
        if let Some(client) = connect::Client::from_id(id) {
            if let Err(error) = connect::disconnect(&env, client) {
                errors.push(error);
            }
        }
    }
    let rotation = mcp::token::rotate();
    mcp::stop().await;
    connect::ConnectState::default()
        .save(&env.fndr_dir)
        .map_err(|error| error.to_string())?;
    if let Err(error) = rotation {
        errors.push(format!("Could not revoke the MCP token: {error}"));
    }
    if errors.is_empty() {
        Ok("MCP access was revoked and the server stopped.".to_string())
    } else {
        Err(format!(
            "MCP access was stopped, but some cleanup failed: {}",
            errors.join("; ")
        ))
    }
}

pub async fn resume_if_connected(app: AppHandle, state: Arc<AppState>) -> Result<(), String> {
    let env = connect::Env::detect();
    let saved = connect::ConnectState::load(&env.fndr_dir);
    if saved.clients.is_empty() || !saved.keep_running {
        return Ok(());
    }
    let status = ensure_server(app, state.clone(), &env).await?;
    if saved.port != Some(status.port) {
        for id in &saved.clients {
            if let Some(client) = connect::Client::from_id(id) {
                connect::apply(
                    &env,
                    state.config.read().actions_kill_switch,
                    client,
                    &server(&status),
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_lists_all_three_clients_and_the_action_gate() {
        let home = tempfile::tempdir().unwrap();
        let env = connect::Env {
            home: home.path().to_path_buf(),
            fndr_dir: home.path().join(".fndr"),
            claude: None,
            npx: None,
        };
        let payload = status_payload(&env, &mcp::status(), true, false);
        assert_eq!(payload.actions_blocked, true);
        assert_eq!(payload.clients.len(), 3);
        assert_eq!(payload.clients[0].id, "claude_code");
        assert_eq!(payload.clients[1].id, "claude_desktop");
        assert_eq!(payload.clients[2].id, "cursor");
    }
}
