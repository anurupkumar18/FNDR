pub const FNDR_MODEL_PROFILE: &str = "m1_8gb_default";

pub const MULTIMODAL_MODEL_REPO: &str = "Qwen/Qwen3-VL-2B-Instruct-GGUF";
pub const MULTIMODAL_MODEL_QUANT: &str = "Q4_K_M";
pub const MULTIMODAL_MODEL_ID: &str = "qwen3-vl-2b";
pub const MULTIMODAL_MODEL_FILENAME: &str = "Qwen3VL-2B-Instruct-Q4_K_M.gguf";
pub const MULTIMODAL_MODEL_DOWNLOAD_URL: &str =
    "https://huggingface.co/Qwen/Qwen3-VL-2B-Instruct-GGUF/resolve/main/Qwen3VL-2B-Instruct-Q4_K_M.gguf";
pub const MULTIMODAL_MODEL_SIZE_BYTES: u64 = 1_500_000_000;
pub const MULTIMODAL_MODEL_RAM_GB: f32 = 3.5;

/// Minimum on-disk size to accept as a real Qwen3-VL-2B GGUF (rejects LFS pointers).
pub const QWEN3_VL_2B_MAIN_GGUF_MIN_BYTES: u64 = 900_000_000;

// ── Embedding contract (single source of truth) ─────────────────────────────
//
// The constants below describe the *current durable text-embedding write path*
// (v4, MiniLM 384). Every embedding-aware module — `src-tauri/src/config.rs`,
// `src-tauri/src/embedding/onnx.rs`, `src-tauri/src/storage/lance_store/*`,
// `src-tauri/src/embed/embedding_gemma.rs` — references these constants
// (directly or via the thin re-exports in `config.rs`) so the model name, file
// name, tokenizer, vector dimension, and Lance table name cannot drift apart.
//
// `MEMORIES_V5_TABLE` is reserved as a forward-intent placeholder for the
// planned BGE 1024-d upgrade. It is NOT wired into any read/write path yet —
// Subagent 6 will switch the write path over and add the matching schema.

pub const EMBEDDING_MODEL_ID: &str = "sentence-transformers/all-MiniLM-L6-v2";
pub const EMBEDDING_MODEL_FILENAME: &str = "all-MiniLM-L6-v2.onnx";
pub const EMBEDDING_TOKENIZER_FILENAME: &str = "tokenizer.json";
/// all-MiniLM-L6-v2 produces 384-dimensional sentence embeddings.
pub const EMBEDDING_DIMENSIONS: usize = 384;
pub const EMBEDDING_DIMENSIONS_I32: i32 = EMBEDDING_DIMENSIONS as i32;
pub const EMBEDDING_MAX_SEQ_LEN: usize = 512;

pub const MAX_CONCURRENT_MULTIMODAL_JOBS: usize = 1;
pub const QWEN_IDLE_UNLOAD_SECONDS: u64 = 90;
pub const MAX_IMAGE_LONG_EDGE: u32 = 1024;
pub const MAX_MEMORY_PROMPT_TOKENS: usize = 3500;
pub const MAX_MEMORY_OUTPUT_TOKENS: usize = 900;
pub const QWEN_CONTEXT_SIZE: u32 = 4096;
pub const QWEN_TEMPERATURE: f32 = 0.1;
pub const QWEN_TOP_P: f32 = 0.8;

/// LanceDB table name for memories using all-MiniLM-L6-v2 384-dim vectors.
/// This is the **current durable write path** for memories. Search, capture,
/// and ingestion all target this table. Re-exported from
/// `lance_store::MEMORIES_TABLE` for legacy callers.
pub const MEMORIES_V4_TABLE: &str = "memories_v4_minilm_384";

/// Forward-intent placeholder for the planned BGE 1024-d migration.
/// **Not wired anywhere yet** — Subagent 6 will add the schema, validation,
/// and write path together. The name is reserved here so doc references stay
/// consistent and no other slice accidentally claims the table name.
pub const MEMORIES_V5_TABLE: &str = "memories_v5_bge_1024";

/// Old model directories to list in cleanup dry-run (not deleted automatically).
pub const CLEANUP_OLD_MODEL_DIRS: &[&str] = &[
    "llama-3.2-1b",
    "smolvlm-500m",
    "qwen3-vl-4b",
    "bge-large-en-v1.5",
    "embeddinggemma-300m",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        DEFAULT_EMBEDDING_MAX_SEQ_LEN, DEFAULT_EMBEDDING_MODEL_FILENAME,
        DEFAULT_EMBEDDING_MODEL_NAME, DEFAULT_EMBEDDING_TOKENIZER_FILENAME,
        DEFAULT_TEXT_EMBEDDING_DIM,
    };

    #[test]
    fn embedding_contract_constants_are_internally_consistent() {
        // model identity
        assert_eq!(EMBEDDING_MODEL_ID, DEFAULT_EMBEDDING_MODEL_NAME);
        assert_eq!(EMBEDDING_MODEL_FILENAME, DEFAULT_EMBEDDING_MODEL_FILENAME);
        assert_eq!(
            EMBEDDING_TOKENIZER_FILENAME,
            DEFAULT_EMBEDDING_TOKENIZER_FILENAME
        );

        // vector dimension
        assert_eq!(EMBEDDING_DIMENSIONS, DEFAULT_TEXT_EMBEDDING_DIM);
        assert_eq!(EMBEDDING_DIMENSIONS_I32, EMBEDDING_DIMENSIONS as i32);
        assert_eq!(EMBEDDING_DIMENSIONS, 384, "v4 MiniLM contract is 384-d");

        // model file matches the model name (MiniLM stem, not BGE / EmbeddingGemma)
        assert!(
            EMBEDDING_MODEL_FILENAME.contains("MiniLM"),
            "filename {EMBEDDING_MODEL_FILENAME} does not match MiniLM model id {EMBEDDING_MODEL_ID}"
        );

        // sequence length
        assert_eq!(EMBEDDING_MAX_SEQ_LEN, DEFAULT_EMBEDDING_MAX_SEQ_LEN);

        // current durable Lance table reflects the model+dim contract
        assert_eq!(MEMORIES_V4_TABLE, "memories_v4_minilm_384");
        assert!(
            MEMORIES_V4_TABLE.contains("minilm")
                && MEMORIES_V4_TABLE.contains(&EMBEDDING_DIMENSIONS.to_string()),
            "v4 table name {MEMORIES_V4_TABLE} must mention model + dim"
        );

        // v5 is a forward placeholder only — it must NOT collide with v4
        assert_ne!(MEMORIES_V5_TABLE, MEMORIES_V4_TABLE);
        assert!(
            MEMORIES_V5_TABLE.contains("v5"),
            "v5 placeholder {MEMORIES_V5_TABLE} should be tagged v5"
        );

        // storage::MEMORIES_TABLE re-exports the same write target
        assert_eq!(crate::storage::MEMORIES_TABLE, MEMORIES_V4_TABLE);
    }
}
