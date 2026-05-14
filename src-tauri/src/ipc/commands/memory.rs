//! Single-memory Tauri commands.

use crate::memory::reopen::ReopenKind;
use crate::AppState;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn delete_memory(
    state: State<'_, Arc<AppState>>,
    memory_id: String,
) -> Result<bool, String> {
    let existing = state
        .inner()
        .store
        .get_memory_by_id(&memory_id)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;

    let deleted = state
        .inner()
        .store
        .delete_memory_by_id(&memory_id)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;

    if deleted == 0 {
        return Ok(false);
    }

    if let Err(err) =
        super::todos::apply_memory_deletion_to_tasks(&state.inner().store, &memory_id).await
    {
        tracing::warn!(
            "Task cleanup after deleting memory {} failed: {}",
            memory_id,
            err
        );
    }

    state.invalidate_memory_derived_caches();

    if let Some(record) = existing {
        if let Some(path) = record.screenshot_path {
            if let Err(err) = std::fs::remove_file(&path) {
                tracing::warn!("Failed to delete screenshot artifact {}: {}", path, err);
            }
        }
    }

    tracing::info!("Deleted memory record {}", memory_id);
    Ok(true)
}

#[tauri::command]
pub async fn reopen_memory(
    state: State<'_, Arc<AppState>>,
    memory_id: String,
) -> Result<bool, String> {
    let record = state
        .inner()
        .store
        .get_memory_by_id(&memory_id)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?
        .ok_or_else(|| format!("Memory not found: {}", memory_id))?;

    let Some(target) = resolve_reopen_target(&record) else {
        return Ok(false);
    };

    open_reopen_target(target)?;
    Ok(true)
}

#[derive(Debug, Clone)]
enum ResolvedReopenTarget {
    BrowserUrl(String),
    FilePath(PathBuf),
    AppBundle(String),
    AppDeepLink(String),
}

fn resolve_reopen_target(record: &crate::storage::MemoryRecord) -> Option<ResolvedReopenTarget> {
    let typed = match &record.reopen_kind {
        ReopenKind::BrowserUrl => record
            .reopen_url
            .as_deref()
            .map(str::trim)
            .filter(|value| is_http_url(value))
            .map(|value| ResolvedReopenTarget::BrowserUrl(value.to_string())),
        ReopenKind::FilePath => record
            .reopen_file_path
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .map(ResolvedReopenTarget::FilePath),
        ReopenKind::AppBundle => record
            .reopen_app_bundle_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| ResolvedReopenTarget::AppBundle(value.to_string())),
        ReopenKind::AppDeepLink => record
            .reopen_app_deep_link
            .as_deref()
            .map(str::trim)
            .filter(|value| is_deep_link(value))
            .map(|value| ResolvedReopenTarget::AppDeepLink(value.to_string())),
        ReopenKind::Unknown => None,
    };
    if typed.is_some() {
        return typed;
    }

    if let Some(legacy) = parse_legacy_reopen_marker(&record.memory_context) {
        if is_http_url(&legacy) {
            return Some(ResolvedReopenTarget::BrowserUrl(legacy));
        }
        if let Some(path) = legacy.strip_prefix("file://") {
            return Some(ResolvedReopenTarget::FilePath(PathBuf::from(path)));
        }
        if is_deep_link(&legacy) {
            return Some(ResolvedReopenTarget::AppDeepLink(legacy));
        }
    }

    if let Some(url) = record
        .url
        .as_deref()
        .map(str::trim)
        .filter(|value| is_http_url(value))
    {
        return Some(ResolvedReopenTarget::BrowserUrl(url.to_string()));
    }

    if let Some(file_path) = record
        .files_touched
        .iter()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
    {
        return Some(ResolvedReopenTarget::FilePath(PathBuf::from(file_path)));
    }

    if let Some(bundle) = record
        .bundle_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(ResolvedReopenTarget::AppBundle(bundle.to_string()));
    }

    None
}

fn parse_legacy_reopen_marker(memory_context: &str) -> Option<String> {
    for line in memory_context.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("Reopen: ") {
            let value = rest.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn is_http_url(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn is_deep_link(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || is_http_url(trimmed) {
        return false;
    }
    trimmed.contains("://")
}

fn open_reopen_target(target: ResolvedReopenTarget) -> Result<(), String> {
    match target {
        ResolvedReopenTarget::BrowserUrl(url) => open_with_system(&url),
        ResolvedReopenTarget::AppDeepLink(link) => open_with_system(&link),
        ResolvedReopenTarget::FilePath(path) => {
            let absolute = canonicalize_relaxed(&path)?;
            open_path_with_system(&absolute)
        }
        ResolvedReopenTarget::AppBundle(bundle_id) => open_app_bundle(&bundle_id),
    }
}

fn canonicalize_relaxed(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        return path
            .canonicalize()
            .map_err(|err| format!("Failed to resolve file path {}: {}", path.display(), err));
    }
    Err(format!("File path no longer exists: {}", path.display()))
}

#[cfg(target_os = "macos")]
fn open_with_system(target: &str) -> Result<(), String> {
    Command::new("open")
        .arg(target)
        .spawn()
        .map_err(|err| format!("Failed to open target '{}': {}", target, err))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_with_system(target: &str) -> Result<(), String> {
    Command::new("cmd")
        .arg("/C")
        .arg("start")
        .arg("")
        .arg(target)
        .spawn()
        .map_err(|err| format!("Failed to open target '{}': {}", target, err))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_with_system(target: &str) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(target)
        .spawn()
        .map_err(|err| format!("Failed to open target '{}': {}", target, err))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_path_with_system(path: &Path) -> Result<(), String> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map_err(|err| format!("Failed to open path '{}': {}", path.display(), err))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_path_with_system(path: &Path) -> Result<(), String> {
    Command::new("cmd")
        .arg("/C")
        .arg("start")
        .arg("")
        .arg(path)
        .spawn()
        .map_err(|err| format!("Failed to open path '{}': {}", path.display(), err))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_path_with_system(path: &Path) -> Result<(), String> {
    Command::new("xdg-open")
        .arg(path)
        .spawn()
        .map_err(|err| format!("Failed to open path '{}': {}", path.display(), err))?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn open_app_bundle(bundle_id: &str) -> Result<(), String> {
    Command::new("open")
        .arg("-b")
        .arg(bundle_id)
        .spawn()
        .map_err(|err| format!("Failed to open app bundle '{}': {}", bundle_id, err))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn open_app_bundle(bundle_id: &str) -> Result<(), String> {
    Err(format!(
        "Opening app bundle '{}' is only supported on macOS",
        bundle_id
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_reopen_target_prefers_typed_url() {
        let record = crate::storage::MemoryRecord {
            reopen_kind: ReopenKind::BrowserUrl,
            reopen_url: Some("https://typed.example".to_string()),
            memory_context: "Reopen: https://legacy.example".to_string(),
            ..Default::default()
        };

        match resolve_reopen_target(&record) {
            Some(ResolvedReopenTarget::BrowserUrl(url)) => {
                assert_eq!(url, "https://typed.example");
            }
            other => panic!("unexpected target: {other:?}"),
        }
    }

    #[test]
    fn resolve_reopen_target_supports_legacy_marker_fallback() {
        let record = crate::storage::MemoryRecord {
            memory_context: "Reopen: https://legacy.example".to_string(),
            ..Default::default()
        };

        match resolve_reopen_target(&record) {
            Some(ResolvedReopenTarget::BrowserUrl(url)) => {
                assert_eq!(url, "https://legacy.example");
            }
            other => panic!("unexpected target: {other:?}"),
        }
    }
}
