//! Opt-in inactive EmbeddingGemma measurement; never opens a memory store.
//!
//! FNDR_EMBED_MODEL_DIR=/separate/pinned-assets /usr/bin/time -l \
//!   target/debug/examples/embedding_measure 256 [synthetic-input.json] [--single-input] > result.json
//!
//! Input reuses the VS-47 reference format: {"items":[{"kind":"query" or
//! "document","text":"...","id":"optional corpus id"}]}. Export the existing
//! bake-off's texts into this format rather than duplicating its composer here.
//! Omit the input path to use the twenty committed synthetic reference inputs.
//! Run a separate process for each dimension/repeat. Model initialization includes
//! its dimension probe. The single corpus pass includes tokenization/chunking and
//! internal deduplication, not just ONNX execution; no warmed cache is re-timed.
//! --single-input measures each distinct input separately with one loaded model;
//! use a query-only corpus for interactive-query measurements. Shared chunks can
//! still hit the normal cache. Token diagnostics run after model teardown and are
//! excluded from inference timing, but remain part of process peak RSS.

use fndr_lib::config::ChunkingConfig;
use fndr_lib::embedding::{Embedder, EmbeddingInput};
use fndr_lib::inference::model_config::embedding_v6_contract;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Query,
    Document,
}

#[derive(Deserialize, Serialize)]
struct Input {
    kind: Kind,
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<String>,
}

#[derive(Deserialize)]
struct Corpus {
    items: Vec<Input>,
}

fn parse_corpus(source: &str) -> Result<Corpus, String> {
    let corpus: Corpus = serde_json::from_str(source).map_err(|error| error.to_string())?;
    if corpus.items.is_empty() || corpus.items.iter().any(|item| item.text.trim().is_empty()) {
        return Err("Corpus must contain nonempty query/document texts".into());
    }
    Ok(corpus)
}

fn untruncated_token_counts(
    mut tokenizer: tokenizers::Tokenizer,
    chunks: &[String],
) -> Result<Vec<usize>, String> {
    tokenizer
        .with_truncation(None)
        .map_err(|error| error.to_string())?;
    tokenizer.with_padding(None);
    chunks
        .iter()
        .map(|chunk| {
            tokenizer
                .encode(chunk.as_str(), true)
                .map(|encoding| encoding.len())
                .map_err(|error| error.to_string())
        })
        .collect()
}

const USAGE: &str = "Usage: embedding_measure <256|768> [synthetic-input.json] [--single-input]";

