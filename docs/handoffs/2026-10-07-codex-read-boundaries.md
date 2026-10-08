# Handoff: Codex read-boundary lane, 2026-10-07

## Goal

Close independent MCP and Agent audit reads that could show excluded or stale memory content while two Claude sessions work on briefing/output quality and agent surfaces in the shared `main` checkout.

## Current state

Four focused commits are on `main` and were pushed to GitLab and GitHub. Both remote heads were verified at `0e81ebf` after the last push. The remaining dirty files in the checkout belong to the Claude sessions; do not stage them as part of this lane.

## Decisions made

| Decision | Reason | Files/Docs |
| --- | --- | --- |
| Use one task admission rule for context packs and MCP To-dos. | Task text, links, and project scope need the same current source checks. | `context_runtime/mod.rs`, `mcp/mod.rs` |
| Filter typed insight graph nodes and edges before BFS. | A hidden node must not reveal itself or bridge to a visible result. | `mcp/mod.rs` |
| Recheck and sanitize saved audit records at the read boundary. | Old audit titles, URLs, output, and ranking prose may survive a later exclusion or edit. | `agent/audit.rs`, `ipc/commands/agent.rs`, `mcp/mod.rs` |
| Defer saved context packs and the legacy graph query. | Their historical rows lack complete source provenance; filtering a few evidence IDs would give a false privacy claim. | `docs/product/mcp-tool-audit.md` |

## Files changed

| File | Change | Notes |
| --- | --- | --- |
| `src-tauri/src/context_runtime/mod.rs` | Exposed the existing task eligibility check for MCP reuse. | `da878f2` |
| `src-tauri/src/mcp/mod.rs` | Authorized To-dos, typed graph neighborhoods, and saved retrieval explanations. | `da878f2`, `e13ccaf`, `d73f95b`, `0e81ebf`; Claude has unrelated formatting hunks still unstaged. |
| `src-tauri/src/agent/audit.rs` | Shared current-source projection for saved audit reads. | `0e81ebf` |
| `src-tauri/src/ipc/commands/agent.rs` | Applied the projection to Agent panel audit list, detail, and explanation commands. | `0e81ebf` |
| `docs/product/mcp-tool-audit.md` | Recorded delivered boundaries and remaining provenance gaps. | `e13ccaf`, `d73f95b`, `0e81ebf` |

## Files inspected but not changed

`src-tauri/src/storage/schema.rs`, `src-tauri/src/storage/lance_store/mod.rs`, `src-tauri/src/graph/legacy.rs`, `src-tauri/src/graph/graph_store.rs`, `src-tauri/src/graph/traversal.rs`, `src-tauri/src/agent/skills.rs`, `src-tauri/src/agent/evals.rs`, and `src/domains/workspace/AgentPanel.tsx`.

## Commands run

| Command | Result |
| --- | --- |
| `CARGO_BUILD_JOBS=1 cargo test --lib mcp::tests:: -- --nocapture` | 37 passed after the audit refactor. |
| `CARGO_BUILD_JOBS=1 cargo test --lib agent::audit::tests:: -- --nocapture` | 3 passed. |
| `CARGO_BUILD_JOBS=1 cargo test --lib context_visibility_ -- --nocapture` | 6 passed after the task change, before later Claude commits. |
| `rustfmt --edition 2021 --check` on changed Rust files | Passed. |
| `git ls-remote` on `origin` and `gh` | Both were `0e81ebf` after the last push. |

## Tests / verification

The To-dos test first failed with seven rows instead of two, then passed. The graph test first returned three nodes instead of two, then passed with hidden bridge and private edge cases. The saved explanation test first returned three memories instead of one, then exposed stale prose and passed after current-source projection. No owner vault was read or changed. Full `make test` and native UI checks were not run while the shared checkout and Rust build were active in the Claude sessions.

## Known issues

- `get_context_runtime_status` and `list_recent_context_packs` still read historical packs. Packs can contain project summaries derived from extra events, tasks, and graph data without complete provenance. Do not certify them by checking `pack.evidence` alone.
- `memory.graph_query` returns legacy string-ID graph nodes. Sessions, URLs, tasks, and audio segments often have no source IDs; its raw metadata may contain captured text.
- `propose_skill_from_run` and `propose_eval_from_run` still consume raw saved audit records to build drafts. The Agent panel's audit list, detail, and explanation are guarded; draft creation needs a separate source and output provenance decision.
- The saved audit projection retains the person's original goal and generic tool policy fields. It clears old output/error details and historical ranking prose.

## Next steps

1. Design a complete source ledger for newly saved packs, including project expansion, tasks, and graph content; fail closed for old packs or rebuild read projections. Test current exclusions and missing sources before touching storage schema.
2. Migrate or narrow `memory.graph_query` with explicit provenance for non-memory nodes. Preserve useful visible memory results and reject untraceable metadata.
3. Gate skill/eval draft creation from historical audit rows without turning the eval's expected outcome into generic placeholder text.
4. Run a combined gate after the Claude batches settle, then test the native audit and MCP approval flows with a disposable profile.

## Risks / do not do

Keep work on shared `main`; do not stash, reset, switch branches, or stage another session's hunks. `git push origin main` pushes to both remotes. Run Cargo serially on this 8 GB Mac. Do not load a local model during another Rust build or touch the real owner vault for this lane.

## Useful context for next agent

`context_runtime::context_source_memories` follows bounded aliases and applies the current visibility policy. `context_runtime::retain_context_events` checks complete event sources. `agent::audit::authorize_audit_record` is the shared read projection for the MCP explanation and Agent panel audit commands. The existing MCP audit in `docs/product/mcp-tool-audit.md` tracks the remaining tools.
