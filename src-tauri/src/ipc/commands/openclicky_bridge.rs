//! Screen Guide pointing through OpenClicky's local control bridge.
//!
//! OpenClicky (github.com/jasonkneen/openclicky) listens on 127.0.0.1:32123
//! and animates its own cursor to a point with a caption. The bridge requires
//! a token; FNDR reads it from the same places OpenClicky does for local
//! setups: `OPENCLICKY_BRIDGE_TOKEN`, then the secrets file. The bridge only
//! draws and speaks — it never clicks on FNDR's behalf.

use serde::Serialize;
use serde_json::json;
use std::path::PathBuf;

const BRIDGE_URL: &str = "http://127.0.0.1:32123";
const TOKEN_KEY: &str = "OPENCLICKY_BRIDGE_TOKEN";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenClickyBridgeStatus {
    /// OpenClicky is running and answered /health.
    pub reachable: bool,
    /// FNDR found a bridge token to authenticate with.
    pub token_found: bool,
    /// OpenClicky reports its own bridge token is set.
    pub bridge_token_configured: bool,
}

fn secrets_file_candidates() -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Some(custom) = std::env::var_os("OPENCLICKY_SECRETS_FILE") {
        files.push(PathBuf::from(custom));
    }
    if let Some(home) = std::env::var_os("HOME") {
        files.push(PathBuf::from(home).join(".config/openclicky/secrets.env"));
    }
    files
}

/// Reads `KEY=value` from a dotenv-style file, tolerating `export` and quotes.
fn env_file_value(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let line = line.trim();
        let line = line.strip_prefix("export ").unwrap_or(line);
        let (name, value) = line.split_once('=')?;
        if name.trim() != key {
            return None;
        }
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        (!value.is_empty()).then(|| value.to_string())
    })
}

fn bridge_token() -> Option<String> {
    if let Ok(value) = std::env::var(TOKEN_KEY) {
        if !value.trim().is_empty() {
            return Some(value.trim().to_string());
        }
    }
    secrets_file_candidates()
        .into_iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .find_map(|contents| env_file_value(&contents, TOKEN_KEY))
}

/// Converts a cue normalized to the main display (origin top-left) into
/// global AppKit points (origin bottom-left of the main display), the space
/// OpenClicky's bridge expects.
fn appkit_point(normalized_x: f64, normalized_y: f64, width_pts: f64, height_pts: f64) -> (f64, f64) {
    (normalized_x * width_pts, (1.0 - normalized_y) * height_pts)
}

pub(crate) async fn bridge_status() -> OpenClickyBridgeStatus {
    let token_found = bridge_token().is_some();
    let health = match crate::http_util::local_service_client() {
        Ok(client) => client.get(format!("{BRIDGE_URL}/health")).send().await.ok(),
        Err(_) => None,
    };
    let body = match health {
        Some(response) if response.status().is_success() => response.json::<serde_json::Value>().await.ok(),
        _ => None,
    };
    OpenClickyBridgeStatus {
        reachable: body.is_some(),
        token_found,
        bridge_token_configured: body
            .as_ref()
            .and_then(|b| b.get("bridgeTokenConfigured"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    }
}

/// Points OpenClicky's cursor at a normalized main-display cue. Returns an
/// error the caller can fall back from (FNDR's own cursor) when OpenClicky
/// isn't running or the token is missing or wrong.
/// `display_points` is the main display's size in points (not pixels).
pub(crate) async fn point_at(
    normalized_x: f64,
    normalized_y: f64,
    caption: Option<&str>,
    display_points: (f64, f64),
) -> Result<(), String> {
    let token = bridge_token().ok_or_else(|| format!("Add {TOKEN_KEY} to ~/.config/openclicky/secrets.env"))?;
    let (width, height) = display_points;
    if width <= 0.0 || height <= 0.0 {
        return Err("Could not read the main display size.".to_string());
    }
    let (x, y) = appkit_point(normalized_x, normalized_y, width, height);
    let client = crate::http_util::local_service_client().map_err(|e| e.to_string())?;
    let response = client
        .post(format!("{BRIDGE_URL}/cursor"))
        .header("x-openclicky-token", token)
        .json(&json!({ "x": x, "y": y, "caption": caption.unwrap_or("") }))
        .send()
        .await
        .map_err(|_| "OpenClicky isn't running.".to_string())?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!("OpenClicky refused the pointer ({}).", response.status()))
    }
}

#[tauri::command]
pub async fn openclicky_bridge_status() -> Result<OpenClickyBridgeStatus, String> {
    Ok(bridge_status().await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flips_the_y_axis_into_appkit_space() {
        assert_eq!(appkit_point(0.0, 0.0, 1440.0, 900.0), (0.0, 900.0));
        assert_eq!(appkit_point(0.5, 0.25, 1440.0, 900.0), (720.0, 675.0));
        assert_eq!(appkit_point(1.0, 1.0, 1440.0, 900.0), (1440.0, 0.0));
    }

    #[test]
    fn reads_tokens_from_dotenv_lines() {
        let file = "# comment\nexport OPENCLICKY_BRIDGE_TOKEN=\"abc123\"\nOTHER=1\n";
        assert_eq!(env_file_value(file, TOKEN_KEY), Some("abc123".into()));
        assert_eq!(env_file_value("OPENCLICKY_BRIDGE_TOKEN=\n", TOKEN_KEY), None);
        assert_eq!(env_file_value("OPENCLICKY_BRIDGE_TOKENX=1\n", TOKEN_KEY), None);
    }
}
