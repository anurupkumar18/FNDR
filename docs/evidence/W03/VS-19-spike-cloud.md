# VS-19 cross-encoder rerank spike (cloud)

Verdict: **rejected** for the embedder ADR 019 provisionally recommends, and **needs-M1** (with a likely latency fail) only if MiniLM stays.

- With EmbeddingGemma the reranker lowers MRR@10 (0.889 to 0.829 at 768 dimensions, 0.875 to 0.829 at 256), so it fails the quality half of the rule (+0.03) outright.
- With MiniLM it raises MRR@10 from 0.751 to 0.831 (+0.080, 14 queries better and 5 worse, sign test p = 0.064), which passes the quality bar, but the gain is office-PM's (+0.150); knowledge-worker gains only +0.016. MiniLM plus the reranker still ranks below EmbeddingGemma alone (0.831 against 0.889).
- Latency on this host is over the 150 ms budget in every group: with each query's fastest of 3 timings, p50 at 30 pairs is 288 to 879 ms and p95 is 345 to 1252 ms. The spread between groups doing the same work comes from other sessions sharing the container, so even the lowest p95 (345 ms) is an upper-noise figure, not a floor; but the work (30 pairs of up to about 200 tokens through a 6-layer MiniLM) is roughly 30 times one MiniLM chunk embedding, which measured 8.7 ms here. The M1 has to show a 2.3 times or larger speedup over the best cloud p95 for MiniLM to keep it.

## Where and how

- Linux x86_64 cloud container, 4 vCPUs (Intel Xeon @ 2.10GHz), PyTorch 2.8 CPU, shared with other sessions; branch `claude/train-d-chunks`.
- Command: `HF_HOME=<scratch> python scripts/audit/embedding_bakeoff.py --models all --personas all --rerank --out-json <scratch>/vs19.json --out-md <scratch>/vs19.md` (exit 0). The embedding tables that the same run printed above this section match `VS-17-bakeoff-cloud.md` exactly (checked: every metric and per-query rank in the JSON is identical), so only the rerank section is pasted.
- Candidates: each embedder's chunk-mode top 30, rescored by `cross-encoder/ms-marco-MiniLM-L-6-v2` at revision `233902d` on (query, the memory's best chunk: window title plus OCR). Knowledge-worker has only 19 searchable memories, so there the reranker orders the whole corpus and every embedder ends at the same reranked list (MRR@10 0.848); office-PM has 39, so 30 pairs per query.
- Tests: `python3 -m unittest scripts/audit/test_embedding_bakeoff.py`: 36 tests, OK (the 5 rerank tests were written first and failed with `AttributeError` until `rerank_candidates`, `apply_rerank`, and `rerank_verdict` existed).

## Script output (rerank section, pasted unedited)

## Cross-encoder rerank of the top 30 (VS-19)

Reranker cross-encoder/ms-marco-MiniLM-L-6-v2@233902d (activation Identity, max length 512) rescores each query's top 30 chunks-mode vector candidates on (query, the memory's best chunk text); fewer than 30 when the persona has fewer memories. Reranked order by cross-encoder score, ties by id; the vector order is kept below the reranked head. Rule: keep only if MRR@10 gains at least 0.03 and the rerank adds under 150 ms at p95 (latency to be confirmed on the M1).

