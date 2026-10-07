# PRD: Agent surfaces (Hermes Agent, Screen Guide, Notch Do)

Date: 2026-10-07. Status: draft for owner review.
Companion decision record: [ADR 024](../../decisions/024-agent-surfaces-egress-and-action-policy.md) (Proposed).

This replaces the 2026-10-07 "takeover QA brief". That brief described the three
features from their docs. This PRD is written from the code on `main` at
`81b6cbd`, and it differs from the brief in places that change what to test and
what to decide. Section "What the brief got wrong" lists them.

Nothing here was verified by running the app. Every finding cites the file that
shows it; items marked **verify** are inferences that need a run.

## Problem

Three agent features shipped in two weeks and each made its own choice about
four things that should be decided once: what may leave the Mac, how the person
agrees to that, what proves it afterwards, and which code decides whether an
action runs. The result is that the product says one thing and does another in
several places, and a person taking over the lane cannot tell from the docs
which behavior is intended.

## Goal

One rule for cloud egress, one consent model, one activity log, and one answer
to "what stops an action", applied to all three features. After that, QA
evidence means something, and new capabilities can be added without a new
exception each time.

## Users / actors

- A knowledge worker on a Mac who wants an answer or a small task done, and
  expects FNDR's "local first" promise to hold unless they opted out of it.
- The owner, who decides PD-01 (ADR-018) and takes over polish of this lane.
- The lane owner, who built the features and stays the reviewer for them.
- Third-party runtimes FNDR launches: Hermes gateway, `codex app-server`,
  OpenAI Computer Use or `open-computer-use`.

## Current behavior (from code)

### Hermes Agent

- The page is `src/domains/workspace/AgentWorkspace.tsx`: one chat with
  history, a memory picker, and a setup pane with four providers.
- The header badge is `configured ? "<Provider> · <model>" : "Not set up"`,
  where `configured = hermes.installed && hermes.configured`
  (`AgentWorkspace.tsx:136-140`). It reflects the saved Hermes configuration
  only. ChatGPT sign-in is a separate state owned by `CodexAccountCard`. So
  the screenshot (Signed in, model picked, badge "Not set up") is the expected
  state before Save, not a bug. It still reads as a contradiction.
- Usage bars are the account's Codex rate-limit windows as reported by the
  app-server. They are account-wide, not FNDR's share.
- `send_hermes_message` retrieves up to five memory snippets for **every**
  message and prepends them, then adds any memories the person attached
  (`hermes_agent.rs:1612-1618`, `operator/memory.rs`). This happens for every
  provider, including ChatGPT, OpenRouter and Custom. The page subtitle says
  "FNDR memories you choose as context".
- The gateway is also given FNDR's MCP endpoint and bearer token on each start
  (`write_hermes_mcp_config`), so the model can search memory on its own.
- FNDR writes only a `model:` block and the MCP block to Hermes's
  `config.yaml`. It does not restrict Hermes's own tools. The limits on what
  Hermes may do are sentences in `SOUL.md`, `.hermes.md` and the request
  `instructions` ("Ask before destructive actions..."). **Verify** which
  toolsets the pinned Hermes enables by default (terminal, files, web).
- `config.yaml` (holds the MCP bearer token) and `.env` (holds the OpenRouter
  or custom API key) are written with `std::fs::write` and no explicit mode.
- One `hermes_chat` row is logged per message with the bytes FNDR sent. The
  gateway's own loop (follow-up model calls, MCP results it forwards) is not
  logged.

### Screen Guide

- Matches `docs/product/screen-guide.md` for the local path.
- With the ChatGPT model selected, OCR text goes to ChatGPT, and the
  screenshot too when `send_screenshot_to_codex` is on
  (`screen_guide.rs:2956`, `codex_account::answer_screen_guide_with_codex`).
  This path makes no `record_model_request` call, so it never appears in
  Privacy Activity.
- The "Operate my Mac" switch for Notch Do is stored as
  `config.screen_guide.operate_computer` and shown in
  `screen-guide/OperatorPermissions.tsx`, while the Screen Guide PRD lists
  clicking and typing as a non-goal.

### Notch Do

- Voice or text request, then a plan card, then the run **starts by itself
  after 1.5 s** unless the person says stop or taps Cancel
  (`doRun.ts:14`, `NotchOperator.tsx:211`). There is no approve step for the
  plan as a whole.
- Codex runs with `sandbox: read-only` and `approvalPolicy: on-request`; the
  person's other Codex MCP servers are disabled for the session; every
  computer-use call arrives as an approval request and is classified by
  `operator/policy.rs` into runs, one confirmation, or never
  (`computer_use.rs:260-290, 400-410`). The model's words are not an input.
