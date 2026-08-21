# FNDR v2 planning deliverables

Produced 2026-08-19 from the v2 discovery brief, a four-agent code audit of this POC, and fresh technology research; revised 2026-08-20 to fold in the team's ten lived v1 pain points (see PRD §6, "Pain points from lived v1 use"). Everything here is **proposed, awaiting review**; no v2 code exists.

| Deliverable | File(s) |
|---|---|
| PRD | `PRD.md` |
| ADR set | `decisions/ADR-001-app-stack.md` (Tauri + Rust engine + Swift sidecar), `ADR-002-storage.md` (SQLite truth + LanceDB index), `ADR-003-inference.md` (Apple frameworks + llama.cpp, model lineup), `ADR-004-local-only-boundary.md`, `ADR-005-poc-reuse-policy.md` (port/reference/discard inventory), `ADR-006-retrieval-architecture.md` (one stack, chunk RAG, eval-gated), `ADR-007-mcp-surface.md` (14 tools, auth-always) |
| Architecture | `ARCHITECTURE.md` |
| Roadmap and tickets | `ROADMAP-TICKETS.md` (15 epics, 122 tickets) and `tickets.csv` (GitLab CSV import; descriptions carry `/label` and `/milestone` quick actions) |
| Plan review (2026-08-20) | `review/REVIEW-2026-08-20.md` (synthesis and action plan) plus the four full reviewer reports in `review/`. The "fix before bootstrap" edits are applied across the docs, and the "decide this week" owner decisions were dropped with defaults recorded in PRD §13 (right-click install path, GitHub-plus-GitLab status quo, DRIs at kickoff). Only the deferred capacity re-cut remains for sprint planning. |
| Engineering skill | `skills/fndr-v2-engineering/` (SKILL.md plus references). Install into the new repo at `.claude/skills/fndr-v2-engineering/`. Replaces, and does not port, the POC's `.agent-skills/portable-engineering/` system. Run the skill-creator eval loop against real repo tasks once the v2 repo exists. |

Reading order for reviewers: PRD, then ADR-001/002/003 (the stack triangle), then ARCHITECTURE, then the rest of the ADRs, then the roadmap.
