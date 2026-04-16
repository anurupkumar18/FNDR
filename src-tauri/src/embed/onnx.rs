//! Local text embedding backend for all-MiniLM-L6-v2 via native ONNX Runtime.

use super::TextChunker;
use ndarray::Array2;
use ort::session::Session;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// Embedding dimension for all-MiniLM-L6-v2.
pub const EMBEDDING_DIM: usize = 384;
/// Maximum token sequence length (matches model training config).
const MAX_SEQ_LEN: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbeddingBackend {
    Real,
    Mock,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRuntimeStatus {
    pub backend: String,
    pub degraded: bool,
    pub detail: String,
}

#[derive(Debug, Clone)]
struct EmbeddingRuntimeState {
    backend: String,
    degraded: bool,
    detail: String,
}

static EMBEDDING_RUNTIME_STATE: OnceLock<Mutex<EmbeddingRuntimeState>> = OnceLock::new();

fn runtime_state() -> &'static Mutex<EmbeddingRuntimeState> {
    EMBEDDING_RUNTIME_STATE.get_or_init(|| {
        Mutex::new(EmbeddingRuntimeState {
            backend: "unknown".to_string(),
            degraded: false,
            detail: "Embedder not initialized yet".to_string(),
        })
    })
}

fn set_runtime_state(backend: &str, degraded: bool, detail: impl Into<String>) {
    if let Ok(mut guard) = runtime_state().lock() {
        guard.backend = backend.to_string();
        guard.degraded = degraded;
        guard.detail = detail.into();
    }
}

pub fn embedding_runtime_status() -> EmbeddingRuntimeStatus {
    if let Ok(guard) = runtime_state().lock() {
        EmbeddingRuntimeStatus {
            backend: guard.backend.clone(),
            degraded: guard.degraded,
            detail: guard.detail.clone(),
        }
    } else {
        EmbeddingRuntimeStatus {
            backend: "unknown".to_string(),
            degraded: true,
            detail: "Embedding runtime state lock poisoned".to_string(),
        }
    }
}

/// Embedder with pluggable backend.
pub struct Embedder {
    chunker: TextChunker,
    backend: Backend,
    degraded_to_mock: AtomicBool,
}

enum Backend {
    Real(RealEmbedder),
    Mock(MockEmbedder),
}

impl Embedder {
    pub fn new() -> Result<Self, String> {
        let chunker = TextChunker::new();

        match RealEmbedder::new() {
            Ok(real) => {
                set_runtime_state("real", false, "MiniLM embedder ready");
                Ok(Self {
                    chunker,
                    backend: Backend::Real(real),
                    degraded_to_mock: AtomicBool::new(false),
                })
            }
            Err(err) => {
                if allow_mock_embedder() {
                    let reason =
                        format!("Semantic embeddings degraded to mock mode. Reason: {}", err);
                    tracing::warn!(
                        "MiniLM embedder fallback active: using MOCK embeddings. {}",
                        reason
                    );
                    set_runtime_state("mock", true, reason);
                    Ok(Self {
                        chunker,
                        backend: Backend::Mock(MockEmbedder::default()),
                        degraded_to_mock: AtomicBool::new(true),
                    })
                } else {
                    set_runtime_state(
                        "unavailable",
                        true,
                        format!(
                            "MiniLM embedder failed and mock fallback is disabled: {}",
                            err
                        ),
                    );
                    Err(format!(
                        "Failed to initialize real all-MiniLM-L6-v2 embedder and mock fallback is disabled: {err}"
                    ))
                }
            }
        }
    }

    pub fn backend(&self) -> EmbeddingBackend {
        if self.degraded_to_mock.load(Ordering::Relaxed) {
            return EmbeddingBackend::Mock;
        }

        match self.backend {
            Backend::Real(_) => EmbeddingBackend::Real,
            Backend::Mock(_) => EmbeddingBackend::Mock,
        }
    }

    /// Chunk text for embedding (char fallback path).
    pub fn chunk_text(&self, text: &str) -> Vec<String> {
        self.chunker.chunk(text)
    }

    /// Generate embeddings for a batch of texts.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut flattened_chunks = Vec::new();
        let mut ranges = Vec::with_capacity(texts.len());

        for text in texts {
            let chunks = self.chunk_text(text);
            let start = flattened_chunks.len();
            if chunks.is_empty() {
                flattened_chunks.push(text.clone());
            } else {
                flattened_chunks.extend(chunks);
            }
            let end = flattened_chunks.len();
            ranges.push((start, end));
        }

        let chunk_embeddings = self.backend_embed_batch(&flattened_chunks)?;
        if chunk_embeddings.len() != flattened_chunks.len() {
            return Err(format!(
                "Embedding backend returned {} vectors for {} chunks",
                chunk_embeddings.len(),
                flattened_chunks.len()
            ));
        }

