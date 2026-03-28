use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaChatTemplate, LlamaModel, Special};
use parking_lot::Mutex;
use std::num::NonZeroU32;
use std::path::{Path, PathBuf};
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
    chat_template: LlamaChatTemplate,
    model_id: String,
    model_path: PathBuf,
}

unsafe impl Send for InferenceEngine {}
unsafe impl Sync for InferenceEngine {}

impl InferenceEngine {
    /// Initialize the engine using the preferred available local model.
    pub async fn new(
        app_data_dir: Option<PathBuf>,
        preferred_model_id: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!("Initializing local LLM via llama-cpp...");

        let backend = get_or_init_backend()?;

        let resolved_model =
            crate::models::resolve_model(preferred_model_id.as_deref(), app_data_dir.as_deref())
                .ok_or_else(|| {
                    let searched_dirs =
                        crate::models::candidate_model_dirs(app_data_dir.as_deref())
                            .into_iter()
                            .map(|path| path.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                    tracing::error!(
                        "Model file not found in any known location. Searched: {}",
                        searched_dirs
                    );
                    format!("Model file missing. Searched: {}", searched_dirs)
                })?;

        let model_id = resolved_model.definition.id.to_string();
        let model_path = resolved_model.path;

        tracing::info!("Loading model {} from {:?}", model_id, model_path);

        let model_params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(&backend, &model_path, &model_params)?;

        // Leak the model to get a 'static reference, allowing the context to be 'static.
        // This is safe since InferenceEngine is a singleton for the application lifetime.
        let model_ref: &'static LlamaModel = Box::leak(Box::new(model));
        let chat_template = match model_ref.chat_template(None) {
            Ok(template) => template,
            Err(err) => {
                tracing::warn!(
                    "Model {} has no baked chat template ({}); falling back to chatml",
                    model_id,
                    err
                );
                LlamaChatTemplate::new("chatml").map_err(
                    |fallback_err| -> Box<dyn std::error::Error + Send + Sync> {
                        Box::new(fallback_err)
                    },
                )?
            }
        };

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(2048))
            .with_n_batch(8192);

        let context = model_ref.new_context(&backend, ctx_params)?;

        Ok(Self {
            model: model_ref,
            context: Mutex::new(context),
            _backend: backend,
            chat_template,
            model_id,
            model_path,
        })
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    pub fn model_path(&self) -> &Path {
        &self.model_path
    }

    /// Summarize noisy OCR text into a clean sentence
    pub async fn summarize(&self, ocr_text: &str) -> String {
        if ocr_text.trim().is_empty() {
            return String::new();
        }

        let prompt = match self.build_prompt(
            "You rewrite noisy OCR into one short action-first summary for a private memory app.",
            &format!(
                "Summarize this screen in one sentence. Start with an action verb.\n\n{}",
                ocr_text.chars().take(800).collect::<String>()
            ),
        ) {
            Ok(prompt) => prompt,
            Err(err) => {
                tracing::error!("Prompt build failed: {}", err);
                return String::new();
            }
        };

        self.complete(&prompt, 40).await
    }

    /// Answer contextual questions using retrieved memories (RAG)
    pub async fn answer(&self, question: &str, context_str: &str) -> String {
        let prompt = match self.build_prompt(
            "You answer questions using local memory snippets. Be direct, grounded, and concise.",
            &format!(
                "Context:\n{}\n\nQuestion: {}",
                context_str.chars().take(1000).collect::<String>(),
                question
            ),
        ) {
            Ok(prompt) => prompt,
            Err(err) => {
                tracing::error!("Prompt build failed: {}", err);
                return String::new();
            }
        };

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

        let prompt = match self.build_prompt(
            "You extract key facts from local screen memories.",
            &format!(
                "Return:\nACTIVITY: what the user was doing\nDETAILS: key names, dates, numbers, and entities\n\nApp: {}\nWindow: {}\nContent:\n{}",
                app_name,
                window_title,
                text.chars().take(1000).collect::<String>()
            ),
        ) {
            Ok(prompt) => prompt,
            Err(err) => {
                tracing::error!("Prompt build failed: {}", err);
                return String::new();
            }
        };

        self.complete(&prompt, 150).await
    }

    /// Synthesize multiple search results into a coherent summary
    pub async fn summarize_search_results(&self, query: &str, results: &[String]) -> String {
        if results.is_empty() {
            return String::new();
        }

        let combined_text = results.join("\n---\n");
        let prompt = match self.build_prompt(
            "You compress multiple local search snippets into one grounded summary.",
            &format!(
                "Query: {}\nSnippets:\n{}\n\nCombine these into one paragraph under 40 words.",
                query,
                combined_text.chars().take(800).collect::<String>()
            ),
        ) {
            Ok(prompt) => prompt,
            Err(err) => {
                tracing::error!("Prompt build failed: {}", err);
                return String::new();
            }
        };

        self.complete(&prompt, 100).await
    }

    /// Extract actionable todos/reminders from memory text
    pub async fn extract_todos(&self, memories_text: &str) -> String {
        if memories_text.trim().is_empty() {
            return String::new();
        }

        let prompt = match self.build_prompt(
            "You identify clear follow-up actions from recent screen activity.",
            &format!(
                "Extract actionable items from these screen captures.\nFormat:\n- TODO: [task]\n- REMINDER: [time-based item]\nMaximum 5 items. Only include clear actions.\n\n{}",
                memories_text.chars().take(2000).collect::<String>()
            ),
        ) {
            Ok(prompt) => prompt,
            Err(err) => {
                tracing::error!("Prompt build failed: {}", err);
                return String::new();
            }
        };

        self.complete(&prompt, 200).await
    }

    fn build_prompt(&self, system_message: &str, user_message: &str) -> Result<String, String> {
        let messages = vec![
            LlamaChatMessage::new("system".to_string(), system_message.replace('\0', " "))
                .map_err(|err| err.to_string())?,
            LlamaChatMessage::new("user".to_string(), user_message.replace('\0', " "))
                .map_err(|err| err.to_string())?,
        ];

        self.model
            .apply_chat_template(&self.chat_template, &messages, true)
            .map_err(|err| err.to_string())
    }

    async fn complete(&self, prompt: &str, max_tokens: i32) -> String {
        let mut ctx = self.context.lock();

        // Clear KV cache (kv_cache_clear or just reset)
        // In llama-cpp-2 wrapper, context management is through KvCache
        ctx.clear_kv_cache();

        let tokens_list = match self.model.str_to_token(prompt, AddBos::Never) {
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
