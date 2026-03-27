use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use parking_lot::Mutex;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

mod vlm;

/// Global shared LlamaBackend singleton.
/// Both InferenceEngine and VlmEngine must share one backend instance
/// to avoid BackendAlreadyInitialized panics from Metal/CPU init.
static LLAMA_BACKEND: OnceLock<Arc<LlamaBackend>> = OnceLock::new();

pub fn get_or_init_backend() -> Result<Arc<LlamaBackend>, Box<dyn std::error::Error + Send + Sync>>
{
    if let Some(backend) = LLAMA_BACKEND.get() {
        return Ok(Arc::clone(backend));
    }
    let backend = Arc::new(LlamaBackend::init()?);
    // If another thread raced us, that's fine – just return our copy
    let _ = LLAMA_BACKEND.set(Arc::clone(&backend));
    Ok(backend)
}
pub use vlm::VlmEngine;

/// AI Inference Engine for FNDR using llama-cpp-2
/// Persists the LlamaContext to prevent Metal resource exhaustion crashes.
pub struct InferenceEngine {
    model: &'static LlamaModel,
    context: Mutex<LlamaContext<'static>>,
    _backend: Arc<LlamaBackend>,
}

unsafe impl Send for InferenceEngine {}
unsafe impl Sync for InferenceEngine {}

impl InferenceEngine {
    /// Initialize the engine (uses Meta Llama 3.2 1B)
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!("Initializing local LLM via llama-cpp (Llama 3.2 1B)...");

        let backend = get_or_init_backend()?;

        // Try multiple locations for model file (dev vs release)
        let model_name = "gemma-3-4b-it-q4_0.gguf";
        let possible_paths = vec![
            // Dev mode: relative to src-tauri
            PathBuf::from(format!("models/{}", model_name)),
            // Dev mode: relative to project root
            PathBuf::from(format!("src-tauri/models/{}", model_name)),
            // Release: next to executable
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("models").join(model_name)))
                .unwrap_or_default(),
            // Release: in Resources folder (macOS bundle)
            std::env::current_exe()
                .ok()
                .and_then(|p| {
                    p.parent()
                        .map(|d| d.join("../Resources/models").join(model_name))
                })
                .unwrap_or_default(),
            // Absolute fallback
            dirs::data_dir()
                .unwrap_or_default()
                .join("fndr/models")
                .join(model_name),
        ];

        let model_path = possible_paths
            .into_iter()
            .find(|p| p.exists())
            .ok_or_else(|| {
                tracing::error!(
                    "Model file not found in any location. AI features will be disabled."
                );
                "Model file missing. Run ./download_model.sh to get the model."
            })?;

        tracing::info!("Loading model from {:?}", model_path);

        let model_params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(&backend, &model_path, &model_params)?;

        // Leak the model to get a 'static reference, allowing the context to be 'static.
        // This is safe since InferenceEngine is a singleton for the application lifetime.
        let model_ref: &'static LlamaModel = Box::leak(Box::new(model));

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(2048))
            .with_n_batch(8192);

        let context = model_ref.new_context(&backend, ctx_params)?;

        Ok(Self {
            model: model_ref,
            context: Mutex::new(context),
            _backend: backend,
        })
    }

    /// Summarize noisy OCR text into a clean sentence
    pub async fn summarize(&self, ocr_text: &str) -> String {
        if ocr_text.trim().is_empty() {
            return String::new();
        }

        let prompt = format!(
            "<start_of_turn>user\nOne sentence summary. Start with action.\n\n{}<end_of_turn>\n<start_of_turn>model\n",
            ocr_text.chars().take(800).collect::<String>()
        );

        self.complete(&prompt, 40).await
    }

    /// Answer contextual questions using retrieved memories (RAG)
    pub async fn answer(&self, question: &str, context_str: &str) -> String {
        let prompt = format!(
            "<start_of_turn>user\nAnswer directly. No preamble.\n\nContext:\n{}\n\nQ: {}<end_of_turn>\n<start_of_turn>model\n",
            context_str.chars().take(1000).collect::<String>(), question
        );

        self.complete(&prompt, 150).await
    }

    /// Provide a detailed summary of a memory, extracting key information
    pub async fn summarize_memory_detail(
        &self,
        app_name: &str,
        window_title: &str,
        text: &str,
    ) -> String {
        if text.trim().is_empty() {
            return "No content to summarize.".to_string();
        }

        let prompt = format!(
            "<start_of_turn>user\nExtract key info.\nACTIVITY: what user was doing\nDETAILS: names, dates, numbers\n\n{}: {}\n{}<end_of_turn>\n<start_of_turn>model\nACTIVITY: ",
            app_name, window_title, text.chars().take(1000).collect::<String>()
        );

        self.complete(&prompt, 150).await
    }

    /// Synthesize multiple search results into a coherent summary
    pub async fn summarize_search_results(&self, query: &str, results: &[String]) -> String {
        if results.is_empty() {
            return String::new();
        }

        let combined_text = results.join("\n---\n");
        let prompt = format!(
            "<start_of_turn>user\nCombine into one paragraph. Max 40 words.\n\nQuery: {}\nSnippets:\n{}<end_of_turn>\n<start_of_turn>model\n",
            query, combined_text.chars().take(800).collect::<String>()
        );

        self.complete(&prompt, 100).await
    }

    /// Extract actionable todos/reminders from memory text
    pub async fn extract_todos(&self, memories_text: &str) -> String {
        if memories_text.trim().is_empty() {
            return String::new();
        }

        let prompt = format!(
            "<start_of_turn>user\nExtract tasks from screen captures. Format:\n- TODO: [task]\n- REMINDER: [time-based item]\nMax 5 items. Only clear actions.\n\n{}<end_of_turn>\n<start_of_turn>model\n",
            memories_text.chars().take(2000).collect::<String>()
        );

        self.complete(&prompt, 200).await
    }

    async fn complete(&self, prompt: &str, max_tokens: i32) -> String {
        let mut ctx = self.context.lock();

        // Clear KV cache (kv_cache_clear or just reset)
        // In llama-cpp-2 wrapper, context management is through KvCache
        ctx.clear_kv_cache();

        // Use AddBos::Always because our prompt template doesn't explicitly include the <bos> token
        let tokens_list = match self.model.str_to_token(prompt, AddBos::Always) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("Tokenization failed: {}", e);
                return "AI Error: Tokenization failed.".to_string();
            }
        };

        let mut batch = LlamaBatch::new(2048, 1);
        for (i, token) in tokens_list.iter().enumerate() {
            let last = i == tokens_list.len() - 1;
            let _ = batch.add(*token, i as i32, &[0], last);
        }

        if let Err(e) = ctx.decode(&mut batch) {
            tracing::error!("Decode failed: {}", e);
            return "AI Error: LLM Decode failed.".to_string();
        }

        let mut result = String::new();
        let mut n_cur = tokens_list.len() as i32;

        for _ in 0..max_tokens {
            let candidates = ctx.candidates();
            let token_data = candidates
                .max_by(|a, b| {
                    a.logit()
                        .partial_cmp(&b.logit())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap();

            let token = token_data.id();

            if self.model.is_eog_token(token) {
                break;
            }

            let piece = match self.model.token_to_str(token, Special::Plaintext) {
                Ok(s) => s,
                Err(_) => String::new(),
            };
            result.push_str(&piece);

            batch.clear();
            let _ = batch.add(token, n_cur, &[0], true);
            if let Err(e) = ctx.decode(&mut batch) {
                tracing::error!("Incremental decode failed: {}", e);
                break;
            }
            n_cur += 1;
        }

        result.trim().to_string()
    }
}
