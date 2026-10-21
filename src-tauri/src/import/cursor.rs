//! Resumable import cursors, kept in the app state store
//! (`storage/state_store.rs`) under `import_cursor:<kind>`.
//!
//! The runner saves a cursor after every batch it has fully stored, so a
//! cancelled or crashed run resumes where it stopped and a finished run
//! continues from its high-water mark. Re-running is still safe without a
//! cursor: the runner deduplicates by kind, external id and content hash.

use super::{Cursor, ImportKind};
use crate::storage::StateStore;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorState {
    pub cursor: Option<String>,
    /// The window the cursor was made under.
    pub window_days: u32,
    /// The last run reached the end of its window.
    pub complete: bool,
    pub updated_at_ms: i64,
}

pub fn key(kind: ImportKind) -> String {
    format!("import_cursor:{}", kind.as_str())
}

pub fn load(store: &StateStore, kind: ImportKind) -> Result<Option<CursorState>, String> {
    store.load_json(&key(kind))
}

pub fn save(store: &StateStore, kind: ImportKind, state: &CursorState) -> Result<(), String> {
    store.save_json(&key(kind), state)
}

pub fn clear(store: &StateStore, kind: ImportKind) -> Result<(), String> {
    store.delete_key(&key(kind))
}

fn tombstone_key(kind: ImportKind) -> String {
    format!("import_tombstones:{}", kind.as_str())
}

pub fn load_tombstones(store: &StateStore, kind: ImportKind) -> Result<BTreeSet<String>, String> {
    Ok(store.load_json(&tombstone_key(kind))?.unwrap_or_default())
}

pub fn add_tombstones(store: &StateStore, kind: ImportKind, ids: &[String]) -> Result<(), String> {
    let mut tombstones = load_tombstones(store, kind)?;
    tombstones.extend(ids.iter().cloned());
    store.save_json(&tombstone_key(kind), &tombstones)
}

pub fn clear_tombstones(store: &StateStore, kind: ImportKind) -> Result<(), String> {
    store.delete_key(&tombstone_key(kind))
}

/// Where a new run starts. A saved cursor is used unless the person now asks
/// for a longer window than it was made under: the cursor would skip the older
/// items, so the run starts over (duplicates are dropped by the runner).
pub fn resume_point(saved: Option<&CursorState>, window_days: u32) -> Option<Cursor> {
    let saved = saved?;
    if window_days > saved.window_days {
        return None;
    }
    saved.cursor.clone().map(Cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(cursor: Option<&str>, window_days: u32) -> CursorState {
        CursorState {
            cursor: cursor.map(str::to_string),
            window_days,
            complete: false,
            updated_at_ms: 1,
        }
    }

    #[test]
    fn a_saved_cursor_resumes_unless_the_window_grew() {
        let saved = state(Some("page-3"), 30);
        assert_eq!(
            resume_point(Some(&saved), 30),
            Some(Cursor("page-3".into()))
        );
        assert_eq!(resume_point(Some(&saved), 7), Some(Cursor("page-3".into())));
        assert_eq!(resume_point(Some(&saved), 90), None);
        assert_eq!(resume_point(None, 30), None);
        assert_eq!(resume_point(Some(&state(None, 30)), 30), None);
    }

    #[test]
    fn cursors_survive_the_state_store() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = StateStore::new(dir.path()).expect("state store");
        assert_eq!(load(&store, ImportKind::Mail).unwrap(), None);
        let saved = state(Some("uid-9"), 30);
        save(&store, ImportKind::Mail, &saved).unwrap();
        assert_eq!(load(&store, ImportKind::Mail).unwrap(), Some(saved));
        assert_eq!(load(&store, ImportKind::Slack).unwrap(), None);
        clear(&store, ImportKind::Mail).unwrap();
        assert_eq!(load(&store, ImportKind::Mail).unwrap(), None);
    }

    #[test]
    fn deleted_import_ids_stay_tombstoned_until_forget() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = StateStore::new(dir.path()).expect("state store");
        let kind = ImportKind::Mail;
        add_tombstones(&store, kind, &["imp_a".into(), "imp_b".into()]).unwrap();
        add_tombstones(&store, kind, &["imp_a".into()]).unwrap();
        assert_eq!(load_tombstones(&store, kind).unwrap().len(), 2);
        assert!(load_tombstones(&store, kind).unwrap().contains("imp_a"));
        assert!(load_tombstones(&store, ImportKind::File)
            .unwrap()
            .is_empty());
        clear_tombstones(&store, kind).unwrap();
        assert!(load_tombstones(&store, kind).unwrap().is_empty());
    }
}
