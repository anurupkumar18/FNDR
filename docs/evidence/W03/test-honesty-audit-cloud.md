# Test-suite honesty audit (cloud, Part 9 stretch)

Question: which tests pass without checking what their names promise? The scan covered main at c124957: 1,013 Rust tests and 473 frontend tests. This branch is rebased on 479e478.

## Fixed here

1. **Five placeholder agent tests.**
   - `tests/agent_regression.rs` had five `#[ignore]` tests behind a `todo!()` state factory: context pack determinism, no actions in Ask mode, no execution in Plan mode, an audit record per run, and dangerous commands refused. They never ran.
   - All five now run against a temporary store with two synthetic memories, and route budgets lifted.
   - The placeholder's determinism check compared `task_id`, which is a fresh UUID per pack, so it would have failed if anyone had filled in the factory. The real test checks the same memories in the same order, the same tool policy, and the same privacy scope, and that the ids differ.
2. **The dangerous-command test found two gaps in `agent::execution::validate_command`.** The test failed first on both. These commands only run after the person approves the action, but the allowlist promises read-only:
   - `git branch -D main` passed, and so did `git branch new-name`, which creates a branch. `git branch` now only lists (`-a`, `-r`, `-v`, `--list`, `--show-current`).
   - `git diff --output=<file>` and `git log --output <file>` passed and write a file. Any `--output` argument is now refused.

Results:
- `cargo test --test agent_regression`: 22 passed.
- Lib: 1,064 passed, 0 failed (Linux, 479e478 plus this change).

## Found, not changed (for local)

| Finding | Where | Suggested fix |
|---|---|---|
| A test that asserts nothing: `_suppress_unused_warning` builds a `FusedHit` and drops it. `FusedHit` is used in production code, so it silences no warning. | `context_runtime/composer.rs` | Delete the test. |
| Six `#[ignore]` tests give no reason or run command. | `codex_account.rs` (3 live Codex tests), `clip_vision.rs` (1), `tests/storage_scale.rs` (2) | Add `#[ignore = "why; how to run"]`. |
| Three tests return early and pass when the model is missing, so on CI they test nothing. Their names say so. | `meeting/mod.rs` (`transcript_embeddings_are_non_zero_for_real_backend_when_available`), `ipc/commands/mod.rs` (`focus_task_embedding_is_present_for_real_backend_when_available`), `tests/query_plan_rules.rs` (`llm_refinement_smoke_skips_when_model_missing`) | Leave as is, or move them to `#[ignore]` with a run command so the report does not count them as coverage. |
| `validate_command` blocks the substring "rm" anywhere in the command, so `git log --format=...` ("format") and `ls firmware` are refused. This is safe but over-blocks. | `agent/execution.rs` | Match whole words, if anyone needs those commands. |

The frontend has no `skip`, `todo`, or `only`. The one test without an `expect` (`renders the running server endpoint`) awaits `findByText`, which fails when the text is missing, so it is a real check.

## Method

The scanners, run from the repository root on `git archive` of main:
- each `#[test]` or `#[tokio::test]` body with no `assert`, `panic!`, `expect(`, `unwrap(`, `?;`, or `matches!`;
- `todo!` and `unimplemented!` bodies;
- `#[ignore]` reasons;
- an early `return` after an `if` or `else`;
- frontend `it` and `test` calls with `skip`, `todo`, or `only`, or with no `expect`, `assert`, or `waitFor`.

Every hit was read by hand before it went into this file.
