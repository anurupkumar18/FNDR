# VS-17 embedding bake-off (cloud)

Quality numbers for the four VS-17 candidates (plus EmbeddingGemma cut to 256 dimensions) on both labeled personas, from `scripts/audit/embedding_bakeoff.py`. Latency and memory for the decision come from the M1 rerun below; the cost table here is from a shared cloud container and only its ratios are indicative.

- Where: Linux x86_64 cloud container, 4 vCPUs (Intel Xeon @ 2.10GHz), 15 GB RAM, no GPU, branch `claude/train-d-chunks` on top of `aed1315`. Other sessions shared the container, so a model's timings moved by up to about 4 times between runs before the harness switched to the fastest of 3 passes; in the final run below the two EmbeddingGemma rows (same weights) agree within 4%, but Qwen3's passes still varied up to 6 times (all passes are in the JSON as `pass_ms`).
- Command: `HF_HOME=<scratch> python scripts/audit/embedding_bakeoff.py --models all --personas all --out-json docs/evidence/W03/VS-17-bakeoff-cloud.json --out-md <scratch>/vs17.md` (exit 0).
- Determinism: three full runs gave identical quality columns and per-query ranks.
- Tests: `python3 -m unittest scripts/audit/test_embedding_bakeoff.py`: 31 tests, OK (written first; the first run failed with `ModuleNotFoundError: No module named 'embedding_bakeoff'`).
- EmbeddingGemma: `google/embeddinggemma-300m` is gated and failed without a token (recorded in the output); the harness fell back to `unsloth/embeddinggemma-300m@bfa3c846`, whose weight and config files have the same sha256 and git blob ids as the Google repo at `57c266a7` (checked through the Hugging Face API).
- JSON: `docs/evidence/W03/VS-17-bakeoff-cloud.json` (same run): metrics, per-query ranks, and model metadata.
- Decision draft: `docs/decisions/019-embedding-model.md`.

## Script output (pasted unedited)

# Embedding bake-off (VS-17)

Generated 2026-10-04T23:39:33+00:00 on Linux-6.18.44-fc-v70-x86_64-with-glibc2.39 (x86_64, 4 CPUs, device cpu).
Pure vector retrieval: cosine over L2-normalized vectors, top 10, ties by memory id. Recall@5 is case-level over keyword and paraphrase cases (the headline); MRR@10 over the same cases.
record = one vector per memory from the app-like primary text; chunks = window title plus OCR in windows of about 300 tokens (60 overlap, 4 chars per token), memory score = best chunk + 0.01 per other chunk within 0.05 of the best (at most 3).
Timings and memory are from this host through PyTorch on CPU: compare models with each other only. Document timings are the fastest of 3 passes (batches of 16); query timings pool 3 passes of single-query encodes. Peak RSS is per model process and includes the PyTorch runtime.

Corpus: knowledge-worker 19 searchable memories, 39 queries (14 keyword, 8 paraphrase, 8 time, 5 app, 4 negative); office-pm 39 searchable memories, 37 queries (11 keyword, 9 paraphrase, 8 time, 5 app, 4 negative).

## Retrieval quality

Neg/Pos median top = median top-1 cosine on negative / positive queries; Gap = their difference (raw cosine, so not comparable across models); AUC = chance a positive query's top score beats a negative query's (scale free).

