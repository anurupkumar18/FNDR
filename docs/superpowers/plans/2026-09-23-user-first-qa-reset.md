# User-first QA reset (Phase 0) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Parts 1 to 3 are decision material for the owner. Tasks 1 to 7 are for implementers. Tasks 8 and 9 need the owner.

**Goal:** Make FNDR testable the way a knowledge worker would actually use it, and put hard numbers on the Vault and search before anyone changes them, so the owner can score every feature hands-on before the October month plan (`docs/team/2026-10-month-plan.md`) is finalized and handed to the team.

**Architecture:** No new subsystems. Six small slices on top of what exists: a written walkthrough with honest "how it works" notes, a seeded QA profile whose memories carry the structured fields Resume Work needs, a Resume Work list on Home that calls the existing `resume_work` command, a Settings card that turns on the existing MCP server on a stable port, an aggregate-only vault health report, and a retrieval baseline that runs the same query set through both of FNDR's retrieval paths (the Search screen and Ask/agents).

**Tech Stack:** Tauri 2 + Rust (`src-tauri/`), React + TypeScript (`src/`), Vitest + Testing Library, LanceDB, the existing seeder example `src-tauri/examples/seed_demo.rs`.

**Spec:** The owner's request of 2026-09-23 (restated in Part 2) and the evidence in Part 1. Context: `docs/superpowers/plans/2026-09-21-beta-final-master-plan.md`, `docs/superpowers/plans/2026-09-21-ws3-feature-thesis-and-research.md`, `docs/product/value-scorecard.md`, `docs/product/UI-UX-OVERHAUL-PROGRAM.md`, `docs/product/mcp-tool-audit.md`.

## Global Constraints

- No em dashes or en dashes in docs, code comments, or commit messages.
- Commits are authored by the owner's git identity with no AI co-author trailer. Work on branch `qa/user-first-reset`. Never commit to `main`. Never create worktrees automatically.
- Never commit real captures, database files, tokens, or anything from `~/Library/Application Support/com.fndr.app*`. The QA profile lives at `~/Library/Application Support/com.fndr.app.qa`; the seeder already refuses to write into the real profile.
- Phase 0 changes no capture behavior, storage schema, privacy gate, or MCP auth rule. ADR-017 stays: every `tools/call` needs the bearer token.
- Show the engine's real output. Do not prettify Resume Work text in the UI; the point of the QA pass is to see what the engine produces.
- The strictly-local model rule stays in force for Phase 0. Loosening it is a month-plan decision (ADR-018 proposal, Part 3).
- Reference machine: Apple M1, 8 GB. State what you ran to verify. Full sweep: `make test`.

---

## Part 1: What the sweep found (verified in code on 2026-09-23)

The strategy docs say "Resume Work is the product, Deja vu is the delight, Privacy Activity is the trust proof." The code says something different today:

| Surface | What we say | What actually happens today | Evidence |
|---|---|---|---|
| Resume Work | The front door | Backend exists and is correct for what it does: groups the last N hours of memories by project, then session, then domain, then app, and packs cited evidence inside a token budget. **There is no screen for it.** Home is a greeting and a search pill whose subtitle says "Let's pick up where you left off." | `src-tauri/src/resume/mod.rs`; no caller in `src/`; `src/app/HomeHero.tsx:38` |
| Resume quality | Cited next steps | The thread title, "last state," and next steps come straight from `project`, `topic`, `outcome`, `next_steps` stored at capture time. Those fields are filled by the local 2B model only when memory pressure allows; how often that happens is unmeasured. `outcome` is often a status token (`in_progress`) and `topic` defaults to `unknown`, so "last state" can read "unknown in_progress." | `resume/mod.rs:46-57`, `storage/schema.rs:300,418`, MEM-06 notes |
| Seeded demo vault | Realistic week | `seed_demo.rs` sets none of `project`, `topic`, `outcome`, `next_steps`, `decisions`, `errors`. Resume on the demo profile would show threads titled by raw session slugs with empty state. | `src-tauri/examples/seed_demo.rs:62-110` |
| Agents over MCP | "Your agent pulls the same context" | The server exists (52 tool names, auth required). **Nothing in the shipped UI starts it**: the only `startMcpServer` caller was removed in commit `6cb6a1d` ("simplify settings for alpha rehearsal"). When started it picks a random port, so a saved agent config breaks on every launch. No screen tells a user how to connect Claude Code. | `src/shared/ipc/tauri.ts:1299` has no caller; `mcp/mod.rs:616` |
| Two-way MCP | Agents add to memory | Only two write tools, and neither writes a memory: `fndr_remember_decision` appends to a separate decision ledger, `agent.rate_result` logs feedback without changing ranking. | `context_runtime/mod.rs:667`, `docs/product/mcp-tool-audit.md` |
| Deja vu | The delight | No code exists. The nearest thing is a proactive toast every 30 seconds for any memory above 0.82 vector similarity, with no reason shown. | `main.rs:400-445`, `config.rs:75,79` |
| Ask FNDR, graph routes | Graph-aware recall | The live query plan runs its graph route over an empty in-memory graph; the typed graph is never persisted. `fndr_get_memory_subgraph` returns an empty descriptor. README lists the insight graph as "Stable." | `context_runtime/mod.rs:3069-3075`, `README.md` feature table |
| To-dos | Follow-ups from your work | A manual list. `extract_task_candidates` (turns next steps and errors into tasks) has tests and no production caller. Meetings can add tasks, but the Meetings screen is not mounted. | `tasks/extract_from_memory.rs:15`, `meeting/mod.rs:1441` |
| Daily Summary | Reflection | A deterministic template over the day's memories (apps, time span, top activities). No model call. Follow-ups come from the whole task list, not that day. | `ipc/commands/stats.rs:488,872` |
| Quick Find (Alt+Space) and Autofill | Keyboard-first recall | Built by Kunj, then switched off for the Alpha demo: the windows and global shortcuts are not registered. | `main.rs:653-655` |
| Privacy Activity | Trust proof | Real session counters and FNDR egress counter. Does not show what an agent read over MCP. | `privacy_proof.rs`, `domains/privacy-proof/PrivacyProof.tsx` |
| Everything else | Supporting | 9 mounted destinations plus 13 compiled but unmounted panels (Agent, Automation, Focus, Time Tracking, Research, Quick Skills, Glasses import, Pipeline Inspector, Companion, full graph, Meetings, Search History UI). | `src/app/panels.ts`, UI-UX program surface inventory |

The one-line reading: the spine (capture, OCR, storage, hybrid search, cited answers) is real; the three things we lead with are either invisible (Resume), unreachable (agent access), or not built (Deja vu). A hands-on QA pass today would mostly exercise the Reflect panels, which are not the product.

## Part 1b: Second sweep, the owner's seven points (verified 2026-09-23)

Measured on the owner's real profile with the vault health script from Task 5 (aggregate counts only, no memory text) and read from code. The owner's priority order after this sweep: **Vault and search quality first**, then reopen, voice, and quick actions.

| # | Owner's concern | What is actually true | Evidence |
|---|---|---|---|
| 1 | Reopen where you left off | 26 of 29 real memories reopen only the app, not the page or file (10% specific); a URL is stored on 10%. Native document apps expose the open file through the Accessibility attribute `AXDocument`; FNDR reads it for Autofill but never during capture. A file path is stored only when the local model lists one in `files_touched` (0 of 29). Downloads are detected (`downloads.rs` watches ~/Downloads) and stored as memories, but with no reopen target, no source URL, no link to the page that started them, and no file content indexed. | vault health; `memory/reopen.rs:83`; `accessibility/mod.rs:588`; `downloads.rs:212-238` |
| 2 | Vault and search are not good enough | The owner's vault has 29 memories over 5 active days in the last week (about 6 per active day) with a median of 129 characters of stored text. Either FNDR barely ran or it dropped nearly everything; we cannot tell which, because skip counts reset on quit and no capture baseline was ever recorded. No retrieval quality number exists anywhere: the gold labels are drafts and no eval run has been recorded. | vault health; `docs/evidence/` has no retrieval or capture baseline |
| 2 | Consistent no matter what or when | Two retrieval stacks. The Search screen uses `search/` (`HybridSearcher` plus a word-overlap reranker); Ask and every MCP tool use `context_runtime/` (planner routes plus fusion). The same question can rank different memories in each. | `ipc/commands/search.rs:530`; `context_runtime/mod.rs:3059`; `mcp/mod.rs:2413` |
| 2 | Semantic | The Search reranker drops any result whose word overlap with the query is under 15%, whatever its vector similarity. That removes exactly the paraphrase matches semantic search is for. | `search/reranker.rs:9-30` |
| 2 | Keyword side | Keyword search is `LOWER(column) LIKE '%term%'` over seven columns with the row limit applied before scoring, so it returns the first matches found on disk, not the best ones. There is no full-text (BM25) index, although the LanceDB 0.27 crate in use ships one. | `storage/lance_store/mod.rs:1693-1750` |
| 6 | Is anything embedded; is there RAG? | Yes, embedded: every memory has three non-zero 384-dimension MiniLM vectors (0% zero vectors). No, not the RAG we describe: the parent-plus-chunk design in ADR-008 uses BGE 1024-dimension chunk tables that have **0 rows** on the owner's profile, because chunk indexing only runs from a manual reindex. So retrieval matches one short composed summary per memory, not the text you actually saw. MiniLM-L6 (2019) is also the weakest embedder still in common use. | vault health; ADR-008, ADR-010 |
| 6 | What LanceDB does | LanceDB is an embedded, file-based database (no server process) that stores rows with vector columns and answers nearest-neighbor queries combined with SQL-style filters. FNDR keeps each memory as one 113-column row in `memories_v4_minilm_384` and has 17 other tables (graph, tasks, meetings, context packs, chunks); 9 of the 18 are empty on the owner's profile. | `~/Library/Application Support/com.fndr.app/lancedb`, vault health |
| 3 | Screen Guide and computer use | Screen Guide is a local re-implementation of Clicky (ADR-014) that replaced Clicky's cloud models with the local 2B vision model and was defined read-only: it can point, never click, type, or open. On 8 GB that model is slow (local extraction p50 8 s, max 36 s from the owner's trace log) and shares one inference lock with capture enrichment. Voice commands in Search are hard-coded `includes("pause capture")` string checks. The in-app agent (`agent/`) has twelve developer-oriented action kinds (read-only shell commands, delegate to Claude Code) and its panel is not mounted. There is no typed action layer for "open this app, open that doc, make a reminder." Kunj's notch HUD work sits on an unmerged branch on the GitHub mirror: 61 files, about 10,500 lines added and 5,800 removed, diverging from `main` since Sep 9. | ADR-014; `llm_traces.jsonl` aggregates; `agent/actions.rs:7`; `SearchBar.tsx:334-375`; `git diff --stat main...github/kunj-notch-hud` |
| 4 | Skills from actions | Scaffolding exists: `agent/skills.rs` turns an agent audit record into a skill draft appended to a JSONL file, and the MCP prompt `turn_workflow_into_skill` exists. Nothing in the shipped app creates audit records, so no skill has ever been drafted. | `agent/skills.rs:43,113` |
| 5 | Voice | Browser `MediaRecorder` audio is written to a file, then transcribed by a whisper.cpp CLI if one is installed, otherwise by a Python sidecar (`whisper_cpp_python`, unmaintained, needs Homebrew Python 3.10 to 3.13) that loads the 466 MB `ggml-small` model from disk on every request. No streaming, no partial text, no progress beyond a status string. macOS 26 includes an on-device streaming recognizer (SpeechAnalyzer) with partial results; FNDR does not use it. | `speech.rs:350-460,712`; `sidecars/whisper_gguf_runner.py` |

Further flags nobody asked about:

| Flag | Why it matters |
|---|---|
| Nobody is dogfooding: the diary is empty, the owner's vault holds 29 memories, and no capture baseline exists. | We are designing for usage we have never observed. |
| The always-on cost is unmeasured (the CAP-02 real run was never recorded). | "Is it worth running all day?" has no number behind it. |
| 1.9 GB of model files on an 8 GB machine (Qwen3-VL-2B, whisper-small, BGE-large, MiniLM, CLIP); BGE is downloaded and unused. | Onboarding weight and memory pressure with no retrieval benefit. |
| The local model runs rarely and slowly: 79 traced calls in total; extraction p50 8 s, review p50 22 s, daily briefing p50 17 s; 0% of real memories have a project or next steps. | Resume, To-dos, Seen-before, and agent packs all read fields that are empty. |
| Privacy Activity counters reset on quit. | Trust cannot be shown over a day or a week. |
| Planning outweighs product: 20 plan documents under `docs/superpowers/plans` and 47 manifest tickets, far more than the user-facing change shipped since June. | This month plan has to replace the pile, not add to it. |
| 798 Rust tests and 55 frontend test files pass while the real vault holds 29 thin memories. | Green CI proves fixtures, not usefulness. Each lane needs an end-to-end number. |

## Part 2: How useful the hands-on pass is, and what has to be true first

The owner asked to use FNDR for real and score each feature. That is the highest-value activity available this week, on one condition: it must separate two questions that are currently tangled.

1. **Is the screen useful if the engine gives it good inputs?** (Design and product question. Answered on a seeded profile in 90 minutes.)
2. **Does the engine produce good inputs from my real work?** (AI engineering question. Answered by two days of live capture.)

If these are not separated, every weak screen gets blamed on the model and every weak model output gets blamed on design. So Phase 0 builds exactly what is needed to ask both questions, plus a third pass for agents:

| Pass | Question | Needs | Built by |
|---|---|---|---|
| A. Seeded | Useful given good inputs? | A QA profile whose memories carry project, topic, outcome, next steps, decisions, errors, for knowledge-worker threads | Task 2 |
| B. Live | Does the engine produce them? | The real app on the real profile, and a Resume screen to look at | Task 3 |
| C. Agent | Can my assistant use my memory? | A way to turn on MCP on a stable port and connect Claude Code without reading source | Task 4 |
| D. Numbers | How good are the Vault and search, before we change them? | A vault health report for any profile, and one query set run through both retrieval paths with Recall@5, MRR@10, latency, and agreement | Tasks 5 and 6 |
| All | Consistent scoring | A walkthrough with promise, how it works, known gaps, steps, and a score row per feature | Task 1 |

Estimated effort: Tasks 1 to 7 are one to two working days with an agent. Pass A is 90 minutes, Pass B is two normal working days plus 45 minutes of review, Pass C is 30 minutes, Pass D is 10 minutes of commands.

## Part 3: Where the SWOT is right, and where this plan pushes back

Agree: the product is sprawling, trust is existential, and "Did FNDR let the user correctly resume faster than their existing workflow?" is the right single metric. Four refinements:

1. **The hidden critical path is extraction quality, not UI.** Resume Work, Deja vu, To-dos, and agent packs all read the structured fields the local model writes at capture time. On an 8 GB Mac that model is pressure-gated. Before Beta we need one number: the share of memories with usable project and next steps, and whether a stronger model changes it.
2. **"Strictly local" was already partial.** The moment Claude Code reads a context pack over MCP, that text goes to a cloud model. The honest and more impressive position is: *capture, OCR, storage, and embeddings never leave the Mac; reasoning can run locally or, if you opt in, on a cloud model you choose; every byte that leaves (to a cloud model or to your agent) is logged and visible.* Proposed as ADR-018 in week 1 of the month plan. It turns the 8 GB weakness into an AI engineering showcase: model routing with a measured local versus cloud quality delta.
3. **Two-way MCP needs a poisoning defense to be credible.** If agents can write into memory, an agent (or a prompt-injected web page it read) can write false memories. Design rule: agent-written items are a distinct source with provenance (which client, when, which tool), shown with a badge, never silently merged into screen memories; changes an agent proposes to an existing memory go to a review inbox. That is both a safety property and a strong interview story.
4. **Narrowing to knowledge workers changes the hero examples, not the engine.** "I saw this compiler error before" becomes "I saw this error or this document before" (a student's `ModuleNotFoundError`, an analyst's broken spreadsheet formula, a reading from last week). The seeded QA corpus in Task 2 uses a student who also interns and leads a capstone team, so all three settings (school, office, project) are covered.

