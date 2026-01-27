//! Embedding module using ONNX Runtime

mod chunking;
mod onnx;

pub use chunking::TextChunker;
pub use onnx::Embedder;
