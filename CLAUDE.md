# Claude Code: FNDR

**As of 2026-09-21 (ADR-015), this repo is the Beta and Final product.** FNDR v2 (`~/FNDR-2.0`) is a read-only knowledge source: take findings and targeted ports from it, do not build there. The Beta-to-Final plan lives in `docs/superpowers/plans/2026-09-21-beta-final-master-plan.md`. The historical v2 plan and kickoff stay under `docs/v2/`.

Follow **`AGENTS.md`** in this repository for every task. It defines mandatory defaults and which portable engineering skill to open under `.agent-skills/portable-engineering/` without waiting for the user to name one.

**Skill precedence (owner decision, 2026-10-06):** the portable skills are the repo's process. When a plugin skill, such as a superpowers skill, covers the same situation as a portable skill (`tdd`, `diagnose`, `grill-with-docs`, `handoff`), follow the portable one. Plugin skills are welcome only where the portable set has nothing.
