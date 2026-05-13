//! Text chunking and ONNX embedding generation for the memory pipeline.

mod chunking;
mod onnx;

pub use chunking::{chunk_screen_text, TextChunk, TextChunker};
pub use onnx::{
    embedding_runtime_status, Embedder, EmbeddingBackend, EmbeddingRuntimeStatus, EMBEDDING_DIM,
};
