//! Blocklist, wipe-all-data, proactive privacy alerts.

use super::common::push_unique_case_insensitive;
use crate::privacy::Blocklist;
use crate::AppState;
use crate::{graph::GraphStore, storage::Store};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn get_agent_notes_enabled(state: State<'_, Arc<AppState>>) -> Result<bool, String> {
    Ok(state.config.read().agent_notes_enabled)
}

#[tauri::command]
pub async fn set_agent_notes_enabled(
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<(), String> {
    update_agent_notes_enabled(&state.config, enabled, |config| {
        config.save().map_err(|error| error.to_string())
    })
}

fn update_agent_notes_enabled(
    config: &parking_lot::RwLock<crate::config::Config>,
    enabled: bool,
    save: impl FnOnce(&crate::config::Config) -> Result<(), String>,
) -> Result<(), String> {
    let mut current = config.write();
    let mut next = current.clone();
    next.agent_notes_enabled = enabled;
    // A failed save must not enable assistant writes for the current session.
    save(&next)?;
    *current = next;
    Ok(())
}

/// "Keep the full text of what FNDR sees on this Mac" (ADR 036).
#[tauri::command]
pub async fn get_verbatim_memory_enabled(state: State<'_, Arc<AppState>>) -> Result<bool, String> {
    Ok(state.config.read().verbatim.enabled)
}

/// Turning it off deletes every stored text at once; memories and their
/// summaries stay.
#[tauri::command]
pub async fn set_verbatim_memory_enabled(
    state: State<'_, Arc<AppState>>,
    enabled: bool,
) -> Result<(), String> {
    update_verbatim_enabled(&state.config, enabled, |config| {
        config.save().map_err(|error| error.to_string())
    })?;
    if !enabled {
        let verbatim = state.config.read().verbatim.clone();
        crate::verbatim::sweep(
            &state.store,
            &verbatim,
            chrono::Utc::now().timestamp_millis(),
        )
        .await?;
    }
    Ok(())
}

fn update_verbatim_enabled(
    config: &parking_lot::RwLock<crate::config::Config>,
    enabled: bool,
    save: impl FnOnce(&crate::config::Config) -> Result<(), String>,
) -> Result<(), String> {
    let mut current = config.write();
    let mut next = current.clone();
    next.verbatim.enabled = enabled;
    // A failed save must not widen what memory keeps for this session.
    save(&next)?;
    *current = next;
    Ok(())
}

/// Get blocklist
#[tauri::command]
pub async fn get_blocklist(state: State<'_, Arc<AppState>>) -> Result<Vec<String>, String> {
    let config = state.inner().config.read();
    Ok(config.blocklist.clone())
}

/// Set blocklist
#[tauri::command]
pub async fn set_blocklist(
    state: State<'_, Arc<AppState>>,
    apps: Vec<String>,
) -> Result<(), String> {
    {
        let mut config = state.inner().config.write();
        config.blocklist = apps;
        config.blocklist = config
            .blocklist
            .iter()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .fold(Vec::new(), |mut acc, value| {
                if !acc
                    .iter()
                    .any(|existing: &String| existing.eq_ignore_ascii_case(&value))
                {
                    acc.push(value);
                }
                acc
            });
        config
            .save()
            .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    }
    cancel_screen_guide_after_blocklist_change(state.inner());
    Ok(())
}

/// Delete all data
#[tauri::command]
pub async fn delete_all_data(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    // 1. Clear memory records
    state
        .inner()
        .store
        .delete_all()
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    // 2. Clear both graph table pairs and prune old Lance versions.
    state
        .inner()
        .graph
        .clear_all()
        .await
        .map_err(|error| error.to_string())?;
    state
        .inner()
        .store
        .compact_and_prune_erased_data()
        .await
        .map_err(|error| error.to_string())?;
    state.invalidate_memory_derived_caches();

    // 3. Delete persisted capture artifacts
    for artifact_dir in ["frames", "screenshots", "meetings"] {
        let path = state.inner().store.data_dir().join(artifact_dir);
        if path.exists() {
            if let Err(e) = std::fs::remove_dir_all(&path) {
                tracing::warn!("Failed to remove {} dir: {}", artifact_dir, e);
            }
        }
    }

    tracing::info!("All FNDR data deleted");
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub struct EraseRangeReport {
    pub matched: usize,
    pub deleted: usize,
}

/// Preview or erase memories with timestamps in [start_ms, end_ms).
pub(crate) async fn erase_memories_in_range(
    store: &Arc<Store>,
    graph: &GraphStore,
    start_ms: i64,
    end_ms: i64,
    dry_run: bool,
    tombstones: Option<&crate::storage::StateStore>,
) -> Result<EraseRangeReport, String> {
    if start_ms >= end_ms {
        return Err("The end of the range must be after its start.".to_string());
    }
    let mut records = store
        .get_memories_in_range(start_ms, end_ms)
        .await
        .map_err(|error| error.to_string())?;
    records.retain(|record| record.timestamp < end_ms);
    let matched = records.len();
    if dry_run {
        return Ok(EraseRangeReport {
            matched,
            deleted: 0,
        });
    }
    if let Some(state_store) = tombstones {
        crate::import::tombstone_records(state_store, &records)?;
    }
    let deleted = crate::privacy::erase::erase_memories(store, graph, &records).await?;
    Ok(EraseRangeReport { matched, deleted })
}

#[tauri::command]
pub async fn delete_memories_in_range(
    state: State<'_, Arc<AppState>>,
    start_ms: i64,
    end_ms: i64,
    dry_run: bool,
) -> Result<EraseRangeReport, String> {
    let report = erase_memories_in_range(
        &state.store,
        &state.graph,
        start_ms,
        end_ms,
        dry_run,
        Some(&*state.state_store),
    )
    .await?;
    if report.deleted > 0 {
        state.invalidate_memory_derived_caches();
    }
    Ok(report)
}

#[tauri::command]
pub async fn export_memories_archive(
    state: State<'_, Arc<AppState>>,
) -> Result<crate::privacy::export::ExportReport, String> {
    let directory = tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("Choose where to save the FNDR archive")
            .pick_folder()
    })
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "No folder selected".to_string())?;
    let archive_dir = directory.join(format!(
        "FNDR export {} {}",
        chrono::Local::now().format("%Y-%m-%d %H-%M-%S"),
        uuid::Uuid::new_v4().simple()
    ));
    let blocklist = state.config.read().blocklist.clone();
    crate::privacy::export::export_archive(
        &state.store,
        &blocklist,
        &crate::privacy::vault::KeychainKey::default(),
        &archive_dir,
    )
    .await
}

