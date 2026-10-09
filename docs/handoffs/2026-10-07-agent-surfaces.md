# Handoff: agent surfaces (Agent page, Notch Do, Screen Guide's ChatGPT path)

Current as of 2026-10-08. This page is rewritten, not appended to; earlier states are in git history.

## What this lane is

The owner took over three features from Kunj: the Agent page (Hermes chat), Notch Do (voice-driven computer use) and Screen Guide's ChatGPT answer. The owner has delegated product and architecture decisions in this lane: decide after weighing options, record as "Owner" in an ADR and `docs/team/decision-log.md`, then build.

Read in this order:

1. `docs/agent.md`: what the Agent page is and every command it calls.
2. `docs/decisions/024-agent-surfaces-egress-and-action-policy.md`: the rules, and the table of decisions E1 to E16 with their state.
3. `docs/decisions/026-notch-do-acts-with-fndrs-own-hands.md`: where Notch Do is going.
4. `docs/evidence/W03/agent-surfaces-phase1.md`: what was confirmed, including the live runs.
5. `docs/superpowers/plans/2026-10-07-agent-surfaces-work-breakdown.md`: 184 parts with status lines; use it as a lookup, not a reading.

## State

Everything below is on `main` and pushed to both remotes.

| Area | State | Verified how |
| --- | --- | --- |
| Hermes tool limit (planning list and read-only memory search only) | Built | Unit tests, and a live gateway on 2026-10-07 |
| Related memories to cloud providers off by default; disclosure in setup; chips in chat | Built | Unit and front-end tests |
| Agent Stop, failed sends kept, owner-only chat file, history bound (200 chats, 400 messages) | Built | Unit tests. Stop is not checked in the running app |
| Privacy Activity: persistent, per feature, kinds of content, recent Notch Do runs | Built | Unit and front-end tests |
| Notch Do policy: planning does not act, tap-only approval, browsers ask by default, blocklist, halts | Built | Unit tests, fake Codex server, and live runs on six fixture cases on 2026-10-08 |
| Notch Do on the installed Codex and the account's model | Built 2026-10-08 | Live |
| One no covers the rest of a step; a finished run says what it left out | Built 2026-10-08 | Fake Codex server test, and live |
| Screen Guide ChatGPT answer: logged, private screenshot staging, falls back to this Mac | Built | Unit tests only. No live ChatGPT answer through the app since the Codex flag fix |
| Dead Agent panels, their eight backend commands, `agent_runner.py` | Removed | Build and tests |

## Decisions in force

- ADR-018, accepted 2026-10-08: a cloud model only on a turn the person starts, on a surface where they chose the provider. No background task ever calls one.
- ADR 026: FNDR never attaches a tool that runs model-written code. Beta runs Notch Do only where `open-computer-use` is installed. For Final, FNDR serves the computer-use tools itself.
- Agent and Notch Do stay in Labs for Beta (ADR 024, E12). The Beta demo does not depend on Notch Do.

## Known and open

1. **The helper sends a picture.** With Screen Recording granted, `open-computer-use` attaches a PNG of the operated app's window to every read. FNDR discloses and logs it and cannot stop it. Only the ADR 026 executor removes it.
2. **"This page" now resolves to the app in front** (the planner is told its name). Checked live at the planning step only.
3. **A read-only request now shows what was reported**, marked "Reported:". Unit test only; the live operate step could not read a Chrome window that evening.
4. **Not checked in the running app:** quit and Stop during a Notch Do run, Agent Stop, quit with the Hermes gateway up, a Screen Guide ChatGPT answer.
5. **Not measured:** the twenty-task set (`docs/evidence/W03/notch-do-task-set.md`), Agent latency, Spotify or Music playback.
6. **Waiting on others:** Notch Do in the SK-01 journal (E11).
7. The Tauri commands in `ipc/commands/agent.rs` have no page calling them. They are recorded in `src/shared/ipc/ipcDriftBaseline.ts`; another session is working in `src-tauri/src/agent/`, so they were left.

## Next, in order

1. Recheck item 3 live, and stop the address bar counting as a page's search box (evidence file, last section).
2. The ADR 026 executor. Start with `get_app_state` and `click` against `src-tauri/tests/fixtures/operator/pages/`; keep the tool names `operator/policy.rs` already classifies.
3. The twenty-task set on the executor.

## How to run a live Notch Do check

Needs a signed-in Codex and `open-computer-use` with Accessibility granted. It operates real apps and spends requests on the signed-in ChatGPT plan. The harness starts every plan and answers no to every question.

```bash
open src-tauri/tests/fixtures/operator/pages/shop.html
cd src-tauri && FNDR_LIVE_REQUEST="in Google Chrome, add this to my cart on the page that is open" \
  cargo test --lib live_notch_do -- --ignored --nocapture
```

`FNDR_CODEX_TRACE=1` prints what Codex sends. The journal path is printed at the end of the run.

## What went wrong in this lane, and the correction

- **A check that only passes offline hides breakage.** Codex dropped a feature flag and every Notch Do and Screen Guide ChatGPT turn died at launch; unit tests with a fake server stayed green, and a fallback hid it in the app. Correction: FNDR now asks the installed Codex what it supports, and an exit carries its reason. When a feature depends on an outside program, run it for real before calling it done.
- **Dead code was found by hand.** Correction: `src/shared/ipc/ipcDrift.test.ts` and `src/dev/docsDrift.test.ts` now fail on a new dead command or a doc that points at something gone.
- **A push looked done and was not.** `origin` pushes to GitLab and GitHub; GitHub's "Everything up-to-date" hid GitLab's rejection. Correction: confirm with `git ls-remote origin main`.
- **This page grew by stacking updates** until its top contradicted its middle. Correction: rewrite it.

## Shared checkout

Other sessions work in this checkout at the same time (MCP, `src-tauri/src/agent/`, storage tests). Commit by path. Where a file holds someone else's uncommitted lines too, stage only your own hunks. Never stash or switch branches without the owner saying so.
