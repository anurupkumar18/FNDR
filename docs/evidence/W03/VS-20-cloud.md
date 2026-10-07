# VS-20 retrieve at 10,000 memories and 60,000 chunks: harness and cloud numbers (M1 run open)

## What exists

`src-tauri/tests/storage_scale.rs::retrieve_latency_at_10k_memories_and_60k_chunks` is ignored by default because its timings depend on the machine. It seeds:

- 10,000 synthetic memories: varied text drawn from a 40-word vocabulary, readable OCR fields so `retrieve` does not hide them, and deterministic 384-d vectors;
- 10,000 BGE parent rows;
- 60,000 chunks, 6 per memory, with their own text and deterministic 1024-d vectors.

It then:

1. times the chunk route's two store calls on their own;
2. for production route budgets and for lifted budgets, times eight queries three times each through `retrieve` (limit 20) and Search, with the chunk route on;
3. counts empty results and hits that name a chunk;
4. prints each route's median time from Ask's trace.

The test body runs on a 64 MB thread, because debug-build retrieval futures overflow the default test-thread stack.

Run on the M1:

```
cargo test --test storage_scale retrieve_latency -- --ignored --nocapture
```

Set `FNDR_EMBED_MODEL_DIR` to a folder with the BGE model to include chunk vectors.

## Cloud run (Linux, 4 shared vCPUs, unoptimized debug build, BGE-large quantized on CPU)

```
VS-20 scale: 10000 memories, 60000 chunks; seeding 62031 ms
chunk store calls: flat vector p50 654 ms, BM25 p50 84 ms (first BM25 call builds the index)
budgets lifted: route medians in ms: Chunk 1142, Graph 1, Keyword 492, Vector 728
budgets lifted: first query 3922 ms; retrieve p50 1242.1 ms p95 1355.3 ms; Search p50 1256.4 ms p95 1333.2 ms; empty results 0/24; hits naming a chunk 417
production budgets: route medians in ms: Chunk 1243, Graph 1, Keyword 903, Vector 908
production budgets: first query 1251 ms; retrieve p50 1320.1 ms p95 1435.7 ms; Search p50 1352.6 ms p95 1428.4 ms; empty results 0/24; hits naming a chunk 420
```

- **The done-when (p95 at most 500 ms on the M1) is open.** This debug build on a shared container measures p95 1,355 to 1,436 ms. Release builds of LanceDB and ONNX Runtime are much faster, so this number does not predict the M1, and no claim is made either way.
- **The routes run in parallel, so the slowest sets the total.** That is the chunk route, at about 1.1 to 1.2 s. Within it:
  - the flat vector scan over 60,000 by 1024-d chunks takes 654 ms at p50 here;
  - BM25 over chunk text takes 84 ms;
  - the rest is mostly the BGE-large query embedding.
- **Decision rule for the M1 run** (the ticket: "add a vector index only if flat search exceeds the budget"). If the chunk route's median, or the flat chunk scan, keeps p95 over 500 ms, the first step is an ANN index on `memory_chunks_v1_bge_1024.embedding` (LanceDB IVF-PQ). It is not added here, because this build cannot show it is needed.
- **Cold caches drop results.** An earlier run measured production budgets first and returned 4 of 24 queries empty: on a cold cache the routes ran past their budgets and silently returned nothing. Run after the lifted-budget pass, nothing came back empty. This is the same budget behavior VS-04 and the memory-journey CI test show. The first query after start-up (index builds) took 1.1 to 7.2 s across runs.
- **Search equals `retrieve` plus row lookups,** as expected since VS-10: p50 1,256 against 1,242 ms.

## Caveats

- The vectors are synthetic. Flat-scan cost does not depend on content, but the vector route's dedup and the relevance gate may behave differently on real text.
- One machine and one run per configuration; the order effect (cold versus warm) was checked by swapping the order once.
- The existing MEM-08 test in the same file writes `docs/evidence/W04/storage-indexes.md` when run, so run this test by name.
