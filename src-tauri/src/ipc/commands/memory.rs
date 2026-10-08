//! Single-memory Tauri commands.

use crate::graph::GraphStore;
use crate::memory::reopen::{
    is_blocked_scheme, pick_moved_file, should_reveal_in_finder, url_with_pdf_page,
    url_with_text_anchor, volume_is_disconnected, volume_root, ReopenKind, ReopenOutcome,
};
use crate::storage::Store;
use crate::AppState;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tauri::State;

/// Deletes one memory and everything it owns: the row, its chunks (inside
/// `Store::delete_memory_by_id`), its screenshot artifact if any, and any
/// graph nodes/edges it produced (MEM-07 invariant 10 — a deletion must not
/// leave graph nodes orphaned).
pub(crate) async fn delete_memory_logic(
    store: &Store,
    graph: &GraphStore,
    memory_id: &str,
) -> Result<bool, String> {
    let existing = store
        .get_memory_by_id(memory_id)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;

    let deleted = store
        .delete_memory_by_id(memory_id)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;

    if deleted == 0 {
        return Ok(false);
    }

    if let Err(err) = super::todos::apply_memory_deletion_to_tasks(store, memory_id).await {
        tracing::warn!(
            "Task cleanup after deleting memory {} failed: {}",
            memory_id,
            err
        );
    }

    if let Some(record) = existing {
        if let Some(path) = record.screenshot_path {
            if let Err(err) = std::fs::remove_file(&path) {
                tracing::warn!("Failed to delete screenshot artifact {}: {}", path, err);
            }
        }
    }

    if let Err(err) = graph.delete_memory_node(memory_id).await {
        tracing::warn!(
            "Failed to delete graph node for memory {}: {}",
            memory_id,
            err
        );
    }

    tracing::info!("Deleted memory record {}", memory_id);
    Ok(true)
}

#[tauri::command]
pub async fn delete_memory(
    state: State<'_, Arc<AppState>>,
    memory_id: String,
) -> Result<bool, String> {
    let deleted =
        delete_memory_logic(&state.inner().store, &state.inner().graph, &memory_id).await?;
    if deleted {
        state.invalidate_memory_derived_caches();
    }
    Ok(deleted)
}

#[tauri::command]
pub async fn reopen_memory(
    state: State<'_, Arc<AppState>>,
    memory_id: String,
) -> Result<ReopenOutcome, String> {
    let record = state
        .inner()
        .store
        .get_memory_by_id(&memory_id)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?
        .ok_or_else(|| format!("Memory not found: {}", memory_id))?;

    let target = resolve_reopen_target(&record);
    let app_name = record
        .reopen_app_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            let trimmed = record.app_name.trim();
            (!trimmed.is_empty()).then_some(trimmed)
        });
    let needs_lookup = matches!(
        &target,
        Some(ResolvedReopenTarget::FilePath(path)) if file_needs_moved_lookup(path)
    );
    let moved_candidates =
        if let (true, Some(ResolvedReopenTarget::FilePath(path))) = (needs_lookup, &target) {
            find_moved_file_candidates(path).await
        } else {
            Vec::new()
        };

    let plan = plan_reopen(
        target,
        app_name,
        |_| moved_candidates.clone(),
        app_is_installed,
    );
    if let Some(action) = plan.action {
        open_reopen_target(action)?;
    }
    Ok(plan.outcome)
}

#[derive(Debug, Clone, PartialEq)]
enum ResolvedReopenTarget {
    BrowserUrl(String),
    FilePath(PathBuf),
    AppBundle(String),
    AppDeepLink(String),
}

#[derive(Debug, Clone, PartialEq)]
struct ReopenPlan {
    action: Option<ResolvedReopenTarget>,
    outcome: ReopenOutcome,
}

const REOPEN_MDFIND_TIMEOUT: Duration = Duration::from_secs(2);
const REOPEN_MDFIND_BYTES: usize = 64 * 1024;
const REOPEN_MDFIND_MAX_CANDIDATES: usize = 64;

fn file_needs_moved_lookup(path: &Path) -> bool {
    path.is_absolute() && !path.exists() && !volume_is_disconnected(path)
}

