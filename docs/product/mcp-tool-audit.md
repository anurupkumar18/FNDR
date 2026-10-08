# MCP tool surface audit (RET-02)

50 statically declared tools plus the dynamically registered `fndr.remember`
(51 total) after migration steps 1 and 2 (verified 2026-10-07 with the
extraction script below). The 2026-09-23 surface had 52 statically declared
tools plus `fndr.remember`. The classification table retains the two removed
names as migration history, so its rows are not a count of the active surface.

```bash
python3 - <<'PY'
import re
s = open("src-tauri/src/mcp/mod.rs").read()
start = s.index("fn tools_list_result()")
end = s.index("\nfn ", start + 10)
names = re.findall(r'"name":\s*"([^"]+)"', s[start:end])
print(len(names))
PY
```

## Classification

Columns: **kind** (read / write / execute), **raw text** (does the response
release raw captured screen/OCR text, as opposed to composed summaries or
counts), **side effects**, **overlaps**, **v2 equivalent** (from the 12 of 14
founding tools documented in `~/FNDR-2.0/crates/fndr-mcp/src/server.rs`'s
module doc comment), **verdict**.

| Tool | Kind | Raw text | Side effects | Overlaps with | v2 equivalent | Verdict |
|---|---|---|---|---|---|---|
| `memory.search_full_context` | read | yes | none | `fndr.search` | `fndr.search` | merge |
| `memory.get_context_pack` | read | no (composed) | none | `agent.build_context_pack`, `fndr.build_context_pack` | `fndr.context_pack` | merge |
| `memory.agent_brief` | read | no | none | `memory.warm_start`, `memory.agent_onboarding` | none | merge |
| `agent.build_context_pack` | read | no | none | `memory.get_context_pack`, `fndr.build_context_pack` | `fndr.context_pack` | merge |
| `agent.run` | execute | no | runs an agent action | none | none (v1-only, post-dates v2) | approval card required; closed if unavailable, declined, or expired |
| `agent.privacy_status` | read | no | none | `fndr.privacy_status` | `fndr.privacy_status` | merge |
| `agent.explain_retrieval` | read | no | none | none | `fndr.explain_retrieval` | keep |
| `agent.rate_result` | write | no | appends to `RetrievalFeedbackRating` | none | `fndr.feedback` | keep; valid MCP token, actions on, assistant notes on |
| `agent.list_prompts` | read | no | none | none | none | keep |
| `agent.get_prompt` | read | no | none | none | none | keep |
| `memory.timeline` | read | no | none | `fndr.timeline` | `fndr.timeline` | merge |
| `memory.active_focus` | read | no | none | none | `fndr.active_focus` | keep |
| `memory.warm_start` | read | no | none | `memory.agent_brief` | none | merge |
| `memory.agent_onboarding` | read | no | none | `memory.agent_brief` | none | merge |
| `memory.project_wiki` | read | no | none | none | none | keep |
| `memory.claims` | read | partial | none | none | none | keep |
| `memory.breakthroughs` | read | partial | none | none | none | keep |
| `memory.source_evidence` | read | **yes** | none | none | `fndr.source_evidence` (gated by `include_raw`, default closed) | keep; raw text defaults off |
| `memory.search_raw` | read | **yes** | none | `memory.search_full_context` | none | removed in migration step 2; retained here as history |
| `memory.projects` | read | no | none | none | none | keep; authorize every activity source before grouping |
| `memory.project_context` | read | no | none | `memory.project_wiki` | none | merge; authorize activity before derived errors and memory rows |
| `memory.decisions` | read | no | none | none | `fndr.recall` (decisions only) | keep |
| `memory.errors` | read | partial | none | none | none | keep; authorize every activity source before returning errors |
| `memory.blockers` | read | partial | none | none | none | keep |
| `memory.todos` | read | no | none | none | none | keep |
| `memory.graph_query` | read | no | none | `memory.graph_context` | none | merge |
| `memory.graph_context` | read | no | none | `memory.graph_query`, `fndr.get_memory_subgraph` | none | merge |
| `memory.recent_changes` | read | partial | none | `fndr_get_recent_working_state` | none | merge |
| `search_memories` | read | yes | none | `memory.search_full_context`, `fndr.search` | `fndr.search` | removed in migration step 2; retained here as history |
| `ask_fndr` | read | no (composed) | none | `fndr.answer` | none | merge |
| `get_fndr_stats` | read | no | none | `fndr.quality_status` | none | merge |
| `start_meeting` | execute | no | starts audio capture | none | none | approval card required; closed if unavailable, declined, or expired |
| `stop_meeting` | execute | no | stops audio capture | none | none | approval card required; closed if unavailable, declined, or expired |
| `get_meeting_transcript` | read | yes | none | none | none | keep |
| `search_meeting_transcripts` | read | yes | none | none | none | keep |
| `get_ambient_context` | read | yes | none | `fndr_context` | none | merge |
| `fndr_context` | read | yes | none | `get_ambient_context` | none | merge |
| `fndr_search_code_context` | read | partial | none | none | none | keep |
| `fndr_diff` | read | no | none | none | none | keep |
| `fndr_get_recent_working_state` | read | partial | none | `memory.recent_changes` | none | merge |
| `fndr_remember_decision` | write | no | appends to the decision ledger | none | `fndr.remember_decision` | keep |
| `fndr_health_check` | read | no | none | none | none | keep |
| `fndr.search` | read | yes | none | `memory.search_full_context` | `fndr.search` | **keep as the canonical search tool** |
| `fndr.answer` | read | no (composed) | none | `ask_fndr` | none | **keep as the canonical answer tool** |
| `fndr.build_context_pack` | read | no | none | `memory.get_context_pack`, `agent.build_context_pack` | `fndr.context_pack` | **keep as the canonical context-pack tool** |
| `fndr.get_related_memories` | read | no | none | none | none | keep |
| `fndr.get_memory_subgraph` | read | no | none | `memory.graph_query`, `memory.graph_context` | none | merge |
| `fndr.timeline` | read | no | none | `memory.timeline` | `fndr.timeline` | **keep as the canonical timeline tool** |
| `fndr.quality_status` | read | no | none | `get_fndr_stats` | none | merge |
| `fndr.privacy_status` | read | no | none | `agent.privacy_status` | `fndr.privacy_status` | **keep as the canonical privacy-status tool** |
| `fndr.open_target` | execute | no | opens a URL/app/file | none | `fndr.open_target` (sanitized, else explicit unavailable) | approval card required; closed if unavailable, declined, or expired |

