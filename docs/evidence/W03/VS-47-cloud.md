# VS-47 the ONNX embedder reproduces EmbeddingGemma (cloud)

## Result

The ONNX embedder now matches the model's reference implementation on all 20 sentences, at both dimension options. The ticket's bar is a cosine of at least 0.999 per sentence.

```
$ FNDR_EMBED_MODEL_DIR=/path/to/embeddinggemma \
    cargo test --test embeddinggemma_reference -- --ignored --nocapture
768 dimensions: lowest cosine 0.999978, mean 0.999986
256 dimensions: lowest cosine 0.999980, mean 0.999988
test onnx_embedder_matches_the_reference_at_768_and_256_dimensions ... ok
```

The old code path, which mean-pools the hidden states, was tried once with the same test and then restored. Its vectors are unrelated to the reference: mean cosine 0.007261, lowest -0.088. That is the failure the ticket warned about, measured.

## How it works

- **Pooling, dense layers, and normalization come from the model's own graph.** The onnx-community export (revision `5090578d9565bb06545b4552f76e6bc2c93e4a66`) has two outputs: `last_hidden_state` and `sentence_embedding`. The second already applies mean pooling, both dense layers, and normalization, so the embedder reads it rather than re-implementing them in Rust. `onnx.rs` prefers `sentence_embedding`, then `last_hidden_state`, `token_embeddings`, and finally the first output.
- **MiniLM and BGE are unaffected.** Their exports have `last_hidden_state` only, so they take the same path as before. The lib tests, `embedding_audit`, `chunk_retrieval_quality`, and the retrieval gate all pass unchanged.
- **768 or 256 dimensions.** `embedding_v6_contract(dimensions)` keeps the first `dimensions` values and renormalizes them (Matryoshka truncation). Each size gets its own table: `memories_v6_embeddinggemma_768`, `_512`, `_256`, and `_128`. Only the v6 contract may truncate; for v4 and v5 a size mismatch is still an error.
- **A wrong vector fails loudly.** If a v6 model only provides hidden states, the embedder returns an error ("needs the model's sentence_embedding output") rather than mean-pooling them.
- **Prompts follow the model card.** Queries get `task: search result | query: `, documents get `title: none | text: `. `prefixes::query_text_for` and `document_text_for` pick the prompts by contract. v5 keeps its BGE prefixes and v4 gets trimmed text, as before.

## Reference vectors

- `scripts/audit/embeddinggemma_reference.py` uses sentence-transformers, the model's reference implementation. It runs on `unsloth/embeddinggemma-300m` at revision `bfa3c846ac738e62aa61806ef9112d34acb1dc5a`, an ungated mirror of `google/embeddinggemma-300m`, with the prompts passed explicitly and `normalize_embeddings=True`.
- `src-tauri/tests/fixtures/embeddinggemma_reference.json` holds 10 queries and 10 documents, all synthetic, at 768 dimensions. As a sanity check, each query's best match among the documents is its own document.

## Tests

- `embeddinggemma_reference.rs` is ignored by default. It needs the 1.2 GB export in `FNDR_EMBED_MODEL_DIR`: `model.onnx`, `model.onnx_data`, and `tokenizer.json`.
- Unit tests:
  - `a_matryoshka_vector_is_cut_then_renormalized`;
  - `only_embeddinggemma_may_be_truncated`;
  - `each_contract_gets_its_own_prompts`.
- Lib: 921 passed. The gate on both committed personas: PASS, no per-query rank changed.

## Not done here

- **The v6 contract is not active.** Making it active is VS-48, the M1 decision (latency, RAM, and license), and VS-49, the migration.
- **The export's file layout is the onnx-community one.** VS-49 will need a download entry with a pinned sha256 for `model.onnx` and `model.onnx_data`.
- **These numbers come from Linux x86_64 CPU.** They have not been run on the M1.
