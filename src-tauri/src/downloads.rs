//! Downloads folder watcher.
//!
//! Monitors the user's Downloads folder for new, completed files
//! and injects synthetic memory records so they become searchable.

use crate::config::DEFAULT_IMAGE_EMBEDDING_DIM;
use crate::embedding::{Embedder, EMBEDDING_DIM};
use crate::memory::reopen::{ReopenKind, ReopenValidationStatus};
use crate::memory_compaction::{
    build_lexical_shadow, compact_summary_embedding_text, mean_pool_embeddings,
    support_embedding_texts_with_config,
};
use crate::storage::MemoryRecord;
use crate::AppState;
use chrono::{DateTime, Local};
use notify::{
    event::{ModifyKind, RenameMode},
    EventKind, RecursiveMode, Watcher,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

const SIZE_STABLE: Duration = Duration::from_secs(3);
const LINK_DELAY_BUFFER_SECS: u64 = 15;
const SOURCE_MEMORY_WINDOW_MS: i64 = 120_000;
const INGESTED_CAP: usize = 1000;
const SETTLER_TTL: Duration = Duration::from_secs(60 * 60);
const PARTIAL_SIBLING_EXTS: [&str; 3] = ["crdownload", "download", "part"];
const WHERE_FROMS_XATTR: &str = "com.apple.metadata:kMDItemWhereFroms";

/// Run the async background loop watching the Downloads folder.
pub async fn run_watcher(state: Arc<AppState>) {
    let download_dir = match dirs::download_dir() {
        Some(d) => d,
        None => {
            tracing::warn!("Could not find user Downloads directory; tracker disabled.");
            return;
        }
    };

    run_watch_loop(state, download_dir).await;
}

async fn run_watch_loop(state: Arc<AppState>, watch_path: PathBuf) {
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    let watcher_res = notify::recommended_watcher(move |res| {
        if let Ok(event) = res {
            let _ = tx.blocking_send(event);
        }
    });

    let mut watcher = match watcher_res {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("Failed to create watcher: {}", e);
            return;
        }
    };

    if let Err(e) = watcher.watch(&watch_path, RecursiveMode::NonRecursive) {
        tracing::error!("Failed to watch downloads dir: {}", e);
        return;
    }

    tracing::info!("Downloads tracker watching: {}", watch_path.display());

    let chunking_config = state.config.read().chunking.clone();
    let mut text_embedder = match Embedder::with_chunking_config(&chunking_config) {
        Ok(e) => Some(e),
        Err(e) => {
            tracing::warn!("Failed to initialize embedder for downloads tracker: {}", e);
            None
        }
    };

    let link_delay = Duration::from_secs(
        state
            .config
            .read()
            .capture_pipeline
            .flush_interval_secs
            .saturating_add(LINK_DELAY_BUFFER_SECS),
    );
    let mut settler = DownloadSettler::new(link_delay);
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            maybe_event = rx.recv() => {
                let Some(event) = maybe_event else {
                    break;
                };
                observe_download_event(&mut settler, event);
            }
            _ = ticker.tick() => {
                for path in settler.ready(Instant::now()) {
                    if let Some(filename) = path.file_name().and_then(|f| f.to_str()) {
                        tracing::info!("Download detected: {}", filename);
                        inject_download_memory(&state, &mut text_embedder, &path, filename).await;
                    }
                }
            }
        }
    }
}

fn observe_download_event(settler: &mut DownloadSettler, event: notify::Event) {
    let is_interesting = match event.kind {
        EventKind::Create(_) => true,
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => true,
        EventKind::Modify(ModifyKind::Name(RenameMode::Any)) => true,
        EventKind::Modify(ModifyKind::Data(_)) => true,
        EventKind::Modify(ModifyKind::Any) => true,
        _ => false,
    };
    if !is_interesting {
        return;
    }

    let now = Instant::now();
    for path in event.paths {
        if is_temp_file(&path) {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let size = meta.len();
        if size == 0 {
            continue;
        }
        settler.observe(path, size, now);
    }
}

fn is_temp_file(path: &Path) -> bool {
    let Some(filename) = path.file_name().and_then(|f| f.to_str()) else {
        return true;
    };

    if filename.starts_with('.') {
        return true; // .DS_Store, .crdownload, etc
    }

    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false; // No extension is fine, might be a binary or some app file
    };

    let ext = ext.to_lowercase();
    matches!(
        ext.as_str(),
        "crdownload" | "download" | "part" | "tmp" | "temp"
    )
}

