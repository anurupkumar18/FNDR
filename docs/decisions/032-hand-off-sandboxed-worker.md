# ADR 032: Hand-off runs a sandboxed worker on the Mac

## Status

**Proposed 2026-10-16.** Drafted under the owner's delegation of product and architecture decisions (2026-10-08); it becomes Accepted when the owner approves the plan, and a row is added to `docs/team/decision-log.md` in that change.

- Requirements: [Hand-off PRD](../superpowers/specs/2026-10-16-hand-off-prd.md)
- Parts and gates: [work breakdown](../superpowers/plans/2026-10-16-hand-off-work-breakdown.md)

## Question

Which open-source project or agentic product should FNDR integrate so that a person can hand a remembered piece of work to an agent and get finished work back, and what boundary keeps that consistent with ADR 018 (person-started cloud turns), ADR 024 (one egress rule), ADR 025 (peer results are untrusted) and ADR 026 (FNDR never runs a tool that executes model-written code on the Mac)?

## What is true today

Read from `origin/main` at `2b39c01`.

- FNDR finds commitments on screen with a checked quote (`tasks/suggest.rs`), resolves a thread of work on the Mac with no model (`workset::resolve`, ADR 027), renders an exact reviewable payload (`agent/delegation.rs`, ADR 025 part 3) and logs each cloud request with bytes and kinds, persisted with mode 0600 (`privacy_proof::record_model_request_including`).
- FNDR speaks JSON-RPC to `codex app-server` through one client, `codex_account::AppServer`, used by sign-in, Screen Guide, Notch Do (`computer_use.rs`), Hermes's login refresh and spoken lines (`speech_out`). Every current thread is `sandbox: read-only`.
- No surface can produce a file. Hermes is answer-only by decision (ADR 024, ADR 025 part 1). Notch Do acts on apps step by step and refuses model-written code (ADR 026).

### Measured on 2026-10-16 (Codex CLI 0.151.0, this Mac)

`codex app-server generate-json-schema` shows:

- `turn/start` takes `sandboxPolicy` with `workspaceWrite { writableRoots, networkAccess: false }`, `approvalPolicy` (`never` available), `outputSchema` (a JSON Schema for the final message) and `effort`.
- `thread/start` takes `cwd`, `ephemeral`, `developerInstructions` and a `config` object.
- Thread items stream as typed notifications: `plan`, `reasoning`, `commandExecution` (command, exit code, duration, output), `fileChange`, `webSearch` (query and results), `subAgentActivity`, `contextCompaction`.
- Permission profiles (`[permissions.<name>]`, `default_permissions`) take filesystem entries `read`, `write` or `deny` per path, including the special `:minimal`, and a network table.

A spike ran `codex sandbox` (the same Seatbelt policy the app-server applies to shell commands) under a throwaway `CODEX_HOME` with a profile that reads `:minimal` and `/opt/homebrew` and writes one job folder, network off:

| Probe | Result |
| --- | --- |
| `cat ~/.zshrc` | `Operation not permitted` |
| `ls ~/Documents` | `Operation not permitted` |
| write inside the job folder | succeeded |
| write to `~/` | `Operation not permitted`; no file created |
| `curl https://example.com` | `Could not resolve host` |
| `/usr/bin/python3` | blocked: the Command Line Tools path is not readable under `:minimal` |
| `/opt/homebrew/bin/python3` with that path readable | 3.14.7 ran; `pandas` present; `matplotlib`, `python-docx`, `openpyxl`, `python-pptx` absent |

Not yet measured: the same profile applied through `thread/start` `config` and read back with `config/read`; web search with shell network off; whether an ephemeral thread leaves a rollout under `~/.codex/sessions`; skill discovery from the job folder; a TCC prompt when a FNDR child writes under `~/Documents`. These are spikes S1 to S6 in the work breakdown and gate the parts that depend on them.

## Options

Scored 1 (poor) to 5 (strong). The total is a guide; the rejections below give the deciding reason.

| | A. Codex app-server worker | B. Browser Use | C. GPT Researcher | D. Hermes with tools | E. Claude Code headless | F. OpenAdapt |
| --- | --- | --- | --- | --- | --- | --- |
| What it is | OpenAI's open-source coding agent (Apache-2.0), as a long-running thread in a sandboxed folder | Open-source browser agent over Playwright and CDP | Open-source deep-research agent with local documents | The Hermes FNDR already pins, given terminal and file tools | Anthropic's agent CLI in non-interactive mode | Open-source process automation learned from recorded demonstrations |
| Demo impact | 5: a finished file with checked sources | 4: watching a browser act | 3: a report | 4 | 5 | 5 if it worked |
| Uses FNDR's memory | 5: drafts from work sets and suggestions | 3: memory fills forms | 3: memories as local documents | 4 | 5 | 3 |
| Fits accepted ADRs | 5: sandbox and egress fit 018, 024, 025; never touches the Mac's apps, so 026 does not apply | 2: model-chosen browser actions bypass per-action policy unless wrapped tool by tool (026) | 3: a new provider key path (018) | 1: reverses the answer-only decision in 024 and 025 | 4: same shape as A | 1: needs keyboard and mouse event recording, which FNDR has never done |
| Setup on the demo Mac | 5: ChatGPT sign-in exists | 3: Chromium, Python, a model key | 2: OpenAI and Tavily keys, Python deps | 4 | 2: a second account and sign-in | 2 |
| Live reliability | 4: no third-party site in the loop except search | 2: sign-ins, CAPTCHAs, layout changes | 3 | 3: Hermes's local terminal has no OS sandbox on macOS | 4 | 1 |
| New code | 3 | 3 | 2 | 4 | 3 | 1 |
| **Total** | **27** | 17 | 16 | 20 | 23 | 13 |

