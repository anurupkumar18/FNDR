# VS-05 remove the hard word-overlap cutoff from Search (cloud)

## Change

`src-tauri/src/search/reranker.rs` dropped every result sharing under 15% of the query's anchor words (`HARD_COVERAGE_THRESHOLD`) before scoring. That deletion is gone: coverage stays a soft 0.3 weight next to 0.7 vector similarity. `RerankStats` (which only counted the exclusions) and its log line and explain field are deleted with it.

Tests, written first and failing on the old code with "left: 0, right: 1" (the paraphrase result was dropped) and "left: 1, right: 2" (the zero-overlap result was dropped):

- `keeps_a_strong_vector_match_that_shares_no_query_words`: a 0.9 vector match that shares no query words survives with coverage 0.
- `word_coverage_still_breaks_a_vector_tie`: with equal vector scores the result containing the query word ranks first, so coverage still counts.

`cargo test --lib search`: 52 passed.

## Retrieval check (Linux, both personas reseeded, against the train A references)

| Persona | Path | Recall@5 before | after | Paraphrase before | after | Keyword before | after | MRR@10 before | after |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| office-PM | Search | 0.700 | 0.800 | 0.333 | 0.556 | 1.000 | 1.000 | 0.589 | 0.664 |
| knowledge-worker | Search | 0.955 | 0.955 | 0.875 | 0.875 | 1.000 | 1.000 | 0.909 | 0.909 |
| both | Ask | unchanged | | | | | | | |

Queries Search now finds: "is there a way to stop the nudges for just one bill" (miss to rank 1) and "who is filling in during my end-of-year vacation" (miss to rank 2). No query got worse on either path; time and app rows are unchanged. Search's negative queries returned nothing 1 of 4 times on office-PM (was 2 of 4): removing the cutoff means Search shows weak results where it used to show none, which VS-12 handles with a threshold instead of word overlap.

`make qa-retrieval-check` passes for both personas (exit 0).

## What did not help (negative result, reverted)

`HybridSearcher::apply_relevance_gate` (`search/hybrid.rs`) has a second hard overlap rule (minimum term coverage 0.17 to 0.30, and a required anchor term). Removing those two rules as well gave **identical** recall on both personas and made Search return something for every negative query, so it was reverted. The four office-PM paraphrase queries Search still misses are lost to ranking, not to a hard drop: the hybrid reranker multiplies zero-overlap candidates by 0.68 and missing-anchor candidates by 0.22. VS-07/VS-08 (BM25 plus rank fusion) are the place to fix that, and VS-25 deletes this code once Search uses `retrieve`.

## Found on the way (for VS-21)

Ask's rank for "the product requirements doc I drafted last week" moved from 10 to 8 although this change touches no Ask code. A probe on the office-PM profile shows why: the same Ask query puts the target card at rank 2 when Ask runs alone in a fresh process, and at rank 11 when Search ran earlier in the same process. Something process-wide couples the two paths; the first suspect is the text-keyed embedding cache in `src-tauri/src/embedding/onnx.rs`. The gate stays deterministic because the harness always runs Search then Ask in the same order, but in the app Ask results can depend on what was searched before. VS-21 owns the fix.

The references in `scripts/demo/retrieval-reference/` are updated to these runs in the same commit (ratchet), so a later change that loses these queries fails the gate.
