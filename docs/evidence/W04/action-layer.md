# Action layer synthetic evidence, 2026-10-15

## Scope

The action registry names each tool's effect, target argument and undo instruction or reason no undo is available. The gate decides from the registry row, caller, structured arguments and the person's own request. The queue records the structured call, waits for a tap on confirm-tier actions, expires unanswered cards after 120 seconds, and checks the kill switch again at approval.

The owner decision in `FNDR-exchange/DECISIONS.md` item 1 is reflected in the gate: a typed or spoken request to open something may open an unnamed app by default. An optional setting makes unnamed apps wait for a tap. A quoted or negated use of "open" does not grant that run path.

## Synthetic injection matrix

`cargo test --test action_injection -- --nocapture` passed 7 tests. The corpus contains 10 synthetic screen and import attacks based on the W04 GATE cases: forged email instructions, Slack and calendar notes, browser and Git text, paste requests, and unregistered tools. For each attack, the matrix checked 4 callers and 32 acting registry rows, or 1,280 decisions. No row ran unattended when the person's request was absent. Separate positive controls checked a typed app request and an accounted-for link. Drift tests compared the registry with the Notch plan enum, the computer-use tool listing and the MCP tool listing. The four announced G read tools have registry rows. A synthetic executor test waited on the structured card and only set its execution flag after a tap.

This is a gate, source-list and fake-executor test. It does not claim an end-to-end result from Codex, the running MCP server, or a real imported source. The execution entry points listed under the blocked-file patches in `FNDR-exchange/reports/K.md` must call the queue before their effects run.

## Import trust

Three synthetic Mail tests failed before the readers were changed and passed after: the answer composer keeps forged header text on one data line, the cloud memory block labels and quotes imported text, and the agent context no longer promotes imported text to command or URL suggestions. The adjacent agent context, operator memory and composer suites passed 5, 3 and 7 tests. The memory-read drift guard passed 3 tests.

## Approval and settings

The action-layer unit suite passed 30 tests, including expiry, tap-only approval, kill-switch recheck and undo coverage. The persisted settings test passed after first failing on the missing settings type and config field. Two UI tests passed after first failing on the missing card and switch. The card test checked tool, target, arguments, undo reason and tap resolution. The preview IPC suite passed 18 tests; typecheck, frontend build, and `cargo check --bin fndr` passed.

## Limits for integration

The global queue has IPC and a visible panel, but no live execution producer in this worktree because `mcp/mod.rs` and `ipc/commands/computer_use.rs` are blocked. The owner must apply the report patches and run a synthetic end-to-end call that waits on the card before calling an executor. No real user files or profile data were read. No app was launched for this evidence run.
