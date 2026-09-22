# FNDR Beta-to-Final Master Plan Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. This is the master plan. Four workstream plans hang off it (links in section 9). Ticket IDs in the manifest (section 7) are the single source of truth for who does what and when.

**Goal:** Ship the strongest Beta (assumed Wed 2026-10-21) and Final (assumed week of 2026-12-14) FNDR in the capstone class, built on this v1 codebase, with a team that always knows what to do next, why, how to verify it, and where the evidence lives.

**Architecture:** Five workstreams (capture pipeline, local-model harness, features, team operating system, everything else) run as twelve weekly sprints on a GitLab board. Beta is a measured, demoable "hero loop" plus a trust layer. Final adds an approve-then-act local agent and a demonstrated self-improvement loop for the local model. Planning is rolling-wave: W1 to W4 are ticket-level, W5 to W12 are epic-level until the Beta retro.

**Tech Stack:** Tauri 2 + Rust (`src-tauri/`), React + TypeScript (`src/`), LanceDB, ONNX Runtime embeddings, llama.cpp via `llama-cpp-2` (Metal), Apple Vision OCR, GitLab at `capstone.cs.utah.edu` (remote `git@capstone.cs.utah.edu:fndr/fndr.git`).

**Spec:** The owner's request of 2026-09-21 (restated in section 1) plus `docs/v2/PRD.md` (historical v2 plan, mined for findings, see section 6). Decision record: section 3.

## Global Constraints

- Product base: this repo (FNDR v1). FNDR v2 (`~/FNDR-2.0`) is a read-only knowledge source. Owner decision 2026-09-21.
- Strictly local models: no cloud LLM at runtime, not even opt-in.
- Authorship: a human commits and pushes under their own name. AI agents (Claude Code, Codex) write and verify changes and leave them uncommitted; no commit, PR, or ticket carries an AI co-author trailer.
- Automations are FNDR-internal scheduled jobs only. No external webhooks or integrations.
- Mobile companion (`apps/ios/`, `src-tauri/src/companion/`) is parked until after Final. (Assumption D-2, confirm.)
- Reference machine: Apple M1, 8 GB RAM (`FNDR_MODEL_PROFILE = "m1_8gb_default"` in `src-tauri/src/inference/model_config.rs`). Every performance budget is stated for it.
- Never commit real captures, database blobs, tokens, or contents of ignored data directories (`AGENTS.md`).
- No commits to `main`. Branch, then merge request. Never auto-create worktrees.
- Verification default: `make test` from the repo root. State what you ran.
- No em dashes in any document, ticket, or commit message (owner preference, recorded in the v2 kickoff conventions).
- Capacity: four people, 15+ hours per week each. Plan P0 work to 85 percent of that.
- Instructors inspect three things: the live demo with slides, a written evidence packet, and the GitLab board plus commit history.

---

## 1. The ask, restated

1. Break the capture pipeline into its smallest parts and improve each with resource usage and best-in-market practice in mind. See `2026-09-21-ws1-capture-pipeline-teardown.md`.
2. Build a support system around the local model so stored output is best for agents and humans, and so the model is easy to customize, tune, RLHF, and improve. Teach the owner how all of it works through staged questions and answers. See `2026-09-21-ws2-local-model-harness.md`.
3. Replace cool-but-weak features with ones that matter for daily development, are local, agentic, and stand out on a resume. Research externally. See `2026-09-21-ws3-feature-thesis-and-research.md`.
4. Fix team clarity: a GitLab board where every ticket says what, when, why, how to test, what evidence to attach, and how to hand off, plus a shared picture of the 3-month product. See `2026-09-21-ws4-team-operating-system.md`.
5. Recommend everything else worth time. Section 10 below.
6. Break down everything after capture (embeddings, RAG, similar-memory merge and dedup, better local-model output, storage and finalization) and what it means for search, recall, ranking, MCP, agents, and computer use. See `2026-09-21-ws5-post-capture-memory-pipeline.md`. The owner is accountable for both pipelines.
7. Research Jev (TypeSafe AI) and other fast, cheap local models; decide where they fit. See `2026-09-21-ws6-bounded-decisions-and-fast-local-models.md`.
8. Give every ticket the detail the class expects (PR shape, what happened earlier, next steps, phase progress, time, owner) in a form that works on GitLab and for Claude Code and Codex with switching on limits. See section 7 and WS4 section 8.

## 2. State of the product (verified 2026-09-21)

| Area | Finding | Source |
|---|---|---|
| Codebases | Teammates commit to this repo (Wrapped, Screen Guide, BGE fixes, Sep 8 to 11). The v2 rebuild lives in `~/FNDR-2.0` (GitHub) and only the owner commits there. Two planning worlds, one team. | `git log` in both repos |
| Capture loop | One 6,485-line file, `src-tauri/src/capture/mod.rs`, with `run_capture_loop` at line 1872 spanning about 2,000 lines. | file |
| Pixel capture | `CGDisplay::screenshot` at `capture/macos.rs:106`. Deprecated Apple API family. | file |
| App context | Window title and browser URL come from `osascript` child processes (`capture/macos.rs:250, 298, 382`). A process spawn per capture. | file |
| Timing data | Only `capture.flush_ms` is recorded inside the loop. No per-stage latency, no percentiles. | `capture/mod.rs:2002`, `telemetry/runtime_metrics.rs` |
| Drop reasons | Good news: `SkipReason` has 15 variants with per-reason atomic counters (`lib.rs:152`, `CapturePipelineStats`). | file |
| Resource sampling | Good news: `telemetry/system_metrics.rs` samples process CPU, RSS, energy wakeups, host memory, and GPU at 1 Hz. | file |
| Models | Qwen3-VL-2B Q4_K_M VLM (3.5 GB RAM budget), a 1B-class GGUF text LLM, MiniLM 384-d live embeddings, BGE-large 1024-d chunk table, CLIP 512-d image embeddings. | `inference/model_config.rs` |
| Model evals | Retrieval tests exist (`tests/search_relevance_eval.rs`, `tests/chunk_retrieval_quality.rs`, `src/evals/`). No labeled set for LLM output quality. No feedback capture. | files |
| MCP security | **Beta-blocking finding, verified in code.** In the default Local mode: `default_require_auth` is false (`mcp/mod.rs:149`), so `should_bypass_http_auth` returns true for every request (`mcp/mod.rs:856`); `is_origin_allowed` returns true immediately (`mcp/mod.rs:906`); the CORS layer allows any origin, method, and header (`mcp/mod.rs:674`). The server exposes 51 tools without a token, including `memory.search_raw`, `memory.source_evidence`, `agent.run`, and `start_meeting`. It binds `127.0.0.1` on a random port, so a caller must find the port, but any local process or web page that does can read the memory store. Matches v2's audit. | file |
| Post-capture code | **Three embedding-document composers exist; one is live.** `compose_primary_embedding_text` (`capture/mod.rs:1697`) is called only from tests; the typed `memory/` distill, decide, and embed_doc path is reachable only from `ipc/commands/debug.rs`; four `memory/` and compaction helpers have no callers. The live composer is `compose_memory_embedding_document` (`memory_embedding_document.rs:180`). | `scripts/audit/callers.py` run, WS5 section 1 |
| Agent and computer-use code | v1 already has `agent/` (2,418 lines: modes Ask, Plan, Act, Learn; permission scopes; risk levels; approvals by action status; audit; a command allowlist) and `accessibility/mod.rs` (806 lines, AXUIElement bindings for autofill). `policy_for_action` allows `OpenUrl` and `OpenFile` in Act mode without approval. Retrieval feedback (`RetrievalFeedbackRating`, MCP `agent.rate_result`) already exists in `agent/audit.rs`. | `agent/*.rs`, `accessibility/mod.rs` |
| MCP surface | 51 tools in `tools_list_result` (`memory.search_full_context` to `fndr.open_target`), mixing read, write, and execute powers with overlapping names | `mcp/mod.rs:1380` |
| Team ops | No `.gitlab/` templates. CI exists only under `.github/workflows/`. Existing planning docs are v2-shaped (122 tickets, 6 milestones). | repo |
| Dev machine | Apple M1, 8 GB. Determines what fine-tuning is realistic (WS2). | `sysctl` |

