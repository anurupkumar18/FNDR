//! EmbeddingGemma 2 on llama.cpp: the image and audio encoders (mtmd) feed the
//! text model, whose mean-pooled, L2-normalized output is the vector. A port
//! of DigUp's `LlamaEmbedder` (MIT), on llama.cpp's C API.
//!
//! Inputs are framed as the reference frames them: `<bos> text <eos>`, and
//! `<bos> <|image> soft tokens <image|> <eos>` (the same for audio). The model
//! is non-causal with no KV cache, so each whole input (token ids and encoder
//! rows together) goes through one `llama_process` call. Running the pieces as
//! separate decodes (`mtmd_helper_eval_chunks`) would pool each piece apart.
//!
//! Runs only in the Files helper (`fndr-files-helper`), never beside Qwen in
//! the app: it links its own llama.cpp (scripts/files/build-llama.sh) and
//! initializes the backend itself. One thread at a time.

use crate::image_sizing;
use crate::sys;
use std::ffi::{CStr, CString};
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Once;

/// Which vectors this pipeline makes. It changes only when the vectors do;
/// the reference-vector check says when that is. Indexes record it.
pub const VECTOR_VERSION: u32 = 1;

/// One input must fit one micro-batch: 2048 tokens (the reference's cut),
/// which also holds a 1120-token image or a 750-token 30 s clip. Queries are
/// short, so a text-only context reserves less.
const MAX_TOKENS_FULL: usize = 2048;
const MAX_TOKENS_TEXT: usize = 512;
const MAX_SEQUENCES: i32 = 16;

static BACKEND: Once = Once::new();

pub struct GemmaEmbedder {
    model: NonNull<sys::llama_model>,
    context: NonNull<sys::llama_context>,
    media: Option<NonNull<sys::mtmd_context>>,
    batch: NonNull<sys::llama_batch_ext>,
    vocab: *const sys::llama_vocab,
    embedding_size: usize,
    row_size: usize,
    max_tokens: usize,
    sequences: i32,
    tokens: usize,
    position: sys::llama_pos,
}

// The raw handles are owned by this value and used from one thread at a time.
unsafe impl Send for GemmaEmbedder {}

impl GemmaEmbedder {
    /// `media`: the image and audio encoders; `None` loads the text model only (queries).
    pub fn load(text_model: &Path, media: Option<&Path>, cpu_only: bool) -> Result<Self, String> {
        BACKEND.call_once(|| unsafe { sys::llama_backend_init() });
        let text_path = c_path(text_model)?;
        let mut model_params = unsafe { sys::llama_model_default_params() };
        model_params.n_gpu_layers = if cpu_only { 0 } else { 999 };
        let model = NonNull::new(unsafe {
            sys::llama_model_load_from_file(text_path.as_ptr(), model_params)
        })
        .ok_or_else(|| format!("can't load {}", text_model.display()))?;
        let max_tokens = if media.is_some() {
            MAX_TOKENS_FULL
        } else {
            MAX_TOKENS_TEXT
        };
        let mut params = unsafe { sys::llama_context_default_params() };
        params.n_ctx = max_tokens as u32;
        params.n_batch = max_tokens as u32;
        params.n_ubatch = max_tokens as u32;
        params.n_seq_max = MAX_SEQUENCES as u32;
        params.kv_unified = true; // each input may use the whole context, as in llama-embedding
        params.embeddings = true;
        params.pooling_type = sys::LLAMA_POOLING_TYPE_MEAN;
        let Some(context) =
            NonNull::new(unsafe { sys::llama_init_from_model(model.as_ptr(), params) })
        else {
            unsafe { sys::llama_model_free(model.as_ptr()) };
            return Err("can't create a llama.cpp context".into());
        };
        let mut media_context = None;
        if let Some(media_path) = media {
            let path = c_path(media_path)?;
            let mut media_params = unsafe { sys::mtmd_context_params_default() };
            media_params.use_gpu = !cpu_only;
            media_params.print_timings = false;
            media_params.warmup = true;
            media_params.image_min_tokens = 1; // never rescale: images arrive at their final size
            media_params.image_max_tokens = 1120;
            let loaded = NonNull::new(unsafe {
                sys::mtmd_init_from_file(path.as_ptr(), model.as_ptr(), media_params)
            });
            if loaded.is_none() {
                unsafe {
                    sys::llama_free(context.as_ptr());
                    sys::llama_model_free(model.as_ptr());
                }
                return Err(format!("can't load {}", media_path.display()));
            }
            media_context = loaded;
        }
        let batch = NonNull::new(unsafe { sys::llama_batch_ext_init(context.as_ptr()) })
            .ok_or("can't make a batch")?;
        let mut embedder = Self {
            model,
            context,
            media: media_context,
            batch,
            vocab: unsafe { sys::llama_model_get_vocab(model.as_ptr()) },
            embedding_size: unsafe { sys::llama_model_n_embd_out(model.as_ptr()) } as usize,
            row_size: unsafe { sys::llama_model_n_embd_inp(model.as_ptr()) } as usize,
            max_tokens,
            sequences: 0,
            tokens: 0,
            position: 0,
        };
        // The first calls build the Metal pipelines: pay that now.
        for _ in 0..2 {
            embedder.embed_texts(&["task: search result | query: warm up".to_string()])?;
        }
        Ok(embedder)
    }

