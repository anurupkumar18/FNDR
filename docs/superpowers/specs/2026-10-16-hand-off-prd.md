# PRD: Hand-off

**Status:** Approved by the owner 2026-10-16. Spikes S1 to S5 measured the same day ([evidence](../../evidence/W05/hand-off-spikes.md)); S6 is pending. Decision record: [ADR 032](../../decisions/032-hand-off-sandboxed-worker.md). Work breakdown: [2026-10-16 Hand-off work breakdown](../plans/2026-10-16-hand-off-work-breakdown.md).

Read against `origin/main` at `2b39c01` (2026-10-15). The shared checkout is 277 commits behind and was not used as the source of truth.

## One sentence

A person says "do the readout Dana asked for", FNDR finds the request, the survey file and the notes on this Mac, shows exactly what will be sent, and after one tap a sandboxed agent works for a few minutes and returns a finished document whose every claim FNDR has checked against its sources.

## Problem

FNDR remembers work and can reopen it (work sets, ADR 027), act on the Mac step by step (Notch Do, ADR 026) and answer questions (Ask, Hermes). It cannot **finish a piece of work**. The commitments it already finds on screen (`tasks/suggest.rs`: "Dana asked Sam to own a one-page Q4 onboarding survey readout with the top three churn drivers by Thursday") end as a to-do row. The person still opens the CSV, does the analysis, writes the doc and checks it.

Every surface that could do this today is blocked on purpose:

| Surface | Why it cannot finish work | Where it is decided |
| --- | --- | --- |
| Notch Do | Operates apps one action at a time; never runs model-written code | ADR 026 item 1 |
| Hermes (Agent page) | Answers only: a planning list and four read-only memory tools, no terminal or files | ADR 024, ADR 025 part 1, `docs/agent.md` |
| A2A peers | Remote; only reviewed text and memory summaries leave; artifacts are text | ADR 025 |
| MCP clients (Claude Code) | Outside FNDR's control; FNDR cannot show what the client did | ADR 017 |

## Goal

From one request, one reviewed draft and one tap, a finished deliverable (document, spreadsheet, slides or brief) lands in a folder the person owns, with:

1. the request, the attached memories and files listed before anything leaves the Mac;
2. the agent unable to read, write or reach anything except its job folder, at the operating-system level, not by instruction;
3. live, plain progress while it works, and Stop that always works;
4. every claim in the result tagged with its source and checked by FNDR's code, not the model's word;
5. one line in Privacy Activity saying what left, to where, and how many bytes.

## Why this one (the choice, in short)

Five integrations were weighed against six criteria. Scores are 1 to 5; the reasoning for each cell is in ADR 032.

| Option | Demo impact | Uses FNDR's memory | Fits accepted ADRs | Setup on the demo Mac | Live reliability | New code | Total |
| --- | --- | --- | --- | --- | --- | --- | --- |
| **A. Hand-off: Codex app-server as a sandboxed worker (chosen)** | 5 | 5 | 5 | 5 (sign-in exists) | 4 | 3 | **27** |
| B. Browser Use as a web agent | 4 | 3 | 2 (ADR 026) | 3 | 2 (live sites, sign-ins, CAPTCHAs) | 3 | 17 |
| C. GPT Researcher as a research sidecar | 3 | 3 | 3 | 2 (two new API keys) | 3 | 2 | 16 |
| D. Hermes with terminal and file tools | 4 | 4 | 1 (reverses ADR 024/025) | 4 | 3 | 4 | 20 |
| E. Claude Code headless as the worker | 5 | 5 | 4 | 2 (second account and sign-in) | 4 | 3 | 23 |
| F. OpenAdapt, learn a workflow by demonstration | 5 | 3 | 1 (needs input event recording) | 2 | 1 | 1 | 13 |

A wins because the rest of FNDR already pays for it: the ChatGPT sign-in, the JSON-RPC client (`codex_account::AppServer`, four callers), the egress log, the delegation draft (ADR 025 part 3) and the work-set resolver (ADR 027). The new parts are the ones a judge sees: a sandboxed long-running worker, live progress, and checked citations. Codex CLI is open source (Apache-2.0); this is a new integration mode of it, not the planner turn Notch Do uses. E stays possible later behind the same job record; it is not built now.

