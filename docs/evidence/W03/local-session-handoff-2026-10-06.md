# Handoff: FNDR local integration

## Goal

Finish the owner's ordered follow-up to cloud PRs #31, #33–#37. The owner
requested a stopping point on October 5 at about 10:08 PM America/Denver to
conserve credits; resume with the owner tomorrow. No automation was scheduled.

## Current state

Resumed October 6: three persona gates completed (knowledge-worker PASS,
software-engineer PASS, office-pm FAIL with installed tokenizer). A separate
pinned-tokenizer office-PM run passed with every reference rank restored.
Real-vault read-only health audit completed. Exact manual checklist is still
missing; native QA remains pending. See the updated report below.

Steps 1–4 are committed on `main`: `f9caa25`, `09e7748`, `a1eee41`, `802dcf3`.
PR #36 was merged after CI passed; baseline `1b3638c` was synced to GitLab.
See [verification and manual QA status](manual-qa-2026-10-06.md) for exact
changes, test evidence, logs, and the missing-checklist boundary.

## Decisions made

| Decision | Reason | Files/Docs |
|---|---|---|
| Preserve existing capture filters; add replay test | Production exclusion already landed in `86aaefe` | `tests/merge_replay.rs` |
| Persist before changing live consent; reject stale UI loads | Failed saves and old loads must not misrepresent permission | `privacy.rs`, `ControlPanel.tsx` |
| Reuse the MCP write gate | One policy for both assistant-write tools | `risk_policy.rs`, `remember_http_tests.rs` |
| Keep native/manual QA pending | Owner requested stopping; exact ten-step list absent | Manual QA report |

## Files changed

The four commits contain the focused replay, Settings/IPC, MCP gate, and test
honesty changes. This handoff commit adds only this file and the QA report.
Use `git show --stat` for the exact paths; no generated captures or vault
data belong in these commits.

## Files inspected but not changed

`AGENTS.md`, portable diagnose/TDD/handoff skills, `Makefile`, `config.rs`,
capture merge logic, cloud outbox, and seed/retrieval scripts.

## Commands run

Baseline full test and all focused checks passed. Final full test: 545 frontend
and 1,160 Rust tests passed, 15 Rust tests ignored; typecheck/build passed.
Persona commands were pending at the initial stop; the resume results are
recorded under Current state and in the linked report. The temporary Vite server and browser
tab were closed at the checkpoint; no native FNDR process was launched.

## Tests / verification

See the linked report for red/green failures, passing counts, and browser
limitations. Native IPC dispatch and real-vault operations remain untested.

## Known issues

Fresh `gh/claude/cloud-outbox` (`6f871ea`) contains no ten-step manual QA list.
The owner was asked for it; no answer received before stopping. An unrelated
storage-scale test regenerates tracked `docs/evidence/W04/storage-indexes.md`;
restore only this generated diff after checking that no user edits preceded it.

## Next steps

1. Check clean state/remotes; preserve any new work. GitLab fetch succeeded
   on retry during the resumed session; GitHub outbox remains `6f871ea`.
2. Obtain the exact ten-step cloud checklist from the owner. Launch the
   updated native app, run the authorized real-vault checks, and update the
   manual QA report. No app or dev server was started by the resume session.
3. Keep the installed-tokenizer office-PM failure visible. Pinned comparison
   passes, but owner assets/vectors have not been changed. Do not loosen gates.
4. Commit remaining native evidence and push to the explicit GitLab URL.

## Risks / do not do

- `origin` has two push URLs. Use `git@capstone.cs.utah.edu:fndr/fndr.git` to
  push only GitLab.
- Use `CARGO_BUILD_JOBS=1`; this Mac has 8 GiB RAM and limited free disk.
- `FNDR_DATA_DIR` redirects the vault but **does not** redirect `Config::save`.
- Keep EmbeddingGemma inactive. Do not migrate, reset, or seed the owner vault.
- Do not equate preview/mock/temporary-store results with native QA.

## Useful context for next agent

The owner wants separate understandable commits and truthful verification.
The real vault is under `~/Library/Application Support/com.fndr.app`, while
configuration uses `~/Library/Application Support/com.fndr.FNDR/config.toml`.