    pub fn dimension(&self) -> usize {
        self.embedding_size
    }

    pub fn has_media(&self) -> bool {
        self.media.is_some()
    }

    pub fn embed_texts(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.reset();
        let mut vectors = Vec::with_capacity(texts.len());
        for text in texts {
            let tokens = self.tokenize(text)?;
            if self.sequences == MAX_SEQUENCES || self.tokens + tokens.len() > self.max_tokens {
                vectors.extend(self.run()?);
            }
            self.begin();
            for token in tokens {
                self.add_token(token)?;
            }
        }
        if self.sequences > 0 {
            vectors.extend(self.run()?);
        }
        Ok(vectors)
    }

    /// A picture at `budget` soft tokens (70, 140, 280, 560 or 1120).
    pub fn embed_image(&mut self, path: &Path, budget: u32) -> Result<Vec<f32>, String> {
        if !image_sizing::BUDGETS.contains(&budget) {
            return Err(format!("budget must be one of {:?}", image_sizing::BUDGETS));
        }
        let bitmap = self.load_media(path, Some(budget))?;
        self.embed_bitmap(bitmap, path)
    }

    /// Up to 30 s of sound (longer input is cut, as the reference cuts it).
    pub fn embed_audio(&mut self, path: &Path) -> Result<Vec<f32>, String> {
        let bitmap = self.load_media(path, None)?;
        self.embed_bitmap(bitmap, path)
    }

    fn embed_bitmap(&mut self, bitmap: Bitmap, path: &Path) -> Result<Vec<f32>, String> {
        let media = self.media.ok_or("this embedder was started text-only")?;
        let chunks = Chunks::new()?;
        let marker = unsafe { CStr::from_ptr(sys::mtmd_default_marker()) };
        let text = sys::mtmd_input_text {
            text: marker.as_ptr(),
            text_len: marker.to_bytes().len(),
            add_special: true,
            parse_special: true,
        };
        let mut bitmaps = [bitmap.0.as_ptr() as *const sys::mtmd_bitmap];
        let status = unsafe {
            sys::mtmd_tokenize(
                media.as_ptr(),
                chunks.0.as_ptr(),
                &text,
                bitmaps.as_mut_ptr(),
                1,
            )
        };
        if status != 0 {
            return Err(format!("can't tokenize {} ({status})", path.display()));
        }
        self.reset();
        self.begin();
        let count = unsafe { sys::mtmd_input_chunks_size(chunks.0.as_ptr()) };
        for index in 0..count {
            let chunk = unsafe { sys::mtmd_input_chunks_get(chunks.0.as_ptr(), index) };
            if unsafe { sys::mtmd_input_chunk_get_type(chunk) } == sys::MTMD_INPUT_CHUNK_TYPE_TEXT {
                let mut n = 0_usize;
                let tokens = unsafe { sys::mtmd_input_chunk_get_tokens_text(chunk, &mut n) };
                if tokens.is_null() {
                    continue;
                }
                for token in unsafe { std::slice::from_raw_parts(tokens, n) } {
                    self.add_token(*token)?;
                }
            } else {
                if unsafe { sys::mtmd_encode_chunk(media.as_ptr(), chunk) } != 0 {
                    return Err(format!("the encoder failed on {}", path.display()));
                }
                let rows = unsafe { sys::mtmd_get_output_embd(media.as_ptr()) };
                if rows.is_null() {
                    return Err(format!("the encoder gave nothing for {}", path.display()));
                }
                let n = unsafe { sys::mtmd_input_chunk_get_n_tokens(chunk) };
                self.add_rows(rows, n)?;
            }
        }
        self.run()?.pop().ok_or_else(|| "no vector".to_string())
    }