What to contemplate before adding external or cloud resources: add one only if it (a) makes Resume or Seen-before more correct, (b) uses a tool the user already has, and (c) can be shown in Privacy Activity. Under that test: opt-in cloud reasoning passes; agent write-back passes; pulling from Google Drive or Notion through MCP-client connectors fails for this month (big scope, weak link to resuming) and is deferred.

---

## File map (Phase 0)

| File | Change | Responsibility |
|---|---|---|
| `docs/product/qa-walkthrough.md` | Create | The owner's script and scorecard for Passes A, B, C |
| `src-tauri/examples/seed_demo.rs` | Modify | Accept structured fields and `minutes_ago` in seed entries; unit tests |
| `scripts/demo/knowledge-worker-week.json` | Create | Synthetic week for a student, intern, and capstone lead |
| `scripts/demo/seed-demo-profile.sh` | Modify | Corpus path from `FNDR_DEMO_CORPUS` |
| `Makefile` | Modify | `qa-seed`, `qa-app`, `qa-preview`, `vault-health`, `qa-retrieval` targets |
| `src/shared/ipc/tauri.ts` | Modify | `ResumeThread` types and `resumeWork()` wrapper |
| `src/domains/resume/resumeFormat.ts` | Create | Pure helpers: age label, pack text for an AI assistant |
| `src/domains/resume/resumeFormat.test.ts` | Create | Helper tests |
| `src/domains/resume/ResumeWork.tsx` | Create | Home section: loading, error, empty, threads |
| `src/domains/resume/ResumeWork.test.tsx` | Create | Component tests |
| `src/domains/resume/ResumeWork.css` | Create | Minimal layout using existing tokens |
| `src/app/App.tsx` | Modify | Mount the section under the hero |
| `src/app/App.onboarding.test.tsx` | Modify | Add `resumeWork` to the IPC mock |
| `src-tauri/src/mcp/mod.rs` | Modify | Stable default port, `FNDR_MCP_PORT`, busy-port fallback; tests |
| `src/domains/workspace/agentConnect.ts` | Create | Pure snippet builder for Claude Code and JSON configs |
| `src/domains/workspace/agentConnect.test.ts` | Create | Snippet tests |
| `src/domains/workspace/AgentAccessSection.tsx` | Create | Settings card: turn on, endpoint, copy buttons |
| `src/domains/workspace/AgentAccessSection.test.tsx` | Create | Component tests |
| `src/domains/workspace/ControlPanel.tsx` | Modify | Mount the card after Local models |
| `src/domains/workspace/ControlPanel.css` | Modify | One layout rule for the copy buttons |
| `src/dev/previewIpc.ts`, `src/dev/previewIpc.test.ts` | Modify | Model `resume_work`, `get_mcp_server_status`, `start_mcp_server` for the browser preview |
| `docs/mcp.md` | Modify | Document the stable port and the Settings card |
| `scripts/audit/vault_health.py`, `scripts/audit/test_vault_health.py` | Create | Aggregate-only store report: memories per day, vector health, text length, structured-field fill, reopen specificity, chunk rows |
| `src-tauri/src/ipc/commands/search.rs` | Modify | Extract `search_ranked_results`, the exact ranked list the Search screen uses, so the eval can call it |
| `src-tauri/examples/retrieval_qa.rs` | Create | Runs a query set through Search and Ask paths on a seeded profile; Recall@5, MRR@10, latency, agreement |
| `scripts/demo/knowledge-worker-queries.json` | Create | 22 queries with expected memory ids for the knowledge-worker corpus, tagged keyword or paraphrase |

---

### Task 1: QA walkthrough and scorecard

**Files:**
- Create: `docs/product/qa-walkthrough.md`

**Interfaces:**
- Consumes: `make qa-seed` and `make qa-app` (Task 2), the Resume section (Task 3), the Agent access card (Task 4), `make vault-health` (Task 5), `make qa-retrieval` (Task 6). Write the doc first; it names these commands.
- Produces: the filled scorecard that Task 7 turns into the month plan's "What we heard" table.

- [ ] **Step 1: Create the walkthrough with this exact content**

````markdown
# FNDR hands-on QA walkthrough

Use FNDR the way a knowledge worker would, then decide feature by feature whether it earns its place. Score what you see, not what a doc promises. If something breaks, write what you did and what happened, then move on; do not debug during a pass.

## Three passes

| Pass | Question it answers | Setup | Time |
|---|---|---|---|
| A. Seeded | If the engine gave this screen perfect inputs, is the screen useful? | `make qa-seed`, then `make qa-app` (separate profile, your real data is untouched) | 90 minutes |
| B. Live | Does the engine produce those inputs from my real work? | `npm run tauri dev` on your normal profile; work normally | 2 working days, then 45 minutes of review |
| C. Agent | Can my AI assistant use my FNDR memory? | Settings, Agent access, connect Claude Code | 30 minutes |
| D. Numbers | How healthy is my vault, and how good is search today? | `make vault-health` on your real profile; `make qa-retrieval` after `make qa-seed` | 10 minutes |

Pass A persona: Sam, a senior who is writing a history essay (HIST 2100), finishing a data assignment (CS 3500), interning as an analyst (Q4 onboarding survey readout), and leading a capstone team (pantry app demo). All data is synthetic.

## Scoring

- **Useful (1 to 5):** 1 never open it; 3 nice, would not miss it; 5 would be upset if it disappeared.
- **Trust (1 to 5):** 1 something worried or confused me; 5 nothing surprising.
- **Verdict:** keep, fix, merge (say into what), hide in Labs, or cut.
- **One sentence:** what would make you open it every day.

## Before you start

1. `git switch qa/user-first-reset && npm install` and confirm `make test` passes.
2. Quit heavy apps. On 8 GB, a browser with many tabs plus FNDR plus the local model is enough to trigger memory pressure, which changes what FNDR does.
3. Have a stopwatch for the Resume task at the end.
4. Optional: screen-record each pass. Keep recordings outside the repo.
5. Run Pass D first and keep both outputs next to you: `make vault-health` (your real profile; counts only, no memory text) and `make qa-retrieval` (the seeded profile). They tell you what the engine has to work with before you judge any screen.

## Feature cards

### 1. First run and permissions

- **Promise:** a new user understands what FNDR captures, what stays on the Mac, and grants only what is needed.
- **How it works today:** onboarding steps are name, optional Touch ID lock, privacy promise, models (the ~90 MB MiniLM search model is required; the ~1.5 GB Qwen3-VL-2B model is optional), then Screen Recording and Accessibility.
- **Known gaps:** consent "Skip" and "Continue" are not yet distinct in effect; model download failure states are only partly designed.
- **Steps:** optional fresh profile: `FNDR_DATA_DIR="$HOME/Library/Application Support/com.fndr.app.firstrun" npm run tauri dev`. Go through every step. Note any sentence you would not say to a friend.
- **Good looks like:** under 3 minutes, no surprise permission, you could explain what is stored.

### 2. Capture and pause (background)

- **Promise:** FNDR quietly remembers useful work and never keeps screenshots.
- **How it works today:** a timer loop (faster when active, slower when idle, a forced capture at least every 60 seconds) reads the frontmost app, window title, and URL; privacy gates run before any pixels (blocklist, sensitive context, private browsing, password managers, FNDR itself); then a screenshot is taken in memory, deduplicated, read with Apple Vision OCR, quality-gated, optionally structured by the local model when memory pressure allows, embedded with MiniLM, and stored in LanceDB. The screenshot is discarded. Pause survives relaunch.
- **Known gaps:** how often the local model actually structures a memory is unmeasured. Capture is timer-driven, not event-driven.
- **Pass B steps:** work normally for two days. Once a day, pause for 10 minutes and confirm nothing new appears.
- **Good looks like:** you forget it is running; the Mac does not feel slower.

### 3. Privacy Activity and blocklist

- **Promise:** you can see what was skipped and why, and what left the Mac.
- **How it works today:** counters since app launch: frames evaluated, stored, skipped by reason, and FNDR's own network requests by host. Reset when FNDR quits.
- **Known gaps:** does not show what an AI agent read over MCP. Not a full network audit (child processes are not counted).
- **Pass B steps:** add a site to the blocklist in Settings, visit it for 30 seconds, then search for words from that page in Search, Resume, and Vault. Open Privacy Activity.
- **Good looks like:** zero results anywhere; a "Blocked app or site" count went up; you trust it more after looking.

### 4. Resume Work (Home)

- **Promise:** open FNDR after an interruption and see where you left each thread, with sources.
- **How it works today:** Home calls `resume_work(72 hours, 1200 tokens)`. It groups memories from the last 72 hours by project, else session, else website domain, else app. "Last state" is the newest memory's stored topic plus outcome, next steps are the newest memory's stored next steps, sources are memory ids. No model runs when you open Home; quality depends entirely on what was stored at capture time.
- **Known gaps:** on real data expect raw values such as `unknown` or `in_progress` in the state line, and threads named after session keys or domains when no project was detected. That is the engine's real output; score it as you see it.
- **Pass A steps:** open Home. For each thread: can you say what Sam was doing and what to do next within 10 seconds? Click "Open latest source." Click "Copy for AI assistant" and paste into a text editor.
- **Pass B steps:** after each working day, open Home cold. Do the threads match what you actually worked on? Count wrong, missing, and stale threads.
- **Good looks like:** at least 3 of your real threads appear, correctly named, with a next step you agree with.

### 5. Search (Home search box)

- **Promise:** find something you saw even if you only remember the gist.
- **How it works today:** hybrid retrieval: MiniLM vector search plus keyword search, fused and reranked; time and app filters. No model call.
- **Pass A steps:** search "pandas error", "churn drivers", "architecture slide", "Field Order 15".
- **Pass B steps:** five searches for things you really needed this week. Record for each: found in the top 3, found lower, or not found.
- **Good looks like:** 4 of 5 real searches in the top 3.

### 6. Ask FNDR

- **Promise:** ask a question about your past work and get a cited answer.
- **How it works today:** a query planner picks retrieval routes, fuses evidence, and asks the local model to answer from that evidence with citations; if the model is missing, returns something invalid, or cites nothing, FNDR returns a partial answer built from the evidence instead.
- **Known gaps:** the graph route searches an empty graph. Answer quality is not yet measured on human-labeled questions. Under memory pressure the model may not run.
- **Pass A steps:** ask "What did Dana ask me to deliver and by when?", "How did I fix the pandas import last time?", "What is left on the capstone slides?"
- **Pass B steps:** three real questions. Mark each answer correct, partly correct, or wrong, and whether the citations support it.
- **Good looks like:** correct and cited, in under 10 seconds.

### 7. Memory Vault

