# VS-09 one retrieval function every surface calls (cloud)

## What exists now

`context_runtime::retrieve(state, &RetrieveRequest { query, time, app, limit }) -> RetrieveResult { hits: Vec<RetrieveHit { memory_id, chunk_id, score, why }> }` in `src-tauri/src/context_runtime/retrieve.rs`.

- It runs the same plan, route dispatch, fusion, and low-signal drop as Ask: both now start from one shared function, `retrieve_fused` in `context_runtime/mod.rs`, and `run_query` was changed to call it with no change in behavior. Ask then adds evidence, verification, and cards; `retrieve` stops at ranked ids.
- `why.routes` names the routes that found the memory (vector, keyword, chunk, temporal, entity, graph). `why.matched_terms` lists the query words (or contiguous phrases) present in the memory's text for hits the keyword route found, in query order; meaning-only matches have none.
- `score` is the memory's evidence strength (the weighted sum of its route scores), the scale Ask's verifier and cards already use; VS-12 sets its "no strong match" threshold on it. `chunk_id` stays empty until VS-18.
- `time` takes the store's existing filters (1h, 24h, 7d, today, yesterday) and `app` an exact app name; VS-13 turns phrases in the query into these.
- Types derive `serde` and `specta` so VS-10 and VS-11 can expose them over IPC and MCP.

Performance fix found on the way: `drop_low_signal_hits` looked up every fused hit separately (about 28 ms each, 39 hits per query on office-PM, roughly 1.1 s). A new `Store::get_memories_by_ids` fetches them in one `id IN (...)` scan. On the office-PM profile `retrieve` went from 1.4 to 1.8 s per query to 0.43 to 0.64 s, and Ask from about 3.0 s to 2.0 s (probe, Linux, unoptimized build).

## Tests

- `tests/retrieve.rs` (written first; failed to compile before the API existed): the top hit for "zephyr contract" is the vendor memory, found by the keyword route, with `matched_terms` `["zephyr", "contract"]`, and no meaning-only hit carries words; `retrieve` and Ask's cards return the same memories in the same order for three queries; the app filter and the limit hold; the request and hit JSON carry exactly the documented fields. 4 passed.
- `retrieve.rs` unit tests for `matched_terms` (whole words only, query order, contiguous phrases): 2 passed.
- `get_memories_by_ids_fetches_many_rows_in_one_call_and_skips_unknown_ids` (including an id with a quote): passed.
- `cargo test --lib` 894 passed, 0 failed; `end_to_end_fndr_query`, `search_flow`, `resume_work`: passed. `cargo test --example retrieval_qa`: 15 passed.

## Report with three paths (both personas reseeded)

`retrieval_qa` now measures `retrieve` as a third path (`retrieve_rank_at_10`, `retrieve_top_score`; the gate treats it as a new path, and the references are ratcheted to include it).

| Persona | Path | Recall@5 | MRR@10 | Paraphrase | Time | App | p50 ms | p95 ms |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| knowledge-worker | search | 0.955 | 0.909 | 0.875 | 1.000 | 1.000 | 243 | 278 |
| knowledge-worker | ask | 1.000 | 0.966 | 1.000 | 1.000 | 1.000 | 1407 | 1533 |
| knowledge-worker | retrieve | 1.000 | 0.966 | 1.000 | 1.000 | 1.000 | 482 | 575 |
| office-pm | search | 0.800 | 0.664 | 0.556 | 0.875 | 1.000 | 302 | 378 |
| office-pm | ask | 0.900 | 0.670 | 0.778 | 0.875 | 1.000 | 1975 | 2122 |
| office-pm | retrieve | 0.900 | 0.670 | 0.778 | 1.000 | 1.000 | 447 | 515 |

Done when: "`retrieve` exists, is tested, and its report row is at least as good as the better of the two current paths." Recall@5: retrieve equals the better path on both personas (1.000 and 0.900). MRR@10: equal to Ask on both personas, and above Search on both. Office-PM time Recall@5 is 1.000 on `retrieve` against 0.875 on Search and Ask (Ask's card citations shift one time query). Latency: `retrieve` p95 575 and 515 ms here, about twice Search's 278 and 378 ms; VS-10 points Search at it, so VS-20's 10,000-memory measurement matters.

## Measurement caveat found here

The seeded corpora mix entries placed at clock times on given days (`day_offset` plus `time`) with entries placed minutes before seeding (`minutes_ago`). Their relative ages therefore depend on the time of day the profile is seeded; this run was seeded just after midnight UTC, the previous references in the late evening, and a handful of time-sensitive Ask ranks moved by one or two places (for example "the product requirements doc I drafted last week" 5 to 7). The gate's tolerance absorbs this, but compare evidence runs seeded at similar local times, and expect VS-13's time filters to need a pinned clock in the harness.
