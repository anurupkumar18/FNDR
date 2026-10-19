//! Files search while FNDR runs (ADR 033 part 6): one worker thread keeps the
//! index in step with the chosen folders, and searches are answered from the
//! same index file.
//!
//! - A folder watcher marks the index dirty; the worker syncs (metadata only)
//!   a few seconds after things go quiet, so a new file is searchable soon.
//! - Reading waits while the person paused it, outside their indexing hours
//!   for a big backlog, or while [`Hooks::hold`] says so (FNDR paused, memory
//!   pressure, Qwen holding the model lock).
//! - The model helper with its media encoders runs only while there is work
//!   and quits when the work is done. Searches start a text-only helper (310
//!   MB) that, with the loaded vectors, is let go after a few idle minutes.
//!
//! Status goes out through [`Hooks::emit`] as counts and a state, never file
//! names or contents.

use super::helper::FilesHelper;
use super::indexer::{self, IndexRun, Tools};
use super::reader::FilesReader;
use super::searcher::{self, FileHit, SearchOptions, Searcher};
use super::settings::FilesSettings;
use super::store::{FilesStore, IndexCounts};
use super::{model, rules, FileKind};
use parking_lot::{Condvar, Mutex, RwLock};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Files read per batch before the worker looks at holds and settings again.
const BATCH: usize = 4;
/// A sync this long after the last change the watcher saw.
const QUIET: Duration = Duration::from_secs(3);
/// How often an idle worker looks around anyway.
const TICK: Duration = Duration::from_secs(60);
/// Search's model and vectors go after this long unused.
const SEARCH_IDLE: Duration = Duration::from_secs(5 * 60);
/// A backlog bigger than this waits for the indexing hours and for power; a
/// few new files are read at once, they take seconds.
const BIG_BACKLOG: i64 = 50;

/// What the app supplies: FNDR's own reasons to wait, a clock, the Mac's
/// power, a status sink.
pub struct Hooks {
    /// Why reading must wait now, or `None`.
    pub hold: Box<dyn Fn() -> Option<String> + Send + Sync>,
    /// Local time, in minutes after midnight.
    pub minute: Box<dyn Fn() -> u16 + Send + Sync>,
    pub power: Box<dyn Fn() -> Power + Send + Sync>,
    pub emit: Box<dyn Fn(&FilesStatus) + Send + Sync>,
}

/// The Mac's power, for DigUp's power policy: a big catch-up waits in Low
/// Power Mode, while the Mac is warm, and on battery when the person said so.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Power {
    pub low_power_mode: bool,
    /// Thermal state serious or critical.
    pub warm: bool,
    pub on_battery: bool,
}

impl Power {
    /// Why a big catch-up waits, or `None`.
    pub fn blocker(self, index_on_battery: bool) -> Option<&'static str> {
        if self.low_power_mode {
            Some("waiting while Low Power Mode is on")
        } else if self.warm {
            Some("waiting while the Mac is warm")
        } else if self.on_battery && !index_on_battery {
            Some("waiting for power: on battery")
        } else {
            None
        }
    }
}

/// The helper programs, found once at start.
#[derive(Debug, Clone, Default)]
pub struct Programs {
    pub helper: Option<PathBuf>,
    pub reader: Option<PathBuf>,
}

