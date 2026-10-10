//! Opening what a work set holds, through the reopen core the Vault uses.
//! Privacy is checked again here: a memory excluded since it was resolved,
//! or an app blocklisted since, is left out.

use std::future::Future;

use serde::{Deserialize, Serialize};

use super::arrange::Arrangement;
use super::rank::{eligible, item_of};
use super::{ItemKind, WorkItem, MAX_ITEMS};
use crate::memory::reopen::ReopenOutcome;
use crate::storage::{MemoryRecord, Store};
use crate::AppState;

/// What became of one item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemOutcome {
    pub memory_id: String,
    pub label: String,
    /// None when FNDR no longer has the memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ItemKind>,
    /// FNDR opened it, judged from the typed outcome, never from a report.
    pub ok: bool,
    pub detail: String,
    /// None when FNDR did not try: a halt, a missing memory, or a privacy rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<ReopenOutcome>,
    /// On an opened item when a layout was asked for: whether the windows
    /// were arranged, and why not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrangement: Option<Arrangement>,
}

pub(crate) const LEFT_OUT: &str = "Left out: FNDR does not reopen private or blocklisted memories";
const GONE: &str = "FNDR no longer has this memory";

/// Why nothing may open right now: actions switched off, or FNDR private.
fn halted(state: &AppState) -> Option<String> {
    if state.config.read().actions_kill_switch {
        Some("Actions are turned off in Settings.".to_string())
    } else if state.is_incognito.load(std::sync::atomic::Ordering::SeqCst) {
        Some("FNDR is private right now.".to_string())
    } else {
        None
    }
}

/// Opens a set's items in order and says what became of each.
pub async fn open_items(state: &AppState, items: &[WorkItem]) -> Vec<ItemOutcome> {
    let ids: Vec<String> = items.iter().map(|item| item.memory_id.clone()).collect();
    open_ids(state, &ids).await
}

/// `open_items` by memory id, for a caller that holds only the ids.
pub async fn open_ids(state: &AppState, memory_ids: &[String]) -> Vec<ItemOutcome> {
    let blocklist = state.config.read().blocklist.clone();
    open_items_with(
        &state.store,
        &blocklist,
        || halted(state),
        memory_ids,
        |record| async move { crate::ipc::commands::memory::reopen_record(&record).await },
    )
    .await
}

