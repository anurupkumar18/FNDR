# Handoff: Agent surfaces (Hermes Agent, Screen Guide, Notch Do)

Date: 2026-10-07. Written at the owner's request when the session ran low on context.

## Update, later on 2026-10-07

The fourth batch is committed as `f45faa0` (116 Rust tests in the repo, 260 front-end). It was **not pushed**: the push failed with "Could not read from remote repository"; retry `git push origin main`. The shared Rust test build works again, so the copy method below is no longer needed. The owner approved the six live checks. Next in the five-hour plan: tests for the four untested pieces, the fixtures and task set, the live checks, then fixes. See "Phase 5 progress" in the breakdown.

## Update 2, 2026-10-07

- Local `main` is ahead of `origin/main` by my commits `f45faa0`, `c156d93`, `055d36f` and the Agent stop test, plus another session's `98fe16e`; it is also behind by nine upstream commits (Minh's reopen merges).
- **The push is blocked.** A merge is needed first, and upstream changed `src-tauri/src/mcp/mod.rs`, which another session has uncommitted edits in. Git will refuse to merge until that session commits. Do not stash it. Once it is committed: `git merge origin/main`, run the focused tests, `git push origin main`.
- Upstream also changed `src/shared/ipc/tauri.ts` and `ipc/commands/screen_guide.rs`; expect to check those after the merge.
- Done from the five-hour plan: land the batch, docs, tests for the mid-run halt and Agent reply Stop.
- Left: tests for the stale-gateway kill and the operate setter; ten fixture screens; twenty-task Notch Do set; the six live checks (approved by the owner); fixes from them.

## Goal

Take over Kunj's agent features, find what is unsafe or unclear, decide, fix,
and prove it. The plan, findings and decisions live in:

- `docs/superpowers/plans/2026-10-07-agent-surfaces-work-breakdown.md` (184 parts, progress tables, findings N1 to N14 and P1 to P5)
- `docs/decisions/024-agent-surfaces-egress-and-action-policy.md` (accepted by the owner, decision table E1 to E14)
- `docs/superpowers/specs/2026-10-07-agent-surfaces-prd.md`
- `docs/evidence/W03/agent-surfaces-phase1.md`

## Current state

Three commits are on `main` and pushed to both remotes:

- `5b98a86` first safety and privacy fixes
- `780feee` PRD, ADR 024, breakdown, phase 1 evidence
- `ed175dd` the ADR 024 decisions for Hermes and Notch Do

A fourth batch is **written but not committed and its Rust tests have not been
run**. The owner stopped the Rust test run. It builds "decided but not built":

| Item | State |
| --- | --- |
| E6: the notch shows what it heard for 1.2 s before sending (`heard` phase, Cancel, Send now); typed requests skip it | Written; front-end tests pass |
| C3.1: "Operate my Mac" moved to its own config section `operator.enabled`, read once from the old `screen_guide.operate_computer`; new command `set_computer_use_enabled` | Written; compiles; Rust test not run |
| A6.7, A6.8: Hermes instruction strings moved to `inference/prompts.rs` (`HERMES_CHAT_INSTRUCTIONS`, `HERMES_IDENTITY`, `HERMES_OPERATING_NOTES`, `HERMES_ATTACHED_MEMORIES_HEADER`) with fingerprints and catalog rows; text rewritten to match what Hermes can do | Written; compiles; fingerprint test not run |
| E12: sidebar group "Assist" split into "Trust" (Privacy Activity) and "Labs" (Hermes Agent, Screen Guide, Engine diagnostics) | Written; app tests pass |
| E10: OpenClicky and Operate switches carry a "Labs" tag | Written |
| E11: Notch Do in the SK-01 journal | Not started; waits for SK-01 |

## Decisions made

| Decision | Reason | Files/Docs |
|---|---|---|
| All of E1 to E14 taken as recommended | Owner, 2026-10-07; Kunj agreed with the findings | ADR 024 "Decisions taken" |
| "Next" is not on the browser run list | A Next button often submits a form step | ADR 024 |
| No cap on chat history yet | Deleting chats is a retention choice for PD-09 | Breakdown, phase 2 |
| Gateway errors kept in memory, not a log file | Hermes output may contain message text | Breakdown, phase 2 |
| Notch default stays Do | Alt+N is designed to open Do and listen; E6 was solved with the heard step instead | `NotchOperator.tsx` |
| E12 done as a small regroup, not the full five destinations | The full regroup is ticket PX-01 in another lane | `src/app/App.tsx` |

## Files changed (uncommitted, mine)

| File | Change | Notes |
|---|---|---|
| `src-tauri/src/config.rs` | `OperatorConfig`, migration, test | Mine only |
| `src-tauri/src/ipc/commands/computer_use.rs` | reads `operator.enabled`; `set_computer_use_enabled` | Mine only |
| `src-tauri/src/ipc/commands/hermes_agent.rs`, `agent_chats.rs` | use the prompt constants; header wording | Mine only |
| `src-tauri/src/inference/prompts.rs` | four Hermes constants, `live_prompts` and `FINGERPRINTS` rows | **Shared.** The other session's hunks: version v6 to v7, daily briefing text, two briefing fingerprints |
| `src-tauri/src/main.rs` | registers `set_computer_use_enabled` | **Shared.** Their hunk: `resolve_mcp_approval` |
| `src/shared/ipc/tauri.ts` | `setComputerUseEnabled`; removed `operate_computer` | **Shared.** Their hunks: MCP approval types and function |
| `src/domains/notch/doRun.ts`, `NotchOperator.tsx`, both notch tests | heard step | Mine only |
| `src/domains/screen-guide/ScreenGuidePanel.tsx`, `.css`, its test | new setter, copy, Labs tags | Mine only |
| `src/app/App.tsx` | Trust and Labs groups | Mine only |
| `docs/product/llm-task-catalog.md` | four Hermes rows; two stale rows fixed | **Check:** it no longer shows as modified. The other session's commit `145a0eb` may have included it. Confirm the rows are there |