#[tauri::command]
pub async fn import_memories_archive(state: State<'_, Arc<AppState>>) -> Result<usize, String> {
    let path = tokio::task::spawn_blocking(|| {
        rfd::FileDialog::new()
            .set_title("Choose an encrypted FNDR JSONL archive")
            .add_filter("FNDR archives", &["fndrenc"])
            .pick_file()
    })
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "No archive selected".to_string())?;
    let blocklist = state.config.read().blocklist.clone();
    let restored = crate::privacy::export::import_archive(
        &state.store,
        &blocklist,
        &crate::privacy::vault::KeychainKey::default(),
        &path,
    )
    .await?;
    if restored > 0 {
        state.invalidate_memory_derived_caches();
    }
    Ok(restored)
}

#[tauri::command]
pub async fn get_encryption_statement() -> Result<crate::privacy::vault::EncryptionStatement, String>
{
    let filevault = tokio::task::spawn_blocking(crate::privacy::vault::file_vault_status)
        .await
        .map_err(|error| error.to_string())?;
    Ok(crate::privacy::vault::encryption_statement(filevault))
}

#[tauri::command]
pub fn get_privacy_exclusions() -> Vec<crate::privacy::exclusions::ExclusionCategory> {
    crate::privacy::exclusions::categories()
}

// ── Proactive Privacy Shield Commands ───────────────────────────────────────────