| Persona | Mode | Model | Recall@5 | MRR@10 | Keyword R@5 | Paraphrase R@5 | Time R@5 | App R@5 | Neg median top | Pos median top | Gap | AUC |
|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| knowledge-worker | record | minilm | 0.955 (21/22) | 0.846 | 0.929 | 1.000 | 1.000 | 1.000 | 0.149 | 0.502 | 0.353 | 0.986 |
| knowledge-worker | record | bge-small | 0.955 (21/22) | 0.863 | 0.929 | 1.000 | 1.000 | 1.000 | 0.584 | 0.719 | 0.135 | 0.986 |
| knowledge-worker | record | embeddinggemma | 0.955 (21/22) | 0.895 | 0.929 | 1.000 | 1.000 | 1.000 | 0.188 | 0.437 | 0.249 | 0.986 |
| knowledge-worker | record | embeddinggemma-256 | 0.955 (21/22) | 0.904 | 0.929 | 1.000 | 1.000 | 1.000 | 0.292 | 0.528 | 0.236 | 1.000 |
| knowledge-worker | record | qwen3 | 0.955 (21/22) | 0.819 | 0.929 | 1.000 | 1.000 | 1.000 | 0.294 | 0.559 | 0.264 | 0.943 |
| knowledge-worker | chunks | minilm | 0.955 (21/22) | 0.832 | 0.929 | 1.000 | 0.875 | 1.000 | 0.190 | 0.489 | 0.298 | 0.986 |
| knowledge-worker | chunks | bge-small | 0.955 (21/22) | 0.878 | 0.929 | 1.000 | 1.000 | 1.000 | 0.564 | 0.690 | 0.126 | 0.957 |
| knowledge-worker | chunks | embeddinggemma | 1.000 (22/22) | 0.856 | 1.000 | 1.000 | 1.000 | 1.000 | 0.171 | 0.442 | 0.270 | 0.986 |
| knowledge-worker | chunks | embeddinggemma-256 | 0.955 (21/22) | 0.871 | 0.929 | 1.000 | 1.000 | 1.000 | 0.258 | 0.520 | 0.262 | 0.986 |
| knowledge-worker | chunks | qwen3 | 1.000 (22/22) | 0.770 | 1.000 | 1.000 | 0.875 | 1.000 | 0.293 | 0.505 | 0.212 | 0.957 |
| office-pm | record | minilm | 0.900 (18/20) | 0.688 | 1.000 | 0.778 | 0.750 | 1.000 | 0.233 | 0.476 | 0.243 | 0.932 |
| office-pm | record | bge-small | 0.900 (18/20) | 0.747 | 1.000 | 0.778 | 0.875 | 1.000 | 0.623 | 0.737 | 0.114 | 0.894 |
| office-pm | record | embeddinggemma | 1.000 (20/20) | 0.933 | 1.000 | 1.000 | 1.000 | 1.000 | 0.227 | 0.490 | 0.263 | 0.977 |
| office-pm | record | embeddinggemma-256 | 1.000 (20/20) | 0.910 | 1.000 | 1.000 | 1.000 | 1.000 | 0.317 | 0.580 | 0.263 | 0.992 |
| office-pm | record | qwen3 | 0.800 (16/20) | 0.756 | 0.909 | 0.667 | 1.000 | 1.000 | 0.368 | 0.522 | 0.154 | 0.970 |
| office-pm | chunks | minilm | 0.950 (19/20) | 0.662 | 1.000 | 0.889 | 0.875 | 1.000 | 0.193 | 0.479 | 0.285 | 0.992 |
| office-pm | chunks | bge-small | 0.900 (18/20) | 0.704 | 1.000 | 0.778 | 0.875 | 1.000 | 0.603 | 0.715 | 0.112 | 0.932 |
| office-pm | chunks | embeddinggemma | 1.000 (20/20) | 0.925 | 1.000 | 1.000 | 0.875 | 1.000 | 0.223 | 0.492 | 0.269 | 1.000 |
| office-pm | chunks | embeddinggemma-256 | 1.000 (20/20) | 0.879 | 1.000 | 1.000 | 0.875 | 1.000 | 0.298 | 0.555 | 0.256 | 1.000 |
| office-pm | chunks | qwen3 | 0.900 (18/20) | 0.715 | 1.000 | 0.778 | 0.875 | 1.000 | 0.337 | 0.500 | 0.163 | 0.939 |
| both | record | minilm | 0.929 (39/42) | 0.771 | 0.960 | 0.882 | 0.875 | 1.000 | 0.179 | 0.482 | 0.302 | 0.969 |
| both | record | bge-small | 0.929 (39/42) | 0.807 | 0.960 | 0.882 | 0.938 | 1.000 | 0.592 | 0.726 | 0.135 | 0.952 |
| both | record | embeddinggemma | 0.976 (41/42) | 0.913 | 0.960 | 1.000 | 1.000 | 1.000 | 0.207 | 0.455 | 0.248 | 0.982 |
| both | record | embeddinggemma-256 | 0.976 (41/42) | 0.907 | 0.960 | 1.000 | 1.000 | 1.000 | 0.296 | 0.536 | 0.241 | 0.991 |
| both | record | qwen3 | 0.881 (37/42) | 0.789 | 0.920 | 0.824 | 1.000 | 1.000 | 0.322 | 0.532 | 0.210 | 0.950 |
| both | chunks | minilm | 0.952 (40/42) | 0.751 | 0.960 | 0.941 | 0.875 | 1.000 | 0.193 | 0.487 | 0.294 | 0.991 |
| both | chunks | bge-small | 0.929 (39/42) | 0.795 | 0.960 | 0.882 | 0.938 | 1.000 | 0.588 | 0.700 | 0.112 | 0.943 |
| both | chunks | embeddinggemma | 1.000 (42/42) | 0.889 | 1.000 | 1.000 | 0.938 | 1.000 | 0.192 | 0.460 | 0.268 | 0.993 |
| both | chunks | embeddinggemma-256 | 0.976 (41/42) | 0.875 | 0.960 | 1.000 | 0.938 | 1.000 | 0.261 | 0.534 | 0.273 | 0.991 |
| both | chunks | qwen3 | 0.952 (40/42) | 0.744 | 1.000 | 0.882 | 0.875 | 1.000 | 0.315 | 0.505 | 0.189 | 0.954 |

