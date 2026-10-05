//! VS-47: the ONNX embedder reproduces EmbeddingGemma's reference vectors.
//!
//! The reference (`tests/fixtures/embeddinggemma_reference.json`) comes from
//! sentence-transformers, the model's reference implementation, through
//! `scripts/audit/embeddinggemma_reference.py`. The test needs the
//! onnx-community export (`model.onnx`, `model.onnx_data`, `tokenizer.json`,
//! revision 5090578d9565bb06545b4552f76e6bc2c93e4a66, 1.2 GB), so it is
//! ignored by default:
//!
//! FNDR_EMBED_MODEL_DIR=/path/to/embeddinggemma \
//!   cargo test --test embeddinggemma_reference -- --ignored --nocapture

use fndr_lib::config::ChunkingConfig;
use fndr_lib::embedding::prefixes::{document_text_for, query_text_for};
use fndr_lib::embedding::Embedder;
use fndr_lib::inference::model_config::embedding_v6_contract;
use serde::Deserialize;

#[derive(Deserialize)]
struct Reference {
    dimensions: usize,
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    kind: String,
    text: String,
    embedding: Vec<f32>,
}

fn reference() -> Reference {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/embeddinggemma_reference.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("reference file"))
        .expect("reference json")
}

fn cut_and_normalize(vector: &[f32], dimensions: usize) -> Vec<f32> {
    let cut = &vector[..dimensions];
    let norm = cut.iter().map(|value| value * value).sum::<f32>().sqrt();
    cut.iter().map(|value| value / norm).collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot = a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    let norm = |v: &[f32]| v.iter().map(|x| x * x).sum::<f32>().sqrt();
    dot / (norm(a) * norm(b))
}

#[test]
#[ignore = "needs the EmbeddingGemma ONNX export in FNDR_EMBED_MODEL_DIR"]
fn onnx_embedder_matches_the_reference_at_768_and_256_dimensions() {
    let reference = reference();
    assert_eq!(reference.dimensions, 768);
    assert_eq!(reference.items.len(), 20);

    for dimensions in [768, 256] {
        let contract = embedding_v6_contract(dimensions).expect("supported dimension");
        let embedder = Embedder::with_contract_and_chunking_config(
            contract,
            &ChunkingConfig::default(),
            false,
        )
        .expect("EmbeddingGemma embedder (is FNDR_EMBED_MODEL_DIR set?)");
        let texts = reference
            .items
            .iter()
            .map(|item| match item.kind.as_str() {
                "query" => query_text_for(contract, &item.text),
                _ => document_text_for(contract, &item.text),
            })
            .collect::<Vec<_>>();
        let vectors = embedder.embed_batch(&texts).expect("embed");

        let cosines = reference
            .items
            .iter()
            .zip(&vectors)
            .map(|(item, vector)| {
                assert_eq!(vector.len(), dimensions);
                cosine(&cut_and_normalize(&item.embedding, dimensions), vector)
            })
            .collect::<Vec<_>>();
        let lowest = cosines.iter().copied().fold(f32::INFINITY, f32::min);
        let mean = cosines.iter().sum::<f32>() / cosines.len() as f32;
        println!("{dimensions} dimensions: lowest cosine {lowest:.6}, mean {mean:.6}");
        for (item, value) in reference.items.iter().zip(&cosines) {
            assert!(
                *value >= 0.999,
                "{dimensions} dimensions: cosine {value:.6} for {:?}",
                item.text
            );
        }
    }
}
