# Anti-bloat review: `search/` and `context_runtime/` (cloud)

Part 9 stretch item. Scope: `src-tauri/src/search/` (5 files) and `src-tauri/src/context_runtime/` (17 files), 13,362 lines before this change.

## Method

- A script listed every function, struct, enum, const, and type alias defined outside `#[cfg(test)]` in those folders. It counted references by name in production code anywhere under `src-tauri/src`, and separately in tests and examples.
- Anything referenced only by its own definition was a candidate. Each candidate was read by hand.
- No type was unused. Eight functions were.

## Deleted (no production caller)

| Item | Where | Why it was dead |
|---|---|---|
| `MemoryCardSynthesizer::build_memory_cards` | `search/memory_cards.rs` | four-line wrapper around `from_results`; no caller |
| `QueryContext::debug_plan` and `QueryExpansionDebug` | `search/query_processor.rs`, `search/mod.rs` | debug struct for the retired search path; still listed a "graph" retrieval step |
| `get_context_pack_detail` | `context_runtime/mod.rs` | no command or tool calls it |
| `refine_plan_with_llm` | `context_runtime/query_plan.rs` | the LLM plan refinement was never wired into `plan` or `retrieve` |
| `wiki_policy.rs` (module) | `context_runtime/` | one `#[allow(dead_code)]` function that set a page's stability, and its test |
| `apply_recency_decay` | `context_runtime/temporal_route.rs` | a wrapper with a fixed 24-hour half-life that only tests called. The route uses `recency_decay` with a query-dependent half-life. The two recency tests now call `recency_decay` directly, so they test the function the route runs. |

105 lines removed and 14 added (the recency tests rewritten against `recency_decay`, with a named half-life constant).

## Kept, with the reason

| Item | Why it stays |
|---|---|
| `RouteCtx::with_graph`, `RouteCtx::allowing_mock_vectors` | test-only builders; the entity and graph route tests and the mock-vector fixtures need them |
| `GraphRoute`, `Route::Graph`, graph fields in fusion, the verifier, and the context pack | VS-33: serialized contract fields in MCP and Ask JSON; the persisted graph may feed them (proposal VS-69) |
| `apply_refinement_json` | parses a refinement; `tests/query_plan_rules.rs` covers it. With `refine_plan_with_llm` gone it has no production caller. Its producer, `InferenceEngine::refine_query_plan`, is in `inference/`, outside this lane. Both can go together when the inference owner agrees. |
| `Store::get_context_pack_by_id` | now has no production caller; it is in the shared storage layer, so it is listed for its owner rather than deleted here |
| `search/hybrid.rs` (2,486 lines) | `HybridSearcher` still serves five callers outside this lane (VS-25 table: companion search, the legacy graph builder, MCP `memory.search_raw`, `search_relevance_eval`, `fndr_diagnostic`). Moving those to `retrieve` lets most of the file go. This is the largest remaining reduction. |

## Checks

- `cargo test --lib`: 909 passed, 0 failed, 9 ignored (910 before; the deleted module's one test is gone).
- All integration targets and example tests pass.
- Retrieval gate: see below.

The deleted code had no production caller, so the gate ran after the change only, on fresh seeds (America/Denver, 2026-10-05), against the committed references. All three personas pass with "No per-query rank changed":

| Persona | Recall@5 (all paths) | MRR@10 (all paths) | retrieve p95 ms, reference / now |
|---|---:|---:|---|
| knowledge-worker | 1.000 | 0.966 | 320 / 318 |
| office-pm | 0.900 | 0.661 | 334 / 318 |
| software-engineer | 1.000 | 0.831 | 315 / 301 |
