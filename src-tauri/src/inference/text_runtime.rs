//! The text model and its borrowing context stay on one owning thread.
use super::{
    inference_should_stop, prepare_prompt_tokens, CompletionTrace, InferenceRunControl, TokenUsage,
};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
#[allow(deprecated)]
use llama_cpp_2::model::Special;
use llama_cpp_2::model::{AddBos, LlamaChatMessage, LlamaChatTemplate, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use parking_lot::Mutex;
use std::num::NonZeroU32;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use tokio::sync::oneshot;

enum Request {
    Format {
        system: String,
        user: String,
        reply: oneshot::Sender<Result<String, String>>,
    },
    Complete {
        job: CompletionJob,
        reply: oneshot::Sender<String>,
    },
    Shutdown,
    #[cfg(test)]
    Hold {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    },
}

struct CompletionJob {
    prompt: String,
    max_tokens: i32,
    control: Option<InferenceRunControl>,
    trace: CompletionTrace,
}

pub(super) struct TextRuntime {
    requests: mpsc::Sender<Request>,
    stopping: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl TextRuntime {
    pub(super) async fn new(
        backend: Arc<LlamaBackend>,
        model_path: PathBuf,
        model_id: String,
        n_ctx: NonZeroU32,
        n_batch: u32,
        n_ubatch: u32,
    ) -> Result<Self, String> {
        let (requests, receiver) = mpsc::channel();
        let (ready, initialized) = oneshot::channel();
        let stopping = Arc::new(AtomicBool::new(false));
        let worker_stop = stopping.clone();
        let thread = std::thread::Builder::new()
            .name("fndr-text-model".into())
            .spawn(move || {
                // These locals are created and dropped on this thread. The
                // context's borrow cannot escape to a request or caller.
                let model = match LlamaModel::load_from_file(
                    &backend,
                    model_path,
                    &LlamaModelParams::default(),
                ) {
                    Ok(model) => model,
                    Err(error) => {
                        let _ = ready.send(Err(format!("Model load failed: {error}")));
                        return;
                    }
                };
                if worker_stop.load(Ordering::SeqCst) {
                    let _ = ready.send(Err("Text model initialization stopped".to_string()));
                    return;
                }
                let params = LlamaContextParams::default()
                    .with_n_ctx(Some(n_ctx))
                    .with_n_batch(n_batch)
                    .with_n_ubatch(n_ubatch);
                let mut context = match model.new_context(&backend, params) {
                    Ok(context) => context,
                    Err(error) => {
                        let _ = ready.send(Err(format!("Context creation failed: {error}")));
                        return;
                    }
                };
                let template = match model.chat_template(None) {
                    Ok(template) => template,
                    Err(error) => {
                        tracing::warn!(
                            "Model {model_id} has no baked chat template ({error}); falling back to chatml"
                        );
                        match LlamaChatTemplate::new("chatml") {
                            Ok(template) => template,
                            Err(error) => {
                                let _ = ready.send(Err(error.to_string()));
                                return;
                            }
                        }
                    }
                };
                let _ = ready.send(Ok(()));
                while let Ok(request) = receiver.recv() {
                    if worker_stop.load(Ordering::SeqCst) {
                        break;
                    }
                    match request {
                        Request::Format { system, user, reply } => {
                            let _ = reply.send(format_prompt(&model, &template, &system, &user));
                        }
                        Request::Complete { job, reply } => {
                            let mut usage = TokenUsage::default();
                            let output = complete(
                                &model, &mut context, &job, &worker_stop, &mut usage,
                            );
                            // The trace belongs to real work, including when
                            // its waiting future has already been cancelled.
                            job.trace.record(&job.prompt, &output, usage, job.max_tokens);
                            let _ = reply.send(output);
                        }
                        Request::Shutdown => break,
                        #[cfg(test)]
                        Request::Hold { entered, release } => {
                            let _ = entered.send(());
                            let _ = release.recv();
                        }
                    }
                }
                // Reverse local drop order releases the context before its
                // model, and both before the backend handle.
            })
            .map_err(|error| format!("Text model thread failed to start: {error}"))?;
        let runtime = Self {
            requests,
            stopping,
            thread: Mutex::new(Some(thread)),
        };
        initialized
            .await
            .map_err(|error| format!("Text model initialization stopped: {error}"))??;
        Ok(runtime)
    }

    pub(super) async fn format(&self, system: String, user: String) -> Result<String, String> {
        if self.stopping.load(Ordering::SeqCst) {
            return Err("Text model runtime is stopped".to_string());
        }
        let (reply, result) = oneshot::channel();
        self.requests
            .send(Request::Format {
                system,
                user,
                reply,
            })
            .map_err(|_| "Text model runtime is stopped".to_string())?;
        result
            .await
            .map_err(|_| "Text model runtime is stopped".to_string())?
    }

    pub(super) async fn complete(
        &self,
        prompt: String,
        max_tokens: i32,
        control: Option<InferenceRunControl>,
        trace: CompletionTrace,
    ) -> String {
        if self.stopping.load(Ordering::SeqCst) {
            return String::new();
        }
        let (reply, result) = oneshot::channel();
        let job = CompletionJob {
            prompt,
            max_tokens,
            control,
            trace,
        };
        if self
            .requests
            .send(Request::Complete { job, reply })
            .is_err()
        {
            tracing::error!("Text model runtime is stopped");
            return String::new();
        }
        result.await.unwrap_or_else(|error| {
            tracing::error!("Text model completion stopped: {error}");
            String::new()
        })
    }

    pub(super) fn shutdown(&self) {
        self.stopping.store(true, Ordering::SeqCst);
        let _ = self.requests.send(Request::Shutdown);
        // Retain the guard through join so concurrent shutdown callers also
        // wait for the actual resource drops.
        let mut owner = self.thread.lock();
        if let Some(thread) = owner.take() {
            if let Err(error) = thread.join() {
                tracing::error!("Text model thread stopped with a panic: {error:?}");
            }
        }
    }

    #[cfg(test)]
    pub(super) fn hold_for_test(&self) -> mpsc::Sender<()> {
        let (entered, waiting) = mpsc::channel();
        let (release, held) = mpsc::channel();
        self.requests
            .send(Request::Hold {
                entered,
                release: held,
            })
            .expect("hold request");
        waiting.recv().expect("worker reached hold");
        release
    }
}

impl Drop for TextRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn format_prompt(
    model: &LlamaModel,
    template: &LlamaChatTemplate,
    system: &str,
    user: &str,
) -> Result<String, String> {
    let system = if system.contains('\0') {
        tracing::warn!("build_prompt: null byte in system message (stripping)");
        system.replace('\0', " ")
    } else {
        system.to_string()
    };
    let user = if user.contains('\0') {
        tracing::warn!("build_prompt: null byte in user message (stripping)");
        user.replace('\0', " ")
    } else {
        user.to_string()
    };
    let messages = vec![
        LlamaChatMessage::new("system".to_string(), system).map_err(|error| error.to_string())?,
        LlamaChatMessage::new("user".to_string(), user).map_err(|error| error.to_string())?,
    ];
    model
        .apply_chat_template(template, &messages, true)
        .map_err(|error| error.to_string())
}

struct KvCacheClearGuard<'borrow, 'model> {
    context: &'borrow mut LlamaContext<'model>,
}

impl<'borrow, 'model> KvCacheClearGuard<'borrow, 'model> {
    fn new(context: &'borrow mut LlamaContext<'model>) -> Self {
        context.clear_kv_cache();
        Self { context }
    }
}

impl<'model> Deref for KvCacheClearGuard<'_, 'model> {
    type Target = LlamaContext<'model>;
    fn deref(&self) -> &Self::Target {
        self.context
    }
}

impl DerefMut for KvCacheClearGuard<'_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.context
    }
}

