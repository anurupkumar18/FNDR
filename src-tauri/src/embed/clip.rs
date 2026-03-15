//! Lightweight local image embedding helper.
//!
//! This provides a CLIP-compatible interface and deterministic output while
//! keeping the app fully local-first. Replace internals with a true CLIP model
//! runtime when integrating production embeddings.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Common CLIP embedding dimension.
pub const CLIP_EMBEDDING_DIM: usize = 512;

pub struct ClipEmbedder;

impl ClipEmbedder {
    pub fn new() -> Self {
        Self
    }

    /// Generate an image embedding from PNG bytes.
    pub fn embed_image(&self, image_bytes: &[u8]) -> Vec<f32> {
        if image_bytes.is_empty() {
            return vec![0.0; CLIP_EMBEDDING_DIM];
        }

        let mut embedding = vec![0.0f32; CLIP_EMBEDDING_DIM];

        let mut hasher = DefaultHasher::new();
        image_bytes.hash(&mut hasher);
        let global_hash = hasher.finish();

        let chunk_size = (image_bytes.len() / 32).max(1);
        let mut chunk_hashes = [0u64; 32];
        for (idx, chunk) in image_bytes.chunks(chunk_size).take(32).enumerate() {
            let mut h = DefaultHasher::new();
            chunk.hash(&mut h);
            chunk_hashes[idx] = h.finish();
        }

        for i in 0..CLIP_EMBEDDING_DIM {
            let g = ((global_hash.rotate_right((i % 64) as u32) & 0xFF) as f32) / 255.0;
            let local_hash = chunk_hashes[i % chunk_hashes.len()];
            let l = ((local_hash.rotate_left((i % 32) as u32) & 0xFF) as f32) / 255.0;
            embedding[i] = (g * 0.55) + (l * 0.45);
        }

        l2_normalize(embedding)
    }
}

impl Default for ClipEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

fn l2_normalize(mut values: Vec<f32>) -> Vec<f32> {
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut values {
            *value /= norm;
        }
    }
    values
}
