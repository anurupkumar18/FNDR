//! Embedding module using ONNX Runtime

mod chunking;
mod clip;
mod onnx;

pub use chunking::TextChunker;
pub use clip::ClipEmbedder;
pub use onnx::Embedder;
pub use onnx::EMBEDDING_DIM;
