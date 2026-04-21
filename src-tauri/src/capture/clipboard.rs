//! Clipboard watcher — monitors macOS pasteboard for changes and indexes
//! copies into the FNDR knowledge graph.
//!
//! Each clipboard event creates a `Clipboard` node with the copied text,
//! linked to the most recent `MemoryChunk` via a `ClipboardCopied` edge.

use crate::capture::text_cleanup;
use crate::store::{EdgeType, GraphEdge, GraphNode, NodeType, Store};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;

/// How often we poll NSPasteboard for changes (milliseconds).
const POLL_INTERVAL_MS: u64 = 1500;

/// Maximum clipboard text length we'll index. Ignore huge pastes (binary/images).
const MAX_CLIP_TEXT_LEN: usize = 4000;

/// Minimum clipboard text length worth indexing.
const MIN_CLIP_TEXT_LEN: usize = 4;

/// Start the clipboard watcher as a background loop.
///
/// Polls `NSPasteboard.generalPasteboard` for `changeCount` changes, extracts
/// the string contents, and writes a `Clipboard` graph node linked to the
/// most recent memory chunk via a `ClipboardCopied` edge.
pub async fn run_clipboard_watcher(store: Arc<Store>) {
    tracing::info!("Clipboard watcher started");

    let mut last_change_count: isize = current_change_count();
    let mut last_clip_fingerprint: u64 = 0;
    let mut interval = tokio::time::interval(Duration::from_millis(POLL_INTERVAL_MS));

    loop {
        interval.tick().await;

        let change_count = current_change_count();
        if change_count == last_change_count {
            continue;
        }
        last_change_count = change_count;

        // Read the current clipboard string
        let text = match read_pasteboard_string() {
            Some(text) if text.len() >= MIN_CLIP_TEXT_LEN && text.len() <= MAX_CLIP_TEXT_LEN => {
                text
            }
            _ => continue,
        };

        let trimmed = text.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_low_signal_clip(trimmed) {
            continue;
        }

        let fingerprint = clipboard_fingerprint(trimmed);
        if fingerprint == last_clip_fingerprint {
            continue;
        }
        last_clip_fingerprint = fingerprint;

        tracing::info!("Clipboard change detected: {} chars", trimmed.len());

        // Create a graph node for this clipboard event
        let now = chrono::Utc::now().timestamp_millis();
        let clip_id = format!("clipboard:{}", uuid::Uuid::new_v4());
        let clip_label: String = trimmed.chars().take(120).collect();

        let clip_node = GraphNode {
            id: clip_id.clone(),
            node_type: NodeType::Clipboard,
            label: clip_label,
            created_at: now,
            metadata: json!({
                "full_text": trimmed.chars().take(2000).collect::<String>(),
                "length": trimmed.len(),
            }),
        };

        if let Err(e) = store.upsert_nodes(&[clip_node]).await {
            tracing::warn!("Failed to store clipboard node: {}", e);
            continue;
        }

        // Link clipboard to the most recent memory chunk (if any exists in the last 60s)
        let recent_memories = store
            .get_memories_in_range(now - 60_000, now)
            .await
            .unwrap_or_default();

        if let Some(recent) = recent_memories.last() {
            let edge = GraphEdge {
                id: uuid::Uuid::new_v4().to_string(),
                source: format!("memory:{}", recent.id),
                target: clip_id,
                edge_type: EdgeType::ClipboardCopied,
                timestamp: now,
                metadata: json!({
                    "app_name": recent.app_name,
                    "window_title": recent.window_title,
                }),
            };
            if let Err(e) = store.upsert_edges(&[edge]).await {
                tracing::warn!("Failed to store clipboard edge: {}", e);
            }
        }
    }
}

fn clipboard_fingerprint(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.trim().to_lowercase().hash(&mut hasher);
    hasher.finish()
}

fn is_low_signal_clip(text: &str) -> bool {
    if text.len() < MIN_CLIP_TEXT_LEN {
        return true;
    }
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return true;
    }
    if normalized.len() <= 2 {
        return true;
    }
    if normalized.chars().all(|ch| ch.is_ascii_punctuation()) {
        return true;
    }
    if text_cleanup::symbol_ratio(&normalized) > 0.70 {
        return true;
    }
    if normalized.len() > 24
        && normalized
            .chars()
            .collect::<std::collections::HashSet<_>>()
            .len()
            <= 3
    {
        return true;
    }

    false
}

// ── macOS NSPasteboard via objc2 ────────────────────────────────────────────

#[cfg(target_os = "macos")]
fn current_change_count() -> isize {
    use objc2_app_kit::NSPasteboard;

    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        pb.changeCount()
    }
}

#[cfg(target_os = "macos")]
fn read_pasteboard_string() -> Option<String> {
    use objc2_app_kit::NSPasteboard;
    use objc2_foundation::NSString;

    unsafe {
        let pb = NSPasteboard::generalPasteboard();
        let ns_string = NSString::from_str("public.utf8-plain-text");
        let result = pb.stringForType(&ns_string)?;
        Some(result.to_string())
    }
}

#[cfg(not(target_os = "macos"))]
fn current_change_count() -> isize {
    0
}

#[cfg(not(target_os = "macos"))]
fn read_pasteboard_string() -> Option<String> {
    None
}