struct PendingDownload {
    size: u64,
    size_changed_at: Instant,
    first_seen: Instant,
}

struct DownloadSettler {
    pending: HashMap<PathBuf, PendingDownload>,
    ingested: HashMap<PathBuf, Instant>,
    link_delay: Duration,
}

impl DownloadSettler {
    fn new(link_delay: Duration) -> Self {
        Self {
            pending: HashMap::new(),
            ingested: HashMap::new(),
            link_delay,
        }
    }

    fn observe(&mut self, path: PathBuf, size: u64, now: Instant) {
        if size == 0 || is_temp_file(&path) || self.ingested.contains_key(&path) {
            return;
        }
        match self.pending.get_mut(&path) {
            Some(pending) => {
                if pending.size != size {
                    pending.size = size;
                    pending.size_changed_at = now;
                }
            }
            None => {
                self.pending.insert(
                    path,
                    PendingDownload {
                        size,
                        size_changed_at: now,
                        first_seen: now,
                    },
                );
            }
        }
    }

    fn ready(&mut self, now: Instant) -> Vec<PathBuf> {
        self.prune(now);
        let pending = std::mem::take(&mut self.pending);
        let mut still = HashMap::new();
        let mut done = Vec::new();
        for (path, mut pending) in pending {
            if self.ingested.contains_key(&path) {
                continue;
            }
            match std::fs::metadata(&path) {
                Ok(meta) if meta.is_file() => {
                    let size = meta.len();
                    if size != pending.size {
                        pending.size = size;
                        pending.size_changed_at = now;
                    }
                }
                _ => continue,
            }
            let stable = now.duration_since(pending.size_changed_at) >= SIZE_STABLE;
            let delayed = now.duration_since(pending.first_seen) >= self.link_delay;
            if pending.size > 0 && stable && delayed && !has_partial_sibling(&path) {
                self.ingested.insert(path.clone(), now);
                done.push(path);
            } else {
                still.insert(path, pending);
            }
        }
        self.pending = still;
        done
    }

    fn prune(&mut self, now: Instant) {
        self.pending
            .retain(|_, pending| now.duration_since(pending.first_seen) < SETTLER_TTL);
        if self.ingested.len() > INGESTED_CAP {
            self.ingested
                .retain(|_, seen| now.duration_since(*seen) < SETTLER_TTL);
        }
        self.ingested
            .retain(|_, seen| now.duration_since(*seen) < SETTLER_TTL);
    }
}

fn has_partial_sibling(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    let Some(parent) = path.parent() else {
        return false;
    };
    PARTIAL_SIBLING_EXTS
        .iter()
        .any(|ext| parent.join(format!("{name}.{ext}")).exists())
}

fn read_where_froms(path: &Path) -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    {
        read_where_froms_macos(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
}

#[cfg(target_os = "macos")]
fn read_where_froms_macos(path: &Path) -> Option<Vec<u8>> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let c_path = CString::new(path.as_os_str().as_bytes()).ok()?;
    let c_name = CString::new(WHERE_FROMS_XATTR).ok()?;
    let size = unsafe {
        libc::getxattr(
            c_path.as_ptr(),
            c_name.as_ptr(),
            std::ptr::null_mut(),
            0,
            0,
            0,
        )
    };
    if size <= 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    let n = unsafe {
        libc::getxattr(
            c_path.as_ptr(),
            c_name.as_ptr(),
            buf.as_mut_ptr() as *mut libc::c_void,
            buf.len(),
            0,
            0,
        )
    };
    if n <= 0 {
        return None;
    }
    buf.truncate(n as usize);
    Some(buf)
}

