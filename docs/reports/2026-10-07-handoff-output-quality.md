# Handoff: output quality lane, 2026-10-07

Written at the end of a long session. Everything below is on `main` unless it says "uncommitted". Read this with `docs/evidence/W04/` and `docs/team/tickets/anurup-embeddings-retrieval-2026-10.md`.

## Goal of the lane

Every sentence FNDR shows is a finished, neutral, past-tense statement backed by the capture, and nothing is presented as a task or a confident match without evidence.

## How to work in this checkout

- A second agent session (ChatGPT, committing as the owner) works in the same checkout on `main`. Its uncommitted edits are in the tree, mostly MCP, Notch, Hermes and setup files.
- Run `git status -sb` before every commit. Commit by explicit path. Never stash, reset, rebase or switch branches.
- When a file holds both sessions' edits, commit only your hunks: `git diff -- <file>`, keep your hunks in a patch, `git apply --cached <patch>`. This was done for `ipc/commands/stats.rs`, `ipc/commands/hermes_agent.rs` and `CHANGELOG.md`.
- The library's own test build in this checkout is often broken by the other session's in-progress MCP test code. To run unit tests anyway, export `HEAD` to a scratch folder (`git archive HEAD src-tauri | tar -x -C <scratch>`), copy your changed files over it, symlink `dist`, and run `cargo test --lib` there. Integration tests (`cargo test --test <name>`) build against the library and run in the checkout.
- Two library tests fail for reasons outside this lane: `agent::risk_policy::tests::mcp_call_tool_checks_the_gate_before_dispatching_any_tool` (looks for a string the other session's committed MCP change removed) and `ipc::commands::agent::action_lifecycle_tests::propose_then_approve_then_execute...` (runs `git status`; fails only in a scratch copy that is not a git repository).
- `git push origin main` pushes to GitLab and GitHub. No co-author trailers. No em or en dashes anywhere.
- The machine has 8 GB. A hook blocks shell commands under critical memory pressure. Do not load the local model while FNDR is running.
- Tools refuse the real profile without `--allow-real-profile`. Rewriting the real vault needs FNDR closed and a backup first.

## Uncommitted work in progress (finish this first)

The briefing. State of the tree:

- `src-tauri/src/briefing.rs`: `plain_briefing` added (deterministic fallback).
- `src-tauri/src/inference/mod.rs`: `copies_briefing_notes` and its unit test; `generate_daily_briefing` returns an empty string when the model only copies its notes; a by-hand model test `briefing_reports_and_does_not_advise`.
- `src-tauri/src/inference/prompts.rs`: one more rule in both briefing prompts ("Write new sentences that combine the notes..."), `LLM_PROMPT_VERSION` v8, and new fingerprints for `daily_briefing_evening` and `daily_briefing_morning`. The fingerprints were computed outside the test build with a script whose method was checked against the committed values earlier; confirm with the guard test.
- `src-tauri/src/ipc/commands/hermes_agent.rs`: the command falls back to `plain_briefing` when the model returns nothing. This file also holds the other session's hunks.
- `src-tauri/tests/daily_briefing_inputs.rs`: test for `plain_briefing`.

None of this has been compiled or run since the last edit. Next steps, in order:

1. `cargo fmt` the five files, `cargo check --lib`, `cargo test --test daily_briefing_inputs`.
2. Run the unit tests in a scratch copy (`prompt_changes_require_a_version_bump`, `a_briefing_that_hands_the_notes_back_is_recognized`, `briefing_drops_advice_the_notes_never_gave`).
3. With FNDR closed, run `cargo test --lib briefing_reports_and_does_not_advise -- --ignored --nocapture` and read the CHECK line. The previous prompt (v7) gave no advice and did not name an app as the actor, but copied the notes back word for word in all three cases. If v8 still copies, the fallback covers it; consider making the plain briefing the default and dropping the model call.
4. Commit by path (hunk-split `hermes_agent.rs`), update `docs/product/llm-task-catalog.md` and `CHANGELOG.md`, push.

Note: the startup briefing notification in `src-tauri/src/main.rs` builds its own lines in the old `[App] title — snippet` format and does not use `briefing.rs`. It was left alone because the file holds the other session's edits. It should use `briefing::briefing_lines` and the fallback.

## Shipped today, by area

**Search**
- Strong-match rule: a top result backed only by a loose keyword hit is weak (`context_runtime/retrieve.rs`). Evidence: `docs/evidence/W04/strong-match.md`. Fitted negatives 5 of 12 to 0; twelve fresh negatives 8 to 3; 3 of 98 real persona queries with the right memory first are now marked weak.
- Companion search uses the live retrieval function; unused `GraphStore::reconstruct` removed; `tests/search_relevance_eval.rs` measures the live engine (MRR gate 0.90).
- VS-87: no weight change. The config weights belong to the older `HybridSearcher`, still used by the raw MCP search tool.

**Memory text**
- VS-89: one activity label list; `observing` and `screen_review` store as `unknown`. Intent text in the embedding measured as neutral and kept.
- VS-93: prose is dropped from the `commands` field.
- VS-91: stored narration reworded by the repair scan; a review cannot keep a placeholder as the card line. Applied to the real vault (narrated 26 to 3; search by earlier sentences 37 of 40).
- Voice cleanup: strips has, have and had with the narrator, drops the orphaned possessive, puts listed leading -ing verbs into the past tense, drops "The session involves". "In a ... window titled" counts as narration.
- A capture that is mostly a macOS permission dialog is low signal (`LowSignalReason::SystemPrompt`).

**Tasks** (`docs/evidence/W04/task-audit.md`)
- Old extractor: 471 open tasks from 147 memories, none ever completed. Replaced by `tasks/suggest.rs`: a suggestion must be supported by a sentence on the screen that states a commitment or request; requests count only in mail, chat and notes; a deadline needs a date; the window title and text addressed to an AI never count; asked once per new memory, not on AI chat or system screens, at most once per app per ten minutes; repeats dropped by meaning (cosine 0.80).
- To-dos shows the person's tasks, then at most five suggestions with their quote, offered for three days. Opening the list no longer creates tasks.
- Daily Summary counts only the person's own open tasks; system processes are not apps used.
- The 471 old suggestions were dismissed on the real vault with `retire_task_suggestions` (nothing deleted).
- Replay on the real vault with the real model: 147 memories, 51 asked, 0 suggestions.

**Tools added** (`src-tauri/examples/`): `task_audit`, `retire_task_suggestions`, `task_suggestion_replay`; `vault_qa` gained weak-summary, unrelated-query and score sections; `retrieval_qa` prints the product's own strong-match flag.

## Real vault state

- Backups in `~/Library/Application Support/`: `com.fndr.app.lancedb-backup-20261007-161357-before-task-cleanup-and-repair`, plus two from 2026-10-06. The owner has not been asked to delete them.
- The Quality Lab profile (`com.fndr.app.quality-lab/knowledge-worker`) is the other session's synthetic profile. It was never modified. Its 241 open tasks are old unquoted suggestions, so its To-dos is empty under the new rule.
- Stored summaries written before the latest voice cleanup are fixed at display time. Running `repair_truncated_summaries --apply --allow-real-profile` again (FNDR closed, backup first) would also fix the stored text and vectors.

## Open items

1. Finish the briefing (above).
2. Recall of task suggestions on real mail, chat and notes is unmeasured. The owner's vault has almost none. It needs the owner to mark the tasks they would have written on a day or two of such captures; `task_suggestion_replay --show` prints kept suggestions with quotes.
3. The personal-surface app list in `tasks/suggest.rs` is fixed, not a setting.
4. Strong match: a time phrase counts as topic evidence ("last weekend"); needs more time-phrased queries before changing. One fresh negative passed on keyword and vector support and was not looked into.
5. Six reviewed placeholder summaries need another review pass (model run, FNDR closed).
6. VS-88 (BGE prefixes) waits for the model to be installed. VS-92 (one session memory) is not started; true once-per-session task extraction depends on it.
7. The other session's lane, not verified here: MCP gating, confidence badge, Vault layout, VS-90 remainder.
8. The owner reported the To-dos panel as "blank with no content" on the real profile. Expected content is the header, the add row and "Nothing on your list...". Whether the empty-state sentence showed was not confirmed.
9. The new To-dos screen was verified in a browser preview with sample data and by tests, not in the native window.

## Decisions the owner made today

- Work stays on `main`; both sessions commit there.
- Figma design-review gold rows are labelled `unknown`.
- Tasks are suggestions until accepted; AI chat screens are not asked for tasks.
- Task cleanup and summary repair were applied to the real vault.
