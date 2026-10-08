# Agent surfaces: work breakdown

Date: 2026-10-07. Status: planning draft for owner review. Nothing here is scheduled yet.

Read with the [PRD](../specs/2026-10-07-agent-surfaces-prd.md) and
[ADR 024](../../decisions/024-agent-surfaces-egress-and-action-policy.md). This
file replaces the PRD's short issue table. It splits Hermes Agent, Screen
Guide, the notch (Ask and Do) and the parts they share into small pieces, so
each can be opened on its own when its turn comes.

Written from the code on `main` at `81b6cbd`. The app was not run.

## How to read a part

```
### <ID> Title
- kind: qa | fix | decide | doc | measure | cleanup      size: S (under 2h) | M (half day) | L (over a day)
- needs: other parts or decisions that must come first
- Found: what the code does today, with the file. (C) confirmed by reading. (V) needs a run to confirm.
- Do: the steps.
- Done when: a check anyone can run.
```

A part is small on purpose. When a part's turn comes, start by re-reading its
"Found" lines against the code, because the tree is moving.

## Counts

| Track | Parts |
| --- | --- |
| 0. Ground work | 7 |
| A. Hermes Agent | 65 |
| B. Screen Guide | 30 |
| C. Notch (Ask and Do) | 59 |
| D. Shared rules | 14 |
| E. Decisions | 14 |
| F. Measurement and long term | 9 |

## Phase 1 results (2026-10-07)

The unverified findings were checked; details and tables are in
[the phase 1 evidence](../../evidence/W03/agent-surfaces-phase1.md).

| Finding | Result |
| --- | --- |
| N1 planning turn can act | Confirmed on FNDR's side by a test (see Phase 2); fixed. One live run would show whether the model ever asked |
| N2 spoken approval | Confirmed by an executed probe; short phrases that merely begin with "ok" or "sure" approve |
| N4 browser clicks and keys | Confirmed by an executed probe |
| N5 links | Confirmed by an executed probe |
| N6 gateway survives quit | Confirmed by code; `ps` after a real quit still to do |
| N6 Notch Do run survives quit | Open |
| N7 Sign out is Mac-wide | Confirmed by configuration; logout not executed |
| N7 forced file credential store | Not supported on this Mac; downgraded to a note |
| N14 typing and focus | Confirmed on FNDR's side by an executed probe, and worse: a fresh screen read does not correct it |
| A9.1 Hermes tools | Confirmed for the Hermes this Mac would run: terminal, file write, code execution, browser control, cron |
| C8.9 stale index | Half confirmed; needs a live run |

New from phase 1: P1 FNDR runs any system Hermes when its pinned copy is missing (A1.4). P2 spoken Stop needs the stop word first; "please stop now" is not a stop (C10.1). P3 and P5 feed C8.6. P4 `open_app` runs for Terminal, Script Editor, Shortcuts and FNDR itself (C8.1).

Part 0.6 baseline: 118 front-end tests pass across the three features; 14 policy tests pass in an isolated copy; repo Rust tests not run because another session was building.

## Phase 2 progress (2026-10-07)

First batch of fixes that need no decision. Uncommitted on `main`.

| Part | What changed | Check |
| --- | --- | --- |
| C6.3 | A planning turn is refused every tool request | `a_planning_turn_cannot_use_tools`; fails with the fix removed, which also confirms N1 on FNDR's side |
| C12.2, C12.3 | Apps on the blocklist, and FNDR itself, are never read or operated; a plan step naming one fails | `an_off_limits_app_is_never_read_or_operated`; fails with the fix removed |
| C8.10, C12.1 | Actions switched off, or Private Mode, refuses a new plan and ends a run at its next action or step | `a_halted_run_refuses_the_next_action` |
| C4.4 | An approval FNDR cannot read ends the run with the cause | `an_unreadable_action_ends_the_turn_with_the_cause` |
| C10.4, A5.5 | The exit handler stops any Notch Do run and the Hermes gateway; a run that ends in an error kills its process group | Compiles; needs the `ps` check after a real quit |
| C10.1 (P2) | "Please stop now", "ok stop", "no wait" stop a run | `doRun.test.ts` |
| B7.1, D1.3 | Screen Guide's ChatGPT request is logged; rows say when memories, on-screen text or a screenshot went along | `privacy_proof` and `PrivacyProof.test.tsx` |
| D1.1 | The log is kept in app data (`0600`, 200 rows, 30 days) and restored at start | `saved_rows_survive_a_restart_within_the_age_and_count_bounds` |
| A10.1 | Hermes `config.yaml`, `.env` and setup record are `0600`, the folder `0700`, including files from older builds | `secret_files_are_readable_by_the_owner_only_even_when_they_already_exist` |
| B7.3 | The staged screenshot is `0600` in a `0700` folder; leftovers are removed at start; Screen Guide FR7 amended | `a_staged_screenshot_is_private_and_a_leftover_one_is_swept` |
| C4.3 | `@openai/codex` pinned to `0.151.0`, the version on the development Mac | `setup_center` test |
| A4.1 | Header chip: "Set up a model", "Choose a model", "Reconnect ChatGPT", "Hermes not installed", or the saved model | `AgentWorkspace.test.tsx` |
| A3.4 | Usage card says the limits cover the whole account and when they were read | `CodexAccountCard.test.tsx` |
| A11.4 | A failed send puts the text and attachments back | `AgentWorkspace.test.tsx` |

Second batch, same day:

| Part | What changed | Check |
| --- | --- | --- |
| C12.1, C8.10 | A run in progress rechecks the kill switch and Private Mode every half second and ends at once, killing its process group | Compiles; not covered by a test (needs app state) |
| A11.3 | Stop button on the Agent page: the composer returns at once with the message, a late reply is ignored, the backend stops waiting | `AgentWorkspace.test.tsx`; the backend half is not covered by a test |
| A8.3 | A failed send is kept in chat history and shown as "Not sent" | `a_failed_send_is_kept_in_a_file_only_the_owner_can_read`, `AgentWorkspace.test.tsx` |
| A8.1 | Chat history has one writer at a time and is `0600` | Same Rust test |
| A5.4 | The gateway's last 40 error lines are held in memory; a failed start reports the last one | `a_failed_gateway_start_reports_its_last_output_bounded` |
| A5.5 | A gateway left by a crash is stopped at the next start, only if the saved process id is still a Hermes gateway | `only_a_hermes_gateway_process_counts_as_a_stale_gateway`; the kill itself needs a real run |
| D1.4 | Callers name their feature with a type, not a string | Compiles; existing log tests |

Results: 77 Rust tests pass for the touched modules and 221 front-end tests pass; typecheck is clean; the app and library compile in the shared tree. The Rust tests were run in a throwaway copy of `src-tauri`, because the shared tree's test build is broken by another session's unfinished MCP changes (`mcp/mod.rs`, `mcp/remember_http_tests.rs`). Rerun them in the repo once that lands.

Left out on purpose:

- **A cap on chat history (the other half of A8.1).** Deleting old chats is a retention choice, which belongs with PD-09 and E-level decisions, not a no-decision fix.
- **A gateway log file on disk (A5.4 as first written).** Hermes's own output may contain message text, so it is kept in memory and only the last line is shown.

Known limits: the blocklist check matches the app name or bundle id the model passes, with the same loose matching capture uses; Privacy Activity counts Screen Guide's text bytes, not the image size; stopping an Agent reply ends FNDR's wait, and Hermes may finish the turn on its own side; a retried message that failed first appears twice in history, once as "Not sent".

The no-decision list is complete. What remains needs a live run (the six checks in the phase 1 evidence) or a decision (Track E).

## Phase 4 progress (2026-10-07)

The owner took every recommendation in Track E; ADR 024 is accepted. Built the same day:

| Part | What changed | Check |
| --- | --- | --- |
| C9.2 (E5) | Speech never approves an action; it can decline or stop | `doRun.test.ts` |
| C7.2 (E3) | The plan card counts down only when no step can need a yes; otherwise it waits for Start or "go" | `a_plan_starts_by_itself_only_when_no_step_can_need_a_yes`, `NotchHud.test.tsx` |
| C8.3, C8.4, C8.7 (E7) | Browsers ask by default; run list is links, tabs, search boxes, play and pause, navigation keys; unreadable labels are unknown | Five new tests in `operator/policy.rs` |
| C8.2 (E7) | A planned link opens unasked only when the person's words account for it; otherwise an approval card | `only_links_the_persons_words_account_for_open_without_asking` |
| C8.6 | Focus is forgotten after a key press, a click by position, or a screen reading that no longer shows the same element | `typing_runs_only_while_fndr_knows_the_search_box_has_focus` |
| C8.1 | Shells, Script Editor, Shortcuts, Automator, Wallet, Disk Utility and Activity Monitor are never opened or operated | `apps_that_run_commands_or_move_money_are_never_opened` |
| C3.2 | The operate switch's copy and comment describe the tiers as they are | Read |
| A6.2, A9.5 (E2) | Related memories and memory search for Hermes only for a provider on this Mac, or when turned on | `memories_fndr_finds_itself_go_to_a_cloud_provider_only_when_turned_on` |
| A2.6, A6.3 | Agent setup says what is sent and where before Save; the page subtitle matches | `AgentWorkspace.test.tsx` |
| A6.4 | Memories FNDR added are returned with the reply, shown on the message and kept in history | `AgentWorkspace.test.tsx` |
| A9.3 (E4) | Hermes's API server is limited to a planning list | `hermes_is_given_a_planning_list_and_nothing_that_acts`; the effect on a running gateway needs a live check |
| A9.4 | Hermes's MCP block lists four read-only memory tools | `hermes_codex` test; needs a live check that a write tool is refused |
| A1.4 (P1) | Only the pinned Hermes runs; a Hermes elsewhere on the Mac is ignored | Compiles; on a Mac without the pinned copy the page now asks to install it |
| A1.3 (E9) | The Hermes update control and its commands are removed | `SetupCenter.test.tsx` |
| A3.6 (E8) | Sign out says it signs Codex out on this Mac too | Read |

