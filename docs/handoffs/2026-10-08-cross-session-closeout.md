# Cross-session closeout, 2026-10-08

## Read this first

Work stops at this checkpoint. `main` contains intertwined commits from the Codex peer/MCP lane, the Claude output-quality lane, and the Claude agent-surfaces lane. The shared checkout also holds other sessions' uncommitted edits. Inspect `git status --short` before any change; never stash, reset, or stage a shared file wholesale. `git push origin main` sends to GitLab and the GitHub mirror. The detailed source handoffs are [Codex peer/MCP](2026-10-08-codex-agent-peer.md), [output quality](../reports/2026-10-07-handoff-output-quality.md) and [agent surfaces](2026-10-07-agent-surfaces.md); their older interim status sections are superseded by later dated sections and this checkpoint.

## Shipped this week

| Lane | What reached `main` | Evidence and limits |
| --- | --- | --- |
| Output quality | Grounded task suggestions and direct task finder; To-dos separation; model-free daily briefing; neutral memory and voice cleanup; storage-safe repair; stronger match labeling and title search. | `docs/evidence/W04/` and the output-quality handoff. The 30-screen task eval is synthetic. Real mail/chat recall remains unmeasured. |
| Agent surfaces | Notch Do and Hermes safety/UX work, explicit approval, scoped tool reads, privacy visibility, Labs and Operate settings, ADR 018/024/026 decisions, unmounted-panel cleanup. | Agent-surfaces handoff, ADRs, and native smoke notes. Pinned Hermes gateway and several native action checks remain open. |
| Codex peer/MCP | MCP server enforces Hermes's four read tools, current sharing consent and raw-evidence refusal (`aeb8a05`, `4dbcc95`). Bounded A2A Card validation, saved peer directory and review-only task draft (`d1a575f`, `adb2891`, `80340a9`). Draft attachments use current authorized display summaries; empty summaries cannot fall back to raw OCR (`bf7e126`). | ADR 025. A2A task send, authentication, cancellation, durable egress accounting and independent peer interoperability do not exist yet. |

## Verification and what it proves

- Codex: focused MCP tests 39 + 15; peer parser 7; peer store 1; delegation 2 (the final privacy check ran in a detached clean worktree because another session's uncommitted `codex_account.rs` did not compile); frontend 23; TypeScript and Vite build passed. Browser preview exercised peer add/remove and draft preview with synthetic IPC.
- Disposable `FNDR_DATA_DIR` native Tauri build launched and confirmed profile override. No FNDR window appeared in the automation/window inventory, so native UI and IPC behavior were **not** verified. The temporary profile was removed after stopping the app. No real peer request was sent.
- Other lanes' detailed command results and real-vault evidence are in their source handoffs. Do not turn a browser preview, unit test, or vault-copy result into a claim about the running owner profile.
- `make test` on the shared tree is not a reliable gate while other sessions are editing Rust files. Re-run a clean full gate when the tree settles, with `CARGO_BUILD_JOBS=1` on this 8 GB Mac.

## Decisions and long-term sequence

1. **Finish in-flight changes, then integrate.** Each Claude session owns its unstaged files and commits only its own hunks. Resolve any compile break at its source. Run focused checks, then a clean `make test`; preserve failed gates and exact causes.
2. **Prove the core daily loop.** Run native To-dos empty-state and real mail/chat task-recall QA with reviewed labels; finish the second summary repair only with FNDR closed and a fresh backup; do not change the owner vault from an automated scratch test. Confirm strong-match negatives on freshly seeded dates. For agent surfaces, run the twenty-task Notch Do set, pinned Hermes tool-list/refusal check, Stop/halt checks, and native permission/voice flows.
3. **Finish peer delegation as one privacy-safe vertical slice.** Revalidate the Card at Send, bind credentials outside the Card, rebuild and compare the exact preview from current visible sources, create durable local run and content-free egress records, then send the exact reviewed bytes to an independent A2A peer. Deny stale/hidden/deleted aliases and unexpected remote content; test failure/retry/cancel before calling the feature complete. Keep it behind explicit person action.
4. **Measure quality before expanding scope.** Use Quality Lab's isolated native profile, reviewed synthetic cases and owner-approved real-task labels. Record support, refusal, latency and resource cost per workflow. Advance board tickets only when their stated Done when and Evidence conditions hold.
5. **Beta closure.** Review unresolved VS/PD/agent tickets, run the serial full gate and release smoke on a clean checkout, then have a reviewer close Evidence tickets. No new framework or second capture/retrieval pipeline for these tasks.

## Doctor: what worked and what did not

- **Worked:** small commits on `main`; explicit-path staging; focused tests in a detached worktree when shared edits broke compilation; current-record authorization at the MCP and draft boundaries; browser preview for mounted UI; disposable profile for native build isolation; model-free briefing after real-model comparison showed fabrication/copying.
- **Did not work:** shared uncommitted Rust files repeatedly blocked whole-tree tests; a Card can be saved but not yet used; native app launch could not prove visible interaction; the browser fixture could not prove Tauri permissions or networking; model prompt iteration alone did not make briefing trustworthy; appending dated handoff sections left stale instructions visible beside new facts; the board did not move with commits.
- **Process correction:** write one current-state checkpoint that links deep evidence, mark superseded interim states, attach a ticket comment and status to each merged slice, and distinguish implemented, measured, native-verified and unverified. Do not copy private text into tickets or handoffs.

## Ownership and guardrails

Codex owned `src-tauri/src/agent/{peer,peer_store,delegation}.rs`, the peer workspace UI, ADR 025 and scoped Hermes MCP changes. Claude owns the current dirty `risk_policy`, `computer_use`, `search`, `storage`, `memory-vault`, and related files. Check current ownership from `git diff` before touching any shared path. Prompt changes require a version bump, catalog update and fingerprint test. Embedding-contract changes require a reindex plan and retrieval evidence. Real vault writes require the owner-approved workflow, stopped app and backup. Keep source captures, secrets and database files out of Git and GitLab.

## Board at closeout

Eight focused tickets were created: AG-01 through AG-05 and OQ-01 through OQ-03. AG-01, AG-03, OQ-01 and OQ-02 are in Evidence; AG-02/04/05 and OQ-03 remain Ready for their stated gates. Existing PD-01, VS-61, VS-85/86/87 and VS-93 moved to Evidence with commit/test comments; VS-84/88/89/90/91/92 remain Ready with precise partial-work comments. A concurrent `dadc2d9` added decision, next-step and error counts to the Vault session row after this handoff was drafted; VS-92 still lacks durable session memory and cross-app linking. Reviewers, not agents, close Evidence tickets.