## Against MiniLM on the headline cases (paired, per query)

Gains / losses = keyword or paraphrase cases this model recalls at 5 and MiniLM misses, or the reverse. RR better / worse = cases where the reciprocal rank within the top 10 is higher or lower than MiniLM's. p = two-sided exact sign test (McNemar) on those discordant cases.

| Persona | Mode | Model | Gains | Losses | p (Recall@5) | RR better | RR worse | p (RR) |
|---|---|---|---:|---:|---:|---:|---:|---:|
| knowledge-worker | record | bge-small | 0 | 0 | 1.000 | 4 | 1 | 0.375 |
| knowledge-worker | record | embeddinggemma | 0 | 0 | 1.000 | 2 | 1 | 1.000 |
| knowledge-worker | record | embeddinggemma-256 | 0 | 0 | 1.000 | 4 | 2 | 0.688 |
| knowledge-worker | record | qwen3 | 0 | 0 | 1.000 | 1 | 2 | 1.000 |
| knowledge-worker | chunks | bge-small | 1 | 1 | 1.000 | 5 | 3 | 0.727 |
| knowledge-worker | chunks | embeddinggemma | 1 | 0 | 1.000 | 5 | 3 | 0.727 |
| knowledge-worker | chunks | embeddinggemma-256 | 1 | 1 | 1.000 | 5 | 4 | 1.000 |
| knowledge-worker | chunks | qwen3 | 1 | 0 | 1.000 | 2 | 7 | 0.180 |
| office-pm | record | bge-small | 1 | 1 | 1.000 | 4 | 3 | 1.000 |
| office-pm | record | embeddinggemma | 2 | 0 | 0.500 | 8 | 1 | 0.039 |
| office-pm | record | embeddinggemma-256 | 2 | 0 | 0.500 | 7 | 1 | 0.070 |
| office-pm | record | qwen3 | 0 | 2 | 0.500 | 5 | 4 | 1.000 |
| office-pm | chunks | bge-small | 0 | 1 | 1.000 | 5 | 3 | 0.727 |
| office-pm | chunks | embeddinggemma | 1 | 0 | 1.000 | 9 | 0 | 0.004 |
| office-pm | chunks | embeddinggemma-256 | 1 | 0 | 1.000 | 9 | 1 | 0.021 |
| office-pm | chunks | qwen3 | 1 | 2 | 1.000 | 5 | 2 | 0.453 |
| both | record | bge-small | 1 | 1 | 1.000 | 8 | 4 | 0.388 |
| both | record | embeddinggemma | 2 | 0 | 0.500 | 10 | 2 | 0.039 |
| both | record | embeddinggemma-256 | 2 | 0 | 0.500 | 11 | 3 | 0.057 |
| both | record | qwen3 | 0 | 2 | 0.500 | 6 | 6 | 1.000 |
| both | chunks | bge-small | 1 | 2 | 1.000 | 10 | 6 | 0.454 |
| both | chunks | embeddinggemma | 2 | 0 | 0.500 | 14 | 3 | 0.013 |
| both | chunks | embeddinggemma-256 | 2 | 1 | 1.000 | 14 | 5 | 0.064 |
| both | chunks | qwen3 | 2 | 2 | 1.000 | 7 | 9 | 0.804 |

## Cost on this host (relative only)

