# Changelog

Development log and pending-decision tracker for FNDR. See the [Beta-to-Final master plan](docs/superpowers/plans/2026-09-21-beta-final-master-plan.md) for the full roadmap.

---

## Recent Development

Latest work on the Beta-to-Final push:

- **Agent approval gating** — `Act` mode now requires explicit approval before `OpenUrl` or `OpenFile`, closing a policy gap where those actions previously executed without a prompt.
- **Screen Guide observability** — every voice/ask stage transition is now logged, and the flow recovers cleanly from a stuck transcription or ask call.
- **MCP tool surface audit (RET-02)** — all 51 MCP tools classified by risk and overlap in `docs/product/mcp-tool-audit.md`, with keep/merge/remove verdicts (see below).
- **Capture enrichment scheduling policy (MEM-06)** — policy code merged but **not yet wired into the live pipeline**.
- **Eval gold set v0 (MOD-04)** — draft evaluation set added for review, not yet finalized.
- Trust and honesty pass across Search, Ask, Timeline, Memory Vault, Daily Summary, and Stats: clearer pending/error/stale states and consumer-honest labeling.

---

## Flagged for Removal or Change — Needs Approval

These are called out in-repo (commit messages, `docs/product/mcp-tool-audit.md`, and the master plan) as pending a decision before they're acted on. Nothing below has been removed or changed yet; listing them here so they don't get merged silently.

| Item | What | Where |
| --- | --- | --- |
| `memory.search_raw` MCP tool | Flagged **remove**: duplicate of `memory.search_full_context`, and releases raw captured text without a gate | `docs/product/mcp-tool-audit.md` |
| `search_memories` MCP tool | Flagged **remove**: pure duplicate of `fndr.search` | `docs/product/mcp-tool-audit.md` |
| `get_ambient_context` / `fndr_context` | Flagged to **fold into one tool** | `docs/product/mcp-tool-audit.md` |
| `memory.source_evidence` | Flagged to **add a default-closed `include_raw` gate** (currently releases raw text ungated) | `docs/product/mcp-tool-audit.md` |
| `CGDisplay::screenshot` capture path | Deprecated Apple API family; plan proposes replacing with ScreenCaptureKit one-shot capture (CAP-06) | `src-tauri/src/capture/macos.rs:106` |
| Dead/duplicate post-capture code | MEM-02 in the master plan calls for deleting or adopting dead and duplicate post-capture paths; each behavior test should be ported to the live function before deletion | master plan MEM-02 |
| Enrichment scheduling policy (MEM-06) | Merged but not wired into the live pipeline yet — needs a decision on when/whether to activate it | `src-tauri/src/capture/` |