- **Promise:** browse and check what FNDR remembered, and remove what you do not want.
- **How it works today:** list with app, time, and source filters; an expanded view with provenance, Open source, Find similar, Copy for Agent (builds a context pack as Markdown), and Delete. A separate tab lists excluded low-signal captures.
- **Known gaps:** most memories reopen only the app, not the page or file (10% specific on the owner's profile). Downloads are stored as memories but cannot be opened from the Vault.
- **Steps:** open five memories. Is the summary right? Click "Open source" on each: did it land on the exact page or document, only the app, or nothing? Pass B: download a PDF in your browser, then find it in the Vault and Search and try to open it. Delete one memory and confirm it is gone from Search.
- **Good looks like:** summaries read like something a person wrote; "Open source" lands on the exact page or file; the download is findable by its name and by what it is about; delete is obvious and final.

### 8. Agent access (Claude Code over MCP)

- **Promise:** your AI assistant can pull your work context without you pasting it.
- **How it works today:** Settings, Agent access, "Turn on agent access" starts FNDR's MCP server on `127.0.0.1:47821` (or a free port if that one is busy). Every tool call needs the token stored in `~/.fndr/mcp_token`. Access turns off when FNDR quits.
- **Known gaps:** agents can read but cannot yet add notes to the Vault (`fndr_remember_decision` writes to a separate decision ledger). There are 52 tool names with many duplicates. Privacy Activity does not show agent reads.
- **Pass C steps:** turn on agent access, click "Copy Claude Code command," run it in a terminal, start `claude`, then ask: "Use FNDR's memory.resume_work tool and tell me what I was working on." Then: "Search FNDR for the pandas fix." Then: "Record a decision in FNDR that the essay argues economic causes first."
- **Good looks like:** setup under 2 minutes; the assistant's answer is right and cites memory ids; you would do this every morning.

### 9. Daily Summary

- **Promise:** a short account of your day and what to follow up on.
- **How it works today:** a fixed template over the day's memories (apps, time span, top activities). No model. Follow-ups come from your whole task list. PDF export.
- **Steps:** open today and yesterday. Would you send any of it to a manager or teammate?
- **Good looks like:** you learn something you did not remember.

### 10. To-dos

- **Promise:** follow-ups from your work do not get lost.
- **How it works today:** a manual list with To-do, Reminder, and Follow-up types. Nothing is extracted from your screen automatically (an extractor exists in code but is not connected). Meetings can add items, but the Meetings screen is hidden.
- **Steps:** add, edit, and complete one item. Ask yourself whether FNDR should have created any of Sam's next steps here.
- **Good looks like:** you would use this instead of your current list. If not, say which list you use.

### 11. Stats

- **Promise:** understand how you spent your time.
- **How it works today:** counts and charts over stored memories (apps, time, signal health).
- **Steps:** open it once. Did it change anything you would do?

### 12. FNDR Wrapped

- **Promise:** a weekly or monthly recap you might share.
- **How it works today:** aggregates for a chosen week or month with PDF export.
- **Steps:** generate last week. Would you share it?

### 13. Screen Guide

- **Promise:** ask a question about what is on your screen right now.
- **How it works today:** only when you invoke it (shortcut or panel), FNDR takes one in-memory capture of the main display, sends it with your question (typed or spoken) to the local vision model, and shows the answer in an overlay with an optional pointer. Nothing is stored. Needs the optional Qwen model, Screen Recording, and the microphone for voice.
- **Steps:** open a spreadsheet or a PDF and ask "What is the total in column D?" or "Summarize this page."
- **Good looks like:** correct in under 10 seconds, and better than asking ChatGPT or Claude with a screenshot.

### 14. "Seen before" toasts

- **Promise:** FNDR reminds you when you return to something you already worked on.
- **How it works today:** every 30 seconds FNDR embeds the current context, searches recent memories, and shows the first match above 0.82 similarity that it has not shown recently. It does not say why. The planned error-to-prior-fix feature (Deja vu) is not built.
- **Pass B steps:** count toasts per day and how many were useful.
- **Good looks like:** at most a few per day, each one useful.

### 15. Engine diagnostics

- **Promise:** developers can see stage timings and counters.
- **Steps:** open once. Audience is developers; score whether it belongs in the main sidebar.

### 16. Voice input

- **Promise:** say what you want instead of typing it.
- **How it works today:** the window records audio in the browser engine, writes it to a temporary file, then transcribes it with a whisper.cpp command-line tool if one is installed, otherwise with a Python helper that loads the 466 MB Whisper small model from disk for every request. The text comes back all at once; there is no partial text while you speak. In Search, a few hard-coded phrases ("pause capture," "open memory vault") act as commands; anything else becomes a search.
- **Steps:** ten short utterances each on Home, in Search, and in Screen Guide: five searches ("the pandas fix from last week"), three commands ("pause capture"), two you wish worked ("open the essay draft," "remind me to email Dana at 4"). Time from releasing the button to text appearing. Count failures and wrong words.
- **Good looks like:** text appears in under a second, nine of ten are right, and you can see what stage it is in while you wait.

## Not testable in this build

Deja vu (error to prior fix), agent write-back into the Vault, cloud reasoning, Meetings screen, full knowledge graph, Quick Find (Alt+Space) and Autofill (switched off for the Alpha), approve-then-act agent, phone companion. List any of these you missed during Pass B.

## The Resume task (do this at the end of Pass B)

1. At the end of a working day, write down (on paper) the three things you were in the middle of.
2. The next morning, before opening anything, start a stopwatch. Using only your normal tools (browser history, recent files, Slack, email), get back to the exact place in thread 1 and write its next step. Stop the watch. Record the time.
3. For thread 2, open FNDR Home first, then continue. Record the time.
4. For thread 3, ask Claude Code with FNDR connected: "What was I doing on <thread> and what is next?" Record the time and whether the answer was right.

| Thread | Method | Seconds | Correct? (yes, partly, no) | Notes |
|---|---|---|---|---|
| 1 | Normal tools | | | |
| 2 | FNDR Home | | | |
| 3 | Claude Code plus FNDR | | | |

## Scorecard

| # | Feature | Pass | Useful 1 to 5 | Trust 1 to 5 | Verdict | One sentence |
|---|---|---|---|---|---|---|
| 1 | First run and permissions | B | | | | |
| 2 | Capture and pause | B | | | | |
| 3 | Privacy Activity and blocklist | B | | | | |
| 4 | Resume Work | A and B | | | | |
| 5 | Search | A and B | | | | |
| 6 | Ask FNDR | A and B | | | | |
| 7 | Memory Vault | A | | | | |
| 8 | Agent access | C | | | | |
| 9 | Daily Summary | A | | | | |
| 10 | To-dos | A | | | | |
| 11 | Stats | A | | | | |
| 12 | Wrapped | A | | | | |
| 13 | Screen Guide | B | | | | |
| 14 | Seen-before toasts | B | | | | |
| 15 | Engine diagnostics | A | | | | |
| 16 | Voice input | B | | | | |

## Pass D numbers

Paste the summary lines from `make vault-health` and the first table from `make qa-retrieval` here (counts and scores only).

## Open notes

Anything that surprised you, anything you wanted and could not do, and the one feature you would build next.
````

- [ ] **Step 2: Verify the doc has every card and no dashes that break the house style**

Run: `grep -c '^### ' docs/product/qa-walkthrough.md && grep -nP '[\x{2013}\x{2014}]' docs/product/qa-walkthrough.md; echo "exit $?"`
Expected: `16` cards, then no dash matches and `exit 1` from the second grep.

- [ ] **Step 3: Commit**

The branch `qa/user-first-reset` already exists and holds this plan and the month plan.

```bash
git switch qa/user-first-reset
git add docs/product/qa-walkthrough.md
git commit -m "docs(qa): hands-on walkthrough and scorecard for seeded, live, and agent passes"
```

### Task 2: Seeded QA profile with structured fields

**Files:**
- Modify: `src-tauri/examples/seed_demo.rs`
- Create: `scripts/demo/knowledge-worker-week.json`
- Modify: `scripts/demo/seed-demo-profile.sh`
- Modify: `Makefile`

**Interfaces:**
- Consumes: `MemoryRecord` fields `project: String`, `topic: String`, `outcome: String`, `next_steps: Vec<String>`, `decisions: Vec<String>`, `errors: Vec<String>` (`src-tauri/src/storage/schema.rs:300-418`).
- Produces: seed entries may carry those six fields plus `minutes_ago: Option<i64>`; `make qa-seed` builds `~/Library/Application Support/com.fndr.app.qa`; `make qa-app` launches FNDR on it. The corpus memory ids (`kw-*`) are used as fixtures by Task 3's preview data and by later Deja vu work.

- [ ] **Step 1: Write the failing tests at the bottom of `src-tauri/examples/seed_demo.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> SeedEntry {
        serde_json::from_str(json).expect("seed entry")
    }

    fn fixed_now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 9, 24, 15, 0, 0).unwrap()
    }

    fn knowledge_worker_corpus() -> Vec<SeedEntry> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../scripts/demo/knowledge-worker-week.json"
        );
        serde_json::from_slice(&std::fs::read(path).expect("corpus file")).expect("valid corpus")
    }

    #[test]
    fn legacy_entries_without_structured_fields_still_parse() {
        let entry = parse(
            r#"{"id":"a","day_offset":-1,"time":"09:00","app_name":"Notion",
                "window_title":"W","summary":"S","ocr_text":"T","session":"s1"}"#,
        );
        assert!(entry.project.is_empty());
        assert!(entry.next_steps.is_empty());
        assert_eq!(entry.minutes_ago, None);
    }

    #[test]
    fn minutes_ago_overrides_day_offset_and_time() {
        let entry = parse(
            r#"{"id":"a","minutes_ago":90,"app_name":"Google Chrome",
                "window_title":"W","summary":"S","ocr_text":"T","session":"s1"}"#,
        );
        assert_eq!(
            timestamp_ms(&entry, fixed_now()),
            fixed_now().timestamp_millis() - 90 * 60_000
        );
    }

    #[test]
    fn structured_fields_reach_the_memory_record() {
        let entry = parse(
            r#"{"id":"a","minutes_ago":30,"app_name":"Google Chrome","window_title":"W",
                "summary":"S","ocr_text":"T","session":"s1",
                "project":"HIST 2100 essay: Reconstruction",
                "topic":"Drafting the counterargument section",
                "outcome":"Draft at 1,450 words.",
                "next_steps":["Finish the counterargument paragraph"],
                "decisions":["Argue economic causes first"],
                "errors":[]}"#,
        );
        let record = record_at(&entry, vec![0.0; EMBEDDING_DIM], fixed_now());
        assert_eq!(record.project, "HIST 2100 essay: Reconstruction");
        assert_eq!(record.topic, "Drafting the counterargument section");
        assert_eq!(record.outcome, "Draft at 1,450 words.");
        assert_eq!(record.next_steps, vec!["Finish the counterargument paragraph".to_string()]);
        assert_eq!(record.decisions, vec!["Argue economic causes first".to_string()]);
        assert!(record.errors.is_empty());
    }

    #[test]
    fn knowledge_worker_corpus_has_unique_ids_and_resumable_threads() {
        let entries = knowledge_worker_corpus();
        let ids: std::collections::HashSet<&str> = entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids.len(), entries.len(), "ids must be unique");
        let recent_projects: std::collections::HashSet<&str> = entries
            .iter()
            .filter(|e| e.minutes_ago.is_some_and(|m| m <= 48 * 60) && !e.project.is_empty())
            .map(|e| e.project.as_str())
            .collect();
        assert!(
            recent_projects.len() >= 3,
            "need at least three projects inside a 48 hour window, got {recent_projects:?}"
        );
        assert_eq!(entries.iter().filter(|e| e.low_signal).count(), 1);
    }

    #[test]
    fn knowledge_worker_corpus_repeats_one_error_days_apart() {
        let entries = knowledge_worker_corpus();
        let with_error: Vec<&SeedEntry> = entries
            .iter()
            .filter(|e| {
                e.errors
                    .iter()
                    .any(|err| err == "ModuleNotFoundError: No module named 'pandas'")
            })
            .collect();
        assert!(with_error.iter().any(|e| e.minutes_ago.is_some()), "the error recurs today");
        assert!(
            with_error
                .iter()
                .any(|e| e.minutes_ago.is_none() && e.day_offset <= -2 && !e.decisions.is_empty()),
            "an older memory holds the fix"
        );
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test --example seed_demo`
Expected: compile FAIL: no field `project` on `SeedEntry`, cannot find function `record_at`, and `timestamp_ms` takes 2 arguments of different types.

- [ ] **Step 3: Implement in `seed_demo.rs`**

Change the chrono import on line 5:

```rust
use chrono::{DateTime, Duration as ChronoDuration, Local, NaiveTime, TimeZone};
```

Replace the `SeedEntry` struct with:

```rust
#[derive(Deserialize)]
struct SeedEntry {
    id: String,
    #[serde(default)]
    day_offset: i64,
    #[serde(default)]
    time: String,
    /// Places the entry relative to seeding time, so a Resume window always has recent threads.
    #[serde(default)]
    minutes_ago: Option<i64>,
    app_name: String,
    #[serde(default)]
    bundle_id: Option<String>,
    window_title: String,
    #[serde(default)]
    url: Option<String>,
    summary: String,
    ocr_text: String,
    session: String,
    #[serde(default)]
    low_signal: bool,
    // Fields the local model normally fills at capture time. Seeding them lets
    // QA judge a screen separately from extraction quality.
    #[serde(default)]
    project: String,
    #[serde(default)]
    topic: String,
    #[serde(default)]
    outcome: String,
    #[serde(default)]
    next_steps: Vec<String>,
    #[serde(default)]
    decisions: Vec<String>,
    #[serde(default)]
    errors: Vec<String>,
}
```

Replace `fn timestamp_ms(day_offset: i64, time: &str) -> i64` with:

```rust
fn timestamp_ms(entry: &SeedEntry, now: DateTime<Local>) -> i64 {
    if let Some(minutes) = entry.minutes_ago {
        return now.timestamp_millis() - minutes * 60_000;
    }
    let t = NaiveTime::parse_from_str(&entry.time, "%H:%M").expect("time HH:MM");
    let date = now.date_naive() + ChronoDuration::days(entry.day_offset);
    Local
        .from_local_datetime(&date.and_time(t))
        .single()
        .expect("unambiguous local time")
        .timestamp_millis()
}
```

Rename `fn record(entry: &SeedEntry, embedding: Vec<f32>) -> MemoryRecord` to `fn record_at(entry: &SeedEntry, embedding: Vec<f32>, now: DateTime<Local>) -> MemoryRecord`, change its first line to `let ts = timestamp_ms(entry, now);`, and add these six lines inside the `MemoryRecord { ... }` literal, just above `..Default::default()`:

```rust
        project: entry.project.clone(),
        topic: entry.topic.clone(),
        outcome: entry.outcome.clone(),
        next_steps: entry.next_steps.clone(),
        decisions: entry.decisions.clone(),
        errors: entry.errors.clone(),
```

In `main`, add `let now = Local::now();` right after `let embedder = Embedder::new()?;`, and change `records.push(record(entry, vector));` to `records.push(record_at(entry, vector, now));`.

- [ ] **Step 4: Create `scripts/demo/knowledge-worker-week.json`**

```json
[
  {
    "id": "kw-hist-outline",
    "day_offset": -4, "time": "10:15",
    "app_name": "Google Chrome", "bundle_id": "com.google.Chrome",
    "window_title": "Reconstruction essay outline - Google Docs",
    "url": "https://docs.google.com/document/d/hist2100-essay-outline/edit",
    "summary": "Outlined the HIST 2100 Reconstruction essay with an economics-first thesis; still need two primary sources.",
    "ocr_text": "Reconstruction essay outline\nThesis: Reconstruction failed economically before it failed politically\nI. Land redistribution promises (Field Order 15)\nII. Freedmen's Bureau funding and labor contracts\nIII. Compromise of 1877\nSources needed: 2 primary, 3 secondary\nDue Friday 11:59 PM on Canvas",
    "session": "hist-essay-outline",
    "project": "HIST 2100 essay: Reconstruction",
    "topic": "Outlining the Reconstruction essay",
    "outcome": "Thesis drafted: Reconstruction failed economically before it failed politically.",
    "next_steps": ["Find two primary sources on the Freedmen's Bureau"],
    "decisions": ["Structure the essay economic causes first, political second"],
    "errors": []
  },
  {
    "id": "kw-hist-rubric",
    "day_offset": -4, "time": "10:40",
    "app_name": "Google Chrome", "bundle_id": "com.google.Chrome",
    "window_title": "Essay 2 rubric - HIST 2100 - Canvas",
    "url": "https://canvas.example.edu/courses/2100/assignments/7",
    "summary": "Read the Essay 2 rubric: evidence use is 40 percent of the grade and two primary sources are required.",
    "ocr_text": "Essay 2: Reconstruction\nLength: 1,800 to 2,200 words\nRubric\nThesis clarity 20%\nUse of evidence 40% (at least two primary sources)\nCounterargument 20%\nMechanics 20%\nSubmit as PDF",
    "session": "hist-essay-outline",
    "project": "HIST 2100 essay: Reconstruction",
    "topic": "Reading the essay 2 rubric",
    "outcome": "Rubric weights evidence use at 40 percent.",
    "next_steps": ["Cite at least two primary sources"],
    "decisions": [],
    "errors": []
  },
  {
    "id": "kw-hist-reader",
    "minutes_ago": 1560,
    "app_name": "Preview", "bundle_id": "com.apple.Preview",
    "window_title": "Harlan - The Unfinished Settlement - course reader ch4.pdf (page 112 of 160)",
    "summary": "Read chapter 4 of the course reader on Bureau labor contracts and found a usable passage on page 112.",
    "ocr_text": "CHAPTER 4\nContracts and Consent\nBureau agents pressed freedpeople to sign annual labor contracts\nthat tied wages to the harvest and limited movement between employers.\nBy 1867 most contracts in the district paid in shares rather than cash.\n112",
    "session": "hist-reading",
    "project": "HIST 2100 essay: Reconstruction",
    "topic": "Reading course reader chapter 4 on Bureau labor contracts",
    "outcome": "Found a usable passage on labor contracts on page 112.",
    "next_steps": ["Add the page 112 passage to the evidence section"],
    "decisions": [],
    "errors": []
  },
  {
    "id": "kw-hist-draft",
    "minutes_ago": 190,
    "app_name": "Google Chrome", "bundle_id": "com.google.Chrome",
    "window_title": "Reconstruction essay draft - Google Docs",
    "url": "https://docs.google.com/document/d/hist2100-essay-draft/edit",
    "summary": "Drafted the essay to 1,450 words; the counterargument paragraph is half written and the page 112 quote is not in yet.",
    "ocr_text": "Reconstruction essay draft\nWord count: 1,450\nCounterargument\nSome historians argue the collapse was primarily political, pointing to the Compromise of 1877.\nThis reading underplays how labor contracts had already narrowed freedpeople's choices.\n[add Harlan p. 112 passage here]",
    "session": "hist-drafting",
    "project": "HIST 2100 essay: Reconstruction",
    "topic": "Drafting the counterargument section",
    "outcome": "Draft at 1,450 words; counterargument paragraph half written.",
    "next_steps": ["Finish the counterargument paragraph", "Add the page 112 passage from the course reader", "Export as PDF and submit on Canvas by Friday"],
    "decisions": ["Use the political-failure reading as the counterargument"],
    "errors": []
  },
  {
    "id": "kw-cs-a4-spec",
    "day_offset": -5, "time": "14:05",
    "app_name": "Google Chrome", "bundle_id": "com.google.Chrome",
    "window_title": "Assignment 4: Data pipeline - CS 3500 - Canvas",
    "url": "https://canvas.example.edu/courses/3500/assignments/4",
    "summary": "Read the CS 3500 Assignment 4 spec: clean the transit dataset and produce three matplotlib charts.",
    "ocr_text": "Assignment 4: Data pipeline\nInput: data/transit.csv (ridership by stop and day)\n1. clean.py removes rows with missing stop ids\n2. charts.py produces three charts with matplotlib only\n3. Submit a zip with a README\nDue next Thursday",
    "session": "cs-a4-start",
    "project": "CS 3500 Assignment 4",
    "topic": "Reading the assignment 4 spec",
    "outcome": "Need a cleaning script and three charts from the transit dataset.",
    "next_steps": ["Set up the course virtual environment"],
    "decisions": [],
    "errors": []
  },
  {
    "id": "kw-cs-a4-error-old",
    "day_offset": -5, "time": "14:40",
    "app_name": "Terminal", "bundle_id": "com.apple.Terminal",
    "window_title": "zsh - a4",
    "summary": "clean.py failed on the first run because Python could not find pandas.",
    "ocr_text": "$ python3 clean.py data/transit.csv\nTraceback (most recent call last):\n  File \"/Users/sam/cs3500/a4/clean.py\", line 3, in <module>\n    import pandas as pd\nModuleNotFoundError: No module named 'pandas'",
    "session": "cs-a4-start",
    "project": "CS 3500 Assignment 4",
    "topic": "Running clean.py for the first time",
    "outcome": "Script failed because pandas was not found.",
    "next_steps": [],
    "decisions": [],
    "errors": ["ModuleNotFoundError: No module named 'pandas'"]
  },
  {
    "id": "kw-cs-a4-fix-old",
    "day_offset": -5, "time": "14:52",
    "app_name": "Google Chrome", "bundle_id": "com.google.Chrome",
    "window_title": "Why can't Python find pandas? - CS 3500 Q&A",
    "url": "https://piazza.example.com/class/cs3500/post/212",
    "summary": "Fixed the missing pandas import: the course virtual environment was not active; activating it resolved the error.",
    "ocr_text": "Q: ModuleNotFoundError: No module named 'pandas'\nTA answer (Priya): You are running the system Python. Activate the course environment first:\n  source .venv/bin/activate\n  python clean.py data/transit.csv\n14 students found this helpful",
    "session": "cs-a4-start",
    "project": "CS 3500 Assignment 4",
    "topic": "Fixing the missing pandas import",
    "outcome": "Fixed: the course venv was not active; activating it resolved the import.",
    "next_steps": [],
    "decisions": ["Always run source .venv/bin/activate before running course scripts"],
    "errors": ["ModuleNotFoundError: No module named 'pandas'"]
  },
  {
    "id": "kw-cs-a4-charts",
    "day_offset": -2, "time": "20:10",
    "app_name": "Visual Studio Code", "bundle_id": "com.microsoft.VSCode",
    "window_title": "charts.py - a4 - Visual Studio Code",
    "summary": "Built two of the three ridership charts; the weekday versus weekend chart is still missing.",
    "ocr_text": "charts.py\ndef ridership_by_stop(df):\n    ...\ndef ridership_by_hour(df):\n    ...\n# TODO weekday vs weekend chart\nif __name__ == \"__main__\":\n    main()",
    "session": "cs-a4-charts",
    "project": "CS 3500 Assignment 4",
    "topic": "Building the ridership charts",
    "outcome": "Two of three charts done; weekday versus weekend chart remains.",
    "next_steps": ["Build the weekday versus weekend ridership chart"],
    "decisions": ["Use matplotlib only, no seaborn, per the spec"],
    "errors": []
  },
  {
    "id": "kw-cs-a4-vscode-new",
    "minutes_ago": 70,
    "app_name": "Visual Studio Code", "bundle_id": "com.microsoft.VSCode",
    "window_title": "charts.py - a4 - Visual Studio Code",
    "summary": "Wrote the weekday versus weekend chart function but have not run it yet.",
    "ocr_text": "charts.py\ndef weekday_vs_weekend(df):\n    df[\"is_weekend\"] = df[\"date\"].dt.dayofweek >= 5\n    totals = df.groupby(\"is_weekend\")[\"riders\"].sum()\n    totals.plot(kind=\"bar\")\n    plt.savefig(\"out/weekday_weekend.png\")",
    "session": "cs-a4-final",
    "project": "CS 3500 Assignment 4",
    "topic": "Writing the weekday versus weekend chart",
    "outcome": "Chart function written, not yet run.",
    "next_steps": ["Run charts.py and check the weekend bars"],
    "decisions": [],
    "errors": []
  },
  {
    "id": "kw-cs-a4-error-new",
    "minutes_ago": 55,
    "app_name": "Terminal", "bundle_id": "com.apple.Terminal",
    "window_title": "zsh - a4",
    "summary": "charts.py failed again on the pandas import after restarting the laptop.",
    "ocr_text": "$ python3 charts.py\nTraceback (most recent call last):\n  File \"/Users/sam/cs3500/a4/charts.py\", line 2, in <module>\n    import pandas as pd\nModuleNotFoundError: No module named 'pandas'",
    "session": "cs-a4-final",
    "project": "CS 3500 Assignment 4",
    "topic": "Running the chart script after a restart",
    "outcome": "Script failed again on the pandas import.",
    "next_steps": ["Fix the pandas import and rerun charts.py"],
    "decisions": [],
    "errors": ["ModuleNotFoundError: No module named 'pandas'"]
  },
  {
    "id": "kw-intern-brief",
    "day_offset": -3, "time": "09:30",
    "app_name": "Slack", "bundle_id": "com.tinyspeck.slackmacgap",
    "window_title": "#insights-team - Northwind Analytics - Slack",
    "summary": "Dana asked Sam to own a one-page Q4 onboarding survey readout with the top three churn drivers by Thursday.",
    "ocr_text": "Dana Kim 9:24 AM\n@sam can you own the Q4 onboarding survey readout? One page, top 3 churn drivers, by Thursday end of day\nsam 9:27 AM\nOn it. Is the export in the shared drive?\nDana Kim 9:28 AM\nYes, Survey Exports / Q4_onboarding.csv",
    "session": "intern-q4-brief",
    "project": "Internship: Q4 onboarding survey",
    "topic": "Receiving the Q4 survey readout brief from Dana",
    "outcome": "Dana wants a one-page readout by Thursday with the top three churn drivers.",
    "next_steps": ["Clean the survey export", "Draft the one-page readout"],
    "decisions": [],
    "errors": []
  },
  {
    "id": "kw-intern-clean",
    "day_offset": -3, "time": "11:10",
    "app_name": "Google Chrome", "bundle_id": "com.google.Chrome",
    "window_title": "Q4 onboarding survey - Google Sheets",
    "url": "https://docs.google.com/spreadsheets/d/q4-onboarding-survey/edit",
    "summary": "Cleaned the survey export: removed 38 duplicates and speed-run responses, leaving 412 valid rows.",
    "ocr_text": "Q4 onboarding survey\nRows: 450 raw, 412 valid\nFilter: completion_seconds >= 60\nRemoved duplicates by respondent_id: 38\nColumns: plan, seats, setup_days, nps, open_text",
    "session": "intern-q4-cleaning",
    "project": "Internship: Q4 onboarding survey",
    "topic": "Cleaning the survey export",
    "outcome": "Removed 38 duplicate responses; 412 valid rows remain.",
    "next_steps": ["Code the open-text answers into themes"],
    "decisions": ["Drop responses completed in under 60 seconds"],
    "errors": []
  },
  {
    "id": "kw-intern-themes",
    "minutes_ago": 1320,
    "app_name": "Google Chrome", "bundle_id": "com.google.Chrome",
    "window_title": "Q4 onboarding survey - Themes - Google Sheets",
    "url": "https://docs.google.com/spreadsheets/d/q4-onboarding-survey/edit#gid=themes",
    "summary": "Coded 292 of 412 open-text answers; top themes are slow setup, unclear pricing, and missing single sign-on.",
    "ocr_text": "Themes tab\nCoded: 292 of 412\nSETUP_SLOW 31%\nPRICING_UNCLEAR 22%\nNO_SSO 17%\nSUPPORT_WAIT 9%\nOTHER 21%",
    "session": "intern-q4-themes",
    "project": "Internship: Q4 onboarding survey",
    "topic": "Coding open-text answers into themes",
    "outcome": "Top themes so far: slow setup, unclear pricing, missing single sign-on.",
    "next_steps": ["Finish coding the last 120 responses", "Draft the readout in the team slide template"],
    "decisions": ["Use five theme codes at most so the readout stays one page"],
    "errors": []
  },
  {
    "id": "kw-intern-email",
    "minutes_ago": 1260,
    "app_name": "Google Chrome", "bundle_id": "com.google.Chrome",
    "window_title": "Re: Q4 readout format - Gmail",
    "url": "https://mail.google.com/mail/u/0/#inbox/q4-readout-format",
    "summary": "Dana confirmed the readout should use the team slide template and include one customer quote per theme.",
    "ocr_text": "Re: Q4 readout format\nDana Kim to me\nSlides are better than a doc for this one. Use the team template.\nPlease add one customer quote under each theme.\nThursday end of day still works.",
    "session": "intern-q4-themes",
    "project": "Internship: Q4 onboarding survey",
    "topic": "Confirming the readout format with Dana",
    "outcome": "Dana confirmed the slide template and asked for one quote per theme.",
    "next_steps": ["Pull one customer quote per theme"],
    "decisions": ["Use the team slide template, not a doc"],
    "errors": []
  },
  {
    "id": "kw-cap-standup",
    "day_offset": -2, "time": "16:00",
    "app_name": "zoom.us", "bundle_id": "us.zoom.xos",
    "window_title": "Capstone weekly standup - Zoom",
    "summary": "Capstone standup: Sam owns the demo script and slides, Marco owns the backup video.",
    "ocr_text": "Capstone weekly standup\nAgenda\n1. Demo day logistics\n2. Owners: Sam demo script and slides, Marco backup video, Lena donor report feature\n3. Risk: barcode scanner flaky on older phones\nNext standup Monday",
    "session": "capstone-standup",
    "project": "Capstone: Pantry app demo",
    "topic": "Weekly capstone standup",
    "outcome": "Sam owns the demo script and slides; Marco owns the backup video.",
    "next_steps": ["Write the five-minute demo script"],
    "decisions": ["Demo opens with the inventory scan, not the login screen"],
    "errors": []
  },
  {
    "id": "kw-cap-script",
    "day_offset": -2, "time": "16:45",
    "app_name": "Notion", "bundle_id": "notion.id",
    "window_title": "Demo script v1 - Pantry App Capstone",
    "summary": "Wrote demo script v1 covering the inventory scan, low-stock alert, and donor report; timing not yet checked.",
    "ocr_text": "Demo script v1\n0:00 Problem: pantry volunteers track stock on paper\n0:40 Scan a can, stock updates live\n1:30 Low-stock alert to the coordinator\n2:30 Donor report export\n3:30 What is next\nTODO: time it with a stopwatch",
    "session": "capstone-script",
    "project": "Capstone: Pantry app demo",
    "topic": "Writing demo script v1",
    "outcome": "Script covers scan, alert, and donor report; timing unknown.",
    "next_steps": ["Time the script with a stopwatch"],
    "decisions": [],
    "errors": []
  },
  {
    "id": "kw-cap-slides",
    "minutes_ago": 300,
    "app_name": "Keynote", "bundle_id": "com.apple.iWork.Keynote",
    "window_title": "Pantry App - Final Demo.key",
    "summary": "Built demo slides 1 to 6; the architecture slide is still blank.",
    "ocr_text": "Pantry App - Final Demo\n1 Title\n2 The problem\n3 Live demo\n4 Results from the pilot pantry\n5 Team\n6 Thank you\n7 Architecture (empty)",
    "session": "capstone-slides",
    "project": "Capstone: Pantry app demo",
    "topic": "Building the demo slides",
    "outcome": "Slides 1 to 6 done; architecture slide still blank.",
    "next_steps": ["Finish the architecture slide", "Send the deck to Marco for the backup video"],
    "decisions": ["Keep the deck to eight slides"],
    "errors": []
  },
  {
    "id": "kw-cap-slack",
    "minutes_ago": 240,
    "app_name": "Slack", "bundle_id": "com.tinyspeck.slackmacgap",
    "window_title": "#capstone-pantry - Slack",
    "summary": "Marco needs the final deck by Wednesday night to record the backup video.",
    "ocr_text": "Marco 3:12 PM\nI can record the backup video Thursday morning\nNeed the final deck by Wednesday night though\nsam 3:15 PM\nWill send it once the architecture slide is done",
    "session": "capstone-slides",
    "project": "Capstone: Pantry app demo",
    "topic": "Coordinating the backup video with Marco",
    "outcome": "Marco needs the final deck by Wednesday night to record.",
    "next_steps": ["Send the deck to Marco by Wednesday night"],
    "decisions": [],
    "errors": []
  },
  {
    "id": "kw-inbox-triage",
    "minutes_ago": 30,
    "app_name": "Google Chrome", "bundle_id": "com.google.Chrome",
    "window_title": "Inbox (14) - Gmail",
    "url": "https://mail.google.com/mail/u/0/#inbox",
    "summary": "Triaged the inbox and flagged the registrar email about spring enrollment opening Monday.",
    "ocr_text": "Inbox (14)\nRegistrar: Spring enrollment opens Monday 8:00 AM\nDana Kim: Re: Q4 readout format\nCapstone Pantry: Marco shared a folder\nCampus Rec: Intramural signups",
    "session": "inbox-triage",
    "project": "",
    "topic": "Triaging the inbox",
    "outcome": "Flagged the registrar email about spring enrollment.",
    "next_steps": ["Register for spring classes when enrollment opens Monday"],
    "decisions": [],
    "errors": []
  },
  {
    "id": "kw-low-signal-desktop",
    "day_offset": -6, "time": "08:02",
    "app_name": "Finder", "bundle_id": "com.apple.finder",
    "window_title": "Desktop",
    "summary": "",
    "ocr_text": "",
    "session": "desktop",
    "low_signal": true
  }
]
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test --example seed_demo`
Expected: PASS, 5 tests.

- [ ] **Step 6: Let the seed script take a corpus path, and add the Make targets**

In `scripts/demo/seed-demo-profile.sh`, add below the `DEMO=` line:

```bash
CORPUS="${FNDR_DEMO_CORPUS:-$REPO/scripts/demo/demo-week.json}"
```

and change the last `cargo run` line's argument `--corpus "$REPO/scripts/demo/demo-week.json"` to `--corpus "$CORPUS"`.

In `Makefile`, add `qa-seed qa-app qa-preview` to the `.PHONY` line and append:

```make
QA_PROFILE := $(HOME)/Library/Application Support/com.fndr.app.qa
QA_CORPUS := $(CURDIR)/scripts/demo/knowledge-worker-week.json

qa-seed:
	FNDR_DEMO_DIR="$(QA_PROFILE)" FNDR_DEMO_CORPUS="$(QA_CORPUS)" ./scripts/demo/seed-demo-profile.sh --reset

qa-app:
	FNDR_DEMO_DIR="$(QA_PROFILE)" FNDR_DEMO_CORPUS="$(QA_CORPUS)" ./scripts/demo/run-demo.sh

qa-preview:
	@echo "When Vite is up, open http://localhost:1420/ui-preview.html"
	npm run dev
```

- [ ] **Step 7: Seed for real and check the count**

Run: `make qa-seed`
Expected: last lines include `seeded 20 records into .../com.fndr.app.qa`, `needs-signal 1` (the seeder exits with "quality gate mismatch" if any other entry is judged low signal; if so, lengthen that entry's `ocr_text`), and `Demo profile ready`. A `warning: insert-time dedup merged` line is acceptable; record it in the commit message if it appears. The command must not touch `com.fndr.app`.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/examples/seed_demo.rs scripts/demo/knowledge-worker-week.json scripts/demo/seed-demo-profile.sh Makefile
git commit -m "feat(qa): seeded knowledge-worker profile with structured fields for Resume QA"
```

### Task 3: Resume Work on Home

**Files:**
- Modify: `src/shared/ipc/tauri.ts` (add after `getPrivacyProof`, near line 1515)
- Create: `src/domains/resume/resumeFormat.ts`, `src/domains/resume/resumeFormat.test.ts`
- Create: `src/domains/resume/ResumeWork.tsx`, `src/domains/resume/ResumeWork.test.tsx`, `src/domains/resume/ResumeWork.css`
- Modify: `src/app/App.tsx` (the `home-hero-stage` block near line 633), `src/app/App.onboarding.test.tsx` (IPC mock near line 48)
- Modify: `src/dev/previewIpc.ts`, `src/dev/previewIpc.test.ts`

**Interfaces:**
- Consumes: Tauri command `resume_work(hours: u32, budget_tokens: usize) -> Vec<ResumeThread>` (`src-tauri/src/resume/mod.rs:128`), serialized as `{ title, last_state, age_minutes, next_steps, evidence, pack: { items: [{ memory_id, text, ts_ms }], dropped_for_budget, estimated_tokens } }`. `evidence` is ordered oldest to newest. `handleOpenMemoryById(memoryId: string)` in `App.tsx:264`.
- Produces: `resumeWork(hours: number, budgetTokens: number): Promise<ResumeThread[]>`, types `ResumeThread`, `ResumePack`, `ResumePackItem`; `formatAge(ageMinutes: number): string`; `buildAgentPack(thread: ResumeThread): string`; component `ResumeWorkSection({ onOpenMemoryById })`; constants `RESUME_WINDOW_HOURS = 72`, `RESUME_BUDGET_TOKENS = 1200`.

- [ ] **Step 1: Add the IPC types and wrapper to `src/shared/ipc/tauri.ts`**

```ts
// Resume Work

export interface ResumePackItem {
    memory_id: string;
    text: string;
    ts_ms: number;
}

export interface ResumePack {
    items: ResumePackItem[];
    dropped_for_budget: number;
    estimated_tokens: number;
}

export interface ResumeThread {
    title: string;
    last_state: string;
    age_minutes: number;
    next_steps: string[];
    /** Memory ids, oldest first. */
    evidence: string[];
    pack: ResumePack;
}

export async function resumeWork(hours: number, budgetTokens: number): Promise<ResumeThread[]> {
    return invoke<ResumeThread[]>("resume_work", { hours, budgetTokens });
}
```

- [ ] **Step 2: Write the failing helper tests in `src/domains/resume/resumeFormat.test.ts`**

```ts
import { describe, expect, it } from "vitest";
import type { ResumeThread } from "@/shared/ipc/tauri";
import { buildAgentPack, formatAge } from "./resumeFormat";

const thread: ResumeThread = {
    title: "HIST 2100 essay: Reconstruction",
    last_state: "Drafting the counterargument section Draft at 1,450 words.",
    age_minutes: 190,
    next_steps: ["Finish the counterargument paragraph"],
    evidence: ["kw-hist-reader", "kw-hist-draft"],
    pack: {
        items: [{ memory_id: "kw-hist-draft", text: "Finish the counterargument paragraph", ts_ms: 1 }],
        dropped_for_budget: 2,
        estimated_tokens: 9,
    },
};

describe("formatAge", () => {
    it.each([
        [0, "just now"],
        [42, "42 min ago"],
        [190, "3 h ago"],
        [1500, "1 day ago"],
        [4000, "2 days ago"],
    ])("formats %i minutes as %s", (minutes, label) => {
        expect(formatAge(minutes)).toBe(label);
    });
});

describe("buildAgentPack", () => {
    it("keeps the memory id on every evidence line and reports budget drops", () => {
        const text = buildAgentPack(thread);
        expect(text).toContain("Resume context from FNDR: HIST 2100 essay: Reconstruction (last active 3 h ago)");
        expect(text).toContain("Where I left off: Drafting the counterargument section Draft at 1,450 words.");
        expect(text).toContain("- Finish the counterargument paragraph");
        expect(text).toContain("- [kw-hist-draft] Finish the counterargument paragraph");
        expect(text).toContain("(2 more items left out to fit the size limit)");
    });

    it("omits empty sections instead of printing blank headings", () => {
        const text = buildAgentPack({
            ...thread,
            last_state: " ",
            next_steps: [],
            pack: { items: [], dropped_for_budget: 0, estimated_tokens: 0 },
        });
        expect(text).not.toContain("Where I left off");
        expect(text).not.toContain("Next steps:");
        expect(text).not.toContain("Evidence:");
        expect(text).not.toContain("left out");
    });
});
```

- [ ] **Step 3: Run to verify they fail**

Run: `npx vitest run src/domains/resume/resumeFormat.test.ts`
Expected: FAIL, cannot resolve `./resumeFormat`.

- [ ] **Step 4: Implement `src/domains/resume/resumeFormat.ts`**

```ts
import type { ResumeThread } from "@/shared/ipc/tauri";

/** Plain age label: "just now", "42 min ago", "3 h ago", "1 day ago", "2 days ago". */
export function formatAge(ageMinutes: number): string {
    if (ageMinutes < 1) return "just now";
    if (ageMinutes < 60) return `${ageMinutes} min ago`;
    const hours = Math.floor(ageMinutes / 60);
    if (hours < 24) return `${hours} h ago`;
    const days = Math.floor(hours / 24);
    return days === 1 ? "1 day ago" : `${days} days ago`;
}

/** Text a person pastes into any AI assistant to resume a thread.
 *  Every evidence line keeps its memory id so the assistant can cite it. */
export function buildAgentPack(thread: ResumeThread): string {
    const lines = [`Resume context from FNDR: ${thread.title} (last active ${formatAge(thread.age_minutes)})`];
    if (thread.last_state.trim()) {
        lines.push(`Where I left off: ${thread.last_state.trim()}`);
    }
    if (thread.next_steps.length > 0) {
        lines.push("Next steps:");
        thread.next_steps.forEach((step) => lines.push(`- ${step}`));
    }
    if (thread.pack.items.length > 0) {
        lines.push("Evidence:");
        thread.pack.items.forEach((item) => lines.push(`- [${item.memory_id}] ${item.text.trim()}`));
    }
    if (thread.pack.dropped_for_budget > 0) {
        lines.push(`(${thread.pack.dropped_for_budget} more items left out to fit the size limit)`);
    }
    return lines.join("\n");
}
```

- [ ] **Step 5: Run to verify they pass**

Run: `npx vitest run src/domains/resume/resumeFormat.test.ts`
Expected: PASS, 7 tests.

- [ ] **Step 6: Write the failing component tests in `src/domains/resume/ResumeWork.test.tsx`**

```tsx
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { RESUME_BUDGET_TOKENS, RESUME_WINDOW_HOURS, ResumeWorkSection } from "./ResumeWork";

vi.mock("@/shared/ipc/tauri", () => ({
    resumeWork: vi.fn(),
}));

import { resumeWork } from "@/shared/ipc/tauri";

const thread = {
    title: "CS 3500 Assignment 4",
    last_state: "Running the chart script after a restart Script failed again on the pandas import.",
    age_minutes: 55,
    next_steps: [
        "Fix the pandas import and rerun charts.py",
        "Run charts.py and check the weekend bars",
        "Build the weekday versus weekend chart",
        "A fourth step that stays hidden",
    ],
    evidence: ["kw-cs-a4-vscode-new", "kw-cs-a4-error-new"],
    pack: { items: [], dropped_for_budget: 0, estimated_tokens: 0 },
};

afterEach(() => {
    cleanup();
    vi.mocked(resumeWork).mockReset();
});

describe("ResumeWorkSection", () => {
    it("asks for the last 72 hours and shows each thread with age, state, and at most three steps", async () => {
        vi.mocked(resumeWork).mockResolvedValue([thread]);
        render(<ResumeWorkSection onOpenMemoryById={vi.fn()} />);
        expect(await screen.findByText("CS 3500 Assignment 4")).toBeInTheDocument();
        expect(resumeWork).toHaveBeenCalledWith(RESUME_WINDOW_HOURS, RESUME_BUDGET_TOKENS);
        expect(screen.getByText("55 min ago · 2 sources")).toBeInTheDocument();
        expect(screen.getByText(thread.last_state)).toBeInTheDocument();
        expect(screen.getByText("Fix the pandas import and rerun charts.py")).toBeInTheDocument();
        expect(screen.queryByText("A fourth step that stays hidden")).not.toBeInTheDocument();
    });

    it("opens the newest source first", async () => {
        vi.mocked(resumeWork).mockResolvedValue([thread]);
        const open = vi.fn();
        render(<ResumeWorkSection onOpenMemoryById={open} />);
        fireEvent.click(await screen.findByRole("button", { name: "Open latest source" }));
        expect(open).toHaveBeenCalledWith("kw-cs-a4-error-new");
    });

    it("copies a cited pack for an AI assistant", async () => {
        const writeText = vi.fn().mockResolvedValue(undefined);
        Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
        vi.mocked(resumeWork).mockResolvedValue([thread]);
        render(<ResumeWorkSection onOpenMemoryById={vi.fn()} />);
        fireEvent.click(await screen.findByRole("button", { name: "Copy for AI assistant" }));
        await waitFor(() =>
            expect(writeText).toHaveBeenCalledWith(
                expect.stringContaining("Resume context from FNDR: CS 3500 Assignment 4"),
            ),
        );
        expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();
    });

    it("says so when there is nothing to resume", async () => {
        vi.mocked(resumeWork).mockResolvedValue([]);
        render(<ResumeWorkSection onOpenMemoryById={vi.fn()} />);
        expect(await screen.findByText(/nothing to resume/i)).toBeInTheDocument();
    });

    it("shows the error and retries on request", async () => {
        vi.mocked(resumeWork).mockRejectedValueOnce("store unavailable").mockResolvedValueOnce([thread]);
        render(<ResumeWorkSection onOpenMemoryById={vi.fn()} />);
        expect(await screen.findByRole("alert")).toHaveTextContent("store unavailable");
        fireEvent.click(screen.getByRole("button", { name: "Try again" }));
        expect(await screen.findByText("CS 3500 Assignment 4")).toBeInTheDocument();
    });
});
```

- [ ] **Step 7: Run to verify they fail**

Run: `npx vitest run src/domains/resume/ResumeWork.test.tsx`
Expected: FAIL, cannot resolve `./ResumeWork`.

- [ ] **Step 8: Implement `src/domains/resume/ResumeWork.tsx` and `ResumeWork.css`**

```tsx
import { useCallback, useEffect, useState } from "react";
import { Button } from "@/shared/components/atoms";
import { resumeWork, type ResumeThread } from "@/shared/ipc/tauri";
import { buildAgentPack, formatAge } from "./resumeFormat";
import "./ResumeWork.css";

/** 72 hours covers a weekend away. */
export const RESUME_WINDOW_HOURS = 72;
export const RESUME_BUDGET_TOKENS = 1200;
const MAX_THREADS = 5;
const MAX_STEPS = 3;

type LoadState =
    | { kind: "loading" }
    | { kind: "error"; message: string }
    | { kind: "ready"; threads: ResumeThread[] };

interface ResumeWorkSectionProps {
    onOpenMemoryById: (memoryId: string) => void;
}

/** Home's answer to "what was I doing?". Shows the engine's stored fields as they are. */
export function ResumeWorkSection({ onOpenMemoryById }: ResumeWorkSectionProps) {
    const [state, setState] = useState<LoadState>({ kind: "loading" });
    const [copiedTitle, setCopiedTitle] = useState<string | null>(null);

    const load = useCallback(async () => {
        setState({ kind: "loading" });
        try {
            const threads = await resumeWork(RESUME_WINDOW_HOURS, RESUME_BUDGET_TOKENS);
            setState({ kind: "ready", threads });
        } catch (error) {
            setState({ kind: "error", message: String(error) });
        }
    }, []);

    useEffect(() => {
        void load();
    }, [load]);

    async function copyForAgent(thread: ResumeThread) {
        try {
            await navigator.clipboard.writeText(buildAgentPack(thread));
            setCopiedTitle(thread.title);
        } catch {
            setCopiedTitle(null);
        }
    }

    return (
        <section className="resume-work" aria-labelledby="resume-work-title">
            <h2 id="resume-work-title" className="resume-work-heading">
                Pick up where you left off
            </h2>
            {state.kind === "loading" && <p className="resume-work-muted">Looking at your last 3 days…</p>}
            {state.kind === "error" && (
                <div role="alert" className="resume-work-muted">
                    <p>Couldn't load your recent work: {state.message}</p>
                    <Button variant="secondary" onClick={() => void load()}>
                        Try again
                    </Button>
                </div>
            )}
            {state.kind === "ready" && state.threads.length === 0 && (
                <p className="resume-work-muted">
                    Nothing to resume from the last 3 days. Work for a while and FNDR will pick up your threads.
                </p>
            )}
            {state.kind === "ready" && state.threads.length > 0 && (
                <ul className="resume-work-list">
                    {state.threads.slice(0, MAX_THREADS).map((thread) => {
                        const sources = thread.evidence.length;
                        const newestFirst = thread.evidence.slice(-2).reverse();
                        return (
                            <li key={thread.title} className="resume-work-card">
                                <h3>{thread.title}</h3>
                                <p className="resume-work-meta">
                                    {`${formatAge(thread.age_minutes)} · ${sources} ${sources === 1 ? "source" : "sources"}`}
                                </p>
                                {thread.last_state.trim() && <p>{thread.last_state}</p>}
                                {thread.next_steps.length > 0 && (
                                    <ul className="resume-work-steps" aria-label={`Next steps for ${thread.title}`}>
                                        {thread.next_steps.slice(0, MAX_STEPS).map((step) => (
                                            <li key={step}>{step}</li>
                                        ))}
                                    </ul>
                                )}
                                <div className="resume-work-actions">
                                    {newestFirst.map((memoryId, index) => (
                                        <Button key={memoryId} variant="ghost" onClick={() => onOpenMemoryById(memoryId)}>
                                            {index === 0 ? "Open latest source" : "Open earlier source"}
                                        </Button>
                                    ))}
                                    <Button variant="secondary" onClick={() => void copyForAgent(thread)}>
                                        {copiedTitle === thread.title ? "Copied" : "Copy for AI assistant"}
                                    </Button>
                                </div>
                            </li>
                        );
                    })}
                </ul>
            )}
        </section>
    );
}
```

`src/domains/resume/ResumeWork.css`:

```css
/* Functional layout for QA. Visual design is owned by the product experience lane. */
.resume-work {
    width: min(720px, 100%);
    margin: 20px auto 0;
    display: grid;
    gap: 10px;
    text-align: left;
    max-height: 42vh;
    overflow-y: auto;
}

.resume-work-heading {
    margin: 0;
    font-family: var(--film-font-ui);
    font-size: 0.95rem;
    letter-spacing: 0.04em;
    color: var(--text-secondary);
}

.resume-work-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 10px;
}

.resume-work-card {
    border: 1px solid var(--hairline);
    background: var(--surface);
    border-radius: 12px;
    padding: 14px 16px;
    color: var(--text-primary);
}

.resume-work-card h3 {
    margin: 0 0 4px;
    font-size: 1rem;
}

.resume-work-meta,
.resume-work-muted {
    margin: 0 0 6px;
    font-size: 0.85rem;
    color: var(--text-muted);
}

.resume-work-steps {
    margin: 6px 0;
    padding-left: 18px;
}

.resume-work-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 8px;
}
```

- [ ] **Step 9: Run to verify they pass**

Run: `npx vitest run src/domains/resume`
Expected: PASS, 12 tests (7 helper, 5 component).

- [ ] **Step 10: Mount it on Home and keep the App tests' mock complete**

In `src/app/App.tsx`, add the import beside the other domain imports:

```tsx
import { ResumeWorkSection } from "@/domains/resume/ResumeWork";
```

and inside `<div className="home-hero-stage">`, directly after the closing `/>` of `<HomeHero ... />`, add:

```tsx
                        <ResumeWorkSection onOpenMemoryById={handleOpenMemoryById} />