        let mut merged = Vec::with_capacity(ranges.len());
        for (start, end) in ranges {
            let vectors = &chunk_embeddings[start..end];
            merged.push(mean_pool(vectors));
        }

        Ok(merged)
    }

    fn backend_embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        match &self.backend {
            Backend::Real(real) => {
                if self.degraded_to_mock.load(Ordering::Relaxed) {
                    return Ok(MockEmbedder.embed_batch(texts));
                }

                match real.embed_batch(texts) {
                    Ok(vectors) => Ok(vectors),
                    Err(err) => {
                        if allow_mock_embedder() {
                            self.degraded_to_mock.store(true, Ordering::Relaxed);
                            let detail = format!(
                                "Runtime embedding failure; switched to mock mode: {}",
                                err
                            );
                            tracing::warn!("{}", detail);
                            set_runtime_state("mock", true, detail);
                            Ok(MockEmbedder.embed_batch(texts))
                        } else {
                            set_runtime_state(
                                "unavailable",
                                true,
                                format!("Runtime embedding failure: {}", err),
                            );
                            Err(err)
                        }
                    }
                }
            }
            Backend::Mock(mock) => Ok(mock.embed_batch(texts)),
        }
    }
}

impl Default for Embedder {
    fn default() -> Self {
        Self::new().expect("Failed to create embedder")
    }
}

struct RealEmbedder {
    session: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
}

impl RealEmbedder {
    fn new() -> Result<Self, String> {
        let model_dir =
            resolve_model_dir().ok_or_else(|| "Could not determine model directory".to_string())?;

        let onnx_path = model_dir.join("all-MiniLM-L6-v2.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !onnx_path.exists() {
            return Err(format!("ONNX model not found at {}", onnx_path.display()));
        }
        if !tokenizer_path.exists() {
            return Err(format!(
                "Tokenizer not found at {}",
                tokenizer_path.display()
            ));
        }

        let session = Session::builder()
            .map_err(|e| format!("Failed to create ort session builder: {e}"))?
            .commit_from_file(&onnx_path)
            .map_err(|e| {
                format!(
                    "Failed to load ONNX model from {}: {e}",
                    onnx_path.display()
                )
            })?;

        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).map_err(|e| {
            format!(
                "Failed to load tokenizer from {}: {e}",
                tokenizer_path.display()
            )
        })?;

        tracing::info!(
            model = %onnx_path.display(),
            "Native ort text embedder initialized"
        );
        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
        })
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| format!("Tokenization failed: {e}"))?;

        let batch_size = texts.len();
        let seq_len = encodings
            .iter()
            .map(|e| e.get_ids().len())
            .max()
            .unwrap_or(0)
            .min(MAX_SEQ_LEN);

        if seq_len == 0 {
            return Ok(vec![vec![0.0f32; EMBEDDING_DIM]; batch_size]);
        }

        let mut input_ids = Array2::<i64>::zeros((batch_size, seq_len));
        let mut attention_mask = Array2::<i64>::zeros((batch_size, seq_len));
        let token_type_ids = Array2::<i64>::zeros((batch_size, seq_len));

        for (i, enc) in encodings.iter().enumerate() {
            let ids = enc.get_ids();
            let mask = enc.get_attention_mask();
            let len = ids.len().min(seq_len);
            for j in 0..len {
                input_ids[[i, j]] = ids[j] as i64;
                attention_mask[[i, j]] = mask[j] as i64;
            }
        }

        // Wrap ndarray arrays into ort Tensors (requires ndarray feature).
        // Clone attention_mask for mean-pooling after ownership is transferred to the session.
        let attention_mask_pooling = attention_mask.clone();
        let ids_t = ort::value::Tensor::from_array(input_ids)
            .map_err(|e| format!("Failed to create input_ids tensor: {e}"))?;
        let mask_t = ort::value::Tensor::from_array(attention_mask)
            .map_err(|e| format!("Failed to create attention_mask tensor: {e}"))?;
        let types_t = ort::value::Tensor::from_array(token_type_ids)
            .map_err(|e| format!("Failed to create token_type_ids tensor: {e}"))?;
        let mut session_guard = self
            .session
            .lock()
            .map_err(|e| format!("Session mutex poisoned: {e}"))?;
        let outputs = session_guard
            .run(ort::inputs![
                "input_ids" => ids_t,
                "attention_mask" => mask_t,
                "token_type_ids" => types_t,
            ])
            .map_err(|e| format!("ONNX inference failed: {e}"))?;

        // ort 2.x RC: try_extract_tensor returns (Shape, &[T]).
        let (shape, data) = outputs["last_hidden_state"]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Failed to extract hidden state tensor: {e}"))?;

        // shape: [batch, seq_len, EMBEDDING_DIM]
        let actual_seq = shape.get(1).copied().unwrap_or(seq_len as i64) as usize;
        let actual_dim = shape.get(2).copied().unwrap_or(EMBEDDING_DIM as i64) as usize;
        if actual_dim != EMBEDDING_DIM {
            return Err(format!(
                "Unexpected hidden state dim {actual_dim}, expected {EMBEDDING_DIM}"
            ));
        }

        let mut embeddings = Vec::with_capacity(batch_size);
        for i in 0..batch_size {
            let mut sum = vec![0.0f32; EMBEDDING_DIM];
            let mut count = 0.0f32;
            for j in 0..actual_seq {
                let mask_j = j.min(seq_len - 1);
                if attention_mask_pooling[[i, mask_j]] > 0 {
                    let offset = (i * actual_seq + j) * EMBEDDING_DIM;
                    for k in 0..EMBEDDING_DIM {
                        sum[k] += data[offset + k];
                    }
                    count += 1.0;
                }
            }
            if count > 0.0 {
                for v in &mut sum {
                    *v /= count;
                }
            }
            normalize(&mut sum);
            embeddings.push(sum);
        }
        Ok(embeddings)
    }
}

