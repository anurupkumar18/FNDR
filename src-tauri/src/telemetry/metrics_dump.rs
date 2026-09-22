//! Optional NDJSON dump of runtime metrics for baseline and soak reports.
//!
//! Enabled only when `FNDR_METRICS_DUMP` names a file. One line per minute. Contains counters and
//! timings only, never captured content.

use crate::AppState;
use serde_json::json;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

pub fn spawn_if_enabled(state: Arc<AppState>) {
    let Some(path) = std::env::var_os("FNDR_METRICS_DUMP") else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let snapshot = crate::ipc::commands::current_runtime_snapshot(&state);
            let stats = &state.capture_stats;
            let line = json!({
                "snapshot": snapshot,
                "skips": stats.skip_counts(),
                "totals": {
                    "evaluated": stats.evaluated_total(),
                    "stored": stats.total_stored(),
                    "skipped": stats.total_skipped(),
                },
            });
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = writeln!(file, "{line}");
            }
        }
    });
}