## Users and actors

- **The person** at the Mac: knowledge worker (school, office, project work). Starts, reviews, stops and keeps.
- **FNDR**: resolves context on the Mac, renders the draft, owns the sandbox profile, runs the job, checks the result, logs egress.
- **The worker**: one Codex app-server thread on the person's ChatGPT plan, in a job folder, under a FNDR-written permission profile. Untrusted.
- **The web**: reached only by the worker's model-side web search, never by its shell.

## Current behavior (what exists and is reused)

| Need | Already exists | File |
| --- | --- | --- |
| Codex JSON-RPC over stdio | `AppServer::spawn_with`, `request` | `ipc/commands/codex_account.rs:106` |
| Sign-in on the ChatGPT plan | Codex app-server owns tokens in `~/.codex/auth.json` | `codex_account.rs` header |
| Find the thread of work | `workset::resolve(state, query)` | `workset/mod.rs:229` |
| Find past references in a request | `plan::refers_to_past`, `plan::asks_for_work_set` | `operator/plan.rs:538`, `:557` |
| A reviewable, exact payload | `DelegationPreview`, `DelegationAttachment` | `agent/delegation.rs` |
| Source visibility at send time | `memory_is_visible`, `memory_is_permitted` | `context_runtime/retrieve.rs` |
| Commitments seen on screen | `Suggestion { title, quote }` | `tasks/suggest.rs:21` |
| Egress log, persisted, 0600 | `record_model_request_including(Feature, host, bytes, kinds)` | `privacy_proof.rs:126` |
| Ratings and learning loop | `operator::runs::Rating`, Settings > AI > Notch Do | `operator/runs.rs`, `NotchDoSettings.tsx` |
| Spoken progress lines from state | `doNarration.ts` | `src/domains/notch/` |
| Python runtime install pattern | uv venv in app data for Hermes | `hermes_agent.rs:508` |
| Agent notes with provenance | `WriteAgentNote`, `decide_mcp_write` | `agent/policy.rs`, `agent/risk_policy.rs` |

## Proposed behavior

### The flow

```text
"do the readout Dana asked for"          (notch, command bar, or To-dos "Hand off")
  -> plan::asks_for_hand_off              decided on this Mac, no model
  -> handoff::draft                       resolve context on this Mac, no egress
       task text (the person's words, or the suggestion's quote)
       output goal (Doc | Spreadsheet | Slides | Brief), inferred, editable
       attachments: up to 8 memories, up to 5 files, each removable
       destination: "OpenAI, on your ChatGPT plan", limits, what it cannot do
  -> person taps Hand off (or says "go")
  -> handoff::start                       recheck sources, copy files to inputs/,
                                          verify sandbox profile, log egress, start turn
  -> live progress                        typed events -> trace lines + spoken milestones
  -> handoff::verify                      FNDR checks files and every citation
  -> finished card                        Open, Sources, Revise, Keep in Vault, rate
```

### Entry points, in build order

1. **To-dos.** A source-backed suggestion row gets a "Hand off" button. Its draft attaches the suggestion's memory and the work set of its thread. This is the most reliable entry: the task text is a quote FNDR already checked.
2. **Notch and command bar.** `plan::asks_for_hand_off(request)` recognizes "hand this off", "take it from here", "do the X for me", and "make, write, draft, prepare, put together" followed by a deliverable noun (readout, report, summary, brief, deck, slides, spreadsheet, table, comparison, plan, memo, draft). A match skips the Codex planner turn entirely, the way a work-set request does.
3. **Home, later wave.** "Hand off the next step" on a Resume thread. Not in the first release.

### The draft card

