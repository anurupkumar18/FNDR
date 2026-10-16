//! Named work sets: a set the person saved under a name, kept in the state
//! store as memory ids only. Its items are resolved again on every read, so
//! a memory deleted since, or one now private or blocklisted, drops out.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::rank::{eligible, item_of};
use super::{WorkItem, WorkSet, MAX_ITEMS};
use crate::storage::{MemoryRecord, StateStore};
use crate::AppState;

const STATE_KEY: &str = "named_work_sets_v1";
pub const MAX_SETS: usize = 30;
pub const MAX_NAME_CHARS: usize = 60;

/// What the state store keeps for one set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSet {
    pub id: String,
    pub name: String,
    pub memory_ids: Vec<String>,
    pub saved_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_opened_at: Option<i64>,
}

/// A saved set as surfaces see it: only the items FNDR may still reopen.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamedWorkSet {
    pub id: String,
    pub name: String,
    /// The ids of `items`, in order: what to pass to `open_work_set`.
    pub memory_ids: Vec<String>,
    pub items: Vec<WorkItem>,
    pub saved_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_opened_at: Option<i64>,
}

fn tidy(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Adds a set under `name`. Names are unique ignoring case and spacing.
pub fn add(
    sets: &mut Vec<SavedSet>,
    name: &str,
    memory_ids: &[String],
    now_ms: i64,
    id: String,
) -> Result<SavedSet, String> {
    let name = tidy(name);
    if name.is_empty() {
        return Err("Give the set a name.".to_string());
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(format!(
            "Keep the name to {MAX_NAME_CHARS} characters or fewer."
        ));
    }
    if sets
        .iter()
        .any(|set| tidy(&set.name).to_lowercase() == name.to_lowercase())
    {
        return Err(format!("A set is already called \u{201c}{name}\u{201d}."));
    }
    if sets.len() >= MAX_SETS {
        return Err(format!(
            "FNDR keeps at most {MAX_SETS} named sets. Delete one first."
        ));
    }
    let mut ids: Vec<String> = Vec::new();
    for memory_id in memory_ids {
        let memory_id = memory_id.trim();
        if !memory_id.is_empty() && !ids.iter().any(|seen| seen == memory_id) {
            ids.push(memory_id.to_string());
        }
    }
    ids.truncate(MAX_ITEMS);
    if ids.is_empty() {
        return Err("Nothing to save.".to_string());
    }
    let saved = SavedSet {
        id,
        name,
        memory_ids: ids,
        saved_at: now_ms,
        last_opened_at: None,
    };
    sets.insert(0, saved.clone());
    Ok(saved)
}

/// The saved set holding exactly these memories, in any order.
pub fn holding<'a>(sets: &'a [SavedSet], memory_ids: &[String]) -> Option<&'a SavedSet> {
    let mut wanted = memory_ids.to_vec();
    wanted.sort();
    wanted.dedup();
    sets.iter().find(|set| {
        let mut held = set.memory_ids.clone();
        held.sort();
        held.dedup();
        held == wanted
    })
}

/// A saved set with its items resolved from `records` now. A memory that is
/// gone, excluded by the privacy rules or without a place to reopen is left out.
pub fn present(
    set: &SavedSet,
    records: &HashMap<String, MemoryRecord>,
    blocklist: &[String],
) -> NamedWorkSet {
    let items: Vec<WorkItem> = set
        .memory_ids
        .iter()
        .filter_map(|id| records.get(id))
        .filter(|record| eligible(record, blocklist))
        .filter_map(|record| item_of(record).map(|(_, item)| item))
        .collect();
    NamedWorkSet {
        id: set.id.clone(),
        name: set.name.clone(),
        memory_ids: items.iter().map(|item| item.memory_id.clone()).collect(),
        items,
        saved_at: set.saved_at,
        last_opened_at: set.last_opened_at,
    }
}

/// A saved set as a work set Notch Do can plan from.
pub fn as_work_set(named: &NamedWorkSet) -> WorkSet {
    WorkSet {
        id: format!("named:{}", named.id),
        title: named.name.clone(),
        reason: format!("Saved as \u{201c}{}\u{201d}", named.name),
        score: 1.0,
        items: named.items.clone(),
    }
}

/// One writer at a time, so two saves cannot drop each other.
static WRITE: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

pub fn load(store: &StateStore) -> Result<Vec<SavedSet>, String> {
    Ok(store.load_json(STATE_KEY)?.unwrap_or_default())
}

fn update<T>(
    store: &StateStore,
    change: impl FnOnce(&mut Vec<SavedSet>) -> Result<T, String>,
) -> Result<T, String> {
    let _held = WRITE.lock();
    let mut sets = load(store)?;
    let result = change(&mut sets)?;
    store.save_json(STATE_KEY, &sets)?;
    Ok(result)
}

pub fn save(store: &StateStore, name: &str, memory_ids: &[String]) -> Result<SavedSet, String> {
    let now = chrono::Utc::now().timestamp_millis();
    update(store, |sets| {
        add(
            sets,
            name,
            memory_ids,
            now,
            uuid::Uuid::new_v4().to_string(),
        )
    })
}

pub fn delete(store: &StateStore, id: &str) -> Result<(), String> {
    update(store, |sets| {
        sets.retain(|set| set.id != id);
        Ok(())
    })
}

