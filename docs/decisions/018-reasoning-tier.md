# ADR-018: Reasoning tier (local by default, optional cloud model with your own key)

## Status

**Proposed, 2026-10-04. Not accepted.** Draft for PD-01. The decision is the owner's.

Until this is accepted:

- Every new model path is local only. No captured text goes to a cloud model from new code (`docs/superpowers/plans/2026-10-04-parallel-sessions/README.md`, Global Constraints).
- The master plan constraint "Strictly local models: no cloud LLM at runtime, not even opt-in" stands as written.

Two inputs are missing: Kunj's PD-08 task table (not in the repo on 2026-10-04) and any measured local versus cloud difference (none exists).

## Question

Should FNDR let a person opt in to a cloud model, with their own API key, for structuring memories, the command router, Ask answers, and "about this screen", while capture, OCR, storage, and embeddings stay on the Mac? (PD-01, month plan section 6 "Reasoning tier", section 12 row 1.)

## What is true today

From reading the code on 2026-10-04. "Strictly local" is already partial; every option below has to say what happens to these paths.

| Path | What leaves the Mac | Where the key or token lives | In Privacy Activity? |
|---|---|---|---|
| Capture, OCR, storage, embeddings, and the local model tasks in `docs/product/llm-task-catalog.md` | Nothing | None | Not applicable |
| Screen Guide with the ChatGPT model (opt-in; default is `Local` in `config.rs`) | The question, the position-annotated OCR text of the screen, and the recent Guide conversation. A downscaled screenshot only with a second opt-in, `send_screenshot_to_codex`, default off (`ipc/commands/codex_account.rs`, `answer_screen_guide_with_codex`) | ChatGPT sign-in tokens held by the `codex` app-server in `$CODEX_HOME/auth.json`, file storage on purpose, not the Keychain (header of `codex_account.rs`) | No. It runs in a `codex` subprocess and never calls `record_egress` |
| Hermes Agent (Labs) with OpenRouter, ChatGPT sign-in, or a custom endpoint | The chat, plus the FNDR context snapshot FNDR writes for Hermes (`FNDR_CONTEXT.md`) | OpenRouter or custom key written to `hermes-home/.env` in FNDR's app data (`ipc/commands/hermes_agent.rs`) | Partly. FNDR counts its call to the local Hermes server (127.0.0.1); the provider call happens inside Hermes and is not counted |
| MCP clients (Claude Code and others) | Whatever the client reads (context packs, search results) goes to that client's model. FNDR does not control it | Bearer token in `~/.fndr/mcp_token` (ADR-017) | No. MCP reads are not shown yet (parallel-session plan, N8) |

Privacy Activity today (`src-tauri/src/privacy_proof.rs`) keeps one request counter and the set of hosts, in memory. It resets on quit and has no feature or byte counts.

## Options

| | A. Local only | B. Opt-in cloud per task, your own key | C. Cloud by default |
|---|---|---|---|
| Default | Local | Local; every task off until the person turns it on | Cloud; the person opts out |
| What leaves the Mac | Nothing for reasoning. To make this true, the Screen Guide ChatGPT path and Hermes cloud providers are removed, or this ADR names them as Labs exceptions | Only for tasks the person enabled: text that already passed the privacy gates. Today those gates skip a whole frame whose text matches a secret pattern rather than redacting it (`capture_context_skip_reason` in `capture/mod.rs`, patterns in `privacy/safety_gate.rs`). For "about this screen", the one image the person asked about, with consent on that request | Same inputs as B, for every reasoning task, from the first run |
| Never leaves, in every option | Pixels from normal capture, the database, vectors, audio, anything from a skipped, blocklisted, or incognito context | same | same |
| Logged in Privacy Activity | Nothing new; MCP reads still need N8 | One local record per request: time, task, provider host, model id, bytes sent and received, image yes or no, outcome, latency. No content. Persisted, bounded, deletable, shown in plain words ("Ask sent 3 KB to the provider at 10:42"). A cloud call that skips the log is a bug | Same as B |
| Key storage | None | macOS Keychain, one generic-password item per provider, read at request time. Never in a file, settings JSON, environment variable, log, trace, or MCP response. Removing the key deletes the item | FNDR's own key shipped in the app (extractable, and FNDR pays), or a key forced at onboarding |
| Quality difference | Not measured; local numbers only | Measured per task before the task can be enabled (below) | Measured, but after the default is already cloud |
| Main risks | Structured fields stay thin if the local 2B model cannot fill them; Ask stays slower; memory pressure on 8 GB | A secret the patterns miss is sent; provider retention and training terms; screen text that tries to instruct the model; cost on the person's key; offline must fall back to local; more code before Beta | Contradicts "the work memory for your Mac" and the README's local-first promise; consent by default is weak; cost; the first question any judge asks |

Facts behind the risks: on the owner profile, project, outcome, and next-steps fields are 0.0% filled and median stored text is 129 characters (`docs/evidence/W02/VS-01-baseline.md`). The cause is not diagnosed yet; parse failures in `memory_review` and structured extraction are one known issue (parallel-session plan, N2). The live text model is Qwen3-VL-2B Q4_K_M with a 4,096-token context (`docs/product/llm-task-catalog.md`). Screen text can never add a tool or argument, and a model can never lower a tool's risk level (ADR-022), which bounds what an injected instruction can do.

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
