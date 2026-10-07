# VS-33 no graph route in retrieval until `retrieve` loads the graph (cloud)

## What the code did

`retrieve_fused` (`src-tauri/src/context_runtime/mod.rs`) built an empty node list, an empty edge list, and a `GraphIndex` over them on every query. It passed them to the routes with `with_graph`. The planner added the graph route for seven of the intents (resume, debug, definition, related-to, lookup, how-to, timeline). The route always ran over nothing.

The failing test, run on the code before the change, showed it in Ask's debug trace for "zephyr contract":

```
[("Chunk", 0), ("Vector", 4), ("Keyword", 1), ("Graph", 0)]
```

## The ticket's premise, corrected

The ticket says the graph is in memory and empty after a restart. The insight graph is in fact persisted: `graph_nodes` and `graph_edges` are LanceDB tables (`src-tauri/src/graph/graph_store.rs`), written by the capture flush and the idle `commit_graph_updates`, and read by the graph commands and MCP `memory.graph_context`. What was missing is that `retrieve` never loads it. The route was empty by construction, not because data is lost.

Two consequences, both recorded as a proposal rather than done here:

- **The entity route is starved the same way.** It matches query entities against `ctx.graph_nodes`, and `retrieve` never supplies any (before this change it passed the same empty list). So on every real query the entity route returns nothing. This change does not alter that.
- **Turning the graph on is a ranking change, not a fix.** Loading the persisted graph per query costs a full read of two tables, and the graph route's fusion weight is 0.20. Neither seeded persona writes graph rows, so the retrieval report cannot measure the effect. It needs a seeded graph first.

## What changed

- `query_plan::route_selection` no longer adds `Route::Graph`, and it lost the `intent` argument that existed only for that rule.
- `retrieve_fused` no longer builds the empty graph or calls `with_graph`.
- `src-tauri/src/graph/graph_rerank.rs` is deleted. It was a no-op stub (`rerank_with_graph_signals` had an empty body) with one ignored test and no callers.
- `README.md` and `docs/architecture/graph-schema.md` now say that `retrieve` does not load the insight graph, so it plans no graph route, and that the entity route finds nothing. Two dashes in the touched doc were replaced.

## Not deleted, and why

`GraphRoute`, the `Route::Graph` variant, and the graph fields that flow through fusion, the verifier, the evidence pack and the composer stay. `Route::Graph` and the graph path appear in 20 files, including serialized types (`FusionWeights.graph`, `FusionSignals.graph`, `SurfacingReason.graph_path`) that MCP and Ask return. Removing them is a contract change several times the ticket's two hours, and it would delete a working route that becomes useful once `retrieve` loads the persisted graph. `GraphRoute` stays covered by `tests/retrieval_routes.rs`.

```
$ git grep -n "push(Route::Graph)" -- src-tauri/src
(no output: no planner adds the graph route)

$ git grep -c "Route::Graph" -- src-tauri/src src-tauri/tests
src-tauri/src/context_runtime/fusion.rs:4
src-tauri/src/context_runtime/graph_route.rs:6
src-tauri/src/context_runtime/retrieval_routes.rs:5
src-tauri/src/context_runtime/retrieve.rs:1
src-tauri/src/context_runtime/verifier.rs:2
src-tauri/tests/retrieval_routes.rs:3
```

The remaining callers are the route's own dispatch (`run_route`, reached only when a plan asks for it, which no planner now does), fusion and verifier arms for its hits, and tests.

## Tests

- `tests/retrieve.rs::retrieval_runs_no_graph_route_until_it_loads_the_graph`: Ask's trace for a lookup query has a Keyword route and no Graph route. It failed before the change with the trace above.
- `tests/query_plan_rules.rs`: the definition-query test now expects `[Chunk, Vector, Keyword]`, and is renamed to `definition_query_plans_evidence_hops_without_a_graph_route`, since it still checks the graph expansion settings.
- Results:
  - `cargo test --lib`: 910 passed, 0 failed, 9 ignored (10 before; the deleted stub's ignored test is gone).
  - All integration targets pass: `retrieve` 14, `query_plan_rules` 11, `retrieval_routes`, `end_to_end_fndr_query`, `search_flow`, `resume_work`, `agent_regression`, `anti_overfitting`, `chunk_retrieval_quality`, `low_signal_surface`, `memory_browse`, `search_relevance_eval`, `store_graph_regressions`, `storage_scale`, `capture_fixtures`, `embedding_audit`, `merge_replay`.
  - Examples: `retrieval_qa` 16, `seed_demo` 6.

## Retrieval gate, before and after, same seed

Both personas were seeded once under America/Denver time on 2026-10-05. The gate then ran on main (342e5a2) and on this change rebased onto it. All four runs pass.

| Persona | Path | Recall@5 before / after | MRR@10 before / after | p50 ms before / after | p95 ms before / after |
|---|---|---|---|---|---|
| knowledge-worker | search | 1.000 / 1.000 | 0.966 / 0.966 | 237 / 226 | 443 / 373 |
| knowledge-worker | ask | 1.000 / 1.000 | 0.966 / 0.966 | 917 / 905 | 1019 / 1011 |
| knowledge-worker | retrieve | 1.000 / 1.000 | 0.966 / 0.966 | 213 / 203 | 379 / 334 |
| office-pm | search | 0.900 / 0.900 | 0.661 / 0.661 | 256 / 223 | 448 / 383 |
| office-pm | ask | 0.900 / 0.900 | 0.661 / 0.661 | 1348 / 1294 | 1832 / 1467 |
| office-pm | retrieve | 0.900 / 0.900 | 0.661 / 0.661 | 211 / 194 | 411 / 353 |

- **No query moved.** Every per-query rank is the same on all three paths for both personas. So are the no-match counts (2 and 1) and Top-1 agreement (39/39 and 37/37). Top scores differ by at most 0.0003: recency moves during the minutes between runs.
- **Latency moved within run-to-run noise.** The after runs were a little faster here, and a little slower in the first pair below. The removed route took about 1 ms (VS-20's route medians), so neither direction is a measured effect.
- **An earlier pair, on train D's head (d3b6735) before the rebase, also showed identical ranks.** In that pair the knowledge-worker gate failed both before and after, on one query: "churn drivers due Thursday" went from rank 1 to a miss on every path. Its memory is seeded three days before the run day, and VS-13 read "Thursday" as the day something was captured. Three days back is a Thursday only on a Sunday, which is when the reference was recorded (2026-10-04). Local fixed the rule at its merge gate (b1a1776: a weekday after "due", "by", "until", "before", "till", or "next" is a deadline), which is why the pair above passes on a Monday.
