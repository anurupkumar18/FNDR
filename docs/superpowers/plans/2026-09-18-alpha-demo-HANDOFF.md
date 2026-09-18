# HANDOFF — FNDR 1.0 Alpha demo hardening (2026-09-18, ~08:00 MDT)

Paste this whole file into Codex/ChatGPT to continue. The demo is **today, 1–3 PM**. Code freezes: **no Rust changes after 11:00, no UI changes after 12:15**, except blocker fixes.

## Where things are

### Continuation update (08:01 MDT)

- `6cb6a1d` is pushed to `alpha-demo` and `main`: Settings is now one demo-safe sheet (Profile, Capture, Privacy/blocklist, read-only Local Models). Danger Zone, updater, retention, auto-fill, MCP, model mutation, and appearance picker UI were removed.
- `bd885b2` is pushed: Memory cards no longer expose FRAME ids, empty placeholder rows, or raw synthesis branch identifiers.
- `16f3d2a` is pushed: Vault has a separate **Needs more signal (N)** review queue. It exposes only app, timestamp, and a plain-language reason; normal Search/Vault/Ask still exclude those records.
- Current verification: `npm run typecheck` and `npm test` are green (34 files, 183 tests). The demo app is running from `./scripts/demo/run-demo.sh`; do not stop it for a Rust-inclusive gate unless the presenter is ready to restart it.

Remaining product work is now Home polish (T10) and optional visual cleanup. The remaining mandatory work is human-operated: Screen Recording permission/live capture, warm Ask FNDR latency, refusal question, and a complete rehearsal. Do not repeat the completed Settings, card, or needs-signal tasks.

- Repo: `/Users/anurupkumar/FNDR` (GitLab `git@capstone.cs.utah.edu:fndr/fndr.git`). **Do not touch `/Users/anurupkumar/FNDR-2.0`.**
- All work below is committed and pushed to **`main`** (fast-forward from branch `alpha-demo`, which has the same head).
- Full plan with code for every remaining task: `docs/superpowers/plans/2026-09-18-alpha-demo-hardening.md`. The binding owner decisions are D1–D23 at the top of that file.
- Progress ledger: `docs/superpowers/plans/2026-09-18-alpha-demo-progress.md`.
- The app may still be running on the demo profile (`npm run tauri dev` with `FNDR_DATA_DIR`). To relaunch: `./scripts/demo/run-demo.sh`. The real-profile app is plain `npm run tauri dev`.

## Done and verified (tests green: `make test` → vitest 180, cargo lib 635 + all integration suites; typecheck clean)

| Task | What | Commit |
|---|---|---|
| T0 | Branch from teammates' main (Felipe's 3 commits kept) + cherry-picked 0465dac | 7411402, cb9846c |
| T1 | `memory_quality::low_signal_reason` / `result_low_signal_reason` / `record_low_signal_reason` / `partition_surfaceable`: image-only, filename-only, visual-fallback, unreadable and quarantined captures are classified as low-signal. Tests: 8 unit + `tests/low_signal_surface.rs` | eec69a9 |
| T2 | The policy is applied in `search_memory_cards`, `list_memory_cards`, and `fndr_answer` (`context_runtime::drop_low_signal_hits`). New IPC `list_needs_signal_memory_cards` + TS `listNeedsSignalMemoryCards()` / `NeedsSignalCard` | 4eacc5f |
| T3 | `config::fndr_app_data_dir()` honors `FNDR_DATA_DIR`, and all 14 app-data-dir call sites use it. `FNDR_DEMO_REVIEW_BACKFILL=1` queues on-device review of all memories at startup. The Omnibar (Alt+Space) and Auto-Fill windows and hotkeys are no longer registered | fe5ca48 |
| T4 | Seeded demo week: `scripts/demo/demo-week.json` (82 entries, days −7…−1, 3 intentionally low-signal), `src-tauri/examples/seed_demo.rs` (asserts exactly 3 needs-signal), `scripts/demo/seed-demo-profile.sh [--reset]`, `scripts/demo/run-demo.sh`. Profile is at `~/Library/Application Support/com.fndr.app.demo` (models symlinked, biometric off). **The real `com.fndr.app` profile is untouched** | e20c100 |
| T5 | `src/domains/ask/AskPanel.tsx` (+css, +3 tests): Ask FNDR on `fndrAnswer` with a grounded answer, clickable sources, the honest refusal "Not in your memories", and a 60s timeout | 67b12a5 |
| T6 | Curated surface. Sidebar: Home · Memory Vault · Ask FNDR · Daily Summary · FNDR Wrapped · Screen Guide · Cmd+K. Palette `DEMO_COMMAND_IDS` = go-home, memory-cards, ask-fndr, daily-summary, wrapped, screen-guide, pause/resume. Engine Metrics, Context pack, Research, Automation, Quick Skills, Glasses and Pipeline are removed from nav and palette. `AppPanels.tsx` mounts only Ask/Vault/DailySummary/Wrapped/ScreenGuide | ba85953 |

Verified live: the app boots on the demo profile (log shows `FNDR_DATA_DIR override active` and `demo review backfill queued`), and Home renders with no Touch ID lock.

## Remaining, in priority order (the full code for each is in the plan file)