/// `open_items` with the opener passed in, so a test can stand in for the
/// Mac. At most `MAX_ITEMS` distinct ids are opened; `halt` is asked before
/// each one.
pub async fn open_items_with<F, Fut>(
    store: &Store,
    blocklist: &[String],
    halt: impl Fn() -> Option<String>,
    memory_ids: &[String],
    mut open: F,
) -> Vec<ItemOutcome>
where
    F: FnMut(MemoryRecord) -> Fut,
    Fut: Future<Output = Result<ReopenOutcome, String>>,
{
    let mut ids: Vec<String> = Vec::new();
    for id in memory_ids {
        if !ids.contains(id) && ids.len() < MAX_ITEMS {
            ids.push(id.clone());
        }
    }
    let skipped = |id: &String, detail: String| ItemOutcome {
        memory_id: id.clone(),
        label: String::new(),
        kind: None,
        ok: false,
        detail,
        outcome: None,
        arrangement: None,
    };
    let mut records = match store.get_memories_by_ids(&ids).await {
        Ok(records) => records,
        Err(error) => {
            let detail = format!("FNDR could not read its memories: {error}");
            return ids.iter().map(|id| skipped(id, detail.clone())).collect();
        }
    };
    let mut outcomes = Vec::with_capacity(ids.len());
    for id in &ids {
        if let Some(reason) = halt() {
            outcomes.push(skipped(id, reason));
            continue;
        }
        let Some(record) = records.remove(id) else {
            outcomes.push(skipped(id, GONE.to_string()));
            continue;
        };
        let (label, kind) = item_of(&record)
            .map(|(_, item)| (item.label, Some(item.kind)))
            .unwrap_or_else(|| (record.window_title.clone(), None));
        if !eligible(&record, blocklist) {
            outcomes.push(ItemOutcome {
                label,
                kind,
                ..skipped(id, LEFT_OUT.to_string())
            });
            continue;
        }
        let (ok, detail, outcome) = match open(record).await {
            Ok(outcome) => {
                let verdict = crate::operator::plan::verify_reopen(&outcome);
                (verdict.ok, verdict.detail, Some(outcome))
            }
            Err(error) => (false, error, None),
        };
        outcomes.push(ItemOutcome {
            memory_id: id.clone(),
            label,
            kind,
            ok,
            detail,
            outcome,
            arrangement: None,
        });
    }
    outcomes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::reopen::ReopenKind;
    use std::sync::{Arc, Mutex};

    fn page(id: &str, app: &str, title: &str, url: &str) -> MemoryRecord {
        MemoryRecord {
            id: id.into(),
            timestamp: 1_800_000_000_000,
            app_name: app.into(),
            window_title: title.into(),
            session_id: "s".into(),
            day_bucket: "2027-01-15".into(),
            clean_text: format!("{title} was open"),
            snippet: format!("{title} was open"),
            embedding: vec![0.01; crate::embedding::EMBEDDING_DIM],
            reopen_kind: ReopenKind::BrowserUrl,
            reopen_url: Some(url.into()),
            url: Some(url.into()),
            ..Default::default()
        }
    }

    /// A real store and a fake Mac: what is opened, in what order, and what
    /// is refused before the opener is ever called.
    #[tokio::test]
    async fn a_set_opens_in_order_and_privacy_is_checked_again_at_open_time() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).unwrap())
            .await
            .unwrap();
        let records = vec![
            page(
                "canvas",
                "Google Chrome",
                "Assignment 3",
                "https://canvas.example/a/3",
            ),
            page(
                "doc",
                "Google Chrome",
                "Lab report - Google Docs",
                "https://docs.google.com/d/1",
            ),
            page("chat", "Slack", "Lab group", "https://slack.example/c"),
            page("gone", "Google Chrome", "Old page", "https://old.example"),
        ];
        store.add_batch_preserving_ids(&records).await.unwrap();
        store.delete_memory_by_id("gone").await.unwrap();

        let opened: Arc<Mutex<Vec<String>>> = Arc::default();
        let seen = opened.clone();
        let ids: Vec<String> = ["doc", "canvas", "chat", "gone", "doc"]
            .iter()
            .map(|id| id.to_string())
            .collect();
        let outcomes = open_items_with(
            &store,
            &["Slack".to_string()],
            || None,
            &ids,
            |record| {
                let seen = seen.clone();
                async move {
                    seen.lock().unwrap().push(record.id.clone());
                    Ok(if record.id == "canvas" {
                        ReopenOutcome::Opened
                    } else {
                        ReopenOutcome::Blocked { target: "x".into() }
                    })
                }
            },
        )
        .await;

        assert_eq!(
            *opened.lock().unwrap(),
            ["doc", "canvas"],
            "the blocklisted chat never reaches the Mac"
        );
        let summary: Vec<(&str, bool, &str)> = outcomes
            .iter()
            .map(|o| (o.memory_id.as_str(), o.ok, o.detail.as_str()))
            .collect();
        assert_eq!(summary.len(), 4, "a repeated id opens once");
        assert_eq!(summary[0].0, "doc");
        assert!(!summary[0].1, "a blocked outcome is a failed step");
        assert_eq!(summary[1], ("canvas", true, "Opened"));
        assert_eq!(summary[2], ("chat", false, LEFT_OUT));
        assert_eq!(summary[3], ("gone", false, GONE));
        assert_eq!(outcomes[1].label, "Assignment 3");
        assert_eq!(outcomes[1].kind, Some(ItemKind::Url));
    }

    #[tokio::test]
    async fn a_halt_stops_everything_still_waiting() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).unwrap())
            .await
            .unwrap();
        store
            .add_batch_preserving_ids(&[page("a", "Safari", "A", "https://a.example")])
            .await
            .unwrap();
        let mut called = false;
        let outcomes = open_items_with(
            &store,
            &[],
            || Some("Actions are turned off in Settings.".to_string()),
            &["a".to_string()],
            |_| {
                called = true;
                async { Ok(ReopenOutcome::Opened) }
            },
        )
        .await;
        assert!(!called);
        assert_eq!(outcomes[0].detail, "Actions are turned off in Settings.");
        assert!(!outcomes[0].ok);
    }
}
