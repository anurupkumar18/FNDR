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

## Checked on queries the rule was not fitted to

Twelve new no-match queries were added afterwards, four per persona, written to use common words ("notes", "list", "documents", "meeting", "report") that exist in the corpora. None of their topics appears in a corpus.

| No-match queries marked strong | Old rule (score against the bar) | Rule now |
|---|---|---|
| The 12 the rule was fitted on | 5 | 0 |
| The 12 new ones | 8 | 3 |
| All 24 | 13 | 3 |

Real queries marked weak stayed at 6 of 98. So the rule holds up on new queries but is not complete. Of the three it still passes, one names a time ("last weekend"), where the time route counts as evidence although it says nothing about the topic; one is close to a real memory (a different hiring search); and one ("notes from the podcast about sourdough baking") passed on keyword and vector support alone and was not looked into. `retrieval_qa` now names each no-match query it marks strong.

## Two ideas measured and not adopted

- **Stemming the word match.** The three real queries that lose their strong mark are paraphrases: "vacation" for "PTO", "rollback" for "Rolled back", "new teammate" for "new backend engineer". Stemming recovers none of them.
- **Requiring the top result to stand out.** The gap between the top result's vector signal and the mean of the next three was at most 0.07 for the fitted persona negatives, but 0.11 on the vault's unrelated queries, against 0.15 for the nearest real query it would rescue. Too close to rely on.

## Also found

- `tests/search_relevance_eval.rs` measured the older `HybridSearcher`. It now runs the live retrieval function (average MRR 0.984 on its 31 real cases, gate raised from 0.72 to 0.90). Its five no-match cases are reported, not gated, because the mock embedder gives no real vectors.
- In that test the "Display Settings" memory was captured in System Settings, which is on the default list of excluded apps, so the live path never returned it. The fixture is now a support page in Safari and both of its queries find it first, which confirms the cause.

## A time phrase is not topic evidence, 2026-10-08

The rule let a hit stay strong when the time route had found it. The time route finds every memory in the named time, so an unrelated query with "yesterday" or "last week" on the end was marked a confident match for whatever was on screen then. The time route no longer counts.

Measured by adding a time phrase (" from yesterday", " last week", " from two days ago", " this week") to the 24 no-match queries of the three persona sets, on freshly seeded profiles:

| | Before | After |
|---|---|---|
| Knowledge worker, no-match with a time phrase marked strong | 2 of 8 | 0 of 8 |
| Office PM | 4 of 8 | 1 of 8 |
| Software engineer | 3 of 8 | 0 of 8 |
| Real queries marked weak, all three sets | 6 of 98 | 7 of 98 |

The one real query that became weak is "the product requirements doc I drafted last week": a paraphrase whose top result holds one of its words and is not close in meaning. The other six weak real queries are paraphrases with no time phrase, and were weak before.

The gate passes on all three sets. `retrieval_qa` now names each real query it marks weak, with the routes and the words found.

Note for anyone running the gate: `QA_SKIP_SEED=1` reuses a profile whose dates were written when it was seeded. On a later day every "yesterday" query misses and the gate reports FAIL. Reseed first.
