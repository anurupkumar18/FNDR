//! ONNX Runtime embedder using MiniLM
//!
//! Uses the all-MiniLM-L6-v2 model for generating embeddings.
//! For prototype, we use a simplified tokenization approach.

use super::TextChunker;
// use std::path::PathBuf;

/// Embedding dimension for MiniLM
pub const EMBEDDING_DIM: usize = 384;

/// Embedder using ONNX Runtime
/// For the prototype, we use a placeholder that generates deterministic embeddings
/// based on text hash. This allows the search infrastructure to work while
/// we integrate the full ONNX model.
pub struct Embedder {
    chunker: TextChunker,
}

impl Embedder {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            chunker: TextChunker::new(),
        })
    }

    /// Chunk text for embedding
    pub fn chunk_text(&self, text: &str) -> Vec<String> {
        self.chunker.chunk(text)
    }

    /// Generate embeddings for a batch of texts
    /// For prototype: generates deterministic embeddings based on text content
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let embeddings: Vec<Vec<f32>> = texts.iter().map(|text| self.embed_single(text)).collect();

        Ok(embeddings)
    }

    /// Generate embedding for a single text
    /// Uses a deterministic hash-based approach for the prototype
    fn embed_single(&self, text: &str) -> Vec<f32> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut embedding = vec![0.0f32; EMBEDDING_DIM];

        // Generate embedding based on text characteristics
        // This creates a pseudo-embedding that clusters similar texts together

        // 1. Hash-based components
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        let hash = hasher.finish();

        // 2. Character frequency components
        let char_freqs: Vec<f32> = (b'a'..=b'z')
            .map(|c| {
                let count = text
                    .to_lowercase()
                    .chars()
                    .filter(|&ch| ch == c as char)
                    .count();
                count as f32 / (text.len().max(1) as f32)
            })
            .collect();

        // 3. Word-based components
        let words: Vec<&str> = text.split_whitespace().collect();
        let word_count = words.len() as f32;
        let avg_word_len = if words.is_empty() {
            0.0
        } else {
            words.iter().map(|w| w.len()).sum::<usize>() as f32 / word_count
        };

        // 4. Combine components into embedding
        for i in 0..EMBEDDING_DIM {
            let hash_component = ((hash >> (i % 64)) & 0xFF) as f32 / 255.0;
            let char_component = char_freqs.get(i % 26).copied().unwrap_or(0.0);
            let word_component = if i < 10 {
                (word_count / 100.0).min(1.0)
            } else if i < 20 {
                (avg_word_len / 10.0).min(1.0)
            } else {
                0.0
            };

            embedding[i] = hash_component * 0.5 + char_component * 0.3 + word_component * 0.2;
        }

        // L2 normalize
        let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut embedding {
                *val /= norm;
            }
        }

        embedding
    }
}

impl Default for Embedder {
    fn default() -> Self {
        Self::new().expect("Failed to create embedder")
    }
}