fn parse_args(
    args: impl IntoIterator<Item = String>,
) -> Result<(usize, Option<PathBuf>, bool), String> {
    let mut positional = Vec::new();
    let mut single_input = false;
    for arg in args {
        if arg == "--single-input" && !single_input {
            single_input = true;
        } else if arg.starts_with("--") {
            return Err(USAGE.into());
        } else {
            positional.push(arg);
        }
    }
    if positional.is_empty() || positional.len() > 2 {
        return Err(USAGE.into());
    }
    let dimensions = positional[0]
        .parse::<usize>()
        .map_err(|_| USAGE.to_string())?;
    if ![256, 768].contains(&dimensions) {
        return Err("Measurement dimension must be 256 or 768".into());
    }
    Ok((
        dimensions,
        positional.get(1).map(PathBuf::from),
        single_input,
    ))
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (dimensions, input_path, single_input) = parse_args(std::env::args().skip(1))?;
    let input_path = input_path.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/embeddinggemma_reference.json")
    });
    let corpus = parse_corpus(&std::fs::read_to_string(&input_path)?)?;
    let contract = embedding_v6_contract(dimensions)?;
    let model_dir = PathBuf::from(
        std::env::var_os("FNDR_EMBED_MODEL_DIR")
            .ok_or("Set FNDR_EMBED_MODEL_DIR to the isolated pinned model directory")?,
    )
    .canonicalize()?;
    for filename in [contract.model_filename, contract.tokenizer_filename] {
        if !model_dir.join(filename).is_file() {
            return Err(format!(
                "Required asset missing: {}",
                model_dir.join(filename).display()
            )
            .into());
        }
    }
    let inputs: Vec<EmbeddingInput<'_>> = corpus
        .items
        .iter()
        .map(|item| match item.kind {
            Kind::Query => EmbeddingInput::Query(&item.text),
            Kind::Document => EmbeddingInput::Document {
                text: &item.text,
                app_name: "",
                window_title: "",
            },
        })
        .collect();
    let started = Instant::now();
    let embedder =
        Embedder::with_contract_and_chunking_config(contract, &ChunkingConfig::default(), false)?;
    let load_and_probe_ms = started.elapsed().as_secs_f64() * 1000.0;

    // Count the real inputs the public embedder will deduplicate. This pass
    // does not embed or populate its cache and is outside the timed inference.
    let chunk_groups: Vec<Vec<String>> = inputs
        .iter()
        .map(|input| embedder.prepare_input_chunks(*input))
        .collect();
    if single_input && chunk_groups.iter().collect::<HashSet<_>>().len() != inputs.len() {
        return Err(
            "--single-input requires distinct prepared inputs to avoid cached repeat timings"
                .into(),
        );
    }
    let chunks: Vec<String> = chunk_groups.iter().flatten().cloned().collect();
    let unique_chunks = chunks.iter().collect::<HashSet<_>>().len();
    let mut input_latencies_ms = Vec::new();
    let started = Instant::now();
    let vectors = if single_input {
        let mut vectors = Vec::with_capacity(inputs.len());
        for input in &inputs {
            let input_started = Instant::now();
            vectors.extend(embedder.embed_inputs(&[*input])?);
            input_latencies_ms.push(input_started.elapsed().as_secs_f64() * 1000.0);
        }
        vectors
    } else {
        embedder.embed_inputs(&inputs)?
    };
    let corpus_embedding_ms = started.elapsed().as_secs_f64() * 1000.0;
    drop(embedder);
    if vectors.len() != corpus.items.len()
        || vectors.iter().any(|vector| {
            vector.len() != dimensions
                || vector.iter().any(|value| !value.is_finite())
                || vector.iter().all(|value| *value == 0.0)
        })
    {
        return Err("Embedding output has wrong count/dimension or nonfinite/zero vectors".into());
    }
    let tokenizer = tokenizers::Tokenizer::from_file(model_dir.join(contract.tokenizer_filename))
        .map_err(|error| error.to_string())?;
    let token_counts = untruncated_token_counts(tokenizer, &chunks)?;
    let rows: Vec<_> = corpus.items.iter().zip(&vectors).map(|(input, vector)| {
        json!({"kind": input.kind, "text": input.text, "id": input.id, "embedding": vector})
    }).collect();
    serde_json::to_writer(
        std::io::stdout().lock(),
        &json!({
            "schema": "fndr-onnx-embedding-measure/1",
            "model": contract.model_id,
            "dimensions": dimensions,
            "model_dir": model_dir,
            "model_filename": contract.model_filename,
            "input_path": input_path,
            "backend_batch_size": contract.max_batch_size,
            "input_count": inputs.len(),
            "unique_input_count": inputs.iter().collect::<HashSet<_>>().len(),
            "mode": if single_input { "single_input" } else { "batch" },
            "input_latencies_ms": input_latencies_ms,
            "chunk_count": chunks.len(),
            "unique_chunk_count": unique_chunks,
            "token_diagnostics": {
                "includes_special_tokens": true,
                "truncation_and_padding_disabled": true,
                "contract_max_sequence_length": contract.max_sequence_length,
                "max_chunk_tokens": token_counts.iter().copied().max().unwrap_or(0),
                "over_limit_chunk_count": token_counts.iter().filter(|&&count| count > contract.max_sequence_length).count(),
                "total_chunk_tokens": token_counts.iter().sum::<usize>(),
                "chunk_token_counts": token_counts,
                "phase": "after_model_teardown_outside_inference_timing",
            },
            "load_and_probe_ms": load_and_probe_ms,
            "corpus_embedding_ms": corpus_embedding_ms,
            "ms_per_unique_chunk_including_preprocessing": corpus_embedding_ms / unique_chunks as f64,
            "timed_corpus_passes": 1,
            "items": rows,
        }),
    )?;
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("Embedding measurement failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measurement_mode_is_explicit_and_unknown_or_repeated_flags_are_rejected() {
        let parse = |args: &[&str]| parse_args(args.iter().map(|arg| arg.to_string()));
        assert_eq!(parse(&["256"]).unwrap(), (256, None, false));
        assert_eq!(
            parse(&["768", "queries.json", "--single-input"]).unwrap(),
            (768, Some(PathBuf::from("queries.json")), true)
        );
        for args in [
            vec!["256", "--single-input", "--single-input"],
            vec!["256", "--unknown"],
            vec!["256", "one.json", "two.json"],
            vec!["512"],
            vec![],
        ] {
            assert!(parse(&args).is_err());
        }
    }

    #[test]
    fn diagnostic_counts_ignore_padding_and_truncation_but_include_special_tokens() {
        use tokenizers::models::wordlevel::WordLevel;
        use tokenizers::pre_tokenizers::whitespace::Whitespace;
        use tokenizers::processors::template::TemplateProcessing;
        use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

        let model = WordLevel::builder()
            .vocab([("<unk>".to_string(), 0)].into_iter().collect())
            .build()
            .unwrap();
        let mut tokenizer = Tokenizer::new(model);
        tokenizer.with_pre_tokenizer(Some(Whitespace));
        tokenizer.with_post_processor(Some(
            TemplateProcessing::builder()
                .try_single("[CLS] $A [SEP]")
                .unwrap()
                .special_tokens(vec![("[CLS]", 1), ("[SEP]", 2)])
                .build()
                .unwrap(),
        ));
        tokenizer
            .with_truncation(Some(TruncationParams {
                max_length: 4,
                ..Default::default()
            }))
            .unwrap();
        tokenizer.with_padding(Some(PaddingParams {
            strategy: PaddingStrategy::Fixed(8),
            ..Default::default()
        }));

        let chunks = vec!["one two three four five".into(), "one".into()];
        assert_eq!(
            untruncated_token_counts(tokenizer, &chunks).unwrap(),
            vec![7, 3]
        );
    }

    #[test]
    fn invalid_corpus_is_rejected_before_loading_a_model() {
        for source in [
            r#"{"items":[]}"#,
            r#"{"items":[{"kind":"query","text":" "}]}"#,
            r#"{"items":[{"kind":"unknown","text":"some text"}]}"#,
        ] {
            assert!(parse_corpus(source).is_err());
        }
    }

    #[test]
    fn reference_fixture_and_duplicate_corpus_rows_are_accepted() {
        let fixture = include_str!("../tests/fixtures/embeddinggemma_reference.json");
        assert_eq!(parse_corpus(fixture).unwrap().items.len(), 20);
        let duplicates = r#"{"items":[{"kind":"query","text":"find the runbook"},{"kind":"query","text":"find the runbook"}]}"#;
        assert_eq!(parse_corpus(duplicates).unwrap().items.len(), 2);
    }
}