Everything else modified in the tree belongs to the other session.

## Commands run

| Command | Result |
|---|---|
| `npx tsc --noEmit -p .` | clean |
| `npx vitest run src/domains/workspace src/domains/notch src/domains/screen-guide src/domains/privacy-proof src/domains/setup src/app` | 35 files, 260 tests pass |
| `cargo check --lib --bin fndr` (in `src-tauri`) | Finished, no errors |
| Rust tests for the fourth batch | **Not run** |

## Tests / verification

Rust tests cannot build in the shared tree while the other session's MCP work
is unfinished (`mcp/mod.rs`, `mcp/remember_http_tests.rs`). The method used so
far: copy `src-tauri` without `target` to a scratch folder, symlink `target`
and `../dist` back to the repo, remove the `remember_http_tests` module line
and put `#[cfg(any())]` on `localhost_handshake_bypasses_auth_but_tools_call_requires_token`
in the copy, then run:

```bash
CARGO_BUILD_JOBS=2 cargo test --lib -- ipc::commands::computer_use privacy_proof ipc::commands::hermes_ ipc::commands::setup_center ipc::commands::codex_account ipc::commands::agent_chats operator:: inference::prompts config::
```

The owner rejected this run once; ask before repeating it. Try the repo first:
the shared build may be fixed by now.

## Known issues

- The four new prompt fingerprints were computed outside Rust. If
  `prompt_changes_require_a_version_bump` fails it prints the right values;
  paste them into `FINGERPRINTS`.
- The committed tree after a by-hunk commit has never been built on its own.
- Hermes tool limits (`platform_toolsets.api_server: [todo]`, MCP
  `tools.include`) were written against Hermes 0.13's source. The pinned 0.18
  copy is not on this Mac.
- Only the pinned Hermes runs now, so the Agent page on this Mac will ask to
  install it.
- Catalog row `hermes_memory_context` still says snippets go with every
  message; it should say "for a provider on this Mac, or when turned on".
- Not covered by a test: the half-second halt check, backend Stop for Agent
  replies, the stale-gateway kill, `set_computer_use_enabled`.

## Next steps

1. Run the Rust tests for the fourth batch (repo first, then the copy method with the owner's go-ahead). Fix fingerprints if the test asks.
2. Commit the fourth batch by path. For `main.rs`, `tauri.ts` and `prompts.rs` stage only my hunks: `git diff -U0` the file, drop hunks that mention `mcp_approval`, `McpApproval`, `MCP_APPROVAL`, `resolveMcpApproval`, `LLM_PROMPT_VERSION`, or `daily_briefing`, then `git apply --cached --unidiff-zero`. No co-author trailer.
3. Add a "phase 5" table to the breakdown and a changelog line for the fourth batch; update ADR 024's table (E6, E10, E12 to Built).
4. Fix the `hermes_memory_context` catalog row.
5. Live checks, with the owner's go-ahead on the ChatGPT account (list at the end of the phase 1 evidence): quit with the gateway up and `ps`; quit during a Notch Do run; Tab-then-type and reordering fixtures; Hermes tool list on a running pinned gateway; a write tool refused through Hermes's MCP entry; the Spotify and web-search example under the tighter browser rules.
6. Then Track F in the breakdown: the twenty-task Notch Do set and Agent latency.

## Risks / do not do

- Shared checkout with a second agent on `main`: never stash, switch branches, or `git add` a shared file whole.
- Do not run `rustfmt` on `prompts.rs`; it holds the other session's edits.
- Memory pressure on this Mac goes critical during Rust builds; a hook blocks tool calls when it does. Use `CARGO_BUILD_JOBS=2` and do not build while another `rustc` is running.
- Do not run Codex logout, a live Notch Do run, or a ChatGPT request without the owner saying so.
- No em or en dashes in docs, tickets or commit messages.

## Useful context for next agent

- Notch Do's policy is `src-tauri/src/operator/policy.rs`; the run loop and guards are in `ipc/commands/computer_use.rs` (`Guards`, `run_turn`, `plan_starts_by_itself`, `ask_person`).
- The fake Codex for tests is `src-tauri/tests/fixtures/operator/fake_codex_app_server.py`; "PROBE" and "REWORDED_APPROVAL" in a turn's text trigger the special cases.
- Hermes limits are in `ipc/commands/hermes_codex.rs` (`HERMES_TOOLS_YAML`, `HERMES_MCP_TOOLS`) and `hermes_agent.rs` (`provider_is_local`, `sends_related_memories`).
- Privacy Activity log: `src-tauri/src/privacy_proof.rs` (`Feature`, `record_model_request_including`, `init_model_request_log`).
- GitLab is the source of truth and GitHub mirrors it; a push to `origin` reaches both.
