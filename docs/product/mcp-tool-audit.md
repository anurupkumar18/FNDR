# MCP tool surface audit (RET-02)

51 tools registered as of this audit (verified 2026-09-23 with the extraction
script below; matches the count the WS5 plan expected on 2026-09-21, so the
surface has not grown since planning). No code change in this ticket.

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
| `memory.search_full_context` | read | yes | none | `search_memories`, `fndr.search` | `fndr.search` | merge |
| `memory.get_context_pack` | read | no (composed) | none | `agent.build_context_pack`, `fndr.build_context_pack` | `fndr.context_pack` | merge |
| `memory.agent_brief` | read | no | none | `memory.warm_start`, `memory.agent_onboarding` | none | merge |
| `agent.build_context_pack` | read | no | none | `memory.get_context_pack`, `fndr.build_context_pack` | `fndr.context_pack` | merge |
| `agent.run` | execute | no | runs an agent action | none | none (v1-only, post-dates v2) | gate behind approval |
| `agent.privacy_status` | read | no | none | `fndr.privacy_status` | `fndr.privacy_status` | merge |
| `agent.explain_retrieval` | read | no | none | none | `fndr.explain_retrieval` | keep |
| `agent.rate_result` | write | no | appends to `RetrievalFeedbackRating` | none | `fndr.feedback` | keep |
| `agent.list_prompts` | read | no | none | none | none | keep |
| `agent.get_prompt` | read | no | none | none | none | keep |
| `memory.timeline` | read | no | none | `fndr.timeline` | `fndr.timeline` | merge |
| `memory.active_focus` | read | no | none | none | `fndr.active_focus` | keep |
| `memory.warm_start` | read | no | none | `memory.agent_brief` | none | merge |
| `memory.agent_onboarding` | read | no | none | `memory.agent_brief` | none | merge |
| `memory.project_wiki` | read | no | none | none | none | keep |
| `memory.claims` | read | partial | none | none | none | keep |
| `memory.breakthroughs` | read | partial | none | none | none | keep |
| `memory.source_evidence` | read | **yes** | none | none | `fndr.source_evidence` (gated by `include_raw`, default closed) | keep, add the same default-closed gate |
| `memory.search_raw` | read | **yes** | none | `memory.search_full_context` | none | remove (redundant with the gated `source_evidence`) |
| `memory.projects` | read | no | none | none | none | keep |
| `memory.project_context` | read | no | none | `memory.project_wiki` | none | merge |
| `memory.decisions` | read | no | none | none | `fndr.recall` (decisions only) | keep |
| `memory.errors` | read | partial | none | none | none | keep |
| `memory.blockers` | read | partial | none | none | none | keep |
| `memory.todos` | read | no | none | none | none | keep |
| `memory.graph_query` | read | no | none | `memory.graph_context` | none | merge |
| `memory.graph_context` | read | no | none | `memory.graph_query`, `fndr.get_memory_subgraph` | none | merge |
| `memory.recent_changes` | read | partial | none | `fndr_get_recent_working_state` | none | merge |
| `search_memories` | read | yes | none | `memory.search_full_context`, `fndr.search` | `fndr.search` | remove |
| `ask_fndr` | read | no (composed) | none | `fndr.answer` | none | merge |
| `get_fndr_stats` | read | no | none | `fndr.quality_status` | none | merge |
| `start_meeting` | execute | no | starts audio capture | none | none | keep, requires approval |
| `stop_meeting` | execute | no | stops audio capture | none | none | keep, requires approval |
| `get_meeting_transcript` | read | yes | none | none | none | keep |
| `search_meeting_transcripts` | read | yes | none | none | none | keep |
| `get_ambient_context` | read | yes | none | `fndr_context` | none | merge |
| `fndr_context` | read | yes | none | `get_ambient_context` | none | merge |
| `fndr_search_code_context` | read | partial | none | none | none | keep |
| `fndr_diff` | read | no | none | none | none | keep |
| `fndr_get_recent_working_state` | read | partial | none | `memory.recent_changes` | none | merge |
| `fndr_remember_decision` | write | no | appends to the decision ledger | none | `fndr.remember_decision` | keep |
| `fndr_health_check` | read | no | none | none | none | keep |
| `fndr.search` | read | yes | none | `search_memories`, `memory.search_full_context` | `fndr.search` | **keep as the canonical search tool** |
| `fndr.answer` | read | no (composed) | none | `ask_fndr` | none | **keep as the canonical answer tool** |
| `fndr.build_context_pack` | read | no | none | `memory.get_context_pack`, `agent.build_context_pack` | `fndr.context_pack` | **keep as the canonical context-pack tool** |
| `fndr.get_related_memories` | read | no | none | none | none | keep |
| `fndr.get_memory_subgraph` | read | no | none | `memory.graph_query`, `memory.graph_context` | none | merge |
| `fndr.timeline` | read | no | none | `memory.timeline` | `fndr.timeline` | **keep as the canonical timeline tool** |
| `fndr.quality_status` | read | no | none | `get_fndr_stats` | none | merge |
| `fndr.privacy_status` | read | no | none | `agent.privacy_status` | `fndr.privacy_status` | **keep as the canonical privacy-status tool** |
| `fndr.open_target` | execute | no | opens a URL/app/file | none | `fndr.open_target` (sanitized, else explicit unavailable) | keep, requires approval |

## Summary

- **3 remove** (`memory.search_raw`, `search_memories`, plus folding `get_ambient_context`/`fndr_context` into one): pure duplicates of an already-kept tool with no distinct behavior.
- **~18 merge**: same information under a second name, usually from the `memory.*` namespace duplicating a newer `fndr.*` one. The `fndr.*` namespace should become canonical; `memory.*`/bare-name equivalents are the legacy surface to fold in.
- **4 execute-class tools carry real side effects with no approval gate today**: `agent.run`, `start_meeting`, `stop_meeting`, `fndr.open_target`. This is the same gap WS3's `policy_for_action` work is closing for the in-app agent (see today's fix requiring approval before `OpenUrl`/`OpenFile`) — these MCP-exposed equivalents should go through the same approval path, not a separate one.
- **2 tools release raw captured text without a gate**: `memory.search_raw` and `memory.source_evidence`. v2's equivalent gates this behind an explicit `include_raw` parameter that defaults closed. `memory.search_raw` should simply be removed (redundant); `source_evidence` should get the same default-closed gate.

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
move behind approval before anything else changes about them.

## Migration order

1. Gate `agent.run`, `start_meeting`, `stop_meeting`, `fndr.open_target`, and
   `memory.source_evidence` (raw-text default-closed) — safety first, no
   removals yet.
2. Remove `memory.search_raw` and `search_memories` (pure duplicates).
3. Point every merge candidate's callers at its `fndr.*` equivalent, then
   remove the old name once nothing calls it.
4. Rename `agent.rate_result` to `fndr.rate_result` for naming consistency.

This ticket stops at the recommendation; migration is separate follow-up work
so it can be reviewed and sequenced deliberately rather than landing as one
large surface change.
