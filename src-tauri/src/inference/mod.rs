use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
#[allow(deprecated)]
use llama_cpp_2::model::Special;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
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

    // Suppress overly verbose metal/llama.cpp internal logs for cleaner developer output
    std::env::set_var("GGML_METAL_LOG_INFO", "0");
    std::env::set_var("GGML_METAL_LOG_WARN", "0");
    std::env::set_var("GGML_LOG_LEVEL", "0");

    let backend = Arc::new(LlamaBackend::init()?);
    // If another thread raced us, that's fine – just return our copy
    let _ = LLAMA_BACKEND.set(Arc::clone(&backend));
    Ok(backend)
}
pub use vlm::VlmEngine;

const MAX_OCR_SUMMARY_CHARS: usize = 1100;
const MAX_SUMMARY_CHARS: usize = 120;

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let keep = max_chars.saturating_sub(3);
    let mut out: String = value.chars().take(keep).collect();
    out.push_str("...");
    out
}

fn is_separator_line(line: &str) -> bool {
    !line.is_empty()
        && line
            .chars()
            .all(|ch| ch == '-' || ch == '_' || ch == '=' || ch == '.' || ch == ' ')
}

fn symbol_ratio(line: &str) -> f32 {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return 1.0;
    }
    let symbols = chars
        .iter()
        .filter(|ch| !ch.is_alphanumeric() && !ch.is_whitespace())
        .count();
    symbols as f32 / chars.len() as f32
}

fn looks_like_file_inventory(line: &str) -> bool {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.len() < 5 {
        return false;
    }

    let pathish = tokens
        .iter()
        .filter(|token| {
            let token = token.trim_matches(|ch: char| ",;:()[]{}".contains(ch));
            token.contains('/')
                || token.contains('\\')
                || (token.contains('.')
                    && (token.contains('_') || token.contains('-') || token.ends_with(".rs")))
        })
        .count();

    pathish >= 4
}

fn strip_known_prefixes(value: &str) -> String {
    let trimmed = value.trim();
    let lower = trimmed.to_lowercase();

    for prefix in [
        "summary:",
        "summary -",
        "summary",
        "activity:",
        "action:",
        "output:",
    ] {
        if lower.starts_with(prefix) {
            return trimmed[prefix.len()..].trim().to_string();
        }
    }

    if lower.starts_with("the screen shows ") {
        return trimmed["the screen shows ".len()..].trim().to_string();
    }
    if lower.starts_with("screen shows ") {
        return trimmed["screen shows ".len()..].trim().to_string();
    }
    if lower.starts_with("i see ") {
        return trimmed["i see ".len()..].trim().to_string();
    }

    trimmed.to_string()
}

fn clean_summary_output(raw: &str) -> String {
    let mut candidate = raw
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !is_separator_line(line))
        .unwrap_or(raw.trim())
        .trim_matches(|ch| ch == '"' || ch == '\'' || ch == '`')
        .to_string();

    // Handle "Action: X | Context: Y" style output.
    if let Some((left, right)) = candidate.split_once("| Context:") {
        let action = strip_known_prefixes(left);
        let context = right.trim();
        candidate = format!("{} {}", action, context);
    }

    for _ in 0..3 {
        let stripped = strip_known_prefixes(&candidate);
        if stripped == candidate {
            break;
        }
        candidate = stripped;
    }
    candidate = normalize_whitespace(&candidate);
    truncate_chars(candidate.trim(), MAX_SUMMARY_CHARS)
}

fn is_usable_summary(summary: &str) -> bool {
    let trimmed = summary.trim();
    if trimmed.len() < 8 {
        return false;
    }
    if trimmed.split_whitespace().count() < 2 {
        return false;
    }
    if is_separator_line(trimmed) {
        return false;
    }
    if symbol_ratio(trimmed) > 0.34 {
        return false;
    }
    if looks_like_file_inventory(trimmed) {
        return false;
    }

    let lower = trimmed.to_lowercase();
    if lower == "n/a" || lower == "none" || lower == "unknown" {
        return false;
    }
    if lower.contains("ocr text") || lower.contains("raw text") {
        return false;
    }

    true
}

