# VS-20: retrieve at 10,000 memories on the M1, 2026-10-09

The harness and the cloud numbers are in `docs/evidence/W03/VS-20-cloud.md`. This is the run that ticket left open: the owner's M1 with 8 GB.

```
cargo test --test storage_scale retrieve_latency -- --ignored --nocapture
```

Run from a scratch copy of the published tree, debug build, with the BGE model not installed (so the chunk route's keyword search and store calls run, and its query embedding does not). Other agent sessions were building on the machine at the time: the load average was 7.5.

| | p50 | p95 |
|---|---|---|
| `retrieve`, production route budgets | 448 ms | 481 ms |
| Search, production route budgets | 451 ms | 488 ms |
| `retrieve`, budgets lifted | 457 ms | 494 ms |
| Search, budgets lifted | 452 ms | 540 ms |

Route medians with production budgets: vector 361 ms, chunk 307 ms, keyword 264 ms. The flat scan over 60,000 chunk vectors took 198 ms at p50 and BM25 over chunk text 53 ms. No query of 24 came back empty. Seeding took 27 s.

## Reading

- **The done-when is met for what ships:** p95 is under 500 ms with production budgets, in a debug build, on a loaded machine. A release build is faster.
- **No vector index is added.** The ticket says to add one only if flat search exceeds the budget. It does not.
- **Not measured: the BGE query embedding.** With the model installed the chunk route also embeds the query with a 1024-dimension model, which the cloud run suggests is the largest single cost. The chunk route is off in the product and ADR 019 has not chosen the chunk model, so this belongs to that decision.
- **A first run the same hour is not reported as a result.** It ran while another build was compiling on the same machine and measured p95 1.2 to 2.3 s. Latency here is sensitive to what else the machine is doing; the route budgets exist for that reason, and in neither run did a query come back empty.
- The margin is thin (481 against 500). If the vault grows well past 10,000 memories, the vector route's 361 ms median is the first thing to index.