```text
+-----------------------------------------------------------+
| Hand off                                          [x]     |
|                                                           |
| "Own a one-page Q4 onboarding survey readout with the     |
|  top three churn drivers by Thursday"                     |
|   from Mail, Tue 14:02                                     |
|                                                           |
| Make:  (Doc)  Spreadsheet  Slides  Brief                  |
|                                                           |
| Sends to OpenAI on your ChatGPT plan                      |
|   [x] Mail: Dana, "Q4 onboarding readout"         memory  |
|   [x] Notes: churn hypotheses                      memory  |
|   [x] q4-onboarding-survey.csv  48 KB              file    |
|   [ ] Safari: Typeform results page                memory  |
|   + Add from Vault                                        |
|                                                           |
| It works in ~/Documents/FNDR Hand-offs/Q4 readout.        |
| It can search the web. It cannot open apps, read other    |
| files, send anything, or reach FNDR.        Up to 15 min  |
|                                                           |
|                          [ Not now ]   [ Hand off ]       |
+-----------------------------------------------------------+
```

Rules for the card:

- The quoted task is the person's words or the checked suggestion quote, never a model paraphrase.
- Candidates beyond the cap are listed unticked, never silently dropped.
- A file appears only when a visible memory's reopen target is a local file of an allowed type and size (see FR4). A file is copied into `inputs/`; the original is never opened for writing.
- The byte estimate of what will be sent is shown under the destination ("about 14 KB to start").
- With the actions kill switch or Private Mode on, the card says which one and offers no Hand off button.

### While it works

The notch shows a compact live pill; the Hand-offs list shows the full trace.

```text
Notch pill:   Q4 readout  .  working 2:14  .  7 steps        [Stop]

Trace (Agent > Hand-offs > Q4 readout):
  09:41:02  Started with 2 memories and 1 file              FNDR
  09:41:09  Planned 5 steps                                  worker
  09:41:20  Read q4-onboarding-survey.csv                    worker
  09:41:48  Ran analysis code                                worker, sandbox
  09:42:15  Searched the web, 2 searches                     worker, OpenAI
  09:43:30  Wrote out/q4-readout.docx                        worker, sandbox
  09:43:41  Checked 9 claims against their sources           FNDR
```

- Trace lines are composed in code from typed events (`plan`, `commandExecution`, `fileChange`, `webSearch`), each naming its actor. No reasoning text, no prediction of remaining time, no percentage (ADR 021). Search queries and command text are in a Details drawer, not the trace.
- Spoken milestones, composed in code like `doNarration.ts`: "Handing it off", "Reading the survey", "Writing the readout", "The readout is ready". At most one line per 20 seconds; mute and Stop silence it (ADR 020 amendment).
- Stop is always live. Stopping interrupts the turn, confirms the interrupt, and keeps what is already in the folder, marked "stopped".

### The result

```text
+-----------------------------------------------------------+
| Ready: Q4 onboarding readout                     3m 41s   |
|   q4-readout.docx    1 page                     [Open]    |
|   churn-by-segment.png                          [Open]    |
|                                                           |
| 9 claims. 8 checked against their sources, 1 without one. |
| Not done: "Thursday deadline not confirmed in the notes"  |
|                                                           |
| Sent 16 KB to chatgpt.com: your words, 2 memories, 1 file |
|                                                           |
| [Sources] [Revise] [Keep in Vault]   Did that work? [Y][N]|
+-----------------------------------------------------------+
```

- **Open** opens the file in its default app on a tap only. FNDR never opens a deliverable by itself.
- **Sources** lists each claim with its sources: a memory (opens it in the Vault), an input file and its locator (row range, page), or a web page (the URL, opened on tap). A source FNDR could not check is shown as "not checked" with the reason.
- **Revise** takes one typed or spoken instruction and runs a second turn on the same thread in the same folder. After FNDR restarts, Revise starts a new job with the previous outputs as inputs.
- **Keep in Vault** saves the summary and the deliverable paths as an agent note with provenance `hand-off:<job id>`, through the existing agent-note path. Nothing is imported automatically (ADR 025 item 5).
- **Did that work?** stores a rating on the job, the same `Rating` the Notch Do learning loop uses. A "No" offers to save the job as a draft eval case, with the inputs referenced by id, never copied.

### States

| State | Shown as | Leaves by |
| --- | --- | --- |
| Draft | Draft card | Hand off, Not now |
| Starting | "Starting" with a spinner, under 5 s normally | First event, or a refusal |
| Working | Pill and trace | Turn completes, Stop, timeout, error |
| Stopping | "Stopping" | Interrupt confirmed, or the process killed after 5 s |
| Checking | "Checking 9 claims" | Verifier returns |
| Ready | Result card | Revise, Keep, rate |
| Ready with gaps | Result card with "Not done" and unsupported counts first | Same |
| Stopped | "Stopped. 2 files were written before Stop." | Revise, delete folder |
| Failed | One sentence of reason and the next step | Try again, which makes a new draft |