fn validate_memory_card_draft(mut draft: MemoryCardDraft) -> Option<MemoryCardDraft> {
    draft.title = normalize_whitespace(draft.title.trim());
    draft.summary = normalize_whitespace(draft.summary.trim());
    draft.action = normalize_whitespace(draft.action.trim());
    draft.context = draft
        .context
        .into_iter()
        .map(|value| normalize_whitespace(value.trim()))
        .filter(|value| !value.is_empty())
        .collect();

    if draft.title.is_empty() || draft.summary.is_empty() || draft.action.is_empty() {
        return None;
    }

    if draft.summary.contains('\n')
        || draft.summary.contains('*')
        || draft.summary.contains('#')
        || draft.summary.contains('`')
    {
        return None;
    }

    let summary_lower = draft.summary.to_lowercase();
    if summary_lower.starts_with("the screen shows")
        || summary_lower.starts_with("i see")
        || summary_lower.contains("new tab")
        || summary_lower.contains("toolbar")
        || summary_lower.contains("tab strip")
    {
        return None;
    }

    let words = draft.summary.split_whitespace().count();
    if !(8..=22).contains(&words) {
        return None;
    }

    if !draft.summary.ends_with('.') {
        draft.summary.push('.');
    }

    draft.context.dedup();
    draft.context.truncate(4);
    if draft.context.is_empty() {
        draft.context.push("recent activity".to_string());
    }

    Some(draft)
}