## 3. Decision record

| ID | Decision | Status |
|---|---|---|
| D-1 | v1 is the Beta and Final product. v2 supplies findings, tests, and ported ideas. This reverses the v2 PRD "Semester execution addendum" (2026-09-05). | Decided by owner 2026-09-21. ADR-015 (Task 2 below) makes it official. |
| D-2 | iOS/Watch companion parked until after Final. | Assumed. Confirm. |
| D-3 | Fine-tuning data policy: training labels come from hand labeling and a local teacher model only. No cloud model generates labels. | Proposed. Confirm (WS2 Stage 5 discusses trade-offs). |
| D-4 | Beta date 2026-10-21 and Final week of 2026-12-14. | Assumed. Confirm with instructors. |
| D-5 | Hero feature set: Resume Work, Deja vu, Privacy Proof, Intelligence panel for Beta; approve-then-act agent and self-improvement demo for Final. | Proposed. WS3 explains the scoring. Pick at the Fri 2026-10-02 feature lock (or today). |
| D-6 | GitLab is the source of truth for board and merge requests. GitHub remains a push mirror only. | Proposed. |
| D-7 | Lane assignments in section 5. | Proposed from commit history. Confirm with the team Monday. |
| D-8 | Hosted decision models such as Jev never see captured content. A benchmark on synthetic or public data needs explicit owner approval and should wait for a local baseline (WS6 section 3). | Proposed. Confirm. |
| D-9 | **Owner is the accountable DRI; Executor is who does the work.** Anurup is Owner of the capture, post-capture, model, retrieval, and decision workstreams and may delegate execution. Agent leverage is upside and is not counted in capacity: hours are nominal (one competent person, no agent); measure actual hours with `/spend` in W1 and W2 before adding scope. | Decided by owner 2026-09-21 (ownership); capacity rule proposed. |

## 4. What FNDR looks like in three months

**One sentence:** FNDR is the local memory of your work. It watches, remembers, and hands you or any agent exactly the cited context needed to resume, and the model behind it measurably improves from your feedback while nothing leaves your Mac.

Three scenes carry the demo.

1. **Resume (morning).** Home shows "Resume Work" with your three active threads and their last state. Click one, or let Claude Code call `memory.get_context_pack` over MCP. The pack is cited: every claim links to a capture.
2. **Deja vu (during work).** An error appears on screen. FNDR nudges: "You fixed this on Sep 30" with the two captures that show how.
3. **Act, with approval (Final).** "Open the doc I read yesterday and paste its summary into this issue." The agent proposes three steps (open target, copy text, paste), you approve each, it executes locally, and every action lands in an audit log.

The trust layer runs under all three:

- **Privacy Proof screen.** Blocked and skipped content counts by reason, a network egress counter, and a live "visit a bank site, prove it is absent" check.
- **Intelligence screen.** Which local model is loaded, its measured scores, latency and RAM, prompt versions, feedback counts, and (Final) which fine-tuned adapter is active and how it scored against the base model.

Screens: Home (Resume plus daily brief), Vault and Search (existing), Privacy Proof, Intelligence, Approval Queue (Final), Settings.

### Beta demo script (5 minutes, target 2026-10-21)

| Min | Beat | Proof shown |
|---|---|---|
| 0:00 | Problem: agents forget your context and cloud memory tools upload it (Rewind went to Meta, Recall data was extracted). | One slide, cited sources in WS3 |
| 0:45 | Work normally for 60 seconds across editor, browser, terminal. Pipeline strip shows per-stage latency and 2 to 3 percent CPU. | Live scoreboard |
| 1:45 | Resume Work, then Claude Code pulls the cited pack over MCP. | Live |
| 2:45 | Deja vu nudge on an error (P1, cut to a recorded clip if not stable). | Live or clip |
| 3:15 | Privacy Proof: open a blocklisted site, show absence in vault, pack, and proof screen. Egress counter reads zero. | Live |
| 4:00 | Intelligence screen: bake-off table and eval scores before and after. "We chose the model by data." | Screenshot from `make eval` |
| 4:45 | What is next: approve-then-act agent, self-improving adapter. | Slide |

### Final demo additions (week of 2026-12-14)

Approve-then-act agent with three verbs; a trained adapter that beats the base model on the gold set and can be rolled back; the same demo running on a clean second Mac from the DMG.

### Success metrics (baselines measured in W1, targets proposed)

| Metric | Measured by | Beta target | Final target |
|---|---|---|---|
| Idle capture CPU average (M1 8 GB, 10 minute window) | CAP-02 report | 3 percent or lower | 2 percent or lower |
| Resident memory, VLM not loaded | CAP-02 report | 700 MB or lower | 600 MB or lower |
| Frames with exactly one terminal outcome (stored or one skip reason) | CAP-02 report | 100 percent | 100 percent |
| Extraction format validity on gold set | `make eval` | 98 percent or higher | 99.5 percent or higher |
| Extraction grounding rate (fields supported by OCR evidence) | `make eval` | 90 percent or higher | 95 percent or higher |
| Retrieval Recall@5 on gold questions | `make eval` | baseline plus 10 percent relative | baseline plus 20 percent relative |
| Context pack latency p95 | MCP test | 2 seconds or lower | 1.5 seconds or lower |
| Adapter vs base on gold target metric | `make eval` | not applicable | plus 5 points with no regression on the guard set |
| Users who finish the Resume task unaided | user study | not applicable | 4 of 5 |

## 5. Calendar and lanes

### Twelve weeks

| Week | Dates (Mon to Sun) | Theme | Gate at end of week |
|---|---|---|---|
| W01 | Sep 21 to 27 | Baseline and Board | Board live, scoreboard baseline, vision storyboard v0 |
| W02 | Sep 28 to Oct 4 | Measure and Fix | Feature lock (Fri Oct 2), gold set v0, native context capture |
| W03 | Oct 5 to 11 | Build the hero loop | Feature complete, ugly allowed (Fri Oct 9) |
| W04 | Oct 12 to 18 | Harden and Prove | Freeze (Fri Oct 16), evidence packet, two rehearsals |
| Beta | Wed Oct 21 (assumed) | Present | |
| W05 | Oct 19 to 25 | Retro and Final plan | Epic decomposition (Fri Oct 23) |
| W06 | Oct 26 to Nov 1 | Foundations for Final | Event-driven capture, prompt registry, dogfood cohort recruited |
| W07 | Nov 2 to 8 | First fine-tune | LoRA experiment reported |
| W08 | Nov 9 to 15 | Preference loop | Feedback to DPO pairs pipeline works |
| W09 | Nov 16 to 22 | Agent and integration | Approve-then-act v0 |
| W10 | Nov 23 to 29 | Thanksgiving-lite | Hardening only, docs |
| W11 | Nov 30 to Dec 6 | Freeze and package | Freeze (Fri Dec 4), report draft, video |
| W12 | Dec 7 to 13 | Rehearse and submit | Final packet complete |
| Final | Week of Dec 14 (assumed) | Present and submit | |

### Lanes (proposed from commit history, confirm Monday)