Failure sentences, each with its next step: not signed in to ChatGPT ("Sign in to ChatGPT in Settings > AI"); plan limit reached ("Your ChatGPT plan's limit is reached until 14:00"); the sandbox profile could not be confirmed ("Hand-off is off on this Mac: Codex did not confirm the sandbox"); timeout ("Stopped after 15 minutes; what it wrote is kept"); toolchain missing ("Install the document tools (about 200 MB)"); no deliverable ("It finished without writing a file").

## Non-goals

- Sending, posting, emailing or uploading anything on the person's behalf. A reply can be a text file the person copies; Notch Do's paste step can paste it on a tap.
- Operating apps or the browser. The worker has no computer-use or browser tools.
- Reading anything on the Mac that the person did not attach.
- Background or scheduled hand-offs. Every job is a turn a person starts (ADR 018 decision B).
- Code repositories. Developer work already has Claude Code and Codex with their own review flows.
- Multiple worker vendors in the first release.
- Importing results into Memory without the person's Keep.

## Functional requirements

- **FR1. Start is the person's.** No byte leaves the Mac before the person taps Hand off or says "go" on a visible draft.
- **FR2. The payload is the draft.** The rendered prompt sent in `turn/start` is byte-for-byte the text the draft rendered, plus the input files listed on it. A golden test compares them.
- **FR3. Sources are rechecked at start.** Every attached memory goes through the visibility rule (`memory_is_visible`, `memory_is_permitted`) at start. A source removed, soft-deleted, newly blocklisted or now private blocks the start and the card says which one.
- **FR4. Files.** At most 5 files, 25 MB each, 60 MB total; types `csv`, `tsv`, `xlsx`, `docx`, `pptx`, `pdf`, `md`, `txt`, `json`. Copied into `inputs/` with the original name, after a check that the path still matches the memory's reopen target and the file is a regular file, not a link.
- **FR5. Sandbox, fail closed.** The worker process starts with FNDR's permission profile: read `:minimal`, the toolchain and the job folder; write the job folder only; shell network off. FNDR reads the effective config back (`config/read`) before the first turn and refuses the job on any mismatch. A reported policy is not proof (S1: `thread/start` reported `/tmp` excluded while it was open), so the gate is the probe self-test (FR15), run through `command/exec` with no model. Known gap: `/private/tmp` stays readable and writable under every profile tried; FNDR keeps nothing there (its temp files go to `$TMPDIR`, which is denied) and the self-test reports the gap so a Codex change that closes it is noticed.
- **FR6. Nothing else attached.** No MCP servers, plugins, apps, hooks, computer use, browser use, Codex memories, image generation or the person's own Codex skills. Feature switches alone are not enough (S1, S3): FNDR reads the person's MCP servers, plugins and skills from `config/read` and `skills/list`, disables each by name for the worker process, and refuses the job when a name cannot be written as a bare `-c` key (a dot in it) or still reads enabled. Codex's own state and logs for the job go to a per-job folder in FNDR's app data (`sqlite_home`, `log_dir`, `history.persistence="none"`) that FNDR deletes when the job ends, because an ephemeral thread still writes its prompt to `~/.codex/logs_2.sqlite` (S2). The thread is ephemeral. A test asserts each from the effective config.
- **FR7. Approvals never reach the person.** `approvalPolicy: never`: a sandbox escape request is denied, never routed to a prompt.
- **FR8. Limits.** 15 minutes wall clock, 3 minutes without any event, 60 MB folder, 40 files in `out/`, 200 commands. Hitting one interrupts the turn and keeps the folder.
- **FR9. Stop.** Stop interrupts, confirms with `turn/completed` status `interrupted`, and kills the process 5 s later if needed. Quitting FNDR stops every job the same way. A job found `working` at launch is marked `stopped`.
- **FR10. Manifest.** The final message follows the manifest JSON schema (`outputSchema`): deliverables, claims with sources, not done, assumptions. A turn that ends without a valid manifest is "Ready with gaps" if files exist, else "Failed".
- **FR11. Verification is FNDR's.** A deliverable must be inside `out/`, exist, be of an allowed output type (`docx`, `xlsx`, `pptx`, `pdf`, `md`, `txt`, `csv`, `png`) and parse as that type. A memory source must be an attached id. A file source must be under `inputs/` or `out/`. A web source must be a URL that appeared in this job's `webSearch` events. A claim without a source is "unsupported". The counts on the result card come from this check only.
- **FR12. Egress is logged.** One `HandOff` row per turn in the model request log: host, bytes sent (prompt plus every command output the worker read back, which is what reaches the model), kinds (`words`, `memories`, `files`, `web_search`). Not the content.
- **FR13. Kill switch and Private Mode.** Both refuse a draft and a start. Private Mode turning on during a job stops it.
- **FR14. Job record.** Each job is kept in FNDR's state store under `hand_off_jobs_v1`, like peer runs (newest 50, at most one running): id, created, request, goal, attachment ids, folder, status, timings, bytes, verification counts, rating. No memory text, no file content.
- **FR15. Off by default.** Hand-off is a Labs feature, off until turned on in Settings > AI, and offered only when ChatGPT sign-in is present and the sandbox self-test passed once on this Mac.
- **FR16. The plan's limit is checked before a job.** Before start, FNDR reads `account/rateLimits/read`. At 100 percent of the 5-hour or weekly window the draft says when it resets and offers no Hand off button, so a job never spends purchased credits without a separate, explicit choice. The model is the account's default from `model/list`, never the `model` in the person's `config.toml` (S1: it named a model the ChatGPT plan refuses).