#[tauri::command]
pub async fn get_privacy_alerts(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<crate::PrivacyAlert>, String> {
    let pending = state.pending_privacy_alerts.read();
    Ok(pending.clone())
}

/// Push the current pending privacy alerts to the UI over `privacy://alerts`.
///
/// No-op before the app handle is registered during setup.
pub fn emit_privacy_alerts(state: &AppState) {
    let alerts = state.pending_privacy_alerts.read().clone();
    let handle = state.app_handle.read().clone();
    if let Some(handle) = handle {
        let _ = tauri::Emitter::emit(&handle, "privacy://alerts", alerts);
    }
}

#[tauri::command]
pub async fn dismiss_privacy_alert(
    site: String,
    state: State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let site_key = privacy_site_key(&site);
    {
        let mut pending = state.pending_privacy_alerts.write();
        pending.retain(|a| !privacy_site_matches(&a.domain_or_title, &site_key));
    }
    {
        let mut snoozed = state.snoozed_privacy_alerts.write();
        // Keep the in-memory cache aligned until the persisted dismissal is reloaded.
        let expire = chrono::Local::now().timestamp() + (30 * 24 * 60 * 60);
        snoozed.insert(site_key.clone(), expire);
    }
    {
        let mut config = state.config.write();
        push_unique_case_insensitive(&mut config.dismissed_privacy_alerts, site_key);
        config.save().map_err(|e| e.to_string())?;
    }
    emit_privacy_alerts(state.inner());
    Ok(())
}

#[tauri::command]
pub async fn add_to_blocklist(site: String, state: State<'_, Arc<AppState>>) -> Result<(), String> {
    let site_key = privacy_site_key(&site);

    // 1. Remove from pending alerts
    {
        let mut pending = state.pending_privacy_alerts.write();
        pending.retain(|a| !privacy_site_matches(&a.domain_or_title, &site_key));
    }

    // 2. Add to config blocklist
    {
        let mut config = state.config.write();
        if !config
            .blocklist
            .iter()
            .any(|b| b.eq_ignore_ascii_case(&site_key))
        {
            config.blocklist.push(site_key.clone());
        }
        config
            .dismissed_privacy_alerts
            .retain(|value| !privacy_site_matches(value, &site_key));
        config.save().map_err(|e| e.to_string())?;
    }
    // Treat a newly persisted exclusion as an immediate privacy boundary.
    // In-flight OCR/model/speech from the formerly allowed context must not
    // survive while the slower retroactive memory deletion runs below.
    cancel_screen_guide_after_blocklist_change(state.inner());

    // 3. Retroactively delete memories with this site if we grabbed it during the alert period
    let deleted_ids: Vec<String> = match state
        .store
        .delete_memories_by_domain(&site_key)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())
    {
        Err(e) => {
            tracing::error!(
                "Failed to retroactively delete memories for blocked site {}: {}",
                site_key,
                e
            );
            Vec::new()
        }
        Ok(deleted_ids) => {
            state.invalidate_memory_derived_caches();
            deleted_ids
        }
    };
    for memory_id in &deleted_ids {
        if let Err(e) = state
            .graph
            .delete_memory_node(memory_id)
            .await
            .map_err(|e: Box<dyn std::error::Error>| e.to_string())
        {
            tracing::warn!(
                "Failed to delete graph node for retroactively deleted memory {}: {}",
                memory_id,
                e
            );
        }
    }

    emit_privacy_alerts(state.inner());
    Ok(())
}

fn cancel_screen_guide_after_blocklist_change(state: &AppState) {
    if let Some(app_handle) = state.app_handle.read().clone() {
        super::screen_guide::cancel_screen_guide_for_privacy_settings(&app_handle);
    }
}

fn privacy_site_key(site: &str) -> String {
    Blocklist::context_key(Some(site), Some(site)).unwrap_or_else(|| site.trim().to_string())
}

fn privacy_site_matches(value: &str, site_key: &str) -> bool {
    if value.eq_ignore_ascii_case(site_key) {
        return true;
    }
    let site_values = vec![site_key.to_string()];
    let value_values = vec![value.to_string()];
    Blocklist::is_context_blocked(Some(value), Some(value), &site_values)
        || Blocklist::is_context_blocked(Some(site_key), Some(site_key), &value_values)
}

#[cfg(test)]
mod agent_notes_settings_tests {
    use super::*;
    use crate::config::Config;
    use parking_lot::RwLock;

    #[test]
    fn agent_notes_setting_persists_both_values_without_changing_other_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let config = RwLock::new(Config::default());
        let initial_blocklist = config.read().blocklist.clone();
        assert!(!config.read().agent_notes_enabled);