- Memory snippets are added only when `plan::refers_to_past(transcript)`.
- Plan and step requests are logged to Privacy Activity. Stop kills Codex's
  process group.
- `operator/policy.rs` is a second policy engine beside
  `agent/risk_policy.rs`. Notch Do does not read `actions_kill_switch`. The
  command-surface contract says the kill switch makes every tool refuse.
- Runs are journaled in `operator/journal.rs`, separate from
  `agent/audit.rs` and the SK-01 command journal.
- `open-computer-use` is pinned (`0.3.6`); `@openai/codex` installs unpinned.
  The computer-use binary is resolved from several per-user paths.

### Shared

- Privacy Activity's model log (`privacy_proof.rs`) lives in memory, holds
  200 rows, and is empty after a restart.
- ADR-018 is still Proposed and PD-01 is the owner's decision. Option B was
  accepted for Notch Do and Hermes by the lane owner on 2026-10-06. Screen
  Guide's ChatGPT path shipped on 2026-09-23 and is covered by neither.
- `docs/agent.md` describes a deterministic Ask / Plan / Act / Learn runner.
  The shipped Agent page is Hermes chat. **Verify** whether `AgentPanel.tsx`
  (1,837 lines) is still mounted anywhere.

## What the brief got wrong

| Brief said | Code says | Why it matters |
| --- | --- | --- |
| Notch Do shows an approval, then executes | The plan auto-starts after 1.5 s; approvals are per call, only for the confirm tier | The safety test is "can I stop it in 1.5 s", not "does deny work" |
| Notch Do uses the shared risk policy | It has its own policy module and ignores the kill switch | Re-running the GS-11 tests proves nothing about Notch Do |
| Hermes sends memories the person optionally attaches | It also auto-retrieves five per message, for any provider | This is the largest privacy gap in the lane |
| "Not set up" may be a defect | It is the saved-configuration state | A copy fix, not a bug hunt |
| Cloud-call visibility is an open gap to check | Partly built; Screen Guide's cloud path is the missing caller, and the log does not survive restart | The ticket is specific, not exploratory |
| Tickets were QA only | Five of the findings are product or architecture decisions | QA cannot pass or fail an undecided behavior |

## Proposed behavior

1. **One egress rule.** A cloud model request may carry only: the person's own
   words, memories the person attached, and context a per-feature consent
   names. Automatic retrieval to a cloud provider needs its own switch,
   default off. Local providers (Ollama on loopback) keep automatic retrieval.
2. **One consent model.** Each feature has one named switch whose copy lists
   exactly what leaves the Mac. Choosing a cloud provider in Agent setup shows
   that list before Save. Screen Guide and Notch Do keep theirs.
3. **One activity log.** Every cloud model request from all three features is
   recorded with feature, host, bytes, and whether memories or a screenshot
   were included. It persists across restarts with a bounded retention.
4. **One stop.** The actions kill switch refuses Notch Do runs and Hermes
   sends that could act. Notch Do's policy stays in `operator/policy.rs` but is
   called through the same entry point the kill switch guards.
5. **Honest readiness.** The Agent badge has three states: "Sign in",
   "Choose a model", and "<Provider> · <model>". The usage card says the limits
   are for the whole ChatGPT account and when they were read.
6. **Hermes is bounded in code.** FNDR writes an explicit tool allowlist into
   Hermes's config and gives it an MCP token limited to read tools. Prompt
   sentences stay as a second layer, not the only one.
7. **Notch Do's start is a decision.** Keep the 1.5 s auto-start only for
   plans whose every step is in the runs tier; a plan containing a confirm-tier
   step waits for a tap. (Owner decides; see ADR 024.)

## Non-goals

- New Notch Do capabilities, new providers, or multi-display Screen Guide.
- Replacing Hermes or the Codex app-server.
- Merging the two policy modules' rule tables. Only the entry point and the
  kill switch are shared.
- Changing the "never" tier (send, delete, purchase, secure fields).
- Per-request cost accounting for ChatGPT plans. The account does not expose it.

## Functional requirements

- FR1: With a cloud provider and automatic context off, a Hermes request body
  contains the person's text and attached memories only.
- FR2: The Agent subtitle and setup pane state which of the two memory sources
  are active for the selected provider.
- FR3: Saving a cloud provider shows what will be sent and to which host
  before the configuration is written.
- FR4: Every `answer_screen_guide_with_codex` call records a model request,
  flagged when a screenshot is attached.
- FR5: The model request log is stored in app data, mode `0600`, capped by
  count and age, and readable in Privacy after a restart.
- FR6: A Hermes row states that the gateway may make further calls, or the
  gateway's calls are counted. (Pick one in ADR 024.)