1. **T9 Settings: one clean sheet (next, highest visible risk: the Danger Zone and model Delete buttons are still visible).** In `src/domains/workspace/ControlPanel.tsx`, remove the tab `<nav>` and render Profile → Capture (pause/resume + "Stored · Skipped · Needs signal") → Privacy (the existing `<PrivacyPanel embedded>` + the Blocked Apps & Sites block) → Local models (read-only ✓ list of downloaded models, no buttons). Delete the Updates, Indexing, Auto-Fill, MCP, model manage and Danger Zone JSX, **plus all now-unused state/handlers/imports** (TS `noUnusedLocals` is on; a previous attempt listed ~35 of them). Update `ControlPanel.test.tsx`: drop the tab clicks and delete the two Updates tests (see the plan, Task 9).
   - A lower-effort fallback if time is short: keep the tabs, but delete only the Danger Zone section, the model Delete/Download buttons, and the Updates/Indexing/Auto-Fill/MCP sections.
2. **T8 Clean cards.** `InsightLayers.tsx`: hide empty rows and the snake_case branch chip. `MemoryCard.tsx`: chip labels REVIEWED / AWAITING REVIEW / CAPTURED / REVIEW INCOMPLETE / UNREADABLE, remove "FRAME XXXX", skip the preview line when it's ≥80% token-overlap with the title (`tokenOverlap` in `src/shared/utils/cardCleanup`). `ExpandedMemoryCard.tsx`: remove the "0 nodes · 0 edges" subgraph, and put evidence in a `<details>` "How FNDR built this". `Timeline.tsx`: remove the Delete button from search results and skip duplicate title/summary. Hide the voice "Speak" buttons (`VOICE_INPUT_ENABLED=false` in `src/shared/utils/config.ts`, used in `HomeHero.tsx` + `SearchBar.tsx`). Delete the "SCROLL TO EXPLORE" indicator in `HomeHero.tsx`. Update `__tests__/MemoryCard.lifecycle.test.tsx` labels.
3. **T7 Vault "Needs more signal (N)" toggle** in `MemoryCardsPanel.tsx` using `listNeedsSignalMemoryCards(200)`. Show app, time and reason only (never the title, which can hold filenames). Change "N cards" → "N memories".
4. **T10 Home.** New `src/app/HomeRecent.tsx`: capture chip (`getStats().today_count`) + a 5-tile Recent row from `listMemoryCards(5, null)` that opens `handleOpenMemoryById`. Mount it under `<HomeHero>` in `App.tsx`. Lighten `.sidebar-scrim` in `src/app/styles/App.css`.
5. Tier 2, only if time allows:
   - T11: theme toggle + static background (replace `MotionWallpaper` in `AppShell.tsx`).
   - T12: QA teammates' Daily Summary / Wrapped / Screen Guide on the demo profile. Keep them visible even if flaky.
   - T13: dead-code deletion (AgentPanel, Automation, QuickSkills, Research, EngineMetrics*, PipelineInspector, GlassesImport, AutofillOverlay + `autofill.html`, `hermes_agent.rs`, `glasses_import.rs`). This is invisible to judges, so skip it before the demo.

After each task, run `npm run typecheck && npx vitest run <touched dirs>`, then commit and push. For Rust, stop the app first, then `cd src-tauri && CARGO_BUILD_JOBS=2 cargo test --lib <module>`.

## Gotchas learned tonight

- `cargo fmt` reformats the **whole crate**, including teammates' files. Use `rustfmt --edition 2021 <file>` only on files you own, and never on `stats.rs` or teammate files.
- `Store::new` calls `block_on`, so tests must use `#[test]` + `tokio::runtime::Runtime::new()`, not `#[tokio::test]`.
- The main-window registration is `ipc::commands::search::list_memory_cards` (namespaced) in `main.rs`.
- GitLab SSH blipped once. Retry with `GIT_SSH_COMMAND="ssh -o ConnectTimeout=8" git push`.
- Self-QA of the native window without stealing focus: find the FNDR window id with a `swift -e 'import CoreGraphics; …CGWindowListCopyWindowInfo…'` one-liner (the main window is 900×700), then `screencapture -x -o -l<id> /tmp/x.png`.
- Kill only FNDR processes: `pkill -f "cargo-target-shared/debug/fndr"; pkill -f "FNDR/node_modules/.bin/tauri dev"; pkill -f "FNDR/node_modules/.bin/vite --host 127.0.0.1 --port 1420"`. There's an unrelated Vite on port 5173.

## Demo script (D23) and expected results

1. **Home:** greeting, and no lock screen.
2. **Privacy:** Settings → pause/resume, then add `Messages` to the blocklist.
3. **Live capture:** open https://en.wikipedia.org/wiki/Retrieval-augmented_generation for ~20s; it gets captured.
4. **Search:** `E0502` returns the Tuesday Cursor capture-loop error. `that rust borrowing error` returns the same memory.
5. **Ask FNDR:**
   - `What chunk size did the chunking paper recommend?` → 512-token parents / 128-token children, citing the Preview PDF + design doc.
   - `What's my bank balance?` → "Not in your memories". The seed has no finance content.
6. **Vault:** "Needs more signal (3)" (after T7). The three are Preview, ChatGPT and Photos.
7. **Daily Summary** (yesterday), then **FNDR Wrapped** (this week).
8. **Screen Guide.**

Say verbally that the week is seeded demo data plus today's live capture (D20). Judge takeaway (D19): "FNDR remembers what you did on your Mac, privately and on-device, and answers questions about it with cited evidence."

## Unverified / needs a human

- Live capture needs Screen Recording permission for the dev binary (a rebuild may re-prompt).
- Ask FNDR latency on 8 GB with Qwen loaded hasn't been measured. Warm it up with one question before presenting.
- Whether the review backfill finished (cards show REVIEWED vs AWAITING REVIEW). Both are honest.
- The refusal question hasn't been tested live.