```

In `src/app/App.onboarding.test.tsx`, inside the `vi.mock("@/shared/ipc/tauri", () => ({ ... }))` object, add:

```tsx
    resumeWork: vi.fn().mockResolvedValue([]),
```

- [ ] **Step 11: Model Resume Work in the browser preview**

In `src/dev/previewIpc.ts`, add `type ResumeThread` to the existing type import from `@/shared/ipc/tauri`, add this fixture next to `previewPrivacyProof`:

```ts
const previewResumeThreads: ResumeThread[] = [
    {
        title: "CS 3500 Assignment 4",
        last_state: "Running the chart script after a restart Script failed again on the pandas import.",
        age_minutes: 55,
        next_steps: ["Fix the pandas import and rerun charts.py"],
        evidence: ["kw-cs-a4-charts", "kw-cs-a4-vscode-new", "kw-cs-a4-error-new"],
        pack: {
            items: [
                { memory_id: "kw-cs-a4-error-new", text: "ModuleNotFoundError: No module named 'pandas'", ts_ms: 0 },
            ],
            dropped_for_budget: 0,
            estimated_tokens: 12,
        },
    },
    {
        title: "HIST 2100 essay: Reconstruction",
        last_state: "Drafting the counterargument section Draft at 1,450 words; counterargument paragraph half written.",
        age_minutes: 190,
        next_steps: ["Finish the counterargument paragraph", "Add the page 112 passage from the course reader"],
        evidence: ["kw-hist-reader", "kw-hist-draft"],
        pack: { items: [], dropped_for_budget: 0, estimated_tokens: 0 },
    },
    {
        title: "Internship: Q4 onboarding survey",
        last_state: "Confirming the readout format with Dana Dana confirmed the slide template and asked for one quote per theme.",
        age_minutes: 1260,
        next_steps: ["Pull one customer quote per theme"],
        evidence: ["kw-intern-themes", "kw-intern-email"],
        pack: { items: [], dropped_for_budget: 0, estimated_tokens: 0 },
    },
];
```

and add this case beside `case "get_privacy_proof":`:

```ts
            case "resume_work":
                return clonePreview(previewResumeThreads);