- FR7: With `actions_kill_switch` on, `computer_use` refuses to start a run
  and stops an active one.
- FR8: Hermes's config carries an explicit tool allowlist; a tool outside it
  is not available to the model.
- FR9: The MCP token handed to Hermes cannot call a tool that writes.
- FR10: `config.yaml`, `.env` and `fndr_setup.json` under `hermes-home` are
  written `0600` in a `0700` directory.
- FR11: The badge shows "Sign in", "Choose a model", or the saved provider and
  model; it never shows "Not set up" while an account is signed in.
- FR12: The usage card labels the windows as account-wide and shows the time
  of the last read; a failed refresh shows the stale time, not fresh numbers.
- FR13: The operate switch moves to its own config section with a migration
  that preserves the stored value.
- FR14: A Notch Do plan with any confirm-tier step does not auto-start.
- FR15: `@openai/codex` installs at a pinned version.

## Non-functional requirements

- Performance: persisting a log row adds no awaited I/O to the request path.
- Reliability: a log write failure never fails the request; it is counted.
- Security/privacy: retrieved memory, OCR and accessibility text stay labeled
  as evidence in every prompt that embeds them; no content in any log.
- Accessibility: the plan card's countdown is announced and honors Reduce
  Motion; Stop is reachable by keyboard.
- Maintainability: Hermes's strings move into `inference/prompts.rs` with a
  catalog row and fingerprint, as `AGENTS.md` requires. The catalog currently
  lists them as living in `hermes_agent.rs`.

## Domain language

| Term | Meaning | Existing code/docs |
| --- | --- | --- |
| Agent | The Hermes chat page | `AgentWorkspace.tsx` |
| Attached memory | A memory the person picked for one message | `agent_chats::load_attached_memories` |
| Automatic context | Up to five snippets FNDR retrieves per message | `operator/memory.rs` |
| Model request | One logged cloud call: feature, host, bytes | `privacy_proof.rs` |
| Operate switch | The one-time opt-in for Notch Do | `screen_guide.operate_computer` |
| Plan card | Notch Do's transcript and steps, shown before a run | `NotchOperator.tsx` |
| Run tier / confirm tier / never tier | Notch Do's per-call levels | `operator/policy.rs`, ADR-022 amendment |
| Kill switch | Settings control that refuses every action | `config.actions_kill_switch`, GS-11 |

## Affected modules and interfaces

| Module | Change | Interface impact | Tests |
| --- | --- | --- | --- |
| `ipc/commands/hermes_agent.rs` | Gate automatic context by provider and switch; file modes; tool allowlist; move strings | `HermesBridgeStatus` gains context-source fields | Request-body tests per provider |
| `privacy_proof.rs` | Persist model requests; add `included` flags | `ModelRequest` additive fields | Restart, cap, no-content |
| `ipc/commands/codex_account.rs` | Record Screen Guide cloud calls | None | Call-site test |
| `ipc/commands/computer_use.rs` | Read kill switch at start and during a run | New refusal reason | Refuse and stop cases |
| `config.rs` | `operator` section, migration | Additive TOML, old key read once | Migration |
| `mcp/` | Read-only token scope for Hermes | Token carries a scope | Write tool refused |
| `AgentWorkspace.tsx`, `CodexAccountCard.tsx` | Badge states, usage copy, pre-save disclosure | None | Existing Vitest files |
| `NotchOperator.tsx`, `doRun.ts` | Auto-start only for run-tier plans | Plan carries a tier summary | `doRun.test.ts` |
| `docs/agent.md`, `screen-guide.md`, `command-surface.md` | Rewrite or amend to match | None | None |

## Data flow

```text
Agent message
  -> attached memories (always)
  -> automatic context? local provider: yes | cloud: only if switch on
  -> log model request (feature, host, bytes, included)
  -> Hermes gateway -> provider
        gateway tools: allowlist only; fndr MCP: read scope only

Screen Guide turn (ChatGPT selected)
  -> privacy gate -> capture -> OCR
  -> log model request (screenshot: yes/no) -> Codex app-server

Notch Do
  -> kill switch? refuse
  -> transcript -> plan (snippets only if it refers to the past) -> log
  -> plan card: all run tier -> 1.5 s auto-start | else wait for tap
  -> each call -> operator::policy -> run / confirm / never -> journal
  -> Stop or kill switch -> kill process group
```

## Acceptance criteria

- [ ] A captured Hermes request to a cloud provider with the switch off holds
      no snippet block (unit test on the built body).