| Persona | Embedder | Pairs per query | MRR@10 vector | MRR@10 reranked | Gain | Recall@5 vector | Recall@5 reranked | Gains | Losses | RR better | RR worse | p (RR) | AUC reranked |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| knowledge-worker | minilm | 19 | 0.832 | 0.848 | +0.016 | 0.955 (21/22) | 0.955 (21/22) | 1 | 1 | 5 | 4 | 1.000 | 1.000 |
| knowledge-worker | bge-small | 19 | 0.878 | 0.848 | -0.029 | 0.955 (21/22) | 0.955 (21/22) | 1 | 1 | 3 | 2 | 1.000 | 1.000 |
| knowledge-worker | embeddinggemma | 19 | 0.856 | 0.848 | -0.008 | 1.000 (22/22) | 0.955 (21/22) | 0 | 1 | 2 | 2 | 1.000 | 1.000 |
| knowledge-worker | embeddinggemma-256 | 19 | 0.871 | 0.848 | -0.023 | 0.955 (21/22) | 0.955 (21/22) | 1 | 1 | 3 | 2 | 1.000 | 1.000 |
| knowledge-worker | qwen3 | 19 | 0.770 | 0.848 | +0.078 | 1.000 (22/22) | 0.955 (21/22) | 0 | 1 | 4 | 2 | 0.688 | 1.000 |
| office-pm | minilm | 30 | 0.662 | 0.812 | +0.150 | 0.950 (19/20) | 1.000 (20/20) | 1 | 0 | 9 | 1 | 0.021 | 0.962 |
| office-pm | bge-small | 30 | 0.704 | 0.808 | +0.104 | 0.900 (18/20) | 1.000 (20/20) | 2 | 0 | 6 | 3 | 0.508 | 0.962 |
| office-pm | embeddinggemma | 30 | 0.925 | 0.808 | -0.117 | 1.000 (20/20) | 0.950 (19/20) | 0 | 1 | 2 | 5 | 0.453 | 0.970 |
| office-pm | embeddinggemma-256 | 30 | 0.879 | 0.807 | -0.072 | 1.000 (20/20) | 0.950 (19/20) | 0 | 1 | 3 | 3 | 1.000 | 0.970 |
| office-pm | qwen3 | 30 | 0.715 | 0.803 | +0.088 | 0.900 (18/20) | 0.950 (19/20) | 2 | 1 | 7 | 2 | 0.180 | 0.970 |
| both | minilm | 19-30 | 0.751 | 0.831 | +0.080 | 0.952 (40/42) | 0.976 (41/42) | 2 | 1 | 14 | 5 | 0.064 | 0.980 |
| both | bge-small | 19-30 | 0.795 | 0.829 | +0.034 | 0.929 (39/42) | 0.976 (41/42) | 3 | 1 | 9 | 5 | 0.424 | 0.980 |
| both | embeddinggemma | 19-30 | 0.889 | 0.829 | -0.060 | 1.000 (42/42) | 0.952 (40/42) | 0 | 2 | 4 | 7 | 0.549 | 0.983 |
| both | embeddinggemma-256 | 19-30 | 0.875 | 0.829 | -0.046 | 0.976 (41/42) | 0.952 (40/42) | 1 | 2 | 6 | 5 | 1.000 | 0.983 |
| both | qwen3 | 19-30 | 0.744 | 0.827 | +0.083 | 0.952 (40/42) | 0.952 (40/42) | 2 | 2 | 11 | 4 | 0.118 | 0.983 |

Gains / losses and RR better / worse compare the reranked list with the same embedder's vector list.

Reranker cost on this host (relative only): load 1.0 s, peak RSS 760 MB (after imports 434 MB), download 92 MB; each query is one predict call over all its pairs, timed 3 times; a query's latency is its fastest timing.

| Embedder | Queries at 30 pairs | ms p50 (30 pairs) | ms p95 (30 pairs) | ms p95 (all queries) |
|---|---:|---:|---:|---:|
| minilm | 37 | 292.4 | 965.9 | 877.5 |
| bge-small | 37 | 788.1 | 1252.1 | 1176.8 |
| embeddinggemma | 37 | 878.8 | 1027.0 | 988.0 |
| embeddinggemma-256 | 37 | 287.6 | 345.2 | 546.6 |
| qwen3 | 37 | 298.8 | 355.5 | 355.4 |

| Embedder | Scope | Verdict | Why |
|---|---|---|---|
| minilm | both | needs-M1 | MRR@10 gain +0.080 meets +0.03; p95 965.9 ms is over 150 ms on this host; confirm latency on the M1 |
| bge-small | both | needs-M1 | MRR@10 gain +0.034 meets +0.03; p95 1252.1 ms is over 150 ms on this host; confirm latency on the M1 |
| embeddinggemma | both | rejected | MRR@10 gain -0.060 is under +0.03 |
| embeddinggemma-256 | both | rejected | MRR@10 gain -0.046 is under +0.03 |
| qwen3 | both | needs-M1 | MRR@10 gain +0.083 meets +0.03; p95 355.5 ms is over 150 ms on this host; confirm latency on the M1 |

## Rerun on the M1 8 GB (local)

Only needed if MiniLM stays (if ADR 019 adopts EmbeddingGemma, the reranker is rejected on quality whatever its latency). With the virtualenv from `VS-17-bakeoff-cloud.md`:

```
/tmp/fndr-bakeoff/bin/python scripts/audit/embedding_bakeoff.py --models minilm,embeddinggemma-256 --rerank \
    --out-md /tmp/VS-19-rerank-m1.md
```

- Keep the reranker only if the MiniLM row's gain stays at or above +0.03 (it should match this file exactly) and "ms p95 (30 pairs)" is under 150 ms.
- These timings are PyTorch on CPU. The app would load the reranker as ONNX through `ort` (the same Hugging Face repo ships `onnx/model.onnx`, 91 MB fp32, and int8 builds of about 23 MB), so a pass here should be confirmed once more in the app path; a fail at 30 pairs could be retried at a smaller depth, which this spike did not measure.