```

In `src/dev/previewIpc.test.ts`, add inside the `describe` block:

```ts
    it("models Resume Work threads without native services", async () => {
        const invoke = createPreviewIpcHandler();
        await expect(invoke("resume_work", { hours: 72, budgetTokens: 1200 })).resolves.toEqual(
            expect.arrayContaining([expect.objectContaining({ title: "HIST 2100 essay: Reconstruction" })]),
        );
    });
```

- [ ] **Step 12: Run the focused suites and typecheck**

Run: `npx vitest run src/domains/resume src/app src/dev && npm run typecheck`
Expected: all PASS, typecheck clean.

- [ ] **Step 13: Commit**

```bash
git add src/shared/ipc/tauri.ts src/domains/resume src/app/App.tsx src/app/App.onboarding.test.tsx src/dev/previewIpc.ts src/dev/previewIpc.test.ts
git commit -m "feat(resume): show Resume Work threads on Home with sources and a copyable pack"
```

### Task 4: Agent access on a stable port with setup in Settings

**Files:**
- Modify: `src-tauri/src/mcp/mod.rs` (constant and two functions near `LOOPBACK_HOST` at line 493; `start()` at lines 616 and 632 to 650; tests in the module at line 5091)
- Create: `src/domains/workspace/agentConnect.ts`, `src/domains/workspace/agentConnect.test.ts`
- Create: `src/domains/workspace/AgentAccessSection.tsx`, `src/domains/workspace/AgentAccessSection.test.tsx`
- Modify: `src/domains/workspace/ControlPanel.tsx` (after the Local models section, line 521), `src/domains/workspace/ControlPanel.css`
- Modify: `src/dev/previewIpc.ts`, `src/dev/previewIpc.test.ts`, `docs/mcp.md`

**Interfaces:**
- Consumes: `getMcpServerStatus(): Promise<McpServerStatus>` and `startMcpServer(port?: number): Promise<McpServerStatus>` (`src/shared/ipc/tauri.ts:1295-1300`); `McpServerStatus { running, endpoint, token, require_auth, ... }`.
- Produces: `DEFAULT_MCP_PORT: u16 = 47_821`; `resolve_mcp_port(requested: Option<u16>, env_value: Option<&str>) -> u16`; `probe_bind_addr(host: &str, port: u16) -> Result<SocketAddr, String>`; env var `FNDR_MCP_PORT`; `buildAgentConnectSnippets(endpoint: string): { claudeCode: string; jsonConfig: string }`; component `AgentAccessSection()`.

- [ ] **Step 1: Write the failing Rust tests inside `mod tests` in `src-tauri/src/mcp/mod.rs`**

```rust
    #[test]
    fn mcp_port_prefers_request_then_env_then_stable_default() {
        assert_eq!(resolve_mcp_port(Some(9000), Some("9100")), 9000);
        assert_eq!(resolve_mcp_port(None, Some("9100")), 9100);
        assert_eq!(resolve_mcp_port(None, Some(" 0 ")), 0);
        assert_eq!(resolve_mcp_port(None, Some("not-a-port")), DEFAULT_MCP_PORT);
        assert_eq!(resolve_mcp_port(None, None), DEFAULT_MCP_PORT);
    }

    #[test]
    fn mcp_probe_falls_back_to_a_free_port_when_the_preferred_one_is_busy() {
        let busy = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind");
        let busy_port = busy.local_addr().expect("addr").port();
        let chosen = probe_bind_addr("127.0.0.1", busy_port).expect("fallback");
        assert_ne!(chosen.port(), busy_port);
        assert_ne!(chosen.port(), 0);
        drop(busy);
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test --lib mcp::tests::mcp_p`
Expected: compile FAIL, cannot find `resolve_mcp_port`, `DEFAULT_MCP_PORT`, `probe_bind_addr`.

- [ ] **Step 3: Implement**

Below `const LOOPBACK_HOST: &str = "127.0.0.1";` add:

```rust
/// Stable loopback port so an agent's saved configuration keeps working across
/// launches. `FNDR_MCP_PORT` overrides it; `0` means any free port.
pub const DEFAULT_MCP_PORT: u16 = 47_821;

/// An explicit request wins, then `FNDR_MCP_PORT`, then the stable default.
fn resolve_mcp_port(requested: Option<u16>, env_value: Option<&str>) -> u16 {
    if let Some(port) = requested {
        return port;
    }
    env_value
        .and_then(|value| value.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_MCP_PORT)
}

/// The address to serve on: the preferred port when it is free, otherwise any
/// free port, so a busy port never blocks agent access.
fn probe_bind_addr(host: &str, port: u16) -> Result<SocketAddr, String> {
    let probe = |port: u16| -> std::io::Result<SocketAddr> {
        std::net::TcpListener::bind((host, port))?.local_addr()
    };
    match probe(port) {
        Ok(addr) => Ok(addr),
        Err(err) if port != 0 => {
            tracing::warn!(port, "Preferred MCP port unavailable ({err}); using a free port");
            probe(0).map_err(|e| format!("Failed to probe for free port: {e}"))
        }
        Err(err) => Err(format!("Failed to probe for free port: {err}")),
    }
}
```

In `start()`, replace `let port = port.unwrap_or(0);` with:

```rust
    let port = resolve_mcp_port(port, std::env::var("FNDR_MCP_PORT").ok().as_deref());
```

and replace the whole block from `let addr: SocketAddr = format!("{host}:{port}")` through the end of the `let actual_addr = if port == 0 { ... } else { addr };` statement with:

```rust
    let actual_addr = probe_bind_addr(&host, port)?;
```

- [ ] **Step 4: Run to verify they pass, plus the existing MCP tests**

Run: `cd src-tauri && cargo test --lib mcp::`
Expected: PASS, including the two new tests and the existing `mcp_rejects_*` tests.

- [ ] **Step 5: Write the failing snippet tests in `src/domains/workspace/agentConnect.test.ts`**

```ts
import { describe, expect, it } from "vitest";
import { buildAgentConnectSnippets } from "./agentConnect";

describe("buildAgentConnectSnippets", () => {
    const endpoint = "http://127.0.0.1:47821/mcp";

    it("builds a Claude Code command that reads the token from disk instead of embedding it", () => {
        expect(buildAgentConnectSnippets(endpoint).claudeCode).toBe(
            'claude mcp add --transport http fndr http://127.0.0.1:47821/mcp --header "Authorization: Bearer $(cat ~/.fndr/mcp_token)"',
        );
    });

    it("builds a JSON config with the endpoint and a token placeholder", () => {
        const parsed = JSON.parse(buildAgentConnectSnippets(endpoint).jsonConfig);
        expect(parsed.mcpServers.fndr.url).toBe(endpoint);
        expect(parsed.mcpServers.fndr.type).toBe("http");
        expect(parsed.mcpServers.fndr.headers.Authorization).toMatch(/^Bearer </);
    });
});
```

- [ ] **Step 6: Write the failing component tests in `src/domains/workspace/AgentAccessSection.test.tsx`**

```tsx
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { AgentAccessSection } from "./AgentAccessSection";

vi.mock("@/shared/ipc/tauri", () => ({
    getMcpServerStatus: vi.fn(),
    startMcpServer: vi.fn(),
}));

import { getMcpServerStatus, startMcpServer } from "@/shared/ipc/tauri";

const off = {
    running: false,
    mode: "local",
    host: "127.0.0.1",
    port: 0,
    endpoint: "",
    token: "secret-token",
    use_tls: false,
    require_auth: true,
    auth_mode: "bearer",
    last_error: null,
};
const on = { ...off, running: true, port: 47821, endpoint: "http://127.0.0.1:47821/mcp" };

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("AgentAccessSection", () => {
    it("offers to turn agent access on, then shows the endpoint and that a token is required", async () => {
        vi.mocked(getMcpServerStatus).mockResolvedValue(off);
        vi.mocked(startMcpServer).mockResolvedValue(on);
        render(<AgentAccessSection />);
        fireEvent.click(await screen.findByRole("button", { name: "Turn on agent access" }));
        expect(await screen.findByText("http://127.0.0.1:47821/mcp")).toBeInTheDocument();
        expect(screen.getByText(/token required/)).toBeInTheDocument();
    });

    it("never renders the token itself", async () => {
        vi.mocked(getMcpServerStatus).mockResolvedValue(on);
        render(<AgentAccessSection />);
        await screen.findByRole("button", { name: "Copy token" });
        expect(document.body.textContent).not.toContain("secret-token");
    });

    it("copies the Claude Code command", async () => {
        const writeText = vi.fn().mockResolvedValue(undefined);
        Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
        vi.mocked(getMcpServerStatus).mockResolvedValue(on);
        render(<AgentAccessSection />);
        fireEvent.click(await screen.findByRole("button", { name: "Copy Claude Code command" }));
        expect(await screen.findByRole("button", { name: "Copied" })).toBeInTheDocument();
        expect(writeText).toHaveBeenCalledWith(
            expect.stringContaining("claude mcp add --transport http fndr http://127.0.0.1:47821/mcp"),
        );
    });

    it("shows a start failure", async () => {
        vi.mocked(getMcpServerStatus).mockResolvedValue(off);
        vi.mocked(startMcpServer).mockRejectedValue("Failed to probe for free port");
        render(<AgentAccessSection />);
        fireEvent.click(await screen.findByRole("button", { name: "Turn on agent access" }));
        expect(await screen.findByRole("alert")).toHaveTextContent("Failed to probe for free port");
    });
});
```

- [ ] **Step 7: Run to verify both files fail**

Run: `npx vitest run src/domains/workspace/agentConnect.test.ts src/domains/workspace/AgentAccessSection.test.tsx`
Expected: FAIL, cannot resolve `./agentConnect` and `./AgentAccessSection`.

- [ ] **Step 8: Implement `src/domains/workspace/agentConnect.ts`**

```ts
/** Copy-paste setup for connecting an AI assistant to FNDR's local MCP server.
 *  The shell command reads the token from disk so it never appears on screen. */
export interface AgentConnectSnippets {
    claudeCode: string;
    jsonConfig: string;
}

export const MCP_TOKEN_PATH = "~/.fndr/mcp_token";

export function buildAgentConnectSnippets(endpoint: string): AgentConnectSnippets {
    const claudeCode =
        `claude mcp add --transport http fndr ${endpoint} ` +
        `--header "Authorization: Bearer $(cat ${MCP_TOKEN_PATH})"`;
    const jsonConfig = JSON.stringify(
        {
            mcpServers: {
                fndr: {
                    type: "http",
                    url: endpoint,
                    headers: { Authorization: "Bearer <paste the token from Copy token>" },
                },
            },
        },
        null,
        2,
    );
    return { claudeCode, jsonConfig };
}
```

- [ ] **Step 9: Implement `src/domains/workspace/AgentAccessSection.tsx`**

```tsx
import { useEffect, useState } from "react";
import { Button } from "@/shared/components/atoms";
import { getMcpServerStatus, startMcpServer, type McpServerStatus } from "@/shared/ipc/tauri";
import { buildAgentConnectSnippets } from "./agentConnect";

type CopyTarget = "claudeCode" | "jsonConfig" | "token";

/** Settings card that turns on FNDR's local MCP server and hands out setup snippets. */
export function AgentAccessSection() {
    const [status, setStatus] = useState<McpServerStatus | null>(null);
    const [error, setError] = useState<string | null>(null);
    const [busy, setBusy] = useState(false);
    const [copied, setCopied] = useState<CopyTarget | null>(null);

    useEffect(() => {
        getMcpServerStatus()
            .then(setStatus)
            .catch((err) => setError(String(err)));
    }, []);

    async function turnOn() {
        setBusy(true);
        setError(null);
        try {
            setStatus(await startMcpServer());
        } catch (err) {
            setError(String(err));
        } finally {
            setBusy(false);
        }
    }

    async function copy(target: CopyTarget, text: string) {
        try {
            await navigator.clipboard.writeText(text);
            setCopied(target);
        } catch {
            setError("Couldn't copy to the clipboard.");
        }
    }

    const snippets = status?.running ? buildAgentConnectSnippets(status.endpoint) : null;

    return (
        <section className="panel-section" aria-labelledby="settings-agent-title">
            <h3 id="settings-agent-title">Agent access</h3>
            <p className="section-hint">
                Let AI assistants such as Claude Code read your FNDR memory on this Mac. Every request needs your
                private token.
            </p>
            {error && (
                <p className="model-error" role="alert">
                    {error}
                </p>
            )}
            {status && !status.running && (
                <Button variant="secondary" onClick={() => void turnOn()} disabled={busy}>
                    {busy ? "Turning on…" : "Turn on agent access"}
                </Button>
            )}
            {status?.running && snippets && (
                <>
                    <p>
                        Running at <code>{status.endpoint}</code>
                        {status.require_auth ? ", token required" : ", no token required"}
                    </p>
                    <div className="agent-access-actions">
                        <Button variant="secondary" onClick={() => void copy("claudeCode", snippets.claudeCode)}>
                            {copied === "claudeCode" ? "Copied" : "Copy Claude Code command"}
                        </Button>
                        <Button variant="ghost" onClick={() => void copy("jsonConfig", snippets.jsonConfig)}>
                            {copied === "jsonConfig" ? "Copied" : "Copy JSON config"}
                        </Button>
                        <Button variant="ghost" onClick={() => void copy("token", status.token)}>
                            {copied === "token" ? "Copied" : "Copy token"}
                        </Button>
                    </div>
                    <p className="section-hint">Agent access turns off when FNDR quits. Turn it on again after relaunch.</p>
                </>
            )}
        </section>
    );
}
```

- [ ] **Step 10: Run to verify they pass**

Run: `npx vitest run src/domains/workspace/agentConnect.test.ts src/domains/workspace/AgentAccessSection.test.tsx`
Expected: PASS, 6 tests.

- [ ] **Step 11: Mount in Settings, style, and model it in the preview**

In `src/domains/workspace/ControlPanel.tsx`, add `import { AgentAccessSection } from "./AgentAccessSection";` with the other local imports, and add `<AgentAccessSection />` directly after the closing `</section>` of the Local models section (the one with `aria-labelledby="settings-models-title"`).

Append to `src/domains/workspace/ControlPanel.css`:

```css
.agent-access-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 8px 0;
}
```

In `src/dev/previewIpc.ts`, add `type McpServerStatus` to the type import, add inside `createPreviewIpcHandler` next to `let previewBlocklist`:

```ts
    let previewMcpStatus: McpServerStatus = {
        running: false,
        mode: "local",
        host: "127.0.0.1",
        port: 0,
        endpoint: "",
        token: "preview-token-not-real",
        use_tls: false,
        require_auth: true,
        auth_mode: "bearer",
        last_error: null,
    };