/// Notes when the set holding these memories was opened. Returns its id.
pub fn mark_opened(store: &StateStore, memory_ids: &[String], now_ms: i64) -> Option<String> {
    update(store, |sets| {
        let id = holding(sets, memory_ids).map(|set| set.id.clone());
        if let Some(set) = sets.iter_mut().find(|set| Some(&set.id) == id.as_ref()) {
            set.last_opened_at = Some(now_ms);
        }
        Ok(id)
    })
    .unwrap_or_else(|error| {
        tracing::warn!(%error, "workset:named_mark_failed");
        None
    })
}

/// Every saved set with its items resolved now, newest first.
pub async fn list(state: &AppState) -> Result<Vec<NamedWorkSet>, String> {
    let sets = load(&state.state_store)?;
    let ids: Vec<String> = sets
        .iter()
        .flat_map(|set| set.memory_ids.iter().cloned())
        .collect();
    let records = state
        .store
        .get_memories_by_ids(&ids)
        .await
        .map_err(|e: Box<dyn std::error::Error>| e.to_string())?;
    let blocklist = state.config.read().blocklist.clone();
    Ok(sets
        .iter()
        .map(|set| present(set, &records, &blocklist))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::reopen::ReopenKind;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|id| id.to_string()).collect()
    }

    fn saved(sets: &mut Vec<SavedSet>, name: &str) -> Result<SavedSet, String> {
        let id = format!("id-{}", sets.len());
        add(sets, name, &ids(&["a", "b"]), 1, id)
    }

    #[test]
    fn a_name_is_required_unique_ignoring_case_and_short() {
        let mut sets = Vec::new();
        let first = saved(&mut sets, "  Capstone   demo prep ").unwrap();
        assert_eq!(first.name, "Capstone demo prep");
        assert_eq!(
            saved(&mut sets, "capstone DEMO prep").unwrap_err(),
            "A set is already called \u{201c}capstone DEMO prep\u{201d}."
        );
        assert_eq!(saved(&mut sets, "   ").unwrap_err(), "Give the set a name.");
        assert!(saved(&mut sets, &"x".repeat(MAX_NAME_CHARS)).is_ok());
        assert!(saved(&mut sets, &"y".repeat(MAX_NAME_CHARS + 1))
            .unwrap_err()
            .contains("60 characters"));
        assert_eq!(sets.len(), 2);
        assert_eq!(sets[0].name, "x".repeat(MAX_NAME_CHARS), "newest first");
    }

    #[test]
    fn at_most_thirty_sets_and_six_distinct_ids_each() {
        let mut sets = Vec::new();
        for n in 0..MAX_SETS {
            saved(&mut sets, &format!("set {n}")).unwrap();
        }
        assert!(saved(&mut sets, "one more")
            .unwrap_err()
            .contains("at most 30"));
        let mut sets = Vec::new();
        let many = ids(&["a", "b", "a", " ", "c", "d", "e", "f", "g"]);
        let set = add(&mut sets, "many", &many, 1, "m".into()).unwrap();
        assert_eq!(set.memory_ids, ids(&["a", "b", "c", "d", "e", "f"]));
        assert_eq!(
            add(&mut sets, "empty", &ids(&[" "]), 1, "e".into()).unwrap_err(),
            "Nothing to save."
        );
    }

    #[test]
    fn the_set_holding_the_same_memories_is_found_in_any_order() {
        let mut sets = Vec::new();
        add(&mut sets, "lab", &ids(&["a", "b"]), 1, "lab".into()).unwrap();
        assert_eq!(
            holding(&sets, &ids(&["b", "a"])).map(|s| s.id.as_str()),
            Some("lab")
        );
        assert!(holding(&sets, &ids(&["a"])).is_none());
    }

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
            reopen_kind: ReopenKind::BrowserUrl,
            reopen_url: Some(url.into()),
            url: Some(url.into()),
            ..Default::default()
        }
    }

    #[test]
    fn items_are_resolved_again_and_excluded_memories_drop_out() {
        let set = SavedSet {
            id: "s".into(),
            name: "Capstone".into(),
            memory_ids: ids(&["canvas", "chat", "gone", "fndr", "doc"]),
            saved_at: 1,
            last_opened_at: Some(2),
        };
        let records: HashMap<String, MemoryRecord> = [
            page(
                "canvas",
                "Google Chrome",
                "Capstone",
                "https://canvas.example/c",
            ),
            page("chat", "Slack", "Team chat", "https://slack.example/c"),
            page("fndr", "FNDR", "FNDR", "https://fndr.example"),
            page(
                "doc",
                "Google Chrome",
                "Report - Google Docs",
                "https://docs.google.com/d/1",
            ),
        ]
        .into_iter()
        .map(|record| (record.id.clone(), record))
        .collect();
        let named = present(&set, &records, &["Slack".to_string()]);
        assert_eq!(named.memory_ids, ids(&["canvas", "doc"]));
        assert_eq!(named.items[1].label, "Report - Google Docs");
        assert_eq!(named.last_opened_at, Some(2));
        let value = serde_json::to_value(&named).unwrap();
        assert_eq!(value["memoryIds"][0], "canvas");
        assert_eq!(value["savedAt"], 1);
        let work = as_work_set(&named);
        assert_eq!(work.title, "Capstone");
        assert_eq!(work.items.len(), 2);
    }
}