## Non-functional requirements

- **Performance.** Draft in under 1.5 s at 10,000 memories (one `workset::resolve` plus visibility checks; no model). Start to first event under 5 s p95. A typical job is 2 to 6 minutes; measured on the M1 8 GB in the eval.
- **Memory.** One extra Codex app-server process per job, at most one job at a time in the first release. Measured RSS is recorded in the evidence file; no local model is loaded for Hand-off.
- **Reliability.** A job survives the notch closing and the main window closing. It does not survive FNDR quitting (FR9).
- **Security and privacy.** Enforced by the OS sandbox, the effective-config check and FNDR's verifier; prompt text is a second layer, never the first.
- **Accessibility.** Every control is a button with a label; the trace is a list with time, text and actor; voice can say "go", "stop", "open it", "revise: make it shorter".
- **Maintainability.** The profile builder, the event mapper, the verifier and the narration are pure functions with fixture tests; the process client is reused, not copied.

## Domain language

| Term | Meaning | Existing code or doc |
| --- | --- | --- |
| Hand-off | A person-approved job where a sandboxed worker turns a task plus attached memories and files into finished files in one job folder | New, `src-tauri/src/handoff/` |
| Hand-off draft | The delegation draft with the local worker as destination: task, output goal, attachments, destination, limits | Extends `agent/delegation.rs` (ADR 025 "Delegation draft") |
| Worker | The Codex app-server thread that does the job; untrusted | New use of `codex_account::AppServer` |
| Job folder | `~/Documents/FNDR Hand-offs/<date> <title>/` with `inputs/`, `work/`, `out/` | New |
| Hand-off profile | The Codex permission profile FNDR writes for a job | New, `handoff/profile.rs` |
| Manifest | The worker's final structured message: deliverables, claims, sources, not done | New, `handoff/manifest.rs` |
| Checked claim | A claim whose every source passed FNDR's verifier | New, `handoff/verify.rs` |

"Hand-off" (the feature) is distinct from "handoff" (the session notes under `docs/handoffs/`). Code uses `handoff` as the module name; UI copy says "Hand off" for the verb and "Hand-off" for the noun.

## Affected modules and interfaces