```

and these cases beside `case "resume_work":`:

```ts
            case "get_mcp_server_status":
                return { ...previewMcpStatus };
            case "start_mcp_server":
                previewMcpStatus = {
                    ...previewMcpStatus,
                    running: true,
                    port: 47821,
                    endpoint: "http://127.0.0.1:47821/mcp",
                };
                return { ...previewMcpStatus };
```

In `src/dev/previewIpc.test.ts`, add:

```ts
    it("models turning on agent access without starting a server", async () => {
        const invoke = createPreviewIpcHandler();
        await expect(invoke("get_mcp_server_status")).resolves.toMatchObject({ running: false });
        await expect(invoke("start_mcp_server", {})).resolves.toMatchObject({
            running: true,
            endpoint: "http://127.0.0.1:47821/mcp",
        });
        await expect(invoke("get_mcp_server_status")).resolves.toMatchObject({ running: true });
    });
```

- [ ] **Step 12: Document it in `docs/mcp.md`**

Add this section after "## Modes":

````markdown
## Connect an assistant (Settings, Agent access)

Open Settings, then **Agent access**, then **Turn on agent access**. FNDR serves MCP on `http://127.0.0.1:47821/mcp` (set `FNDR_MCP_PORT` to change it; if the port is busy FNDR picks a free one and shows it). Click **Copy Claude Code command** and run it once:

```bash
claude mcp add --transport http fndr http://127.0.0.1:47821/mcp --header "Authorization: Bearer $(cat ~/.fndr/mcp_token)"
```

Agent access turns off when FNDR quits; turn it on again after relaunch. The token never appears on screen; **Copy token** puts it on the clipboard for clients that need a JSON config.
````

- [ ] **Step 13: Run the focused suites**

Run: `npx vitest run src/domains/workspace src/dev && npm run typecheck`
Expected: PASS. If an existing `ControlPanel.test.tsx` assertion now finds two matching elements, scope that assertion with `within(...)` to its own section rather than changing the new card.

- [ ] **Step 14: Commit**