fn extract_json_object(raw: &str) -> Option<String> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(raw[start..=end].to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCardDraft {
    pub title: String,
    pub summary: String,
    pub action: String,
    #[serde(default)]
    pub context: Vec<String>,
}

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

        let backend_clone = Arc::clone(&backend);
        let model_path_clone = model_path.clone();

        let model_ref = tokio::task::spawn_blocking(move || {
            let model_params = LlamaModelParams::default();
            let model =
                LlamaModel::load_from_file(&backend_clone, &model_path_clone, &model_params)?;

            // Leak the model to get a 'static reference, allowing the context to be 'static.
            // This is safe since InferenceEngine is a singleton for the application lifetime.
            let model_ref: &'static LlamaModel = Box::leak(Box::new(model));
            Ok::<&'static LlamaModel, Box<dyn std::error::Error + Send + Sync>>(model_ref)
        })
        .await
        .map_err(|e| format!("Join error during model load: {}", e))?
        .map_err(|e| format!("Model load failed: {}", e))?;

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(NonZeroU32::new(2048))
            .with_n_batch(8192);

        let context = model_ref.new_context(&backend, ctx_params)?;

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
        self.summarize_memory_node("", "", ocr_text).await
    }

    /// Summarize OCR text into a concise memory snippet for storage and graph nodes.
    pub async fn summarize_memory_node(
        &self,
        app_name: &str,
        window_title: &str,
        ocr_text: &str,
    ) -> String {
        if ocr_text.trim().is_empty() {
            return String::new();
        }

        let prompt = match self.build_prompt(
            "You generate memory snippets from OCR text.\n\
            RULES:\n\
            - Output exactly one concise sentence, maximum 14 words.\n\
            - Keep only the primary user activity and key object/topic.\n\
            - Ignore UI chrome, menu labels, status bars, repeated file/path lists, and separators.\n\
            - No preambles like 'I see' or 'The screen shows'.\n\
            - No markdown, no bullet points, no extra labels.",
            &format!(
                "APP: {}\nWINDOW: {}\n\nOCR TEXT:\n\"\"\"\n{}\n\"\"\"\n\nTASK: Return only the best memory snippet.",
                app_name,
                window_title,
                ocr_text.chars().take(MAX_OCR_SUMMARY_CHARS).collect::<String>()
            ),
        ) {
            Ok(prompt) => prompt,
            Err(err) => {
                tracing::error!("Prompt build failed: {}", err);
                return String::new();
            }
        };

        tracing::info!(
            "Summarizing OCR text for memory node ({} chars)...",
            ocr_text.len()
        );
        let raw_summary = self.complete(&prompt, 48).await;
        let summary = clean_summary_output(&raw_summary);

        if !is_usable_summary(&summary) {
            tracing::warn!("Discarded low-signal OCR summary: {}", raw_summary);
            return String::new();
        }

        tracing::info!("OCR summary result: {}", summary);
        summary
    }

    /// Answer contextual questions using retrieved memories (RAG)
    pub async fn answer(&self, question: &str, context_str: &str) -> String {
        let prompt = match self.build_prompt(
            "You answer questions using local memory snippets. Be direct, grounded, and concise.",
            &format!(
                "Context Snippets:\n{}\n\nQuestion: {}",
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
                "MEMORY CONTENT:\nApp: {}\nWindow: {}\nContent: {}\n\nREQUEST: Return ACTIVITY and DETAILS. Be concise.",
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
            "You help users find what they remember by summarizing search results. Respond ONLY with the summary.",
            &format!(
                "SEARCH QUERY: \"{}\"\n\nSNIPPETS:\n\"\"\"\n{}\n\"\"\"\n\nTASK: Combine these snippets into one paragraph that answers the query. Keep it under 40 words. Ground your answer in the facts provided.",
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

        tracing::info!(
            "Summarizing search results for query: '{}' with {} snippets",
            query,
            results.len()
        );
        let summary = self.complete(&prompt, 100).await;
        tracing::info!("Search summary result: {}", summary);
        summary
    }

    /// Generate a structured memory card draft from grouped snippets.
    pub async fn synthesize_memory_card(
        &self,
        query: &str,
        app_name: &str,
        window_title: &str,
        snippets: &[String],
    ) -> Option<MemoryCardDraft> {
        if snippets.is_empty() {
            return None;
        }

        let snippet_block = snippets
            .iter()
            .take(6)
            .enumerate()
            .map(|(idx, snippet)| format!("{}. {}", idx + 1, snippet))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = self
            .build_prompt(
                "You synthesize one memory card from grouped search snippets.\n\
                RULES:\n\
                - Return ONLY strict JSON with keys: title, summary, action, context.\n\
                - summary must be exactly one sentence, 8-22 words.\n\
                - No markdown, no OCR labels, no browser chrome, no preambles.\n\
                - Focus on one dominant activity with 1-3 high-signal details.\n\
                - context must be an array of 1-4 short strings.",
                &format!(
                    "QUERY: {}\nAPP: {}\nWINDOW: {}\nSNIPPETS:\n{}\n\nReturn JSON only.",
                    query, app_name, window_title, snippet_block
                ),
            )
            .ok()?;

        let raw = self.complete(&prompt, 180).await;
        let candidate = extract_json_object(&raw)?;
        let draft: MemoryCardDraft = serde_json::from_str(&candidate).ok()?;
        validate_memory_card_draft(draft)
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

            #[allow(deprecated)]
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

        tracing::debug!(
            "Completion result ({} tokens): {}",
            n_cur - tokens_list.len() as i32,
            result.trim()
        );
        result.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_common_summary_preambles() {
        let cleaned = clean_summary_output("Summary: The screen shows reviewing PR comments");
        assert_eq!(cleaned, "reviewing PR comments");
    }

    #[test]
    fn rejects_file_inventory_noise() {
        let noisy = "src/app.tsx src/lib.rs src/main.rs src-tauri/src/store/schema.rs src-tauri/src/graph/mod.rs";
        assert!(!is_usable_summary(noisy));
    }

    #[test]
    fn accepts_concise_activity_summary() {
        assert!(is_usable_summary(
            "Reviewing download_model.sh changes in FNDR"
        ));
    }
}
