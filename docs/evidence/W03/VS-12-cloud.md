# VS-12 "No strong matches" (cloud, partly a negative result)

## Result in one paragraph

Search now says "No strong matches" when a query's best result is weak, and folds the weak results behind a "Show N weaker matches" button. The bar is the fused score 0.25 (`STRONG_MATCH_SCORE`); a result that contains every word of the query also counts as strong. On the labeled sets this catches 3 of the 8 no-match queries (2 of 4 knowledge-worker, 1 of 4 office-PM) and no real query falls under it. The ticket's done-when ("negative rows pass on both personas") is **not met**: no threshold on the fused score, and no combination with vector similarity, word coverage, or how far the top score stands above the rest, separates all eight no-match queries from the real ones on these sets. The bar sits where it hides no real match.

## Why no single threshold works (measured)

A probe ran every labeled query through `retrieve` and `run_query` on copies of both seeded profiles (VS-21 code, America/Denver seed). It recorded the best hit's fused score, its vector and keyword signals, and how many query words it contains.

| | knowledge-worker | office-pm |
|---|---|---|
| No-match queries, best fused score | 0.178, 0.190, 0.308, 0.351 | 0.162, 0.284, 0.315, 0.334 |
| Lowest best scores among real queries | 0.290 ("single sign-on"), 0.294, 0.331, 0.335 | 0.330 ("how much money are we losing to customers leaving"), 0.355, 0.359, 0.363 |

- **Fused score:** no-match queries reach 0.351 and real ones go down to 0.290. Any bar that catches all eight hides real queries.
- **Vector similarity plus word coverage:** "organic chemistry lab report" has vector 0.28 and coverage 0.20, while real paraphrase queries sit at vector 0.274 to 0.284 with coverage 0 to 0.14.
- **How far the top score stands above the next four:** this does not separate them either. "SOC 2 audit evidence request" stands 0.012 above the rest, and real queries stand 0.015 to 0.04 above.
- **Why weak results are not hidden one by one:** two real queries ("which plotting library am I allowed to use", "how much money are we losing to customers leaving") have their right answer at score 0.215 or 0.216, ranked below a wrong top hit. Hiding each weak result would hide those answers, so the whole query is judged by its best result.
- **Why the word rule exists:** with no embedding model loaded, only the keyword route scores, and a perfect match ("zephyr vendor contract") fuses to 0.204. Without the word rule every query would say "No strong matches" in that mode. On the labeled sets no no-match query's best hit contains all its words; their best hits match 0 or 1 of their 4 to 5 words.

## What changed

- `context_runtime::retrieve`: a `RetrieveResult.strong_match` flag (best hit at or above `STRONG_MATCH_SCORE` = 0.25, or containing every single-word query term found by the keyword route).
- Search: every card carries `weak_match` (true when the query has no strong match). `search_memory_cards` sets it from `retrieve`'s flag.
- Search screen (`Timeline`): when every card is weak it shows "No strong matches", the query, a hint, and a "Show N weaker matches" button; after expanding, a note says these are the closest saved memories. Results with a strong match render as before.
- Report (`retrieval_qa`) and gate (`retrieval_check.py`): the no-match table adds "negatives under the bar" and "positives under the bar" (reported, not gated). Older reports show n/a.
- Fixed in passing: the comparator test that pinned the committed reference to two paths had failed since VS-09 added `retrieve`; it now checks both personas' references with three paths.
- Test fixtures in `tests/retrieve.rs`, the Search card test, and the MCP contract test lift route time budgets as `retrieval_qa` does. With production budgets, twelve tests in parallel on a debug build dropped keyword hits and failed `search_lists_what_retrieve_found_in_the_same_order` in two of three runs. These tests check ranking, not timing.

## Tests

The new tests failed or did not compile before the change. The Rust paths were also run with the embedding model unreachable (`HOME` pointed at an empty directory), as CI may run them.

- `tests/retrieve.rs`:
  - `retrieve_says_when_nothing_matches_well`: "dentist appointment reminder" is not a strong match, and "zephyr vendor contract" is. Without a model it is strong through the word rule; its score is 0.204.
  - The JSON shape test now includes `strong_match`.
  - 12 passed, five runs in a row, and 12 passed without a model.
- `ipc::commands::search::tests::search_cards_are_weak_when_nothing_matches_well` drives `search_memory_cards`. It passes with and without a model.
- `retrieval_qa`: `no_match_counts_queries_whose_best_result_is_weak`, plus updated JSON-key and markdown tests. 16 passed.
- `cargo test --lib`: 907 passed, three runs in a row.
- Frontend: two Timeline tests, one for the folded state and one showing strong results render as usual. `npm run typecheck` passes; `npm test` gives 73 files, 460 tests passed.
- `python3 scripts/audit/test_retrieval_check.py`: 25 OK.

## Retrieval gate, before and after, same seed

Both personas were seeded once under America/Denver time. The gate then ran on fd03afa and on this change. All four runs exit 0, Recall@5 and MRR@10 are unchanged on every path, and no rank changed.

| Persona | Path | Negative cases | Under the bar (right) | Positives under the bar (wrong) | Median best score, negative / positive |
|---|---|---:|---:|---:|---|
| knowledge-worker | search, ask, retrieve | 4 | 2 | 0 | 0.249 / 0.464 |
| office-pm | search, ask, retrieve | 4 | 1 | 0 | 0.299 / 0.466 |

The report counts by score alone, so it understates strength: the word rule can only make a query strong, never weak.

## Not done here

- No browser-preview screenshot. The preview answers Search from a mock that matches any query word (`src/dev/previewIpc.ts`, which train C also edits). The Timeline tests cover the rendered states.
- Ask has its own "not enough evidence" verifier, and MCP `search_memories` does not yet return the flag. Both are follow-ups.
- Separating the remaining five no-match queries needs a better signal. Chunk-level scores (VS-18) are the next candidate; the cross-encoder (VS-19) was rejected for latency.

References in `scripts/demo/retrieval-reference/` are ratcheted to the after runs.
