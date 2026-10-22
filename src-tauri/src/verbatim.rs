//! Keep the captured text beside a memory (ADR 036).
//!
//! `compact_memory_record_payload` drops a screen memory's text when the row
//! is stored (`memory_compaction.rs`): what survives is a summary and
//! embeddings. This module saves the cleaned text the capture saw into
//! `memory_verbatim_v1`, so a consumer that needs an exact value (a phone
//! number on a page, a line to quote) can ask for it by memory id.
//!
//! The text never changes how a memory is embedded or ranked: the table has no
//! vector and no full-text index, and search does not read it.
//!
//! What gates it, in order: the switch, Private Mode, the memory's kind (screen
//! and browser captures only), the blocklist, and the same deterministic
//! `safety_gate` the capture ran, evaluated again on the exact string stored.

use crate::config::{Config, VerbatimConfig};
use crate::privacy::safety_gate::{self, SafetyDecision};
use crate::privacy::Blocklist;
use crate::storage::{MemoryRecord, Store, VerbatimEntry};
use crate::AppState;

/// Why a memory's text was not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    SwitchOff,
    PrivateMode,
    NotAScreenCapture,
    Blocked,
    Sensitive,
    Empty,
}

/// `text` trimmed and cut to at most `max_bytes` on a character boundary.
pub fn cap_text(text: &str, max_bytes: usize) -> String {
    let trimmed = text.trim();
    if trimmed.len() <= max_bytes {
        return trimmed.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    trimmed[..end].trim_end().to_string()
}

/// The text to store for `record`, or the reason not to store any.
pub fn admit(
    record: &MemoryRecord,
    text: &str,
    config: &Config,
    private_mode: bool,
) -> Result<String, Skip> {
    if !config.verbatim.enabled {
        return Err(Skip::SwitchOff);
    }
    if private_mode {
        return Err(Skip::PrivateMode);
    }
    if !matches!(record.source_type.as_str(), "screen" | "browser") {
        return Err(Skip::NotAScreenCapture);
    }
    if record.is_soft_deleted
        || Blocklist::is_internal_app(&record.app_name, record.bundle_id.as_deref())
        || Blocklist::is_blocked(&record.app_name, &config.blocklist)
        || Blocklist::is_context_blocked(
            record.url.as_deref(),
            Some(&record.window_title),
            &config.blocklist,
        )
    {
        return Err(Skip::Blocked);
    }
    let capped = cap_text(text, config.verbatim.max_bytes_per_memory);
    if capped.is_empty() {
        return Err(Skip::Empty);
    }
    if safety_gate::evaluate(
        Some(&record.app_name),
        record.bundle_id.as_deref(),
        record.url.as_deref(),
        Some(&record.window_title),
        Some(&capped),
        &config.blocklist,
    ) != SafetyDecision::Allow
    {
        return Err(Skip::Sensitive);
    }
    Ok(capped)
}

fn source_text(record: &MemoryRecord) -> &str {
    if record.clean_text.trim().is_empty() {
        &record.text
    } else {
        &record.clean_text
    }
}

/// Save the text of the records in a just-flushed batch. Only memories that
/// landed in the table get a row, so a record the store dropped as a duplicate
/// leaves nothing behind. Returns the rows written.
pub async fn keep_batch(
    store: &Store,
    records: &[MemoryRecord],
    config: &Config,
    private_mode: bool,
) -> Result<usize, String> {
    if !config.verbatim.enabled || private_mode {
        return Ok(0);
    }
    let mut candidates = Vec::new();
    for record in records {
        if let Ok(text) = admit(record, source_text(record), config, private_mode) {
            candidates.push(VerbatimEntry {
                memory_id: record.id.clone(),
                text,
                captured_at: record.timestamp,
            });
        }
    }
    if candidates.is_empty() {
        return Ok(0);
    }
    let ids: Vec<String> = candidates.iter().map(|e| e.memory_id.clone()).collect();
    let landed = store
        .persisted_memory_ids(&ids)
        .await
        .map_err(|e| e.to_string())?;
    candidates.retain(|e| landed.contains(&e.memory_id));
    store
        .put_verbatim(&candidates)
        .await
        .map_err(|e| e.to_string())?;
    Ok(candidates.len())
}

/// `keep_batch` for the live capture flush.
pub async fn keep_flushed_batch(state: &AppState, records: &[MemoryRecord]) {
    let config = state.config.read().clone();
    let private = state.is_incognito.load(std::sync::atomic::Ordering::SeqCst);
    match keep_batch(&state.store, records, &config, private).await {
        Ok(_) => {}
        Err(err) => tracing::warn!("Verbatim text not kept for a flushed batch: {err}"),
    }
}

/// The text a persisted memory holds now, read before a merge replaces its
/// row (`delete_memory_by_id` removes the text with it).
pub async fn text_before_merge(state: &AppState, existing_id: &str) -> Option<String> {
    if !state.config.read().verbatim.enabled {
        return None;
    }
    state
        .store
        .verbatim_text_unchecked(existing_id)
        .await
        .ok()
        .flatten()
}

/// After a new capture merged into a persisted memory: keep the older text
/// (if any) followed by the new capture's, under the same cap and checks.
pub async fn keep_after_persisted_merge(
    state: &AppState,
    carried: Option<String>,
    merged: &MemoryRecord,
    incoming: &MemoryRecord,
) {
    let config = state.config.read().clone();
    if !config.verbatim.enabled {
        return;
    }
    let private = state.is_incognito.load(std::sync::atomic::Ordering::SeqCst);
    let combined = join_texts(carried.as_deref(), source_text(incoming));
    let Ok(text) = admit(merged, &combined, &config, private) else {
        return;
    };
    let entry = VerbatimEntry {
        memory_id: merged.id.clone(),
        text,
        captured_at: merged.timestamp,
    };
    if let Err(err) = state.store.put_verbatim(&[entry]).await {
        tracing::warn!("Verbatim text not kept after a merge: {err}");
    }
}

fn join_texts(older: Option<&str>, newer: &str) -> String {
    let newer = newer.trim();
    match older.map(str::trim).filter(|text| !text.is_empty()) {
        None => newer.to_string(),
        Some(older) if older.contains(newer) => older.to_string(),
        Some(older) if newer.is_empty() => older.to_string(),
        Some(older) => format!("{older}\n\n{newer}"),
    }
}

/// What a sweep removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SweepReport {
    pub expired: usize,
    pub over_budget: usize,
    pub switched_off: bool,
}