        for enabled in [true, false] {
            update_agent_notes_enabled(&config, enabled, |next| {
                let contents = toml::to_string_pretty(next).map_err(|error| error.to_string())?;
                std::fs::write(&path, contents).map_err(|error| error.to_string())
            })
            .unwrap();

            let saved: Config = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(saved.agent_notes_enabled, enabled);
            assert_eq!(config.read().agent_notes_enabled, enabled);
            assert_eq!(saved.blocklist, initial_blocklist);
        }
    }

    #[test]
    fn agent_notes_setting_save_failure_preserves_previous_consent() {
        for previous in [false, true] {
            let mut initial = Config::default();
            initial.agent_notes_enabled = previous;
            let config = RwLock::new(initial);

            let result = update_agent_notes_enabled(&config, !previous, |_| {
                Err("config is read-only".to_string())
            });

            assert_eq!(result.unwrap_err(), "config is read-only");
            assert_eq!(config.read().agent_notes_enabled, previous);
        }
    }

    #[test]
    fn verbatim_switch_persists_and_a_failed_save_keeps_the_previous_value() {
        let config = RwLock::new(Config::default());
        assert!(!config.read().verbatim.enabled);
        let saved = std::cell::Cell::new(false);
        update_verbatim_enabled(&config, true, |next| {
            saved.set(next.verbatim.enabled);
            Ok(())
        })
        .unwrap();
        assert!(saved.get() && config.read().verbatim.enabled);
        assert_eq!(config.read().verbatim.retention_days, 30);

        let result = update_verbatim_enabled(&config, false, |_| Err("read-only".to_string()));
        assert_eq!(result.unwrap_err(), "read-only");
        assert!(config.read().verbatim.enabled);
    }
}

#[cfg(test)]
mod erasure_tests {
    use super::*;
    use crate::graph::GraphStore;
    use crate::graph::{graph_store, schema};
    use crate::storage::{MemoryChunkRecord, MemoryRecord, Store, Task, TaskType};