```bash
git add src-tauri/src/mcp/mod.rs src/domains/workspace docs/mcp.md src/dev/previewIpc.ts src/dev/previewIpc.test.ts
git commit -m "feat(mcp): stable local port and an Agent access card with Claude Code setup"
```

### Task 5: Vault health report

**Files:**
- Create: `scripts/audit/vault_health.py`, `scripts/audit/test_vault_health.py`
- Modify: `Makefile`
- Create: `docs/evidence/W01/vault-health-owner.md` (aggregate output only)

**Interfaces:**
- Consumes: any FNDR LanceDB directory; the `memories_v4_minilm_384` columns `embedding`, `timestamp`, `clean_text`, `project`, `topic`, `outcome`, `next_steps`, `decisions`, `errors`, `summary_source`, `reopen_kind`; table names ending in `.lance`.
- Produces: `summarize(db_path: str) -> dict` with keys `tables`, `memories`, `first_day`, `last_day`, `active_days`, `memories_per_active_day`, `zero_or_missing_vectors_pct`, `clean_text_chars_p50`, `structured_pct`, `summary_source`, `reopen_specific_pct`, `chunk_rows`; `render(report: dict) -> str` (Markdown, never memory text); `make vault-health [DB=<dir>]`. Lane 3 reruns this every Friday.

Requires `python3 -m pip install lancedb numpy` (lancedb 0.29 reads the tables the Rust crate writes; checked against the owner's profile on 2026-09-23).

- [ ] **Step 1: Write the failing test `scripts/audit/test_vault_health.py`**

```python
import os
import sys
import tempfile
import unittest

import lancedb
import pyarrow as pa

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from vault_health import MEMORY_TABLE, render, summarize  # noqa: E402


class VaultHealthTest(unittest.TestCase):
    def test_counts_structure_reopen_and_empty_chunk_tables(self):
        with tempfile.TemporaryDirectory() as d:
            db = lancedb.connect(d)
            day_ms = 86_400_000
            start = 1_758_600_000_000
            db.create_table(
                MEMORY_TABLE,
                pa.table(
                    {
                        "id": ["a", "b"],
                        "timestamp": [start, start + day_ms],
                        "embedding": pa.array([[0.1, 0.2], [0.0, 0.0]], type=pa.list_(pa.float32(), 2)),
                        "clean_text": ["secret-text-" * 10, "y" * 300],
                        "project": ["Essay", ""],
                        "topic": ["unknown", "Drafting"],
                        "outcome": ["", ""],
                        "next_steps": [["Finish"], []],
                        "decisions": pa.array([[], []], type=pa.list_(pa.string())),
                        "errors": pa.array([[], []], type=pa.list_(pa.string())),
                        "summary_source": ["llm", "fallback"],
                        "reopen_kind": ["browser_url", "app_bundle"],
                    }
                ),
            )
            db.create_table("memory_chunks_v1_bge_1024", pa.table({"id": pa.array([], type=pa.string())}))
            report = summarize(d)

        self.assertEqual(report["memories"], 2)
        self.assertEqual(report["active_days"], 2)
        self.assertEqual(report["zero_or_missing_vectors_pct"], 50.0)
        self.assertEqual(report["structured_pct"]["project"], 50.0)
        self.assertEqual(report["structured_pct"]["topic"], 50.0)
        self.assertEqual(report["structured_pct"]["next_steps"], 50.0)
        self.assertEqual(report["structured_pct"]["decisions"], 0.0)
        self.assertEqual(report["reopen_specific_pct"], 50.0)
        self.assertEqual(report["chunk_rows"]["memory_chunks_v1_bge_1024"], 0)
        text = render(report)
        self.assertIn("| project | 50.0% |", text)
        self.assertNotIn("secret-text", text)

    def test_missing_memory_table_is_reported_not_raised(self):
        with tempfile.TemporaryDirectory() as d:
            lancedb.connect(d).create_table("tasks", pa.table({"id": ["t"]}))
            report = summarize(d)
        self.assertNotIn("memories", report)
        self.assertIn("No `memories_v4_minilm_384` table found.", render(report))


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 2: Run to verify it fails**

Run: `python3 scripts/audit/test_vault_health.py`
Expected: FAIL with `ModuleNotFoundError: No module named 'vault_health'`.

- [ ] **Step 3: Implement `scripts/audit/vault_health.py`**

```python
#!/usr/bin/env python3
"""Aggregate-only health report for an FNDR LanceDB store.

Prints counts and percentages, never memory text, so the output is safe to
paste into an evidence file. Requires: pip install lancedb numpy

Usage: python3 scripts/audit/vault_health.py [--db <lancedb dir>] [--out <md>]
"""
from __future__ import annotations

import argparse
import collections
import datetime
import os

import numpy as np

DEFAULT_DB = os.path.expanduser("~/Library/Application Support/com.fndr.app/lancedb")
MEMORY_TABLE = "memories_v4_minilm_384"
CHUNK_TABLES = ("memories_v5_bge_1024", "memory_chunks_v1_bge_1024")
STRUCTURED = ("project", "topic", "outcome", "next_steps", "decisions", "errors")
SPECIFIC_REOPEN = ("browser_url", "file_path", "app_deep_link")


def _filled(values) -> int:
    return sum(1 for v in values if v not in (None, "", "unknown") and v != [])


def _pct(part: int, whole: int) -> float:
    return round(100.0 * part / whole, 1) if whole else 0.0


def summarize(db_path: str) -> dict:
    import lancedb

    db = lancedb.connect(db_path)
    names = sorted(d[: -len(".lance")] for d in os.listdir(db_path) if d.endswith(".lance"))
    tables = {name: db.open_table(name).count_rows() for name in names}
    report: dict = {"tables": tables}
    if MEMORY_TABLE not in tables:
        return report

    t = db.open_table(MEMORY_TABLE).to_arrow()
    n = t.num_rows
    cols = set(t.column_names)

    def col(name: str) -> list:
        return t.column(name).to_pylist() if name in cols else [None] * n

    zero = sum(1 for v in col("embedding") if v is None or not np.any(np.asarray(v, dtype=np.float32)))
    days = collections.Counter(
        datetime.datetime.fromtimestamp(ms / 1000).strftime("%Y-%m-%d") for ms in col("timestamp") if ms
    )
    text_lens = sorted(len(v or "") for v in col("clean_text"))
    reopen = collections.Counter(str(v) for v in col("reopen_kind"))
    specific = sum(reopen.get(kind, 0) for kind in SPECIFIC_REOPEN)
    report.update(
        {
            "memories": n,
            "first_day": min(days) if days else None,
            "last_day": max(days) if days else None,
            "active_days": len(days),
            "memories_per_active_day": round(n / len(days), 1) if days else 0.0,
            "zero_or_missing_vectors_pct": _pct(zero, n),
            "clean_text_chars_p50": int(np.percentile(text_lens, 50)) if text_lens else 0,
            "structured_pct": {field: _pct(_filled(col(field)), n) for field in STRUCTURED},
            "summary_source": dict(collections.Counter(str(v) for v in col("summary_source"))),
            "reopen_specific_pct": _pct(specific, n),
            "chunk_rows": {name: tables.get(name, 0) for name in CHUNK_TABLES},
        }
    )
    return report


def render(report: dict) -> str:
    lines = ["# FNDR vault health", "", "Aggregate counts only; no memory text.", ""]
    lines += ["## Tables", "", "| table | rows |", "|---|---|"]
    lines += [f"| {name} | {rows} |" for name, rows in sorted(report["tables"].items())]
    if "memories" not in report:
        lines += ["", f"No `{MEMORY_TABLE}` table found."]
        return "\n".join(lines) + "\n"
    chunks = ", ".join(f"{name} = {rows}" for name, rows in report["chunk_rows"].items())
    lines += [
        "",
        "## Memories",
        "",
        f"- Memories: {report['memories']} over {report['active_days']} active days "
        f"({report['first_day']} to {report['last_day']}), {report['memories_per_active_day']} per active day",
        f"- Zero or missing primary vectors: {report['zero_or_missing_vectors_pct']}%",
        f"- Stored clean text, median characters: {report['clean_text_chars_p50']}",
        f"- Reopens to a specific page or file, not just the app: {report['reopen_specific_pct']}%",
        f"- Chunk-level rows: {chunks}",
        "",
        "## Structured fields filled",
        "",
        "| field | filled |",
        "|---|---|",
    ]
    lines += [f"| {field} | {pct}% |" for field, pct in report["structured_pct"].items()]
    lines += ["", "## Summary source", "", "| source | memories |", "|---|---|"]
    lines += [f"| {src} | {count} |" for src, count in sorted(report["summary_source"].items(), key=lambda kv: -kv[1])]
    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description="Aggregate-only FNDR vault health report")
    parser.add_argument("--db", default=DEFAULT_DB)
    parser.add_argument("--out")
    args = parser.parse_args()
    text = render(summarize(os.path.expanduser(args.db)))
    if args.out:
        with open(args.out, "w", encoding="utf-8") as fh:
            fh.write(text)
    print(text)


if __name__ == "__main__":
    main()
```

- [ ] **Step 4: Run to verify it passes**

Run: `python3 scripts/audit/test_vault_health.py`
Expected: `Ran 2 tests` and `OK`. (This exact script and test passed in a scratch copy on 2026-09-23.)

- [ ] **Step 5: Add the Make target and record the owner baseline**

Add `vault-health` to `.PHONY` and append to `Makefile`:

```make
vault-health:
	python3 scripts/audit/vault_health.py $(if $(DB),--db "$(DB)") $(if $(OUT),--out "$(OUT)")
```

Run: `make vault-health OUT=docs/evidence/W01/vault-health-owner.md`
Expected on the owner's profile as of 2026-09-23: `Memories: 29 over 5 active days`, `Zero or missing primary vectors: 0.0%`, `Stored clean text, median characters: 129`, `Reopens to a specific page or file, not just the app: 10.3%`, both chunk tables `= 0`, `project | 0.0%`, `next_steps | 0.0%`. Newer numbers are fine; the point is a dated baseline. Open the file and confirm it contains no memory text before committing.

- [ ] **Step 6: Commit**

```bash
git add scripts/audit/vault_health.py scripts/audit/test_vault_health.py Makefile docs/evidence/W01/vault-health-owner.md
git commit -m "feat(audit): aggregate-only vault health report and owner baseline"
```

### Task 6: Retrieval baseline through both search paths

**Files:**
- Modify: `src-tauri/src/ipc/commands/search.rs` (extract a public helper from `search_memory_cards`, lines 511 to 541)
- Create: `src-tauri/examples/retrieval_qa.rs`
- Create: `scripts/demo/knowledge-worker-queries.json`
- Modify: `Makefile`
- Create: `docs/evidence/W01/retrieval-baseline-seeded.md` (generated)

**Interfaces:**
- Consumes: `run_search_query(state: &AppState, query: &str, time_filter: Option<&str>, app_filter: Option<&str>, limit: usize) -> Result<Vec<SearchResult>, String>` (`search.rs:21`), `partition_surfaceable`, `QueryContext::from_query`, `rerank_results`; `fndr_lib::context_runtime::{run_query, ComposeMode}` returning `ComposedAnswer { cards: Vec<MemoryCard>, .. }` where `MemoryCard { id, evidence_ids, .. }`; `AppState::new(app_data_dir, Config, Arc<Store>, Arc<StateStore>, GraphStore, None, None)`; the seeded QA profile from Task 2.
- Produces: `pub async fn search_ranked_results(state: &AppState, query: &str, time_filter: Option<&str>, app_filter: Option<&str>, raw_limit: usize) -> Result<Vec<SearchResult>, String>` (used by the Search screen and the eval); `cargo run --example retrieval_qa -- --data-dir <profile> --cases <json> [--out <md>]`; `make qa-retrieval`. The month plan's retrieval work is judged by this report.

- [ ] **Step 1: Write the failing tests at the bottom of a new `src-tauri/examples/retrieval_qa.rs`**

Create the file with only the header, imports, and this test module for now:

```rust
//! Retrieval baseline on a seeded profile, through the two paths the app uses:
//! the Search screen (`search_ranked_results`) and Ask plus every MCP tool
//! (`context_runtime::run_query`). Read-only; refuses the real profile.
//! Usage: cargo run --example retrieval_qa -- --data-dir <profile> --cases <json> [--out <md>]

