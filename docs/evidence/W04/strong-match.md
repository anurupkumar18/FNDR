# Strong match: unrelated searches no longer look confident

Date: 2026-10-07. Search never returns nothing: for a query about something never captured it returns the nearest memories and should mark them weak, so the surface can say "No strong matches". This note measures how often that mark was wrong and what changed.

## What was wrong

A top result was strong when its fused score reached 0.25, or when every query word was found in it. The bar was set before the query-prompt change raised vector scores (`2026-10-06-query-prompt.md`).

Logging the route signals of each top result showed the cause. For unrelated queries the keyword route gave 0.5 to 0.65 to rows that held none, or one, of the query's whole words, and that lifted the fused score over the bar while closeness in meaning stayed low.

| Corpus | Vector signal of the top result, no-match queries | Vector signal, real queries |
|---|---|---|
| Owner vault copy, 8 unrelated and 131 known-item queries | 0.00 to 0.31 | 5th percentile 0.39, median 0.59 |
| Three personas, 12 labeled negatives | 0.15 to 0.28 | paraphrases reach down to 0.20 |

No single threshold separates paraphrases from negatives, so the vector floor is applied only in the failing case.

## The rule now

`context_runtime::retrieve::is_strong_match`. Unchanged: every word found is strong; otherwise the score must reach the bar. New: the hit is weak when all of these hold: the keyword route found it, the chunk, time and entity routes did not, under a third of the query's words are in it, and its vector signal is under `STRONG_MATCH_MIN_VECTOR` (0.33).

## Result

| Measure | Before | After |
|---|---|---|
| Personas: labeled no-match queries marked strong | 5 of 12 | 0 of 12 |
| Personas: real queries marked weak | 0 of 98 | 6 of 98 |
| of those, right memory was first | 0 | 3 |
| Vault copy: unrelated queries marked strong | 3 of 8 | 0 of 8 |
| Vault copy: right memory first but marked weak, summary gist | 1 of 60 | 1 of 60 |
| Vault copy: same, summary words | 1 of 60 | 1 of 60 |
| Vault copy: same, window title | 0 of 11 | 0 of 11 |

The "before" persona figures are the score against the bar, which is how `retrieval_qa` reported strength until today; it now also prints the flag the product sets. The retrieval gate passes on all three personas. Ranking is untouched: this rule only sets the flag.

## Reading

- The cost is three real persona queries (3 percent) that find the right memory first and are now marked weak. All three are paraphrases or an app-filtered query whose wording shares at most one whole word with the memory. The other three marked weak had a wrong memory first, where weak is the honest mark.
- Word matching is whole-word and exact. "rollback" does not match "rolled back". Light stemming in `matched_terms` would likely recover part of the cost.
- The two conditions were chosen on the same 20 no-match queries they are reported on. Treat the result as fitted until new no-match queries are added.
- First-place counts on the vault copy moved by one to three between identical runs while FNDR was running beside the measurement, so small differences in that column are noise here.

## Also found

- `tests/search_relevance_eval.rs` measured the older `HybridSearcher`. It now runs the live retrieval function (average MRR 0.919 on its 31 real cases, gate 0.72). Its five no-match cases are reported, not gated, because the mock embedder gives no real vectors.
- In that test the "Display Settings" memory, captured in System Settings, is never returned by the live path for its two queries. System Settings is in the default list of excluded apps (`config.rs`), which the live path honors and the older engine did not, so this looks intended; the two cases should be dropped or moved to an app that is not excluded.