impl Drop for KvCacheClearGuard<'_, '_> {
    fn drop(&mut self) {
        self.context.clear_kv_cache();
    }
}

fn complete(
    model: &LlamaModel,
    context: &mut LlamaContext<'_>,
    job: &CompletionJob,
    stopping: &AtomicBool,
    usage: &mut TokenUsage,
) -> String {
    let prompt = &job.prompt;
    let max_tokens = job.max_tokens;
    let control = job.control.as_ref();
    let task = job.trace.task;
    let t0 = std::time::Instant::now();
    if stopping.load(Ordering::SeqCst) || inference_should_stop(control) {
        return String::new();
    }
    let mut ctx = KvCacheClearGuard::new(context);
    let n_batch = ctx.n_batch().max(1) as usize;
    let n_ctx = ctx.n_ctx() as usize;
    let mut tokens_list = match model.str_to_token(prompt, AddBos::Never) {
        Ok(tokens) => tokens,
        Err(error) => {
            tracing::error!("Tokenization failed: {error}");
            return "AI Error: Tokenization failed.".to_string();
        }
    };
    if tokens_list.is_empty() {
        tracing::error!("Tokenization produced empty output");
        return String::new();
    }
    usage.prompt_tokens = tokens_list.len() as u32;
    if !prepare_prompt_tokens(&mut tokens_list, n_ctx, max_tokens, task) {
        return String::new();
    }
    let prompt_len = tokens_list.len();
    usage.prompt_tokens = prompt_len as u32;
    let mut batch = LlamaBatch::new(n_batch, 1);
    let mut offset = 0usize;
    while offset < prompt_len {
        if stopping.load(Ordering::SeqCst) || inference_should_stop(control) {
            return String::new();
        }
        let end = (offset + n_batch).min(prompt_len);
        batch.clear();
        for pos in offset..end {
            if let Err(error) = batch.add(tokens_list[pos], pos as i32, &[0], pos == prompt_len - 1)
            {
                tracing::error!("Batch add failed during prefill: {error:?}");
                return "AI Error: LLM batch setup failed.".to_string();
            }
        }
        if let Err(error) = ctx.decode(&mut batch) {
            tracing::error!("Decode failed: {error}");
            return "AI Error: LLM Decode failed.".to_string();
        }
        offset = end;
    }
    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::penalties(64, 1.1, 0.0, 0.0),
        LlamaSampler::greedy(),
    ]);
    let mut result = String::new();
    let mut n_cur = tokens_list.len() as i32;
    for _ in 0..max_tokens {
        if stopping.load(Ordering::SeqCst) || inference_should_stop(control) {
            return String::new();
        }
        let token = sampler.sample(&ctx, -1);
        sampler.accept(token);
        if model.is_eog_token(token) {
            break;
        }
        #[allow(deprecated)]
        let piece = model
            .token_to_str(token, Special::Plaintext)
            .unwrap_or_default();
        result.push_str(&piece);
        usage.output_tokens += 1;
        batch.clear();
        let _ = batch.add(token, n_cur, &[0], true);
        if let Err(error) = ctx.decode(&mut batch) {
            tracing::error!("Incremental decode failed: {error}");
            break;
        }
        n_cur += 1;
    }
    tracing::debug!(
        "Completion finished ({} tokens, {} chars)",
        n_cur - tokens_list.len() as i32,
        result.trim().chars().count()
    );
    crate::telemetry::runtime_metrics::record_ms(
        "llm.complete_ms",
        t0.elapsed().as_millis() as u64,
    );
    result.trim().to_string()
}