use fndr_lib::config::Config;
use fndr_lib::context_runtime::{run_query, ComposeMode};
use fndr_lib::graph::GraphStore;
use fndr_lib::ipc::commands::search::search_ranked_results;
use fndr_lib::storage::{StateStore, Store};
use fndr_lib::AppState;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn rank_counts_a_card_as_relevant_when_any_cited_id_is_relevant() {
        let relevant: HashSet<String> = ["b".to_string()].into();
        let ranked = vec![ids(&["x"]), ids(&["y", "b"]), ids(&["b"])];
        assert_eq!(first_relevant_rank(&ranked, &relevant, 10), Some(2));
        assert_eq!(first_relevant_rank(&ranked, &relevant, 1), None);
    }

    #[test]
    fn score_reports_recall_at_5_and_mrr() {
        let mut score = Score::default();
        score.add(Some(1));
        score.add(Some(4));
        score.add(Some(7));
        score.add(None);
        assert!((score.recall_at_5() - 0.5).abs() < 1e-6);
        assert!((score.mrr_at_10() - (1.0 + 0.25 + 1.0 / 7.0) / 4.0).abs() < 1e-6);
    }

    #[test]
    fn percentile_uses_nearest_rank_and_handles_empty() {
        assert_eq!(percentile(&[], 95.0), 0);
        assert_eq!(percentile(&[10, 20, 30, 40, 50], 50.0), 30);
        assert_eq!(percentile(&[10, 20, 30, 40, 50], 95.0), 50);
    }

    #[test]
    fn every_expected_id_exists_in_the_knowledge_worker_corpus() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/demo/");
        let cases = load_cases(format!("{root}knowledge-worker-queries.json")).expect("cases");
        let corpus: Vec<serde_json::Value> = serde_json::from_slice(
            &std::fs::read(format!("{root}knowledge-worker-week.json")).expect("corpus"),
        )
        .expect("json");
        let known: HashSet<&str> = corpus.iter().filter_map(|e| e["id"].as_str()).collect();
        for case in &cases {
            assert!(!case.relevant_ids.is_empty(), "{} has no expected ids", case.query);
            for id in &case.relevant_ids {
                assert!(known.contains(id.as_str()), "{} expects unknown id {id}", case.query);
            }
        }
        assert!(cases.iter().filter(|c| c.kind == "paraphrase").count() >= 8);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd src-tauri && cargo test --example retrieval_qa`
Expected: compile FAIL: unresolved import `search_ranked_results`, cannot find `first_relevant_rank`, `Score`, `percentile`, `load_cases`.

- [ ] **Step 3: Extract `search_ranked_results` in `src-tauri/src/ipc/commands/search.rs`**

Add this function directly below `run_search_query`:

```rust
/// The ranked list the Search screen shows before card synthesis: hybrid
/// retrieval, low-signal removal, then the anchor-coverage rerank. Public so
/// `examples/retrieval_qa.rs` measures exactly what users see.
pub async fn search_ranked_results(
    state: &AppState,
    query: &str,
    time_filter: Option<&str>,
    app_filter: Option<&str>,
    raw_limit: usize,
) -> Result<Vec<SearchResult>, String> {
    let mut raw_results = run_search_query(state, query, time_filter, app_filter, raw_limit).await?;
    raw_results.truncate(raw_limit);
    let (raw_results, low_signal) = partition_surfaceable(raw_results);
    if !low_signal.is_empty() {
        tracing::info!(hidden = low_signal.len(), "search_memory_cards:low_signal_hidden");
    }
    let query_context = QueryContext::from_query(query);
    let (mut reranked, rerank_stats) = rerank_results(&query_context, raw_results);
    if rerank_stats.excluded_for_coverage > 0 {
        tracing::info!(
            excluded_for_coverage = rerank_stats.excluded_for_coverage,
            query = %query_context.raw_query,
            "search_memory_cards:coverage_gate"
        );
    }
    reranked.truncate(raw_limit);
    Ok(reranked)
}
```

In `search_memory_cards`, replace everything from `let mut raw_results = run_search_query(` through the line `tracing::info!(count = raw_results.len(), "search_memory_cards:rerank:done");` with:

```rust
    let raw_results = search_ranked_results(
        state.inner(),
        &query,
        time_filter.as_deref(),
        app_filter.as_deref(),
        raw_limit,
    )
    .await?;
    tracing::info!(count = raw_results.len(), "search_memory_cards:rerank:done");
```

Keep the `let raw_limit = limit.max(18).min(50);` line above it. If the compiler reports that `raw_results` must be mutable further down, declare it `let mut raw_results`. This is a behavior-preserving extraction; the existing search tests are its regression check.

- [ ] **Step 4: Create `scripts/demo/knowledge-worker-queries.json`**

```json
[
  { "query": "pandas import error fix", "kind": "keyword", "relevant_ids": ["kw-cs-a4-fix-old"] },
  { "query": "how did I get python to find the data library last time", "kind": "paraphrase", "relevant_ids": ["kw-cs-a4-fix-old"] },
  { "query": "what does Dana want for the survey readout", "kind": "keyword", "relevant_ids": ["kw-intern-brief", "kw-intern-email"] },
  { "query": "churn drivers due Thursday", "kind": "keyword", "relevant_ids": ["kw-intern-brief"] },
  { "query": "what customers complained about during onboarding", "kind": "paraphrase", "relevant_ids": ["kw-intern-themes"] },
  { "query": "single sign-on", "kind": "keyword", "relevant_ids": ["kw-intern-themes"] },
  { "query": "essay rubric weights", "kind": "keyword", "relevant_ids": ["kw-hist-rubric"] },
  { "query": "how much of my history grade depends on sources", "kind": "paraphrase", "relevant_ids": ["kw-hist-rubric"] },
  { "query": "Field Order 15", "kind": "keyword", "relevant_ids": ["kw-hist-outline"] },
  { "query": "labor contracts passage page 112", "kind": "keyword", "relevant_ids": ["kw-hist-reader", "kw-hist-draft"] },
  { "query": "counterargument paragraph", "kind": "keyword", "relevant_ids": ["kw-hist-draft"] },
  { "query": "weekday vs weekend ridership chart", "kind": "keyword", "relevant_ids": ["kw-cs-a4-vscode-new", "kw-cs-a4-charts"] },
  { "query": "which plotting library am I allowed to use", "kind": "paraphrase", "relevant_ids": ["kw-cs-a4-charts", "kw-cs-a4-spec"] },
  { "query": "who is recording the backup video", "kind": "keyword", "relevant_ids": ["kw-cap-standup", "kw-cap-slack"] },
  { "query": "architecture slide", "kind": "keyword", "relevant_ids": ["kw-cap-slides"] },
  { "query": "how long is the capstone demo script", "kind": "paraphrase", "relevant_ids": ["kw-cap-script"] },
  { "query": "spring enrollment", "kind": "keyword", "relevant_ids": ["kw-inbox-triage"] },
  { "query": "duplicate survey responses removed", "kind": "keyword", "relevant_ids": ["kw-intern-clean"] },
  { "query": "food bank inventory app presentation", "kind": "paraphrase", "relevant_ids": ["kw-cap-slides", "kw-cap-script", "kw-cap-standup"] },
  { "query": "transit dataset assignment requirements", "kind": "keyword", "relevant_ids": ["kw-cs-a4-spec"] },
  { "query": "my thesis about why Reconstruction failed", "kind": "paraphrase", "relevant_ids": ["kw-hist-outline", "kw-hist-draft"] },
  { "query": "the problem our capstone app solves", "kind": "paraphrase", "relevant_ids": ["kw-cap-script"] }
]
```

- [ ] **Step 5: Implement the example above the test module**

Insert between the imports and `#[cfg(test)]`:

```rust
/// Same default the Search screen uses for its raw result list.
const SEARCH_LIMIT: usize = 20;
const ASK_LIMIT: usize = 10;

#[derive(Deserialize)]
struct Case {
    query: String,
    relevant_ids: Vec<String>,
    #[serde(default)]
    kind: String,
}

#[derive(Default)]
struct Score {
    n: usize,
    hits_at_5: usize,
    reciprocal_rank_sum: f32,
}

impl Score {
    fn add(&mut self, rank: Option<usize>) {
        self.n += 1;
        if let Some(rank) = rank {
            if rank <= 5 {
                self.hits_at_5 += 1;
            }
            self.reciprocal_rank_sum += 1.0 / rank as f32;
        }
    }

    fn recall_at_5(&self) -> f32 {
        self.hits_at_5 as f32 / self.n.max(1) as f32
    }

    fn mrr_at_10(&self) -> f32 {
        self.reciprocal_rank_sum / self.n.max(1) as f32
    }
}

/// 1-based rank of the first result, within `k`, that cites a relevant memory id.
fn first_relevant_rank(ranked: &[Vec<String>], relevant: &HashSet<String>, k: usize) -> Option<usize> {
    ranked
        .iter()
        .take(k)
        .position(|ids| ids.iter().any(|id| relevant.contains(id)))
        .map(|index| index + 1)
}

fn percentile(values: &[u128], p: f64) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let index = ((p / 100.0) * (sorted.len() - 1) as f64).round() as usize;
    sorted[index]
}

fn rank_label(rank: Option<usize>) -> String {
    rank.map_or_else(|| "miss".to_string(), |r| r.to_string())
}

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn load_cases(path: impl AsRef<Path>) -> Result<Vec<Case>, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(arg("--data-dir").ok_or("--data-dir required")?);
    let cases_path = PathBuf::from(arg("--cases").ok_or("--cases required")?);
    if let Some(real) = dirs::data_dir().map(|d| d.join("com.fndr.app")) {
        let real = real.canonicalize().unwrap_or(real);
        if data_dir.canonicalize()? == real {
            return Err("refusing to evaluate against the real FNDR profile".into());
        }
    }
    let cases = load_cases(&cases_path)?;

    let store = Arc::new(Store::new(&data_dir)?);
    let state_store = Arc::new(StateStore::new(&data_dir)?);
    let graph = GraphStore::new(store.clone());
    let state = AppState::new(data_dir.clone(), Config::default(), store, state_store, graph, None, None);
    let rt = tokio::runtime::Runtime::new()?;

    let (mut search_score, mut ask_score) = (Score::default(), Score::default());
    let (mut search_ms, mut ask_ms) = (Vec::new(), Vec::new());
    let mut agree_top1 = 0usize;
    let mut rows = Vec::new();
    for case in &cases {
        let relevant: HashSet<String> = case.relevant_ids.iter().cloned().collect();

        let started = Instant::now();
        let search_ranked: Vec<Vec<String>> = rt
            .block_on(search_ranked_results(&state, &case.query, None, None, SEARCH_LIMIT))?
            .into_iter()
            .map(|result| vec![result.id])
            .collect();
        search_ms.push(started.elapsed().as_millis());

        let started = Instant::now();
        let answer = rt.block_on(run_query(&state, &case.query, ASK_LIMIT, ComposeMode::Cards))?;
        ask_ms.push(started.elapsed().as_millis());
        let ask_ranked: Vec<Vec<String>> = answer
            .cards
            .into_iter()
            .map(|card| std::iter::once(card.id).chain(card.evidence_ids).collect())
            .collect();

        let search_rank = first_relevant_rank(&search_ranked, &relevant, 10);
        let ask_rank = first_relevant_rank(&ask_ranked, &relevant, 10);
        search_score.add(search_rank);
        ask_score.add(ask_rank);
        if let (Some(s), Some(a)) = (search_ranked.first(), ask_ranked.first()) {
            if s.iter().any(|id| a.contains(id)) {
                agree_top1 += 1;
            }
        }
        let kind = if case.kind.is_empty() { "unlabeled" } else { case.kind.as_str() };
        rows.push(format!(
            "| {} | {} | {} | {} |",
            case.query,
            kind,
            rank_label(search_rank),
            rank_label(ask_rank)
        ));
    }

    let mut report = vec![
        format!("# Retrieval baseline: {} queries from `{}`", cases.len(), cases_path.display()),
        String::new(),
        "Search is the Search screen's ranked list; Ask is the path Ask FNDR and every MCP tool use.".to_string(),
        "No local model is loaded, so query expansion is off; this measures retrieval, not answers.".to_string(),
        String::new(),
        "| Path | Recall@5 | MRR@10 | p50 ms | p95 ms |".to_string(),
        "|---|---|---|---|---|".to_string(),
        format!(
            "| Search | {:.2} | {:.2} | {} | {} |",
            search_score.recall_at_5(),
            search_score.mrr_at_10(),
            percentile(&search_ms, 50.0),
            percentile(&search_ms, 95.0)
        ),
        format!(
            "| Ask | {:.2} | {:.2} | {} | {} |",
            ask_score.recall_at_5(),
            ask_score.mrr_at_10(),
            percentile(&ask_ms, 50.0),
            percentile(&ask_ms, 95.0)
        ),
        String::new(),
        format!("Both paths put the same memory first on {agree_top1} of {} queries.", cases.len()),
        String::new(),
        "| Query | Kind | Search rank | Ask rank |".to_string(),
        "|---|---|---|---|".to_string(),
    ];
    report.extend(rows);
    let text = report.join("\n") + "\n";
    if let Some(out) = arg("--out") {
        if let Some(parent) = Path::new(&out).parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&out, &text)?;
    }
    print!("{text}");
    Ok(())
}
```

- [ ] **Step 6: Run the tests, then the existing search tests**

Run: `cd src-tauri && cargo test --example retrieval_qa && cargo test --test search_flow && cargo test --lib search`
Expected: 4 example tests PASS; `search_flow` and the `search` unit tests pass unchanged.

- [ ] **Step 7: Add the Make target and record the seeded baseline**

Add `qa-retrieval` to `.PHONY` and append to `Makefile`:

```make
QA_QUERIES := $(CURDIR)/scripts/demo/knowledge-worker-queries.json

qa-retrieval:
	cd src-tauri && CARGO_BUILD_JOBS=2 cargo run --example retrieval_qa -- --data-dir "$(QA_PROFILE)" --cases "$(QA_QUERIES)" --out "$(CURDIR)/docs/evidence/W01/retrieval-baseline-seeded.md"
```

Run: `make qa-seed && make qa-retrieval`
Expected: a table with Recall@5 and MRR@10 for Search and Ask, latency, the agreement line, and one row per query. Do not tune anything in this task; whatever the numbers are, they are the baseline. Read the paraphrase rows closely: a Search miss where Ask hits points at the word-overlap cutoff in `search/reranker.rs`.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/ipc/commands/search.rs src-tauri/examples/retrieval_qa.rs scripts/demo/knowledge-worker-queries.json Makefile docs/evidence/W01/retrieval-baseline-seeded.md
git commit -m "feat(eval): retrieval baseline through the Search and Ask paths on the seeded profile"
```

### Task 7: Integration check and hand the QA build to the owner

**Files:**
- No source changes expected. Evidence notes go in the MR description, not in git.

**Interfaces:**
- Consumes: Tasks 1 to 6.
- Produces: a green branch, a browser screenshot of Home and Settings, and a seeded QA profile ready for Pass A.

- [ ] **Step 1: Full sweep**

Run: `make test`
Expected: typecheck, Vitest, Vite build, and `cargo test` all pass. Paste the tail of the output into the hand-off message.

- [ ] **Step 2: Browser preview check**

Run: `make qa-preview`, open `http://localhost:1420/ui-preview.html`.
Expected: Home shows "Pick up where you left off" with three threads visible without scrolling at 1280 by 800; Settings shows "Agent access" and, after clicking "Turn on agent access", the endpoint and three copy buttons. Take one screenshot of each and attach them to the hand-off message. Stop Vite.

- [ ] **Step 3: Native smoke on the QA profile**

Run: `make qa-app`
Expected: FNDR opens on the QA profile with Home listing `inbox-triage` (no project, so it falls back to its session key; that is real engine behavior worth scoring), CS 3500, HIST 2100, Capstone, and Internship threads (ages depend on when you seeded; re-run `make qa-seed` to refresh). Settings, Agent access, Turn on: the endpoint reads `http://127.0.0.1:47821/mcp`. From a terminal:

```bash
curl -s -X POST http://127.0.0.1:47821/mcp -H 'content-type: application/json' \
  -H "Authorization: Bearer $(cat ~/.fndr/mcp_token)" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory.resume_work","arguments":{"hours":72}}}' | head -c 600
```

Expected: JSON containing `"CS 3500 Assignment 4"`. Quit FNDR.

- [ ] **Step 4: Push the branch only after the owner has read the month plan**

The month plan names each teammate's lane. Do not push `qa/user-first-reset` to the shared remote until the owner has reviewed `docs/team/2026-10-month-plan.md`. When approved:

```bash
git push -u origin qa/user-first-reset
```

### Task 8: Owner QA session (needs the owner)

**Files:**
- Modify: `docs/product/qa-walkthrough.md` (fill the tables; keep personal or sensitive details out of it)

- [ ] **Step 1: Pass D (10 minutes).** `make vault-health` and `make qa-retrieval`; paste the summary lines into the walkthrough.
- [ ] **Step 2: Pass A (90 minutes).** `make qa-seed && make qa-app`. Work through cards 4 to 12 and 15 as Sam.
- [ ] **Step 3: Pass C (30 minutes).** Card 8 on the QA profile.
- [ ] **Step 4: Pass B (two working days).** Run the real app on your real profile all day, every day; do cards 1 to 7, 13, 14, 16 and the Resume task. At the end, run `make vault-health` again and compare memories per active day with the Pass D baseline.
- [ ] **Step 5: Fill the scorecard and open notes, then commit on the branch.**

```bash
git add docs/product/qa-walkthrough.md
git commit -m "docs(qa): owner scores from seeded, live, and agent passes"
```

### Task 9: Turn the scores into the month plan and hand it to the team

**Files:**
- Modify: `docs/team/2026-10-month-plan.md` (section "What the hands-on pass found" and any lane item a verdict changes)
- Modify: `docs/team/TEAM.md` (point "What we are building" at the month plan)

- [ ] **Step 1:** Copy each scorecard row into the month plan's findings table with the verdict and the one sentence.
- [ ] **Step 2:** For each "cut" or "hide in Labs" verdict, confirm the matching item in the product experience lane's week 1 list; for each "fix" verdict on an engine feature, confirm it has an owner and a "done when."
- [ ] **Step 3:** Change the month plan status line from "Draft v0" to "v1, agreed on <date>" after the Monday meeting.
- [ ] **Step 4:** Commit, push, and share the link in the team chat.

```bash
git add docs/team/2026-10-month-plan.md docs/team/TEAM.md
git commit -m "docs(team): October month plan v1 from the hands-on QA pass"
git push
```

---

## Self-review

**Spec coverage.** Own sweep from user and company roles: Part 1 (evidence), Part 3 (SWOT pushback), and the month plan's role lens. The owner's second list (reopen, Vault and RAG quality, computer use, skills from actions, voice, LanceDB, other flags): Part 1b, with measurement in Tasks 5 and 6, walkthrough cards 7 and 16, and month-plan lanes. How useful the QA pass is and what has to be true first: Part 2. Short-term QA enablers first: Tasks 1 to 7. Month-long hand-off split into four lanes: `docs/team/2026-10-month-plan.md`, finalized by Task 9. Knowledge-worker niche: the seeded persona (Task 2) and the month plan audience. Two-way MCP and loosened local rules: Part 3 and the month plan's decisions and lanes. "How it actually works" for each feature: Task 1 cards.

**Placeholder scan.** Every code step has code; the walkthrough is complete; Task 6 is intentionally human work with no code. No "TBD."

**Type consistency.** `search_ranked_results(&AppState, &str, Option<&str>, Option<&str>, usize)` is defined in Task 6 Step 3 and called with those types in the Task 6 example. `summarize` and `render` keys match between the Task 5 script and its test. `ResumeThread` fields match `resume/mod.rs` and `pack.rs` serialization (`title`, `last_state`, `age_minutes`, `next_steps`, `evidence`, `pack.items[].memory_id`, `pack.dropped_for_budget`). `resumeWork(hours, budgetTokens)` maps to Rust `hours`, `budget_tokens` through Tauri's camelCase argument convention used elsewhere (`dateStr` for `date_str`). `McpServerStatus` fields match `mcp/mod.rs:56-69`. `record_at` and `timestamp_ms(entry, now)` are used consistently in `main` and the tests.