Anurup is the accountable Owner of every capture, post-capture, model, retrieval, decision, and security ticket (`Owner` column). The `Executor` column says who does the work and whether an AI agent is expected (`Kunj+agent`). Changing an executor is a one-cell edit in the manifest; the board is regenerated from it.

| Person | Lane (as executor) | Why this lane | Executes |
|---|---|---|---|
| Anurup | Lead, model harness, integration, post-capture pipeline | Architecture owner, wants the model story as the resume centerpiece | WS2 logging and prompts, WS4 setup, Resume Work backend, WS5 audit and contract work, WS6 inventory and decision core, demo script |
| Kunj | Native macOS and capture | Built the native Screen Guide | WS1 timings, baseline, native context, privacy data, dedupe; post-capture timings |
| Minh | Retrieval, evaluation, storage | Fixed BGE embedding init and chunk index smoke evals | Fixtures, gold set, eval runner, bake-off, MCP security, storage indexes, embedding ablation, ranking audit |
| Felipe | Design and product surfaces | Built Wrapped, Home hero, theme fixes | Vision storyboard, Resume screen, Privacy Proof, Intelligence panel, evidence packet, slides |

### Beta load (nominal hours by executor, 4 weeks, capacity 60 each at 15 hours per week)

Generated from the manifest by `load_by_executor` in `plan_to_csv.py`, not typed by hand.

| Executor | P0 hours | P1 hours | Total | P0 share of 60 |
|---|---|---|---|---|
| Anurup | 54 | 60 | 114 | 90 percent |
| Kunj | 56 | 14 | 70 | 93 percent |
| Minh | 54 | 38 | 92 | 90 percent |
| Felipe | 55 | 14 | 69 | 92 percent |
| **Team** | **219** | **126** | **345** | **91 percent of 240** |

P0 is 91 percent of capacity by design, so P0 fits without counting any agent help. The P1 hours (126 in total) exceed what is left (21 hours) on purpose: P1 is the backlog the team pulls from once the week's P0 is on track, and it is where agent leverage shows up. Rule: pull a P1 only when your P0 for the week is done or blocked. If the actual-hours data from W1 and W2 shows agents are saving time, move P1 tickets up; if not, they slide to W5. Felipe's FEA-04 and Kunj's CAP-06 are the first P1 items to drop.

## 6. What we import from v2 (findings, not code wholesale)

Ports follow v2 ADR-005 discipline: targeted function, test alongside, comment `// Ported from FNDR v2 <path>`.

| v2 asset or finding | Where in `~/FNDR-2.0` | Use in v1 | Ticket |
|---|---|---|---|
| Screen capture via ScreenCaptureKit one-shot | `crates/fndr-capture/src/source.rs` (T-302) | Replace deprecated `CGDisplay::screenshot` | CAP-06 |
| dHash plus A-B-A loop dedupe | `crates/fndr-capture/src/dedup.rs` (T-303) | Fewer redundant frames | CAP-08 |
| Browser-surface admission policy as pure function | `crates/fndr-capture/src/admission.rs` (T-304) | Testable admission | Final epic E-F1 |
| Unicode-safe scoring, `[LOW_CONF]` preservation, bundle-ID app identity (retired the "Search contains arc" bug) | `crates/fndr-textsignal` (T-301, T-306) | Check v1 text cleanup for the same defects | Final epic E-F1 |
| Adaptive sampling policy | `SamplingPolicy` (T-308) | Idle and deep-idle cadence | Final epic E-F1 |
| Soak runner with RSS trend | `fndr-shell` example `capture_soak` (T-310) | Model for CAP-02 report | CAP-02 |
| FNDR-Bench corpus format, Recall@5, MRR@10, p50 and p95 | `crates/fndr-bench` (E05) | Model for `make eval` | MOD-05 |
| Embedding contract with asymmetric prefixes and matryoshka truncation | `fndr-inference/src/embedding.rs` (T-402) | Embedding ablation | Final epic E-F3 |
| Single model-worker priority queue replacing a global mutex | `fndr-inference/src/model_worker.rs` (T-403) | Replace `model_pipeline_lock` contention | Final epic E-F2 |
| Auth-always MCP plus two adversarial tests named `mcp_rejects_unauthenticated_loopback` and `mcp_rejects_web_origin_with_valid_token` | `fndr-mcp/tests/auth_surface.rs` (T-701) | Security check | SEC-01 |
| Context pack with token budget, citations, dropped-for-budget count | v2 `fndr.context_pack` (T-702); v1 equivalent is `memory.get_context_pack` | Resume Work | FEA-02 |
| `session_story` cited narrative | ADR-007, T-709 | Session Story | Final epic E-F5 |
| Evaluation and claims rules | `docs/skills-and-evals.md` and the evaluation section of `docs/v2/PRD.md` (v2 ADR-009 is not in this repo) | Evidence rules in TEAM.md | OPS-04 |
| Founder review: only the spine matters, cut everything else cleanly | `~/FNDR-2.0/docs/journal/2026-09-07-founder-review.md` | Cut lines in section 9 | this plan |
| Lesson: a stale progress ledger hid what was missing | v2 review docs | Board is the ledger, generated not hand-edited | WS4 |

## 7. Ticket manifest

Machine-readable. `scripts/team/plan_to_csv.py` (WS4 Task 4) turns these rows into a GitLab CSV, and `scripts/team/phase_progress.py` joins them with the board to show how far along each phase is. Columns must not contain the pipe character.

- **Owner:** accountable person. **Executor:** who does the work; `+agent` means an AI coding agent (Claude Code or Codex) is expected.
- **Hours:** nominal effort for one competent person without an agent. **Prio:** P0 is required for the Beta; P1 is pulled only with slack.
- **Earlier:** what was true before the ticket. **PR shape:** what the pull request touches; the generator adds the branch name, size class, commit and review rules, and the computed Next and phase-position lines.
- Weeks map to milestones `W01-Baseline` through `W12-Submit`.

### 7.1 Beta tickets (W1 to W4): 47 tickets

