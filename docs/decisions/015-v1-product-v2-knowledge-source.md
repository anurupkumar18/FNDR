# ADR-015: FNDR v1 is the product; FNDR v2 is a knowledge source

Status: Accepted 2026-09-21
Decider: project owner (Anurup). Team review happens on the merge request.

## Context

FNDR v2 (`~/FNDR-2.0`) was planned as a rebuild, and its PRD names it the mainline for the semester (Semester execution addendum, 2026-09-05, in `docs/v2/PRD.md`). Since then the team has kept shipping in this v1 repo (the Screen Guide in ADR-014, the Wrapped recap, and the BGE lazy-embedding fixes), and the GitLab remote used for the course is this repo (`capstone.cs.utah.edu:fndr/fndr`). Work split across two repos hides progress from teammates and from instructors, who inspect the board and the commit history.

## Decision

The Beta (about 2026-10-21) and Final (about 2026-12-14) run on this repo. v2 is a read-only reference: we take its audit findings, tests, and targeted ports (with a `// Ported from FNDR v2 <path>` note), not its architecture wholesale.

## Consequences

- One repo, one board, one commit history for instructors to inspect.
- v2's crate boundaries are not adopted; stage modules are extracted opportunistically instead.
- The v2 PRD addendum is superseded for the schedule. Its MCP surface rules (`docs/v2/decisions/ADR-007-mcp-surface.md`) and local-only boundary (`docs/v2/decisions/ADR-004-local-only-boundary.md`) remain useful references.
- `CLAUDE.md` no longer directs v2 work elsewhere.
- The Beta and Final dates above are working assumptions until the team confirms them.