fn plan_reopen(
    target: Option<ResolvedReopenTarget>,
    app_name: Option<&str>,
    mut find_moved: impl FnMut(&Path) -> Vec<PathBuf>,
    mut app_installed: impl FnMut(&str) -> bool,
) -> ReopenPlan {
    let app_name = app_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    match target {
        None => ReopenPlan {
            action: None,
            outcome: ReopenOutcome::NoTarget,
        },
        Some(ResolvedReopenTarget::BrowserUrl(url)) => {
            if is_blocked_scheme(&url) || !is_http_url(&url) {
                ReopenPlan {
                    action: None,
                    outcome: ReopenOutcome::Blocked { target: url },
                }
            } else {
                ReopenPlan {
                    action: Some(ResolvedReopenTarget::BrowserUrl(url)),
                    outcome: ReopenOutcome::Opened,
                }
            }
        }
        Some(ResolvedReopenTarget::AppDeepLink(link)) => {
            if is_blocked_scheme(&link) {
                ReopenPlan {
                    action: None,
                    outcome: ReopenOutcome::Blocked { target: link },
                }
            } else {
                ReopenPlan {
                    action: Some(ResolvedReopenTarget::AppDeepLink(link)),
                    outcome: ReopenOutcome::Opened,
                }
            }
        }
        Some(ResolvedReopenTarget::AppBundle(bundle_id)) => {
            if app_installed(&bundle_id) {
                ReopenPlan {
                    action: Some(ResolvedReopenTarget::AppBundle(bundle_id)),
                    outcome: ReopenOutcome::AppOnly { app_name },
                }
            } else {
                ReopenPlan {
                    action: None,
                    outcome: ReopenOutcome::AppMissing {
                        bundle_id,
                        app_name,
                    },
                }
            }
        }
        Some(ResolvedReopenTarget::FilePath(path)) => plan_file_reopen(path, &mut find_moved),
    }
}

fn plan_file_reopen(
    path: PathBuf,
    find_moved: &mut impl FnMut(&Path) -> Vec<PathBuf>,
) -> ReopenPlan {
    let path_string = path.display().to_string();
    if !path.is_absolute() {
        return ReopenPlan {
            action: None,
            outcome: ReopenOutcome::Missing { path: path_string },
        };
    }
    if path.exists() {
        return ReopenPlan {
            action: Some(ResolvedReopenTarget::FilePath(path)),
            outcome: ReopenOutcome::Opened,
        };
    }
    if let Some(root) = volume_root(&path) {
        if !root.exists() {
            let volume = root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("external drive")
                .to_string();
            return ReopenPlan {
                action: None,
                outcome: ReopenOutcome::DriveNotConnected {
                    volume,
                    path: path_string,
                },
            };
        }
    }
    let candidates = find_moved(&path);
    if let Some(new_path) = pick_moved_file(&path, &candidates) {
        return ReopenPlan {
            action: Some(ResolvedReopenTarget::FilePath(new_path.clone())),
            outcome: ReopenOutcome::OpenedMoved {
                new_path: new_path.display().to_string(),
            },
        };
    }
    ReopenPlan {
        action: None,
        outcome: ReopenOutcome::Missing { path: path_string },
    }
}

async fn find_moved_file_candidates(original: &Path) -> Vec<PathBuf> {
    let Some(name) = original.file_name().and_then(|name| name.to_str()) else {
        return Vec::new();
    };
    let output = tokio::time::timeout(
        REOPEN_MDFIND_TIMEOUT,
        crate::spotlight::run_mdfind_name(name, None, REOPEN_MDFIND_BYTES),
    )
    .await;
    let Ok(Ok(bytes)) = output else {
        return Vec::new();
    };
    crate::spotlight::parse_mdfind_nul_paths(&bytes)
        .take(REOPEN_MDFIND_MAX_CANDIDATES)
        .collect()
}

#[cfg(target_os = "macos")]
fn app_is_installed(bundle_id: &str) -> bool {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;

    unsafe {
        NSWorkspace::sharedWorkspace()
            .URLForApplicationWithBundleIdentifier(&NSString::from_str(bundle_id))
            .is_some()
    }
}

