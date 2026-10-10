//! What changed in a Resume Work thread since the person last viewed it.
//! The last-seen markers live in the app state store.

use crate::resume::{admits, thread_key, thread_title};
use crate::storage::{MemoryRecord, StateStore, Store, Task};
use crate::AppState;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

const SEEN_STATE_KEY: &str = "resume_thread_seen_v1";
static SEEN_MUTATION: Mutex<()> = Mutex::new(());
/// Markers kept; the oldest go first.
pub const MAX_MARKERS: usize = 500;
/// A thread never viewed is compared with the last day.
pub const FIRST_VIEW_WINDOW_MS: i64 = 24 * 3_600_000;
/// An older marker is read as a week old, to bound the scan.
pub const MAX_LOOKBACK_MS: i64 = 7 * 24 * 3_600_000;
const MAX_LISTED: usize = 5;
/// Files, tasks and commits together that make a background nudge worth it.
const NUDGE_MIN_CHANGES: usize = 3;
/// A nudge waits until the thread was last viewed at least this long ago.
const NUDGE_MIN_AGE_MS: i64 = 2 * 3_600_000;
/// Seen threads the background check compares, most recently viewed first.
const NUDGE_MAX_THREADS: usize = 20;

/// When each thread was last viewed, by thread key, in epoch milliseconds.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SeenMarkers(pub HashMap<String, i64>);

impl SeenMarkers {
    pub fn mark(&mut self, key: &str, at_ms: i64) {
        self.0.insert(key.to_string(), at_ms);
        while self.0.len() > MAX_MARKERS {
            let Some(oldest) = self
                .0
                .iter()
                .min_by_key(|(key, at)| (**at, (*key).clone()))
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.0.remove(&oldest);
        }
    }
}

pub fn seen_markers(store: &StateStore) -> Result<SeenMarkers, String> {
    store
        .load_json::<SeenMarkers>(SEEN_STATE_KEY)
        .map(Option::unwrap_or_default)
}

pub fn last_seen(store: &StateStore, key: &str) -> Result<Option<i64>, String> {
    Ok(seen_markers(store)?.0.get(key).copied())
}

pub fn mark_seen(store: &StateStore, key: &str, now_ms: i64) -> Result<(), String> {
    let _lock = SEEN_MUTATION.lock();
    let mut markers = seen_markers(store)?;
    markers.mark(key, now_ms);
    store.save_json(SEEN_STATE_KEY, &markers)
}

/// What is new in a thread since a moment: counts, and up to five names of
/// each kind, newest first. Names come from window titles, file names and
/// task titles, never from a model's summary.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ThreadDigest {
    pub thread_key: String,
    /// The thread's name from its newest new memory; empty when nothing is new.
    pub title: String,
    pub since_ms: i64,
    /// True when the thread was never marked seen; `since_ms` is then a day ago.
    pub first_view: bool,
    pub new_memories: usize,
    pub page_count: usize,
    pub pages: Vec<String>,
    pub file_count: usize,
    pub files: Vec<String>,
    pub task_count: usize,
    pub tasks: Vec<String>,
    /// Distinct `git commit` commands seen on screen.
    pub commit_count: usize,
    pub newest_memory_id: Option<String>,
}

/// Where a digest starts: the marker, at most a week back, or a day back
/// for a thread never viewed. The flag says which.
pub fn since_for(marker: Option<i64>, now_ms: i64) -> (i64, bool) {
    match marker {
        Some(at) => (at.max(now_ms - MAX_LOOKBACK_MS), false),
        None => (now_ms - FIRST_VIEW_WINDOW_MS, true),
    }
}