| Model | Loaded from | Dim | Download MB | Peak RSS MB | RSS after imports MB | Load s | ms per record text | ms per chunk | Query ms p50 | Query ms p95 | Max input tokens | Truncated |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| minilm | sentence-transformers/all-MiniLM-L6-v2@1110a24 | 384 | 92 | 636 | 434 | 0.6 | 15.4 | 8.7 | 6.0 | 9.8 | 245 of 256 | 0 |
| bge-small | BAAI/bge-small-en-v1.5@5c38ec7 | 384 | 135 | 678 | 434 | 0.8 | 29.6 | 15.8 | 12.3 | 15.4 | 245 of 512 | 0 |
| embeddinggemma | unsloth/embeddinggemma-300m@bfa3c84 | 768 | 1270 | 1517 | 434 | 1.8 | 130.1 | 73.9 | 54.3 | 63.7 | 288 of 2048 | 0 |
| embeddinggemma-256 | unsloth/embeddinggemma-300m@bfa3c84 | 256 | 1270 | 1521 | 434 | 1.8 | 134.1 | 76.5 | 52.9 | 67.3 | 288 of 2048 | 0 |
| qwen3 | Qwen/Qwen3-Embedding-0.6B@97b0c61 | 1024 | 1207 | 4629 | 434 | 2.3 | 1644.9 | 1086.7 | 228.0 | 2236.0 | 277 of 32768 | 0 |

Chunking: knowledge-worker 19 chunks for 19 memories (at most 1 per memory); office-pm 39 chunks for 39 memories (at most 1 per memory).
Real tokens per estimated token (chars / 4), worst input in chunks mode: minilm 1.78, bge-small 1.78, embeddinggemma 1.78, embeddinggemma-256 1.78, qwen3 1.74.
Versions: torch 2.8.0+cpu, transformers 4.57.6, sentence-transformers 5.1.2; 4 torch threads.

Sources that failed before the one used: embeddinggemma: google/embeddinggemma-300m@57c266a740f5: OSError: You are trying to access a gated repo. / embeddinggemma-256: google/embeddinggemma-300m@57c266a740f5: OSError: You are trying to access a gated repo.

## Models

| Model | Query prompt | Document prompt | Pooling | License | ONNX for the app's ort runtime |
|---|---|---|---|---|---|
| minilm (sentence-transformers/all-MiniLM-L6-v2) | '' | '' | mean, L2 normalized | Apache-2.0 | In use today: Xenova/all-MiniLM-L6-v2 onnx/model.onnx, 90.4 MB fp32 (pinned in model_config.rs); int8 builds about 23 MB in the sentence-transformers repo. App pools by mean (matches). |
| bge-small (BAAI/bge-small-en-v1.5) | 'Represent this sentence for searching relevant passages: ' | '' | CLS token, L2 normalized | MIT | Official BAAI/bge-small-en-v1.5 onnx/model.onnx, 133.1 MB fp32; Xenova/bge-small-en-v1.5 onnx/model_quantized.onnx, 34.0 MB int8. App must pool by CLS (it pools by mean today). |
| embeddinggemma (google/embeddinggemma-300m) | 'task: search result \| query: ' | 'title: none \| text: ' | mean, two dense layers, L2 normalized | Gemma Terms of Use (google repo is gated; mirrors carry the same terms) | onnx-community/embeddinggemma-300m-ONNX (ungated): fp32 1235 MB, q8 309 MB, q4 197 MB; graph outputs a pooled sentence_embedding. No fp16 (activations overflow, per the model card). |
| embeddinggemma-256 (google/embeddinggemma-300m (Matryoshka 256)) | 'task: search result \| query: ' | 'title: none \| text: ' | as embeddinggemma, first 256 dims, L2 normalized | Gemma Terms of Use | Same ONNX builds as embeddinggemma; the app slices and re-normalizes. |
| qwen3 (Qwen/Qwen3-Embedding-0.6B) | 'Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery:' | '' | last token, L2 normalized | Apache-2.0 | onnx-community/Qwen3-Embedding-0.6B-ONNX (license field unset; upstream Apache-2.0): fp32 2400 MB, int8 614 MB, fp16 1200 MB. App must pool by last token. |

## Rank of the first relevant memory per query (record / chunks; miss = not in top 10)