| Module | Change | Interface | Tests |
| --- | --- | --- | --- |
| `handoff/` (new) | Draft, profile, folder, session, events, manifest, verify, store | `draft`, `start`, `stop`, `revise`, `keep`, `list`, `get` | Pure fixtures; fake app-server; one real run |
| `ipc/commands/codex_account.rs` | `AppServer` gains notification streaming and `interrupt`; no behavior change for current callers | `next_notification`, `interrupt` | Existing tests unchanged; new stream test |
| `agent/delegation.rs` | A destination enum: `Peer` (today) or `LocalWorker` | `DelegationPreview.destination` | Existing peer tests unchanged; worker preview golden |
| `operator/plan.rs` | `asks_for_hand_off` | pure fn | Phrase table, negatives (work sets, skills, plain search) |
| `privacy_proof.rs` | `Feature::HandOff` | enum value | Log row test |
| `inference/prompts.rs` | Worker instructions and manifest schema, versioned | `HAND_OFF_PROMPT_VERSION` | Fingerprint test; `llm-task-catalog.md` row |
| `src/domains/handoff/` (new) | Draft card, pill, trace, result, list | IPC wrappers | Component tests, preview fixtures |
| `src/domains/workspace/TodoPanel.tsx` | "Hand off" on a suggestion | none | Component test |
| `src/domains/notch/` | Hand-off states in the notch | none | NotchHud tests |
| Settings > AI | Hand-off section: on/off, web search on/off, toolchain install, self-test | IPC | Component test |
| `src/dev/memoryReadDrift.test.ts` | Lists `handoff/draft.rs` with its visibility check | baseline row | Existing test |

## Acceptance criteria

- [ ] The sandbox self-test passes on the M1 8 GB through `command/exec`, with the 17 probes of S1: the home folder, the Codex sign-in, FNDR's app data and MCP token, a symlink out of the folder, the clipboard, AppleScript, opening an app and `curl` are each denied, while writing in `out/`, the toolchain's Python and system reads succeed; `/private/tmp` is reported as the known gap. A web search succeeds with shell network off (S6).
- [ ] The rendered draft equals the sent prompt byte for byte (golden test).
- [ ] A memory blocklisted after it was attached blocks the start (test on a seeded store).
- [ ] Stop during a command confirms within 5 s and keeps the folder; quitting FNDR leaves no Codex process behind (native check).
- [ ] On the 12-task eval (work breakdown, part 11): at least 10 Ready, 0 sandbox escapes, citation validity at least 95 percent, gold facts present at least 90 percent, forbidden claims 0, p95 under 8 minutes on the M1.
- [ ] On the 8-case red-team set: every case refused, denied or reported as written in the set, and each denial visible in the trace.
- [ ] Privacy Activity shows one Hand-off row per turn with bytes and kinds, after a restart.
- [ ] The demo beat below runs end to end on the demo Mac three times in a row on a seeded profile.

## The demo beat (90 seconds)

| Time | What happens | What it proves |
| --- | --- | --- |
| 0:00 | To-dos shows "Own a one-page Q4 onboarding survey readout... by Thursday", quoted from Mail | FNDR saw the commitment |
| 0:08 | Tap Hand off. The draft lists the mail, the notes and the CSV, and says what it cannot do | Nothing has left yet; the person sees the payload |
| 0:15 | Tap Hand off. The notch pill starts; the voice says "Handing it off" | One tap |
| 0:20 to 2:30 | The trace fills: planned, read the survey, ran analysis, searched the web, wrote the readout. Meanwhile the presenter shows Privacy Activity and the sandbox line "Read outside its folder: denied" from the self-test | Long-horizon autonomy, visible and bounded |
| 2:30 | "The readout is ready." Open: a one-page doc with a chart and three drivers | Finished work, not an answer |
| 2:45 | Sources: each claim links to the CSV rows, the mail, or a web page it searched; one claim marked "without a source" | FNDR checks the agent |
| 3:00 | Revise: "add a sentence for the exec summary"; Keep in Vault | It iterates, and memory grows from work done |

A recorded fallback run is kept for the stage in case the network fails (month plan section 10: a feature that cannot run end to end is removed from the demo, not simulated; the recording is labeled as a recording).

## Test plan

Summarized here; the parts and their gates are in the work breakdown.