| ID | Title | WS | Owner | Executor | Week | Hours | Prio | Deps | Why | Earlier | PR shape | Verify | Evidence |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| OPS-01 | Record ADR-015 (v1 is the product, v2 is the knowledge source) and update pointers | OPS | Anurup | Anurup+agent | W1 | 2 | P0 | none | Work split across two repos hides progress from teammates and instructors | CLAUDE.md says v2 work lives in ~/FNDR-2.0 and the v2 PRD calls v2 mainline | docs only: one ADR, CLAUDE.md, docs/README.md | `git grep -n ADR-015 CLAUDE.md docs/README.md` returns both files and the ADR file exists | Merge request link |
| OPS-02 | Bootstrap GitLab labels, weekly milestones, and board with script | OPS | Anurup | Anurup+agent | W1 | 2 | P0 | none | The board is how every teammate sees what to do next | No labels, milestones, or board exist in the capstone GitLab project | one script; a human runs it with a token | `scripts/team/gitlab_bootstrap.sh --dry-run` lists all labels and 12 milestones, then a real run shows a board with 6 lists | Board screenshot |
| OPS-03 | Commit issue templates and merge request template | OPS | Anurup | Anurup+agent | W1 | 2 | P0 | none | Templates force why, verify, evidence, and handoff into every ticket | No .gitlab directory exists and git shortlog splits each person into 2 to 3 rows | templates, .mailmap, one script | Creating a new issue in GitLab offers Task, Spike, Bug templates | Screenshot of template picker |
| OPS-04 | Write docs/team/TEAM.md (roles, workflow, definition of done, evidence rules, handoff format) | OPS | Anurup | Anurup | W1 | 5 | P0 | OPS-03 | Teammates need one page that answers what do I do now | Existing docs point agents at v2 and at portable skills, not at a teammate workflow | one Markdown file plus a README row | Each teammate reads it and picks up a ticket unaided in W2 | TEAM.md merged, W2 retro note |
| OPS-05 | Generate and import W1 to W4 tickets from this manifest | OPS | Anurup | Anurup+agent | W1 | 3 | P0 | OPS-02, OPS-03, OPS-12 | Turns this plan into assignable work with why, verify, and evidence on each | The manifest exists in the master plan but no board exists | two scripts copied from tested assets | `python3 scripts/team/test_plan_to_csv.py` passes and the board shows 47 open issues | Board screenshot |
| OPS-06 | Hardware inventory and bench machine designation | OPS | Minh | Minh | W1 | 2 | P0 | none | Benchmarks and fine-tuning plans depend on each machine's chip and RAM | Nobody has listed team hardware; only the M1 8 GB dev machine is known | one Markdown table | `docs/team/hardware.md` lists chip, RAM, macOS version per person and names one bench machine | File merged |
| OPS-07 | Weekly status report template and evidence folder convention | OPS | Felipe | Felipe | W2 | 6 | P0 | OPS-04 | Instructors get a predictable weekly artifact and we get evidence for free | No weekly status or evidence folder convention exists | templates and folders only | `docs/status/2026-W40.md` exists and follows the template | The report |
| OPS-09 | Assemble Beta evidence packet | OPS | Felipe | Felipe | W4 | 5 | P0 | all Beta P0 | Instructors grade the written evidence | Evidence files accumulate under docs/evidence as tickets close | one index file | Packet index links every claim in the demo to a file | Packet file |
| OPS-10 | Beta slides, storyboard, two rehearsals, backup video | OPS | Felipe | Felipe | W4 | 6 | P0 | FEA-03, FEA-05 | A demo that fails live needs a recorded backup | Storyboard frames and the Resume and Privacy screens exist by W3 | outline and rehearsal log; the deck lives outside git | Two timed rehearsals recorded; backup video plays offline | Videos index |
| OPS-11 | Beta demo script, hard-question prep, and dry run with a non-team person | OPS | Anurup | Anurup | W4 | 8 | P0 | FEA-02 | The owner presents the model story and answers "how is this different from Recall" | The Beta demo script exists in the master plan section 4 | one Q and A document | Dry run under 5 minutes; 10 hard questions answered in `docs/product/qa-prep.md` | Q and A file |
| OPS-12 | Agent switching protocol: docs/team/agent-switch.md with checkpoint rules, resume prompt, and budget playbook | OPS | Anurup | Anurup+agent | W1 | 2 | P0 | none | The owner and teammates will switch between Claude Code and Codex when a 5-hour limit or credits run out, and work must survive the switch | AGENTS.md is shared by both tools and a handoff skill exists; no protocol for mid-ticket switching exists | one Markdown file | A test switch on a small ticket: start in one tool, stop after step 1, finish in the other from the handoff note alone | The handoff note and the finished merge request |
| CAP-01 | Per-stage timings with p50 and p95 in runtime metrics | CAP | Anurup | Kunj+agent | W1 | 6 | P0 | none | Only flush time is recorded today, so nothing else can be optimized | Only capture.flush_ms is recorded in the loop; runtime metrics keep averages but no percentiles | telemetry plus capture loop; unit tests | `cd src-tauri && cargo test runtime_metrics` passes; Pipeline Inspector shows `capture.ocr_ms` p95 | Test output plus screenshot |
| CAP-02 | Metrics NDJSON dump and `make capture-baseline` report | CAP | Anurup | Kunj+agent | W1 | 8 | P0 | CAP-01 | The baseline every later optimization is judged against | Metrics live only in memory and the Pipeline Inspector; nothing is exported for a report | Rust dump task, one Rust method, Python report script with tests, Makefile target | `python3 scripts/bench/test_summarize_metrics.py` passes; a 2-hour real run writes `docs/evidence/W01/capture-baseline.md` | Report plus machine spec line |
| CAP-03 | 30-screen synthetic fixture corpus with ground truth and OCR plus cleanup CER test | CAP | Anurup | Minh+agent | W1 | 10 | P0 | none | Repeatable input compares optimizations without live screens or real captures | No fixture corpus exists; OCR quality has never been measured on repeatable input | 30 images, a manifest, one Rust integration test | `cd src-tauri && cargo test capture_fixtures` prints CER per app class under budget | Test output plus fixture README |
| CAP-05 | Native window title and URL via Accessibility and NSWorkspace, remove per-tick osascript | CAP | Anurup | Kunj+agent | W2 | 14 | P0 | CAP-02 | Each capture spawns osascript children (`capture/macos.rs:250, 298, 382`) which costs CPU and wakeups | Title and URL come from osascript children; accessibility/mod.rs already holds AXUIElement bindings for autofill | extend the accessibility module, fallback logic, before and after measurements | Before and after `capture.context_ms` p95 and idle wakeups in the baseline report; `cargo test macos` passes | Before and after table |
| CAP-07 | Privacy Proof data over IPC: skip counts by reason, egress counter, secure-input check | CAP | Anurup | Kunj+agent | W3 | 8 | P0 | CAP-01 | The demo must prove sensitive content never entered storage; counts live only inside `CapturePipelineStats` today | Skip counts exist in CapturePipelineStats but no IPC command exposes them | new module, one command, http_util counter | `cargo test privacy_proof` passes; `get_privacy_proof` returns counts and no content | Test output plus JSON sample |
| CAP-08 | Dedupe upgrade: port v2 dHash and A-B-A loop detection, compare on fixtures | CAP | Anurup | Kunj+agent | W3 | 8 | P0 | CAP-03 | Fewer redundant frames means less OCR, embedding, and storage | Dedupe uses img_hash and a novelty ring; the false-drop rate is unknown | dedupe functions plus a sequences fixture test | Fixture replay shows a higher dedupe ratio with zero false drops on the 30-screen corpus | Before and after ratio table |
| CAP-06 | ScreenCaptureKit one-shot capture replacing CGDisplay screenshot | CAP | Anurup | Kunj+agent | W4 | 14 | P1 | CAP-05 | `CGDisplay::screenshot` (`capture/macos.rs:106`) is a deprecated API family | CGDisplay::screenshot is the only backend; v2 has a working ScreenCaptureKit source | one PR behind an env switch, spike note first | Before and after `capture.pixels_ms` p95; capture recovers after revoking and re-granting Screen Recording | Table plus recording |
| SEC-01 | Require MCP auth by default in Local mode, enforce Origin, and restrict CORS, with two adversarial tests | SEC | Anurup | Minh+agent | W2 | 6 | P0 | none | Default Local mode needs no token, skips the origin check, and allows any CORS origin while exposing 51 tools (`mcp/mod.rs:149, 674, 856, 906`) | Local mode needs no token, skips Origin, and allows any CORS origin (verified 2026-09-21) | mcp/mod.rs, tests, docs, one ADR | `cd src-tauri && cargo test mcp_rejects` shows both new tests passing and a before and after curl transcript is saved | Test output plus curl transcript plus ADR-017 |
| MOD-02 | Inventory every LLM call site and add `llm_traces` (task, prompt version, tokens, latency, validator result) | MOD | Anurup | Anurup+agent | W2 | 8 | P0 | none | Every model improvement needs to know what was asked and what came back | Prompts are inline constants, no call is traced, and one generation runs at a time | trace module, inference return type, catalog doc | `docs/product/llm-task-catalog.md` lists every call site; `cargo test llm_traces` passes | Catalog file plus sample trace rows without content |
| MOD-04 | Gold set v0: 50 labeled extraction cases and 30 retrieval questions | MOD | Anurup | Minh | W2 | 8 | P0 | CAP-03 | No labeled set exists for LLM output; without it model claims are opinion | No labeled data exists for LLM output | fixtures and a README, no code | `src-tauri/tests/fixtures/gold/v0/` has 50 cases; a second person re-labels 10 and agreement is recorded | Dataset README plus agreement figure |
| MOD-05 | Eval runner: `make eval` scores extraction and retrieval, writes JSON and markdown | MOD | Anurup | Minh+agent | W3 | 12 | P0 | MOD-04 | Turns model and prompt changes into numbers | Retrieval tests exist but nothing scores LLM output or reports intervals | scripts, one example binary, Makefile | `make eval` exits 0 and writes `docs/evidence/W03/eval-run.md` with format validity, grounding rate, Recall@5, MRR@10 | Report file |
| MOD-06 | Baseline the current models and prompts on the gold set | MOD | Anurup | Minh+agent | W3 | 4 | P0 | MOD-05 | The before number for every Beta claim | The current models and prompts have never been scored | two evidence files | `docs/evidence/W03/eval-baseline.md` committed | Report file |
| MOD-08 | Model bake-off on the M1 8 GB: current 1B vs Qwen3 4B Q4 vs Gemma 4 E2B and E4B (Apple Foundation Models optional spike) | MOD | Anurup | Minh+agent | W4 | 12 | P0 | MOD-05, MOD-06 | Chooses the model by accuracy, latency, and RAM instead of by default | The model was chosen by default (1B text model, Qwen3-VL-2B) | evidence files and one ADR | `docs/evidence/W04/model-bakeoff.md` has model, quant, peak RAM, tokens per second, format validity, grounding rate | Report plus ADR |
| MOD-15 | Intelligence panel v0: model profile, eval scores, trace counts, resource gauges | MOD | Felipe | Felipe+agent | W4 | 8 | P0 | MOD-02, MOD-05 | Makes the model work visible to the audience and to instructors | Pipeline Inspector shows latencies but no eval scores are visible | one component, types, one IPC command | `npm test intelligence` passes; screenshot shows numbers from a real eval report | Screenshot plus test output |
| FEA-01 | Final Vision storyboard: five Figma frames (Resume, Deja vu, Privacy Proof, Intelligence, Approval Queue) | FEA | Felipe | Felipe | W1 | 12 | P0 | none | Teammates and instructors see the 3-month product before it is built | Existing screens are v1 features; no picture of the Final product exists | PNG exports and docs | Figma link in the issue; PNG exports in `docs/product/vision/`; team review on Friday | Figma link plus PNGs |
| FEA-02 | Resume Work backend: `resume_work` returns cited context per active thread within a token budget | FEA | Anurup | Anurup+agent | W3 | 14 | P0 | MOD-02 | The headline daily-use feature: pick up where you left off without re-explaining | memory.get_context_pack and context_runtime exist but nothing returns a per-thread resume view | new resume module, MCP wiring, tests | `cargo test resume_work` passes; a 4-hour fixture session yields at least 3 citations inside the budget; MCP `memory.get_context_pack` returns the same pack | Test output plus sample pack |
| FEA-03 | Resume Work screen on Home | FEA | Felipe | Felipe+agent | W3 | 10 | P0 | FEA-01 | Turns the backend into the first thing a user sees | Home shows a hero and a daily summary but no resume view | one component, IPC types, one test file | `npm test resume` passes; recording shows click-through on fixture data | Recording plus test output |
| FEA-05 | Privacy Proof screen | FEA | Felipe | Felipe+agent | W3 | 8 | P0 | CAP-07, FEA-01 | Trust is the differentiator against Recall and Rewind | No privacy summary screen exists | one component and IPC types | `npm test privacy-proof` passes; recording shows a blocklisted visit absent in vault, pack, and this screen | Recording |
| FEA-04 | Deja vu: detect an error on screen and surface a prior fix | FEA | Felipe | Felipe+agent | W4 | 14 | P1 | CAP-03, FEA-02 | "Have I seen this error before" is a daily developer pain and proves proactive memory | Extraction already emits an errors field but nothing matches errors across time | new module, capture hook, one toast component | Fixture error screen triggers a toast citing a prior fixture memory; zero false triggers on the 30-screen corpus | Recording plus corpus result |
| LRN-01 | Model learning Stage 1: what runs when FNDR calls the LLM | LRN | Anurup | Anurup | W1 | 2 | P0 | none | The owner must explain the model stack in the presentation and in interviews | Owner has prompting and RAG experience and has not run local inference tooling | one page of notes | One-page `docs/learning/stage-1.md` plus lab output; exit check explained aloud to a teammate | Summary file |
| LRN-02 | Model learning Stage 2: prompts, structured output, validators | LRN | Anurup | Anurup | W2 | 2 | P0 | LRN-01 | Explains why a 1B model needs a harness | Stage 1 covers the runtime; Stage 2 covers the prompt harness | one page of notes | `docs/learning/stage-2.md` plus lab output | Summary file |
| LRN-03 | Model learning Stage 3: evals and gold sets | LRN | Anurup | Anurup | W3 | 2 | P0 | LRN-02 | Explains what a claim of "better" means | MOD-04 and MOD-05 produce the first gold set and scores to reason about | one page of notes | `docs/learning/stage-3.md` plus lab output | Summary file |
| LRN-04 | Model learning Stage 4: embeddings and retrieval | LRN | Anurup | Anurup | W4 | 2 | P0 | LRN-03 | Explains why search works and how to prove it | Retrieval eval rows exist from MOD-05 | one page of notes | `docs/learning/stage-4.md` plus lab output | Summary file |
| MEM-01 | Trace the live post-capture path and list every duplicate or dead function | MEM | Anurup | Kunj+agent | W1 | 6 | P0 | none | Three embedding-document composers exist and nobody knows which one is live | callers.py on 2026-09-21 found compose_primary_embedding_text test-only and memory/ distill, decide, embed_doc debug-only; the live composer is compose_memory_embedding_document | audit script plus one doc, no production code change | `python3 scripts/audit/test_callers.py` passes and `docs/product/post-capture-live-path.md` names one live function per stage | The live-path doc |
| MEM-02 | Delete or adopt the dead and duplicate post-capture code | MEM | Anurup | Anurup+agent | W2 | 8 | P1 | MEM-01 | Dead code with passing tests hides which behavior is real and costs every teammate reading time | MEM-01 lists the candidates; the AGENTS.md anti-bloat rule says delete before adding | two PRs: remove the test-only composer, then remove or adopt the memory/ debug path; no behavior change | `make test` passes, removed function names return no hits in `git grep`, and the test count change is explained in the MR | Before and after git grep and test counts |
| MEM-03 | Post-capture stage timings and memory outcome counters | MEM | Anurup | Kunj+agent | W2 | 6 | P0 | CAP-01 | Only capture stages are timed; embed, merge, and store cost and the new versus merged mix are invisible | CAP-01 adds since_ms and percentiles; before it only capture.flush_ms existed | telemetry plus capture loop; unit tests for the counters | `cd src-tauri && cargo test runtime_metrics` passes and the baseline report shows `mem.` rows and an outcome mix | Baseline report with mem rows |
| MEM-04 | Replay sessions with gold stories and a merge and dedup baseline | MEM | Anurup | Anurup+agent | W3 | 10 | P1 | CAP-03, MEM-03 | We do not know how often the merge thresholds join unrelated frames or leave duplicates | CAP-03 provides single screens; merge_or_append_memory_record has unit tests but no labeled sessions | 6 fixture sessions of 8 frames, a replay test, and score_merge.py; tests only | `python3 scripts/model/test_score_merge.py` passes and `docs/evidence/W03/merge-baseline.md` shows precision, recall, false merges, and fragmentation | Baseline report |
| MEM-05 | Embedding text and chunk audit | MEM | Anurup | Anurup+agent | W3 | 8 | P1 | MEM-01 | Retrieval quality depends on exactly which text is embedded; boilerplate and repeated lines waste vectors | The live composer builds parent and chunk vectors (ADR 008 and 010); chunk statistics were never measured | one audit test plus up to two targeted fixes with before and after numbers | Chunk count, mean tokens, and duplicate share before and after are in `docs/evidence/W03/embedding-audit.md` | Audit file |
| MEM-06 | Enrichment scheduling: when the local model runs and what happens when it cannot | MEM | Anurup | Anurup+agent | W4 | 10 | P1 | MOD-06, MEM-03 | On 8 GB the VLM cannot always run, and the fallback path decides memory quality for many frames | build_low_ram_semantic_fusion and pressure gates exist; enriched share and time to enriched are unmeasured | one policy function with tests plus counters for enriched versus fallback | A 2-hour run reports the enriched share and time to enriched p95 in the baseline report | Report |
| MEM-07 | Memory finalization contract: invariants with tests | MEM | Anurup | Anurup+agent | W4 | 8 | P1 | MEM-02 | A stored memory must always carry provenance, a real embedding, and a lifecycle state or search and agents mislead | The rules are scattered across memory_quality.rs, validate_structured_memory_extraction, and the review lifecycle chip in MemoryCard.tsx | one assert_memory_contract test helper plus one test per invariant | `cd src-tauri && cargo test memory_contract` passes with one test per invariant in the WS5 plan section 4 | Test output |
| MEM-08 | Storage indexes and compaction for the Lance tables | MEM | Anurup | Minh+agent | W4 | 12 | P1 | MEM-01 | v2's audit found no index on any table; query time and disk grow with use | Search works at demo scale; index and compaction state of the v1 tables is unmeasured | index creation and a compaction job with a 100,000-row fixture test | Query plans use indexes on the fixture and the version count stays bounded over a simulated day | Fixture test output |
| MEM-09 | Embedding ablation: MiniLM, BGE-large, Qwen3-Embedding, and a static Model2Vec baseline | MEM | Anurup | Minh+agent | W4 | 12 | P1 | MOD-05 | Two contracts coexist (384-d live, 1024-d chunks) and no ablation says which to keep on an 8 GB machine | Minh fixed BGE lazy init on Sep 9; v2 picked Qwen3-Embedding-0.6B without a v1 ablation | ablation config and one decision ADR; no schema change | `make eval` retrieval rows per candidate with RAM and milliseconds per chunk are in `docs/evidence/W04/embedding-ablation.md` | Ablation report and ADR |
| RET-01 | Ranking and recall audit on the gold questions | RET | Anurup | Minh+agent | W4 | 8 | P1 | MOD-05 | Fusion weights, rerank, and chunk-first were tuned by hand and never ablated against labeled questions | context_runtime/fusion.rs and search/reranker.rs exist with tests that do not measure ablations | ablation switches through config and one results table | Recall@5 and MRR@10 per configuration are in `docs/evidence/W04/ranking-audit.md` | Audit file |
| RET-02 | MCP tool surface audit | RET | Anurup | Minh+agent | W3 | 6 | P1 | SEC-01 | 51 tools mix read, write, and execute powers with overlapping names that agents and reviewers cannot reason about | mcp/mod.rs lists tools from memory.search_full_context to fndr_search_code_context; v2 settled on 14 | one classification document, no code change | `docs/product/mcp-tool-audit.md` lists every tool with a risk class and a keep, merge, or remove verdict | The audit doc |
| DEC-01 | Bounded-decision inventory and three pilots | DEC | Anurup | Anurup+agent | W2 | 6 | P1 | none | FNDR makes many small semantic decisions by thresholds or by prompting a general LLM and we do not know which deserve a dedicated decision model | Activity type, admission, merge, sensitive context, query route, and tool-call policy are decided by rules or prompts today | one inventory document scored with the WS6 rubric | `docs/product/decision-inventory.md` scores each decision and names three pilots | The inventory doc |
| DEC-02 | Decision core: calibration, thresholds, and a coverage versus precision report | DEC | Anurup | Anurup+agent | W3 | 10 | P1 | DEC-01, MOD-05 | Without calibration a confidence number is decoration | Calibration and threshold tools were written and tested in the plan assets | copy the tested module, add a report script and a Rust Decision trait with a fake implementation | `python3 scripts/model/test_decision_calibration.py` passes and a coverage versus precision table exists for one pilot | The table |

