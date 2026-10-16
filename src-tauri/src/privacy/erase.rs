//! Shared memory erasure used by range deletion and, after import integration,
//! source Forget. Import callers must persist tombstones before calling this.

use crate::graph::GraphStore;
use crate::storage::{MemoryRecord, Store};
use std::sync::Arc;

pub async fn erase_memories(
    store: &Arc<Store>,
    graph: &GraphStore,
    records: &[MemoryRecord],
) -> Result<usize, String> {
    let mut deleted_ids = Vec::new();
    for record in records {
        if crate::ipc::commands::delete_memory_logic(store, graph, &record.id).await? {
            deleted_ids.push(record.id.clone());
        }
    }
    if !deleted_ids.is_empty() {
        crate::graph::graph_store::GraphStore::new(store.clone())
            .forget_memory_sources(&deleted_ids)
            .await
            .map_err(|error| error.to_string())?;
        store
            .compact_and_prune_erased_data()
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(deleted_ids.len())
}