    /// Reads an image or audio file with mtmd's helpers (stb_image, miniaudio
    /// at 16 kHz), resizes an image for `budget`, cuts audio at 30 s.
    fn load_media(&self, path: &Path, image_budget: Option<u32>) -> Result<Bitmap, String> {
        let media = self.media.ok_or("this embedder was started text-only")?;
        let c_path = c_path(path)?;
        let wrapper = unsafe {
            sys::mtmd_helper_bitmap_init_from_file(
                media.as_ptr(),
                c_path.as_ptr(),
                false,
                sys::mtmd_helper_init_opt_default(),
            )
        };
        if !wrapper.video_ctx.is_null() {
            unsafe {
                if !wrapper.bitmap.is_null() {
                    sys::mtmd_bitmap_free(wrapper.bitmap);
                }
                sys::mtmd_helper_video_free(wrapper.video_ctx);
            }
            return Err(format!(
                "video goes in as frames and sound, not as a file: {}",
                path.display()
            ));
        }
        let bitmap = Bitmap(
            NonNull::new(wrapper.bitmap).ok_or_else(|| format!("can't read {}", path.display()))?,
        );
        let is_audio = unsafe { sys::mtmd_bitmap_is_audio(bitmap.0.as_ptr()) };
        if is_audio == image_budget.is_some() {
            return Err(format!(
                "not {}: {}",
                if image_budget.is_some() {
                    "an image"
                } else {
                    "audio"
                },
                path.display()
            ));
        }
        if let Some(budget) = image_budget {
            let width = unsafe { sys::mtmd_bitmap_get_nx(bitmap.0.as_ptr()) } as usize;
            let height = unsafe { sys::mtmd_bitmap_get_ny(bitmap.0.as_ptr()) } as usize;
            let (to_height, to_width) = image_sizing::target_size(height, width, budget)
                .ok_or_else(|| format!("image too thin to embed: {}", path.display()))?;
            if (to_height, to_width) == (height, width) {
                return Ok(bitmap);
            }
            let source = unsafe {
                std::slice::from_raw_parts(
                    sys::mtmd_bitmap_get_data(bitmap.0.as_ptr()),
                    width * height * 3,
                )
            };
            let pixels = image_sizing::resize_bicubic(source, width, height, to_width, to_height);
            let resized = unsafe {
                sys::mtmd_bitmap_init(to_width as u32, to_height as u32, pixels.as_ptr())
            };
            return NonNull::new(resized)
                .map(Bitmap)
                .ok_or_else(|| format!("can't resize {}", path.display()));
        }
        let samples =
            unsafe { sys::mtmd_bitmap_get_n_bytes(bitmap.0.as_ptr()) } / std::mem::size_of::<f32>();
        let limit = 30 * unsafe { sys::mtmd_get_audio_sample_rate(media.as_ptr()) }.max(1) as usize;
        if samples <= limit {
            return Ok(bitmap);
        }
        let data = unsafe { sys::mtmd_bitmap_get_data(bitmap.0.as_ptr()) } as *const f32;
        let prefix = unsafe { sys::mtmd_bitmap_init_from_audio(limit, data) };
        NonNull::new(prefix)
            .map(Bitmap)
            .ok_or_else(|| format!("can't cut {} at 30 s", path.display()))
    }

    /// `<bos> text <eos>`, cut to the context the way transformers cuts (keeping both special tokens).
    fn tokenize(&self, text: &str) -> Result<Vec<sys::llama_token>, String> {
        let bytes = text.as_bytes();
        let mut tokens = vec![0 as sys::llama_token; bytes.len() + 8];
        let mut count = unsafe {
            sys::llama_tokenize(
                self.vocab,
                bytes.as_ptr().cast(),
                bytes.len() as i32,
                tokens.as_mut_ptr(),
                tokens.len() as i32,
                true,
                true,
            )
        };
        if count < 0 {
            tokens = vec![0; (-count) as usize];
            count = unsafe {
                sys::llama_tokenize(
                    self.vocab,
                    bytes.as_ptr().cast(),
                    bytes.len() as i32,
                    tokens.as_mut_ptr(),
                    tokens.len() as i32,
                    true,
                    true,
                )
            };
        }
        if count < 0 {
            return Err("can't tokenize".into());
        }
        tokens.truncate(count as usize);
        if tokens.len() > self.max_tokens {
            tokens.truncate(self.max_tokens - 1);
            tokens.push(unsafe { sys::llama_vocab_eos(self.vocab) });
        }
        Ok(tokens)
    }