## Summary

- **2 removed** (`memory.search_raw`, `search_memories`): pure duplicates of an already-kept tool with no distinct behavior. The `get_ambient_context`/`fndr_context` merge remains a later migration step.
- **~18 merge**: same information under a second name, usually from the `memory.*` namespace duplicating a newer `fndr.*` one. The `fndr.*` namespace should become canonical; `memory.*`/bare-name equivalents are the legacy surface to fold in.
- **4 execute-class tools require approval**: `agent.run`, `start_meeting`, `stop_meeting`, and `fndr.open_target` consult `policy_for_action` and wait for an explicit in-app approval card before dispatch. Missing UI, timeout, decline, or the actions kill switch refuses the action.
- **Raw captured text is opt-in**: `memory.source_evidence` has `include_raw`, which defaults to false. `memory.search_raw` was removed.
- **Retrieval feedback is a write**: `agent.rate_result` requires a valid MCP token, actions enabled, and assistant notes enabled before it can append feedback.
- **Hermes has a server-enforced read grant**: FNDR gives its embedded Hermes runtime a separate process-lifetime token for four memory read tools. The MCP server filters discovery, rejects every other tool or method, and refuses `include_raw=true` on the two reads that support it. It rechecks the saved memory-sharing choice on each request. The full token remains available to local clients through existing owner-controlled discovery; the grant does not isolate a malicious same-user process.
- **Task and graph reads check current sources**: `memory.todos` uses the same source and historical-event admission as context packs. `memory.graph_context` filters its UUID neighborhood before traversal, including edge provenance. Legacy `memory.graph_query` now searches only memory-backed nodes rebuilt from current authorized records; unattributed entities and edges are omitted. Saved context packs still need separate authorization work.
- **Saved audit reads refresh their source fields**: MCP `agent.explain_retrieval` and the Agent panel's audit list, detail, and explanation omit excluded or missing memories. Historical titles, URLs, ranking prose, output text, and exclusion reasons are replaced with current-source data or neutral text. Skill/eval draft creation from a saved run still needs its own provenance gate.

## Proposed target surface

Following v2's precedent (14 tools: 13 read/context + 1 write), a `fndr.*`-only
surface:

`fndr.search`, `fndr.answer`, `fndr.build_context_pack`, `fndr.timeline`,
`fndr.privacy_status`, `fndr.quality_status`, `fndr.active_focus`,
`fndr.explain_retrieval`, `fndr.get_related_memories`,
`fndr.get_memory_subgraph`, `fndr.source_evidence` (raw-text gated,
default closed), `fndr.open_target` (approval-gated), `fndr.rate_result`
(the write tool, renamed from `agent.rate_result`), plus `fndr.run` behind
the same approval gate as `agent.run` today.

Everything else in the `memory.*`, bare-name, and `agent.*` namespaces above
is either folded into one of these or removed. `agent.run` and `fndr.open_target`
are approval-gated today; preserve that gate while migrating their names and
callers.

## Migration order

1. Gate `agent.run`, `start_meeting`, `stop_meeting`, and `fndr.open_target`
   through `policy_for_action` and the explicit in-app approval card. Missing
   UI, timeout, decline, or the actions kill switch fails closed. Make
   `memory.source_evidence` raw-text default-closed. Done in code; the focused
   MCP and UI checks pass.
2. Remove `memory.search_raw` and `search_memories` (pure duplicates). Done.
3. Point every merge candidate's callers at its `fndr.*` equivalent, then
   remove the old name once nothing calls it.
4. Rename `agent.rate_result` to `fndr.rate_result` for naming consistency.

Steps 1 and 2 are implemented as separate migrations. Steps 3 and 4 remain
follow-up work so they can be reviewed independently rather than landing as one
large surface change.
