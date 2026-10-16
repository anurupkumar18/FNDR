//! The only writer of imported rows.
//!
//! `ImportRunner` scans a source in batches, admits every item
//! ([`super::admit::admit`]), drops duplicates, embeds, and writes leaf rows.
//! Nothing else in the crate stores `import:*` rows, and connectors under
//! `sources/` cannot reach the store.

use super::admit::{admit, Admission, AdmitSettings};
use super::consent;
use super::cursor::{self, CursorState};
use super::record::build_record;
use super::{ImportItem, ImportKind, ImportSource, Permission, ITEM_TEXT_MAX_CHARS};
use crate::embedding::{Embedder, EmbeddingBackend};
use crate::memory_compaction::{is_low_signal_embedding, mean_pool_embeddings};
use crate::memory_embedding_document::compose_memory_embedding_document;
use crate::storage::MemoryRecord;
use crate::AppState;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

/// Items asked of a source per `scan` call.
const BATCH_LIMIT: usize = 200;
/// A run stops here even if the source has more. The next run resumes.
const MAX_ITEMS_PER_RUN: usize = 100_000;
const TITLE_MAX_CHARS: usize = 300;
const FORGET_WAIT_ATTEMPTS: u32 = 200;
const MAX_WINDOW_DAYS: u32 = 3650;

/// Where text vectors come from. Production uses the shared capture embedder
/// and refuses to store a row without a real vector; tests pass one in.
#[derive(Clone)]
pub enum ImportEmbedder {
    Shared,
    /// Tests: this embedder, mock or not; `None` is a missing model.
    #[cfg(test)]
    Given(Option<Arc<Embedder>>),
}

impl ImportEmbedder {
    fn with<T>(&self, f: impl FnOnce(&Embedder) -> Option<T>) -> Option<T> {
        match self {
            ImportEmbedder::Shared => crate::ipc::commands::common::shared_embedder()
                .ok()
                .filter(|embedder| matches!(embedder.backend(), EmbeddingBackend::Real))
                .and_then(|embedder| {
                    let result = f(embedder);
                    if matches!(embedder.backend(), EmbeddingBackend::Real) {
                        result
                    } else {
                        None
                    }
                }),
            #[cfg(test)]
            ImportEmbedder::Given(embedder) => embedder.as_deref().and_then(f),
        }
    }

