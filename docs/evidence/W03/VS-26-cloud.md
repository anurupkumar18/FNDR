# VS-26 the README describes search as it really works (cloud)

## Every changed README search claim maps to code

| README claim | Code |
|---|---|
| One function, `retrieve`, ranks for the Search screen, Ask, the raw search commands, autofill, and the MCP search tools other than `memory.search_raw` | `src-tauri/src/context_runtime/retrieve.rs`. Wired up in VS-10 (`ipc/commands/search.rs` `search_ranked_results`), VS-11 (`context_runtime/mod.rs` `run_query`; `mcp/mod.rs` `search_memories`, `memory.search_full_context`, `fndr.search`), and VS-25 (`run_search_query`) |
| BM25 keyword retrieval over seven text columns | `storage/lance_store/mod.rs` `FTS_COLUMNS`, `keyword_search` |
| A temporal route when the query names a time, an entity route when it names a project or entity | `context_runtime/query_plan.rs` `route_selection` |
| Route scores added with per-intent weights | `context_runtime/fusion.rs` `fuse`; `context_pack.rs` `FusionWeights::for_intent` |
| Low-signal captures and FNDR's own windows dropped | `context_runtime/mod.rs` `drop_hidden_hits` |
| Time and app phrases become filters | `context_runtime/query_filters.rs`, `retrieve.rs` `read_filters` |
| Ties break by score, then newest, then id | `retrieval_routes.rs` `sort_route_hits`; `fusion.rs` `fuse` |
| Candidate pool fixed at 50 | `context_runtime/mod.rs` `ROUTE_CANDIDATE_POOL` |
| Keyword score bm25/(bm25+2) blended with recency counted in whole minutes | `lance_store/mod.rs` `keyword_search`; `normalize_embed_migrate.rs` `recency_score` |
| MiniLM 384-d loaded once per process; ADR 019 Proposed | `context_runtime/mod.rs` `retrieve_fused` (shared embedder); `docs/decisions/019-embedding-model.md` |
| Chunk retrieval: chunk BM25 plus BGE-large chunk vectors, rolled up, off by default | `context_runtime/chunk_route.rs`; `config.rs` `DEFAULT_SEARCH_USE_CHUNK_FIRST_RETRIEVAL = false` |
| "No strong matches" | `retrieve.rs` `strong_match`, `is_strong_match`; `src/domains/timeline/Timeline.tsx` |
| The graph is not used for ranking: the graph route runs over an empty in-memory graph | `context_runtime/mod.rs` `retrieve_fused` (`GraphIndex::build(&nodes, &edges)` over empty vectors) |
| The older hybrid searcher is still used by the companion, the legacy graph, and `memory.search_raw` | `companion/handlers/search.rs`, `graph/legacy.rs:388`, `mcp/mod.rs` (VS-25 lists them) |

Removed claims:

- "fused and reranked": Search's coverage rerank was retired in VS-10 and VS-25.
- "graph-aware recall workflows": the graph route has no data yet.

The two pre-existing dashes in the README's audience section are replaced, so the whole-file dash scan passes.

## Not done: walkthrough cards 5 and 6

`docs/product/qa-walkthrough.md` does not exist on main or on any branch. The QA reset plan (`docs/superpowers/plans/2026-09-23-user-first-qa-reset.md`) has the owner create it. When it exists, cards 5 and 6 should say:

- Search and Ask rank the same way.
- Unrelated queries say "No strong matches".
- Chunks are off until EM-03 writes them at capture.

The table above is the source for those cards.
