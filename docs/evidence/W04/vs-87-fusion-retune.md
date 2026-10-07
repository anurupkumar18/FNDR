# VS-87: fusion retune, 2026-10-06

## Outcome

No weight change. Nothing in the sweep beat the current defaults by more than one query, so the defaults stay.

## What the ticket got wrong

VS-87 named `search.vector_weight`, `search.snippet_weight` and `search.keyword_weight`. Those config values are read only by `HybridSearcher` in `search/hybrid.rs`. The live Search, Ask and retrieve paths rank with `context_runtime::fusion::fuse` and `FusionWeights` (`context_runtime/context_pack.rs`): vector 0.45, keyword 0.20, with per-intent variants. Setting the config weights to 1, 0, 0 and to 0, 0, 1 produced identical results on office-pm (Recall@5 0.950, MRR@10 0.654). They have no effect on the live path.

In `fuse`, the two branches of the vector route (primary vector and snippet vector) each add `score x 0.45`, so the snippet vector counts as much as the primary one.

## Sweep

A temporary override set the primary-vector, snippet-vector and keyword weights for every intent. It was removed after the run.

Seeded personas, Search path, Recall@5 and MRR@10:

| vector, snippet, keyword | knowledge-worker | office-pm | software-engineer |
|---|---|---|---|
| per-intent defaults (no override) | 1.000 / 0.909 | 0.950 / 0.654 | 1.000 / 0.859 |
| 0.45, 0.45, 0.20 | 1.000 / 0.909 | 1.000 / 0.662 | 1.000 / 0.859 |
| 0.45, 0.30, 0.20 | 1.000 / 0.909 | 1.000 / 0.653 | 1.000 / 0.859 |
| 0.45, 0.20, 0.20 | 1.000 / 0.909 | 0.950 / 0.656 | 1.000 / 0.859 |
| 0.45, 0.10, 0.20 | 1.000 / 0.909 | 0.950 / 0.630 | 1.000 / 0.859 |
| 0.55, 0.20, 0.20 | 1.000 / 0.909 | 1.000 / 0.653 | 1.000 / 0.859 |
| 0.45, 0.20, 0.30 | 1.000 / 0.909 | 0.900 / 0.643 | 1.000 / 0.859 |
| 0.45, 0.30, 0.30 | 1.000 / 0.909 | 0.950 / 0.629 | 1.000 / 0.859 |
| 0.60, 0.15, 0.25 | 1.000 / 0.909 | 0.950 / 0.656 | 1.000 / 0.859 |

Two personas do not respond to any setting. Seeded memories store one vector for both primary and snippet, so the seeded sets cannot separate those two weights at all.

Copy of the owner vault after the re-embed and re-review, 40 sampled memories, found first / in the top five:

| vector, snippet, keyword | Summary gist | Summary first words | Window title |
|---|---|---|---|
| per-intent defaults (no override) | 33 / 37 | 32 / 35 | 6 / 7 of 9 |
| 0.45, 0.45, 0.20 | 33 / 37 | 32 / 35 | 5 / 6 |
| 0.45, 0.30, 0.20 | 33 / 37 | 32 / 35 | 6 / 6 |
| 0.45, 0.20, 0.20 | 32 / 37 | 31 / 35 | 6 / 6 |
| 0.45, 0.10, 0.20 | 33 / 37 | 31 / 35 | 6 / 6 |
| 0.45, 0.20, 0.30 | 32 / 37 | 31 / 35 | 6 / 6 |
| 0.60, 0.15, 0.25 | 32 / 37 | 31 / 35 | 6 / 6 |

## Reading

- Top-five results are identical across every setting on the vault (37 and 35 of 40). The three remaining misses are not a weighting problem.
- The per-intent defaults are the only setting with 7 of 9 on window titles. Uniform weights gain one office-pm query and lose one title query. Neither is a reason to change.
- The MRR movement after the query-prompt change (knowledge-worker 0.966 to 0.909, software-engineer 0.879 to 0.859) is not recoverable by these weights: both are flat across the whole sweep.

## Follow-ups

- Checked 2026-10-07: the config weights are not dead. `HybridSearcher` is a second search engine that still serves the mobile companion (`companion/handlers/search.rs`), the legacy graph (`graph/legacy.rs`), the raw MCP search tool and `tests/search_relevance_eval.rs`. Those callers get none of the work done on the live path (query text contract, snippet branch, intent weights). The fix is to move them onto `context_runtime` and then delete the old engine and its three weights together, not to wire the weights across.
- The seeded corpora need distinct primary and snippet vectors (seed through the capture embedding path) before they can evaluate fusion.
- The remaining misses and the first-to-second moves need a per-query look at the route scores, not a weight sweep.