Results: 92 Rust tests pass for the touched modules (in a copy of `src-tauri`, for the same reason as before) and 229 front-end tests pass; typecheck is clean; the app and library compile in the shared tree.

Decisions taken but not built yet: E6 (the transcript reaching the cloud before it is seen), E10 (OpenClicky bridge), E11 (SK-01 journal, waits for SK-01), E12 (Labs placement), plus C3.1 (moving the operate switch out of Screen Guide's config) and A6.7 and A6.8 (Hermes's instruction strings, which still mention a snapshot that no longer exists).

To verify on a real run: that Hermes 0.18 at the pinned commit honors `platform_toolsets.api_server` and the MCP `tools.include` list the way 0.13 does (read from 0.13's source, the only complete copy on this Mac); that Notch Do still completes the Spotify and web-search example with the tighter browser rules.

## Phase 5 progress (2026-10-07)

Commit `f45faa0`. 116 Rust tests pass, run in the repo now that the shared test build is fixed; 260 front-end tests pass.

| Part | What changed | Check |
| --- | --- | --- |
| C6.1 (E6) | The notch shows what it heard for 1.2 s before sending it to be planned; Cancel keeps it on the Mac; typed requests skip the wait | `NotchHud.test.tsx`, `doRun.test.ts` |
| C3.1 | "Operate my Mac" is stored under `operator.enabled`, read once from the old Screen Guide key; `set_computer_use_enabled` turns it on or off and ends a run when turned off | `the_operate_opt_in_moves_out_of_screen_guide_and_keeps_its_value`; the setter itself has no test |
| A6.7, A6.8 | Hermes's instruction strings live in `inference/prompts.rs` with fingerprints and catalog rows, and describe what Hermes can do now | `inference::prompts` tests |
| E12 | Sidebar groups: Trust (Privacy Activity) and Labs (Hermes Agent, Screen Guide, Engine diagnostics). The full five destinations remain PX-01 | App tests |
| E10 | The OpenClicky and Operate switches carry a Labs tag | Read |

Lesson for shared files: staging by hunk with zero context put three type fields in the wrong interfaces in `ed175dd`; `f45faa0` corrects them. Stage a shared file whole once it holds only this lane's changes, or use hunks with context.

Still open: E11 (waits for SK-01), tests for the half-second halt, backend Stop, the stale-gateway kill and the operate setter, the fixtures and task set, and the six live checks (owner approved 2026-10-07).

## Findings added since the PRD

The second pass found these. They are not in the PRD yet and several are more
serious than what is.

| # | Finding | Where | Part |
| --- | --- | --- | --- |
| N1 | Notch Do's planning turn runs on a session that already has the computer-use tools, and run-tier calls are approved in that turn too. Screen reading (and clicks the policy allows) can happen before the plan card is shown. (V) | `computer_use.rs` `Session::open`, `run_turn`, `run_with_snippets` | C6.3 |
| N2 | Notch Do accepts a spoken "yes", "ok" or "sure" as approval for a confirm-tier action, with the microphone open during the run. The command-surface contract says spoken approval is not accepted. (C) | `doRun.ts:200-237`, `NotchOperator.tsx` | C9.2 |
| N3 | Notch Do checks Private Mode once, at plan time, and never checks the app blocklist. A blocklisted app's on-screen text can be read and sent. (C) | `computer_use.rs:1086-1092` | C12.1, C12.2 |
| N4 | In browsers and media apps a click on any element whose label is not on two short English word lists runs without asking. "Add to cart", "Apply", "Book", "Sign out", "Follow", "Block", icon-only buttons and all non-English labels fall through. (C for the rule, V for each case) | `operator/policy.rs` `classify` | C8.3, C8.4 |
| N5 | `open_url` runs for any http or https link. A model steered by on-screen text can put memory or screen text in a query string, or open a one-click action link. (C for the rule) | `operator/policy.rs`, `attempt_step` | C8.2 |
| N6 | Quitting FNDR does not stop the Hermes gateway or an active Notch Do run. The exit handler stops Screen Guide, voice and speech only. (C for the handler, V for the orphan) | `main.rs:945-953` | A5.5, C10.4 |
| N7 | "Sign out" on the account card logs the person out of Codex for the whole Mac, because FNDR uses the real `~/.codex`. Sign-in also forces file credential storage there. (C for the home dir, V for the effect) | `codex_account.rs:95, 486, 28` | A3.6, A3.7 |
| N8 | With ChatGPT and the screenshot switch on, Screen Guide writes the screenshot to a file in the system temp folder before sending. ADR-004 and Screen Guide FR7 say normal-turn pixels stay in memory. (C) | `codex_account.rs` `answer_screen_guide_with_codex` | B7.3 |
| N9 | `AgentPanel.tsx` (1,837 lines) and `ResearchPanel.tsx` are imported nowhere. `start_agent_task` launches a Python sidecar and has no live caller. (C) | `src/domains/workspace/`, `hermes_agent.rs:1733` | A12.1, A12.2 |
| N10 | Agent chat history is one JSON file with no cap, no lock and default permissions. A failed send is not recorded, and the draft text is cleared before the reply arrives. (C) | `agent_chats.rs`, `AgentWorkspace.tsx` `send` | A8.x, A11.4 |
| N11 | The memories FNDR adds automatically are not shown in the thread or saved in history, so the person cannot see what was sent. (C) | `hermes_agent.rs:1612`, `AgentWorkspace.tsx` | A6.4 |
| N12 | The operate switch's stored comment says every action needs approval; the shipped policy is tiered. (C) | `config.rs:624-627` | C3.2 |
| N13 | Notch Do reads the tool name out of the wording of Codex's approval message. A Codex update that changes the wording makes every call "unknown", which is refused. Safe, but the feature stops working with no clear cause. (C) | `computer_use.rs` `tool_from_approval_message` | C4.4 |
| N14 | Typing is classified by the element FNDR last saw clicked. If focus moved another way (Tab, a page's autofocus), text judged "search field" can land in a different field. (V) | `operator/policy.rs` `focused` | C8.6 |

---

# Track 0. Ground work

Do these once. Everything else depends on them.

### 0.1 A build that is safe to run in a shared checkout
- kind: qa   size: S
- Found: `main` has another session's uncommitted changes in MCP, inference and app files. (C)
- Do: confirm with the owner which tree to build; record the commit and whether the working tree was clean; run `npm run typecheck` and the focused tests listed in 0.6 before any native run.
- Done when: a one-line build record exists at the top of the evidence folder.

### 0.2 A synthetic profile
- kind: qa   size: M
- Do: a separate app-data directory with seeded, invented memories (reuse the seeded retrieval fixture under `docs/evidence/W02/`); no real captures; Private Mode off; blocklist with two known apps.
- Done when: the app starts on the profile and Search returns seeded results only.

### 0.3 Fixture screens
- kind: qa   size: M
- Do: ten static pages or documents to point Screen Guide and Notch Do at: a form with a password field, a page with a "Buy now" button, a page with injected instruction text, a spreadsheet total, an error dialog, a page in a second language, a page with icon-only buttons, a media player, a blocklisted app window, an empty desktop. GS-10 and GS-12 already ask for fixtures; build one set and share it.
- Done when: the set is in the repo under a fixtures folder with a one-line purpose each.

### 0.4 Evidence template and redaction rule
- kind: doc   size: S
- Do: one template (build, profile, steps, expected, observed, recording link, verdict); rule that the account email, tokens, chat content and real screen text never enter the repo. The takeover screenshot shows an email and must be cropped.
- Done when: the template is in `docs/evidence/W03/`.

### 0.5 Accounts and installs needed for native QA
- kind: decide   size: S
- Found: ChatGPT paths need a signed-in account and count against its limits; Notch Do needs Computer Use or `open-computer-use`; Hermes needs a 2 to 5 minute install. (C)
- Do: owner chooses which account is used for QA and accepts that its limits are spent. Entering credentials is done by the owner, never by an agent.
- Done when: the choice is written in the evidence folder.

### 0.6 Focused test commands, verified once
- kind: qa   size: S
- Do: run and record pass or fail for: `npx vitest run src/domains/workspace/AgentWorkspace.test.tsx src/domains/workspace/CodexAccountCard.test.tsx src/domains/notch src/domains/screen-guide`; then serial Rust: `cd src-tauri && CARGO_BUILD_JOBS=1 cargo test --lib operator::`, `ipc::commands::computer_use`, `ipc::commands::hermes_`, `ipc::commands::codex_account`, `privacy_proof`. Watch memory pressure; the pre-tool hook blocks when it is critical.
- Done when: the table of commands and results is saved. This is the baseline every later part compares to.

### 0.7 Test inventory by behavior
- kind: doc   size: M
- Do: for each of the three features list what the existing tests prove and what no test covers (for example `turns_dispatch_tool_calls_through_the_policy` proves dispatch; nothing proves a planning turn cannot act). Feeds QT-01.
- Done when: a gap list exists per feature.

---

# Track A. Hermes Agent

Files: `src/domains/workspace/AgentWorkspace.tsx`, `CodexAccountCard.tsx`,
`src-tauri/src/ipc/commands/hermes_agent.rs`, `hermes_codex.rs`,
`codex_account.rs`, `agent_chats.rs`, `setup_center.rs`, `operator/memory.rs`.

## A1. Install and runtime

### A1.1 First install on a clean Mac
- kind: qa   size: M   needs: 0.2
- Found: FNDR clones Hermes at pinned commit `b8880f1` from GitHub and builds a private Python runtime with `uv` in app data. (C)
- Do: time the install on a clean profile; record disk used, network hosts contacted, and what the page shows while it runs; pull the network cable halfway once.
- Done when: duration, size, hosts, and the interrupted-install result are recorded.

### A1.2 Install failure states
- kind: qa   size: S
- Do: no network, no `git`, disk nearly full, `uv` download blocked. Record the message for each and whether Retry works.
- Done when: each case has a message that names the fix, or a fix part is opened.

### A1.3 The update path leaves the pin
- kind: decide   size: S
- Found: Setup Center can call `update_hermes`, which checks out the newest upstream release tag. The Codex import path "was read at this commit" only. (C) `hermes_codex.rs:21, 229-262`
- Do: decide whether updating past the pin is allowed without a review of the import path and default tools.
- Done when: the button is either removed, gated behind a reviewed version list, or kept with a written reason.

### A1.4 System Hermes versus FNDR's Hermes
- kind: qa   size: S
- Found: `detect_hermes_runtime` can also find a Hermes on the PATH. (C)
- Do: with a system Hermes of a different version installed, see which one runs and whether its own config or tools leak in.
- Done when: the launcher choice is recorded and, if wrong, a fix part exists.

### A1.5 Resource cost of the gateway
- kind: measure   size: S
- Do: memory and CPU of the gateway process idle and during a message, on an 8 GB machine if one is available. `docs/agent.md` promises "8GB-safe defaults".
- Done when: numbers are in the evidence file.

## A2. Provider setup

### A2.1 ChatGPT provider, happy path
- kind: qa   size: S   needs: 0.5
- Do: sign in, pick a model, Save, send one message. Record each visible state and the badge text at each step.
- Done when: a recording and the state list exist.

### A2.2 Ollama provider
- kind: qa   size: S
- Found: `context_length` is fixed at 32768 in the written config whatever the model supports. (C) `hermes_agent.rs:722`
- Do: not installed, installed but stopped, running with no models, running with one small model; a custom Ollama URL on another host.
- Done when: each state is recorded; note whether a non-loopback Ollama URL is treated as local for egress purposes (see D1.2).

### A2.3 OpenRouter provider
- kind: qa   size: S
- Do: wrong key, right key, key with no credit. Reopen setup: can the person tell a key is saved? Change only the model: is the key kept or silently dropped?
- Found: the key field is cleared after Save and `persist_hermes_setup_files` rewrites `.env` from the payload, so saving again with an empty key field would write no key. (V) `hermes_agent.rs:741-800`
- Done when: the re-save behavior is confirmed and filed if it drops the key.

### A2.4 Custom endpoint
- kind: qa   size: S
- Do: bad URL, unreachable host, http on a LAN address, endpoint that needs no key. Record the host that Privacy Activity shows.
- Done when: recorded.

### A2.5 Switching provider with a chat open
- kind: qa   size: S
- Found: Save stops the gateway; conversations live inside Hermes keyed by id. (C)
- Do: start a chat on one provider, switch, continue the same chat.
- Done when: the result (continues, errors, or loses context) is recorded and the intended behavior is decided.

### A2.6 Disclosure before saving a cloud provider
- kind: fix   size: M   needs: E2
- Found: Save writes the config with no statement of what will be sent or where. (C)
- Do: PRD FR3. One short block above Save listing host and payload for the chosen provider.
- Done when: a Vitest case shows the block for each cloud provider and not for loopback Ollama.

## A3. ChatGPT sign-in and the account card

### A3.1 Codex missing, broken, ready
- kind: qa   size: S
- Found: three states from `detect_codex_executable` and the handshake. (C)
- Do: reproduce each; check the install command shown (`npm install -g @openai/codex`) against what Setup Center says ("Install the ChatGPT app, which includes Codex"). They differ.
- Done when: one instruction is chosen and used in both places.

### A3.2 Sign-in flow states
- kind: qa   size: S   needs: 0.5
- Do: start, finish in browser, close the browser tab, cancel, let the 10 minute timeout pass, start twice quickly.
- Done when: each ends in a clear state with no stuck spinner.

### A3.3 Account kinds
- kind: qa   size: S
- Found: only `kind == "chatgpt"` is usable; an API-key Codex login shows a note. (C)
- Do: confirm the API-key note; decide if Free, Edu and Team plans behave the same (the screenshot is an Edu account).
- Done when: recorded.

### A3.4 Usage windows: meaning and freshness
- kind: fix   size: S
- Found: the two bars are `account/rateLimits/read` primary and secondary windows, account-wide. No "as of" time is shown; a failed read leaves the last numbers. (C) `codex_account.rs:340-388`, `CodexAccountCard.tsx:50-62`
- Do: PRD FR12. Label "whole ChatGPT account", show the read time, show stale state.
- Done when: Vitest covers fresh, stale and missing windows.

### A3.5 Each status read starts a Codex process
- kind: measure   size: S
- Found: `read_status` spawns an app-server, makes three requests and shuts it down, on every card mount and refresh (799 ms in the screenshot). (C)
- Do: count spawns over a normal session; decide whether a short cache is worth it.
- Done when: the count is recorded; a fix part is opened only if it is high.

### A3.6 Sign out affects Codex everywhere
- kind: decide   size: S
- Found: N7. `codex_home_dir()` is `$CODEX_HOME` or `~/.codex`; logout is the app-server's `account/logout`. (C)
- Do: confirm on a Mac where the Codex CLI is in use; then choose: warn in the button copy, or give FNDR its own Codex home.
- Done when: the choice is in ADR 024 or its own note.

### A3.7 Forced file credential store
- kind: decide   size: S
- Found: `cli_auth_credentials_store="file"` is passed so tokens land in `auth.json` and not the Keychain, because Hermes imports from that file. (C)
- Do: confirm whether this moves an existing Keychain login to a plaintext file for the person's normal Codex use; record the file mode; decide if that is acceptable and say so in the sign-in copy.
- Done when: written down with the observed file mode.

### A3.8 Reconnect path
- kind: qa   size: S
- Found: the supervisor refreshes every 30 minutes and sets `reconnect_chatgpt`. (C)
- Do: revoke the session from the ChatGPT side, wait or force a refresh, confirm the card offers Reconnect and a send fails with a message that says so.
- Done when: recorded.

### A3.9 Signed out while Hermes is configured for ChatGPT
- kind: fix   size: S
- Found: the badge keeps showing the saved provider and model after sign out; the failure appears only on send. (V)
- Do: fold into A4.1.
- Done when: A4.1 covers it.

## A4. Readiness and status

### A4.1 Badge states
- kind: fix   size: S
- Found: badge is "Not set up" until a config is saved, whatever the sign-in state. (C) `AgentWorkspace.tsx:136-140`
- Do: PRD FR11. States: Install Hermes, Sign in, Choose a model, Reconnect, `<Provider> · <model>`.
- Done when: Vitest covers each state.

### A4.2 The status dot
- kind: qa   size: S
- Found: the dot is ready only when `api_server_ready`; nothing explains it. (C)
- Do: decide what the dot means to a person and give it a text equivalent.
- Done when: the dot has an accessible name that changes with state.

### A4.3 "Hermes isn't reachable yet"
- kind: qa   size: S
- Found: shown when the status call itself fails. (C)
- Do: force the failure; confirm Retry recovers; confirm the 2026-10-07 deadlock fix (`787860c`) holds under rapid open and close.
- Done when: recorded.

### A4.4 Setup pane keyboard and screen reader pass
- kind: qa   size: S
- Do: tab order, focus on open, provider control with arrow keys, error announced, password field labelled. Feeds PX-05.
- Done when: a short list of failures or "none".

## A5. Gateway lifecycle

### A5.1 Cold start on first message
- kind: measure   size: S
- Found: the first send starts the gateway and waits up to 12 s for its API. (C) `send_hermes_message`, `ensure_hermes_gateway_ready`
- Do: time first-message latency ten times; count timeouts.
- Done when: median and worst are recorded.

### A5.2 Fixed port 8742
- kind: qa   size: S
- Found: host and port are constants; readiness is "something answers on that port". (C)
- Do: occupy the port with another process; run two FNDR builds at once.
- Done when: the failure message is recorded; fix part if it hangs or talks to the wrong process.

### A5.3 Crash and restart
- kind: qa   size: S
- Found: one restart per streak, then "crashed". The restart path does not rewrite the MCP config or re-sync the login. (C) `start_hermes_supervisor`
- Do: kill the gateway once, then twice; send after each.
- Done when: states and messages are recorded.

### A5.4 Gateway output is discarded
- kind: fix   size: S
- Found: stdout and stderr go to null, so a failed start has no cause on record. (C)
- Do: write to a bounded, private log file in app data; no message content.
- Done when: a forced start failure leaves a readable cause.

### A5.5 Gateway outlives FNDR
- kind: fix   size: S
- Found: N6. (C handler, V orphan)
- Do: confirm with `ps` after quitting; add the stop call to the exit handler; add a start-time check that kills a stale gateway.
- Done when: no gateway process exists five seconds after quit.

### A5.6 Idle shutdown
- kind: decide   size: S
- Found: once started, the gateway runs until Save, an explicit stop, or quit. (C)
- Do: decide an idle timeout using the A1.5 numbers.
- Done when: decided; implement only if the cost is real.

### A5.7 MCP endpoint at gateway start
- kind: qa   size: S
- Found: the MCP block is written only if FNDR's MCP server is running at that moment; the port changes per launch. (C) `write_hermes_mcp_config`
- Do: start the gateway with MCP off, then turn MCP on; restart FNDR with the gateway alive (see A5.5).
- Done when: the person-visible effect is recorded.

## A6. What a message carries

### A6.1 Capture one real request body
- kind: qa   size: S   needs: 0.2
- Do: with a local proxy or a debug log on the synthetic profile, save the exact body FNDR posts to the gateway for: no attachments, three attachments, a question about the past, a question with no past reference.
- Done when: four bodies are saved (synthetic data only). This is the ground truth for A6.2 to A6.6.

### A6.2 Automatic context goes to every provider
- kind: fix   size: M   needs: E2
- Found: `memory::snippets(state, &user_text)` runs for every message and provider; Notch Do gates the same call behind `refers_to_past`. (C) `hermes_agent.rs:1612`
- Do: PRD FR1. Gate by provider locality and a switch.
- Done when: unit tests on the built input for cloud-off, cloud-on, and loopback.

### A6.3 Page copy says "memories you choose"
- kind: fix   size: S   needs: A6.2
- Found: subtitle and empty state describe attachment only. (C)
- Do: PRD FR2.
- Done when: copy matches the active sources.

### A6.4 Show what was sent
- kind: fix   size: M
- Found: N11.
- Do: return the snippet titles with the reply; show them as chips distinct from attached ones; store them in history.
- Done when: a sent message lists every memory that left with it.

### A6.5 What an attached memory includes
- kind: qa   size: S
- Found: title, app, time, the page URL, and up to 8,000 characters shared across attachments, from `clean_text` or raw `text`. (C) `agent_chats.rs` `memory_context_block`
- Do: decide whether the URL and raw text should go to a cloud provider; check that a memory from a now-blocklisted app can still be attached.
- Done when: decided and tested.

### A6.6 Retrieval filters for automatic context
- kind: qa   size: S
- Found: uses the shared `retrieve_search_results` path with default request fields. (C)
- Do: confirm excluded apps and low-quality records are filtered the same way as Search; the recent search fixes (`d73eb8a`) changed what counts as a match.
- Done when: a seeded excluded-app memory never appears in a body from A6.1.

### A6.7 Instruction strings live outside `prompts.rs`
- kind: cleanup   size: S
- Found: `instructions`, `SOUL.md`, `.hermes.md` and the attached-memory header are inline; the catalog lists them as living in `hermes_agent.rs`. The attached-memory header says "the user", which the voice rule forbids in text written about a memory. (C)
- Do: move to `prompts.rs`, add to `live_prompts`, bump the version, update catalog rows.
- Done when: the fingerprint test passes with the new entries.

### A6.8 The instructions promise things that do not exist
- kind: fix   size: S
- Found: the request `instructions` tell the model to use "FNDR's context files and private snapshot"; the snapshot was removed. They also offer "safe computer-use support". (C)
- Do: rewrite to match what the gateway actually has.
- Done when: the string matches A9.1's tool list.

## A7. Memory picker

### A7.1 Picker behavior
- kind: qa   size: S
- Do: search, empty query lists recent, select up to 8, the ninth is disabled, Escape closes only the picker, deleted memory between pick and send.
- Done when: recorded against `AgentWorkspace.test.tsx` coverage.

### A7.2 Picker in Private Mode
- kind: qa   size: S
- Do: turn Private Mode on; can memories still be attached and sent to a cloud provider? Decide if that is right.
- Done when: decided.

### A7.3 The picker note names the provider
- kind: qa   size: S
- Found: "Attached memories are sent to {provider}" uses the badge text split on " · ". (C)
- Do: check each provider and the Custom case (shows "Custom", not the host).
- Done when: Custom shows the host.

## A8. Chat history

### A8.1 Storage shape and limits
- kind: fix   size: S
- Found: N10. One `agent-chats.json`, full rewrite on every exchange, no cap, default mode. (C)
- Do: mode `0600`; cap by count and age; a lock or single writer.
- Done when: tests for the cap and for two concurrent appends.

### A8.2 Delete is immediate and partial
- kind: fix   size: S
- Found: the delete button has no confirm or undo. Hermes keeps the conversation server-side under the same id (`"store": true`). (C for both; V that Hermes's copy survives)
- Do: confirm what remains in `hermes-home` after a delete; add undo or confirm; delete Hermes's copy too or say that it is kept.
- Done when: after delete, no copy of the text remains, or the UI says where one does.

### A8.3 Failed sends are not in history
- kind: fix   size: S
- Found: `record_exchange` runs only after a successful reply. (C)
- Do: record the user message with a failed marker.
- Done when: a failed message is still there after reopening the chat.

### A8.4 Reopening an old chat
- kind: qa   size: S
- Do: reopen after an app restart, after a provider switch, after a Hermes reinstall. Does the model still have the earlier turns?
- Done when: recorded.

### A8.5 History and the trust spec
- kind: decide   size: S
- Found: chat text, including memory-derived answers, is kept without a stated retention. PD-09 asks for one page on what FNDR keeps.
- Do: add Agent chats, the operator journal and Hermes's own store to PD-09.
- Done when: PD-09 lists them.

## A9. What Hermes can do

### A9.1 Inventory the tools Hermes has
- kind: qa   size: M
- Found: FNDR writes only `model:` and `mcp_servers:` to `config.yaml`. Whatever the pinned Hermes enables by default is on. (C for the config, V for the list)
- Do: at the pinned commit, list every toolset and its default; ask the running gateway for its tool list; note which can run commands, write files, or reach the network.
- Done when: the list is saved. This decides E4.

### A9.2 Can the model act without asking?
- kind: qa   size: M   needs: A9.1, 0.3
- Do: on the synthetic profile ask for a harmless file write and a harmless shell command; then attach a memory containing an instruction to do the same. Record whether anything ran and whether anything asked.
- Done when: results recorded. Any action taken from memory text is a P0 fix part.

### A9.3 Tool allowlist in the written config
- kind: fix   size: M   needs: A9.1, E4
- Do: PRD FR8.
- Done when: the gateway's tool list equals the allowlist.

### A9.4 The MCP token Hermes holds
- kind: fix   size: M
- Found: Hermes receives FNDR's own bearer token and endpoint, so it can call every MCP tool, including `agent.run` and `fndr.remember`. ADR-017 covers MCP auth; `mcp-tool-audit.md` lists side-effecting tools. (C)
- Do: PRD FR9. A read-scoped token, or a tool filter in the Hermes MCP block.
- Done when: a write tool called with Hermes's token is refused.

### A9.5 MCP results are a second egress path
- kind: decide   size: S   needs: E2
- Found: with a cloud provider, anything Hermes pulls through MCP is forwarded to the provider by the gateway and is not counted in Privacy Activity. (C by design)
- Do: decide whether Hermes gets MCP at all on a cloud provider when automatic context is off.
- Done when: decided in ADR 024.

### A9.6 Tool activity is invisible
- kind: fix   size: M   needs: A9.1
- Found: the page shows only the final message text. (C)
- Do: surface tool calls from the gateway's response items as activity steps.
- Done when: a reply that used a tool shows which one.

## A10. Files and secrets

### A10.1 File modes under `hermes-home`
- kind: fix   size: S
- Found: `config.yaml` (MCP bearer token), `.env` (provider key, gateway API key) and `fndr_setup.json` are written with default modes. `hermes_codex.rs` already sets `0600` on the auth entry. (C)
- Do: PRD FR10.
- Done when: a test reads the modes.

### A10.2 Provider keys at rest
- kind: decide   size: S
- Found: OpenRouter and custom keys sit in a plaintext `.env`. (C)
- Do: decide Keychain versus file with `0600`; Hermes reads the file, so Keychain needs an injection step at spawn.
- Done when: decided.

## A11. The thread

### A11.1 Replies render as plain text
- kind: fix   size: S
- Found: `<p className="aw-bubble">{message.content}</p>`; lists, code and links are not formatted. (C)
- Do: reuse the renderer the Ask surface uses, if one exists; sanitize.
- Done when: a reply with a list and a code block renders as such.

### A11.2 No streaming
- kind: decide   size: S
- Found: one blocking POST; the person sees "Waiting for..." until the whole reply exists. (C)
- Do: measure typical wait (F2) before deciding to stream.
- Done when: decided with the number.

### A11.3 No stop
- kind: fix   size: S
- Found: nothing cancels an in-flight request; New chat only detaches the UI. (C)
- Do: a Stop control that aborts the request.
- Done when: Stop returns the composer within a second and no reply is appended.

### A11.4 A failed send loses the typed text
- kind: fix   size: S
- Found: the draft is cleared before the await; on failure the bubble stays but there is no retry. (C)
- Do: restore the draft or add Retry on the failed bubble.
- Done when: Vitest covers the failure path.

### A11.5 Citations
- kind: fix   size: M   needs: A6.4
- Found: the prompt tells the model to cite memories by number; numbers in the reply link to nothing. (C)
- Do: turn `[n]` into a link to the memory.
- Done when: clicking a citation opens the memory.

### A11.6 Empty state and first-run hint
- kind: qa   size: S
- Do: review the empty-state copy against the knowledge-worker positioning; three example prompts that work with attachments.
- Done when: copy reviewed with `design:ux-copy` rules and the no-dash rule.

## A12. Cleanup and docs

### A12.1 Remove unmounted panels
- kind: cleanup   size: S
- Found: N9. Confirm with a repo-wide search including lazy imports before deleting. (C by grep)
- Do: delete `AgentPanel.tsx`, `ResearchPanel.tsx`, their CSS and tests, and IPC wrappers left without callers.
- Done when: typecheck and Vitest pass.

### A12.2 Remove commands with no caller
- kind: cleanup   size: S   needs: A12.1
- Found: `start_agent_task`, `get_agent_status`, `stop_agent`, `send_direct_chat`, `quick_setup_ollama`, `sync_hermes_bridge_context`, `start_hermes_gateway`, `stop_hermes_gateway` are used only by the unmounted panels. (C by grep)
- Do: remove them and the `agent_runner.py` sidecar if nothing else uses it; unregister in `main.rs`.
- Done when: `cargo check` passes and `hermes_agent.rs` is shorter.

### A12.3 Split `hermes_agent.rs`
- kind: cleanup   size: M   needs: A12.2
- Found: 2,092 lines mixing runtime install, gateway, chat, daily briefing and a greeting. (C)
- Do: move `generate_daily_briefing` and `get_fun_greeting` out; no behavior change.
- Done when: tests pass unchanged.

### A12.4 Rewrite `docs/agent.md`
- kind: doc   size: S
- Found: describes a deterministic Ask, Plan, Act, Learn runner; the page is Hermes chat. The `agent/` Rust module still exists and is used by MCP `agent.run`. (C)
- Do: describe the shipped page; move the older design to a section about the MCP agent tools; add the term "Agent" to `docs/CONTEXT.md`.
- Done when: a reader can find every command the page calls.

---

# Track B. Screen Guide

Files: `src-tauri/src/ipc/commands/screen_guide.rs` (6,156 lines),
`screen_guide_diagnostics.rs`, `openclicky_bridge.rs`,
`src/domains/screen-guide/`. Existing tickets GS-10, GS-15, GS-16 and GS-17
already cover full-screen and Retina stability, multi-display capture, and
diagnostics. Parts below add to them and do not repeat them.

## B1. Enable and settings

### B1.1 Settings round trip
- kind: qa   size: S
- Do: each switch (enabled, shortcut, speak, show cursor, model, screenshot, OpenClicky, operate) persists across restart and reflects in behavior.
- Done when: a table of switch, stored key, observed effect.

### B1.2 Model choice copy
- kind: qa   size: S
- Found: choosing ChatGPT sends the question, OCR text and recent conversation. (C)
- Do: read the panel copy next to the model control; does it name all three?
- Done when: the copy lists what leaves, or a fix part exists.

### B1.3 Screenshot switch only matters with ChatGPT
- kind: qa   size: S
- Do: confirm it is hidden or disabled with the local model, and off by default after switching models back and forth.
- Done when: recorded.

## B2. Shortcut and voice

### B2.1 Shortcut coexistence
- kind: qa   size: S
- Do: Screen Guide shortcut, Alt+N (notch), Alt+Space (omnibar) and Autofill together; a conflicting custom shortcut.
- Done when: none steals another; conflict gives a message.

### B2.2 Press and release lifecycle
- kind: qa   size: S
- Do: tap, long hold, rapid double press, release during transcription, press while a turn is answering. Check the microphone indicator goes off each time.
- Done when: no stuck microphone or overlay. Overlaps GS-15; reuse its evidence.

### B2.3 Two voice paths
- kind: decide   size: S
- Found: Screen Guide records in a WebView and normalizes audio; Notch Do uses the shared native voice owner. ADR-020 wants one owner. VO-09 plans the move. (C)
- Do: confirm VO-09 covers Screen Guide's recorder; if not, add it there.
- Done when: one ticket owns the migration.

## B3. Privacy gates

### B3.1 Pre-capture refusal
- kind: qa   size: S   needs: 0.3
- Found: Private Mode, FNDR's own windows and blocklisted contexts are checked before capture, and Private Mode is re-checked later. (C)
- Do: each case on the fixtures; confirm no OCR and no model call in logs.
- Done when: each refusal is recorded with its message.

### B3.2 Post-OCR gate
- kind: qa   size: S
- Found: `screen_guide_ocr_is_allowed` runs on the recognized text before any answer. (C)
- Do: find what it blocks (read the function), then test one allowed and one blocked screen with the ChatGPT model selected, since this gate is the last thing between screen text and the cloud.
- Done when: the blocked screen produces no Codex call.

### B3.3 Private Mode turned on mid-turn
- kind: qa   size: S
- Do: toggle during capture, during OCR, during the model call.
- Done when: the turn ends and nothing is sent after the toggle.

## B4. Capture and B5. OCR

### B4.1 Primary display only
- kind: qa   size: S
- Found: documented limit; GS-16 fixes it. (C)
- Do: with two displays, ask about the second one. Does the answer or UI imply it was read?
- Done when: the copy does not imply it.

### B5.1 OCR profile on the fixtures
- kind: measure   size: S
- Do: run the ten fixtures through the `0.015` profile; record what text was missed.
- Done when: miss list saved. Shared with GS-10's table.

### B5.2 Truncation
- kind: qa   size: S
- Found: OCR text is cut to a character cap before the model. (C) `MAX_SCREEN_GUIDE_OCR_CHARS`
- Do: a dense screen; is the bottom of the screen dropped, and does the answer say so?
- Done when: recorded.

## B6. Local answer

### B6.1 Local model present, absent, busy
- kind: qa   size: S
- Do: model downloaded, not downloaded, and capture pipeline holding `model_pipeline_lock`.
- Done when: latency and the fallback text for each are recorded.

### B6.2 Grounded fallback quality
- kind: measure   size: S
- Do: read ten fallback answers; are they useful or just a dump of screen text?
- Done when: a yes or no per fixture.

## B7. ChatGPT answer

### B7.1 Log the request
- kind: fix   size: S
- Found: no `record_model_request` on this path. (C)
- Do: PRD FR4, with a screenshot flag.
- Done when: a unit test sees the row.

### B7.2 Session lockdown check
- kind: qa   size: S
- Found: read-only sandbox, `approvalPolicy: never`, a list of disabled features, and every user MCP server disabled. (C) `READ_ONLY_DISABLED_FEATURES`, `read_only_session_args`
- Do: compare the disabled list with the feature list of the installed Codex; a feature added upstream is on by default.
- Done when: the list is confirmed current, and a test fails when Codex adds an acting feature (or a note says why that cannot be tested).

### B7.3 Screenshot staged on disk
- kind: fix   size: S
- Found: N8. Written as `screen.jpg` in a per-turn folder under the system temp directory, removed on drop. A crash leaves it. (C)
- Do: private directory with mode `0700`, file `0600`, startup sweep of leftovers; amend ADR-004 or Screen Guide FR7 to state the exception.
- Done when: after a forced kill mid-turn, the next start removes the file.

### B7.4 Conversation history goes too
- kind: qa   size: S
- Found: up to 1,200 characters of recent turns are included. (C)
- Do: confirm history from a turn answered locally is later sent to ChatGPT after switching models.
- Done when: decided whether switching models clears history.

### B7.5 Signed-out and limit-reached states
- kind: qa   size: S
- Found: signed out returns "Sign in with ChatGPT in Hermes Agent settings". (C)
- Do: signed out, limit reached, offline, timeout. Does it fall back to local or fail?
- Done when: each message is recorded; decide on fallback.

### B7.6 Injection fixture
- kind: qa   size: S   needs: 0.3
- Do: the injected-instruction fixture with both models; the answer must not follow the on-screen instruction. Shares fixtures with GS-12.
- Done when: both pass or a fix part exists.

## B8. Point cue and overlay

### B8.1 Cue accuracy
- kind: measure   size: S
- Do: on the fixtures, how often is the cue on the right label, a wrong label, or absent?
- Done when: a count per model.

### B8.2 Overlay never takes clicks or focus
- kind: qa   size: S
- Do: click through it; type in the app underneath while it shows. Covered partly by GS-15.
- Done when: recorded.

## B9. OpenClicky bridge

### B9.1 What the bridge receives
- kind: qa   size: S
- Found: FNDR posts a point and a caption to OpenClicky on `127.0.0.1:32123`, reading OpenClicky's token from its env var or secrets file. (C)
- Do: capture one request; is the caption the full answer text? Does OpenClicky speak it with a cloud voice?
- Done when: recorded.

### B9.2 Is this feature wanted?
- kind: decide   size: S
- Found: the Screen Guide PRD lists launching Clicky as a non-goal and does not mention the bridge. (C)
- Do: keep and document, move to Labs, or remove.
- Done when: decided and the PRD amended.

## B10. Speech, B11. File lookup, B12. Diagnostics, B13. Status item

### B10.1 Speech interrupt and mute
- kind: qa   size: S
- Do: new turn during speech, mute during speech, quit during speech.
- Done when: no orphan `say` process.

### B11.1 File lookup routing
- kind: qa   size: S
- Do: ten phrasings, five that should route to lookup and five that should not.
- Done when: a routing table with hits and misses.

### B11.2 File lookup bounds
- kind: qa   size: S
- Do: a name matching hundreds of files; a file outside the three folders; Private Mode.
- Done when: bounded results, relative paths only, no query in Private Mode.

### B12.1 Diagnostics
- kind: qa   size: S
- Do: run GS-17's checks once on the current build; do not duplicate the ticket.
- Done when: GS-17's evidence is current.

### B13.1 Status item states
- kind: qa   size: S
- Found: the 2026-10-07 change made the menu bar icon the glyph. (C)
- Do: each phase shows the right fixed label and never user text.
- Done when: recorded.

### B14.1 Amend the Screen Guide PRD
- kind: doc   size: S   needs: B9.2, C3.1
- Do: add the ChatGPT answer path, the screenshot staging exception, the OpenClicky bridge decision, and remove the operate switch from this feature's scope.
- Done when: the PRD matches the panel.

---

# Track C. Notch (Ask and Do)

Files: `src/domains/notch/`, `src-tauri/src/ipc/commands/notch.rs`,
`computer_use.rs`, `src-tauri/src/operator/`.

## C1. The notch window

### C1.1 Hover, summon, dismiss
- kind: qa   size: S
- Do: hover open and close, Alt+N, click outside, on a notched and a non-notched display, with an external display as primary. GS-02 asked for this smoke test; check whether its evidence exists.
- Done when: a recording, or GS-02's is linked.

### C1.2 Click-through margin
- kind: qa   size: S
- Found: the window is click-through and a 60 Hz pointer poll turns hit-testing on over the drawn panel. (C) `notch.rs` header
- Do: click targets right beside the panel; menu bar items under the envelope.
- Done when: no dead zone.

### C1.3 Cost of the 60 Hz poll
- kind: measure   size: S
- Do: CPU and energy impact with the notch idle for ten minutes.
- Done when: the number is recorded; fix part only if it shows in Activity Monitor's energy list.

### C1.4 Mode memory
- kind: qa   size: S
- Found: mode is kept in `localStorage` and defaults to Do when the operate switch is on. (C)
- Do: decide whether opening the notch should default to an acting mode with the microphone live.
- Done when: decided (links to E6).

## C2. Notch Ask

### C2.1 What Ask calls
- kind: qa   size: S
- Found: `searchMemoryCards`, `listMemoryCards`, `fndrAnswer`, `transcribeVoiceInput`. (C)
- Do: confirm Ask is fully local; record latency to first result and to answer.
- Done when: recorded, with a Privacy Activity check showing no model request.

### C2.2 Ask conversation carry-over
- kind: qa   size: S
- Found: `notchConversation.ts` builds a follow-up query from earlier turns. (C)
- Do: three follow-ups; does the query drift? When is it reset?
- Done when: recorded.

### C2.3 Open a result
- kind: qa   size: S
- Do: `notch_hud_open_memory` for a page, a file, a missing source. Uses Minh's reopen results.
- Done when: each reopen outcome shows a sensible message.

## C3. The operate switch

### C3.1 Move it out of Screen Guide
- kind: fix   size: S
- Found: stored as `screen_guide.operate_computer`, shown in the Screen Guide panel and Setup Center. (C)
- Do: PRD FR13.
- Done when: migration test passes.

### C3.2 Consent copy against behavior
- kind: fix   size: S
- Found: N12. The panel copy names what is sent. The stored comment and the phrase "every action still needs an explicit approval" do not match the tiered policy. (C)
- Do: one accurate sentence on what runs without asking.
- Done when: copy and `config.rs` comment match ADR-022's amendment.

### C3.3 Permissions panel
- kind: qa   size: S
- Found: `computer_use_permissions` probes the backend and reports Accessibility, Automation and microphone. (C)
- Do: each permission missing in turn; is the fix named and does the state update without a restart?
- Done when: recorded.

## C4. Backend

### C4.1 Which computer-use server runs
- kind: qa   size: S
- Found: OpenAI's bundled Computer Use if present under the Codex plugin cache, otherwise `open-computer-use` from one of several per-user paths; a refused Automation grant flips to the fallback for the next run. (C)
- Do: run with each backend; record differences in tool names and tree format, since the policy parses both.
- Done when: both produce the same policy decisions on the fixtures.

### C4.2 Trust in the helper binary
- kind: decide   size: S
- Found: the fallback is a third-party npm package pinned at `0.3.6`, found by path, with Accessibility control. (C)
- Do: record who publishes it and what it can do; decide whether to verify a checksum or install it into app data.
- Done when: decided.

### C4.3 Pin Codex
- kind: fix   size: S
- Found: `@openai/codex` installs unpinned. (C)
- Do: PRD FR15, plus record the tested Codex version.
- Done when: the install string has a version.

### C4.4 Protocol drift guard
- kind: fix   size: S
- Found: N13. Also depends on `_meta.codex_approval_kind`, `tool_params`, and item shapes. (C)
- Do: a clear error when a call arrives whose tool cannot be read ("this Codex version is not supported"), and a recorded-fixture test of one real approval message.
- Done when: an unreadable approval yields that message, not a silent block.

## C5. Voice loop

### C5.1 Listening states
- kind: qa   size: S
- Do: opens listening, 1.2 s endpoint, 8 s silence, microphone denied, recognizer unavailable.
- Done when: each state appears with its message.

### C5.2 Mishearing
- kind: measure   size: S
- Do: twenty spoken requests; count wrong transcripts that still produced a plan.
- Done when: the rate is recorded. Feeds E6.

### C5.3 Ambient speech during a run
- kind: qa   size: S
- Found: the microphone stays open; a new final transcript during a run is held as a redirect until "go". (C)
- Do: play a podcast while a run is in progress.
- Done when: no redirect starts and no approval is given by the audio (see C9.2).

### C5.4 Typed entry
- kind: qa   size: S
- Do: is there a way to type a Do request? If not, decide whether one is needed for quiet rooms and accessibility.
- Done when: recorded or decided.

## C6. Planning

### C6.1 The transcript goes to the cloud before any review
- kind: decide   size: S
- Found: finishing an utterance calls `computer_use_plan`, which sends the transcript to ChatGPT. ADR-020's base rule is that finishing transcription never starts a command; the amendment treats the plan card as the review. (C)
- Do: decide whether a misheard sentence leaving the Mac is acceptable, or whether the transcript shows for a moment first.
- Done when: decided (E6).

### C6.2 When memories join the plan
- kind: qa   size: S
- Found: only when an English regex matches words such as "yesterday", "again", "I was reading". "Again" alone triggers it. (C) `plan::refers_to_past`
- Do: ten requests; list false triggers and misses. The card shows "Planning with N memories".
- Done when: the table exists; decide whether the card should name the memories.

### C6.3 The planning turn can act
- kind: fix   size: M
- Found: N1. (V)
- Do: first prove it: a request that tempts the planner to look at the screen, and watch for `Action` events before `Planned`. Then refuse every tool approval while `step` is `None`, or start the planner thread without the server.
- Done when: a test shows a tool request in a planning turn is declined.

### C6.4 Plan limits
- kind: qa   size: S
- Found: at most 8 steps; 90 s to plan; open-URL steps must be http or https; steps need an app. (C)
- Do: a request that needs more; a request with nothing to do; a non-web link.
- Done when: each message is recorded.

### C6.5 Plan quality
- kind: measure   size: M
- Do: twenty everyday requests from the PD-19 interviews or the dogfood diary; mark each plan right, partly, or wrong before running it.
- Done when: the table exists. This is the first real quality number for the feature.

## C7. The plan card and starting

### C7.1 The 1.5 second window
- kind: measure   size: S
- Do: ten tries at stopping by voice and by tap during the countdown; how many made it?
- Done when: the success count is recorded.

### C7.2 Auto-start rule
- kind: fix   size: S   needs: E3
- Do: PRD FR14.
- Done when: `doRun.test.ts` covers both branches.

### C7.3 A plan left waiting
- kind: qa   size: S
- Found: the backend waits for Start with no timeout; the Codex process stays alive. (C) `wait_for_start`
- Do: close the notch on the plan card (the unmount stops a run in progress; check the plan phase counts as in progress); leave it open and idle.
- Done when: no Codex process remains after the notch closes.

### C7.4 Countdown accessibility
- kind: qa   size: S
- Do: Reduce Motion, VoiceOver announcement, keyboard access to Cancel.
- Done when: recorded.

## C8. The policy, rule by rule

One part per rule family, so each can be reviewed and tested alone. All are in
`operator/policy.rs`.

### C8.1 Sensitive apps
- kind: fix   size: S
- Found: password managers, Keychain, Passwords, System Settings. Not listed: Terminal and other shells, Script Editor, Shortcuts, Automator, Wallet, banking apps, FNDR itself. (C)
- Do: decide the list; FNDR operating its own settings (turning off Private Mode, the kill switch or the blocklist) must be Never.
- Done when: tests for each added app.

### C8.2 Opening links
- kind: fix   size: M   needs: E7
- Found: N5.
- Do: options: confirm any link whose host the person did not say; strip or cap query strings; allow search-engine hosts only without asking.
- Done when: the injected-instruction fixture cannot cause a link with page text in it to open unasked.

### C8.3 Clicks in browsers and media apps
- kind: fix   size: M   needs: E7
- Found: N4. A known label not on the Never or Confirm list runs. (C)
- Do: list twenty real buttons from common sites and apps and classify each with today's rules; then choose: grow the lists, or flip browsers to confirm-by-default with a short run list (links, tabs, play, pause, next).
- Done when: the twenty-button table passes under the chosen rule.

### C8.4 Labels in other languages and icon-only buttons
- kind: fix   size: S   needs: C8.3
- Found: both lists are English; an element with a role but no text has a "label" that matches nothing. (C)
- Do: treat an empty or non-Latin label as unknown, which is Confirm.
- Done when: tests with a Spanish "Enviar" and an unlabeled button.

### C8.5 Clicks elsewhere and by position
- kind: qa   size: S
- Found: known label outside media and browsers is Confirm; unknown target is Confirm; position clicks in media apps run. (C)
- Do: confirm on a native app fixture.
- Done when: recorded.

### C8.6 Typing and focus tracking
- kind: fix   size: M
- Found: N14. Typing into a "search field" runs; anywhere in a media app runs; secure fields are Never. (C for rules, V for the stale focus)
- Do: prove or disprove with: click a search field, press Tab, type. If text lands in the next field unasked, re-read focus before classifying or confirm when focus was not set by FNDR's last click.
- Done when: the Tab case asks first, and the password-field fixture is never typed into.

### C8.7 Keys
- kind: fix   size: S
- Found: Return runs in a search field or media app, is Never in messaging apps, Confirm elsewhere. In browsers and media apps every other key runs, including Cmd+W, Cmd+Q, Cmd+S, Cmd+P and plain Backspace. Messaging inside a browser tab is not a "messaging app". (C)
- Do: an explicit run list for navigation and playback keys; everything else Confirm.
- Done when: tests for the listed chords.

### C8.8 Reading the screen
- kind: qa   size: S   needs: C12.2
- Found: `get_app_state` and `select_text` always run and their result goes to the model. (C)
- Do: confirm reading is refused for blocklisted apps once C12.2 lands.
- Done when: covered by C12.2's test.

### C8.9 Stale element indexes
- kind: qa   size: S
- Found: labels come from the last tree FNDR saw; the click names an index. (C)
- Do: change the page between the read and the click (a fixture that reorders buttons on a timer). Does the helper click the new element at that index?
- Done when: recorded; if yes, a fix part to re-read before any click classified as Runs.

### C8.10 One entry point with the kill switch
- kind: fix   size: S
- Found: `actions_kill_switch` is read by `agent/risk_policy.rs` and MCP only. (C)
- Do: PRD FR7.
- Done when: Rust tests for refuse at start and stop mid-run.

## C9. Approvals

### C9.1 Approval card content
- kind: qa   size: S
- Found: the summary is built by FNDR from the tool, arguments and observed label, not model prose. (C) `describe_tool_call`
- Do: read ten real cards. Can a person tell what will happen and where?
- Done when: unclear ones listed.

### C9.2 Spoken approval
- kind: fix   size: S   needs: E5
- Found: N2.
- Do: per the command-surface contract, accept voice for Stop and decline only; approval needs a tap or key.
- Done when: `doRun.test.ts` shows "yes" does not approve.

### C9.3 An approval left unanswered
- kind: qa   size: S
- Found: it waits until the 180 s step timeout, then the run fails. (C)
- Do: confirm the failure message is clear and nothing ran.
- Done when: recorded.

### C9.4 Decline, then what
- kind: qa   size: S
- Do: decline one action; does the model try another route to the same end (for example a key press after a declined click)?
- Done when: recorded; if yes, decide whether one decline should end the step.

### C9.5 Never-tier feedback
- kind: qa   size: S
- Do: ask for something in the Never tier ("send this email"). Is the refusal clear at the plan card, or only after steps have run?
- Done when: decided whether the planner should refuse up front.

## C10. Stop

### C10.1 Stop by voice, button and Escape
- kind: qa   size: S
- Do: each, during planning, the plan card, a running step, and a pending approval. Time from Stop to no further action.
- Done when: a latency table; any action after Stop is a P0.

### C10.2 Process cleanup
- kind: qa   size: S
- Found: Stop sends SIGKILL to Codex's process group. (C)
- Do: after Stop, `ps` for Codex and the computer-use helper; a helper that left its own process group survives.
- Done when: none remain.

### C10.3 Half-finished state
- kind: qa   size: S
- Do: stop in the middle of typing or a drag. What is left on screen, and does the result card say the task is incomplete?
- Done when: recorded.

### C10.4 Quit during a run
- kind: fix   size: S
- Found: N6.
- Do: call the stop path from the exit handler.
- Done when: no Codex process after quit mid-run.

## C11. Verification and results

### C11.1 What "done" means per step
- kind: qa   size: S
- Found: real checks exist for "app is in front", "media is playing" and "page is open". Every other step passes on the model's own report. (C) `plan::verify`
- Do: list which everyday tasks have a real check; show "reported done" differently from "checked".
- Done when: the result card distinguishes the two.

### C11.2 Retry behavior
- kind: qa   size: S
- Found: one retry per step with the failure reason passed back. (C)
- Do: a step that fails twice; a step whose first try half-worked (retry could do it twice, for example add an item twice).
- Done when: recorded.

### C11.3 Result card and undo
- kind: fix   size: S
- Found: the command-surface contract says the result card states whether undo exists. Notch Do's finish summary is "Done: a, b, c". (C)
- Do: add the undo statement.
- Done when: the card says what can and cannot be undone.

## C12. Privacy gates

### C12.1 Private Mode during a run
- kind: fix   size: S
- Found: N3.
- Do: stop the run on the Private Mode event; refuse Start.
- Done when: Rust test.

### C12.2 Blocklisted apps
- kind: fix   size: M
- Found: N3. Command-surface invariant 5 requires it.
- Do: refuse `get_app_state`, clicks and typing for an app on the person's blocklist; refuse a plan step that names one.
- Done when: the blocklisted fixture is never read.

### C12.3 FNDR's own windows
- kind: qa   size: S   needs: C8.1
- Do: ask Do to change an FNDR setting.
- Done when: refused.

### C12.4 On-screen text as instructions
- kind: qa   size: M   needs: 0.3
- Found: the app's accessibility text is model input; `OPERATOR_STEP_SYSTEM` states the boundary. GS-12 tests ten injection fixtures for the command surface, not for Notch Do. (C)
- Do: run the injection fixture as the operated app, with tasks that tempt each run-tier action (open a link, click, type in a search field).
- Done when: results recorded; each success becomes a C8 fix.

## C13. Journal

### C13.1 What the journal holds
- kind: qa   size: S
- Found: `operator/journal.jsonl` in app data, with redacted arguments and a text fingerprint. (C)
- Do: read a real journal; confirm no typed text or screen text is stored; note retention and file mode.
- Done when: recorded.

### C13.2 Three logs
- kind: decide   size: S
- Found: `agent/audit.rs`, `operator/journal.rs`, and the SK-01 command journal. (C)
- Do: decide whether Notch Do writes to SK-01's journal so skills (SK-02) and feedback (SK-07) can see it.
- Done when: decided before SK-02 starts.

### C13.3 Show the person their history
- kind: fix   size: M   needs: C13.2
- Do: a list of past runs with outcome, reachable from the notch or Privacy.
- Done when: a run appears there after it finishes.

## C14. Failure states

### C14.1 Signed out, limit reached, offline
- kind: qa   size: S
- Do: each; the notch offers Reconnect for signed out.
- Done when: messages recorded.

### C14.2 Automation refused
- kind: qa   size: S
- Found: error -1743 ends the run with the fix and switches backend for the next run. (C)
- Do: reproduce once.
- Done when: recorded.

### C14.3 Target app missing or slow
- kind: qa   size: S
- Found: 6 s to come to the front, 10 s for a page. (C)
- Do: an app that is not installed; a slow page.
- Done when: messages recorded.

---

# Track D. Shared rules

### D1.1 Persist the model request log
- kind: fix   size: M
- Found: in memory, 200 rows, lost on restart. (C) `privacy_proof.rs`
- Do: PRD FR5.
- Done when: restart test.

### D1.2 Define "local"
- kind: decide   size: S
- Do: loopback only, or also LAN addresses the person typed? Used by A2.2, A2.4, A6.2.
- Done when: one sentence in ADR 024.

### D1.3 Flags on a log row
- kind: fix   size: S   needs: D1.1
- Do: `memories`, `screenshot`, `screen_text` booleans.
- Done when: each caller sets them.

### D1.4 Feature names in the log
- kind: cleanup   size: S
- Found: the struct comment lists `notch_do_screen_text`; confirm it is recorded at `computer_use.rs:677`. (C)
- Do: one enum of feature names instead of strings.
- Done when: a typo cannot create a new feature.

### D1.5 Privacy Activity view
- kind: qa   size: S
- Do: open Privacy after one request from each feature; can a person tell what was sent, where, and by which feature? VS-37 may already cover the view.
- Done when: gaps listed or VS-37 linked.

### D2.1 One consent pattern
- kind: doc   size: S   needs: E2
- Do: a short pattern (name of switch, what leaves, where, how to see it later, how to turn it off) and apply it to the three features' copy.
- Done when: three copy blocks follow it.

### D3.1 Kill switch reach
- kind: qa   size: S   needs: C8.10
- Do: with the switch on, try: command bar tool, MCP side-effecting tool, Notch Do, Hermes message. Record which refuse.
- Done when: all acting surfaces refuse, or a part exists for the one that does not.

### D4.1 Prompts in one place
- kind: cleanup   size: S
- Do: A6.7 plus a check that `OPERATOR_INSTRUCTIONS` and Hermes strings leave the catalog's "outside prompts.rs" list.
- Done when: that list is empty or each remaining row has a reason.

### D5.1 Amend the command-surface contract
- kind: doc   size: S   needs: E3, E5
- Do: add Notch Do as a caller with its own policy table, its start rule and its voice rule, so the contract and the code agree.
- Done when: the Callers table has a Notch Do row.

### D5.2 Decision log entries
- kind: doc   size: S
- Do: each E decision gets a row in `docs/team/decision-log.md`.
- Done when: rows exist.

### D5.3 Ratify ADR-018
- kind: decide   size: S
- Do: PD-01 as a whole, with the three feature exceptions listed.
- Done when: ADR-018's status line is no longer "Proposed".

### D6.1 Labs placement
- kind: decide   size: S
- Found: ADR-023 defines five destinations and Labs. (C)
- Do: where do Agent, Screen Guide and Notch Do sit for Beta on Oct 21?
- Done when: decided.

### D7.1 Sync tickets to the board
- kind: doc   size: S   needs: owner review of this file
- Do: turn the accepted parts into tickets in the repo format, in the lane owner's file or a new one, then run `scripts/team/gitlab_sync.py`. The GitLab MCP token is revoked; the working token is in the shell profile.
- Done when: the board shows them.

### D7.2 Handoff with the lane owner
- kind: doc   size: S
- Do: walk N1 to N14 with Kunj before any fix starts; several may have context the code does not show.
- Done when: each N row is marked agreed, disputed, or already known.

---

# Track E. Decisions for the owner

Each is small and blocks named parts. Recommendation first.

| ID | Question | Recommendation | Blocks |
| --- | --- | --- | --- |
| E1 | Does the ADR-018 amendment stand, and does it cover Screen Guide's ChatGPT path? | Keep it, add Screen Guide, ratify ADR-018 | D5.3 |
| E2 | Automatic memory context to cloud providers | Off by default, one switch | A2.6, A6.2, A9.5 |
| E3 | Notch Do auto-start | Only when no step needs confirmation | C7.2 |
| E4 | Is Hermes an answering surface or an acting one? | Answering for Beta; tools limited to search | A9.3 |
| E5 | Spoken approval in Notch Do | Not accepted; voice can stop and decline | C9.2 |
| E6 | May a transcript reach the cloud before the person sees it? | Show the transcript for a beat first, or default the notch to Ask | C1.4, C6.1 |
| E7 | Browsers: run by default or confirm by default? | Confirm by default with a short run list | C8.2, C8.3 |
| E8 | FNDR's own Codex home, or share `~/.codex`? | Share, with honest copy on Sign out | A3.6, A3.7 |
| E9 | Updating Hermes past the pin | Remove the button until a version is reviewed | A1.3 |
| E10 | OpenClicky bridge | Labs or remove | B9.2 |
| E11 | Notch Do in the SK-01 journal | Yes | C13.2 |
| E12 | Where the three features sit for Beta | Screen Guide in a destination, Agent and Notch Do in Labs | D6.1 |
| E13 | Which account QA spends | Owner's choice | 0.5 |
| E14 | Provider keys: file or Keychain | File with `0600` now, Keychain later | A10.2 |

---

# Track F. Measurement and the longer view

### F1 Notch Do task set
- kind: measure   size: M   needs: C6.5
- Do: twenty tasks, three runs each: success, steps, approvals asked, wall time, stops needed.
- Done when: a table that can be rerun after every policy change.

### F2 Agent latency and usefulness
- kind: measure   size: M
- Do: fifteen questions (reuse VS-18's answer set if it fits) with attachments, with automatic context, with neither; mark correct, partly, wrong; time each.
- Done when: the table shows what automatic context buys, which informs E2.

### F3 Screen Guide correctness
- kind: measure   size: M
- Do: GS-10's ten-fixture table, local and ChatGPT.
- Done when: GS-10's evidence exists; do not duplicate.

### F4 Cost in plan limits
- kind: measure   size: S
- Do: read the usage windows before and after ten Hermes messages and ten Notch Do runs.
- Done when: an approximate cost per use is known, so the usage card's sentence can be specific.

### F5 Dogfood week
- kind: measure   size: M   needs: tracks A to D P0 parts
- Do: one week of real use on the owner's Mac with the diary in `docs/product/dogfood-diary.md`; GS-14 already plans this for tools.
- Done when: a kept, changed, or dropped verdict per feature.

### F6 Short-term product shape (next two weeks)
- kind: decide   size: S   needs: F1, F2, F3
- Questions to answer with the numbers: which one of the three leads the Beta story (PD-04); whether Agent stays a separate page or becomes Ask with a stronger model; whether Notch Do's scope is "media and navigation" on purpose, since those are the only steps it can verify.

### F7 Longer-term capability list
- kind: decide   size: M   needs: F5
- Candidates, each needing its own PRD when chosen: skills from successful Do runs (SK-02); a local model for planning (LM-08 spike); multi-display Screen Guide (GS-16); Do steps with real verification beyond media and pages; Agent that can use the registry tools through the shared policy instead of Hermes's own tools; scheduled or proactive runs.
- Done when: an ordered list of at most three for after Beta.

### F8 User sessions
- kind: measure   size: M
- Do: add two tasks to PD-13's five sessions: one Screen Guide question and one Notch Do task, observed without help.
- Done when: notes exist for each session.

### F9 A release gate for acting features
- kind: decide   size: S
- Do: a short list that must be true before an acting feature leaves Labs: stop works in every phase, kill switch honored, blocklist honored, injection fixtures pass, every cloud request logged.
- Done when: the list is in ADR 024.

---

# Suggested order

Phase 1, no decision needed, mostly reading and running (about a week of evenings):
0.1 to 0.7, then D7.2 with Kunj, then the proofs for the unverified findings:
C6.3 (first half), C8.6, C8.9, A9.1, A9.2, A5.5, A3.6, A6.1.

Phase 2, fixes that need no decision:
B7.1, D1.1, D1.3, C8.10, C10.4, C12.1, C12.2, A10.1, A4.1, A3.4, A5.4, C4.3, C4.4, A8.1, A8.3, A11.3, A11.4, B7.3.

Phase 3, the owner's decisions E1 to E14 in one sitting, using Phase 1 results.

Phase 4, fixes the decisions unblock:
A6.2, A6.3, A6.4, A2.6, A9.3, A9.4, C7.2, C9.2, C8.1 to C8.7, C3.1, C3.2.

Phase 5, native QA passes and evidence per feature (the remaining qa parts),
then F1 to F5.

Phase 6, cleanup and docs: A12.x, A6.7, D4.1, D5.x, B14.1.

The freeze is Friday Oct 16 and Beta is Wed Oct 21. Phases 1 and 2 fit before
the freeze. Phase 4 mostly does not, which is the argument for E12: keep Agent
and Notch Do in Labs for Beta.
