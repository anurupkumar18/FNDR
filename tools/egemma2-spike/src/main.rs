//! FNDR's EmbeddingGemma 2 embedder against DigUp's reference vectors
//! (llama.cpp b11461, vectors version 1): five texts, three pictures at
//! three budgets, one recording.
//!
//! cargo run --release --features metal -- <embeddinggemma-2-Q8_0.gguf> <mmproj-embeddinggemma-2-Q8_0.gguf>
//!
//! Pass: every cosine >= 0.999, the bar DigUp holds each llama.cpp update to.
//! Without the media file only the text cases run.

#[path = "../../../src-tauri/src/files/mod.rs"]
#[allow(dead_code)]
mod files;

use files::gemma::GemmaEmbedder;
use std::path::Path;
use std::time::Instant;

#[derive(serde::Deserialize)]
struct Reference {
    made_with: String,
    cases: Vec<Case>,
}

#[derive(serde::Deserialize)]
struct Case {
    name: String,
    kind: String,
    input: String,
    budget: Option<u32>,
    vector: Vec<f32>,
}

const AGREEMENT: f32 = 0.999;

fn main() {
    let mut args = std::env::args().skip(1);
    let text_model = args.next().expect("usage: egemma2-spike <text.gguf> [mmproj.gguf]");
    let media_model = args.next();
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let reference: Reference =
        serde_json::from_str(&std::fs::read_to_string(here.join("reference.json")).expect("read reference")).expect("parse");

    let started = Instant::now();
    let mut embedder = GemmaEmbedder::load(Path::new(&text_model), media_model.as_deref().map(Path::new), false)
        .expect("load embedder");
    println!("loaded in {:.2} s; dimension {}; media {}", started.elapsed().as_secs_f64(), embedder.dimension(), embedder.has_media());

    let (mut ran, mut failures) = (0, 0);
    for case in &reference.cases {
        if case.kind != "text" && !embedder.has_media() {
            continue;
        }
        let timer = Instant::now();
        let made = match case.kind.as_str() {
            "text" => embedder.embed_texts(std::slice::from_ref(&case.input)).map(|mut v| v.remove(0)),
            "image" => embedder.embed_image(&here.join(&case.input), case.budget.unwrap_or(280)),
            _ => embedder.embed_audio(&here.join(&case.input)),
        };
        ran += 1;
        match made {
            Ok(vector) => {
                let cosine: f32 = vector.iter().zip(&case.vector).map(|(a, b)| a * b).sum();
                let ok = vector.len() == case.vector.len() && cosine >= AGREEMENT;
                failures += usize::from(!ok);
                println!("{:<14} {:>8.1} ms  cosine {:.5}  {}", case.name, timer.elapsed().as_secs_f64() * 1000.0, cosine, if ok { "ok" } else { "FAIL" });
            }
            Err(error) => {
                failures += 1;
                println!("{:<14} ERROR {error}", case.name);
            }
        }
    }
    // Batched texts must match one at a time (several inputs share one llama_process call).
    let texts: Vec<String> = reference.cases.iter().filter(|c| c.kind == "text").map(|c| c.input.clone()).collect();
    let batched = embedder.embed_texts(&texts).expect("batched");
    let worst = reference
        .cases
        .iter()
        .filter(|c| c.kind == "text")
        .zip(&batched)
        .map(|(case, vector)| vector.iter().zip(&case.vector).map(|(a, b)| a * b).sum::<f32>())
        .fold(1.0_f32, f32::min);
    println!("batched texts: worst cosine {worst:.5} {}", if worst >= AGREEMENT { "ok" } else { "FAIL" });
    failures += usize::from(worst < AGREEMENT);
    println!("reference made with {}; {} of {} cases agree", reference.made_with, ran + 1 - failures, ran + 1);
    std::process::exit(i32::from(failures > 0));
}