1. **Pure units:** profile builder, effective-config checker, event mapper, verifier, narration, detector, folder naming, caps.
2. **Fake app-server:** a small binary replaying recorded JSONL notification streams (completed, interrupted, failed, rate-limited, malformed manifest, no manifest, escape denied).
3. **Seeded store integration:** draft resolution and visibility on the knowledge-worker persona.
4. **Real Codex, synthetic inputs only:** the self-test, one job per output type, the eval and red-team sets. Never the owner's vault.
5. **Browser preview:** every state of the cards from fixtures.
6. **Native check:** batched with the next owner session, following `reference_native_check_recipe` (seed with `--pause-capture`, keys only when FNDR is frontmost).

## Rollout

Labs, off by default (FR15). Turned on for the demo profile only after the eval gate. Promoted out of Labs only after a week of the owner's own use with ratings, decided by the owner.

## Risks

| Risk | Likelihood | Effect | Mitigation |
| --- | --- | --- | --- |
| Codex changes the app-server protocol or permission profile keys | Medium | Job cannot start | Pin the Codex version range the self-test passed; fail closed on config mismatch; the fake server keeps tests independent |
| Web results inject instructions | High | A wrong or odd claim | Worker can only write files in its folder; FNDR verifies sources; claims without checked sources are flagged; prompt states the evidence boundary |
| ChatGPT plan limits during the demo | Medium | Job fails on stage | Rate-limit event shown plainly; rehearse with plan headroom; recorded fallback |
| 8 GB memory pressure while capture runs | Medium | Slow job | No local model for Hand-off; measured RSS; at most one job |
| Folder under `~/Documents` triggers a TCC prompt for FNDR or Codex | Unknown | Prompt on stage | Measured in spike S4; fallback folder under app data with Reveal in Finder |
| Toolchain size on a small disk | Medium | Install refused | Measured 199 MB plus a deletable 162 MB cache (S5); install only on enabling; uninstall removes it |
| A worker reads other programs' files in `/private/tmp` | Medium | Their content can reach the model | Codex's base policy leaves it open (S1); FNDR stores nothing there; the gap is reported by the self-test and named in the consent copy's details; reported upstream |
| A job spends purchased credits when the plan window is full | Medium on a busy plan | Cost to the person | FR16 |
| Tokens per job | High | Plan window fills fast | S1 used 115,000 tokens for a 34-second turn, mostly cached; the eval reports tokens per job |
| Quality looks like a toy | Medium | Weak demo | The eval set with gold facts decides; FNDR skills with tested templates for each output type |

## Open questions for the owner

Each has a default the build follows unless the owner says otherwise.

1. Job folder: `~/Documents/FNDR Hand-offs/` (default) or app data with Reveal in Finder.
2. Web search: `cached` (default, smaller injection surface) or `live`; switched only if the eval shows a gain.
3. After Beta, Claude Code as a second worker destination behind the same job record: yes or no.

## GRILL WITH DOCS RESULT

```text
Shared understanding:
- A new class of job: finished files from a person-approved draft, done by an
  untrusted worker in an OS sandbox, checked by FNDR.
Domain terms to use:
- Hand-off, Hand-off draft, Worker, Job folder, Hand-off profile, Manifest,
  Checked claim. Reuse Delegation draft (ADR 025) rather than a new draft type.
Affected modules/interfaces:
- handoff/ (new), codex_account::AppServer (streaming, interrupt),
  agent/delegation.rs (destination), operator/plan.rs (detector),
  privacy_proof.rs (Feature), prompts.rs, To-dos, notch, Settings > AI.
Decisions resolved:
- Engine: Codex app-server, not Browser Use, GPT Researcher, Hermes tools,
  Claude Code or OpenAdapt (ADR 032).
- Sandbox enforced by a Codex permission profile, read back and verified.
- Resolution on the Mac; nothing leaves before the tap.
- Results are untrusted; Keep is explicit.
Open blockers:
- None for parts 0 to 6. Spikes S1 to S6 can change part 7 (toolchain) and the
  folder location.
CONTEXT.md updates needed:
- Add Hand-off, Hand-off draft, Job folder, Manifest, Checked claim.
ADR updates needed:
- title: ADR 032, Hand-off runs a sandboxed worker on the Mac
- ADR 025: the delegation draft gains a local worker destination.
Recommended next step:
- prototype (spikes S1 to S6), then tdd part by part.
```
