# VS-21 same query, same results, every time (cloud)

Moved ahead of VS-12: its no-match threshold is only meaningful on stable scores.

## Three causes, each found by measurement

1. **The temporal route gave its bonus to a random subset.** `get_search_results_in_range` sets every row's score to a placeholder 1.0, and the route kept `max(1.0, recency)`, so every memory in the asked-about window tied at 1.0 and its recency scoring never counted. It then kept `limit` of them in `HashMap` order. On "the product requirements doc I drafted last week" (office-PM) a probe saw the +0.1 temporal bonus land on `pm-launch-prd`, `pm-hire-jd`, and `pm-hire-scorecard-tobias` on three consecutive calls. The route now scores each memory by its recency within the window.
2. **Queries with two or more numbers embedded different text on each call.** `QueryProfile.number_terms` was a `HashSet`, joined into the text the vector route embeds, so "LL-1482 spam placement 1.8%" was embedded with "1482 1.8" or with "1.8 1482", depending on the set's random order. Embeddings and raw vector search were checked identical across calls; the vector route's score for one memory moved between 0.3435, 0.3492, and 0.34476. It is now a `BTreeSet`.
3. **The ranking depended on the page size.** Every route sized its candidate pool from the caller's limit, so Ask (10 in the report) and Search (20) gave different memories the temporal bonus, and a limit-1 call returned `note-02` where the limit-20 call's first hit was `note-00`. Routes now always gather a pool of 50 (fusion's own cap), and the page size applies after fusion.

Also done:

- **Shared ordering for route hits.** `retrieval_routes::sort_route_hits` orders by score, then newest, then smaller id. The temporal, entity, and graph routes use it before they truncate. The keyword route breaks score-and-timestamp ties by id, and the chunk route breaks score-and-distance ties by id.
- **Fusion.** Ties go to the newer memory, then the smaller id. VS-10's id-only rule was a first step.
- **Card grouping.** Cards, their anchor, and their evidence ids break remaining ties by id, and grouping walks rows newest first, then by id.

## Tests (each failed on the code before it)

- `tests/retrieve.rs`:
  - `the_same_query_returns_the_same_ids_every_time`: 14 similar notes from the last six days and the query "recent launch notes", run five times through `retrieve`, Search, and Ask. The ordered ids must be identical. Before the fix all three paths changed between runs, for example `["note-10", "note-11", "note-03", "note-02", "note-04"]`, then `["note-00", "note-13", "note-10", "note-03", "note-02"]`.
  - `a_short_page_is_the_start_of_a_long_page`: for five queries over two fixtures, `retrieve` with limit 1 to 5 must equal the first hits of limit 20. It failed at limit 1, with `note-02` against `note-00`.
- `fusion::tests::fuse_orders_equal_scores_newest_first_then_by_id`: it was written with the change; the id-only order it replaces gives `a, b, c` where newest-first gives `c, a, b`.
- `search::hybrid::tests::query_embedding_text_lists_numbers_in_a_fixed_order`: 20 profiles of one query must embed one text. It failed on the `HashSet`.
- Results:
  - `cargo test --lib`: 906 passed, 0 failed, 10 ignored.
  - `--test retrieve`: 11 passed, three runs in a row.
  - `end_to_end_fndr_query`, `search_flow`, `resume_work`, `agent_regression`, `anti_overfitting`, `chunk_retrieval_quality`, `low_signal_surface`, `memory_browse`, `retrieval_routes`, `search_relevance_eval`, `store_graph_regressions`, `query_plan_rules`: all pass.
  - `cargo test --example retrieval_qa`: 15 passed.

## Against the seeded profiles: five runs of every labeled query

A probe (not committed) opened a copy of each seeded profile with the evaluation's lifted time budgets. It ran each labeled query five times through `retrieve` (limit 10), Search (`search_ranked_results`, limit 10), and Ask (`run_query` cards). Each count below is the number of queries that returned the same ordered ids in all five runs.

| Code | Persona | Search | Ask | retrieve |
|---|---|---:|---:|---:|
| before (VS-11, 483b6e1) | knowledge-worker | 39/39 | 39/39 | 39/39 |
| before (VS-11, 483b6e1) | office-pm | 36/37 | 36/37 | 36/37 |
| after | knowledge-worker | 39/39 | 39/39 | 39/39 |
| after | office-pm, run 1 | 36/37 | 36/37 | 36/37 |
| after | office-pm, run 2 | 37/37 | 37/37 | 37/37 |

- **Before.** The unstable query was "the product requirements doc I drafted last week" (cause 1). An earlier probe pass, on the code with only causes 1 and 3 still open, also caught "LL-1482 spam placement 1.8%" changing in Ask (cause 2).
- **After, run 1.** "launch checklist with the go/no-go meeting checked off" differed in one of the five runs on all three paths. It did not reproduce:
  - 90 consecutive `retrieve` calls across a minute boundary gave one order;
  - 12 iterations interleaving `retrieve`, Search, and Ask gave one order;
  - a full second pass gave 37/37.
- **What remains.** Recency counts whole minutes. Two results whose scores differ by less than one minute's change in recency can still swap when a run straddles the minute. This is the only remaining dependence on the clock that I know of. Within a minute, repeated calls return identical ids and scores.

## Retrieval gate, before and after, same seed

Both personas were seeded once under America/Denver time, then the gate ran on 483b6e1 and on this change. All four runs exit 0.

| Persona | Path | Recall@5 before / after | MRR@10 before / after | p50 ms before / after | p95 ms before / after |
|---|---|---|---|---|---|
| knowledge-worker | search | 1.000 / 1.000 | 0.966 / 0.966 | 219 / 221 | 353 / 353 |
| knowledge-worker | ask | 1.000 / 1.000 | 0.966 / 0.966 | 884 / 873 | 972 / 923 |
| knowledge-worker | retrieve | 1.000 / 1.000 | 0.966 / 0.966 | 191 / 194 | 317 / 321 |
| office-pm | search | 0.900 / 0.900 | 0.661 / 0.661 | 222 / 230 | 373 / 383 |
| office-pm | ask | 0.900 / 0.900 | 0.661 / 0.661 | 1329 / 1337 | 1456 / 1408 |
| office-pm | retrieve | 0.900 / 0.900 | 0.661 / 0.661 | 198 / 194 | 347 / 339 |

- Search and Ask agree on the top result for 39/39 and 37/37 queries (before: 39/39 and 36/37).
- Every query now has the same rank on all three paths. The one rank change is "the product requirements doc I drafted last week", which was 4 in Search and 1 in Ask and `retrieve`, and is now 2 on all three. It is a time query, so the headline numbers (keyword and paraphrase) do not move.
- The larger route pool did not measurably change latency.

References in `scripts/demo/retrieval-reference/` are ratcheted to the after runs.
