# ADR-018: Reasoning tier (local by default, optional cloud model with your own key)

## Status

**Accepted 2026-10-08, by the owner: option B, bounded to turns a person starts.** See "Decision, 2026-10-08" at the end, which replaces the recommendation and the open PD-08 table below. (Accepted in part on 2026-10-07 for Notch Do, Hermes and Screen Guide's ChatGPT answers; Kunj accepted it for Notch Do and Hermes on 2026-10-06.)

## Question

Should FNDR let a person opt in to a cloud model, with their own API key, for structuring memories, the command router, Ask answers, and "about this screen", while capture, OCR, storage, and embeddings stay on the Mac? (PD-01, month plan section 6 "Reasoning tier", section 12 row 1.)

## What is true today

From reading the code on 2026-10-04. "Strictly local" is already partial; every option below has to say what happens to these paths.

| Path | What leaves the Mac | Where the key or token lives | In Privacy Activity? |
|---|---|---|---|
| Capture, OCR, storage, embeddings, and the local model tasks in `docs/product/llm-task-catalog.md` | Nothing | None | Not applicable |
| Screen Guide with the ChatGPT model (opt-in; default is `Local` in `config.rs`) | The question, the position-annotated OCR text of the screen, and the recent Guide conversation. A downscaled screenshot only with a second opt-in, `send_screenshot_to_codex`, default off (`ipc/commands/codex_account.rs`, `answer_screen_guide_with_codex`) | ChatGPT sign-in tokens held by the `codex` app-server in `$CODEX_HOME/auth.json`, file storage on purpose, not the Keychain (header of `codex_account.rs`) | No. It runs in a `codex` subprocess and never calls `record_egress` |
| Hermes Agent (Labs) with OpenRouter, ChatGPT sign-in, or a custom endpoint | The chat, plus the FNDR context snapshot FNDR writes for Hermes (`FNDR_CONTEXT.md`) | OpenRouter or custom key written to `hermes-home/.env` in FNDR's app data (`ipc/commands/hermes_agent.rs`) | Partly. FNDR counts its call to the local Hermes server (127.0.0.1); the provider call happens inside Hermes and is not counted |
| MCP clients (Claude Code and others) | Whatever the client reads (context packs, search results) goes to that client's model. FNDR does not control it | The MCP bearer token in `~/.fndr/mcp_token` (ADR-017) | No. MCP reads are not shown yet (VS-37) |

Privacy Activity today (`src-tauri/src/privacy_proof.rs`) keeps one request counter and the set of hosts, in memory. It resets on quit and has no feature or byte counts.

## Options

| | A. Local only | B. Opt-in cloud per task, your own key | C. Cloud by default |
|---|---|---|---|
| Default | Local | Local; every task off until the person turns it on | Cloud; the person opts out |
| What leaves the Mac | Nothing for reasoning. To make this true, the Screen Guide ChatGPT path and Hermes cloud providers are removed, or this ADR names them as Labs exceptions | Only for tasks the person enabled: text that already passed the privacy gates. Today those gates skip a whole frame whose text matches a secret pattern rather than redacting it (`capture_context_skip_reason` in `capture/mod.rs`, patterns in `privacy/safety_gate.rs`). For "about this screen", the one image the person asked about, with consent on that request | Same inputs as B, for every reasoning task, from the first run |
| Never leaves, in every option | Pixels from normal capture, the database, vectors, audio, anything from a skipped, blocklisted, or incognito context | same | same |
| Logged in Privacy Activity | Nothing new; MCP reads still need VS-37 | One local record per request: time, task, provider host, model id, bytes sent and received, image yes or no, outcome, latency. No content. Persisted, bounded, deletable, shown in plain words ("Ask sent 3 KB to the provider at 10:42"). A cloud call that skips the log is a bug | Same as B |
| Key storage | None | macOS Keychain, one generic-password item per provider, read at request time. Never in a file, settings JSON, environment variable, log, trace, or MCP response. Removing the key deletes the item | FNDR's own key shipped in the app (extractable, and FNDR pays), or a key forced at onboarding |
| Quality difference | Not measured; local numbers only | Measured per task before the task can be enabled (below) | Measured, but after the default is already cloud |
| Main risks | Structured fields stay thin if the local 2B model cannot fill them; Ask stays slower; memory pressure on 8 GB | A secret the patterns miss is sent; provider retention and training terms; screen text that tries to instruct the model; cost on the person's key; offline must fall back to local; more code before Beta | Contradicts "the work memory for your Mac" and the README's local-first promise; consent by default is weak; cost; the first question any judge asks |

Facts behind the risks: on the owner profile, project, outcome, and next-steps fields are 0.0% filled and median stored text is 129 characters (`docs/evidence/W02/VS-01-baseline.md`). The cause is not diagnosed yet; parse failures in `memory_review` and structured extraction are one known issue (VS-31). The live text model is Qwen3-VL-2B Q4_K_M with a 4,096-token context (`docs/product/llm-task-catalog.md`). Screen text can never add a tool or argument, and a model can never lower a tool's risk level (ADR-022), which bounds what an injected instruction can do.

## Which tasks may use a cloud model (PD-08)

PD-08's table does not exist yet (no file under `docs/product/` or elsewhere mentions it outside the ticket). Placeholder below. The input column is from `docs/product/llm-task-catalog.md`; everything marked "PD-08" is to be filled by PD-08.

| Task | Task ids in the catalog | Input today | Local quality today | Latency | Memory | Privacy sensitivity | Recommendation |
|---|---|---|---|---|---|---|---|
| Memory structuring | `memory_extraction`, `memory_extraction_repair`, `memory_snippet`, `todo_extraction` | OCR text up to 4,000 characters | PD-08 | PD-08 | PD-08 | PD-08 | PD-08 |
| Review | `memory_review`, `memory_review_repair` | Clean text up to 4,000 characters plus 12 same-day candidates | PD-08 | PD-08 | PD-08 | PD-08 | PD-08 |
| Router | None yet (router not built; GS tickets) | The confirmed command text only; screen text is never router input (actions policy) | PD-08 | PD-08 | PD-08 | PD-08 | PD-08 |
| Ask | `answer`, `card_synthesis`, `query_plan`, `query_expansion` | Question plus up to 1,000 characters of context (`answer`); up to 6 snippets (`card_synthesis`) | PD-08 | PD-08 | PD-08 | PD-08 | PD-08 |
| About this screen | `screen_guide` | System prompt plus screen text; one image only with consent | PD-08 | PD-08 | PD-08 | PD-08 | PD-08 |
| Daily brief | `daily_briefing` | Cards up to 900 characters | PD-08 | PD-08 | PD-08 | PD-08 | PD-08 |
| Embeddings | Not an LLM task (MiniLM, ONNX) | Memory text | PD-08 | PD-08 | PD-08 | PD-08 | Local only in every option (month plan section 6) |

## How we measure the local versus cloud difference

1. Same inputs through both paths. Only synthetic or human-approved public inputs go to a cloud model while measuring, never the owner's vault.
2. Sets that exist today: extraction gold v0 (50 cases in `src-tauri/tests/fixtures/gold/v0/`, labeled `claude-draft` until PD-10's human review) for format validity and grounding; the 22-query seeded retrieval set (`scripts/demo/knowledge-worker-queries.json`, baseline in `docs/evidence/W02/retrieval-baseline-seeded.json`) for Ask answers that cite an accepted memory; the 30 synthetic screens in `src-tauri/tests/fixtures/screens/` for "about this screen". The router needs the 50-utterance command script from month plan section 3, which does not exist yet.
3. Per task, report: the quality metric, p50 and p95 latency, bytes sent, cost per 100 calls at the provider's list price, and peak memory for the local run on the M1 8 GB.
4. Runner: the master plan names `make eval`, but the Makefile has no such target on 2026-10-04. Building it is a follow-up.
5. A task can be enabled for opt-in only after its row has numbers. If local is within a margin the owner sets, the task stays local only.
6. Publish the table as an evidence file and in the Beta "Numbers" beat (month plan section 9).

## Recommendation (the owner decides)

Option B, staged, with an honest fallback to A for Beta:

1. Accept B with zero tasks enabled.
2. Close the gaps first: Keychain storage, the per-request log persisted and shown, and the two subprocess paths (Screen Guide ChatGPT, Hermes) either counted or labeled "not counted" in Privacy Activity.
3. Enable a task only after its PD-08 row and its measured difference exist. Candidates from month plan section 12 row 1: structuring, router, Ask, about this screen.
4. If the gaps cannot close before the Oct 16 freeze, choose A for Beta. The 3:40 demo beat ("Privacy Activity shows what Claude read and any cloud requests") cannot be shown truthfully until the log exists.

C is not recommended: it contradicts the product promise in `README.md`, `docs/product/value-scorecard.md`, and month plan section 1.

## On acceptance: exact edits

If B is accepted:

1. `docs/superpowers/plans/2026-09-21-beta-final-master-plan.md`, Global Constraints. Replace
   `- Strictly local models: no cloud LLM at runtime, not even opt-in.`
   with
   `- Local models by default. Capture, OCR, storage, and embeddings never leave the Mac; reasoning may use a cloud model only as ADR-018 allows (opt-in per task, the person's own key, every request logged).`
2. `docs/team/TEAM.md`, "Rules that protect the project". Replace
   `- Strictly local models. No cloud LLM at runtime.`
   with
   `- Local models by default; cloud reasoning only as ADR-018 allows (opt-in per task, your own key, every request logged).`
3. This file: Status becomes `Accepted <date>`, with the decider and who agreed.
4. `docs/team/decision-log.md`: add the row and remove ADR-018 from "Open".

If A is accepted: no constraint edit. Set Status to `Accepted <date> (option A)`, add the decision-log row, file a follow-up to remove or label the Screen Guide ChatGPT and Hermes cloud paths, and edit month plan section 6 "Reasoning tier" and the last row of section 11 to say reasoning stays local.

If C is accepted: this is not a one-line edit. Month plan sections 1 and 6, the README privacy section, and the value scorecard all need rewriting first.

## Consequences if B is accepted

- Follow-up tickets: Keychain key storage; one cloud client that logs before it sends; persisted per-request log in Privacy Activity; per-task toggles in Trust and Settings; the local versus cloud eval runner; the subprocess paths counted or labeled.
- PD-09's trust spec lists the cloud request log and its retention.
- VS-24 and other Ask work may add a cloud path only for a task enabled under this ADR.

This draft changes no code.

## Amendment 2026-10-06: Notch Do and Hermes on the ChatGPT plan

Option B, scoped to two features. The "your own key" condition is met by the person's own ChatGPT sign-in through the official `codex app-server`; FNDR stores no key and never reads token values.

- **Consent:** the "Operate my Mac" toggle is the one-time opt-in for Notch Do. Its copy names what leaves the Mac: the transcript, the accessibility text of the app being operated, and up to 5 retrieved memory snippets.
- **Memories:** sent only when the request refers to the past (decided locally before any request, `operator::plan::refers_to_past`). Bounded to 5 snippets from `context_runtime::retrieve`.
- **Logging:** every Codex and Hermes model request is recorded in Privacy Activity with feature, host and bytes FNDR sent. No content.
- **Hermes** answers with the same ChatGPT sign-in, imported by Hermes itself from `~/.codex`; the Codex app-server is the only refresher.

## Amendment 2026-10-07: owner ratification and Screen Guide

The owner ratified the amendment above and extended it to Screen Guide's ChatGPT answer path, which shipped on 2026-09-23 without a decision. ADR 024 replaces two lines of the amendment above:

- **Memories in Hermes:** FNDR adds related memories, and gives Hermes memory search, only for a provider on this Mac or when the person turns it on for a cloud provider. Attached memories are always sent.
- **Logging:** Screen Guide's ChatGPT requests are logged too, the log survives a restart, and each row says when memories, on-screen text or a screenshot went along.

## Decision, 2026-10-08

**The rule.** A cloud model is used only on a turn the person starts, on a surface where they chose that provider. Nothing FNDR does by itself in the background ever goes to a cloud model.

That is one sentence a person can check against Privacy Activity, and it does not need a per-task table to explain.

| Task | Decision |
|---|---|
| Memory structuring, review, task finding, snippets | Local only, for Beta and Final. They run in the background on everything captured, so the rule forbids them |
| Embeddings | Local only (unchanged) |
| Daily brief | No model at all since 2026-10-07 (`briefing.rs`) |
| Ask and Search answers, the command router | Local only for Beta. May be reopened after Beta, one task at a time, only with a measured row as in "How we measure" above |
| Agent page (Hermes) | Allowed: the person picked the provider. Rules in ADR 024 |
| Notch Do | Allowed: the person's ChatGPT plan, opted in by "Operate my Mac". Rules in ADR 022, 024 and 026 |
| Screen Guide's ChatGPT answer | Allowed when ChatGPT is the chosen model; the screenshot needs its own opt-in. Falls back to this Mac and says so |

Why this and not the staged plan recommended above:

- The staged plan waited on two inputs that never arrived: a per-task table and a measured local versus cloud difference. Nothing in the repo shows a cloud model would fix what is thin locally, and the work since has gone the other way: the briefing, task finding and fallback summaries got better by taking the model out.
- The background tasks see everything captured. A secret the patterns miss would be sent with no one watching. On a turn the person starts, they can see what they asked and Privacy Activity lists it.
- It closes PD-01 before the Oct 16 freeze with a rule the Beta demo can state truthfully.

Conditions from option B and where they stand:

- Logged: every cloud request is in Privacy Activity with feature, host, bytes and kinds of content; the log survives a restart. Met.
- Your own key or sign-in: ChatGPT uses the person's own sign-in and FNDR stores no key. Met. An OpenRouter or custom key is kept in a file only the owner can read, not the Keychain, because Hermes reads it from its own home (ADR 024, E14). Keychain is a Final item.
- Off until turned on: every one of these is off by default. Met.

PD-08's table is no longer needed for Beta; the table above is the decision. PD-08 becomes "measure Ask, local versus cloud" if anyone reopens it.