| Persona | Query | Kind | minilm | bge-small | embeddinggemma | embeddinggemma-256 | qwen3 |
|---|---|---|---|---|---|---|---|
| knowledge-worker | pandas import error fix | keyword | 1 / 1 | 1 / 3 | 1 / 3 | 1 / 3 | 1 / 2 |
| knowledge-worker | how did I get python to find the data library last time | paraphrase | 3 / 1 | 3 / 1 | 3 / 3 | 1 / 3 | 3 / 1 |
| knowledge-worker | what does Dana want for the survey readout | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | churn drivers due Thursday | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | what customers complained about during onboarding | paraphrase | 3 / 1 | 2 / 1 | 1 / 1 | 1 / 1 | 3 / 3 |
| knowledge-worker | single sign-on | keyword | 1 / 5 | 2 / 7 | 1 / 3 | 2 / 6 | 4 / 5 |
| knowledge-worker | essay rubric weights | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | how much of my history grade depends on sources | paraphrase | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | Field Order 15 | keyword | 9 / 9 | 7 / 2 | 9 / 1 | 7 / 1 | 10 / 1 |
| knowledge-worker | labor contracts passage page 112 | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | counterargument paragraph | keyword | 1 / 2 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | weekday vs weekend ridership chart | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | which plotting library am I allowed to use | paraphrase | 1 / 2 | 1 / 1 | 1 / 2 | 1 / 1 | 1 / 3 |
| knowledge-worker | who is recording the backup video | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | architecture slide | keyword | 1 / 1 | 1 / 3 | 1 / 3 | 1 / 3 | 1 / 2 |
| knowledge-worker | how long is the capstone demo script | paraphrase | 2 / 2 | 1 / 1 | 1 / 1 | 1 / 1 | 2 / 4 |
| knowledge-worker | spring enrollment | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | duplicate survey responses removed | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 2 |
| knowledge-worker | food bank inventory app presentation | paraphrase | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | transit dataset assignment requirements | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | my thesis about why Reconstruction failed | paraphrase | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | the problem our capstone app solves | paraphrase | 3 / 2 | 2 / 1 | 4 / 1 | 4 / 1 | 2 / 3 |
| knowledge-worker | the essay draft I was working on earlier today | time | 1 / 2 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 3 |
| knowledge-worker | the pandas error I hit earlier today | time | 3 / 3 | 2 / 1 | 2 / 1 | 2 / 2 | 1 / 2 |
| knowledge-worker | the capstone standup two days ago | time | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | the ridership charts I worked on two days ago | time | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | the survey themes I coded yesterday | time | 2 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | the course reader chapter I read yesterday | time | 1 / miss | 1 / 5 | 1 / 1 | 1 / 1 | 1 / miss |
| knowledge-worker | what Dana asked me to do in Slack | app | 1 / 1 | 1 / 1 | 2 / 1 | 2 / 2 | 5 / 3 |
| knowledge-worker | the labor contracts passage in the PDF | app | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | the survey data in Sheets | app | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| knowledge-worker | the pandas error in Terminal | app | 1 / 2 | 2 / 1 | 1 / 1 | 1 / 1 | 1 / 2 |
| knowledge-worker | the pandas error from five days ago | time | 1 / 1 | 2 / 2 | 2 / 2 | 1 / 1 | 1 / 1 |
| knowledge-worker | the weekday vs weekend chart from two days ago | time | 1 / 2 | 2 / 2 | 2 / 2 | 2 / 2 | 2 / 2 |
| knowledge-worker | the pandas page I had open in Chrome | app | 1 / 1 | 1 / 3 | 2 / 3 | 2 / 3 | 3 / 1 |
| office-pm | how much money are we losing to customers leaving | paraphrase | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| office-pm | why were our payment nudges ending up in junk mail | paraphrase | 1 / 1 | 2 / 2 | 1 / 1 | 1 / 1 | 1 / 1 |
| office-pm | what pay range did leadership sign off on for the design hire | paraphrase | 3 / 7 | 1 / miss | 1 / 2 | 1 / 2 | 1 / 3 |
| office-pm | which applicant came out on top for the design role | paraphrase | 4 / 5 | 2 / 1 | 1 / 1 | 1 / 1 | 2 / 4 |
| office-pm | what share of new accounts got up and running quickly last quarter | paraphrase | 2 / 2 | 3 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| office-pm | would clients recommend us, and did that improve | paraphrase | 6 / 5 | 6 / 3 | 3 / 1 | 1 / 4 | miss / miss |
| office-pm | the applicant's past project about getting tradespeople paid on time | paraphrase | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| office-pm | is there a way to stop the nudges for just one bill | paraphrase | 3 / 2 | 10 / 6 | 1 / 1 | 5 / 3 | 6 / 8 |
| office-pm | who is filling in during my end-of-year vacation | paraphrase | miss / 5 | 2 / 2 | 1 / 1 | 1 / 1 | miss / 3 |
| office-pm | SMB churn 3.9% by segment | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| office-pm | LL-1482 spam placement 1.8% | keyword | 2 / 4 | 2 / 3 | 3 / 2 | 2 / 2 | 3 / 4 |
| office-pm | Rafael O. staging retest 2.1% | keyword | 1 / 4 | 1 / 4 | 1 / 1 | 1 / 1 | 1 / 1 |
| office-pm | Tobias W. interview feedback | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| office-pm | net new ARR $1.24M | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| office-pm | REQ-2207 pipeline stages | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 8 / 1 |
| office-pm | Smart Reminders launch checklist | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| office-pm | launch checklist with the go/no-go meeting checked off | keyword | 3 / 2 | 3 / 2 | 1 / 2 | 2 / 1 | 1 / 2 |
| office-pm | Hannah K. customer announcement email | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| office-pm | Owen P. churn excluding trial cancels | keyword | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| office-pm | Victor S. Q3 metrics summary email | keyword | 3 / 2 | 1 / 2 | 1 / 1 | 1 / 1 | 1 / 2 |
| office-pm | the hiring debrief from yesterday | time | 7 / 2 | 1 / 2 | 1 / 1 | 2 / 1 | 1 / 1 |
| office-pm | the Q3 numbers email I sent this morning | time | 1 / 1 | 1 / 2 | 1 / 1 | 1 / 1 | 2 / 1 |
| office-pm | the launch checklist I updated yesterday | time | 3 / 2 | 2 / 4 | 2 / 2 | 2 / 1 | 1 / 2 |
| office-pm | the launch review deck from two days ago | time | 1 / 2 | 1 / 4 | 1 / 2 | 1 / 1 | 1 / 5 |
| office-pm | the product requirements doc I drafted last week | time | miss / 8 | miss / miss | 1 / miss | 4 / miss | 1 / miss |
| office-pm | what Rafael said about the spam fix earlier today | time | 3 / 1 | 3 / 1 | 1 / 1 | 1 / 1 | 4 / 1 |
| office-pm | the LL-1482 thread in Slack | app | 2 / 1 | 2 / 1 | 2 / 1 | 2 / 1 | 1 / 1 |
| office-pm | the candidate's portfolio case study in the PDF | app | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 3 |
| office-pm | Q3 churn numbers in Sheets | app | 2 / 1 | 2 / 2 | 1 / 1 | 1 / 2 | 1 / 2 |
| office-pm | the offsite venue vote on Zoom | app | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 |
| office-pm | the launch checklist from three days ago | time | 1 / 1 | 1 / 1 | 1 / 1 | 1 / 1 | 5 / 1 |
| office-pm | the LL-1482 spam discussion from four days ago | time | 3 / 1 | 4 / 1 | 1 / 1 | 1 / 1 | 2 / 1 |
| office-pm | the churn breakdown in Excel | app | 1 / 1 | 3 / 1 | 1 / 1 | 2 / 1 | 3 / 1 |