    /// The vectors capture and `fndr.remember` write, from the same document
    /// composition. False when the model is unavailable or returns nothing
    /// usable.
    fn embed(&self, record: &mut MemoryRecord) -> bool {
        let document = compose_memory_embedding_document(record, None);
        let inputs = document
            .text_embedding_inputs()
            .into_iter()
            .map(|text| (record.app_name.clone(), record.window_title.clone(), text))
            .collect::<Vec<_>>();
        let vectors = self
            .with(|embedder| embedder.embed_batch_with_context(&inputs).ok())
            .filter(|vectors| {
                vectors.len() == inputs.len()
                    && !vectors.iter().any(|vector| is_low_signal_embedding(vector))
            });
        let Some(vectors) = vectors else {
            return false;
        };
        record.embedding = vectors[0].clone();
        record.snippet_embedding = vectors[1].clone();
        record.support_embedding = if vectors.len() > 2 {
            mean_pool_embeddings(&vectors[2..])
        } else {
            vec![0.0; vectors[0].len()]
        };
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    #[default]
    Idle,
    Running,
    Done,
    Cancelled,
    Failed,
}

/// Counts and a state for one source's latest run. Never holds item content.
#[derive(Debug, Clone, Serialize, Default, PartialEq, Eq)]
pub struct ImportStatus {
    pub source: String,
    pub state: RunState,
    pub window_days: u32,
    pub scanned: u32,
    pub stored: u32,
    pub updated: u32,
    pub duplicates: u32,
    pub redacted: u32,
    pub skipped: u32,
    /// Skip reason (see `admit::SkipReason::as_str`) to count.
    pub skip_reasons: BTreeMap<String, u32>,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceInfo {
    pub id: String,
    pub kind: ImportKind,
    pub label: String,
    pub permissions: Vec<Permission>,
    /// Rows of this source in the store now.
    pub row_count: usize,
    pub status: ImportStatus,
}

struct Slot {
    status: ImportStatus,
    cancel: Arc<AtomicBool>,
}

struct Inner {
    sources: Vec<Arc<dyn ImportSource>>,
    embedder: ImportEmbedder,
    slots: Mutex<HashMap<String, Slot>>,
}

#[derive(Clone)]
pub struct ImportRunner {
    inner: Arc<Inner>,
}

enum RunEnd {
    Done,
    Cancelled,
}

impl ImportRunner {
    pub fn new(sources: Vec<Arc<dyn ImportSource>>, embedder: ImportEmbedder) -> Self {
        Self {
            inner: Arc::new(Inner {
                sources,
                embedder,
                slots: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// The app's runner: the registered connectors and the shared embedder.
    pub fn global() -> &'static ImportRunner {
        static RUNNER: OnceLock<ImportRunner> = OnceLock::new();
        RUNNER
            .get_or_init(|| ImportRunner::new(super::sources::registered(), ImportEmbedder::Shared))
    }

    fn source(&self, id: &str) -> Result<Arc<dyn ImportSource>, String> {
        self.inner
            .sources
            .iter()
            .find(|source| source.id() == id)
            .cloned()
            .ok_or_else(|| format!("Unknown import source: {id}"))
    }

    pub async fn list_sources(&self, app: &AppState) -> Vec<SourceInfo> {
        let mut infos = Vec::new();
        for source in &self.inner.sources {
            let row_count = app
                .store
                .count_memories_by_source_type(&source.kind().source_type())
                .await
                .unwrap_or(0);
            let mut status = self.status(source.id());
            if status.state == RunState::Idle {
                if let Ok(Some(saved)) = cursor::load(&app.state_store, source.kind()) {
                    status.window_days = saved.window_days;
                    status.finished_at_ms = Some(saved.updated_at_ms);
                }
            }
            infos.push(SourceInfo {
                id: source.id().to_string(),
                kind: source.kind(),
                label: source.label().to_string(),
                permissions: source.permissions_needed(),
                row_count,
                status,
            });
        }
        infos
    }

    pub fn status(&self, source_id: &str) -> ImportStatus {
        self.inner
            .slots
            .lock()
            .get(source_id)
            .map(|slot| slot.status.clone())
            .unwrap_or_else(|| ImportStatus {
                source: source_id.to_string(),
                ..ImportStatus::default()
            })
    }

    /// Starts a background run. The window is clamped to `1..=` the import
    /// retention (rows older than that would be swept at once).
    pub fn start(
        &self,
        app: Arc<AppState>,
        source_id: &str,
        window_days: u32,
    ) -> Result<ImportStatus, String> {
        let source = self.source(source_id)?;
        if !consent::get(&app.state_store, source.kind())? {
            return Err("Import requires consent for this source.".to_string());
        }
        let retention = app.config.read().import_retention_days;
        let ceiling = if retention == 0 {
            MAX_WINDOW_DAYS
        } else {
            retention
        };
        let window_days = window_days.clamp(1, ceiling);
        let cancel = Arc::new(AtomicBool::new(false));
        let status = ImportStatus {
            source: source_id.to_string(),
            state: RunState::Running,
            window_days,
            started_at_ms: now_ms(),
            ..ImportStatus::default()
        };
        {
            let mut slots = self.inner.slots.lock();
            if slots
                .get(source_id)
                .is_some_and(|slot| slot.status.state == RunState::Running)
            {
                return Err("This import is already running.".to_string());
            }
            slots.insert(
                source_id.to_string(),
                Slot {
                    status: status.clone(),
                    cancel: cancel.clone(),
                },
            );
        }
        let runner = self.clone();
        tokio::spawn(async move {
            runner.finish(
                source.id(),
                execute(&runner, app, source, window_days, cancel).await,
            );
        });
        Ok(status)
    }

    fn finish(&self, source_id: &str, outcome: Result<RunEnd, String>) {
        let mut slots = self.inner.slots.lock();
        let Some(slot) = slots.get_mut(source_id) else {
            return;
        };
        slot.status.finished_at_ms = Some(now_ms());
        match outcome {
            Ok(RunEnd::Done) => slot.status.state = RunState::Done,
            Ok(RunEnd::Cancelled) => slot.status.state = RunState::Cancelled,
            Err(message) => {
                tracing::warn!(source = source_id, reason = %message, "import:run_failed");
                slot.status.state = RunState::Failed;
                slot.status.error = Some(message);
            }
        }
    }

    /// Asks a running import to stop. It stops before the next item; what was
    /// stored stays, and the saved cursor is the last whole batch.
    pub fn cancel(&self, source_id: &str) -> bool {
        match self.inner.slots.lock().get(source_id) {
            Some(slot) if slot.status.state == RunState::Running => {
                slot.cancel.store(true, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }

    /// Removes everything one source ever stored: rows, chunks, vectors (they
    /// are columns of the row, and the BGE table's rows), graph links, and
    /// the saved cursor.
    pub async fn forget(&self, app: &AppState, source_id: &str) -> Result<usize, String> {
        let source = self.source(source_id)?;
        self.cancel(source_id);
        for _ in 0..FORGET_WAIT_ATTEMPTS {
            if self.status(source_id).state != RunState::Running {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        if self.status(source_id).state == RunState::Running {
            return Err("The import is still stopping. Try again in a moment.".to_string());
        }

        let records = app
            .store
            .memories_by_source_type(&source.kind().source_type())
            .await
            .map_err(|err| err.to_string())?;
        let removed =
            crate::privacy::erase::erase_memories(&app.store, &app.graph, &records).await?;
        cursor::clear(&app.state_store, source.kind())?;
        cursor::clear_tombstones(&app.state_store, source.kind())?;
        consent::set(&app.state_store, source.kind(), false)?;
        self.inner.slots.lock().remove(source_id);
        app.invalidate_memory_derived_caches();
        tracing::info!(source = source_id, rows = removed, "import:forgotten");
        Ok(removed)
    }
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn update(runner: &ImportRunner, source_id: &str, change: impl FnOnce(&mut ImportStatus)) {
    if let Some(slot) = runner.inner.slots.lock().get_mut(source_id) {
        change(&mut slot.status);
    }
}

async fn execute(
    runner: &ImportRunner,
    app: Arc<AppState>,
    source: Arc<dyn ImportSource>,
    window_days: u32,
    cancel: Arc<AtomicBool>,
) -> Result<RunEnd, String> {
    let kind = source.kind();
    let source_id = source.id();
    let saved = cursor::load(&app.state_store, kind)?;
    let mut position = cursor::resume_point(saved.as_ref(), window_days);
    let mut total = 0usize;

    loop {
        if cancel.load(Ordering::SeqCst) {
            return Ok(RunEnd::Cancelled);
        }
        let batch = {
            let source = source.clone();
            let since = position.clone();
            tokio::task::spawn_blocking(move || source.scan(since, window_days, BATCH_LIMIT))
                .await
                .map_err(|err| err.to_string())?
                .map_err(|err| err.to_string())?
        };
        if cancel.load(Ordering::SeqCst) {
            return Ok(RunEnd::Cancelled);
        }
        let done = batch.done;
        let next = batch.next.clone();
        total += batch.items.len();

        let stored = store_batch(runner, &app, &source, batch.items, &cancel).await?;
        if !stored {
            return Ok(RunEnd::Cancelled);
        }

        let progressed = next.as_ref().is_some_and(|n| Some(n) != position.as_ref());
        if !done && !progressed {
            return Err("The source did not advance its position.".to_string());
        }
        let finished = done || total >= MAX_ITEMS_PER_RUN;
        let new_position = next.or(position.clone());
        cursor::save(
            &app.state_store,
            kind,
            &CursorState {
                cursor: new_position.as_ref().map(|c| c.0.clone()),
                window_days,
                complete: done,
                updated_at_ms: now_ms(),
            },
        )?;
        position = new_position;
        update(runner, source_id, |_| {});
        if finished {
            return Ok(RunEnd::Done);
        }
    }
}

/// Admits, deduplicates, embeds and writes one batch. Returns false when a
/// cancel arrived first; nothing of the batch is written then.
async fn store_batch(
    runner: &ImportRunner,
    app: &Arc<AppState>,
    source: &Arc<dyn ImportSource>,
    items: Vec<ImportItem>,
    cancel: &AtomicBool,
) -> Result<bool, String> {
    let source_id = source.id();
    let settings = AdmitSettings::from_config(&app.config.read());
    let (retention_days, now) = (app.config.read().import_retention_days, now_ms());
    let oldest_kept = (retention_days > 0).then(|| now - i64::from(retention_days) * 86_400_000);
    let tombstones = cursor::load_tombstones(&app.state_store, source.kind())?;

    let mut admitted: Vec<ImportItem> = Vec::new();
    let mut seen = HashSet::new();
    let mut counts = Counts::default();
    for mut item in items {
        if cancel.load(Ordering::SeqCst) {
            return Ok(false);
        }
        counts.scanned += 1;
        item.kind = source.kind();
        item.title = item.title.chars().take(TITLE_MAX_CHARS).collect();
        item.text = item.text.chars().take(ITEM_TEXT_MAX_CHARS).collect();
        if item.external_id.trim().is_empty() {
            counts.skip("invalid_item");
            continue;
        }
        if tombstones.contains(&item.record_id()) {
            counts.skip("erased_by_person");
            continue;
        }
        if oldest_kept.is_some_and(|oldest| item.timestamp_ms < oldest) {
            counts.skip("outside_retention");
            continue;
        }
        match admit(&item, &settings) {
            Admission::Skip(reason) => {
                counts.skip(reason.as_str());
                continue;
            }
            Admission::Redact(text) => {
                item.text = text;
                counts.redacted += 1;
            }
            Admission::Allow => {}
        }
        if !seen.insert(item.record_id()) {
            counts.duplicates += 1;
            continue;
        }
        admitted.push(item);
    }

    let ids = admitted
        .iter()
        .map(ImportItem::record_id)
        .collect::<Vec<_>>();
    let existing = app
        .store
        .get_memories_by_ids(&ids)
        .await
        .map_err(|err| err.to_string())?;

    let mut fresh: Vec<MemoryRecord> = Vec::new();
    let mut changed: Vec<MemoryRecord> = Vec::new();
    for item in &admitted {
        let record = build_record(item, now);
        match existing.get(&record.id) {
            Some(old) if old.content_hash == record.content_hash => counts.duplicates += 1,
            Some(_) => changed.push(record),
            None => fresh.push(record),
        }
    }

    let embedder = runner.inner.embedder.clone();
    let (fresh, changed, unembeddable) =
        tokio::task::spawn_blocking(move || embed_all(&embedder, fresh, changed))
            .await
            .map_err(|err| err.to_string())?;
    if unembeddable > 0 && fresh.is_empty() && changed.is_empty() && counts.duplicates == 0 {
        return Err("FNDR's embedding model is not available, so nothing was stored.".to_string());
    }
    counts.skipped += unembeddable;
    if unembeddable > 0 {
        *counts
            .reasons
            .entry("no_usable_embedding".into())
            .or_default() += unembeddable;
    }

    if cancel.load(Ordering::SeqCst) {
        return Ok(false);
    }
    if !fresh.is_empty() {
        app.store
            .add_batch_preserving_ids(&fresh)
            .await
            .map_err(|err| err.to_string())?;
    }
    for record in &changed {
        app.store
            .delete_chunks_for_memory(&record.id)
            .await
            .map_err(|err| err.to_string())?;
        app.store
            .replace_memory_preserving_chunks(record)
            .await
            .map_err(|err| err.to_string())?;
    }
    if !fresh.is_empty() || !changed.is_empty() {
        app.invalidate_memory_derived_caches();
    }

    let (stored, updated) = (fresh.len() as u32, changed.len() as u32);
    update(runner, source_id, |status| {
        status.scanned += counts.scanned;
        status.stored += stored;
        status.updated += updated;
        status.duplicates += counts.duplicates;
        status.redacted += counts.redacted;
        status.skipped += counts.skipped;
        for (reason, n) in &counts.reasons {
            *status.skip_reasons.entry(reason.clone()).or_default() += n;
        }
    });
    Ok(true)
}

fn embed_all(
    embedder: &ImportEmbedder,
    fresh: Vec<MemoryRecord>,
    changed: Vec<MemoryRecord>,
) -> (Vec<MemoryRecord>, Vec<MemoryRecord>, u32) {
    let mut failed = 0u32;
    let mut run = |records: Vec<MemoryRecord>| {
        records
            .into_iter()
            .filter_map(|mut record| {
                if embedder.embed(&mut record) {
                    Some(record)
                } else {
                    failed += 1;
                    None
                }
            })
            .collect::<Vec<_>>()
    };
    let fresh = run(fresh);
    let changed = run(changed);
    (fresh, changed, failed)
}

#[derive(Default)]
struct Counts {
    scanned: u32,
    duplicates: u32,
    redacted: u32,
    skipped: u32,
    reasons: BTreeMap<String, u32>,
}

impl Counts {
    fn skip(&mut self, reason: &str) {
        self.skipped += 1;
        *self.reasons.entry(reason.to_string()).or_default() += 1;
    }
}