fn parse_where_froms(bytes: &[u8]) -> Vec<String> {
    plist::from_bytes::<Vec<String>>(bytes).unwrap_or_default()
}

fn download_source_url(where_froms: &[String]) -> Option<String> {
    let page = where_froms
        .get(1)
        .map(|value| value.trim())
        .filter(|value| is_http_url(value));
    if let Some(page) = page {
        return Some(crate::capture::strip_url_credentials(page));
    }
    let download = where_froms
        .first()
        .map(|value| value.trim())
        .filter(|value| is_http_url(value))?;
    Some(crate::capture::strip_url_credentials(
        &drop_query_and_fragment(download),
    ))
}

fn drop_query_and_fragment(url: &str) -> String {
    let without_fragment = url.split('#').next().unwrap_or(url);
    without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment)
        .to_string()
}

fn is_http_url(value: &str) -> bool {
    let lower = value.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

fn url_host(url: &str) -> Option<String> {
    let rest = url.split("://").nth(1)?;
    let hostport = rest.split(['/', '?', '#']).next()?.trim();
    let hostport = hostport.split('@').next_back()?.trim();
    let host = hostport.split(':').next()?.trim();
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

fn pick_source_memory(
    candidates: &[MemoryRecord],
    download_ms: i64,
    source_url: Option<&str>,
) -> Option<String> {
    let source_url = source_url
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let window_start = download_ms.saturating_sub(SOURCE_MEMORY_WINDOW_MS);
    let in_window: Vec<&MemoryRecord> = candidates
        .iter()
        .filter(|memory| {
            crate::capture::macos::is_browser_app(&memory.app_name)
                && memory.timestamp >= window_start
                && memory.timestamp <= download_ms
        })
        .collect();
    if in_window.is_empty() {
        return None;
    }
    if let Some(host) = url_host(source_url) {
        if let Some(matched) = in_window
            .iter()
            .filter(|memory| record_host(memory).as_deref() == Some(host.as_str()))
            .max_by_key(|memory| memory.timestamp)
        {
            return Some(matched.id.clone());
        }
    }
    in_window
        .iter()
        .max_by_key(|memory| memory.timestamp)
        .map(|memory| memory.id.clone())
}

fn record_host(memory: &MemoryRecord) -> Option<String> {
    memory
        .url
        .as_deref()
        .and_then(url_host)
        .or_else(|| memory.reopen_url.as_deref().and_then(url_host))
}

fn download_snippet(filename: &str, source_url: Option<&str>) -> String {
    match source_url.and_then(url_host) {
        Some(host) => format!("Downloaded: {filename} from {host}"),
        None => format!("Downloaded: {filename}"),
    }
}

fn build_download_record(
    file_path: &Path,
    filename: &str,
    source_url: Option<String>,
    related_memory_ids: Vec<String>,
    now: DateTime<Local>,
    embedding: Vec<f32>,
    snippet_embedding: Vec<f32>,
    support_embedding: Vec<f32>,
) -> MemoryRecord {
    let path_display = file_path.display().to_string();
    let text = format!("File downloaded locally to file system Tracker: {path_display}");
    let snippet = download_snippet(filename, source_url.as_deref());
    let lexical_shadow = build_lexical_shadow("Downloads", &snippet, &text, source_url.as_deref());
    MemoryRecord {
        id: uuid::Uuid::new_v4().to_string(),
        timestamp: now.timestamp_millis(),
        day_bucket: now.format("%Y-%m-%d").to_string(),
        app_name: "Finder".to_string(),
        bundle_id: Some("com.apple.finder".to_string()),
        window_title: "Downloads".to_string(),
        session_id: format!("{}-downloads", now.format("%Y%m%d")),
        text: String::new(),
        clean_text: text,
        ocr_confidence: 1.0,
        ocr_block_count: 1,
        snippet,
        summary_source: "tracker".to_string(),
        noise_score: 0.0,
        session_key: "filesystem:downloads".to_string(),
        lexical_shadow,
        embedding,
        image_embedding: vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM],
        screenshot_path: None,
        url: source_url,
        snippet_embedding,
        support_embedding,
        decay_score: 1.0,
        last_accessed_at: now.timestamp_millis(),
        reopen_kind: ReopenKind::FilePath,
        reopen_file_path: Some(path_display),
        reopen_captured_at_ms: now.timestamp_millis(),
        reopen_confidence: 0.9,
        reopen_validation_status: ReopenValidationStatus::Valid,
        related_memory_ids,
        ..Default::default()
    }
}

async fn inject_download_memory(
    state: &Arc<AppState>,
    embedder: &mut Option<Embedder>,
    file_path: &Path,
    filename: &str,
) {
    let now = Local::now();
    let source_url = read_where_froms(file_path)
        .as_deref()
        .map(parse_where_froms)
        .as_deref()
        .and_then(download_source_url);
    let related_memory_ids = match source_url.as_deref() {
        Some(url) => {
            let download_ms = now.timestamp_millis();
            let start_ms = download_ms.saturating_sub(SOURCE_MEMORY_WINDOW_MS);
            match state
                .store
                .get_memories_in_range(start_ms, download_ms)
                .await
            {
                Ok(candidates) => pick_source_memory(&candidates, download_ms, Some(url))
                    .into_iter()
                    .collect(),
                Err(err) => {
                    tracing::warn!("Failed to look up download source memory: {}", err);
                    Vec::new()
                }
            }
        }
        None => Vec::new(),
    };

    let snippet = download_snippet(filename, source_url.as_deref());
    let text = format!(
        "File downloaded locally to file system Tracker: {}",
        file_path.display()
    );
    let lexical_shadow = build_lexical_shadow("Downloads", &snippet, &text, source_url.as_deref());
    let compact_summary_text =
        compact_summary_embedding_text("tracker", &snippet, &text, &lexical_shadow);
    let chunking_config = state.config.read().chunking.clone();
    let support_texts = support_embedding_texts_with_config(
        "Finder",
        "Downloads",
        &text,
        &lexical_shadow,
        Some(&chunking_config),
    );

    let (embedding, snippet_embedding, support_embedding) = if let Some(emb) = embedder {
        let mut contexts = vec![
            ("Finder".to_string(), "Downloads".to_string(), text.clone()),
            (
                "Finder".to_string(),
                "Downloads".to_string(),
                compact_summary_text,
            ),
        ];
        contexts.extend(
            support_texts
                .iter()
                .cloned()
                .map(|value| ("Finder".to_string(), "Downloads".to_string(), value)),
        );
        match emb.embed_batch_with_context(&contexts) {
            Ok(vectors) => {
                let text_vec = vectors
                    .first()
                    .cloned()
                    .unwrap_or_else(|| vec![0.0; EMBEDDING_DIM]);
                let snippet_vec = vectors
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| vec![0.0; EMBEDDING_DIM]);
                let support_vec = if vectors.len() > 2 {
                    mean_pool_embeddings(&vectors[2..])
                } else {
                    vec![0.0; EMBEDDING_DIM]
                };
                (text_vec, snippet_vec, support_vec)
            }
            Err(_) => (
                vec![0.0; EMBEDDING_DIM],
                vec![0.0; EMBEDDING_DIM],
                vec![0.0; EMBEDDING_DIM],
            ),
        }
    } else {
        (
            vec![0.0; EMBEDDING_DIM],
            vec![0.0; EMBEDDING_DIM],
            vec![0.0; EMBEDDING_DIM],
        )
    };

    let record = build_download_record(
        file_path,
        filename,
        source_url,
        related_memory_ids,
        now,
        embedding,
        snippet_embedding,
        support_embedding,
    );

    if state.store.add_batch(&[record.clone()]).await.is_err() {
        tracing::error!("Failed to store download memory");
    } else if let Err(err) =
        crate::context_runtime::sync_memory_record(state.as_ref(), &record, Some("file")).await
    {
        tracing::warn!(
            "Failed to sync download memory into context runtime: {}",
            err
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn settler() -> DownloadSettler {
        DownloadSettler::new(Duration::from_secs(10))
    }

    fn write_file(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn sorted_ready(settler: &mut DownloadSettler, now: Instant) -> Vec<PathBuf> {
        let mut paths = settler.ready(now);
        paths.sort();
        paths
    }

    fn browser_memory(id: &str, app: &str, timestamp: i64, url: Option<&str>) -> MemoryRecord {
        MemoryRecord {
            id: id.to_string(),
            app_name: app.to_string(),
            timestamp,
            url: url.map(str::to_string),
            reopen_url: url.map(str::to_string),
            ..Default::default()
        }
    }

    fn where_froms_plist(urls: &[&str]) -> Vec<u8> {
        let owned: Vec<String> = urls.iter().map(|url| (*url).to_string()).collect();
        let mut buf = Vec::new();
        plist::to_writer_binary(&mut buf, &owned).expect("plist");
        buf
    }

    fn empty_embeddings() -> (Vec<f32>, Vec<f32>, Vec<f32>) {
        (
            vec![0.0; EMBEDDING_DIM],
            vec![0.0; EMBEDDING_DIM],
            vec![0.0; EMBEDDING_DIM],
        )
    }

    #[test]
    fn is_temp_file_skips_partial_extensions() {
        assert!(is_temp_file(Path::new("/tmp/file.pdf.crdownload")));
        assert!(is_temp_file(Path::new("/tmp/file.pdf.download")));
        assert!(is_temp_file(Path::new("/tmp/file.pdf.part")));
        assert!(is_temp_file(Path::new("/tmp/.DS_Store")));
        assert!(!is_temp_file(Path::new("/tmp/file.pdf")));
        assert!(!is_temp_file(Path::new("/tmp/report (1).pdf")));
    }

    #[test]
    fn settler_crdownload_then_rename_yields_one_ready_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut settler = settler();
        let t0 = Instant::now();
        let partial = write_file(&dir, "article.pdf.crdownload", b"partial");
        assert!(is_temp_file(&partial));
        settler.observe(partial.clone(), 7, t0);
        assert!(sorted_ready(&mut settler, t0 + Duration::from_secs(60)).is_empty());

        std::fs::remove_file(&partial).unwrap();
        let pdf = write_file(&dir, "article.pdf", b"finished-pdf");
        settler.observe(pdf.clone(), 12, t0);
        assert!(sorted_ready(&mut settler, t0 + Duration::from_secs(2)).is_empty());
        assert!(sorted_ready(&mut settler, t0 + Duration::from_secs(9)).is_empty());
        assert_eq!(
            sorted_ready(&mut settler, t0 + Duration::from_secs(10)),
            vec![pdf.clone()]
        );
        settler.observe(pdf.clone(), 12, t0 + Duration::from_secs(11));
        assert!(sorted_ready(&mut settler, t0 + Duration::from_secs(60)).is_empty());
    }

    #[test]
    fn settler_zero_byte_placeholder_next_to_part_never_becomes_ready() {
        let dir = tempfile::tempdir().unwrap();
        let mut settler = settler();
        let t0 = Instant::now();
        let pdf = write_file(&dir, "article.pdf", b"");
        let _part = write_file(&dir, "article.pdf.part", b"partial");
        settler.observe(pdf, 0, t0);
        assert!(sorted_ready(&mut settler, t0 + Duration::from_secs(60)).is_empty());
    }

    #[test]
    fn settler_growing_file_is_not_ready() {
        let dir = tempfile::tempdir().unwrap();
        let mut settler = settler();
        let t0 = Instant::now();
        let pdf = write_file(&dir, "article.pdf", b"1234567890");
        settler.observe(pdf.clone(), 10, t0);
        std::fs::write(&pdf, b"12345678901234567890").unwrap();
        settler.observe(pdf.clone(), 20, t0 + Duration::from_secs(9));
        assert!(sorted_ready(&mut settler, t0 + Duration::from_secs(10)).is_empty());
        assert_eq!(
            sorted_ready(&mut settler, t0 + Duration::from_secs(12)),
            vec![pdf]
        );
    }

    #[test]
    fn settler_partial_sibling_blocks_until_it_disappears() {
        let dir = tempfile::tempdir().unwrap();
        let mut settler = settler();
        let t0 = Instant::now();
        let pdf = write_file(&dir, "article.pdf", b"finished");
        let crdownload = write_file(&dir, "article.pdf.crdownload", b"partial");
        settler.observe(pdf.clone(), 8, t0);
        assert!(sorted_ready(&mut settler, t0 + Duration::from_secs(10)).is_empty());
        std::fs::remove_file(crdownload).unwrap();
        assert_eq!(
            sorted_ready(&mut settler, t0 + Duration::from_secs(10)),
            vec![pdf]
        );
    }

    #[test]
    fn settler_duplicate_names_are_separate_paths() {
        let dir = tempfile::tempdir().unwrap();
        let mut settler = settler();
        let t0 = Instant::now();
        let first = write_file(&dir, "report.pdf", b"one");
        let second = write_file(&dir, "report (1).pdf", b"two");
        settler.observe(first.clone(), 3, t0);
        settler.observe(second.clone(), 3, t0);
        let mut ready = sorted_ready(&mut settler, t0 + Duration::from_secs(10));
        ready.sort();
        let mut expected = vec![first, second];
        expected.sort();
        assert_eq!(ready, expected);
    }

    #[test]
    fn parse_where_froms_empty_or_garbage_is_empty() {
        assert!(parse_where_froms(b"").is_empty());
        assert!(parse_where_froms(b"not-a-plist").is_empty());
    }

    #[test]
    fn download_source_url_prefers_referring_page() {
        let bytes = where_froms_plist(&[
            "https://cdn.example/file.pdf?X-Amz-Signature=abc&Expires=1",
            "https://example.com/article?utm=1",
        ]);
        let urls = parse_where_froms(&bytes);
        assert_eq!(
            download_source_url(&urls).as_deref(),
            Some("https://example.com/article?utm=1")
        );
    }

    #[test]
    fn download_source_url_drops_query_when_only_download_url_exists() {
        let urls =
            vec!["https://cdn.example/file.pdf?X-Amz-Signature=abc&Expires=1#frag".to_string()];
        assert_eq!(
            download_source_url(&urls).as_deref(),
            Some("https://cdn.example/file.pdf")
        );
    }

    #[test]
    fn download_source_url_strips_credentials() {
        let page = vec![
            "https://cdn.example/file.pdf".to_string(),
            "https://user:pass@example.com/page?token=secret&q=ok".to_string(),
        ];
        assert_eq!(
            download_source_url(&page).as_deref(),
            Some("https://example.com/page?q=ok")
        );
        let download_only = vec!["https://user:pass@cdn.example/file.pdf?token=secret".to_string()];
        assert_eq!(
            download_source_url(&download_only).as_deref(),
            Some("https://cdn.example/file.pdf")
        );
    }

    #[test]
    fn download_source_url_rejects_non_http() {
        assert_eq!(
            download_source_url(&["javascript:alert(1)".to_string()]),
            None
        );
        assert_eq!(
            download_source_url(&["data:text/html,hi".to_string()]),
            None
        );
        assert_eq!(download_source_url(&[]), None);
    }

    #[test]
    fn pick_source_memory_prefers_matching_host_over_newer_other_browser() {
        let download_ms = 200_000;
        let other = browser_memory(
            "newer-other",
            "Safari",
            190_000,
            Some("https://other.example/page"),
        );
        let matched = browser_memory(
            "matched",
            "Google Chrome",
            150_000,
            Some("https://example.com/article"),
        );
        let picked = pick_source_memory(
            &[other, matched],
            download_ms,
            Some("https://example.com/file.pdf"),
        );
        assert_eq!(picked.as_deref(), Some("matched"));
    }

    #[test]
    fn pick_source_memory_ignores_browser_outside_two_minutes() {
        let download_ms = 200_000;
        let old = browser_memory(
            "too-old",
            "Google Chrome",
            download_ms - 121_000,
            Some("https://example.com/article"),
        );
        assert_eq!(
            pick_source_memory(&[old], download_ms, Some("https://example.com/article")),
            None
        );
    }

    #[test]
    fn pick_source_memory_ignores_non_browser_apps() {
        let download_ms = 50_000;
        let finder = MemoryRecord {
            id: "finder".into(),
            app_name: "Finder".into(),
            timestamp: 40_000,
            url: Some("https://example.com/article".into()),
            ..Default::default()
        };
        let vscode = MemoryRecord {
            id: "code".into(),
            app_name: "Code".into(),
            timestamp: 41_000,
            url: Some("https://example.com/article".into()),
            ..Default::default()
        };
        assert_eq!(
            pick_source_memory(
                &[finder, vscode],
                download_ms,
                Some("https://example.com/article")
            ),
            None
        );
    }

    #[test]
    fn pick_source_memory_without_source_url_is_none() {
        let chrome = browser_memory(
            "chrome",
            "Google Chrome",
            10_000,
            Some("https://example.com/article"),
        );
        assert_eq!(pick_source_memory(&[chrome], 20_000, None), None);
    }

    #[test]
    fn pick_source_memory_falls_back_to_newest_browser() {
        let download_ms = 80_000;
        let older = browser_memory("older", "Safari", 20_000, Some("https://a.example/one"));
        let newer = browser_memory(
            "newer",
            "Google Chrome",
            70_000,
            Some("https://b.example/two"),
        );
        assert_eq!(
            pick_source_memory(&[older, newer], download_ms, Some("https://c.example/file")),
            Some("newer".into())
        );
    }

    #[test]
    fn build_download_record_stays_file_path_after_normalize() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_file(&dir, "report.pdf", b"%PDF");
        let (embedding, snippet_embedding, support_embedding) = empty_embeddings();
        let record = build_download_record(
            &path,
            "report.pdf",
            Some("https://en.wikipedia.org/wiki/Nitrogen".into()),
            vec!["page-memory".into()],
            Local::now(),
            embedding,
            snippet_embedding,
            support_embedding,
        );
        assert_eq!(record.reopen_kind, ReopenKind::FilePath);
        assert_eq!(
            record.reopen_file_path.as_deref(),
            Some(path.to_str().unwrap())
        );
        assert_eq!(
            record.url.as_deref(),
            Some("https://en.wikipedia.org/wiki/Nitrogen")
        );
        assert_eq!(record.related_memory_ids, vec!["page-memory".to_string()]);
        assert!(record.snippet.contains("report.pdf"));
        assert!(record.snippet.contains("en.wikipedia.org"));

        let normalized = crate::storage::normalize_record_for_index(&record);
        assert_eq!(normalized.reopen_kind, ReopenKind::FilePath);
        assert_eq!(normalized.reopen_file_path, record.reopen_file_path);
        assert_eq!(
            normalized.url.as_deref(),
            Some("https://en.wikipedia.org/wiki/Nitrogen")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn read_where_froms_roundtrip_on_temp_file() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let dir = tempfile::tempdir().unwrap();
        let path = write_file(&dir, "roundtrip.pdf", b"%PDF");
        let bytes = where_froms_plist(&[
            "https://cdn.example/file.pdf?X-Amz-Signature=abc",
            "https://example.com/article",
        ]);
        let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
        let c_name = CString::new(WHERE_FROMS_XATTR).unwrap();
        let rc = unsafe {
            libc::setxattr(
                c_path.as_ptr(),
                c_name.as_ptr(),
                bytes.as_ptr() as *const libc::c_void,
                bytes.len(),
                0,
                0,
            )
        };
        assert_eq!(rc, 0, "setxattr failed");
        let read = read_where_froms(&path).expect("xattr present");
        assert_eq!(
            download_source_url(&parse_where_froms(&read)).as_deref(),
            Some("https://example.com/article")
        );
    }
}