### 7.2 Final epics (W5 to W12), decomposed at the Beta retro (Fri Oct 23)

| Epic | Lead | Outcome and acceptance |
|---|---|---|
| E-F1 Capture v2 | Kunj | Event-driven triggers (app switch, typing pause, idle), AX-text-first with OCR fallback, adaptive sampling, text-signal fixes, storage indexes and compaction. Acceptance: idle CPU at or below 2 percent, one terminal outcome per frame, all measured in CAP-02 report format. |
| E-F2 Model harness | Anurup | Prompt registry with versions, grammar-constrained JSON, GEPA-style prompt optimization, feedback capture, LoRA SFT experiment, DPO experiment, adapter hot-load with an eval gate and rollback. Acceptance: adapter beats base by the Final target with no guard-set regression. |
| E-F3 Retrieval quality | Minh | Embedding ablation (MiniLM vs BGE vs Qwen3-Embedding), hybrid and rerank ablations, one embedding contract chosen and migrated. Acceptance: chosen contract wins on `make eval` and fits the RAM budget. |
| E-F4 Approve-then-act agent | Kunj with Felipe | Perception (AX tree plus local VLM), three verbs (open target, copy text, paste text), approval queue UI, audit log. Acceptance: each verb refuses without approval, is logged, and passes an adversarial prompt-injection test. |
| E-F5 Session Story and Daily Brief | Felipe | Cited session narrative exportable as standup or PR text; Daily Brief replaces Wrapped with follow-ups that matter. Acceptance: user study task success. |
| E-F6 User study | Felipe with Anurup | Five outside users, 30 minutes each, task script, SUS score, quotes. Acceptance: results table in the Final packet. |
| E-F7 Release hardening | Everyone | 8-hour soak, clean-Mac install from DMG, crash-free run, docs. Acceptance: checklist in WS4 "Final packet". |
| E-F8 Final packet | Felipe with Anurup | Report, slides, poster, video, evidence index. Acceptance: instructors can trace every claim to a file. |

