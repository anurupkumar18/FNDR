use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaModel, AddBos, Special};
use llama_cpp_2::context::LlamaContext;
use std::path::PathBuf;
use std::sync::Arc;
use std::num::NonZeroU32;
use parking_lot::Mutex;

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
    /// Initialize the engine (assumes Llama 3.2 1B GGUF is downloaded)
    pub async fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        tracing::info!("Initializing local LLM via llama-cpp (Llama 3.2 1B)...");

        let backend = LlamaBackend::init()?;
        let backend = Arc::new(backend);
        
        let model_path = PathBuf::from("models/Llama-3.2-1B-Instruct-Q4_K_M.gguf");
        
        if !model_path.exists() {
            tracing::error!("Model file not found at {:?}. AI features will be disabled.", model_path);
            return Err("Model file missing. Please ensure models/Llama-3.2-1B-Instruct-Q4_K_M.gguf exists.".into());
        }

        let model_params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(&backend, &model_path, &model_params)?;
        
        // Leak the model to get a 'static reference, allowing the context to be 'static.
        // This is safe since InferenceEngine is a singleton for the application lifetime.
        let model_ref: &'static LlamaModel = Box::leak(Box::new(model));
        
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(2048));
        
        let context = model_ref.new_context(&backend, ctx_params)?;

        Ok(Self {
            model: model_ref,
            context: Mutex::new(context),
            _backend: backend,
        })
    }

    /// Summarize noisy OCR text into a clean sentence
    pub async fn summarize(&self, ocr_text: &str) -> String {
        if ocr_text.trim().is_empty() { return String::new(); }

        let prompt = format!(
            "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\nSummarize the OCR text into one concise, human-readable sentence. Ignore UI noise. DO NOT include introductory phrases like 'Here is a summary' or 'Here is a concise, human-readable summary of the OCR text'.<|eot_id|><|start_header_id|>user<|end_header_id|>\n\nRAW OCR: \"{}\"<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n",
            ocr_text
        );

        self.complete(&prompt, 64).await
    }

    /// Answer contextual questions using retrieved memories (RAG)
    pub async fn answer(&self, question: &str, context_str: &str) -> String {
        let prompt = format!(
            "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\nYou are FNDR, a helpful local memory assistant. Answer the user based ONLY on the provided context. Be conversational and concise. DO NOT include introductory phrases like 'Here is a summary' or 'Here is a concise, human-readable summary of the OCR text'. RETURN ONLY THE ANSWER, DO NOT include any additional text.<|eot_id|><|start_header_id|>user<|end_header_id|>\n\nCONTEXT:\n{}\n\nQUESTION: {}<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n",
            context_str, question
        );

        self.complete(&prompt, 300).await
    }

    async fn complete(&self, prompt: &str, max_tokens: i32) -> String {
        let mut ctx = self.context.lock();
        
        // Clear KV cache (kv_cache_clear or just reset)
        // In llama-cpp-2 wrapper, context management is through KvCache
        ctx.clear_kv_cache();

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
                .max_by(|a, b| a.logit().partial_cmp(&b.logit()).unwrap_or(std::cmp::Ordering::Equal))
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
