//! Embedding module using ONNX Runtime

mod chunking;
mod clip;
mod onnx;

pub use chunking::TextChunker;
pub use clip::ClipEmbedder;
pub use onnx::{
    embedding_runtime_status, Embedder, EmbeddingBackend, EmbeddingRuntimeStatus, EMBEDDING_DIM,
};