#[cfg(not(target_os = "macos"))]
fn app_is_installed(_bundle_id: &str) -> bool {
    true
}

fn resolve_reopen_target(record: &crate::storage::MemoryRecord) -> Option<ResolvedReopenTarget> {
    let typed = match &record.reopen_kind {
        ReopenKind::BrowserUrl => record
            .reopen_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                let url = if is_http_url(value) {
                    match record.reopen_page {
                        Some(page) => url_with_pdf_page(value, page),
                        None => match record.reopen_text_anchor.as_deref() {
                            Some(anchor) => url_with_text_anchor(value, anchor),
                            None => value.to_string(),
                        },
                    }
                } else {
                    value.to_string()
                };
                ResolvedReopenTarget::BrowserUrl(url)
            }),
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
    let mut cmd = Command::new("open");
    if should_reveal_in_finder(path) {
        cmd.arg("-R");
    }
    cmd.arg(path)
        .spawn()
        .map_err(|err| format!("Failed to open path '{}': {}", path.display(), err))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_path_with_system(path: &Path) -> Result<(), String> {
    let target = if should_reveal_in_finder(path) {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    Command::new("cmd")
        .arg("/C")
        .arg("start")
        .arg("")
        .arg(target)
        .spawn()
        .map_err(|err| format!("Failed to open path '{}': {}", path.display(), err))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn open_path_with_system(path: &Path) -> Result<(), String> {
    let target = if should_reveal_in_finder(path) {
        path.parent().unwrap_or(path)
    } else {
        path
    };
    Command::new("xdg-open")
        .arg(target)
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

    fn deletable_record(id: &str) -> crate::storage::MemoryRecord {
        crate::storage::MemoryRecord {
            id: id.to_string(),
            timestamp: 1_700_000_000_000,
            app_name: "Chrome".to_string(),
            window_title: "Roadmap review".to_string(),
            session_id: "session-1".to_string(),
            day_bucket: "2026-09-22".to_string(),
            clean_text: "Reviewed the quarterly roadmap document with the team".to_string(),
            snippet: "Reviewed the quarterly roadmap document with the team".to_string(),
            embedding: vec![0.01; crate::embedding::EMBEDDING_DIM],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn delete_memory_logic_removes_the_memory_graph_node_but_keeps_the_shared_session_node() {
        // MEM-07 invariant 10: deleting a memory must not leave its own
        // graph node and edges behind. A session node it shares with other
        // memories is left alone, since deleting one memory should not sever
        // another memory's graph connectivity.
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).unwrap())
            .await
            .unwrap();
        let store = std::sync::Arc::new(store);
        let graph = GraphStore::new(store.clone());

        let record = deletable_record("mem-1");
        store
            .add_batch(&[record.clone()])
            .await
            .expect("add memory");
        graph
            .ingest_memory(&record)
            .await
            .expect("ingest into graph");

        let nodes_before = store.get_all_nodes().await.expect("nodes before");
        assert!(
            nodes_before.iter().any(|n| n.id == "memory:mem-1"),
            "expected a memory:mem-1 node before deletion, got {nodes_before:?}"
        );
        let session_node_id = nodes_before
            .iter()
            .find(|n| n.id != "memory:mem-1")
            .map(|n| n.id.clone())
            .expect("a session node should also have been created");

        let deleted = delete_memory_logic(&store, &graph, "mem-1")
            .await
            .expect("delete");
        assert!(deleted);

        assert!(store
            .get_memory_by_id("mem-1")
            .await
            .expect("query")
            .is_none());

        let nodes_after = store.get_all_nodes().await.expect("nodes after");
        assert!(
            !nodes_after.iter().any(|n| n.id == "memory:mem-1"),
            "memory:mem-1 node should be gone, got {nodes_after:?}"
        );
        assert!(
            nodes_after.iter().any(|n| n.id == session_node_id),
            "shared session node {session_node_id} should survive, got {nodes_after:?}"
        );

        let edges_after = store.get_all_edges().await.expect("edges after");
        assert!(
            !edges_after
                .iter()
                .any(|e| e.source == "memory:mem-1" || e.target == "memory:mem-1"),
            "no edge should still reference memory:mem-1, got {edges_after:?}"
        );
    }

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

    use ResolvedReopenTarget as R;
    type Rec = crate::storage::MemoryRecord;

    fn s(value: &str) -> Option<String> {
        Some(value.to_string())
    }

    fn assert_reopen_cases(cases: Vec<(&str, Rec, Option<ResolvedReopenTarget>)>) {
        for (label, record, expected) in cases {
            assert_eq!(resolve_reopen_target(&record), expected, "{label}");
        }
    }

    #[test]
    fn resolve_reopen_target_uses_each_typed_kind() {
        assert_reopen_cases(vec![
            (
                "browser url",
                Rec {
                    reopen_kind: ReopenKind::BrowserUrl,
                    reopen_url: s(" https://example.com/a "),
                    ..Default::default()
                },
                Some(R::BrowserUrl("https://example.com/a".into())),
            ),
            (
                "file path",
                Rec {
                    reopen_kind: ReopenKind::FilePath,
                    reopen_file_path: s("/Users/qa/doc.pdf"),
                    ..Default::default()
                },
                Some(R::FilePath(PathBuf::from("/Users/qa/doc.pdf"))),
            ),
            (
                "app bundle",
                Rec {
                    reopen_kind: ReopenKind::AppBundle,
                    reopen_app_bundle_id: s("com.apple.Preview"),
                    ..Default::default()
                },
                Some(R::AppBundle("com.apple.Preview".into())),
            ),
            (
                "deep link",
                Rec {
                    reopen_kind: ReopenKind::AppDeepLink,
                    reopen_app_deep_link: s("notion://www.notion.so/page-123"),
                    ..Default::default()
                },
                Some(R::AppDeepLink("notion://www.notion.so/page-123".into())),
            ),
        ]);
    }

    #[test]
    fn resolve_reopen_target_without_any_target_is_none() {
        assert_reopen_cases(vec![
            ("default record", Rec::default(), None),
            (
                "unknown kind with typed fields only",
                Rec {
                    reopen_kind: ReopenKind::Unknown,
                    reopen_url: s("https://ignored.example"),
                    reopen_app_bundle_id: s("com.ignored"),
                    ..Default::default()
                },
                None,
            ),
            (
                "blank fallbacks",
                Rec {
                    url: s("  "),
                    bundle_id: s(""),
                    files_touched: vec!["  ".into()],
                    memory_context: "Reopen:   ".into(),
                    ..Default::default()
                },
                None,
            ),
        ]);
    }

    #[test]
    fn resolve_reopen_target_typed_kind_missing_field_falls_back() {
        assert_reopen_cases(vec![
            (
                "browser url without reopen_url uses record url",
                Rec {
                    reopen_kind: ReopenKind::BrowserUrl,
                    url: s("https://fallback.example"),
                    ..Default::default()
                },
                Some(R::BrowserUrl("https://fallback.example".into())),
            ),
            (
                "blank file path uses bundle id",
                Rec {
                    reopen_kind: ReopenKind::FilePath,
                    reopen_file_path: s("   "),
                    bundle_id: s("com.apple.Preview"),
                    ..Default::default()
                },
                Some(R::AppBundle("com.apple.Preview".into())),
            ),
            (
                "app bundle without id uses files touched",
                Rec {
                    reopen_kind: ReopenKind::AppBundle,
                    files_touched: vec!["/Users/qa/doc.pdf".into()],
                    ..Default::default()
                },
                Some(R::FilePath(PathBuf::from("/Users/qa/doc.pdf"))),
            ),
            (
                "deep link without link and no fallbacks",
                Rec {
                    reopen_kind: ReopenKind::AppDeepLink,
                    ..Default::default()
                },
                None,
            ),
        ]);
    }

    #[test]
    fn resolve_reopen_target_rejects_invalid_typed_values() {
        assert_reopen_cases(vec![
            (
                "javascript browser url is kept for plan_reopen to block",
                Rec {
                    reopen_kind: ReopenKind::BrowserUrl,
                    reopen_url: s("javascript:alert(1)"),
                    ..Default::default()
                },
                Some(R::BrowserUrl("javascript:alert(1)".into())),
            ),
            (
                "data browser url is kept instead of falling back to the app",
                Rec {
                    reopen_kind: ReopenKind::BrowserUrl,
                    reopen_url: s("data:text/html,hi"),
                    bundle_id: s("com.google.Chrome"),
                    ..Default::default()
                },
                Some(R::BrowserUrl("data:text/html,hi".into())),
            ),
            (
                "https is not a deep link",
                Rec {
                    reopen_kind: ReopenKind::AppDeepLink,
                    reopen_app_deep_link: s("https://example.com"),
                    ..Default::default()
                },
                None,
            ),
            (
                "deep link without scheme separator",
                Rec {
                    reopen_kind: ReopenKind::AppDeepLink,
                    reopen_app_deep_link: s("mailto:qa@example.com"),
                    ..Default::default()
                },
                None,
            ),
        ]);
    }

    #[test]
    fn resolve_reopen_target_fallback_order() {
        let full = Rec {
            memory_context: "Summary\nReopen: https://legacy.example\n".into(),
            url: s("https://url.example"),
            files_touched: vec!["".into(), "/Users/qa/doc.pdf".into()],
            bundle_id: s("com.apple.Preview"),
            ..Default::default()
        };
        let no_marker = Rec {
            memory_context: String::new(),
            ..full.clone()
        };
        let no_url = Rec {
            url: None,
            ..no_marker.clone()
        };
        let no_files = Rec {
            files_touched: Vec::new(),
            ..no_url.clone()
        };
        let non_http_url = Rec {
            url: s("chrome://settings"),
            ..no_url.clone()
        };
        assert_reopen_cases(vec![
            (
                "legacy marker first",
                full,
                Some(R::BrowserUrl("https://legacy.example".into())),
            ),
            (
                "then record url",
                no_marker,
                Some(R::BrowserUrl("https://url.example".into())),
            ),
            (
                "then first non-empty file touched",
                no_url,
                Some(R::FilePath(PathBuf::from("/Users/qa/doc.pdf"))),
            ),
            (
                "non-http record url is skipped",
                non_http_url,
                Some(R::FilePath(PathBuf::from("/Users/qa/doc.pdf"))),
            ),
            (
                "then bundle id",
                no_files,
                Some(R::AppBundle("com.apple.Preview".into())),
            ),
        ]);
    }

    #[test]
    fn resolve_reopen_target_legacy_marker_variants() {
        let marker = |context: &str| Rec {
            memory_context: context.into(),
            ..Default::default()
        };
        assert_reopen_cases(vec![
            (
                "file url",
                marker("Reopen: file:///Users/qa/doc.pdf"),
                Some(R::FilePath(PathBuf::from("/Users/qa/doc.pdf"))),
            ),
            (
                "deep link",
                marker("Reopen: notion://www.notion.so/page-123"),
                Some(R::AppDeepLink("notion://www.notion.so/page-123".into())),
            ),
            ("empty marker", marker("Reopen: "), None),
            (
                "javascript marker",
                marker("Reopen: javascript:alert(1)"),
                None,
            ),
            (
                "indented marker after other lines",
                marker("App: Chrome\n   Reopen: https://legacy.example  "),
                Some(R::BrowserUrl("https://legacy.example".into())),
            ),
        ]);
    }

    // `chrome:` pages must never be opened (R14). resolve still types them so
    // plan_reopen can return Blocked instead of falling back to the browser app.
    #[test]
    fn resolve_reopen_target_keeps_chrome_scheme_for_blocked_plan_r14() {
        assert_reopen_cases(vec![
            (
                "typed deep link",
                Rec {
                    reopen_kind: ReopenKind::AppDeepLink,
                    reopen_app_deep_link: s("chrome://settings"),
                    ..Default::default()
                },
                Some(R::AppDeepLink("chrome://settings".into())),
            ),
            (
                "legacy marker",
                Rec {
                    memory_context: "Reopen: chrome://settings".into(),
                    ..Default::default()
                },
                Some(R::AppDeepLink("chrome://settings".into())),
            ),
        ]);
    }

    // The stored kind is authoritative; merges keep kind and fields consistent.
    #[test]
    fn resolve_reopen_target_follows_stored_kind() {
        let record = Rec {
            reopen_kind: ReopenKind::AppBundle,
            reopen_app_bundle_id: s("com.google.Chrome"),
            reopen_file_path: s("/Users/qa/doc.pdf"),
            ..Default::default()
        };
        assert_eq!(
            resolve_reopen_target(&record),
            Some(R::AppBundle("com.google.Chrome".into()))
        );
    }

    // A `file://` marker should be percent-decoded to the path on disk.
    #[test]
    fn resolve_reopen_target_keeps_percent_encoding_in_file_url_flips_r25() {
        let record = Rec {
            memory_context: "Reopen: file:///Users/qa/My%20Doc%20caf%C3%A9.pdf".into(),
            ..Default::default()
        };
        assert_eq!(
            resolve_reopen_target(&record),
            Some(R::FilePath(PathBuf::from(
                "/Users/qa/My%20Doc%20caf%C3%A9.pdf"
            )))
        );
    }

    // A relative path is not a file target; this should be app only (R18) or the folder (R20).
    #[test]
    fn resolve_reopen_target_accepts_relative_file_path_flips_r18() {
        assert_reopen_cases(vec![
            (
                "typed relative path",
                Rec {
                    reopen_kind: ReopenKind::FilePath,
                    reopen_file_path: s("plan.md"),
                    bundle_id: s("com.apple.TextEdit"),
                    ..Default::default()
                },
                Some(R::FilePath(PathBuf::from("plan.md"))),
            ),
            (
                "relative files touched over bundle",
                Rec {
                    files_touched: vec!["plan.md".into()],
                    bundle_id: s("com.apple.finder"),
                    ..Default::default()
                },
                Some(R::FilePath(PathBuf::from("plan.md"))),
            ),
        ]);
    }

    #[test]
    fn resolve_reopen_target_appends_pdf_page_to_browser_url() {
        assert_reopen_cases(vec![
            (
                "page on pdf url",
                Rec {
                    reopen_kind: ReopenKind::BrowserUrl,
                    reopen_url: s("https://example.com/doc.pdf"),
                    reopen_page: Some(112),
                    ..Default::default()
                },
                Some(R::BrowserUrl("https://example.com/doc.pdf#page=112".into())),
            ),
            (
                "replaces existing page fragment",
                Rec {
                    reopen_kind: ReopenKind::BrowserUrl,
                    reopen_url: s("https://example.com/doc.pdf#page=1"),
                    reopen_page: Some(12),
                    ..Default::default()
                },
                Some(R::BrowserUrl("https://example.com/doc.pdf#page=12".into())),
            ),
            (
                "no page leaves url unchanged",
                Rec {
                    reopen_kind: ReopenKind::BrowserUrl,
                    reopen_url: s("https://example.com/doc.pdf"),
                    ..Default::default()
                },
                Some(R::BrowserUrl("https://example.com/doc.pdf".into())),
            ),
            (
                "file path ignores stored page on open",
                Rec {
                    reopen_kind: ReopenKind::FilePath,
                    reopen_file_path: s("/Users/qa/doc.pdf"),
                    reopen_page: Some(112),
                    ..Default::default()
                },
                Some(R::FilePath(PathBuf::from("/Users/qa/doc.pdf"))),
            ),
        ]);
    }

    #[test]
    fn resolve_reopen_target_appends_text_anchor_unless_page_or_fragment() {
        let anchor = "Nitrogen is a chemical element with the symbol";
        assert_reopen_cases(vec![
            (
                "anchor on article url",
                Rec {
                    reopen_kind: ReopenKind::BrowserUrl,
                    reopen_url: s("https://example.com/article"),
                    reopen_text_anchor: s(anchor),
                    ..Default::default()
                },
                Some(R::BrowserUrl(format!(
                    "https://example.com/article#:~:text={}",
                    anchor.replace(' ', "%20")
                ))),
            ),
            (
                "page wins over anchor",
                Rec {
                    reopen_kind: ReopenKind::BrowserUrl,
                    reopen_url: s("https://example.com/doc.pdf"),
                    reopen_page: Some(12),
                    reopen_text_anchor: s(anchor),
                    ..Default::default()
                },
                Some(R::BrowserUrl("https://example.com/doc.pdf#page=12".into())),
            ),
            (
                "existing fragment is left unchanged",
                Rec {
                    reopen_kind: ReopenKind::BrowserUrl,
                    reopen_url: s("https://example.com/article#section"),
                    reopen_text_anchor: s(anchor),
                    ..Default::default()
                },
                Some(R::BrowserUrl("https://example.com/article#section".into())),
            ),
        ]);
    }

    fn plan(
        target: Option<ResolvedReopenTarget>,
        app_name: Option<&str>,
        find_moved: impl FnMut(&Path) -> Vec<PathBuf>,
        app_installed: impl FnMut(&str) -> bool,
    ) -> ReopenPlan {
        plan_reopen(target, app_name, find_moved, app_installed)
    }

    #[test]
    fn plan_reopen_existing_pkg_stays_opened_and_is_reveal_only_r35() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("Setup.pkg");
        std::fs::write(&path, b"pkg").unwrap();
        assert!(should_reveal_in_finder(&path));
        let result = plan(Some(R::FilePath(path.clone())), None, |_| vec![], |_| true);
        assert_eq!(result.outcome, ReopenOutcome::Opened);
        assert_eq!(result.action, Some(R::FilePath(path)));
    }

    #[test]
    fn plan_reopen_opens_download_file_even_when_source_url_is_set() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("report.pdf");
        std::fs::write(&path, b"%PDF").unwrap();
        let path_string = path.display().to_string();
        let record = Rec {
            reopen_kind: ReopenKind::FilePath,
            reopen_file_path: s(&path_string),
            url: s("https://example.com/article"),
            bundle_id: s("com.apple.finder"),
            app_name: "Finder".into(),
            summary_source: "tracker".into(),
            ..Default::default()
        };
        let result = plan(
            resolve_reopen_target(&record),
            Some("Finder"),
            |_| vec![],
            |_| true,
        );
        assert_eq!(result.outcome, ReopenOutcome::Opened);
        assert_eq!(result.action, Some(R::FilePath(path)));
    }

    #[test]
    fn plan_reopen_opens_an_existing_temp_file_without_lookup() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("doc.pdf");
        std::fs::write(&path, b"hi").unwrap();
        let mut lookups = 0;
        let result = plan(
            Some(R::FilePath(path.clone())),
            None,
            |_| {
                lookups += 1;
                vec![]
            },
            |_| true,
        );
        assert_eq!(lookups, 0);
        assert_eq!(result.outcome, ReopenOutcome::Opened);
        assert_eq!(result.action, Some(R::FilePath(path)));
    }

    #[test]
    fn plan_reopen_picks_the_moved_file_and_skips_trash_and_similar_names() {
        let dir = tempfile::tempdir().expect("tempdir");
        let original = dir.path().join("original").join("gone.pdf");
        let found = dir.path().join("original").join("moved").join("gone.pdf");
        let farther = dir.path().join("elsewhere").join("gone.pdf");
        let trash = dir.path().join(".Trash").join("gone.pdf");
        let similar = dir.path().join("gone (1).pdf");
        std::fs::create_dir_all(original.parent().unwrap()).unwrap();
        std::fs::create_dir_all(found.parent().unwrap()).unwrap();
        std::fs::create_dir_all(farther.parent().unwrap()).unwrap();
        std::fs::create_dir_all(trash.parent().unwrap()).unwrap();
        std::fs::write(&found, b"found").unwrap();
        std::fs::write(&farther, b"far").unwrap();
        std::fs::write(&trash, b"trash").unwrap();
        std::fs::write(&similar, b"similar").unwrap();
        let mut seen = None;
        let result = plan(
            Some(R::FilePath(original.clone())),
            None,
            |path| {
                seen = Some(path.to_path_buf());
                vec![
                    trash.clone(),
                    similar.clone(),
                    farther.clone(),
                    found.clone(),
                ]
            },
            |_| true,
        );
        assert_eq!(seen.as_ref(), Some(&original));
        assert_eq!(
            result.outcome,
            ReopenOutcome::OpenedMoved {
                new_path: found.display().to_string(),
            }
        );
        assert_eq!(result.action, Some(R::FilePath(found)));
    }

    #[test]
    fn plan_reopen_missing_file_with_no_candidates() {
        let dir = tempfile::tempdir().expect("tempdir");
        let original = dir.path().join("gone.pdf");
        let result = plan(
            Some(R::FilePath(original.clone())),
            None,
            |_| vec![],
            |_| true,
        );
        assert_eq!(
            result.outcome,
            ReopenOutcome::Missing {
                path: original.display().to_string(),
            }
        );
        assert_eq!(result.action, None);
    }

    #[test]
    fn plan_reopen_relative_path_is_missing_and_skips_lookup() {
        let mut lookups = 0;
        let result = plan(
            Some(R::FilePath(PathBuf::from("plan.md"))),
            None,
            |_| {
                lookups += 1;
                vec![]
            },
            |_| true,
        );
        assert_eq!(lookups, 0);
        assert_eq!(
            result.outcome,
            ReopenOutcome::Missing {
                path: "plan.md".into(),
            }
        );
        assert_eq!(result.action, None);
    }

    #[test]
    fn plan_reopen_disconnected_volume_skips_lookup() {
        let path = PathBuf::from("/Volumes/FndrRe07MissingVol/doc.pdf");
        assert!(
            !Path::new("/Volumes/FndrRe07MissingVol").exists(),
            "test volume must be absent"
        );
        let mut lookups = 0;
        let result = plan(
            Some(R::FilePath(path.clone())),
            None,
            |_| {
                lookups += 1;
                vec![]
            },
            |_| true,
        );
        assert_eq!(lookups, 0);
        assert_eq!(
            result.outcome,
            ReopenOutcome::DriveNotConnected {
                volume: "FndrRe07MissingVol".into(),
                path: path.display().to_string(),
            }
        );
        assert_eq!(result.action, None);
    }

    #[test]
    fn plan_reopen_app_installed_is_app_only_missing_is_app_missing() {
        let installed = plan(
            Some(R::AppBundle("com.apple.Preview".into())),
            Some("Preview"),
            |_| vec![],
            |_| true,
        );
        assert_eq!(
            installed.outcome,
            ReopenOutcome::AppOnly {
                app_name: Some("Preview".into()),
            }
        );
        assert_eq!(
            installed.action,
            Some(R::AppBundle("com.apple.Preview".into()))
        );

        let missing = plan(
            Some(R::AppBundle("com.example.Gone".into())),
            Some("Gone"),
            |_| vec![],
            |_| false,
        );
        assert_eq!(
            missing.outcome,
            ReopenOutcome::AppMissing {
                bundle_id: "com.example.Gone".into(),
                app_name: Some("Gone".into()),
            }
        );
        assert_eq!(missing.action, None);
    }

    #[test]
    fn plan_reopen_blocks_chrome_and_javascript_r14() {
        for target in [
            R::AppDeepLink("chrome://settings".into()),
            R::BrowserUrl("javascript:alert(1)".into()),
        ] {
            let result = plan(Some(target.clone()), None, |_| vec![], |_| true);
            let expected = match target {
                R::AppDeepLink(value) | R::BrowserUrl(value) => {
                    ReopenOutcome::Blocked { target: value }
                }
                other => panic!("unexpected {other:?}"),
            };
            assert_eq!(result.outcome, expected);
            assert_eq!(result.action, None);
        }
    }

    #[test]
    fn plan_reopen_empty_record_is_no_target() {
        let result = plan(None, None, |_| vec![], |_| true);
        assert_eq!(result.outcome, ReopenOutcome::NoTarget);
        assert_eq!(result.action, None);
    }
}
