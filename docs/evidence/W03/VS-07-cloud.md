# VS-07 BM25 full-text index for keyword search (cloud; also closes VS-06)

## Change

`Store::keyword_search` (`src-tauri/src/storage/lance_store/mod.rs`) used to run one `LOWER(col) LIKE '%term%'` scan per query term over seven columns, keep the first rows each scan returned (`.limit` before scoring), and score them by word coverage. It now runs one LanceDB full-text query with BM25 scoring:

- One inverted index per column on `window_title`, `clean_text`, `snippet`, `memory_context`, `lexical_shadow`, `url`, `app_name` (`FTS_COLUMNS`), queried together. LanceDB combines columns by taking each row's best column score (a `MultiMatch`, deduplicated by max). Tokenizer defaults: lowercase, English stemming, stop words removed, ASCII folding.
- Indexes are created on the first keyword search that finds them missing (idempotent: one `list_indices` call per query; an overwrite of the table drops indexes and they are rebuilt).
- Rows written after the index are still found: LanceDB scans unindexed rows flat. `insert_memory_batch` counts new rows; once 256 have accumulated, the next keyword search folds them into the index in the background (`optimize_fts_indexes`, claimed with a compare-exchange so two searches cannot start two folds). Building the index resets the count.
- Score: `bm25 / (bm25 + 2)` blended 0.86 with 0.14 recency, as the old lexical score was. Ties break by newest, then id.
- `lexical_keyword_score` (the old coverage scorer) is deleted; nothing else used it.
- Not indexed: `search_aliases`. Adding it made a short generated alias list outrank the row that contains the whole phrase (the max-over-columns rule plus BM25 length normalization favor short fields); recall is unchanged without it on both personas.

Why the saturating score and not "divide by the best hit": dividing by the best hit gives the top keyword hit 1.0 even when it matched one weak word, which swamped the vector route in Ask's score-weighted fusion. Measured through the gate: Ask paraphrase Recall@5 fell from 1.000 to 0.875 (knowledge-worker) and from 0.778 to 0.667 (office-PM), and office-PM Ask Recall@5 fell 0.900 to 0.850, at the 0.05 limit. With `bm25 / (bm25 + 2)` no Recall@5 moved. The constant 2 is a placeholder: VS-08 fuses by rank, which makes the score scale irrelevant.

## Tests (`src-tauri/src/storage/lance_store/tests.rs`)

Seven tests, written first. On the old LIKE code three failed and four passed (the guards):

| Test | Old code | BM25 |
|---|---|---|
| `keyword_search_ranks_the_best_match_first_even_when_stored_last` (VS-06: 500 partial matches, the full match stored last) | fail: `weak-478` ranked first | pass |
| `keyword_search_weights_a_rare_term_above_a_common_one` | fail: a common-word row ranked first | pass |
| `keyword_search_returns_nothing_when_no_word_matches` | fail: returned a hit | pass |
| `keyword_search_matches_word_forms_by_stemming` | pass | pass |
| `keyword_search_ranks_a_row_with_every_query_word_above_partial_matches` | pass | pass |
| `keyword_search_finds_rows_added_after_the_first_search` | pass | pass |
| `keyword_search_keeps_the_app_filter` | pass | pass |

`cargo test --lib`: 891 passed, 0 failed, 10 ignored. Integration: `search_flow` 1, `search_relevance_eval` 1, `resume_work` 2, `low_signal_surface` 1, all passed.

## Latency at 10,000 rows

`cargo test --test storage_scale keyword_search_latency_at_10k_rows -- --ignored --nocapture` (new, ignored by default because timing is machine-dependent; varied synthetic vocabulary, 8 queries x 5). Linux container, unoptimized test build:

```
VS-07 BM25 keyword search on 10000 rows (8 queries x5)
index build: 2333 ms
keyword_search p50: 239.6 ms, p95: 290.3 ms
200 rows written after the index: found 200 of 200 in 127.1 ms before folding
folding 200 rows into the index: 189 ms
```

The ticket's bar is p95 under 300 ms at 10,000 rows: met here at 290 ms in an unoptimized build. Local should rerun on the M1. The one-time index build (2.3 s at 10,000 rows) happens inside the first keyword search after upgrade; if that is too visible, build it at startup in the background (follow-up, not done).

## Retrieval check (both personas reseeded, against the VS-05 references)

| Persona | Path | Recall@5 | MRR@10 | p95 ms before | p95 ms after |
|---|---|---|---|---:|---:|
| knowledge-worker | Search | 0.955 = | 0.909 = | 1357 | 288 |
| knowledge-worker | Ask | 1.000 = | 1.000 to 0.966 | 2790 | 2074 |
| office-PM | Search | 0.800 = | 0.664 = | 827 | 374 |
| office-PM | Ask | 0.900 = | 0.657 to 0.661 | 3033 | 3311 |

Search ranks did not move (its own reranker recomputes lexical scores); its latency fell by 55 to 79 percent because the LIKE scans were most of its time. Ask: office-PM time Recall@5 0.875 to 1.000; per-query moves in both directions on paraphrase ("why were our payment nudges ending up in junk mail" miss to 6, "what pay range..." 7 to 4, "would clients recommend us..." 3 to 9, "which plotting library am I allowed to use" 1 to 4) and one keyword query 2 to 3. Keyword Recall@5 was already 1.000 on both personas, so "keyword rows beat the baseline" cannot show in Recall@5 here; the three failing-then-passing tests are where BM25's gains are proven. The references are ratcheted to these runs.

Measurement note: runs with `QA_SKIP_SEED=1` on a profile seeded hours earlier drift (office-PM Ask MRR 0.636 on an old seed against 0.661 freshly seeded) because recency scores age. Evidence runs always reseed, which is the gate's default.

## VS-06

Superseded by this change, as the plan allows: the VS-06 test (best match stored last among 500 rows) fails on the old code and passes with BM25, and there is no longer an early limit before scoring. VS-06 can move to evidence with this file.
