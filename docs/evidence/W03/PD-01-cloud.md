# PD-01: ADR-018 reasoning tier (cloud draft)

Date: 2026-10-04. Branch `claude/train-e-docs`. Status of the ADR: **Proposed, not accepted.**

## What was produced

- `docs/decisions/018-reasoning-tier.md`:
  - "What is true today": a code read showing three paths where text can already leave the Mac (Screen Guide with the ChatGPT model, Hermes Agent with a cloud provider, MCP clients), where their credentials live, and that none is fully counted in Privacy Activity.
  - Three options (local only; opt-in cloud per task with the person's own key; cloud by default), each with what leaves, how it is logged, key storage (macOS Keychain for B), how quality is measured, and risks.
  - A placeholder task table for PD-08 (seven tasks, input column filled from `docs/product/llm-task-catalog.md`, the rest "PD-08").
  - A measurement plan using sets that exist (gold v0, the 22-query seeded set, 30 synthetic screens) and naming what does not (`make eval`, the 50-utterance command script).
  - A recommendation (B staged, with A as the honest Beta fallback) and the exact one-line edits to make on acceptance. The master plan and `TEAM.md` constraints were not edited.
- `docs/team/decision-log.md`: ADR-018 stays under "Open" with a link to the draft.

## Findings worth the owner's attention

- Screen Guide's ChatGPT path and Hermes Agent cloud providers already send screen-derived or memory-derived text when a person selects them. Neither call is counted by `record_egress`, so Privacy Activity would read zero cloud requests even when they happened.
- Both existing cloud credentials are files (`$CODEX_HOME/auth.json`, `hermes-home/.env`), not Keychain items.
- The Makefile has no `make eval` target, though the master plan and gold set README refer to it.

## What remains for a human

- Kunj: PD-08's table (link it from the ADR's placeholder section).
- Owner: choose A, B, or C, or edit the draft; then apply the listed edits and add the decision-log row. The ticket's "Done when" date (Sep 28) has passed.

## How to verify

- Each "What is true today" row cites a file; open it and check the named function or constant (`ScreenGuideModel` and `send_screenshot_to_codex` in `src-tauri/src/config.rs`, `answer_screen_guide_with_codex` in `codex_account.rs`, `hermes_env_path` in `hermes_agent.rs`, `record_egress` call sites via `grep -rn record_egress src-tauri/src`).
- `git diff origin/main -- src-tauri src Makefile` is empty for this commit: no code was wired.
- `grep -nP '[\x{2013}\x{2014}]' docs/decisions/018-reasoning-tier.md` prints nothing.
