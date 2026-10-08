# VS-11 Ask, agents, and every MCP search tool go through `retrieve` (cloud)

## What changed

- `run_query` (Ask, the `fndr_search` and `fndr_answer` IPC commands, and the MCP `fndr.search` and `fndr.answer` tools) now starts from `retrieve`'s fused retrieval (`retrieve_with_fused`) instead of calling the fusion stage directly. The practical difference is that Ask now reads time and app phrases as filters (VS-13) the same way Search does; before, it searched the whole vault.
- MCP `search_memories`: its `results` come from `retrieve` with the caller's `time_filter`, `app_filter`, and `limit`, replacing a separate `HybridSearcher` call that also loaded the embedding model per call.
- MCP `memory.search_full_context`: `semantic_matches` is `retrieve`'s ranked list (with the tool's time window passed as a time filter, or as a `range:` filter for explicit start and end), and `keyword_matches` is the subset the keyword route found, in the same order. Before, it merged a `HybridSearcher` call with a second keyword scan.
- MCP `ask_fndr`'s sources and `build_context_pack`'s query candidates (behind `fndr.build_context_pack`, `search_memories`' `context_pack`, and the other context tools) also come from `retrieve`, so a tool's results and the pack it returns beside them can no longer disagree.
- Tool names are unchanged: PD-11 (the canonical MCP tool list) is not decided, and its audit proposes keeping `fndr.search` and removing `search_memories` and `memory.search_raw`. `memory.search_raw` keeps its separate semantic and keyword lists because reporting raw route output is its purpose; PD-11 decides whether it stays.
- Resume Work (`resume_work`, `memory.resume_work`) lists the last N hours' memories by time and takes no query, so there is no search in it to route through `retrieve`.
- The seven dash replacements in `src-tauri/src/mcp/mod.rs` are the same lines train F's f27ab97 changes, applied identically, so the two trains merge without a conflict.

## Contract test (written first)

`mcp::tests::search_tools_return_the_search_screens_ids_in_its_order` seeds four synthetic memories and runs four queries, one with an app phrase ("the contract in Slack"). For each, the Search screen's ranked ids (`search_ranked_results`, the IPC path) must equal, in order:

- `search_memories` `results`;
- `memory.search_full_context` `semantic_matches`;
- `fndr.search` `cards`.

On the old code it failed at the first query: `search_memories` returned `["vendor"]` and the Search screen returned `["vendor", "lunch", "standup", "budget"]`. Now it passes.

Results:

- `cargo test --lib`: 904 passed, 0 failed, 10 ignored.
- `--test retrieve`: 9 passed.
- `end_to_end_fndr_query`, `search_flow`, `resume_work`, `agent_regression`, `anti_overfitting`, `chunk_retrieval_quality`, `low_signal_surface`, `memory_browse`, `retrieval_routes`, `search_relevance_eval`, `store_graph_regressions`: all pass.
- `cargo test --example retrieval_qa`: 15 passed.

## Before and after, same seed

Both personas were seeded once under America/Denver time. The gate then ran on the VS-10 commit (5341c0c) and on this change, with `make qa-retrieval-check QA_SKIP_SEED=1 [PERSONA=office-pm]`. All four runs exit 0.

| Persona | Path | Recall@5 before / after | MRR@10 before / after | p50 ms before / after |
|---|---|---|---|---|
| knowledge-worker | search | 1.000 / 1.000 | 0.966 / 0.966 | 224 / 227 |
| knowledge-worker | ask | 1.000 / 1.000 | 0.966 / 0.966 | 848 / 906 |
| knowledge-worker | retrieve | 1.000 / 1.000 | 0.966 / 0.966 | 197 / 202 |
| office-pm | search | 0.900 / 0.900 | 0.661 / 0.661 | 219 / 230 |
| office-pm | ask | 0.900 / 0.900 | 0.661 / 0.661 | 1299 / 1322 |
| office-pm | retrieve | 0.900 / 0.900 | 0.661 / 0.661 | 196 / 197 |

Top result agreement between Search and Ask: 37/39 to 39/39 (knowledge-worker) and 35/37 to 36/37 (office-PM). Ask moved to rank 1 on the four time and app queries VS-10 named ("the pandas error in Terminal" 2 to 1, "the weekday vs weekend chart from two days ago" 2 to 1, "the product requirements doc I drafted last week" 3 to 1, "the LL-1482 spam discussion from four days ago" 3 to 1).

## The one remaining disagreement is a determinism bug, filed under VS-21

On "the product requirements doc I drafted last week" (office-PM), Ask ranks the target 1st, Search 4th, and `retrieve` 5th. All three now run the same code, called one after another. In this run `retrieve` itself moved from rank 1 to 5, although VS-11 does not change `retrieve`. A probe calling `retrieve` and Search four times each on a copy of the seeded profile showed the cause:

- The temporal route gives a different memory the +0.1 temporal bonus on each call (`pm-launch-prd`, then `pm-hire-jd`, then `pm-hire-scorecard-tobias`).
- `get_search_results_in_range` gives every row a placeholder score of 1.0, and the route keeps `max(1.0, recency)`. Its recency scoring therefore never counts, and every memory in the window ties.
- The route then keeps `limit` of the tied memories in `HashMap` order.

This predates train B. It was hidden because Search did not use the temporal route until VS-10. It goes next, as VS-21, ahead of VS-12, whose threshold needs stable scores.

References in `scripts/demo/retrieval-reference/` are ratcheted to the after runs.
