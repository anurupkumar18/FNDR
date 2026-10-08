# VS-07 follow-up: the BM25 indexes are built on write, not inside the first search (cloud)

## The failure

On PR #31 (head 81666e1), "Rust tests (macOS)" failed in a test that local owns, `memory_journey::tests::six_synthetic_journeys_reconstruct_from_temporary_storage_and_search`. The same search run twice gave two different scores:

```
left:  [("research", 0.1901673)]
right: [("research", 0.22735563)]
```

Main passed the same test on c124957, and train F changes nothing in storage. The bug came from VS-07 (42a4c98, train B), which is already on main.

## Root cause, measured

1. **The CI runner has no MiniLM model.** That leaves the keyword route as the only route that finds "research". Without a model and without the keyword route, the test finds nothing.
2. **The keyword route runs three variants, each with a 320 ms budget.** The variants are the phrase "distinctive zephyr fact", then "distinctive zephyr", then "zephyr fact". Each later variant's score is decayed.
3. **Only a missing phrase variant reproduces CI's score.** Skipping that variant on the first call gives 0.1901673, CI's first-call score to the last digit. Skipping any other variant alone leaves the score unchanged.
4. **Why the phrase variant missed.** `Store::keyword_search` built the seven BM25 indexes lazily, on the first query, inside that variant's 320 ms budget.
   - The build took 94 ms on an idle Linux machine, and 108 to 228 ms with 8 copies of the test running at once.
   - Under that load, the first phrase query took 305 to 326 ms in total. Seven of eight runs hit the 320 ms cutoff.
5. **The local reproduction.** Without a model, with 8 copies running at once:

| Code | Runs failed | Left value in every failure |
|---|---:|---|
| main's `keyword_search` | 20 of 24 | `("research", 0.1901673)` |
| index built on write | 0 of 24 | |

Users hit the same thing: the first search after a vault gains rows could lose its phrase match.

## Change

`src-tauri/src/storage/lance_store/mod.rs`:

- `insert_memory_batch`, which every append path goes through, and the full overwrite (`replace_all_memories_preserving_ids`) now build any missing BM25 index right after the write.
- Once the indexes exist, the check costs one manifest read, about 2 ms. If the build fails, it logs `lancedb:fts_index_build_failed` and the write still succeeds. `keyword_search` keeps its own check as a fallback.

After the change, the first phrase query took 122 to 164 ms under the same load: still a cold read, but under half the budget.

## Tests

- **New test:** `writes_build_the_keyword_indexes_so_no_search_pays_for_them`.
  - It failed first on main's code: no indexes after `add_batch`.
  - It checks both write paths. With the overwrite rebuild removed, it fails "after an overwrite", so both halves of the change are needed.
- **Lib:** 923 passed, 0 failed.
- **Retrieval gate**, `make qa-retrieval-check` on three personas, `TZ=America/Denver`: PASS, and no per-query rank changed on any path.

| Persona | Recall@5 (all paths) | MRR@10 (all paths) | retrieve p95 ms, reference then now (not gated) |
|---|---:|---:|---:|
| knowledge-worker | 1.000 | 0.966 | 320, 287 |
| office-pm | 0.900 | 0.661 | 334, 301 |
| software-engineer | 1.000 | 0.831 | 315, 270 |

## What can still go wrong

- **The first search after a restart still reads the indexes cold.** In the load runs that took 122 to 164 ms. It no longer pays for building them.
- **A failed build is only logged at write time**, and `keyword_search` builds the indexes then. That is the old behavior, so the race can come back only while a build keeps failing.
- **Every write now does one extra manifest read**, about 2 ms, to check the indexes.
