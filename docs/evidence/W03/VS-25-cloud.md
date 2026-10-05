# VS-25 retire the search code nothing calls (cloud)

## Moved onto `retrieve`

- `run_search_query` in `src-tauri/src/ipc/commands/search.rs` is now a thin wrapper over `retrieve_search_results`. It backs:
  - the raw `search` and `search_raw_results` commands (the workspace Pipeline Inspector panel);
  - autofill (`autofill.rs`);
  - the quality checks (`quality.rs`).

  It previously ran its own `HybridSearcher` path with a keyword-only fallback. `retrieve` already runs keyword-only when no embedding model is loaded, so the fallback is no longer needed.
- **LLM query expansion moved into `retrieve`.** This fixes a regression from VS-10. The retired Search path asked a loaded local model to widen short abstract queries ("sport" to sports, athletics, match) before embedding them. `retrieve` passed no expansion, so from VS-10 on, Search lost it whenever a local model was loaded. `llm_query_expansion` (`src-tauri/src/search/hybrid.rs`) keeps the old rules: abstract single-concept queries only, a 600 ms limit, and the vector route only. `retrieve_fused` passes its result to the routes, so Ask and agents now get it too. It runs only on the local model; nothing goes off the Mac. The retrieval report runs without a model, so this path is not measured there and has no automated test here: it needs a loaded model.

## Removed

- **From `HybridSearcher`:**
  - `search_with_expansion` and `search_with_expansion_explained`: their only callers were the Search and raw-search paths moved above;
  - `fuse_and_rerank`: no callers;
  - the explain plumbing inside the shared search function, and `branch_trace`, which only that plumbing used;
  - `QueryProfile::embedding_query`: unused; it was a compiler warning before this change.
- **From `reranker.rs`:** `rerank_results` and its weights. Search stopped reranking in VS-10. `anchor_coverage_score` stays, because Search cards group by it.
- **From the IPC search module:** `run_search_query_internal`, and the `HybridSearcher` and `shared_embedder` imports.

```
$ git grep -n "HybridSearcher::\|rerank_results\|search_with_expansion\|fuse_and_rerank\|run_search_query_internal" -- src-tauri
src-tauri/examples/fndr_diagnostic.rs:109:        HybridSearcher::search(
src-tauri/src/companion/handlers/search.rs:41:    let mut results = HybridSearcher::search_hybrid_memories(
src-tauri/src/graph/legacy.rs:388:        let results = HybridSearcher::search(store, embedder, query, limit, None, None).await?;
src-tauri/src/mcp/mod.rs:3812:        HybridSearcher::search(
src-tauri/src/search/hybrid.rs:...  (16 lines, all unit tests of HybridSearcher::rerank and hybrid_fusion; abridged here)
src-tauri/tests/low_signal_surface.rs:106:  (fixture text that mentions rerank_results; not code)
src-tauri/tests/search_relevance_eval.rs:224:                HybridSearcher::search_with_config(...)
```

## Not removed: callers outside this lane keep the engine

`HybridSearcher`'s shared search function, `hybrid_fusion`, `rerank_with_profile`, and the relevance gate still serve five callers. Each needs its owner's decision or a signature change, since `retrieve` takes `&AppState`:

| Caller | What it is | Proposal |
|---|---|---|
| `src/companion/handlers/search.rs` | phone companion search | call `retrieve_search_results` with the companion's `AppState` |
| `src/graph/legacy.rs:388` | legacy graph builder search (has a store and embedder, no `AppState`) | retire with the legacy graph, or pass `AppState` |
| `src/mcp/mod.rs` `memory.search_raw` | raw semantic and keyword lists for agents | PD-11's audit proposes removing the tool |
| `tests/search_relevance_eval.rs` | relevance eval of `HybridSearcher::search_with_config` | port to `retrieve` with the same cases, or retire with the engine |
| `examples/fndr_diagnostic.rs` | developer diagnostic | switch to `retrieve` |

After those, the rest of `hybrid.rs`, except `QueryProfile`, which the routes use, can go.

## Test counts

- `cargo test --lib`: 907 to 906.
  - Removed: two VS-05 tests of `rerank_results`, `keeps_a_strong_vector_match_that_shares_no_query_words` and `word_coverage_still_breaks_a_vector_tie`. The function they tested is gone. `retrieve` keeps meaning-only matches, which `retrieve_explains_which_routes_and_words_found_each_hit` checks.
  - Added: `coverage_counts_the_query_words_a_result_contains`, for the function that stays.
- All integration targets pass: `retrieve` (12), `end_to_end_fndr_query`, `search_flow`, `resume_work`, `agent_regression`, `anti_overfitting`, `chunk_retrieval_quality`, `low_signal_surface`, `memory_browse`, `retrieval_routes`, `search_relevance_eval`, `store_graph_regressions`, and `query_plan_rules`.
- `cargo test --example retrieval_qa`: 16 passed. All examples build.

## Retrieval gate, before and after, same seed

Both personas were seeded once under America/Denver time. The gate then ran on f44f0a1 and on this change. All four runs exit 0.

- Recall@5, MRR@10, every per-query rank, and the no-match counts are identical on all three paths: knowledge-worker 1.000 and 0.966, office-PM 0.900 and 0.661.
- p50 moved within noise: Search 225 to 216 and 218 to 216 ms; `retrieve` 192 to 190 and 192 to 205 ms.
- This is expected. The report runs without a local model, so the expansion never fires, and the report already measured Search through `retrieve`.
