//! RE-11: one resumable pass that persists a more specific reopen target for
//! memories stored before capture recorded one. Runs only while the idle gate
//! allows, never pauses capture, and stops for good once the pass completes
//! (new captures already store their target).

use crate::memory::reopen::upgraded_reopen_target;
use crate::storage::Store;
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Bump to run the pass again over every memory after the upgrade rules change.
const CHECKPOINT_VERSION: u32 = 1;
const BATCH_SIZE: usize = 16;
const GATE_RETRY: Duration = Duration::from_secs(60);
const BATCH_PAUSE: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReopenBackfillCheckpoint {
    pub version: u32,
    pub last_id: Option<String>,
    pub upgraded: usize,
    pub complete: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReopenBackfillProgress {
    pub is_running: bool,
    pub phase: String,
    pub processed: usize,
    pub total: usize,
    pub upgraded: usize,
    pub timestamp_ms: i64,
}

pub fn checkpoint_path(data_dir: &Path) -> PathBuf {
    data_dir.join("reopen_backfill_checkpoint.json")
}

pub fn progress_path(data_dir: &Path) -> PathBuf {
    data_dir.join("reopen_backfill_progress.json")
}

/// The next `limit` ids after `after`, from ids sorted ascending.
fn next_batch<'a>(sorted_ids: &'a [String], after: Option<&str>, limit: usize) -> &'a [String] {
    let start = match after {
        Some(after) => sorted_ids.partition_point(|id| id.as_str() <= after),
        None => 0,
    };
    let end = (start + limit).min(sorted_ids.len());
    &sorted_ids[start..end]
}

fn load_checkpoint(path: &Path) -> ReopenBackfillCheckpoint {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<ReopenBackfillCheckpoint>(&content).ok())
        .filter(|checkpoint| checkpoint.version == CHECKPOINT_VERSION)
        .unwrap_or(ReopenBackfillCheckpoint {
            version: CHECKPOINT_VERSION,
            ..Default::default()
        })
}

fn write_json_atomically<T: Serialize>(path: &Path, value: &T) {
    let tmp = path.with_extension("json.tmp");
    if let Ok(serialized) = serde_json::to_string_pretty(value) {
        if std::fs::write(&tmp, serialized).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

/// Runs the pass until it completes, resuming from the checkpoint. Waits
/// `gate_retry` whenever `allowed` says no, and `batch_pause` between batches.
pub async fn run_reopen_backfill(
    store: &Store,
    data_dir: &Path,
    allowed: impl Fn() -> bool,
    gate_retry: Duration,
    batch_pause: Duration,
) -> Result<ReopenBackfillProgress, String> {
    let checkpoint_path = checkpoint_path(data_dir);
    let progress_path = progress_path(data_dir);
    let mut checkpoint = load_checkpoint(&checkpoint_path);
    if checkpoint.complete {
        return Ok(ReopenBackfillProgress {
            phase: "complete".into(),
            upgraded: checkpoint.upgraded,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            ..Default::default()
        });
    }

    let mut ids = store
        .list_all_memories()
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|record| record.id)
        .filter(|id| !id.trim().is_empty())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();

    let processed_before = |checkpoint: &ReopenBackfillCheckpoint| match &checkpoint.last_id {
        Some(last) => ids.partition_point(|id| id <= last),
        None => 0,
    };
    let mut progress = ReopenBackfillProgress {
        is_running: true,
        phase: if checkpoint.last_id.is_some() {
            "resuming".into()
        } else {
            "scanning".into()
        },
        processed: processed_before(&checkpoint),
        total: ids.len(),
        upgraded: checkpoint.upgraded,
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    };
    write_json_atomically(&progress_path, &progress);

    loop {
        let batch = next_batch(&ids, checkpoint.last_id.as_deref(), BATCH_SIZE);
        let Some(last_in_batch) = batch.last() else {
            break;
        };
        while !allowed() {
            tokio::time::sleep(gate_retry).await;
        }
        // Re-read so a row changed since the scan (review, merge) is not
        // overwritten with its older copy.
        let current = store
            .get_memories_by_ids(batch)
            .await
            .map_err(|e| e.to_string())?;
        let upgraded = batch
            .iter()
            .filter_map(|id| current.get(id))
            .filter_map(|record| {
                let target = upgraded_reopen_target(record)?;
                let mut record = record.clone();
                record.apply_reopen_target(target);
                Some(record)
            })
            .collect::<Vec<_>>();
        store
            .replace_memories_preserving_chunks(&upgraded)
            .await
            .map_err(|e| e.to_string())?;

        checkpoint.last_id = Some(last_in_batch.clone());
        checkpoint.upgraded += upgraded.len();
        write_json_atomically(&checkpoint_path, &checkpoint);
        progress.phase = "running".into();
        progress.processed = processed_before(&checkpoint);
        progress.upgraded = checkpoint.upgraded;
        progress.timestamp_ms = chrono::Utc::now().timestamp_millis();
        write_json_atomically(&progress_path, &progress);
        tokio::time::sleep(batch_pause).await;
    }

    checkpoint.complete = true;
    write_json_atomically(&checkpoint_path, &checkpoint);
    progress.is_running = false;
    progress.phase = "complete".into();
    progress.timestamp_ms = chrono::Utc::now().timestamp_millis();
    write_json_atomically(&progress_path, &progress);
    tracing::info!(
        processed = progress.processed,
        upgraded = progress.upgraded,
        "reopen_backfill:complete"
    );
    Ok(progress)
}

/// Starts the pass in the background after `first_delay`.
pub fn spawn(state: Arc<AppState>, first_delay: Duration) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(first_delay).await;
        let data_dir = state.store.data_dir();
        let gate_state = state.clone();
        let allowed = move || crate::system_resources::allows_graph_idle_commit(&gate_state);
        if let Err(error) =
            run_reopen_backfill(&state.store, &data_dir, allowed, GATE_RETRY, BATCH_PAUSE).await
        {
            tracing::warn!(%error, "reopen_backfill:failed");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn next_batch_resumes_after_the_checkpoint_id() {
        let all = ids(&["a", "b", "c", "d", "e"]);
        assert_eq!(next_batch(&all, None, 2), ids(&["a", "b"]).as_slice());
        assert_eq!(next_batch(&all, Some("b"), 2), ids(&["c", "d"]).as_slice());
        assert_eq!(next_batch(&all, Some("d"), 2), ids(&["e"]).as_slice());
        assert!(next_batch(&all, Some("e"), 2).is_empty());
    }

    #[test]
    fn next_batch_skips_past_a_checkpoint_id_that_was_deleted() {
        let all = ids(&["a", "c", "e"]);
        assert_eq!(next_batch(&all, Some("b"), 5), ids(&["c", "e"]).as_slice());
    }

    #[test]
    fn checkpoint_from_another_version_starts_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = checkpoint_path(dir.path());
        write_json_atomically(
            &path,
            &ReopenBackfillCheckpoint {
                version: CHECKPOINT_VERSION + 1,
                last_id: Some("m9".into()),
                upgraded: 4,
                complete: true,
            },
        );
        let loaded = load_checkpoint(&path);
        assert_eq!(loaded.version, CHECKPOINT_VERSION);
        assert_eq!(loaded.last_id, None);
        assert!(!loaded.complete);
    }
}
