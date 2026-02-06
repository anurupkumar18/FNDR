//! Vision Language Model (VLM) inference engine
//!
//! Uses SmolVLM for intelligent image understanding beyond OCR.
//! Primary: SmolVLM-500M, Fallback: SmolVLM-256M

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use parking_lot::Mutex;
use std::num::NonZeroU32;
use std::path::PathBuf;
use std::sync::Arc;

/// VLM Engine for image understanding
/// Provides intelligent screen analysis beyond raw OCR text
pub struct VlmEngine {
    model: &'static LlamaModel,
    context: Mutex<llama_cpp_2::context::LlamaContext<'static>>,
    _backend: Arc<LlamaBackend>,
    model_size: String,
}

unsafe impl Send for VlmEngine {}
unsafe impl Sync for VlmEngine {}

impl VlmEngine {
    /// Initialize the VLM engine with specified model size
    /// Falls back to 256M if 500M is not available
    pub async fn new(model_size: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let (model_path, size_label) = Self::resolve_model_path(model_size)?;

        tracing::info!(
            "Initializing VLM engine (SmolVLM-{}) from {:?}...",
            size_label,
            model_path
        );

        let backend = LlamaBackend::init()?;
        let backend = Arc::new(backend);

        let model_params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(&backend, &model_path, &model_params)?;

        // Leak the model to get a 'static reference (singleton pattern)
        let model_ref: &'static LlamaModel = Box::leak(Box::new(model));

        let ctx_params = LlamaContextParams::default().with_n_ctx(NonZeroU32::new(2048));

        let context = model_ref.new_context(&backend, ctx_params)?;

        tracing::info!(
            "VLM engine initialized successfully (SmolVLM-{})",
            size_label
        );

        Ok(Self {
            model: model_ref,
            context: Mutex::new(context),
            _backend: backend,
            model_size: size_label,
        })
    }

    /// Resolve model path, trying primary then fallback
    fn resolve_model_path(
        preferred_size: &str,
    ) -> Result<(PathBuf, String), Box<dyn std::error::Error + Send + Sync>> {
        let models_dir = PathBuf::from("models");

        // Try preferred model first
        let primary_path = match preferred_size {
            "500M" => models_dir.join("SmolVLM-500M-Instruct-Q4_K_M.gguf"),
            "256M" => models_dir.join("SmolVLM-256M-Instruct-Q4_K_M.gguf"),
            _ => models_dir.join("SmolVLM-500M-Instruct-Q4_K_M.gguf"),
        };

        if primary_path.exists() {
            return Ok((primary_path, preferred_size.to_string()));
        }

        // Fallback to other model
        let fallback_size = if preferred_size == "500M" {
            "256M"
        } else {
            "500M"
        };
        let fallback_path = match fallback_size {
            "500M" => models_dir.join("SmolVLM-500M-Instruct-Q4_K_M.gguf"),
            _ => models_dir.join("SmolVLM-256M-Instruct-Q4_K_M.gguf"),
        };

        if fallback_path.exists() {
            tracing::warn!(
                "Primary VLM model (SmolVLM-{}) not found, using fallback (SmolVLM-{})",
                preferred_size,
                fallback_size
            );
            return Ok((fallback_path, fallback_size.to_string()));
        }

        Err(format!(
            "No VLM model found. Please run ./download_model.sh to download SmolVLM models."
        )
        .into())
    }

    /// Get the active model size
    pub fn model_size(&self) -> &str {
        &self.model_size
    }

    /// Describe what's visible in a screenshot
    /// Returns a concise description of the screen content
    pub async fn describe_screen(&self, ocr_text: &str) -> String {
        if ocr_text.trim().is_empty() {
            return String::new();
        }

        // For now, use the VLM to enhance OCR output with better semantic understanding
        // In the future, this can be extended to process actual image data via mmproj
        let prompt = format!(
            "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n\
            You are a screen analysis assistant. Given OCR text from a screenshot, \
            describe what the user is doing in one clear, concise sentence. \
            Focus on the main activity, not UI elements. \
            DO NOT start with phrases like 'The user is' or 'Based on the OCR'.\
            <|eot_id|><|start_header_id|>user<|end_header_id|>\n\n\
            Screen text:\n{}\
            <|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n",
            ocr_text
        );

        self.complete(&prompt, 80).await
    }

    /// Analyze screen content and combine with OCR for richer context
    pub async fn analyze_screen(&self, ocr_text: &str, app_name: &str) -> String {
        if ocr_text.trim().is_empty() {
            return format!("Using {}", app_name);
        }

        let prompt = format!(
            "<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\n\
            You are a memory assistant. specific concise keywords and actions only. \
            Output format: 'Action: [Action] | Context: [Details]'. \
            Keep it under 15 words. No filler.\
            <|eot_id|><|start_header_id|>user<|end_header_id|>\n\n\
            App: {}\nScreen text:\n{}\
            <|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n",
            app_name, ocr_text
        );

        self.complete(&prompt, 100).await
    }

    /// Internal completion method
    async fn complete(&self, prompt: &str, max_tokens: i32) -> String {
        let mut ctx = self.context.lock();

        ctx.clear_kv_cache();

        let tokens_list = match self.model.str_to_token(prompt, AddBos::Always) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("VLM tokenization failed: {}", e);
                return String::new();
            }
        };

        let mut batch = LlamaBatch::new(2048, 1);
        for (i, token) in tokens_list.iter().enumerate() {
            let last = i == tokens_list.len() - 1;
            let _ = batch.add(*token, i as i32, &[0], last);
        }

        if let Err(e) = ctx.decode(&mut batch) {
            tracing::error!("VLM decode failed: {}", e);
            return String::new();
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
                tracing::error!("VLM incremental decode failed: {}", e);
                break;
            }
            n_cur += 1;
        }

        result.trim().to_string()
    }
}
