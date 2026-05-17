# Agentic Graph RAG handoff

## Session: Phase 0 graph module restructure

Branch/worktree:
- Branch: `codex/agentic-graph-rag-phase0`
- Worktree: `~/.config/superpowers/worktrees/fndr/codex-agentic-graph-rag-phase0`
- Plan: `docs/superpowers/plans/2026-05-17-agentic-graph-rag.md`

What changed:
- Moved the insight graph module from `src-tauri/src/memory/graph/` to top-level `src-tauri/src/graph/`.
- Moved Lance insight graph persistence from `src-tauri/src/storage/graph_store.rs` to `src-tauri/src/graph/graph_store.rs`.
- Split graph schema ownership:
  - `src-tauri/src/graph/entities.rs`: `GraphNodeType` and node literal helpers.
  - `src-tauri/src/graph/edges.rs`: `GraphEdgeType`, edge literal helpers, and `edge_aliases::canonical`.
  - `src-tauri/src/graph/schema.rs`: `GraphNode`, `GraphEdge`, `GraphSubgraph`, and field-name constants.
- Renamed graph clustering module from `clusters.rs` to `community.rs`.
- Extracted `find_path` into `src-tauri/src/graph/pathfinding.rs`.
- Added `src-tauri/src/graph/graph_index.rs` and `src-tauri/src/graph/graph_rerank.rs` skeletons for later retrieval phases.
- Added node types: `Window`, `App`, `Command`.
- Added edge types: `OccurredInSession`, `BelongsToProject`, `UsedApp`, `SameTaskAs`, `EvidencedBy`.
- Extended `capture/entity_extractor.rs` so each memory emits:
  - `Memory --BelongsToProject--> Project` when project is populated.
  - `Memory --OccurredInSession--> Session` when session id is populated.
- Updated graph import sites from `crate::memory::graph` / `crate::storage::graph_store` to `crate::graph`.
- Updated `docs/architecture/graph-schema.md` for the new module map.

Verification run so far:
- `npm install`
- `npm run build` (needed because fresh worktree lacked `dist/` for Tauri macro)
- Baseline before edits: `cargo test -p fndr memory::graph::` passed 11 graph tests after building `dist/`.
- After edits: `cargo test -p fndr graph::` passed 20 tests, 1 ignored skeleton test.
- After edits: `cargo test -p fndr capture::entity_extractor` passed 8 tests.
- `rg "memory::graph|storage::graph_store" src-tauri/src src-tauri/tests` returned no hits.
- `npm test -- src/domains/memory-vault/MemoryCardsPanel.test.tsx` passed after updating a stale vault-only UI test that expected a removed "All memories" tab.
- `make test` passed: TypeScript typecheck, 74 Vitest tests, full Rust `cargo test`, and doc tests.

Where to look next:
- Phase 1 starts in `docs/superpowers/plans/2026-05-17-agentic-graph-rag.md` under "Phase 1 - Query planner".
- Primary files for Phase 1:
  - `src-tauri/src/context_runtime/query_plan.rs`
  - `src-tauri/src/context_runtime/graph_plan.rs`
  - `src-tauri/src/context_runtime/mod.rs`
  - `src-tauri/src/inference/mod.rs`
  - `src-tauri/src/search/query_processor.rs`
- Reuse graph primitives from:
  - `src-tauri/src/graph/entities.rs`
  - `src-tauri/src/graph/edges.rs`
  - `src-tauri/src/graph/graph_index.rs`
  - `src-tauri/src/graph/graph_rerank.rs`

Remaining plan phases:
- Phase 1: Query planner.
- Phase 2: Retrieval routes and fusion.
- Phase 3: Evidence pack, verifier, composer, and explainability.
- Phase 4: Backward-compatible IPC and MCP shims.
- Phase 5: UI "why surfaced", query-scoped graph, evidence/timeline expansion, Copy for Agent.

Notes for future agents:
- Keep using an isolated worktree unless the user explicitly asks to merge/push.
- Run Rust commands from `src-tauri/`, not the repo root.
- If a fresh worktree fails Rust tests with `frontendDist = "../dist" but this path doesn't exist`, run `npm run build` from the repo root first.
- `graph_rerank.rs` intentionally contains a compile-safe skeleton with an ignored test; the real implementation belongs to the later retrieval/fusion phases.
