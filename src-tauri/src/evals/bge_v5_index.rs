//! Real-model smoke eval for the BGE v5 parent/chunk index and lazy query embedder.
//!
//! Verifies:
//! 1. `shared_bge_v5_query_embedder` loads the real ONNX model once and reuses it.
//! 2. BGE document embeddings can populate `memory_chunks_v1_bge_1024`.
//! 3. `ChunkRoute` returns parent hits when the chunk index is populated.
//!
//! Requires BGE assets. Prefer a dedicated directory so MiniLM's `tokenizer.json`
//! in the default models folder is not overwritten:
//! `~/Library/Application Support/com.fndr.app/models-bge/`
//!
//! Run with:
//! `FNDR_EMBED_MODEL_DIR="$HOME/Library/Application Support/com.fndr.app/models-bge" \
//!   cargo test --lib eval_bge_v5_index_smoke -- --ignored --nocapture`

use crate::config::{ChunkingConfig, SearchConfig};
use crate::context_runtime::chunk_route::ChunkRoute;
use crate::context_runtime::query_plan::{plan, PlanHints};
use crate::context_runtime::retrieval_routes::{RetrievalRoute, RouteCtx};
use crate::embedding::prefixes::prefix_document_for_index;
use crate::embedding::{select_salient_memory_chunks, shared_bge_v5_query_embedder, Embedder, EmbeddingBackend};
use crate::inference::model_config::{embedding_v5_contract, BGE_V5_DIMENSIONS};
use crate::storage::{MemoryChunkRecord, MemoryRecord, Store};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;

fn default_bge_model_dir() -> PathBuf {
    dirs::home_dir()
        .map(|home| {
            home.join("Library/Application Support/com.fndr.app/models-bge")
        })
        .unwrap_or_else(|| PathBuf::from("models-bge"))
}

fn ensure_bge_model_dir() {
    if std::env::var("FNDR_EMBED_MODEL_DIR").is_err()
        && std::env::var("FNDR_MODEL_DIR").is_err()
    {
        let dir = default_bge_model_dir();
        if dir.join(embedding_v5_contract().model_filename).exists() {
            std::env::set_var("FNDR_EMBED_MODEL_DIR", &dir);
        }
    }
}

fn memory_chunk_content_hash(source: &MemoryRecord, chunk_text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source.id.as_bytes());
    hasher.update(b"\n");
    hasher.update(source.content_hash.as_bytes());
    hasher.update(b"\n");
    hasher.update(chunk_text.trim().as_bytes());
    format!("{:x}", hasher.finalize())
}

async fn seed_real_bge_index(
    store: &Store,
    embedder: &Embedder,
) -> Result<(MemoryRecord, MemoryChunkRecord), String> {
    let chunking = ChunkingConfig::default();
    let mut parent = MemoryRecord {
        id: "bge-smoke-parent".to_string(),
        timestamp: 1_700_000_000_000,
        day_bucket: "2026-09-09".to_string(),
        app_name: "Terminal".to_string(),
        window_title: "BGE index smoke".to_string(),
        session_id: "bge-smoke".to_string(),
        text: "BGE chunk index lazy initialization stores child embeddings for chunk-first retrieval.".to_string(),
        clean_text: "BGE chunk index lazy initialization stores child embeddings for chunk-first retrieval.".to_string(),
        snippet: "BGE chunk index lazy initialization".to_string(),
        content_hash: "bge-smoke-content".to_string(),
        ..Default::default()
    };

    let selected = select_salient_memory_chunks(
        &chunking,
        &parent.app_name,
        &parent.window_title,
        &parent.clean_text,
        chunking.max_chunks_per_memory,
    );
    if selected.is_empty() {
        return Err("chunk selection returned no salient chunks".to_string());
    }

    let prefixed = selected
        .iter()
        .map(|chunk| prefix_document_for_index(&chunk.text))
        .collect::<Vec<_>>();
    let vectors = embedder.embed_batch(&prefixed)?;
    let best = selected
        .into_iter()
        .zip(vectors.into_iter())
        .next()
        .ok_or_else(|| "BGE chunk embedding returned no vectors".to_string())?;

    parent.embedding = best.1.clone();
    parent.snippet_embedding = best.1.clone();
    parent.support_embedding = best.1.clone();
    parent.embedding_model = embedding_v5_contract().model_id.to_string();
    parent.embedding_dim = BGE_V5_DIMENSIONS as u32;

    let chunk = MemoryChunkRecord {
        id: format!("{}:chunk:0000:smoke", parent.id),
        memory_id: parent.id.clone(),
        chunk_index: best.0.chunk_index as u32,
        line_kind: best.0.line_kind.to_string(),
        text: best.0.text.clone(),
        embedding: best.1,
        created_at: parent.timestamp,
        app_name: parent.app_name.clone(),
        window_title: parent.window_title.clone(),
        day_bucket: parent.day_bucket.clone(),
        content_hash: memory_chunk_content_hash(&parent, &best.0.text),
    };

    store
        .add_v5_batch_preserving_ids(&[parent.clone()])
        .await
        .map_err(|err| err.to_string())?;
    store
        .upsert_memory_chunks(&[chunk.clone()])
        .await
        .map_err(|err| err.to_string())?;

    Ok((parent, chunk))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires local BGE model assets; set FNDR_EMBED_MODEL_DIR or use models-bge/"]
    async fn eval_bge_v5_index_smoke() {
        ensure_bge_model_dir();

        let reindex = Embedder::new_bge_v5_for_reindex().expect("BGE reindex embedder initializes");
        assert_eq!(reindex.backend(), EmbeddingBackend::Real);
        assert_eq!(reindex.dimension(), BGE_V5_DIMENSIONS);

        let first: Arc<Embedder> = shared_bge_v5_query_embedder().expect("lazy BGE query embedder");
        let second: Arc<Embedder> = shared_bge_v5_query_embedder().expect("cached BGE query embedder");
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.backend(), EmbeddingBackend::Real);
        assert_eq!(first.dimension(), BGE_V5_DIMENSIONS);

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_path_buf();
        let store = tokio::task::spawn_blocking(move || Store::new(&path).expect("store"))
            .await
            .expect("store task");

        let (parent, chunk) = seed_real_bge_index(&store, &reindex)
            .await
            .expect("seed real BGE chunk index");
        assert!(
            store.has_chunk_retrieval_index().await.expect("index probe"),
            "chunk table should be populated"
        );

        let mut search_config = SearchConfig::default().normalized();
        search_config.use_chunk_first_retrieval = true;
        let ctx = RouteCtx::new(&store, &search_config);
        let query_plan = plan("lazy BGE chunk initialization", &PlanHints::default());
        let hits = ChunkRoute.run(&query_plan, &ctx).await;

        assert!(
            !hits.hits.is_empty(),
            "chunk-first route should return at least one parent hit"
        );
        assert_eq!(hits.hits[0].memory_id, parent.id);
        let matched_chunks = hits.hits[0]
            .signals
            .search_result
            .as_ref()
            .map(|result| result.matched_chunk_ids.clone())
            .unwrap_or_default();
        assert!(
            matched_chunks.contains(&chunk.id),
            "winning chunk evidence should reference seeded chunk"
        );

        println!(
            "BGE v5 index smoke OK: parent={} chunk={} top_score={:.3}",
            parent.id,
            chunk.id,
            hits.hits[0].score
        );
    }
}