/// Apply the switch, the retention window and the storage budget. Memories
/// and their summaries are untouched. Safe to run often.
pub async fn sweep(
    store: &Store,
    config: &VerbatimConfig,
    now_ms: i64,
) -> Result<SweepReport, String> {
    let mut report = SweepReport::default();
    if !config.enabled {
        let stats = store.verbatim_stats().await.map_err(|e| e.to_string())?;
        if stats.rows > 0 {
            store
                .delete_all_verbatim()
                .await
                .map_err(|e| e.to_string())?;
            store
                .reclaim_verbatim_space()
                .await
                .map_err(|e| e.to_string())?;
            report.switched_off = true;
        }
        return Ok(report);
    }
    let cutoff = now_ms - i64::from(config.retention_days) * 86_400_000;
    report.expired = store
        .delete_verbatim_older_than(cutoff)
        .await
        .map_err(|e| e.to_string())?;
    report.over_budget = store
        .enforce_verbatim_budget(u64::from(config.total_budget_mb) * 1024 * 1024)
        .await
        .map_err(|e| e.to_string())?;
    if report.expired + report.over_budget > 0 {
        store
            .reclaim_verbatim_space()
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(report)
}

/// `sweep` with the live config and clock; logs instead of failing.
pub async fn sweep_now(store: &Store, config: &VerbatimConfig) {
    match sweep(store, config, chrono::Utc::now().timestamp_millis()).await {
        Ok(report) if report.expired + report.over_budget > 0 || report.switched_off => {
            tracing::info!(
                expired = report.expired,
                over_budget = report.over_budget,
                switched_off = report.switched_off,
                "verbatim text swept"
            );
        }
        Ok(_) => {}
        Err(err) => tracing::warn!("Verbatim sweep failed: {err}"),
    }
}

/// The stored text for a memory the person may see, or `None`. The one door
/// for consumers; it applies `retrieve::memory_is_visible` to the memory
/// first and refuses text older than the retention window even if a sweep has
/// not run yet. At most `max_bytes` come back.
pub async fn text_for_memory(
    state: &AppState,
    memory_id: &str,
    max_bytes: usize,
) -> Result<Option<String>, String> {
    let (verbatim, blocklist) = {
        let config = state.config.read();
        (config.verbatim.clone(), config.blocklist.clone())
    };
    let Some(record) = state
        .store
        .get_memory_by_id(memory_id)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    if !readable(
        &record,
        &verbatim,
        &blocklist,
        chrono::Utc::now().timestamp_millis(),
    ) {
        return Ok(None);
    }
    let text = state
        .store
        .verbatim_text_unchecked(memory_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(text
        .map(|text| cap_text(&text, max_bytes.min(verbatim.max_bytes_per_memory)))
        .filter(|text| !text.is_empty()))
}

/// Whether the text of `record` may be served now.
pub(crate) fn readable(
    record: &MemoryRecord,
    config: &VerbatimConfig,
    blocklist: &[String],
    now_ms: i64,
) -> bool {
    let age_limit = i64::from(config.retention_days) * 86_400_000;
    config.enabled
        && now_ms - record.timestamp <= age_limit
        && crate::context_runtime::context_memory_is_visible(record, blocklist)
}

#[cfg(test)]
mod tests;