    fn memory(id: &str, timestamp: i64) -> MemoryRecord {
        MemoryRecord {
            id: id.to_string(),
            timestamp,
            app_name: "Synthetic Notes".to_string(),
            window_title: "Planning".to_string(),
            session_id: "synthetic-session".to_string(),
            day_bucket: "2026-10-15".to_string(),
            clean_text: "Synthetic planning note".to_string(),
            snippet: "Synthetic planning note".to_string(),
            embedding: vec![0.01; crate::embedding::EMBEDDING_DIM],
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn range_preview_counts_only_matching_rows_without_deleting_anything() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let store = Arc::new(
            tokio::task::spawn_blocking(move || Store::new(&path).unwrap())
                .await
                .unwrap(),
        );
        let graph = GraphStore::new(store.clone());
        store
            .add_batch_preserving_ids(&[
                memory("before", 999),
                memory("inside", 1_000),
                memory("after", 2_000),
            ])
            .await
            .unwrap();

        let preview = erase_memories_in_range(&store, &graph, 1_000, 2_000, true, None)
            .await
            .unwrap();

        assert_eq!(preview.matched, 1);
        assert_eq!(preview.deleted, 0);
        assert_eq!(store.list_all_memories().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn range_erase_removes_v4_v5_chunks_graph_and_task_references() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let store = Arc::new(
            tokio::task::spawn_blocking(move || Store::new(&path).unwrap())
                .await
                .unwrap(),
        );
        let graph = GraphStore::new(store.clone());
        let inside = memory("inside", 1_000);
        let outside = memory("outside", 2_000);
        store
            .add_batch_preserving_ids(&[inside.clone(), outside.clone()])
            .await
            .unwrap();
        let mut v5 = inside.clone();
        for vector in [
            &mut v5.embedding,
            &mut v5.snippet_embedding,
            &mut v5.support_embedding,
        ] {
            *vector = vec![0.01; crate::inference::model_config::BGE_V5_DIMENSIONS];
        }
        store.add_v5_batch_preserving_ids(&[v5]).await.unwrap();
        store
            .upsert_memory_chunks(&[MemoryChunkRecord {
                id: "chunk-inside".to_string(),
                memory_id: "inside".to_string(),
                chunk_index: 0,
                line_kind: "plain".to_string(),
                text: "Synthetic planning note".to_string(),
                embedding: vec![0.01; crate::inference::model_config::BGE_V5_DIMENSIONS],
                created_at: 1_000,
                app_name: "Synthetic Notes".to_string(),
                window_title: "Planning".to_string(),
                day_bucket: "2026-10-15".to_string(),
                content_hash: "synthetic-chunk".to_string(),
            }])
            .await
            .unwrap();
        graph.ingest_memory(&inside).await.unwrap();
        graph.ingest_memory(&outside).await.unwrap();
        let insight = graph_store::GraphStore::new(store.clone());
        let now = chrono::Utc::now();
        let only_inside = schema::GraphNode {
            id: uuid::Uuid::new_v4(),
            node_type: schema::GraphNodeType::Concept,
            label: "Only inside".to_string(),
            confidence: 0.9,
            source_memory_ids: vec!["inside".to_string()],
            embedding: None,
            created_at: now,
            updated_at: now,
            stale: false,
            metadata: serde_json::json!({}),
        };
        let mut shared = only_inside.clone();
        shared.id = uuid::Uuid::new_v4();
        shared.label = "Shared concept".to_string();
        shared.source_memory_ids.push("outside".to_string());
        insight.upsert_node(&only_inside).await.unwrap();
        insight.upsert_node(&shared).await.unwrap();
        insight
            .upsert_edge(&schema::GraphEdge {
                id: uuid::Uuid::new_v4(),
                source_id: only_inside.id,
                target_id: shared.id,
                edge_type: schema::GraphEdgeType::MentionedIn,
                confidence: 0.8,
                conflict_flag: false,
                created_at: now,
                metadata: serde_json::json!({}),
            })
            .await
            .unwrap();
        store
            .upsert_tasks(&[
                Task {
                    id: "grounded".to_string(),
                    title: "Follow up".to_string(),
                    description: String::new(),
                    source_app: "Synthetic Notes".to_string(),
                    source_memory_id: Some("inside".to_string()),
                    created_at: 1_000,
                    due_date: None,
                    is_completed: false,
                    is_dismissed: false,
                    task_type: TaskType::Todo,
                    linked_urls: Vec::new(),
                    linked_memory_ids: Vec::new(),
                },
                Task {
                    id: "linked".to_string(),
                    title: "Review".to_string(),
                    description: String::new(),
                    source_app: "Synthetic Notes".to_string(),
                    source_memory_id: Some("outside".to_string()),
                    created_at: 2_000,
                    due_date: None,
                    is_completed: false,
                    is_dismissed: false,
                    task_type: TaskType::Todo,
                    linked_urls: Vec::new(),
                    linked_memory_ids: vec!["inside".to_string()],
                },
            ])
            .await
            .unwrap();

        let report = erase_memories_in_range(&store, &graph, 1_000, 2_000, false, None)
            .await
            .unwrap();

        assert_eq!(report.matched, 1);
        assert_eq!(report.deleted, 1);
        assert!(store.get_memory_by_id("inside").await.unwrap().is_none());
        assert!(store.get_memory_by_id("outside").await.unwrap().is_some());
        assert!(store
            .list_chunks_for_memory("inside")
            .await
            .unwrap()
            .is_empty());
        assert!(store
            .get_v5_search_results_by_ids(&["inside".to_string()], None, None)
            .await
            .unwrap()
            .is_empty());
        let nodes = store.get_all_nodes().await.unwrap();
        assert!(!nodes.iter().any(|node| node.id == "memory:inside"));
        assert!(nodes.iter().any(|node| node.id == "memory:outside"));
        let insight_nodes = insight.all_nodes().await.unwrap();
        assert!(!insight_nodes.iter().any(|node| node.id == only_inside.id));
        assert_eq!(
            insight_nodes
                .iter()
                .find(|node| node.id == shared.id)
                .unwrap()
                .source_memory_ids,
            vec!["outside"]
        );
        assert!(insight.all_edges().await.unwrap().is_empty());
        let tasks = store.list_tasks().await.unwrap();
        assert!(
            tasks
                .iter()
                .find(|task| task.id == "grounded")
                .unwrap()
                .is_dismissed
        );
        assert!(tasks
            .iter()
            .find(|task| task.id == "linked")
            .unwrap()
            .linked_memory_ids
            .is_empty());
        assert_eq!(
            store.memories_table_scale_stats().await.unwrap().versions,
            1
        );
    }

    #[tokio::test]
    async fn delete_all_graph_clears_nodes_and_edges() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let store = Arc::new(
            tokio::task::spawn_blocking(move || Store::new(&path).unwrap())
                .await
                .unwrap(),
        );
        let graph = GraphStore::new(store.clone());
        graph.ingest_memory(&memory("inside", 1_000)).await.unwrap();
        assert!(!store.get_all_nodes().await.unwrap().is_empty());
        assert!(!store.get_all_edges().await.unwrap().is_empty());

        graph.clear_all().await.unwrap();

        assert!(store.get_all_nodes().await.unwrap().is_empty());
        assert!(store.get_all_edges().await.unwrap().is_empty());
    }
}
