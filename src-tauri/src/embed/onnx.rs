//! Local text embedding backend for all-MiniLM-L6-v2.

use super::TextChunker;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

/// Embedding dimension for all-MiniLM-L6-v2.
pub const EMBEDDING_DIM: usize = 384;
const SIDECAR_HEALTHCHECK_TIMEOUT: Duration = Duration::from_secs(30);
const SIDECAR_EMBED_TIMEOUT: Duration = Duration::from_secs(30);
const HEALTHCHECK_PROBE_TEXT: &str = "fndr embedding smoke check";

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

#[derive(Debug)]
struct RealEmbedder {
    python_cmd: PathBuf,
    script_path: PathBuf,
}

#[derive(Debug, Serialize)]
struct EmbedRequest<'a> {
    texts: &'a [String],
}

#[derive(Debug, Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

impl RealEmbedder {
    fn new() -> Result<Self, String> {
        let python_cmd = python_cmd_for_sidecar();
        let script_path = resolve_embedder_sidecar()
            .ok_or_else(|| "Could not locate minilm_embedder.py sidecar".to_string())?;

        let embedder = Self {
            python_cmd,
            script_path,
        };
        embedder.healthcheck()?;
        Ok(embedder)
    }

    fn healthcheck(&self) -> Result<(), String> {
        let probe = vec![HEALTHCHECK_PROBE_TEXT.to_string()];
        let payload = serde_json::to_vec(&EmbedRequest { texts: &probe })
            .map_err(|e| format!("Failed to serialize embedder healthcheck payload: {e}"))?;
        let output = self.run_sidecar(
            &["--embed-daemon"],
            Some(&payload),
            SIDECAR_HEALTHCHECK_TIMEOUT,
        )?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!(
                "Embedding sidecar healthcheck failed (status={}): {}",
                output.status, stderr
            ));
        }

        let response: EmbedResponse = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Embedding sidecar healthcheck parse failed: {e}"))?;
        let first = response
            .embeddings
            .first()
            .ok_or_else(|| "Embedding sidecar healthcheck returned no vectors".to_string())?;
        if first.len() != EMBEDDING_DIM {
            return Err(format!(
                "Embedding sidecar healthcheck returned dim {}, expected {}",
                first.len(),
                EMBEDDING_DIM
            ));
        }
        let norm = first.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm <= 0.0 {
            return Err("Embedding sidecar healthcheck returned an all-zero vector".to_string());
        }

        Ok(())
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        let payload = serde_json::to_vec(&EmbedRequest { texts })
            .map_err(|e| format!("Failed to serialize embedding request: {e}"))?;

        let output =
            self.run_sidecar(&["--embed-daemon"], Some(&payload), SIDECAR_EMBED_TIMEOUT)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(format!(
                "Embedding sidecar failed (status={}): {}",
                output.status, stderr
            ));
        }

        let response: EmbedResponse = serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Failed to parse embedding output: {e}"))?;

        for (idx, vec) in response.embeddings.iter().enumerate() {
            if vec.len() != EMBEDDING_DIM {
                return Err(format!(
                    "Embedding {} had dim {}, expected {}",
                    idx,
                    vec.len(),
                    EMBEDDING_DIM
                ));
            }
        }

        Ok(response.embeddings)
    }

    fn run_sidecar(
        &self,
        args: &[&str],
        stdin_payload: Option<&[u8]>,
        timeout: Duration,
    ) -> Result<std::process::Output, String> {
        let mut command = Command::new(&self.python_cmd);
        command
            .arg(&self.script_path)
            .args(args)
            .env("TOKENIZERS_PARALLELISM", "false")
            .env(
                "FNDR_EMBEDDER_REQUEST_TIMEOUT_SEC",
                format!("{:.3}", timeout.as_secs_f64()),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command.spawn().map_err(|e| {
            format!(
                "Failed to start embedding sidecar (python={} script={}): {e}",
                self.python_cmd.display(),
                self.script_path.display()
            )
        })?;

        if let Some(payload) = stdin_payload {
            if let Some(stdin) = child.stdin.as_mut() {
                stdin
                    .write_all(payload)
                    .map_err(|e| format!("Failed to write embedding input: {e}"))?;
            }
        }

        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => {
                    return child
                        .wait_with_output()
                        .map_err(|e| format!("Embedding sidecar execution failed: {e}"));
                }
                Ok(None) => {
                    if start.elapsed() > timeout {
                        let _ = child.kill();
                        let output = child.wait_with_output().map_err(|e| {
                            format!("Embedding sidecar timed out and kill wait failed: {e}")
                        })?;
                        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                        tracing::warn!(
                            timeout_ms = timeout.as_millis(),
                            stderr = %stderr,
                            "Embedding sidecar timed out"
                        );
                        return Err(format!(
                            "Embedding sidecar timed out after {}ms (stderr: {})",
                            timeout.as_millis(),
                            if stderr.is_empty() {
                                "<empty>"
                            } else {
                                &stderr
                            }
                        ));
                    }
                    thread::sleep(Duration::from_millis(10));
                }
                Err(err) => {
                    return Err(format!(
                        "Embedding sidecar process status check failed: {err}"
                    ));
                }
            }
        }
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

fn python_cmd_for_sidecar() -> PathBuf {
    if let Some(docs) = dirs::document_dir() {
        let venv_py = docs.join("FNDR Meetings/venv/bin/python3");
        if venv_py.exists() {
            return venv_py;
        }
    }
    PathBuf::from("python3")
}

fn resolve_embedder_sidecar() -> Option<PathBuf> {
    // Packaged: <exe>/../Resources/sidecar/minilm_embedder.py
    let packaged = std::env::current_exe().ok().and_then(|p| {
        p.parent()
            .map(|d| d.join("../Resources/sidecar/minilm_embedder.py"))
    });
    if let Some(ref p) = packaged {
        if p.exists() {
            return Some(p.clone());
        }
    }

    // Dev path.
    let dev = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sidecar/minilm_embedder.py");
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
