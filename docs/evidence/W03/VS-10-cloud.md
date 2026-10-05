# VS-10 the Search screen ranks through `retrieve` (cloud)

## What changed

- `search_ranked_results` is now a thin wrapper over `context_runtime::retrieve_search_results`, which is `retrieve` plus each hit's stored row. The rows come from the lookup the hidden-memory drop already makes, so Search does no second scan. `search_memory_cards` (the Search screen), the Memory Journey Search path, and `examples/retrieval_qa.rs` all go through it, so the report measures what users see. Card synthesis is unchanged; Search's coverage reranker no longer reorders anything, and the share of query words each row covers is still computed because card grouping reads it.
- `retrieve` (and therefore Ask) now drops FNDR's own windows, as Search always did; the check moved into the shared hidden-memory drop (`drop_hidden_hits`, formerly `drop_low_signal_hits`).
- Ask and `retrieve` loaded the ONNX embedding model from disk on every query: `Embedder::new()` took 171 to 263 ms per call here (five calls, debug build). They now reuse the process's loaded model when it is the real one, and still build one per query when it is not, so a model downloaded mid-session is picked up as before. No second copy of the model is held.
- Two determinism fixes the existing Memory Journey test needed once Search ran through fusion. That test calls Search twice and requires identical ids and scores. (1) Fusion now breaks score ties by memory id; it collects hits in a `HashMap`, so five equal scores came back in a different order on each call. (2) Keyword and temporal recency count age in whole minutes; at millisecond resolution two calls a moment apart differed in the eighth digit. A one-minute step changes recency by at most 0.05% at a 24-hour half-life. Both were VS-21 items; VS-21 keeps the Ask-after-Search coupling.

## Tests (written first; each failed on the old code)

- `tests/retrieve.rs`:
  - `search_lists_what_retrieve_found_in_the_same_order`: four queries, same ids in the same order, scores within 1e-5 (recency moves between calls); the top row carries `matched_routes` and a coverage computed for this query. It failed on the old code: Search returned only `vendor`, while `retrieve` returned four hits.
  - `retrieve_never_returns_fndr_own_windows`: a stored FNDR window that the keyword index finds is absent from `retrieve` and Search. It failed on the old code for `retrieve`.
- `fusion::tests::fuse_orders_equal_scores_by_memory_id`, `temporal_route::tests::recency_decay_is_the_same_within_a_minute`, `lance_store::tests::keyword_recency_is_the_same_within_a_minute`: all three failed on the old code.
- Results:
  - `cargo test --lib`: 903 passed, 0 failed, 10 ignored. That includes `memory_journey::tests::six_synthetic_journeys_reconstruct_from_temporary_storage_and_search`, which failed before the two determinism fixes.
  - `--test retrieve`: 9 passed, three runs in a row.
  - `end_to_end_fndr_query`, `search_flow`, `resume_work`, `agent_regression`, `anti_overfitting`, `capture_fixtures`, `chunk_retrieval_quality`, `embedding_audit`, `low_signal_surface`, `memory_browse`, `merge_replay`, `query_plan_rules`, `retrieval_routes`, `search_relevance_eval`, `store_graph_regressions`: all pass.
  - `cargo test --example retrieval_qa`: 15 passed.

## Before and after, same seed

Both personas were seeded once under America/Denver time. The gate then ran on the committed code (3dade40) and on this change, with `make qa-retrieval-check QA_SKIP_SEED=1 [PERSONA=office-pm]`. All four runs exit 0.

| Persona | Path | Recall@5 before | Recall@5 after | MRR@10 before | MRR@10 after | p50 ms before | p50 ms after | p95 ms before | p95 ms after |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| knowledge-worker | search | 0.955 | 1.000 | 0.909 | 0.966 | 201 | 228 | 212 | 374 |
| knowledge-worker | ask | 1.000 | 1.000 | 0.966 | 0.966 | 1151 | 866 | 1267 | 973 |
| knowledge-worker | retrieve | 1.000 | 1.000 | 0.966 | 0.966 | 471 | 200 | 613 | 336 |
| office-pm | search | 0.800 | 0.900 | 0.664 | 0.661 | 249 | 232 | 317 | 391 |
| office-pm | ask | 0.900 | 0.900 | 0.661 | 0.661 | 1607 | 1320 | 1745 | 1444 |
| office-pm | retrieve | 0.900 | 0.900 | 0.661 | 0.661 | 470 | 201 | 580 | 321 |

- Office-PM Search by kind: paraphrase Recall@5 0.556 to 0.778, time 0.875 to 1.000; keyword and app stay 1.000. Three keyword queries move from rank 1 to 2 or 3 in Search (the same ranks Ask and `retrieve` already had), which is the -0.003 MRR.
- Ask's ranks are identical before and after on every query of both personas, so sharing the loaded model does not change Ask. `retrieve` gains one query ("the product requirements doc I drafted last week" 5 to 1).
- Latency is from a shared Linux container running a debug build. Search p95 rose by 74 to 162 ms; p50 moved by +27 and -17 ms. `retrieve`'s p50 fell from about 470 to about 200 ms once the model stopped loading per query. VS-20 measures this at 10,000 memories.

## Done when: Search and Ask agree on the top result for every query

Not yet; this lands with VS-11. Top-1 agreement went from 28/39 to 37/39 (knowledge-worker) and from 16/37 to 35/37 (office-PM). Every remaining disagreement is a time or app query: "the pandas error in Terminal", "the weekday vs weekend chart from two days ago", "the product requirements doc I drafted last week", "the LL-1482 spam discussion from four days ago". On these, Search goes through `retrieve`, which reads the phrase as a filter (VS-13). Ask's `run_query` still calls the shared front half with no filters. VS-11 moves Ask onto `retrieve`.

## Known regression until VS-12

Search used to return nothing for most no-match queries: 3 of 4 on knowledge-worker and 1 of 4 on office-PM, a side effect of the word-overlap cutoff VS-05 removed and of the hybrid gate. It now returns weak results for all of them, as Ask and `retrieve` already did. The median top score is 0.25 to 0.30 on negative queries against 0.46 to 0.47 on positive ones. VS-12 sets the threshold on that score. Both tickets are on train B, so they merge together.

## Not done here

- `make qa-preview`: the browser preview answers `search_memory_cards` from a mock in `src/dev/previewIpc.ts`, so it cannot exercise this Rust change; the IPC contract (`MemoryCard[]`) is unchanged and no frontend file changed.
- `run_search_query` (the raw `search` command, autofill, quality checks) still uses the old hybrid searcher; VS-11 and VS-25 move or remove it.

References in `scripts/demo/retrieval-reference/` are ratcheted to the after runs.
