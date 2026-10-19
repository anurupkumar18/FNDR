//! Loads EmbeddingGemma 2's text model through llama-cpp-2 and compares its
//! vectors with DigUp's reference (llama.cpp b11461, vectors version 1).
//!
//! cargo run --release --features metal -- <embeddinggemma-2-Q8_0.gguf> [reference.json]
//!
//! Pass: every cosine >= 0.999, the bar DigUp holds each llama.cpp update to.

use llama_cpp_2::context::params::{LlamaContextParams, LlamaPoolingType};
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use std::num::NonZeroU32;
use std::time::Instant;

#[derive(serde::Deserialize)]
struct Reference {
    made_with: String,
    cases: Vec<Case>,
}

#[derive(serde::Deserialize)]
struct Case {
    name: String,
    input: String,
    vector: Vec<f32>,
}

const MAX_TOKENS: usize = 2048;
const AGREEMENT: f32 = 0.999;

fn main() {
    let mut args = std::env::args().skip(1);
    let model_path = args.next().expect("usage: egemma2-spike <model.gguf> [reference.json]");
    let reference_path = args.next().unwrap_or_else(|| "reference.json".into());
    let reference: Reference =
        serde_json::from_str(&std::fs::read_to_string(&reference_path).expect("read reference")).expect("parse reference");

    let backend = LlamaBackend::init().expect("backend");
    let started = Instant::now();
    let model = LlamaModel::load_from_file(&backend, &model_path, &LlamaModelParams::default().with_n_gpu_layers(999))
        .expect("load model");
    let params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(MAX_TOKENS as u32))
        .with_n_batch(MAX_TOKENS as u32)
        .with_n_ubatch(MAX_TOKENS as u32)
        .with_n_seq_max(1)
        .with_embeddings(true)
        .with_pooling_type(LlamaPoolingType::Mean);
    let mut context = model.new_context(&backend, params).expect("context");
    println!("loaded in {:.2} s; n_embd_out = {}", started.elapsed().as_secs_f64(), model.n_embd_out());

    let vocab = model.vocab();
    let mut failures = 0;
    for case in &reference.cases {
        // <bos> text <eos>, cut as transformers cuts (keeping both special tokens).
        let mut tokens = vocab.tokenize(case.input.as_bytes(), true, true);
        if tokens.len() > MAX_TOKENS {
            tokens.truncate(MAX_TOKENS - 1);
            tokens.push(vocab.eos());
        }
        let first = tokens.first().map(|t| t.0);
        let last = tokens.last().map(|t| t.0);
        let mut batch = LlamaBatch::new(tokens.len(), 1);
        for (position, token) in tokens.iter().enumerate() {
            batch.add(*token, position as i32, &[0], true).expect("batch add");
        }
        context.clear_kv_cache();
        let timer = Instant::now();
        if let Err(decode) = context.decode(&mut batch) {
            context.encode(&mut batch).unwrap_or_else(|encode| panic!("decode: {decode}; encode: {encode}"));
        }
        let pooled = context.embeddings_seq_ith(0).expect("pooled embedding");
        let norm = pooled.iter().map(|v| (*v as f64).powi(2)).sum::<f64>().sqrt().max(1e-12);
        let vector: Vec<f32> = pooled.iter().map(|v| (*v as f64 / norm) as f32).collect();
        let cosine: f32 = vector.iter().zip(&case.vector).map(|(a, b)| a * b).sum();
        let ok = vector.len() == case.vector.len() && cosine >= AGREEMENT;
        failures += usize::from(!ok);
        println!(
            "{:<14} tokens {:>4} (bos {} eos {} | first {:?} last {:?})  {:>7.1} ms  cosine {:.5}  {}",
            case.name,
            tokens.len(),
            vocab.bos().0,
            vocab.eos().0,
            first,
            last,
            timer.elapsed().as_secs_f64() * 1000.0,
            cosine,
            if ok { "ok" } else { "FAIL" }
        );
    }
    println!("reference made with {}; {} of {} cases agree", reference.made_with, reference.cases.len() - failures, reference.cases.len());
    std::process::exit(i32::from(failures > 0));
}