- [ ] The same request to Ollama on loopback holds up to five snippets.
- [ ] A Screen Guide ChatGPT turn adds one row to Privacy Activity.
- [ ] Rows from before a restart are listed after it.
- [ ] With the kill switch on, a Notch Do request returns a refusal and no
      Codex process starts; switching it on mid-run ends the run.
- [ ] A tool outside the allowlist is absent from Hermes's tool list.
- [ ] The Hermes MCP token is refused on `fndr.remember`.
- [ ] `stat` shows `0600` on the three Hermes files.
- [ ] Signed in and unsaved shows "Choose a model".
- [ ] A plan with a confirm-tier step shows no countdown.
- [ ] `docs/agent.md` describes the page that ships.

## Test plan

Automated first, then one native pass per feature on a synthetic profile.

1. Focused Vitest: `AgentWorkspace.test.tsx`, `CodexAccountCard.test.tsx`,
   `doRun.test.ts`, `NotchHud.test.tsx`, the two Screen Guide tests.
2. Focused Rust, serial: `operator::`, `ipc::commands::computer_use`,
   `ipc::commands::hermes_agent`, `privacy_proof`.
3. Native, synthetic screen, no real account data in evidence:
   - Agent: signed out, signed in unsaved, saved, refresh failure, sign-in
     cancelled, gateway crash and restart, each provider once.
   - Screen Guide: allowed, blocked, blank frame, local and ChatGPT answer,
     screenshot consent on and off, diagnostic arm and delete.
   - Notch Do: stop during listening, during the 1.5 s card, during a step;
     one run-tier task; one confirm-tier task denied; one never-tier request.
4. Evidence goes in `docs/evidence/W03/` with the email, tokens and chat
   content removed. The screenshot in the brief shows an account email and
   should not be committed as is.

## Rollout / migration plan

Automatic context to cloud providers changes from on to off. Existing installs
with a cloud provider get a one-time notice in the Agent page explaining the
switch. The config key move reads the old key once. The persisted log starts
empty. No vector or schema change.

## Risks

- Turning automatic context off for cloud providers will make Hermes answers
  about the past worse until the person attaches memories or turns it on.
- A Hermes tool allowlist may break a workflow the lane owner relies on;
  agree the list with them before shipping.
- The gateway's internal calls may not be observable from FNDR; if so FR6
  becomes a disclosure, not a count.
- Narrowing ADR-018's amendment after features shipped needs a clear owner
  decision so the lane is not left half-approved.

## Open questions (owner)

1. Ratify, narrow, or revert the ADR-018 amendment, and does it cover Screen
   Guide's ChatGPT path?
2. Automatic context to cloud providers: off by default (recommended), or on
   with disclosure?
3. Notch Do auto-start: keep for run-tier plans only (recommended), keep for
   all, or always require a tap?
4. Is Hermes meant to act (terminal, files) or to answer? The allowlist
   follows from this.
5. Does the Agent page stay under Labs (ADR-023) until these land?

## Suggested issues

Superseded by the [work breakdown](../plans/2026-10-07-agent-surfaces-work-breakdown.md), which splits these into 184 small parts and adds fourteen findings (N1 to N14) from a second pass through the code. The table below is kept as the short version.

| ID | Slice | Check | Blocked by |
| --- | --- | --- | --- |
| AS-01 | Log Screen Guide ChatGPT requests in Privacy Activity | Rust call-site test | none |
| AS-02 | Persist the model request log with a cap | Restart test | none |
| AS-03 | Notch Do honors the kill switch at start and mid-run | Rust refuse and stop tests | none |
| AS-04 | Write Hermes files `0600`; pin `@openai/codex` | Mode test | none |
| AS-05 | Agent badge states and account-wide usage copy | Vitest | none |
| AS-06 | Gate automatic context by provider and switch; fix subtitle; pre-save disclosure | Request-body tests, Vitest | Q2 |
| AS-07 | Read-only MCP scope for Hermes | MCP test | none |
| AS-08 | Inventory Hermes default tools, then write an allowlist | Recorded tool list | Q4 |
| AS-09 | Auto-start only for run-tier plans | `doRun.test.ts` | Q3 |
| AS-10 | Move the operate switch to its own config section | Migration test | none |
| AS-11 | Move Hermes strings to `prompts.rs`, catalog rows | Fingerprint test | none |
| AS-12 | Rewrite `docs/agent.md`; amend Screen Guide PRD and command-surface; remove `AgentPanel.tsx` if unmounted | Docs review, typecheck | none |
| AS-13 | Native QA pass and evidence for all three features | Evidence files | AS-01 to AS-05 |
| AS-14 | Dogfood measures: task success, stop latency, time to first answer, per feature | One week of rows | AS-13 |

AS-01 to AS-05 need no decision and can start now.
