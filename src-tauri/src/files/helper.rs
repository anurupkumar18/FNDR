//! The app's side of `fndr-files-helper` (ADR 033): EmbeddingGemma 2 in a
//! process of its own, spoken to over [`LineProcess`].

use super::indexer::Embedder;
use super::process::LineProcess;
use serde_json::{json, Value};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

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
    process: LineProcess,
    pub info: HelperInfo,
}

pub fn locate() -> Option<PathBuf> {
    super::process::locate("fndr-files-helper")
}

impl FilesHelper {
    /// Starts `program` with the text model, and the media encoders when given.
    pub fn start(
        program: &Path,
        text_model: &Path,
        media_model: Option<&Path>,
    ) -> Result<Self, String> {
        let mut args: Vec<&OsStr> = vec![OsStr::new("--text"), text_model.as_os_str()];
        if let Some(media) = media_model {
            args.extend([OsStr::new("--media"), media.as_os_str()]);
        }
        if std::env::var("FNDR_FILES_DEVICE").as_deref() == Ok("cpu") {
            args.push(OsStr::new("--cpu"));
        }
        let (process, ready) = LineProcess::start("Files helper", program, &args)?;
        let info: HelperInfo = serde_json::from_value(ready).map_err(|error| error.to_string())?;
        if info.dimension != super::DIMENSION {
            return Err(format!(
                "the model makes {}-d vectors, not {}",
                info.dimension,
                super::DIMENSION
            ));
        }
        Ok(Self { process, info })
    }

    fn vectors(&mut self, request: Value) -> Result<Vec<Vec<f32>>, String> {
        let reply = self.process.call(request)?;
        serde_json::from_value(reply.get("vectors").cloned().unwrap_or(Value::Null))
            .map_err(|error| error.to_string())
    }

    fn one(&mut self, request: Value) -> Result<Vec<f32>, String> {
        self.vectors(request)?
            .pop()
            .ok_or_else(|| "no vector".to_string())
    }

    /// A search: the model card's query prompt, or its code prompt for the code index.
    pub fn embed_query(&mut self, text: &str, code: bool) -> Result<Vec<f32>, String> {
        let prompt = if code {
            super::prompts::code_query(text)
        } else {
            super::prompts::query(text)
        };
        self.one(json!({ "op": "embed_texts", "texts": [prompt] }))
    }
}

impl Embedder for FilesHelper {
    fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.vectors(json!({ "op": "embed_texts", "texts": texts }))
    }

    fn embed_image(&mut self, path: &Path, budget: u32) -> Result<Vec<f32>, String> {
        self.one(json!({ "op": "embed_image", "path": path, "budget": budget }))
    }

    fn embed_audio(&mut self, path: &Path) -> Result<Vec<f32>, String> {
        self.one(json!({ "op": "embed_audio", "path": path }))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::super::process::tests::fake;
    use super::*;

    const READY: &str = r#"{"ready": True, "dimension": 768, "vector_version": 1, "llama": "b11461", "media": False, "load_seconds": 0.5}"#;
    /// Each text becomes the unit vector on its length, so tests can see what was sent.
    const ANSWER: &str = r#"{"ok": True, "vectors": [[1.0 if i == len(t) % 768 else 0.0 for i in range(768)] for t in request.get("texts", [])]} if request["op"] == "embed_texts" else {"ok": False, "error": "not an image"}"#;

    #[test]
    fn texts_and_queries_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let program = fake(dir.path(), "helper", READY, ANSWER);
        let mut helper = FilesHelper::start(&program, Path::new("text.gguf"), None).unwrap();
        assert_eq!(helper.info.llama, "b11461");
        let vectors = helper.embed_texts(&["ab".into(), "abcd".into()]).unwrap();
        assert_eq!(vectors.len(), 2);
        assert_eq!(vectors[1][4], 1.0);
        let query = helper.embed_query("dog", false).unwrap();
        assert_eq!(query["task: search result | query: dog".len()], 1.0);
        assert_eq!(
            helper.embed_image(Path::new("/x.png"), 280).unwrap_err(),
            "not an image"
        );
    }

    #[test]
    fn the_wrong_dimension_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let program = fake(dir.path(), "helper", &READY.replace("768", "1024"), ANSWER);
        assert!(FilesHelper::start(&program, Path::new("t"), None).is_err());
    }
}