Post-capture and decision epics are specified in WS5 section 6 (E-F9 to E-F15) and WS6 section 10 (DEC-03 to DEC-08).

## 8. Risks and cut lines

| Risk | Likelihood | Effect | Mitigation | Cut line |
|---|---|---|---|---|
| Teammates keep working outside the board | High | Instructors see no contribution trail | OPS-04 rules: no board card, no merge | Weekly Friday review closes any orphan branch |
| 1B model cannot produce reliable structured output | High | Bad stored memories | WS2 harness and bake-off (MOD-08); grammar-constrained decoding | Fall back to deterministic extraction fields |
| Fine-tuning on an 8 GB M1 is too slow or memory-bound | Medium | Final model story weakens | Use smallest model, short sequences, QLoRA; use a larger machine from hardware inventory; public and synthetic data only off-device | Show harness gains (prompt, grammar, few-shot) with measured deltas |
| Deja vu is unstable | Medium | Beta demo embarrassment | It is P1 with a recorded clip fallback | Drop FEA-04 first |
| Scope creep from cool features | High | Beta not evidence-backed | Feature lock Fri Oct 2; freeze Fri Oct 16 | Any feature without a working end-to-end slice and evidence is removed from the demo |
| MCP defaults expose the memory store to any local process or web page that finds the port | High | Real privacy hole in a privacy product, and an easy instructor question | SEC-01 first thing in W2; until then run any demo build with `FNDR_MCP_REQUIRE_AUTH=1` | Flip default to auth-required (this is the fix) |
| Live demo fails | Medium | Lost grade | Fixture-seeded demo vault (`scripts/demo/demo-week.json` exists), recorded backup | Demo from backup |
| Scope outruns capacity: 47 Beta tickets, 126 P1 hours, and an owner who is accountable for six workstreams | High | Beta demo weakens because everything is half done | Hours are nominal and agent leverage is not counted; P1 pulled only with slack; measure actual hours in W1 and W2; Friday review cuts P1 first | Drop P1 tickets in this order: CAP-06, FEA-04, MEM-08, MEM-09, RET-01, MEM-06 |
| Deleting dead code removes tests for real behavior | Medium | Silent loss of coverage | MEM-02 ports each behavior test to the live function before deleting | Keep the test and file a bug if the live path fails it |
| Two coding agents disagree or duplicate work when switching | Medium | Wasted credits, merge conflicts | One ticket per branch, commit per step, handoff note and resume prompt (WS4 section 8) | Owner picks one agent per ticket and says so in the issue |
| Two-repo confusion returns | Medium | Duplicated or invisible work | ADR-015, CLAUDE.md pointer, OPS-01 | n/a |