#[derive(Debug, Default)]
struct MockEmbedder;

impl MockEmbedder {
    fn embed_batch(&self, texts: &[String]) -> Vec<Vec<f32>> {
        texts.iter().map(|text| self.embed_single(text)).collect()
    }

    fn embed_single(&self, text: &str) -> Vec<f32> {
        // Feature-hashing bag-of-words fallback for dev/test only.
        let mut vector = vec![0.0f32; EMBEDDING_DIM];
        let lower = text.to_lowercase();

        for token in lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|tok| tok.len() > 2)
        {
            let idx = stable_hash(token) % EMBEDDING_DIM;
            vector[idx] += 1.0;

            if token.len() > 4 {
                let prefix = &token[..3];
                let suffix = &token[token.len() - 3..];
                vector[stable_hash(prefix) % EMBEDDING_DIM] += 0.4;
                vector[stable_hash(suffix) % EMBEDDING_DIM] += 0.4;
            }
        }

        for window in lower.as_bytes().windows(3) {
            let idx = stable_hash_bytes(window) % EMBEDDING_DIM;
            vector[idx] += 0.05;
        }

        normalize(&mut vector);
        vector
    }
}

fn allow_mock_embedder() -> bool {
    if let Ok(value) = std::env::var("FNDR_ALLOW_MOCK_EMBEDDER") {
        return parse_env_bool(&value);
    }

    if let Ok(value) = std::env::var("FNDR_DISABLE_MOCK_EMBEDDER") {
        if parse_env_bool(&value) {
            return false;
        }
    }

    true
}

fn parse_env_bool(value: &str) -> bool {
    value == "1"
        || value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
}

/// Resolve the directory containing ONNX model files.
/// Checks (in order): env var override, standard app data dir, dev CARGO_MANIFEST_DIR fallback.
fn resolve_model_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("FNDR_MODEL_DIR") {
        let p = PathBuf::from(dir);
        if p.exists() {
            return Some(p);
        }
    }

    if let Some(proj) = directories::ProjectDirs::from("com", "fndr", "FNDR") {
        let data_models = proj.data_dir().join("models");
        if data_models.exists() {
            return Some(data_models);
        }
    }

    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models");
    if dev.exists() {
        return Some(dev);
    }

    None
}

fn stable_hash(input: &str) -> usize {
    stable_hash_bytes(input.as_bytes())
}

fn stable_hash_bytes(input: &[u8]) -> usize {
    let mut hash: u64 = 1469598103934665603; // FNV offset
    for b in input {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash as usize
}

fn mean_pool(vectors: &[Vec<f32>]) -> Vec<f32> {
    if vectors.is_empty() {
        return vec![0.0; EMBEDDING_DIM];
    }

    let mut pooled = vec![0.0f32; EMBEDDING_DIM];
    for vec in vectors {
        for (idx, value) in vec.iter().enumerate().take(EMBEDDING_DIM) {
            pooled[idx] += *value;
        }
    }

    let scale = 1.0 / vectors.len() as f32;
    for value in &mut pooled {
        *value *= scale;
    }

    normalize(&mut pooled);
    pooled
}

fn normalize(vec: &mut [f32]) {
    let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for val in vec {
            *val /= norm;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cosine(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
    }

    #[test]
    fn similar_phrases_score_higher_than_unrelated() {
        let embedder = Embedder::new().expect("embedder should initialize in tests");
        let phrases = vec![
            "schedule project kickoff meeting with alice".to_string(),
            "plan kickoff meeting with alice for the project".to_string(),
            "buy groceries and cook dinner tonight".to_string(),
        ];
        let embeddings = embedder
            .embed_batch(&phrases)
            .expect("embedding should work");

        let similar = cosine(&embeddings[0], &embeddings[1]);
        let unrelated = cosine(&embeddings[0], &embeddings[2]);

        assert!(
            similar > unrelated,
            "expected similar phrases ({similar}) to outrank unrelated ({unrelated})"
        );
    }
}
