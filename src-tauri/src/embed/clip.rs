//! CLIP-compatible image embedding via MobileCLIP-S0 ONNX model.
//!
//! Falls back to a deterministic hash stub when the ONNX model file is absent,
//! preserving startup without the model download.

use ort::session::Session;
use std::path::PathBuf;
use std::sync::Mutex;

/// Common CLIP embedding dimension.
pub const CLIP_EMBEDDING_DIM: usize = crate::config::DEFAULT_IMAGE_EMBEDDING_DIM;

// CLIP / MobileCLIP-S0 image preprocessing constants.
const CLIP_IMAGE_SIZE: u32 = 224;
const CLIP_MEAN: [f32; 3] = [0.48145466, 0.4578275, 0.40821073];
const CLIP_STD: [f32; 3] = [0.26862954, 0.26130258, 0.27577711];

pub struct ClipEmbedder {
    inner: Inner,
}

enum Inner {
    Real(RealClipEmbedder),
    Stub,
}

struct RealClipEmbedder {
    session: Mutex<Session>,
}

impl ClipEmbedder {
    pub fn new() -> Self {
        match RealClipEmbedder::new() {
            Ok(real) => {
                tracing::info!("Native ort CLIP image embedder initialized");
                Self {
                    inner: Inner::Real(real),
                }
            }
            Err(e) => {
                tracing::warn!("CLIP ONNX model not available — using hash stub. Reason: {e}");
                Self { inner: Inner::Stub }
            }
        }
    }

    /// Generate an image embedding from PNG/JPEG bytes.
    pub fn embed_image(&self, image_bytes: &[u8]) -> Vec<f32> {
        match &self.inner {
            Inner::Real(r) => r.embed_image(image_bytes).unwrap_or_else(|e| {
                tracing::warn!("CLIP inference failed, using hash stub: {e}");
                hash_embed(image_bytes)
            }),
            Inner::Stub => hash_embed(image_bytes),
        }
    }
}

impl Default for ClipEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl RealClipEmbedder {
    fn new() -> Result<Self, String> {
        let model_path =
            resolve_clip_model().ok_or_else(|| "MobileCLIP ONNX model not found".to_string())?;

        let session = Session::builder()
            .map_err(|e| format!("ort SessionBuilder error: {e}"))?
            .commit_from_file(&model_path)
            .map_err(|e| {
                format!(
                    "Failed to load CLIP model from {}: {e}",
                    model_path.display()
                )
            })?;

        Ok(Self {
            session: Mutex::new(session),
        })
    }

    fn embed_image(&self, image_bytes: &[u8]) -> Result<Vec<f32>, String> {
        if image_bytes.is_empty() {
            return Ok(vec![0.0; CLIP_EMBEDDING_DIM]);
        }

        // Decode and resize to 224×224 RGB.
        let img = image::load_from_memory(image_bytes)
            .map_err(|e| format!("Image decode failed: {e}"))?
            .resize_exact(
                CLIP_IMAGE_SIZE,
                CLIP_IMAGE_SIZE,
                image::imageops::FilterType::Triangle,
            )
            .to_rgb8();

        // Build NCHW tensor [1, 3, 224, 224] with CLIP normalization.
        let mut tensor =
            vec![0.0f32; 1 * 3 * (CLIP_IMAGE_SIZE as usize) * (CLIP_IMAGE_SIZE as usize)];
        let hw = (CLIP_IMAGE_SIZE * CLIP_IMAGE_SIZE) as usize;

        for (pixel_idx, pixel) in img.pixels().enumerate() {
            for c in 0..3 {
                let raw = pixel[c] as f32 / 255.0;
                let normalized = (raw - CLIP_MEAN[c]) / CLIP_STD[c];
                tensor[c * hw + pixel_idx] = normalized;
            }
        }

        let pixel_values = ndarray::Array4::<f32>::from_shape_vec(
            (1, 3, CLIP_IMAGE_SIZE as usize, CLIP_IMAGE_SIZE as usize),
            tensor,
        )
        .map_err(|e| format!("Tensor shape error: {e}"))?;

        let pixel_tensor = ort::value::Tensor::from_array(pixel_values)
            .map_err(|e| format!("Failed to create pixel_values tensor: {e}"))?;
        let mut session_guard = self
            .session
            .lock()
            .map_err(|e| format!("CLIP session mutex poisoned: {e}"))?;
        let outputs = session_guard
            .run(ort::inputs!["pixel_values" => pixel_tensor])
            .map_err(|e| format!("CLIP ONNX inference failed: {e}"))?;

        // Try common CLIP output names.
        let embed_key = ["image_embeds", "pooler_output", "output"]
            .iter()
            .find(|&&k| outputs.get(k).is_some())
            .copied()
            .ok_or("No recognized CLIP output key found")?;

        // ort 2.x RC: try_extract_tensor returns (Shape, &[T]).
        let (_shape, data) = outputs[embed_key]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Failed to extract CLIP output tensor: {e}"))?;

        if data.len() < CLIP_EMBEDDING_DIM {
            return Err(format!(
                "CLIP output dim {} < expected {}",
                data.len(),
                CLIP_EMBEDDING_DIM
            ));
        }

        let mut embedding = data[..CLIP_EMBEDDING_DIM].to_vec();
        l2_normalize(&mut embedding);
        Ok(embedding)
    }
}

/// Resolve path to MobileCLIP-S0 ONNX file.
fn resolve_clip_model() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("FNDR_MODEL_DIR") {
        let p = PathBuf::from(dir).join("mobileclip_s0.onnx");
        if p.exists() {
            return Some(p);
        }
    }

    if let Some(proj) = directories::ProjectDirs::from("com", "fndr", "FNDR") {
        let p = proj.data_dir().join("models/mobileclip_s0.onnx");
        if p.exists() {
            return Some(p);
        }
    }

    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/mobileclip_s0.onnx");
    if dev.exists() {
        return Some(dev);
    }

    None
}

/// Deterministic hash-based fallback — used when ONNX model is absent.
fn hash_embed(image_bytes: &[u8]) -> Vec<f32> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

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

    l2_normalize(&mut embedding);
    embedding
}

fn l2_normalize(values: &mut Vec<f32>) {
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in values.iter_mut() {
            *value /= norm;
        }
    }
}