## 9. Companion plans

| Plan | File | Tickets |
|---|---|---|
| WS1 Capture pipeline teardown | `2026-09-21-ws1-capture-pipeline-teardown.md` | CAP-01 to CAP-08 |
| WS2 Local model harness and learning path | `2026-09-21-ws2-local-model-harness.md` | MOD-02 to MOD-15, LRN-01 to LRN-07 |
| WS3 Feature thesis and research | `2026-09-21-ws3-feature-thesis-and-research.md` | FEA-01 to FEA-05 |
| WS4 Team operating system | `2026-09-21-ws4-team-operating-system.md` | OPS-01 to OPS-11 |
| WS5 Post-capture memory pipeline | `2026-09-21-ws5-post-capture-memory-pipeline.md` | MEM-01 to MEM-09, RET-01, RET-02 |
| WS6 Bounded decisions and fast local models | `2026-09-21-ws6-bounded-decisions-and-fast-local-models.md` | DEC-01, DEC-02, Final DEC-03 to DEC-08 |

## 10. Other things worth spending time on (answer to ask 5)

| Recommendation | Why | When | Owner |
|---|---|---|---|
| Fixture-seeded demo vault plus recorded backup video | A live demo that depends on what you happened to capture will fail | W3 seed, W4 rehearse | Felipe |
| Threat model one-pager (what is captured, where it lives, what leaves, what an attacker on this Mac gets) | The first question from any instructor is about privacy; Recall's stored data was extracted in March 2026 | W2 | Minh with SEC-01 |
| Five-person user study with a fixed task script and SUS | Real user quotes and a score beat adjectives in a report | Recruit W6, run W8 to W9 | Felipe |
| Clean second Mac install test from the release DMG (`.github/workflows/release.yml` already builds it) | "Works on my machine" is the most common capstone demo failure | W11 | Kunj |
| Incidents and reversals log (`docs/incidents.md`) | v2 kept one; it is the best interview material because it shows judgment. Example already in your history: the 2026-05-17 silent-drop bug that stored nothing | Start W1, append weekly | Everyone |
| Related-work slide with honest comparison (Screenpipe, Recall, Rewind and Limitless, Mem0 and Zep) | Instructors reward positioning; WS3 has cited facts | W3 | Felipe |
| Third-party license check for models (Llama community license vs Apache 2.0 Qwen and Gemma terms) | Redistribution terms matter if the DMG bundles or downloads models | W5 | Minh |
| Split `capture/mod.rs` by stage only as each WS1 task touches it | 6,485 lines in one file slows every teammate; do it opportunistically with tests, never as a drive-by | W2 to W6 | Kunj |
| Accessibility and contrast pass on new screens (the design skill `design:accessibility-review` applies) | Cheap credibility, and Felipe already fixed light-theme contrast once | W4 and W11 | Felipe |
| Course-facing artifacts: final report outline, ethics and privacy statement, poster | The report is graded; starting in W8 avoids a December crunch | Outline W5, draft W11 | Felipe with Anurup |

---

## Plan tasks

### Task 1: Confirm the open decisions

**Files:**
- Modify: this file, section 3 (status column)

**Interfaces:**
- Produces: final answers to D-2, D-3, D-4, D-5, D-6, D-7 that every other plan reads.

- [ ] **Step 1: Hold a 30-minute team meeting Monday 2026-09-21 or Tuesday 2026-09-22**

Agenda: (1) confirm Beta and Final dates with instructors, (2) confirm lanes, (3) confirm parked iOS, (4) pick hero features, (5) confirm GitLab is the board.

- [ ] **Step 2: Update the Status column of section 3 with the answers and commit**

```bash
git checkout -b docs/beta-final-plan
git add docs/superpowers/plans/
git commit -m "docs: add Beta-to-Final master plan and four workstream plans"
```

Expected: commit succeeds on the branch (the repo hook blocks commits to `main`).

### Task 2: Write ADR-015 (ticket OPS-01)

**Files:**
- Create: `docs/decisions/015-v1-product-v2-knowledge-source.md`
- Modify: `CLAUDE.md` (the first paragraph), `docs/README.md` (decisions row)

**Interfaces:**
- Produces: the written decision every teammate and agent reads first.

- [ ] **Step 1: Create the ADR**

```markdown
# ADR-015: FNDR v1 is the product; FNDR v2 is a knowledge source

Status: Accepted 2026-09-21
Deciders: FNDR team (4)

## Context

FNDR v2 (`~/FNDR-2.0`) was planned as a rebuild and its PRD named it the mainline for the semester (Semester execution addendum, 2026-09-05). Since then teammates have shipped features in this v1 repo (Screen Guide, Wrapped, BGE chunk indexing fixes), the GitLab remote used by the course points at this repo, and v2 has a thin UI (a search screen) against a wide v1 feature set.

## Decision

The Beta (about 2026-10-21) and Final (about 2026-12-14) run on this repo. v2 is read-only reference: we take its audit findings, tests, and targeted ports (with a `// Ported from FNDR v2 <path>` note), not its architecture wholesale.

## Consequences

