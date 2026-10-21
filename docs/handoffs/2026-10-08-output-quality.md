# Handoff: output quality lane, state on 2026-10-08

This file states what is true now. It replaces the layered notes of 2026-10-07 and 2026-10-08; their history is in git and in `docs/evidence/W04/`.

## Goal of the lane

Every sentence FNDR shows is a finished, neutral statement backed by the capture, and nothing is presented as a task or a confident match without evidence.

## How to tell whether it still holds

| Question | Command | Passing today |
|---|---|---|
| Did search or ranking regress? | `make qa-retrieval-check PERSONA=<name>` for the three personas | PASS on all three |
| Did summaries, voice, labels or the strong-match rule regress on real data? | `make qa-vault` | PASS, 12 checks |
| Do task suggestions still need evidence? | `cd src-tauri && cargo test --test task_suggestions`, `cargo run --example task_suggestion_eval` | 29 tests; 30 of 30 screens |
| Does my Rust change stand without other sessions' work? | `make test-clean FILES="<files>"` | all green |

Working rules for this checkout are in `AGENTS.md` ("Shared checkout and shipping"). Decisions are rows in `docs/team/decision-log.md`.

The two library tests that used to fail here (the action lifecycle test outside a git checkout, and the Hermes token test on some runs) are fixed; see `2026-10-08-test-determinism.md`.

## Added on 2026-10-09: one visibility rule on every surface

An audit of who reads memories outside the shared retrieval path found four surfaces that skipped the blocklist: the "From your memory" toast, the briefing, the Daily Summary and usage statistics. Each could show a memory from an app or page blocked after the capture. The toast could also show a low-signal capture, raw screen text, or the capture taken seconds earlier. All four now use the rule in `context_runtime::retrieve`. The Hermes status command built unfiltered memory digests that no screen read; they are removed. `src/dev/memoryReadDrift.test.ts` (runs with `npm test`) lists every bulk reader with its check and fails on a new unlisted one. The MCP server's reads were read through as well: each goes through `load_memories_for_results`, which applies the same rule, and the health check reads only a timestamp. Nothing to change there.

Decided: the personal-surface list for task suggestions stays a fixed list. A setting would add a control nobody has asked for; add an app to the list when a real one is missing.

## What the product does now

- **Search.** A result is a confident match only with topic evidence: every query word, or closeness in meaning, or the entity route. Loose keyword support and being in the named time are not evidence. A query that is a window title lifts the memories with that title.
- **Summaries.** The card line is in voice when it is a past-tense statement of what happened or a present-tense statement of what the screen held. Narration is reworded or replaced at display time; a line that only repeats the window title reads "Viewed {title}.". The summary written without a model says "Viewed {title or files} in {app}".
- **Labels.** Activity labels come from the prompt's list. `reviewing_agent_output` stays only when an assistant is on screen.
- **Tasks.** A task is a suggestion until the person accepts it. It is kept only when a sentence on screen states a commitment or request, on mail, chat or notes; never from an AI chat or a system surface. At most five are offered, for three days.
- **Daily Summary and briefing.** Composed in code from memories and accepted tasks. No model writes them.
- **Vault sessions.** A session row is composed from its moments on read: length, files, decisions, next steps, errors, the most detailed earlier sentence, and links to sessions in other apps that share a file or two distinctive title words within ten minutes. Nothing is stored for a session.

## The owner's vault

- 158 memories. Task cleanup and two summary repairs are applied; a repair dry run finds nothing to change.
- Backups in `~/Library/Application Support/`: `com.fndr.app.lancedb-backup-20261008-172306-before-second-repair`, `...-20261007-161357-before-task-cleanup-and-repair`, and two from 2026-10-06. Each is about 740 MB on a disk that is 94 percent full. The owner has not been asked to delete them.
- Each reseed of a QA profile moves the previous one to the Trash, which also takes space.
- The Quality Lab profile (`com.fndr.app.quality-lab`) belongs to another session and was never modified.

## Open

1. **Recall of task suggestions on real mail and chat is unmeasured.** The owner's vault is mostly assistant windows. It needs the owner to mark the tasks in a day or two of such captures; `task_suggestion_replay --show` prints what was kept.
2. **A session is not one search result**, by decision. If that changes, the stored record should be composed by rule from the same fields the row uses, not written by the model.
3. **32 of 132 visible card lines are titles or noun phrases** the title rule does not reach. No rule restates them safely.
4. **Thresholds in `scripts/audit/vault-quality-thresholds.json` have room in them.** Tighten each when its number improves and stays there.
5. To-dos hides a suggestion from a blocked app; a task the person accepted from that app stays, by decision, because it is theirs.
6. VS-88: the BGE prompts wait for ADR 019 to pick the chunk model.
7. 6 of 158 primary vectors have drifted slightly from their text; they can be refreshed with the next rewrite of the vault.
8. The session row's counts, earlier line and cross-app link were looked at in the UI preview (`ui-preview.html`, whose sample data now has such a session) and the link's jump was clicked there. The native window has not been checked.
9. VS-20 is measured on the M1: retrieve p95 481 ms at 10,000 memories (`docs/evidence/W04/vs-20-scale-m1.md`). The margin is thin.
