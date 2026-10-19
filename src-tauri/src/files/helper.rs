//! The app's side of `fndr-files-helper` (ADR 033): starts it, waits for its
//! ready line, and trades one JSON line for one JSON line. The helper quits
//! when its stdin closes, so dropping a [`FilesHelper`] ends it; a helper
//! that dies or answers garbage fails the call, and the caller starts a new
//! one next time.
//!
//! Blocking: run it on a blocking thread, never on an async worker.

use super::indexer::Embedder;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// What the helper reported when it started.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HelperInfo {
    pub dimension: usize,
    pub vector_version: u32,
    pub llama: String,
    pub media: bool,
    pub load_seconds: f64,
}

pub struct FilesHelper {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    pub info: HelperInfo,
}

/// The helper binary: beside FNDR's own executable in the app bundle, or
/// staged in `src-tauri/binaries` by `make files-helper` for development.
pub fn locate() -> Option<PathBuf> {
    let triple_name = if cfg!(target_arch = "aarch64") {
        "fndr-files-helper-aarch64-apple-darwin"
    } else if cfg!(target_os = "linux") {
        "fndr-files-helper-x86_64-unknown-linux-gnu"
    } else {
        "fndr-files-helper-x86_64-apple-darwin"
    };
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        candidates.push(dir.join("fndr-files-helper"));
        candidates.push(dir.join(triple_name));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join(triple_name),
    );
    candidates.into_iter().find(|path| path.is_file())
}

impl FilesHelper {
    /// Starts `program` with the text model, and the media encoders when given.
    pub fn start(
        program: &Path,
        text_model: &Path,
        media_model: Option<&Path>,
    ) -> Result<Self, String> {
        let mut command = Command::new(program);
        command.arg("--text").arg(text_model);
        if let Some(media) = media_model {
            command.arg("--media").arg(media);
        }
        if std::env::var("FNDR_FILES_DEVICE").as_deref() == Ok("cpu") {
            command.arg("--cpu");
        }
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("can't start {}: {error}", program.display()))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let mut stdout = BufReader::new(child.stdout.take().ok_or("no stdout")?);
        let mut line = String::new();
        stdout
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        let ready: Value = serde_json::from_str(line.trim()).map_err(|_| {
            let _ = child.kill();
            format!("the Files helper did not start: {}", line.trim())
        })?;
        if ready.get("ready").and_then(Value::as_bool) != Some(true) {
            let _ = child.kill();
            let _ = child.wait();
            let why = ready
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(format!("the Files helper could not load the model: {why}"));
        }
        let info: HelperInfo = serde_json::from_value(ready).map_err(|error| error.to_string())?;
        if info.dimension != super::DIMENSION {
            let _ = child.kill();
            return Err(format!(
                "the model makes {}-d vectors, not {}",
                info.dimension,
                super::DIMENSION
            ));
        }
        Ok(Self {
            child,
            stdin,
            stdout,
            next_id: 1,
            info,
        })
    }

    fn call(&mut self, mut request: Value) -> Result<Vec<Vec<f32>>, String> {
        let id = self.next_id;
        self.next_id += 1;
        request["id"] = json!(id);
        writeln!(self.stdin, "{request}")
            .and_then(|_| self.stdin.flush())
            .map_err(|_| "the Files helper stopped".to_string())?;
        let mut line = String::new();
        let read = self
            .stdout
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            return Err("the Files helper stopped".into());
        }
        let reply: Value = serde_json::from_str(line.trim())
            .map_err(|_| "the Files helper answered garbage".to_string())?;
        if reply.get("id").and_then(Value::as_u64) != Some(id) {
            return Err("the Files helper answered out of turn".into());
        }
        if reply.get("ok").and_then(Value::as_bool) != Some(true) {
            return Err(reply
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
                .to_string());
        }
        serde_json::from_value(reply.get("vectors").cloned().unwrap_or(Value::Null))
            .map_err(|error| error.to_string())
    }

    /// A search: the model card's query prompt, or the code prompt for the code index.
    pub fn embed_query(&mut self, text: &str, code: bool) -> Result<Vec<f32>, String> {
        let prompt = if code {
            super::prompts::code_query(text)
        } else {
            super::prompts::query(text)
        };
        self.call(json!({ "op": "embed_texts", "texts": [prompt] }))?
            .pop()
            .ok_or_else(|| "no vector".to_string())
    }

    pub fn embed_image(&mut self, path: &Path, budget: u32) -> Result<Vec<f32>, String> {
        self.call(json!({ "op": "embed_image", "path": path, "budget": budget }))?
            .pop()
            .ok_or_else(|| "no vector".to_string())
    }

    pub fn embed_audio(&mut self, path: &Path) -> Result<Vec<f32>, String> {
        self.call(json!({ "op": "embed_audio", "path": path }))?
            .pop()
            .ok_or_else(|| "no vector".to_string())
    }
}

impl Embedder for FilesHelper {
    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.call(json!({ "op": "embed_texts", "texts": texts }))
    }
}

impl Drop for FilesHelper {
    fn drop(&mut self) {
        // Closing stdin asks it to quit; a helper stuck in a long encode is killed.
        let _ = self.stdin.flush();
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A stand-in for the helper speaking its protocol: every text becomes
    /// the unit vector on its length, so tests can check what came back.
    /// The real helper is checked by `make files-check`.
    fn fake(dir: &Path, ready: &str) -> PathBuf {
        let script = dir.join("fake-helper.py");
        std::fs::write(
            &script,
            format!(
                r#"#!/usr/bin/env python3
import json, sys
print(json.dumps({ready}), flush=True)
for line in sys.stdin:
    request = json.loads(line)
    if request["op"] == "embed_texts":
        vectors = []
        for text in request["texts"]:
            v = [0.0] * 768
            v[len(text) % 768] = 1.0
            vectors.append(v)
        print(json.dumps({{"id": request["id"], "ok": True, "vectors": vectors}}), flush=True)
    else:
        print(json.dumps({{"id": request["id"], "ok": False, "error": "not an image"}}), flush=True)
"#
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    const READY: &str = r#"{"ready": True, "dimension": 768, "vector_version": 1, "llama": "b11461", "media": False, "load_seconds": 0.5}"#;

    #[test]
    fn texts_and_queries_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let program = fake(dir.path(), READY);
        let mut helper = FilesHelper::start(&program, Path::new("text.gguf"), None).unwrap();
        assert_eq!(helper.info.llama, "b11461");
        let vectors = helper.embed_texts(&["ab".into(), "abcd".into()]).unwrap();
        assert_eq!(vectors.len(), 2);
        assert_eq!(vectors[1][4], 1.0);
        let query = helper.embed_query("dog", false).unwrap();
        assert_eq!(query["task: search result | query: dog".len()], 1.0);
        let error = helper.embed_image(Path::new("/x.png"), 280).unwrap_err();
        assert_eq!(error, "not an image");
    }

    #[test]
    fn a_helper_that_cannot_load_the_model_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let program = fake(
            dir.path(),
            r#"{"ready": False, "error": "can't load text.gguf"}"#,
        );
        let error = FilesHelper::start(&program, Path::new("text.gguf"), None)
            .err()
            .unwrap();
        assert!(error.contains("can't load text.gguf"), "{error}");
    }

    #[test]
    fn the_wrong_dimension_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let program = fake(dir.path(), &READY.replace("768", "1024"));
        assert!(FilesHelper::start(&program, Path::new("t"), None).is_err());
    }
}
