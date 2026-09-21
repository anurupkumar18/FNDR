//! Local LLM call traces. One JSON line per model call, appended to a bounded file.
//!
//! By default a trace stores a hash and lengths, never prompt or output text. Full text is written only
//! when `include_content` is true (a developer switch), because prompts contain captured screen text.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_TRACE_BYTES: u64 = 5 * 1024 * 1024;

tokio::task_local! {
    static LLM_TASK: (&'static str, &'static str);
}

/// Run `fut` with a task label and prompt version that traces recorded inside it will carry.
pub async fn with_task<F: Future>(task: &'static str, version: &'static str, fut: F) -> F::Output {
    LLM_TASK.scope((task, version), fut).await
}

pub fn current_task() -> (&'static str, &'static str) {
    LLM_TASK.try_with(|t| *t).unwrap_or(("unlabeled", "v0"))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmTrace {
    pub ts_ms: i64,
    pub task: String,
    pub prompt_version: String,
    pub model_id: String,
    pub prompt_tokens: u32,
    pub output_tokens: u32,
    /// The generation cap for this call; `output_tokens >= max_tokens` means the output was cut off.
    pub max_tokens: u32,
    pub latency_ms: u64,
    pub prompt_sha256: String,
    pub output_len: usize,
    pub validator: String,
    pub prompt: Option<String>,
    pub output: Option<String>,
}

pub struct TraceInput<'a> {
    pub ts_ms: i64,
    pub task: &'a str,
    pub prompt_version: &'a str,
    pub model_id: &'a str,
    pub prompt_tokens: u32,
    pub output_tokens: u32,
    pub max_tokens: u32,
    pub latency_ms: u64,
    pub prompt: &'a str,
    pub output: &'a str,
    pub validator: &'a str,
}

pub fn build_trace(input: &TraceInput<'_>, include_content: bool) -> LlmTrace {
    LlmTrace {
        ts_ms: input.ts_ms,
        task: input.task.to_string(),
        prompt_version: input.prompt_version.to_string(),
        model_id: input.model_id.to_string(),
        prompt_tokens: input.prompt_tokens,
        output_tokens: input.output_tokens,
        max_tokens: input.max_tokens,
        latency_ms: input.latency_ms,
        prompt_sha256: format!("{:x}", Sha256::digest(input.prompt.as_bytes())),
        output_len: input.output.chars().count(),
        validator: input.validator.to_string(),
        prompt: include_content.then(|| input.prompt.to_string()),
        output: include_content.then(|| input.output.to_string()),
    }
}

/// Append one trace. When the file exceeds the cap it is renamed to `<name>.jsonl.1` (one generation kept).
pub fn append_trace(path: &Path, trace: &LlmTrace) -> std::io::Result<()> {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() >= MAX_TRACE_BYTES {
            let mut rotated: PathBuf = path.to_path_buf();
            rotated.set_extension("jsonl.1");
            std::fs::rename(path, rotated)?;
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(trace).map_err(std::io::Error::other)?;
    writeln!(file, "{line}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>() -> TraceInput<'a> {
        TraceInput {
            ts_ms: 1,
            task: "memory_extraction",
            prompt_version: "v3",
            model_id: "qwen3-vl-2b",
            prompt_tokens: 1200,
            output_tokens: 180,
            max_tokens: 400,
            latency_ms: 950,
            prompt: "secret OCR text about a bank",
            output: "{\"topic\":\"x\"}",
            validator: "ok",
        }
    }

    #[test]
    fn default_trace_never_contains_text() {
        let t = build_trace(&input(), false);
        let json = serde_json::to_string(&t).unwrap();
        assert!(!json.contains("secret OCR"));
        assert!(t.prompt.is_none() && t.output.is_none());
        assert_eq!(t.output_len, 13);
        assert_eq!(t.prompt_sha256.len(), 64);
    }

    #[test]
    fn content_is_included_only_on_request() {
        let t = build_trace(&input(), true);
        assert_eq!(t.prompt.as_deref(), Some("secret OCR text about a bank"));
    }

    #[test]
    fn append_writes_one_parseable_line_per_trace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("llm_traces.jsonl");
        append_trace(&path, &build_trace(&input(), false)).unwrap();
        append_trace(&path, &build_trace(&input(), false)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        let parsed: LlmTrace = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed.task, "memory_extraction");
        assert_eq!(parsed.max_tokens, 400);
    }

    #[test]
    fn oversized_file_rotates_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("llm_traces.jsonl");
        std::fs::write(&path, vec![b'x'; MAX_TRACE_BYTES as usize]).unwrap();
        append_trace(&path, &build_trace(&input(), false)).unwrap();
        assert!(dir.path().join("llm_traces.jsonl.1").exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 1);
    }

    #[tokio::test]
    async fn task_label_is_visible_inside_scope_and_defaults_outside() {
        assert_eq!(current_task(), ("unlabeled", "v0"));
        let inside = with_task("card_synthesis", "v2", async { current_task() }).await;
        assert_eq!(inside, ("card_synthesis", "v2"));
    }
}