impl Programs {
    pub fn locate() -> Self {
        Self {
            helper: super::helper::locate(),
            reader: super::reader::locate(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FilesState {
    Off,
    NeedsModel,
    Indexing,
    Paused,
    WaitingForHours,
    Held,
    Idle,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct FilesStatus {
    pub state: FilesState,
    /// Why it holds or failed, in a few words. Never a file name.
    pub detail: Option<String>,
    /// Files and code together.
    pub counts: IndexCounts,
    /// Code alone, for its own status line.
    pub code_counts: IndexCounts,
    /// The code index on disk, for "turning code search off deletes N MB".
    pub code_bytes: u64,
    pub model_ready: bool,
    pub helper_found: bool,
    pub reader_found: bool,
}

/// A chosen folder as Settings shows it.
#[derive(Debug, Clone, Serialize)]
pub struct FolderInfo {
    pub folder: String,
    pub reach: super::folders::Reach,
    #[serde(flatten)]
    pub counts: super::store::FolderCounts,
}

#[derive(Debug, Clone, Serialize)]
pub struct FilesSearch {
    pub hits: Vec<FileHit>,
    /// The first `strong` hits are strong; the rest fold under "weaker matches".
    pub strong: usize,
    /// False when the search used words only (no model yet, or it would not start).
    pub meaning: bool,
}

#[derive(Default)]
struct Signal {
    dirty: bool,
    wake: bool,
    stop: bool,
    changed_at: Option<Instant>,
}

/// One index's file and loaded vectors, for searching.
#[derive(Default)]
struct IndexSide {
    store: Option<FilesStore>,
    searcher: Option<(Searcher, u64)>,
}

#[derive(Default)]
struct SearchSide {
    /// The Files index, then the code index.
    indexes: [IndexSide; 2],
    helper: Option<FilesHelper>,
    last_used: Option<Instant>,
}

pub struct FilesService {
    app_data_dir: PathBuf,
    settings: RwLock<FilesSettings>,
    status: RwLock<FilesStatus>,
    signal: Mutex<Signal>,
    wakeup: Condvar,
    /// Bumped whenever the index changes, so search reloads its vectors.
    generation: AtomicU64,
    search_side: Mutex<SearchSide>,
    watcher: Mutex<Option<(Vec<PathBuf>, notify::RecommendedWatcher)>>,
    programs: Programs,
    hooks: Hooks,
}

impl FilesService {
    pub fn index_path(app_data_dir: &Path) -> PathBuf {
        app_data_dir.join("files").join("index.sqlite")
    }

    /// Code has an index of its own and never shows up in other searches.
    pub fn code_index_path(app_data_dir: &Path) -> PathBuf {
        app_data_dir.join("files").join("code.sqlite")
    }

    /// Loads the settings and starts the worker thread.
    pub fn start(app_data_dir: PathBuf, programs: Programs, hooks: Hooks) -> Arc<Self> {
        let settings = FilesSettings::load(&app_data_dir);
        let service = Arc::new(Self {
            status: RwLock::new(FilesStatus {
                state: FilesState::Off,
                detail: None,
                counts: IndexCounts::default(),
                code_counts: IndexCounts::default(),
                code_bytes: 0,
                model_ready: model::missing(&model::dir(&app_data_dir)).is_empty(),
                helper_found: programs.helper.is_some(),
                reader_found: programs.reader.is_some(),
            }),
            app_data_dir,
            settings: RwLock::new(settings),
            signal: Mutex::new(Signal {
                dirty: true,
                wake: true,
                ..Signal::default()
            }),
            wakeup: Condvar::new(),
            generation: AtomicU64::new(0),
            search_side: Mutex::new(SearchSide::default()),
            watcher: Mutex::new(None),
            programs,
            hooks,
        });
        let worker = Arc::clone(&service);
        std::thread::Builder::new()
            .name("files-index".into())
            .spawn(move || worker.run())
            .expect("spawn the Files worker");
        service
    }

    /// The chosen folders: whether each can be read, its files by kind, and how many are read.
    pub fn folders(&self) -> Vec<FolderInfo> {
        let settings = self.settings.read().clone();
        let store = self
            .index_file()
            .exists()
            .then(|| FilesStore::open(&self.index_file()).ok())
            .flatten();
        settings
            .folders
            .iter()
            .map(|folder| FolderInfo {
                folder: folder.clone(),
                reach: super::folders::reach(Path::new(folder)),
                counts: store
                    .as_ref()
                    .and_then(|store| store.folder_counts(folder).ok())
                    .unwrap_or_default(),
            })
            .collect()
    }

    pub fn settings(&self) -> FilesSettings {
        self.settings.read().clone()
    }

    /// Saves new settings and has the worker look again. Folders the person
    /// stopped searching leave the index.
    pub fn set_settings(&self, settings: FilesSettings) -> Result<(), String> {
        settings
            .save(&self.app_data_dir)
            .map_err(|error| error.to_string())?;
        let (dropped, dropped_code): (Vec<String>, Vec<String>) = {
            let old = self.settings.read();
            let gone = |old: &[String], new: &[String]| -> Vec<String> {
                old.iter()
                    .filter(|folder| !new.contains(folder))
                    .cloned()
                    .collect()
            };
            (
                gone(&old.folders, &settings.folders),
                gone(&old.code_folders, &settings.code_folders),
            )
        };
        *self.settings.write() = settings;
        for (file, folders) in [
            (self.index_file(), dropped),
            (self.code_index_file(), dropped_code),
        ] {
            if folders.is_empty() || !file.exists() {
                continue;
            }
            let store = FilesStore::open(&file).map_err(|error| error.to_string())?;
            for folder in folders {
                store
                    .remove_folder(&folder)
                    .map_err(|error| error.to_string())?;
            }
            self.generation.fetch_add(1, Ordering::SeqCst);
        }
        self.poke(true);
        Ok(())
    }

    pub fn status(&self) -> FilesStatus {
        self.status.read().clone()
    }

    /// The model files are in place (after a download, say): look again.
    pub fn models_changed(&self) {
        self.poke(false);
    }

    /// Deletes the whole index (the person turned Files search off and asked to forget it).
    pub fn forget(&self) -> Result<(), String> {
        let mut side = self.search_side.lock();
        *side = SearchSide::default();
        for file in [self.index_file(), self.code_index_file()] {
            for suffix in ["", "-wal", "-shm"] {
                let path = PathBuf::from(format!("{}{suffix}", file.display()));
                if path.exists() {
                    std::fs::remove_file(path).map_err(|error| error.to_string())?;
                }
            }
        }
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.poke(true);
        Ok(())
    }

    pub fn stop(&self) {
        self.signal.lock().stop = true;
        self.wakeup.notify_all();
    }

    fn poke(&self, dirty: bool) {
        let mut signal = self.signal.lock();
        signal.wake = true;
        signal.dirty |= dirty;
        self.wakeup.notify_all();
    }

    fn index_file(&self) -> PathBuf {
        Self::index_path(&self.app_data_dir)
    }

    fn code_index_file(&self) -> PathBuf {
        Self::code_index_path(&self.app_data_dir)
    }

    fn model_dir(&self) -> PathBuf {
        model::dir(&self.app_data_dir)
    }

    fn model_ready(&self) -> bool {
        model::missing(&self.model_dir()).is_empty()
    }

    /// Whether `path` is in the Files index (or the code index): what surfaces may open, reveal, copy or preview.
    pub fn is_indexed(&self, path: &str) -> bool {
        [self.index_file(), self.code_index_file()]
            .iter()
            .any(|file| {
                file.exists()
                    && FilesStore::open(file)
                        .and_then(|store| store.contains(path))
                        .unwrap_or(false)
            })
    }

    pub fn app_data_dir(&self) -> &Path {
        &self.app_data_dir
    }

    pub fn programs(&self) -> &Programs {
        &self.programs
    }

    // ── Search ──────────────────────────────────────────────────────────────

    /// Files matching `query`, best first. `kinds` narrows to some kinds.
    /// `code: ...` searches the code index and nothing else; `file: ...` files only.
    /// Blocking: call it from a blocking thread.
    /// `words_only` is the instant search while typing: no model, and the last
    /// word matches as the start of longer ones (it is probably half typed).
    pub fn search(
        &self,
        query: &str,
        kinds: Option<HashSet<FileKind>>,
        limit: usize,
        words_only: bool,
    ) -> Result<FilesSearch, String> {
        let nothing = || FilesSearch {
            hits: Vec::new(),
            strong: 0,
            meaning: false,
        };
        if !self.settings.read().enabled {
            return Ok(nothing());
        }
        let code_text = super::code_query_text(query);
        let code = code_text.is_some();
        let text = code_text
            .or_else(|| super::file_query_text(query))
            .unwrap_or(query)
            .trim();
        let file = if code {
            self.code_index_file()
        } else {
            self.index_file()
        };
        if text.is_empty() || !file.exists() {
            return Ok(nothing());
        }
        let mut side = self.search_side.lock();
        side.last_used = Some(Instant::now());
        let generation = self.generation.load(Ordering::SeqCst);
        {
            let index = &mut side.indexes[usize::from(code)];
            if index.store.is_none() {
                index.store = Some(FilesStore::open(&file).map_err(|error| error.to_string())?);
            }
            if index
                .searcher
                .as_ref()
                .is_none_or(|(_, seen)| *seen != generation)
            {
                let store = index.store.as_ref().expect("opened above");
                index.searcher = Some((
                    Searcher::load(store).map_err(|error| error.to_string())?,
                    generation,
                ));
            }
        }
        let vector = if words_only {
            None
        } else {
            self.query_vector(&mut side, text, code)
        };
        let options = SearchOptions {
            kinds,
            limit,
            prefix_last: words_only,
            code,
            word_weight: if code {
                searcher::CODE_WORD_WEIGHT
            } else {
                searcher::WORD_WEIGHT
            },
            ..SearchOptions::default()
        };
        let index = &side.indexes[usize::from(code)];
        let (searcher, _) = index.searcher.as_ref().expect("loaded above");
        let store = index.store.as_ref().expect("opened above");
        let hits = searcher
            .search(store, text, vector.as_deref(), &options)
            .map_err(|error| error.to_string())?;
        let strong = searcher::strong_count(&hits, 0.5);
        Ok(FilesSearch {
            hits,
            strong,
            meaning: vector.is_some(),
        })
    }

    /// The query's vector from a text-only helper, started on first use. `None`
    /// (words only) when the model or the helper is missing or fails.
    fn query_vector(&self, side: &mut SearchSide, text: &str, code: bool) -> Option<Vec<f32>> {
        if side.helper.is_none() {
            let program = self.programs.helper.as_ref()?;
            let text_model = self.model_dir().join(model::TEXT.name);
            if !text_model.is_file() {
                return None;
            }
            match FilesHelper::start(program, &text_model, None) {
                Ok(helper) => side.helper = Some(helper),
                Err(error) => {
                    tracing::warn!("Files search runs on words only: {error}");
                    return None;
                }
            }
        }
        match side.helper.as_mut()?.embed_query(text, code) {
            Ok(vector) => Some(vector),
            Err(error) => {
                tracing::warn!("Files search query embedding failed: {error}");
                side.helper = None; // started again next time
                None
            }
        }
    }

    fn release_idle_search(&self) {
        let mut side = self.search_side.lock();
        if side
            .last_used
            .is_some_and(|used| used.elapsed() > SEARCH_IDLE)
        {
            *side = SearchSide::default();
        }
    }

    // ── The worker ──────────────────────────────────────────────────────────

    fn run(self: Arc<Self>) {
        let mut tools: Option<(FilesHelper, Option<FilesReader>)> = None;
        loop {
            {
                let mut signal = self.signal.lock();
                if !signal.wake && !signal.stop {
                    self.wakeup.wait_for(&mut signal, TICK);
                }
                if signal.stop {
                    return;
                }
                // Let a burst of changes settle before syncing.
                if signal.changed_at.is_some_and(|at| at.elapsed() < QUIET) {
                    signal.wake = false;
                    drop(signal);
                    std::thread::sleep(QUIET);
                    continue;
                }
                signal.wake = false;
            }
            self.release_idle_search();
            match self.pass(&mut tools) {
                Ok(more) => {
                    if more {
                        self.signal.lock().wake = true;
                    } else {
                        tools = None; // the model quits when the work is done
                    }
                }
                Err(error) => {
                    tools = None;
                    self.set_status(FilesState::Error, Some(error));
                }
            }
        }
    }

    /// One look: sync if needed, then read one batch. `Ok(true)` when more work remains now.
    fn pass(
        self: &Arc<Self>,
        tools: &mut Option<(FilesHelper, Option<FilesReader>)>,
    ) -> Result<bool, String> {
        let settings = self.settings.read().clone();
        if !settings.enabled {
            *self.watcher.lock() = None;
            self.set_status(FilesState::Off, None);
            return Ok(false);
        }
        let code_roots: Vec<PathBuf> = settings.code_folders.iter().map(PathBuf::from).collect();
        self.watch(&[settings.roots(), code_roots.clone()].concat());
        let files_store =
            FilesStore::open(&self.index_file()).map_err(|error| error.to_string())?;
        let code_store =
            FilesStore::open(&self.code_index_file()).map_err(|error| error.to_string())?;
        let options = settings.index_options();
        let code_options = super::IndexOptions {
            code: true,
            ..settings.index_options()
        };
        if std::mem::take(&mut self.signal.lock().dirty) {
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default();
            indexer::sync(&files_store, &settings.roots(), &options, &home)
                .map_err(|error| error.to_string())?;
            indexer::sync_code(&code_store, &code_roots, &code_options, &home)
                .map_err(|error| error.to_string())?;
            self.generation.fetch_add(1, Ordering::SeqCst);
        }
        // Ordinary files first; code after them (it is text only, and quick).
        let files_counts = files_store.counts().map_err(|error| error.to_string())?;
        let files_partial = files_store
            .partial(1)
            .map_err(|error| error.to_string())?
            .len();
        let (store, options, counts, partial) = if files_counts.pending > 0 || files_partial > 0 {
            (&files_store, &options, files_counts, files_partial)
        } else {
            let code_counts = code_store.counts().map_err(|error| error.to_string())?;
            (&code_store, &code_options, code_counts, 0)
        };
        if counts.pending == 0 && partial == 0 {
            self.set_status(FilesState::Idle, None);
            return Ok(false);
        }
        if settings.paused {
            self.set_status(FilesState::Paused, None);
            return Ok(false);
        }
        if !self.model_ready() {
            self.set_status(FilesState::NeedsModel, None);
            return Ok(false);
        }
        if counts.pending > BIG_BACKLOG {
            if !settings.hours.allows((self.hooks.minute)()) {
                let from = settings
                    .hours
                    .resumes_at()
                    .map(|time| format!("from {time}"));
                self.set_status(FilesState::WaitingForHours, from);
                return Ok(false);
            }
            if let Some(why) = (self.hooks.power)().blocker(settings.index_on_battery) {
                self.set_status(FilesState::Held, Some(why.to_string()));
                return Ok(false);
            }
        }
        if let Some(why) = (self.hooks.hold)() {
            self.set_status(FilesState::Held, Some(why));
            return Ok(false);
        }
        let Some(helper_program) = self.programs.helper.as_ref() else {
            self.set_status(
                FilesState::Error,
                Some("the Files helper is not installed".into()),
            );
            return Ok(false);
        };
        if tools.is_none() {
            let dir = self.model_dir();
            let helper = FilesHelper::start(
                helper_program,
                &dir.join(model::TEXT.name),
                Some(&dir.join(model::MEDIA.name)),
            )?;
            let reader = match self
                .programs
                .reader
                .as_ref()
                .map(|program| FilesReader::start(program))
            {
                Some(Ok(reader)) => Some(reader),
                Some(Err(error)) => {
                    tracing::warn!("Files search reads documents only: {error}");
                    None
                }
                None => None,
            };
            *tools = Some((helper, reader));
        }
        self.set_status(FilesState::Indexing, None);
        let (helper, reader) = tools.as_mut().expect("started above");
        let scratch = self.app_data_dir.join("files").join("scratch");
        let mut parts = Tools {
            embedder: helper,
            reader: reader
                .as_mut()
                .map(|reader| reader as &mut dyn super::reader::Reader),
            scratch: &scratch,
        };
        let run: IndexRun = if counts.pending > 0 {
            indexer::index_pending(store, &mut parts, options, BATCH)
        } else {
            indexer::continue_long_files(store, &mut parts, options, 1)
        }
        .map_err(|error| error.to_string())?;
        self.generation.fetch_add(1, Ordering::SeqCst);
        // Kinds no reader here can open stay queued; when that is all that is left, stop.
        let progressed = run.done + run.partial + run.skipped + run.failed > 0;
        if !progressed {
            self.set_status(
                FilesState::Idle,
                Some("some files need macOS's readers".into()),
            );
        }
        Ok(progressed)
    }

    /// Watches the chosen folders; a change where the walk goes marks the index dirty.
    fn watch(self: &Arc<Self>, roots: &[PathBuf]) {
        use notify::{RecursiveMode, Watcher};
        let mut current = self.watcher.lock();
        if current
            .as_ref()
            .is_some_and(|(watched, _)| watched == roots)
        {
            return;
        }
        *current = None;
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let excluded = self.settings.read().excluded_folders.clone();
        let watched_roots = roots.to_vec();
        // The watcher holds the service weakly, so dropping the service stops it.
        let service = Arc::downgrade(self);
        let mark_dirty = move || {
            let Some(service) = service.upgrade() else {
                return;
            };
            let mut signal = service.signal.lock();
            signal.dirty = true;
            signal.wake = true;
            signal.changed_at = Some(Instant::now());
            service.wakeup.notify_all();
        };
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else { return };
            let relevant = event.paths.iter().any(|path| {
                watched_roots
                    .iter()
                    .any(|root| rules::is_walked(path, root, &home, &excluded))
            });
            if relevant {
                mark_dirty();
            }
        });
        let Ok(mut watcher) = watcher else { return };
        for root in roots {
            let _ = watcher.watch(root, RecursiveMode::Recursive);
        }
        *current = Some((roots.to_vec(), watcher));
    }

    fn set_status(&self, state: FilesState, detail: Option<String>) {
        let read = |file: PathBuf| -> IndexCounts {
            if !file.exists() {
                return IndexCounts::default();
            }
            FilesStore::open(&file)
                .and_then(|store| store.counts())
                .unwrap_or_default()
        };
        let (files, code) = (read(self.index_file()), read(self.code_index_file()));
        let counts = IndexCounts {
            files: files.files + code.files,
            done: files.done + code.done,
            pending: files.pending + code.pending,
            failed: files.failed + code.failed,
            vectors: files.vectors + code.vectors,
        };
        let code_bytes = ["", "-wal"]
            .iter()
            .filter_map(|suffix| {
                std::fs::metadata(format!("{}{suffix}", self.code_index_file().display())).ok()
            })
            .map(|meta| meta.len())
            .sum();
        let status = FilesStatus {
            state,
            detail,
            counts,
            code_counts: code,
            code_bytes,
            model_ready: self.model_ready(),
            helper_found: self.programs.helper.is_some(),
            reader_found: self.programs.reader.is_some(),
        };
        let changed = {
            let mut current = self.status.write();
            let changed = current.state != status.state
                || current.counts != status.counts
                || current.detail != status.detail;
            *current = status.clone();
            changed
        };
        if changed {
            (self.hooks.emit)(&status);
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::process::tests::fake;
    use super::super::settings::{FilesSettings, IndexHours};
    use super::*;

    const HELPER_READY: &str = r#"{"ready": True, "dimension": 768, "vector_version": 1, "llama": "b11461", "media": True, "load_seconds": 0.1}"#;
    /// Every text and picture becomes the same direction: meaning cannot
    /// separate files here, so these tests exercise words and the worker.
    const HELPER_ANSWER: &str =
        r#"{"ok": True, "vectors": [[1.0] + [0.0] * 767 for _ in request.get("texts", [0])]}"#;

    fn service(
        dir: &Path,
        hold: Option<&'static str>,
    ) -> (Arc<FilesService>, Arc<Mutex<Vec<FilesState>>>) {
        service_at(dir, hold, 12 * 60, Power::default())
    }

    fn service_at(
        dir: &Path,
        hold: Option<&'static str>,
        minute: u16,
        power: Power,
    ) -> (Arc<FilesService>, Arc<Mutex<Vec<FilesState>>>) {
        let model_dir = model::dir(dir);
        std::fs::create_dir_all(&model_dir).unwrap();
        for file in model::ALL {
            std::fs::write(model_dir.join(file.name), b"stand-in").unwrap();
        }
        let programs = Programs {
            helper: Some(fake(dir, "helper", HELPER_READY, HELPER_ANSWER)),
            reader: None,
        };
        let seen = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&seen);
        let hooks = Hooks {
            hold: Box::new(move || hold.map(str::to_string)),
            minute: Box::new(move || minute),
            power: Box::new(move || power),
            emit: Box::new(move |status| log.lock().push(status.state)),
        };
        (
            FilesService::start(dir.to_path_buf(), programs, hooks),
            seen,
        )
    }

    fn wait_for(service: &FilesService, state: FilesState) {
        let started = Instant::now();
        while service.status().state != state {
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "stuck at {:?}",
                service.status()
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn documents(dir: &Path) -> PathBuf {
        let folder = dir.join("Documents");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("invoice.md"),
            "Invoice K7Q2LM for the studio rent in March.",
        )
        .unwrap();
        std::fs::write(folder.join("notes.md"), "Planning notes for the offsite.").unwrap();
        folder
    }

    #[test]
    fn off_until_turned_on_then_indexes_and_searches() {
        let dir = tempfile::tempdir().unwrap();
        let folder = documents(dir.path());
        let (service, _) = service(dir.path(), None);
        wait_for(&service, FilesState::Off);
        assert!(service
            .search("invoice", None, 10, false)
            .unwrap()
            .hits
            .is_empty());

        service
            .set_settings(FilesSettings {
                enabled: true,
                folders: vec![folder.to_string_lossy().into()],
                ..FilesSettings::default()
            })
            .unwrap();
        wait_for(&service, FilesState::Idle);
        assert_eq!(service.status().counts.done, 2);

        let found = service.search("K7Q2LM", None, 10, false).unwrap();
        assert!(found.meaning, "the query went through the helper");
        assert!(found.hits[0].path.ends_with("invoice.md"));
        let found = service.search("file: invoi", None, 10, true).unwrap();
        assert!(!found.meaning, "words only never asks the model");
        assert!(
            found.hits[0].path.ends_with("invoice.md"),
            "the prefix narrows, the last word is a prefix"
        );

        // A new file is picked up by the watcher or the next look.
        std::fs::write(folder.join("lease.md"), "The lease allows one cat.").unwrap();
        service.poke(true);
        let started = Instant::now();
        while service
            .search("lease cat", None, 10, false)
            .unwrap()
            .hits
            .is_empty()
        {
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "the new file never arrived"
            );
            std::thread::sleep(Duration::from_millis(100));
        }

        // Dropping the folder drops its files.
        service
            .set_settings(FilesSettings {
                enabled: true,
                folders: vec![],
                ..FilesSettings::default()
            })
            .unwrap();
        wait_for(&service, FilesState::Idle);
        assert!(service
            .search("invoice", None, 10, false)
            .unwrap()
            .hits
            .is_empty());
        service.stop();
    }

    #[test]
    fn code_folders_are_searched_only_with_code() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("app");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("package.json"), "{}").unwrap();
        std::fs::write(
            repo.join("auth.ts"),
            "export function refreshAccessToken(user) {\n  return user;\n}\n",
        )
        .unwrap();
        let (service, _) = service(dir.path(), None);
        service
            .set_settings(FilesSettings {
                enabled: true,
                code_folders: vec![repo.to_string_lossy().into()],
                ..FilesSettings::default()
            })
            .unwrap();
        wait_for(&service, FilesState::Idle);
        let found = service
            .search("code: refreshAccessToken", None, 10, false)
            .unwrap();
        assert!(found.hits[0].path.ends_with("auth.ts"), "{found:?}");
        assert_eq!(found.hits[0].focus, Some(1.0));
        assert!(
            service
                .search("refreshAccessToken", None, 10, false)
                .unwrap()
                .hits
                .is_empty(),
            "code stays out of other searches"
        );
        assert!(service.is_indexed(&repo.join("auth.ts").to_string_lossy()));
        service.stop();
    }

    #[test]
    fn a_hold_or_a_pause_keeps_files_queued() {
        let dir = tempfile::tempdir().unwrap();
        let folder = documents(dir.path());
        let (service, _) = service(dir.path(), Some("FNDR is paused"));
        service
            .set_settings(FilesSettings {
                enabled: true,
                folders: vec![folder.to_string_lossy().into()],
                ..FilesSettings::default()
            })
            .unwrap();
        wait_for(&service, FilesState::Held);
        let status = service.status();
        assert_eq!(
            (status.counts.pending, status.detail.as_deref()),
            (2, Some("FNDR is paused"))
        );
        service
            .set_settings(FilesSettings {
                enabled: true,
                paused: true,
                folders: vec![folder.to_string_lossy().into()],
                ..FilesSettings::default()
            })
            .unwrap();
        wait_for(&service, FilesState::Paused);
        service.stop();
    }

    #[test]
    fn a_big_catch_up_waits_for_power_and_hours_and_says_until_when() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Notes");
        std::fs::create_dir_all(&folder).unwrap();
        for index in 0..(BIG_BACKLOG + 5) {
            std::fs::write(folder.join(format!("note-{index}.md")), "a note").unwrap();
        }
        let on_battery = Power {
            on_battery: true,
            ..Power::default()
        };
        let settings = FilesSettings {
            enabled: true,
            folders: vec![folder.to_string_lossy().into()],
            index_on_battery: false,
            ..FilesSettings::default()
        };
        let (service, _) = service_at(dir.path(), None, 12 * 60, on_battery);
        service.set_settings(settings.clone()).unwrap();
        wait_for(&service, FilesState::Held);
        assert_eq!(
            service.status().detail.as_deref(),
            Some("waiting for power: on battery")
        );
        service.stop();

        let night = FilesSettings {
            index_on_battery: true,
            hours: IndexHours::Between {
                start: 22 * 60,
                end: 7 * 60,
            },
            ..settings
        };
        let (service, _) = service_at(dir.path(), None, 12 * 60, on_battery);
        service.set_settings(night).unwrap();
        wait_for(&service, FilesState::WaitingForHours);
        assert_eq!(service.status().detail.as_deref(), Some("from 22:00"));
        service.stop();

        assert_eq!(
            Power {
                low_power_mode: true,
                ..Power::default()
            }
            .blocker(true),
            Some("waiting while Low Power Mode is on")
        );
        assert_eq!(
            on_battery.blocker(true),
            None,
            "on battery is fine by default"
        );
    }

    #[test]
    fn without_the_model_it_asks_for_it_and_searches_words_only() {
        let dir = tempfile::tempdir().unwrap();
        let folder = documents(dir.path());
        let (service, _) = service(dir.path(), None);
        std::fs::remove_file(model::dir(dir.path()).join(model::MEDIA.name)).unwrap();
        service
            .set_settings(FilesSettings {
                enabled: true,
                folders: vec![folder.to_string_lossy().into()],
                ..FilesSettings::default()
            })
            .unwrap();
        wait_for(&service, FilesState::NeedsModel);
        service.stop();
    }
}