    fn begin(&mut self) {
        self.sequences += 1;
        self.position = 0;
    }

    fn reset(&mut self) {
        unsafe { sys::llama_batch_ext_clear(self.batch.as_ptr()) };
        self.sequences = 0;
        self.tokens = 0;
    }

    fn add_token(&mut self, token: sys::llama_token) -> Result<(), String> {
        let index = unsafe {
            sys::llama_batch_ext_add_token(self.batch.as_ptr(), self.sequences - 1, token)
        };
        self.place(index)
    }

    fn add_rows(&mut self, rows: *mut f32, count: usize) -> Result<(), String> {
        for row in 0..count {
            // Copied by the batch.
            let embd = sys::llama_embd {
                data: unsafe { rows.add(row * self.row_size) },
                n_rows: 1,
                n_embd: self.row_size,
            };
            let index = unsafe {
                sys::llama_batch_ext_add_embd(self.batch.as_ptr(), self.sequences - 1, embd)
            };
            self.place(index)?;
        }
        Ok(())
    }

    fn place(&mut self, index: i32) -> Result<(), String> {
        if index < 0 {
            return Err(format!("batch full ({index})"));
        }
        let position = [self.position, 0, 0, 0];
        let placed = unsafe {
            sys::llama_batch_ext_set_pos(self.batch.as_ptr(), index, position.as_ptr())
                && sys::llama_batch_ext_set_output_embd(self.batch.as_ptr(), index, true)
        };
        if !placed {
            return Err(format!("can't place token {index}"));
        }
        self.position += 1;
        self.tokens += 1;
        Ok(())
    }

    /// Runs the text model over the batch; one L2-normalized vector per input, in order.
    fn run(&mut self) -> Result<Vec<Vec<f32>>, String> {
        let status = unsafe {
            sys::llama_process(
                self.context.as_ptr(),
                sys::LLAMA_PROCESS_TYPE_ENCODE,
                self.batch.as_ptr(),
            )
        };
        let sequences = self.sequences;
        self.reset();
        if status != 0 {
            return Err(format!("llama_process failed ({status})"));
        }
        (0..sequences)
            .map(|sequence| {
                let pooled =
                    unsafe { sys::llama_get_embeddings_seq(self.context.as_ptr(), sequence) };
                if pooled.is_null() {
                    return Err(format!("no pooled embedding for input {sequence}"));
                }
                let pooled = unsafe { std::slice::from_raw_parts(pooled, self.embedding_size) };
                Ok(normalize(pooled))
            })
            .collect()
    }
}

impl Drop for GemmaEmbedder {
    fn drop(&mut self) {
        unsafe {
            sys::llama_batch_ext_free(self.batch.as_ptr());
            if let Some(media) = self.media {
                sys::mtmd_free(media.as_ptr());
            }
            sys::llama_free(self.context.as_ptr());
            sys::llama_model_free(self.model.as_ptr());
        }
    }
}

struct Bitmap(NonNull<sys::mtmd_bitmap>);

impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe { sys::mtmd_bitmap_free(self.0.as_ptr()) }
    }
}

struct Chunks(NonNull<sys::mtmd_input_chunks>);

impl Chunks {
    fn new() -> Result<Self, String> {
        NonNull::new(unsafe { sys::mtmd_input_chunks_init() })
            .map(Self)
            .ok_or_else(|| "out of memory".into())
    }
}

impl Drop for Chunks {
    fn drop(&mut self) {
        unsafe { sys::mtmd_input_chunks_free(self.0.as_ptr()) }
    }
}

fn c_path(path: &Path) -> Result<CString, String> {
    CString::new(path.to_string_lossy().as_bytes())
        .map_err(|_| format!("bad path {}", path.display()))
}

/// L2-normalized, in f64 so long sums keep their precision.
pub fn normalize(values: &[f32]) -> Vec<f32> {
    let sum: f64 = values.iter().map(|v| f64::from(*v) * f64::from(*v)).sum();
    let scale = if sum > 0.0 { 1.0 / sum.sqrt() } else { 0.0 };
    values
        .iter()
        .map(|v| (f64::from(*v) * scale) as f32)
        .collect()
}