fn page_key(url: &str) -> &str {
    url.split('#').next().unwrap_or(url).trim_end_matches('/')
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// Pushes `name` under `key` when the key is new; returns nothing.
fn add_named(seen: &mut HashSet<String>, names: &mut Vec<String>, key: &str, name: String) {
    if !key.is_empty() && seen.insert(key.to_string()) {
        names.push(name);
    }
}

/// The digest for `key` from admitted memories and tasks. Memories outside
/// the thread or not newer than `since_ms` are ignored, as are tasks not
/// drawn from one of the new memories and tasks FNDR only suggested.
pub fn compute_digest(
    key: &str,
    since_ms: i64,
    first_view: bool,
    records: &[MemoryRecord],
    tasks: &[Task],
) -> ThreadDigest {
    let mut rows: Vec<&MemoryRecord> = records
        .iter()
        .filter(|r| r.timestamp > since_ms && thread_key(r) == key)
        .collect();
    rows.sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| b.id.cmp(&a.id)));

    let (mut seen_pages, mut pages) = (HashSet::new(), Vec::new());
    let (mut seen_files, mut files) = (HashSet::new(), Vec::new());
    let mut commits = HashSet::new();
    for row in &rows {
        if let Some(url) = row.url.as_deref().filter(|url| url.starts_with("http")) {
            let title = row.window_title.trim();
            let name = if title.is_empty() {
                crate::capture::extract_domain(url).unwrap_or_else(|| url.to_string())
            } else {
                title.to_string()
            };
            add_named(&mut seen_pages, &mut pages, page_key(url), name);
        }
        for path in row
            .files_touched
            .iter()
            .chain(row.reopen_file_path.iter())
            .map(|path| path.trim())
        {
            add_named(&mut seen_files, &mut files, path, file_name(path));
        }
        for command in &row.commands {
            let command = command.trim();
            if command.starts_with("git commit") {
                commits.insert(command.to_string());
            }
        }
    }
    let ids: HashSet<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    let mut new_tasks: Vec<&Task> = tasks
        .iter()
        .filter(|task| task.created_at > since_ms && !task.is_dismissed)
        .filter(|task| !crate::tasks::suggest::is_suggestion(task))
        .filter(|task| {
            task.source_memory_id
                .as_deref()
                .is_some_and(|id| ids.contains(id))
                || task
                    .linked_memory_ids
                    .iter()
                    .any(|id| ids.contains(id.as_str()))
        })
        .collect();
    new_tasks.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let listed = |names: &[String]| names.iter().take(MAX_LISTED).cloned().collect();
    let task_titles: Vec<String> = new_tasks.iter().map(|task| task.title.clone()).collect();
    ThreadDigest {
        thread_key: key.to_string(),
        title: rows
            .first()
            .map(|row| thread_title(row))
            .unwrap_or_default(),
        since_ms,
        first_view,
        new_memories: rows.len(),
        page_count: pages.len(),
        pages: listed(&pages),
        file_count: files.len(),
        files: listed(&files),
        task_count: task_titles.len(),
        tasks: listed(&task_titles),
        commit_count: commits.len(),
        newest_memory_id: rows.first().map(|row| row.id.clone()),
    }
}

/// Worth a background nudge: a thread viewed before, with new files, tasks
/// or commits, not just more of the same page.
pub fn worth_a_nudge(digest: &ThreadDigest) -> bool {
    !digest.first_view
        && digest.file_count + digest.task_count + digest.commit_count >= NUDGE_MIN_CHANGES
}

fn counted(count: usize, one: &str, many: &str) -> Option<String> {
    match count {
        0 => None,
        1 => Some(format!("1 {one}")),
        n => Some(format!("{n} {many}")),
    }
}

/// The toast: the thread's name and what is new, in counts.
pub fn thread_update_toast(digest: &ThreadDigest) -> (String, String) {
    let parts: Vec<String> = [
        counted(digest.page_count, "page", "pages"),
        counted(digest.file_count, "file", "files"),
        counted(digest.task_count, "task", "tasks"),
        counted(digest.commit_count, "commit", "commits"),
    ]
    .into_iter()
    .flatten()
    .collect();
    (
        "What changed".to_string(),
        format!(
            "{}: {} since you last looked.",
            digest.title,
            parts.join(", ")
        ),
    )
}

async fn admitted_since(
    store: &Store,
    blocklist: &[String],
    since_ms: i64,
    now_ms: i64,
) -> Result<Vec<MemoryRecord>, String> {
    let records = store
        .get_memories_in_range(since_ms + 1, now_ms)
        .await
        .map_err(|e| e.to_string())?;
    Ok(records
        .into_iter()
        .filter(|record| admits(record, blocklist))
        .collect())
}

/// The digest for one thread from the stores: what `what_changed_since`
/// returns.
pub async fn digest_for(
    store: &Store,
    state_store: &StateStore,
    blocklist: &[String],
    key: &str,
    now_ms: i64,
) -> Result<ThreadDigest, String> {
    let (since_ms, first_view) = since_for(last_seen(state_store, key)?, now_ms);
    let records = admitted_since(store, blocklist, since_ms, now_ms).await?;
    let tasks = store.list_tasks().await.map_err(|e| e.to_string())?;
    Ok(compute_digest(key, since_ms, first_view, &records, &tasks))
}

pub async fn what_changed_since(state: &AppState, key: &str) -> Result<ThreadDigest, String> {
    let blocklist = state.config.read().blocklist.clone();
    let now_ms = chrono::Utc::now().timestamp_millis();
    digest_for(&state.store, &state.state_store, &blocklist, key, now_ms).await
}