## Rerun on the M1 8 GB (local)

The cloud host measures quality; latency and memory need the reference Mac. From the repo root:

```
python3 -m venv /tmp/fndr-bakeoff
/tmp/fndr-bakeoff/bin/pip install -r scripts/audit/requirements-bakeoff.txt
python3 -m unittest scripts/audit/test_embedding_bakeoff.py      # pure parts, no torch needed
/tmp/fndr-bakeoff/bin/python scripts/audit/embedding_bakeoff.py --models all --personas all \
    --out-json /tmp/VS-17-bakeoff-m1.json --out-md /tmp/VS-17-bakeoff-m1.md
```

- Close other heavy apps first; each model runs in its own process, so the Peak RSS column is that model alone plus the PyTorch runtime (see "RSS after imports").
- Weights land in the Hugging Face cache (`~/.cache/huggingface`, or `HF_HOME`), about 2.8 GB for all five rows plus 0.1 GB for the VS-19 reranker. Nothing is written into the repo.
- If Qwen3 at fp32 pushes the 8 GB Mac into swap, its timings are not meaningful: run it alone (`--models qwen3`) and record the swap, or drop it with `--models minilm,bge-small,embeddinggemma,embeddinggemma-256`.
- `--timing-repeats 1` shortens the run; the quality columns do not depend on it.
- The quality columns should match this file exactly or within a near tie (different CPU math can swap two memories whose scores differ in the sixth decimal). Paste the M1 "Cost" table into ADR 019 (ms per chunk and peak RSS columns).
- These timings are PyTorch on CPU. The app runs ONNX through `ort`; ms per chunk in the app path is measured separately once a candidate is wired (Minh's embedding tickets).
- With a Hugging Face token that has accepted the Gemma terms, `google/embeddinggemma-300m` loads directly; without one the script falls back to the byte-identical `unsloth/embeddinggemma-300m` mirror and says so.
