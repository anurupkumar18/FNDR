//! Local, per-source permission to scan past data. Missing means off.

use super::ImportKind;
use crate::storage::StateStore;

fn key(kind: ImportKind) -> String {
    format!("import_consent:{}", kind.as_str())
}

pub fn get(store: &StateStore, kind: ImportKind) -> Result<bool, String> {
    Ok(store.load_json(&key(kind))?.unwrap_or(false))
}

pub fn set(store: &StateStore, kind: ImportKind, enabled: bool) -> Result<(), String> {
    if enabled {
        store.save_json(&key(kind), &true)
    } else {
        store.delete_key(&key(kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_and_corrupt_consent_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        let store = StateStore::new(dir.path()).unwrap();
        assert!(!get(&store, ImportKind::Browser).unwrap());
        store
            .save_json(&key(ImportKind::Browser), &"corrupt")
            .unwrap();
        assert!(get(&store, ImportKind::Browser).is_err());
        set(&store, ImportKind::Browser, true).unwrap();
        assert!(get(&store, ImportKind::Browser).unwrap());
        assert!(!get(&store, ImportKind::Mail).unwrap());
        set(&store, ImportKind::Browser, false).unwrap();
        assert!(!get(&store, ImportKind::Browser).unwrap());
    }
}