/// Seen threads with enough new work for a nudge, most changed first. One
/// read of the store covers every thread.
pub async fn nudge_candidates(state: &AppState, now_ms: i64) -> Vec<ThreadDigest> {
    let Ok(markers) = seen_markers(&state.state_store) else {
        return Vec::new();
    };
    let mut markers: Vec<(String, i64)> = markers
        .0
        .into_iter()
        .filter(|(_, at)| now_ms - at >= NUDGE_MIN_AGE_MS)
        .collect();
    markers.sort_by(|a, b| b.1.cmp(&a.1));
    markers.truncate(NUDGE_MAX_THREADS);
    let Some(earliest) = markers
        .iter()
        .map(|(_, at)| since_for(Some(*at), now_ms).0)
        .min()
    else {
        return Vec::new();
    };
    let blocklist = state.config.read().blocklist.clone();
    let Ok(records) = admitted_since(&state.store, &blocklist, earliest, now_ms).await else {
        return Vec::new();
    };
    let tasks = state.store.list_tasks().await.unwrap_or_default();
    let mut digests: Vec<ThreadDigest> = markers
        .iter()
        .map(|(key, at)| {
            let (since_ms, _) = since_for(Some(*at), now_ms);
            compute_digest(key, since_ms, false, &records, &tasks)
        })
        .filter(worth_a_nudge)
        .collect();
    digests.sort_by_key(|d| std::cmp::Reverse(d.file_count + d.task_count + d.commit_count));
    digests
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{Task, TaskType};

    const NOW: i64 = 1_800_000_000_000;
    const HOUR: i64 = 3_600_000;

    fn row(id: &str, project: &str, hours_ago: i64) -> MemoryRecord {
        MemoryRecord {
            id: id.into(),
            timestamp: NOW - hours_ago * HOUR,
            project: project.into(),
            app_name: "Google Chrome".into(),
            clean_text: "Reviewed the parser's query filters and fixed weekday aliases.".into(),
            ocr_block_count: 5,
            ocr_confidence: 0.9,
            ..Default::default()
        }
    }

    fn task(title: &str, memory_id: &str, hours_ago: i64) -> Task {
        Task {
            id: title.into(),
            title: title.into(),
            description: String::new(),
            source_app: "Cursor".into(),
            source_memory_id: Some(memory_id.into()),
            created_at: NOW - hours_ago * HOUR,
            due_date: None,
            is_completed: false,
            is_dismissed: false,
            task_type: TaskType::Todo,
            linked_urls: Vec::new(),
            linked_memory_ids: Vec::new(),
        }
    }

    fn fixture() -> (Vec<MemoryRecord>, Vec<Task>) {
        let mut page = row("page", "Parser", 1);
        page.window_title = "Weekday aliases issue".into();
        page.url = Some("https://github.com/fndr/fndr/issues/12#comment".into());
        let mut same_page = row("same-page", "Parser", 2);
        same_page.window_title = "Weekday aliases issue".into();
        same_page.url = Some("https://github.com/fndr/fndr/issues/12".into());
        let mut code = row("code", "Parser", 3);
        code.app_name = "Cursor".into();
        code.files_touched = vec!["src/search/query_parser.rs".into()];
        code.reopen_file_path = Some("/Users/k/fndr/src/search/aliases.rs".into());
        code.commands = vec![
            "git commit -m \"fix aliases\"".into(),
            "cargo test".into(),
            "git commit -m \"fix aliases\"".into(),
        ];
        let before = row("before", "Parser", 30);
        let other = row("other", "Groceries", 1);
        let tasks = vec![
            task("Add a test for weekday aliases", "code", 2),
            task("Buy milk", "other", 1),
            task("Old parser task", "before", 30),
        ];
        (vec![page, same_page, code, before, other], tasks)
    }

    #[test]
    fn the_digest_counts_only_newer_work_in_the_thread() {
        let (records, tasks) = fixture();
        let digest = compute_digest("Parser", NOW - 24 * HOUR, false, &records, &tasks);
        assert_eq!(digest.new_memories, 3);
        assert_eq!(digest.page_count, 1, "a fragment is the same page");
        assert_eq!(digest.pages, vec!["Weekday aliases issue"]);
        assert_eq!(digest.file_count, 2);
        assert_eq!(digest.files, vec!["query_parser.rs", "aliases.rs"]);
        assert_eq!(digest.task_count, 1);
        assert_eq!(digest.tasks, vec!["Add a test for weekday aliases"]);
        assert_eq!(digest.commit_count, 1, "a commit seen twice is one commit");
        assert_eq!(digest.newest_memory_id.as_deref(), Some("page"));
        assert_eq!(digest.title, "Parser");
    }

    #[test]
    fn a_task_fndr_only_suggested_is_not_new_work() {
        let (records, mut tasks) = fixture();
        let mut suggested = task("Read the aliases RFC", "code", 1);
        suggested.source_app = "Memory: Cursor".into();
        tasks.push(suggested);
        let digest = compute_digest("Parser", NOW - 24 * HOUR, false, &records, &tasks);
        assert_eq!(digest.tasks, vec!["Add a test for weekday aliases"]);
    }

    #[test]
    fn nothing_new_is_an_empty_digest() {
        let (records, tasks) = fixture();
        let digest = compute_digest("Parser", NOW, false, &records, &tasks);
        assert_eq!(digest.new_memories, 0);
        assert!(digest.pages.is_empty() && digest.files.is_empty() && digest.tasks.is_empty());
        assert_eq!(digest.newest_memory_id, None);
        assert!(!worth_a_nudge(&digest));
    }

    #[test]
    fn without_a_marker_the_digest_covers_a_day_and_an_old_marker_a_week() {
        assert_eq!(since_for(None, NOW), (NOW - 24 * HOUR, true));
        assert_eq!(since_for(Some(NOW - HOUR), NOW), (NOW - HOUR, false));
        assert_eq!(since_for(Some(0), NOW), (NOW - 7 * 24 * HOUR, false));
    }

    #[test]
    fn a_nudge_needs_a_seen_thread_with_real_changes() {
        let (records, tasks) = fixture();
        let seen = compute_digest("Parser", NOW - 24 * HOUR, false, &records, &tasks);
        assert!(worth_a_nudge(&seen));
        let first = compute_digest("Parser", NOW - 24 * HOUR, true, &records, &tasks);
        assert!(
            !worth_a_nudge(&first),
            "a thread never viewed has nothing to compare"
        );
        let (title, body) = thread_update_toast(&seen);
        assert!(!title.is_empty());
        assert_eq!(
            body,
            "Parser: 1 page, 2 files, 1 task, 1 commit since you last looked."
        );
    }

    #[test]
    fn markers_persist_across_a_restart_and_keep_the_newest() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = StateStore::new(dir.path()).unwrap();
            assert_eq!(last_seen(&store, "Parser").unwrap(), None);
            mark_seen(&store, "Parser", NOW - HOUR).unwrap();
            mark_seen(&store, "Parser", NOW).unwrap();
            mark_seen(&store, "it's quoted", NOW).unwrap();
        }
        let store = StateStore::new(dir.path()).unwrap();
        assert_eq!(last_seen(&store, "Parser").unwrap(), Some(NOW));
        assert_eq!(last_seen(&store, "it's quoted").unwrap(), Some(NOW));

        let mut markers = SeenMarkers::default();
        for i in 0..(MAX_MARKERS as i64 + 5) {
            markers.mark(&format!("k{i}"), i);
        }
        assert_eq!(markers.0.len(), MAX_MARKERS);
        assert!(!markers.0.contains_key("k0"));
        assert!(markers.0.contains_key(&format!("k{}", MAX_MARKERS + 4)));
    }

    #[tokio::test]
    async fn the_digest_reads_stored_memories_newer_than_the_marker() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let store =
            tokio::task::spawn_blocking(move || Store::new(&path).map_err(|e| e.to_string()))
                .await
                .unwrap()
                .unwrap();
        let state_path = dir.path().to_path_buf();
        let state_store =
            tokio::task::spawn_blocking(move || StateStore::new(&state_path).unwrap())
                .await
                .unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        let mut seen = row("seen", "Parser", 0);
        seen.timestamp = now - 2 * HOUR;
        let mut fresh = row("fresh", "Parser", 0);
        fresh.timestamp = now - HOUR;
        fresh.files_touched = vec!["src/lib.rs".into()];
        let mut hidden = fresh.clone();
        hidden.id = "hidden".into();
        hidden.app_name = "Terminal".into();
        store
            .add_batch_preserving_ids(&[seen, fresh, hidden])
            .await
            .unwrap();
        mark_seen(&state_store, "Parser", now - 90 * 60_000).unwrap();

        let digest = digest_for(
            &store,
            &state_store,
            &["Terminal".to_string()],
            "Parser",
            now,
        )
        .await
        .unwrap();
        assert!(!digest.first_view);
        assert_eq!(digest.new_memories, 1);
        assert_eq!(digest.files, vec!["lib.rs"]);
        assert_eq!(digest.newest_memory_id.as_deref(), Some("fresh"));
    }
}