- One repo, one board, one commit history for instructors to inspect.
- v2's crate boundaries are not adopted; WS1 extracts stage modules opportunistically instead.
- The v2 PRD addendum is superseded for schedule. Its MCP surface rules (`docs/v2/decisions/ADR-007-mcp-surface.md`) and local-only boundary (`ADR-004-local-only-boundary.md`) remain useful references.
- `CLAUDE.md` no longer directs v2 work elsewhere.
```

- [ ] **Step 3: Edit `CLAUDE.md` first paragraph**

Replace the "FNDR v2 lives in its own repo" paragraph with:

```markdown
**As of 2026-09-21 (ADR-015), this repo is the Beta and Final product.** FNDR v2 (`~/FNDR-2.0`) is a read-only knowledge source: take findings and targeted ports from it, do not build there. The Beta-to-Final plan lives in `docs/superpowers/plans/2026-09-21-beta-final-master-plan.md`.
```

- [ ] **Step 4: Verify the pointers**

Run: `git grep -n "ADR-015" -- CLAUDE.md docs/README.md docs/decisions`
Expected: hits in all three paths.

- [ ] **Step 5: Commit**

```bash
git add docs/decisions/015-v1-product-v2-knowledge-source.md CLAUDE.md docs/README.md
git commit -m "docs: ADR-015 v1 is the product, v2 is a knowledge source"
```

### Task 3: Require MCP auth by default (ticket SEC-01)

**Files:**
- Modify: `src-tauri/src/mcp/mod.rs` (`default_require_auth` at line 149, `should_bypass_http_auth` at 856, `is_origin_allowed` at 906, the CORS layer at 674 to 677, and the existing tests near line 5090)
- Modify: `docs/mcp.md` (the connect-your-agent snippet)
- Create: `docs/decisions/017-mcp-auth-default.md`, `docs/evidence/W02/sec-01.md`

**Interfaces:**
- Consumes: the bearer token in `~/.fndr/mcp_token` (`mcp/token.rs`).
- Produces: default behavior where every `tools/call` needs `Authorization: Bearer <token>`, only `initialize` and `tools/list` from a loopback peer are exempt, and a request carrying an `Origin` header is refused unless that origin is listed in `FNDR_MCP_ALLOWED_ORIGINS`.

Immediate mitigation, available today with no code change: launch any demo build with `FNDR_MCP_REQUIRE_AUTH=1`.

- [ ] **Step 1: Capture the before transcript**

With the app running, take the MCP endpoint shown in Settings and run (replace `<endpoint>`):

```bash
curl -s -X POST <endpoint> -H 'content-type: application/json' -H 'Origin: https://evil.example' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory.search_raw","arguments":{"query":"the"}}}' | head -c 400
```

Expected today: a JSON result, not an error. Save the command and the first 400 characters of output with any memory text replaced by `[redacted]` into `docs/evidence/W02/sec-01.md`.

- [ ] **Step 2: Write the two failing tests inside the existing test module of `mcp/mod.rs`**

```rust
    #[test]
    fn mcp_rejects_unauthenticated_tool_call_in_default_local_mode() {
        let mode = McpDeploymentMode::Local;
        let peer: SocketAddr = "127.0.0.1:50000".parse().unwrap();
        let require = mode.default_require_auth();
        let bypass = mode.default_loopback_auth_bypass();
        assert!(require, "Local mode must require auth by default");
        assert!(!should_bypass_http_auth(peer, bypass, require, Some("tools/call")));
        // The handshake stays open so clients can discover the server.
        assert!(should_bypass_http_auth(peer, bypass, require, Some("initialize")));
        assert!(should_bypass_http_auth(peer, bypass, require, Some("tools/list")));
        // A non-loopback peer never bypasses.
        let remote: SocketAddr = "192.168.1.20:50000".parse().unwrap();
        assert!(!should_bypass_http_auth(remote, bypass, require, Some("initialize")));
    }

    #[test]
    fn mcp_rejects_web_origin_in_local_mode() {
        let mut headers = HeaderMap::new();
        headers.insert(header::ORIGIN, HeaderValue::from_static("https://evil.example"));
        assert!(!is_origin_allowed(McpDeploymentMode::Local, &headers, &[]));
        // CLI clients such as Claude Code send no Origin header and stay allowed.
        assert!(is_origin_allowed(McpDeploymentMode::Local, &HeaderMap::new(), &[]));
        // An origin the owner explicitly allowed passes.
        let allowed = vec!["https://evil.example".to_string()];
        assert!(is_origin_allowed(McpDeploymentMode::Local, &headers, &allowed));
    }
```

- [ ] **Step 3: Run to verify both fail for the right reason**

Run: `cd src-tauri && cargo test mcp_rejects`
Expected: both FAIL. The first fails on "Local mode must require auth by default"; the second fails because `is_origin_allowed` returns true for Local. (These tests are written against the functions read on 2026-09-21; if a signature has moved, adjust the call, not the assertions.)

- [ ] **Step 4: Change the defaults**

In `mcp/mod.rs`:

```rust
    fn default_require_auth(self) -> bool {
        true
    }
```

Leave `default_loopback_auth_bypass` returning true for Local: with `require_auth` true, `should_bypass_http_auth` then exempts only the handshake methods for a loopback peer.

In `is_origin_allowed`, delete the early return for Local so the rest of the function runs:

```rust
    // removed: if matches!(mode, McpDeploymentMode::Local) { return true; }
```

The existing logic already allows a missing `Origin` header, rejects `null`, and otherwise requires the origin to be in the allowlist.

Replace the permissive CORS layer (`mcp/mod.rs:674-677`) with one that follows the allowlist:

```rust
    let origins: Vec<HeaderValue> = allowed_origins
        .iter()
        .filter_map(|origin| HeaderValue::from_str(origin).ok())
        .collect();
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::list(origins))
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);
```

- [ ] **Step 5: Run to verify the new tests pass, then fix the old ones honestly**

Run: `cd src-tauri && cargo test mcp`
Expected: the two new tests pass. Existing MCP tests that call tools without a token will now fail. Fix them by supplying the test token in the request or by constructing `HttpState` with an explicit `require_auth: false` inside the test, never by weakening the production default.

- [ ] **Step 6: Update the connect-your-agent docs**

In `docs/mcp.md`, replace any instruction that omits a token with:

```bash
claude mcp add --transport http fndr <endpoint> --header "Authorization: Bearer $(cat ~/.fndr/mcp_token)"
```

Check the exact `claude mcp add` flags with `claude mcp add --help` before committing the line.

- [ ] **Step 7: Capture the after transcript and write ADR-017**

Repeat Step 1's command. Expected: HTTP 403 with "Forbidden: invalid Origin header". Repeat without the `Origin` header and without a token. Expected: an unauthorized JSON-RPC error. Repeat with the token. Expected: a result. Put all three in `docs/evidence/W02/sec-01.md`. Write ADR-017 recording the old default, the risk (any local process or web page that finds the port could read memory and call `agent.run`), the new default, and the migration (agents need the token header).

- [ ] **Step 8: Verify and commit**

Run: `make test`
Expected: PASS.

```bash
git checkout -b fix/sec-01-mcp-auth-default
git add src-tauri/src/mcp docs/mcp.md docs/decisions/017-mcp-auth-default.md docs/evidence/W02/sec-01.md
git commit -m "fix(mcp): require auth by default in Local mode, enforce Origin, restrict CORS"
```

## Self-review

**Spec coverage (the eight asks):**

| Ask | Covered by |
|---|---|
| 1 Capture pipeline teardown | WS1 plan, tickets CAP-01 to CAP-08, Final epic E-F1 |
| 2 Local model support system and learning | WS2 plan, tickets MOD-02 to MOD-15, LRN-01 to LRN-07, Final epic E-F2 |
| 3 Features and external research | WS3 plan, tickets FEA-01 to FEA-05, section 4 vision |
| 4 GitLab board and team clarity | WS4 plan, tickets OPS-01 to OPS-12, section 7 manifest, vision storyboard FEA-01 |
| 5 Other recommendations | Section 10 |
| 6 Post-capture pipeline teardown and downstream considerations | WS5 plan, tickets MEM-01 to MEM-09, RET-01, RET-02, Final epics E-F9 to E-F15 |
| 7 Jev and fast local models | WS6 plan, tickets DEC-01, DEC-02, Final epics DEC-03 to DEC-08, decision D-8 |
| 8 Ticket detail for the class, GitLab, and switchable agents | Section 7 manifest columns, WS4 sections 4 and 8, OPS-05, OPS-12, `phase_progress.py` |

**Placeholder scan:** dates marked "assumed" are decisions awaiting the owner (D-4), not unfinished text. Hour estimates are estimates. No task step says "TBD" or "add appropriate X".

**Consistency check:** ticket IDs in section 7 match the IDs used in the four workstream plans. Owners in section 5 match the Owner column. Week labels match milestone names in WS4 (`W01-Baseline` to `W12-Submit`).