## Decision

**A. FNDR runs Codex app-server as a sandboxed worker for person-approved hand-offs.**

1. **One job, one folder, one thread.** A hand-off is a `codex app-server` child started for that job, with `cwd` set to a new job folder (`inputs/`, `work/`, `out/`), an ephemeral thread, and one turn (a second turn only for Revise).
2. **The sandbox is FNDR's, and checked.** FNDR writes a permission profile for the job: read `:minimal`, the pinned toolchain and the job folder; write the job folder; network off. It is passed as process config, read back with `config/read`, and compared field by field before `turn/start`. Any mismatch refuses the job. `approvalPolicy` is `never`, so an escape request is denied rather than shown.
3. **Nothing else is attached.** MCP servers, plugins, apps, hooks, computer use, browser use, Codex memories and image generation are switched off for the worker process and asserted off in the effective config. The worker cannot reach FNDR's own MCP server.
4. **The only network is model-side web search,** run by OpenAI on the worker's behalf, mode `cached` by default. The shell has no network.
5. **Egress is the draft.** What is sent at start is exactly the reviewed hand-off draft (ADR 025 item 3, extended with a local worker destination): the person's words or a checked suggestion quote, the attached memories' current display text, and the attached files copied into `inputs/`. Sources are rechecked against the visibility rule at start. One `HandOff` row per turn goes to the model request log with bytes and kinds. Bytes count the prompt plus the output of every command the worker ran, which is what the model reads back; this is an upper bound because Codex may truncate long output.
6. **The result is untrusted until FNDR checks it.** The final message must match a manifest schema. FNDR verifies paths, file types, and every claimed source: memories must be attached ids, files must be in the folder, web URLs must have appeared in this job's `webSearch` events. Counts on the result come from this check only. A result enters Memory only when the person taps Keep, as an agent note with provenance.
7. **FNDR never opens, runs or sends a deliverable by itself.** Output types are limited to documents, tables and images (`docx`, `xlsx`, `pptx`, `pdf`, `md`, `txt`, `csv`, `png`); anything else in `out/` is listed and not offered for Open.
8. **Stop always works.** Interrupt, confirm, kill after 5 seconds. Quitting FNDR stops every job. Private Mode and the actions kill switch refuse a start and stop a running job.
9. **Labs, off by default,** offered only with a ChatGPT sign-in and a passing sandbox self-test on this Mac.

## Why

- The product's promise is work memory that stays on the Mac. Hand-off keeps the store, embeddings and resolution local, and sends only what a person reviewed, which is the line ADR 024 already draws.
- The sandbox is enforced by macOS Seatbelt through Codex, not by instructions; the spike shows reads and writes outside the folder fail. Prompt rules remain a second layer.
- Codex is the one agent already signed in and logged on this Mac, so the integration adds a mode, not an account, a key or a vendor.
- Citations checked by code are what separate a demo of an agent from a demo of trust in an agent. The `webSearch` events give FNDR ground truth the model cannot fake.

## Rejected

- **B, Browser Use.** It acts on live sites with model-chosen clicks, which ADR 026 rejected for the Mac and would need per-action policy rebuilt for the browser. Demo reliability depends on third-party sites. Codex's own browser tools are switched off for the worker for the same reason.
- **C, GPT Researcher.** It adds two provider keys and a Python service for a subset of what A does (web research), and produces reports only.
- **D, Hermes with terminal and files.** It reverses an accepted, tested boundary (ADR 025 part 1) and its local terminal has no OS sandbox on macOS.
- **E, Claude Code headless.** A strong worker with the same shape, rejected for now only because it needs a second account and sign-in on the demo Mac. The job record and verifier are written so it can be a second destination later; that needs its own ADR.
- **F, OpenAdapt.** Learning from demonstration needs a recording of keystrokes and clicks, which contradicts FNDR's capture rules, and its replay reliability is the weakest of the set.
- **Run the worker with `danger-full-access` and rely on instructions.** Rejected: the safety argument would rest on a model following text.
- **Let the worker call FNDR's MCP tools to find its own context.** Rejected: context would leave without the person seeing it, against ADR 024 item 1.

## Consequences

- `codex_account::AppServer` gains notification streaming and interrupt, used by the worker; current callers are unchanged.
- `agent/delegation.rs` gains a destination; peer behavior is unchanged.
- A pinned Python toolchain (uv venv in app data, about 120 MB, measured in spike S5) is installed only when Hand-off is turned on.
- A new prompt and schema live in `inference/prompts.rs` with their own version and a row in `docs/product/llm-task-catalog.md`.
- Hand-off depends on Codex's permission profile keys. The self-test runs on enabling and after each Codex version change; a failure turns Hand-off off with the reason.

## Compatibility

Nothing changes for Notch Do, Hermes, peers or MCP clients. A Mac without a ChatGPT sign-in never sees the feature.
