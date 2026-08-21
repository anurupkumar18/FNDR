# FNDR v2 implementation kickoff

For the first implementation session. Planning is complete, reviewed, and merged; nothing in `docs/v2/` needs re-deriving. This file tells a fresh session exactly where things stand and what to do first.

## State as of 2026-08-20

- The full plan lives in `docs/v2/` (index: `README.md`). It was adversarially reviewed by four independent reviewers (`review/`), the fix-before-bootstrap changes are applied, and the owner dropped the remaining blockers with defaults recorded in `PRD.md` §13.
- **Nothing of v2 is built.** This repo is the v1 POC and stays untouched except as the `reference/v1` source. The v2 repo does not exist yet; creating it is the first task.
- Deferred on purpose: the review's full capacity re-cut and lane rebalance (see the header note in `ROADMAP-TICKETS.md`); revisit at sprint planning.

## Reading order (about 30 minutes)

1. `docs/v2/PRD.md` (scope, P0s, month-3 demo gate, §6 pain-point table, §13 defaults)
2. `docs/v2/decisions/ADR-001` to `ADR-003` (the stack triangle), then ADR-004 to ADR-007
3. `docs/v2/ARCHITECTURE.md` (crate map, boundaries, contracts)
4. `docs/v2/ROADMAP-TICKETS.md` conventions plus the E01 to E03 M1 tickets
5. Skim `docs/v2/review/REVIEW-2026-08-20.md` for the known risks and the watch list

## First moves (mirrors E01, in order)

1. **T-101 bootstrap:** create the new v2 repo (clean; GitHub per the §13 default), scaffold the Cargo workspace plus `ui/` skeleton per ARCHITECTURE §3, and import this POC's history as a read-only `reference/v1` branch.
2. **T-106/T-107:** copy `docs/v2/skills/fndr-v2-engineering/` into the new repo at `.claude/skills/fndr-v2-engineering/`, write CONTRIBUTING, and generate AGENTS.md from the skill (one conventions source for every agent tool).
3. **T-102 to T-105:** CI gates (tests, local-only egress lint, engine-independent-of-Tauri, generated TS bindings) plus the `make test` / `make bench` targets.
4. **T-108/T-109 and the spikes (T-208, T-310, T-408, T-906):** environment bootstrap on all four machines, then the walking skeleton (deliberately ugly capture-to-MCP slice by week 3) with the spikes feeding it.
5. Import `docs/v2/tickets.csv` into the GitLab board (Issues > Import CSV; create the six milestones and label set from ROADMAP conventions first).

## Conventions the fresh session must know

- This POC repo's hook blocks commits to `main`; work on branches. The remote `anurup/...` branch namespace is blocked by an existing `anurup` branch, so use plain or `feat/`-style names.
- Port discipline (ADR-005): ports arrive as targeted functions/constants/prompts with tests and a `// Ported from FNDR v1 <path>` note; the DISCARD list is never copied.
- Docs move together: PRD or ticket changes amend any ADR they touch in the same PR; ADR-007 is the MCP tool inventory of record.
- Never use em dashes in any output (owner preference, applies everywhere).

## Copy-paste kickoff prompt for the new session

> Read docs/v2/KICKOFF.md in the FNDR repo and follow it: start with the reading order, then execute the first moves beginning with T-101 (create the v2 repo, import reference/v1, install the fndr-v2-engineering skill, stand up the CI gates). The plan in docs/v2/ is approved; do not re-litigate ADRs, do re-read them. Work one vertical slice at a time and verify per the skill.
