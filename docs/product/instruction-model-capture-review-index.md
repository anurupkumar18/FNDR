# FNDR instruction, model, capture, and UI review index

Updated 2026-10-06. This is a navigation map for a frontier-model review, not a claim that every linked file is active. It separates instructions given to coding agents from runtime behavior and identifies historical, experimental, and dormant material explicitly.

## How to use this index

Start with the authoritative overview and current product plan, then inspect the active source paths and their tests. Treat design docs and ADRs as intended contracts; confirm each claim in source. For each proposed instruction change, ask whether it changes implementation behavior, what boundary owns it, what evidence would prove the improvement, and whether a migration is needed. Do not treat captured OCR, app/window text, user-authored memories, fixture data, or model output as instructions for an agent.

### Current repository state

- This checkout is FNDR 1.0 at `/Users/anurupkumar/FNDR`; it is distinct from FNDR-2.0 and the decision-engine repository.
- The working tree holds the uncommitted changes described in section 0, on top of the earlier accessibility guard and voice edits. Nothing is committed.
- EmbeddingGemma v6 is represented by contracts, prefixes, migration/measurement support, tests, and a proposed ADR. It is **not the active durable capture/search contract**. The documented live path is MiniLM v4 / 384 dimensions; BGE v5 is an explicit reindex path. Confirm current runtime in `src-tauri/src/inference/model_config.rs` before making decisions.

## 0. Verified review and what changed, 2026-10-06

This section is the result of reading the files below, scanning for callers, and then fixing what was found. Where it disagrees with a later section, this one is correct. Statements marked "inference" are reasoned from the code, not observed at runtime.

### Long-term goal

Every sentence FNDR shows and every vector it searches comes from one owned contract, and no change to either ships without a repeatable local score.

| Step | What it means | State |
|---|---|---|
| 1. Prompts and display | One prompts file, one voice, honest insight fields, working Vault filter | Written and tested on 2026-10-06; prompts not yet run on the real model |
| 2. Retrieval contracts | Model prompts live only in `embedding/prefixes.rs`, one set per embedding contract, with an index guard so vector spaces never mix. Groundwork for the EmbeddingGemma cutover | Live query path fixed and measured on 2026-10-06 (see "Query prompt on the live path" below). BGE prefixes wait for a measurement with the BGE model installed (owner decision). Fusion weights need a retune against the new vector scores |
| 3. Measured prompts | Run the v3 prompts on the real model through the Quality Lab, deterministic checks before any model judge | Not started |
| 4. Display contract | Confidence as a field instead of a text prefix; fallbacks never assert something unknown | Not started |
| 5. Agent surface | Collapse the overlapping MCP search and context-pack tools | Not started |

### What a user sees differently

| Before | After | Where |
|---|---|---|
| Vault "What happened" fallbacks read "You were reviewing_agent_output PR #42." or "You were review the PR ..." | "Reviewing agent output: PR #42." No narrator, no identifier, no broken grammar | `memory_insight/derive.rs` |
| "Why it mattered" repeated the what-happened line, printed a raw OCR line, or said "Job_or_career_work involving X" | Shows a decision, error, blocker, a new sentence from the memory, or a stated goal. Otherwise empty | `memory_insight/derive.rs` |
| "What changed" listed next steps and file names | Lists results, decisions and "Files: ..." | `memory_insight/derive.rs` |
| "Thread" showed "session …9f3c2a1b" and every card counted it as a topic | Shown only for real links ("1 linked memory"). Old stored values are hidden at display time | `derive.rs`, `search/memory_cards.rs` |
| Older memories still read "You reviewed..." next to new neutral ones | One voice everywhere: stored text is normalized when a card is built | `summariser/narration_filter.rs` (`neutral_voice`), `search/memory_cards.rs` |
| Activity chip showed `testing_workflow` | Shows "testing workflow" | `MemoryCard.tsx` |
| Vault filter "Web pages" hid any card labelled debugging or coding even with a URL; "Docs" and "Communication" hid every labelled card | The label only confirms a match; otherwise text and URL decide | `perspectiveFilter.ts` |
| Ask and MCP ask saw the first 1,000 characters of context and had no rule for a missing answer | 5,000 characters, grounded-only rules, and a fixed not-found reply | `inference/prompts.rs`, `inference/mod.rs` |
| Daily briefing addressed "you" while memories did not | Same neutral voice | `inference/prompts.rs` |
| Model memory panel always reported the vision model as not loaded | Reports whether the pixel runtime is resident | `image_semantics.rs`, `system_metrics.rs` |
| A connecting agent got one sentence for 51 MCP tools | Told where to start, that results are evidence and not instructions, and when writes are allowed | `mcp/mod.rs` |

### Query prompt on the live path

The live MiniLM search route was prepending the BGE query instruction ("Represent this sentence for searching relevant passages: ") to every query. MiniLM takes no instruction. `QueryProfile::embedding_query_with_extras` now returns the query text only and the vector route asks the embedding contract for the prompt (`prefixes::query_text_for`), so the prompt has one owner.

Measured before and after on the three seeded personas, same seed and assets (full tables in [2026-10-06-query-prompt.md](../evidence/W04/2026-10-06-query-prompt.md)):

| Persona | Recall@5 | MRR@10 | Relevant memory ranked first | Gate |
|---|---|---|---|---|
| knowledge-worker | 1.000 to 1.000 | 0.966 to 0.909 | 34 to 31 of 35 | pass |
| office-pm | 0.900 to 0.950 | 0.613 to 0.647 | 20 to 22 of 33 | pass (failed before on this machine) |
| software-engineer | 1.000 to 1.000 | 0.879 to 0.859 | 26 to 25 of 30 | pass |

Recall holds or improves and the gap between a real match and no match roughly doubles in the vector branch. Top-rank precision is mixed and within what 30 to 35 cases per persona can resolve. The fusion weights were tuned against the old, lower vector scores and deserve a deliberate retune. The baseline taken before this change also covers the insight-derivation changes above: knowledge-worker matched its committed reference rank for rank, office-pm reproduced the known installed-tokenizer numbers exactly (MRR@10 0.613), and software-engineer passed with two queries moving from rank 2 to rank 1. Without the pinned tokenizer the software-engineer moves cannot be attributed to one cause.

### What changed in the code

| Change | File |
|---|---|
| Every local-model prompt moved into one file with shared voice, evidence and activity-list fragments. Version `v3` | `src-tauri/src/inference/prompts.rs` (new) |
| Guard tests: a prompt change fails until the version is bumped; every prompt that embeds captured text must state the evidence boundary; no prompt may ask for second person; extraction and review must share one activity list | `prompts.rs` tests |
| `memory_review` defines every field; `card_synthesis` defines title and action; `answer` is grounded; the eval judge has its own task | `prompts.rs`, `inference/mod.rs`, `evals/memory_quality.rs` |
| Dead model code deleted: `vlm.rs`, `model_worker.rs`, `qwen_vl_memory.rs`, three unused engine methods, seven unread constants, and the unused engine slot in `AppState` | `src-tauri/src/inference/`, `lib.rs` and its call sites |
| Insight derivation rewritten as described above, with tests for each case | `memory_insight/derive.rs` |
| MCP server instructions rewritten; a test checks every tool they name exists | `mcp/mod.rs` |
| Agent rules: repo status, real folder names, runtime-prompt rules, portable skills take precedence over plugin skills | `AGENTS.md`, `CLAUDE.md` |
| Catalog rebuilt around the new prompts file | `llm-task-catalog.md` |

The extraction and Screen Guide prompts were moved byte for byte (checked against `HEAD`), so their measured behavior is unchanged.

### What is not verified

- **The v3 prompts have not been run against the real model.** Tests cover structure, parsing and the deterministic paths. The machine was under critical memory pressure during this pass, so the ignored model tests were not run. Run `cargo test --lib extraction_fits_default_token_budget -- --ignored --nocapture` and a day of normal capture, then compare `v3` against earlier trace rows with the script in the catalog.
- **Insight text is part of the embedding document.** New records get shorter, cleaner what/why/changed lines (and no longer embed a session-id fragment), so their embedding text differs from older records. Nothing was re-embedded. The retrieval gate on the three seeded personas passed or reproduced the known tokenizer discrepancy (details under "Query prompt on the live path"); the owner vault was not measured.

### Still open, ranked by user impact

1. **BGE prefixes differ from the model card and from the repo's own bake-off (left as is by owner decision, 2026-10-06).** `embedding/prefixes.rs` uses "Represent this question for searching relevant passages: " for queries and "Represent this sentence: " for documents. BGE v1.5 specifies "Represent this sentence for searching relevant passages: " for queries and no document prefix, which is what `scripts/audit/embedding_bakeoff.py` uses. The Rust prefixes are live in the optional chunk route (`context_runtime/chunk_route.rs`) and the reindex path (`ipc/commands/maintenance.rs`), so measured numbers and shipped behavior do not describe the same thing. The BGE model is not installed on the owner's Mac, so the change could not be measured, and the BGE reindex is incremental (it skips memories that already have BGE rows), so a prefix change also needs a guard that forces a full rebuild. ADR 019 notes the chunk table is empty on real profiles. EmbeddingGemma's prefixes match its model card.
2. **"Low confidence:" is a text prefix, not a field.** `search/memory_cards.rs` prepends it to the summary when grounding is weak, and the UI prints it as part of the sentence. A flag on the card plus a small badge would read better and keep the summary clean.
3. **Over-budget prompts lose their instructions first.** For every task except extraction, `prepare_prompt_tokens` drops tokens from the start, which is the system prompt. Character caps make this unlikely. It has not been observed.
4. **Card fallbacks assert a verb they cannot know.** `build_story_summary` writes "Reviewed <title> updates on <domain>." for any page, including a video.
5. **`refine_query_plan` is test only.** The LLM planner refinement has a prompt and a test and no production caller. Wire it or remove it.
6. **51 MCP tools with overlapping entry points.** The instructions now say where to start, but `search_memories`, `memory.search_raw`, `memory.search_full_context` and `fndr.search` still all exist, as do four context-pack builders. The keep, merge and remove verdicts already exist in [mcp-tool-audit.md](mcp-tool-audit.md) and are waiting on approval (see `CHANGELOG.md`).

### Agent instruction files

- `AGENTS.md` carries the rules. `CLAUDE.md` and the Cursor rule are thin pointers; keep them that way.
- Portable skills take precedence over plugin skills that cover the same situation (owner decision, recorded in `CLAUDE.md`).
- `ALL_SKILLS_COMBINED.md` (1,421 lines) matches the fifteen `SKILL.md` files today, checked block by block. There is no generator or check, so it will drift on the first skill edit.
- Fifteen skills are "always on". Four are rarely the right answer during product work (`setup-portable-engineering-skills`, `write-a-skill`, `triage`, `caveman`) and dilute the routing table.
- `docs/agent.md`, `docs/agent-context-pack.md` and `docs/skills-and-evals.md` were last committed 2026-05-16 and were not verified against current code in this pass.

### Things to try next, with purpose and payoff

| Idea | What it is | Benefit |
|---|---|---|
| Play with a prompt | Edit one prompt in `prompts.rs`, run `cargo test --lib prompts`, paste the printed fingerprint, bump the version | The whole loop is one file and one command |
| Ablation runner on `synthetic_captures` | Run current and candidate prompt over `src-tauri/tests/fixtures/synthetic_captures/` with the real local model and write outputs side by side | See the effect of a prompt change in minutes without touching stored memories |
| Deterministic card checks before any judge | For each output: no leading pronoun, word budget met, every named entity appears in the source, no instruction echo, valid JSON | Cheap, repeatable, and not graded by the model under test. `eval_judge` still uses the same 2B model to grade itself |
| Confidence as a field | Replace the "Low confidence:" text prefix with a card flag and a badge | Cleaner summaries and one place to style uncertainty |
| Path-scoped agent rules | A nested `AGENTS.md` in `src-tauri/src/inference/`, or a Cursor rule with `globs`, holding the prompt-editing rules | Keeps root `AGENTS.md` short; the rules load only when those files are touched |
| Retire or generate the combined skills file | Delete `ALL_SKILLS_COMBINED.md`, or add a script and CI check that builds it | Removes 1,421 duplicated lines or makes drift impossible |
| Collapse MCP entry points | Approve and apply the verdicts in `mcp-tool-audit.md` | Fewer wrong first calls from connecting agents |

## 1. Instructions that govern coding agents

These can shape future edits to the frontend, capture pipeline, and model stack. Review them as process guidance, not as runtime prompts sent to FNDR’s models.

| File | Reach / purpose | Review focus |
|---|---|---|
| [AGENTS.md](../../AGENTS.md) | Repository-wide mandatory defaults, product context, map, verification, privacy rules, and portable-skill routing. | Is it the single source of truth? Does it separate the v1 checkout from other FNDR repos and define accurate verification? |
| [CLAUDE.md](../../CLAUDE.md) | Claude entry point / compatibility instructions. | Ensure it delegates to the same canonical rules and has no stale duplicate guidance. |
| [Cursor rule](../../.cursor/rules/fndr-portable-engineering.mdc) | Cursor-specific portable engineering rules, likely overlapping AGENTS.md. | Check whether Cursor automatically applies this in addition to AGENTS.md; choose one canonical source or enforce sync. |
| [Portable skills README](../../.agent-skills/portable-engineering/README.md), [manifest](../../.agent-skills/portable-engineering/MANIFEST.md), [combined reference](../../.agent-skills/portable-engineering/ALL_SKILLS_COMBINED.md) | Skill discovery, portability, and fallback reference. | Reduce duplicated copies or establish a generation/check so the combined file cannot drift. |
| [Portable skill catalog](../../.agent-skills/portable-engineering/) | Individual `SKILL.md` workflows listed below. | Decide which workflows are genuinely invoked and which overlap enough to merge. |
| [FNDR v2 engineering skill](../v2/skills/fndr-v2-engineering/SKILL.md) and [references](../v2/skills/fndr-v2-engineering/references/) | Detailed cross-agent v2 workflow/invariants. This is not automatically the authority for this v1 checkout. | Mark ownership/version in its entry point; avoid applying v2 architecture to v1 by accident. |
| [GitLab agent instructions](../team/gitlab-agent-instructions.md) | Narrow rules for ticket board and comment mutations. | Keep scoped to board actions; do not let it become a general product or code rulebook. |

Portable engineering skills present in this checkout:

- Engineering: [zoom-out](../../.agent-skills/portable-engineering/engineering/zoom-out/SKILL.md), [grill-with-docs](../../.agent-skills/portable-engineering/engineering/grill-with-docs/SKILL.md), [to-prd](../../.agent-skills/portable-engineering/engineering/to-prd/SKILL.md), [to-issues](../../.agent-skills/portable-engineering/engineering/to-issues/SKILL.md), [tdd](../../.agent-skills/portable-engineering/engineering/tdd/SKILL.md), [diagnose](../../.agent-skills/portable-engineering/engineering/diagnose/SKILL.md), [triage](../../.agent-skills/portable-engineering/engineering/triage/SKILL.md), [prototype](../../.agent-skills/portable-engineering/engineering/prototype/SKILL.md), [improve-codebase-architecture](../../.agent-skills/portable-engineering/engineering/improve-codebase-architecture/SKILL.md), [anti-bloat-review](../../.agent-skills/portable-engineering/engineering/anti-bloat-review/SKILL.md), [setup-portable-engineering-skills](../../.agent-skills/portable-engineering/engineering/setup-portable-engineering-skills/SKILL.md).
- Productivity: [caveman](../../.agent-skills/portable-engineering/productivity/caveman/SKILL.md), [grill-me](../../.agent-skills/portable-engineering/productivity/grill-me/SKILL.md), [handoff](../../.agent-skills/portable-engineering/productivity/handoff/SKILL.md), [write-a-skill](../../.agent-skills/portable-engineering/productivity/write-a-skill/SKILL.md).

Related context and architectural authority:

- [README](../../README.md), [docs index](../README.md), [domain vocabulary](../CONTEXT.md), [architecture](../architecture/ARCHITECTURE.md), [agent context pack](../agent-context-pack.md), [agent overview](../agent.md), [current October plan](../team/2026-10-month-plan.md), [current status](../team/2026-10-05-status-and-next.md).
- These are high-value inputs to model/frontier review because they explain constraints, intended product direction, and current work. They are not all instructions that execute at runtime.

## 2. What writes or shapes user-visible frontend content

The frontend receives text and state from both Rust-side content builders and TypeScript presentation code. Prompt edits alone will not fix raw identifiers, repetitive wording, weak summaries, or visual hierarchy unless the right producer and display boundary are changed.

### 2.1 Main user-facing surfaces and consumers

| Surface | Frontend files | Rust/data producers to trace |
|---|---|---|
| Home greeting, search entry, resume/recent work | [HomeHero.tsx](../../src/app/HomeHero.tsx), [ResumeWork.tsx](../../src/app/ResumeWork.tsx), [AppShell.tsx](../../src/app/AppShell.tsx) | `src-tauri/src/resume/{mod.rs,pack.rs}`, `src-tauri/src/search/memory_cards.rs`, resume/search IPC commands, `src-tauri/src/summariser/display_summary.rs` |
| Search results and confidence/evidence | `src/domains/search/`, `src/domains/ask/` | `src-tauri/src/search/{hybrid.rs,query_processor.rs,reranker.rs,memory_cards.rs}`, `src-tauri/src/summariser/{display_summary.rs,narration_filter.rs}` |
| Memory Vault list/detail/insights/provenance | `src/domains/memory-vault/` (MemoryCard, ExpandedMemoryCard, InsightLayers, grouping and provenance) | `src-tauri/src/memory_review/`, `src-tauri/src/memory_insight/`, `src-tauri/src/search/memory_cards.rs`, memory IPC commands |
| Timeline and activity traces | `src/domains/timeline/`, `src/shared/activity/`, `src/shared/components/ActivityTrace.tsx` | capture/activity event producers, `src-tauri/src/agent/context.rs`, activity-trace policy modules |
| Startup/onboarding and lock/unlock | `src/app/BiometricLockScreen.tsx`, `src/app/App.tsx`, `src/app/auxiliaryAppearance.ts`; Rust auth helper in `src-tauri/src/accessibility/` and auth commands | onboarding/auth status IPC, native helper packaging and appearance config |
| Screen Guide and model status | `src/domains/screen-guide/`, `src/domains/workspace/ModelDownloadBanner.tsx`, `src/domains/workspace/PipelineInspectorPanel.tsx` | `src-tauri/src/inference/`, Screen Guide IPC, model status and capture status commands |

High-value design rules and user contracts:

- [Design direction](DESIGN_DIRECTION.md), [UI overhaul program](UI-UX-OVERHAUL-PROGRAM.md), [Remember spec](fndr-remember-spec.md), [capture quality contract](CAPTURE-QUALITY-CONTRACT.md), [LLM task catalog](llm-task-catalog.md), [Memory Journey](memory-journey.md), [post-capture live path](post-capture-live-path.md), [activity trace boundaries](activity-trace-boundaries.md), [activity traces](activity-traces.md), [voice pipeline](voice-pipeline.md), [Screen Guide](screen-guide.md), [command surface](command-surface.md), [actions policy](actions-policy.md).
- Relevant decision records: [007 insight-first memory and embedding text](../decisions/007-insight-first-memory-and-embedding-text.md), [011 event-driven UI status](../decisions/011-event-driven-ui-status.md), [014 local Screen Guide](../decisions/014-local-screen-guide.md), [020 voice interaction policy](../decisions/020-voice-interaction-policy.md), [021 privacy-safe activity traces](../decisions/021-privacy-safe-activity-traces.md), [022 command bar actions policy](../decisions/022-command-bar-actions-policy.md), [023 five destinations and labs](../decisions/023-five-destinations-and-labs.md).

### 2.2 Model instructions/prompts that can affect displayed text

These are runtime prompt strings or their call sites. Confirm callers and feature flags before treating any as active.

| Source | Output responsibility / review question |
|---|---|
| [inference/prompts.rs](../../src-tauri/src/inference/prompts.rs) | **Every local-model prompt**, with the shared voice, evidence and activity-list fragments and the guard tests. Start here. |
| [inference/mod.rs](../../src-tauri/src/inference/mod.rs) | The engine methods that add evidence to those prompts, cap inputs, parse and validate output. |
| [inference/image_semantics.rs](../../src-tauri/src/inference/image_semantics.rs) | The pixel runtime. Its instruction is `VISION_SYSTEM` in `prompts.rs`; this file parses and grounds the result. |
| [agent/prompts.rs](../../src-tauri/src/agent/prompts.rs), [agent/context.rs](../../src-tauri/src/agent/context.rs), [agent/tools.rs](../../src-tauri/src/agent/tools.rs) | Agent prompt registry, assembled context, and tool descriptions. prompts.rs exposes runtime templates (for example handoff and resume); review groundedness, privacy, and action limits. |
| [memory_review/pipeline.rs](../../src-tauri/src/memory_review/pipeline.rs), `memory_review/{daily.rs,worker.rs,inference_provider.rs}` | Produces deferred/updated memory descriptions and daily review outputs. Review whether generated text is distinct from original capture evidence and whether older records should only be normalized for display. |
| [mcp/mod.rs](../../src-tauri/src/mcp/mod.rs), [mcp/remember.rs](../../src-tauri/src/mcp/remember.rs) | MCP tool/resource/prompt definitions and externally supplied memory writes; user-facing text can be influenced by tool metadata and client-authored records. |

Deterministic text shaping and filtering (not model instructions):

- `src-tauri/src/capture/text_cleanup.rs` cleans OCR/browser chrome and builds deterministic fallback snippets.
- `src-tauri/src/summariser/display_summary.rs` creates display-safe deterministic summaries/fallbacks.
- `src-tauri/src/summariser/narration_filter.rs` detects narrator/instruction-like summaries and falls back; recently edited, inspect working diff before changing.
- `src-tauri/src/search/memory_cards.rs` creates grounded result cards, confidence, labels and fallback text; recently edited.
- `src-tauri/src/memory_insight/{derive.rs,embedding_text.rs,fluff.rs}` derives insight fields and filters weak/repetitive material.
- `src-tauri/src/inference/extraction_evidence.rs` tracks whether extracted claims are supported by their source spans.

## 3. Everything that shapes capture, memory formation, models, and retrieval

This is the main behavior map for “why does it work the way it does?” Read in data-flow order. Most are code contracts rather than prose instructions.

### 3.1 Capture and admission

| Files | Responsibility |
|---|---|
| `src-tauri/src/capture/{mod.rs,macos.rs,permissions.rs,sampling.rs}` | Native capture ownership, macOS screen/window/context reads, permissions, adaptive sampling. |
| `src-tauri/src/capture/{admission.rs,enrich_policy.rs,dedupe.rs}` | Decide whether captured events/frames qualify, when enrichment runs, and duplicate suppression. |
| `src-tauri/src/capture/{text_cleanup.rs,entity_extractor.rs,clipboard.rs}` | OCR cleanup, named entity extraction, clipboard context, and noisy-line/fallback behavior. |
| `src-tauri/src/accessibility/` | Accessibility-based app/window text and native boundary safeguards; recently contains an uncommitted crash guard. |
| `src-tauri/src/privacy/`, `src-tauri/src/security/`, `src-tauri/src/store/` | Privacy exclusions, secret handling, persistence and durable schema. Find the specific submodule for a changed policy; do not infer safety from the capture path alone. |

Contracts and tests: [capture quality](CAPTURE-QUALITY-CONTRACT.md), [post-capture path](post-capture-live-path.md), [ADR 004 no screenshot persistence](../decisions/004-no-screenshot-persistence.md), [ADR 005 capture rate/performance](../decisions/005-capture-rate-and-performance.md), `src-tauri/tests/capture_fixtures.rs`, `low_signal_surface.rs`, `src-tauri/tests/fixtures/screens/`, `src-tauri/tests/fixtures/synthetic_captures/`.

### 3.2 Enrichment, extraction, and memory writing

- Prompt/model orchestration: `src-tauri/src/inference/{prompts.rs,mod.rs,vlm_router.rs,image_semantics.rs,model_config.rs}`.
- Evidence and memory review: `src-tauri/src/inference/extraction_evidence.rs`, `src-tauri/src/memory_review/{pipeline.rs,queue.rs,worker.rs,daily.rs,backfill.rs,inference_provider.rs}`.
- Insight/embedding text formation: `src-tauri/src/memory_insight/{derive.rs,embedding_text.rs,fluff.rs}` and `src-tauri/src/memory_embedding_document.rs`.
- Persisted types and version gates: `src-tauri/src/store/`, `src-tauri/src/config/`, `src-tauri/src/memory_compaction/`.
- Behavioral specs: [LLM task catalog](llm-task-catalog.md), [ADR 007](../decisions/007-insight-first-memory-and-embedding-text.md), [ADR 010 embedding document manifest](../decisions/010-embedding-document-manifest.md), [ADR 012 required model gating](../decisions/012-required-model-gating.md), [ADR 018 reasoning tier](../decisions/018-reasoning-tier.md).

### 3.3 Embedding model contracts and migration

| File | Why it matters |
|---|---|
| [inference/model_config.rs](../../src-tauri/src/inference/model_config.rs) | Source of truth for embedding model IDs, dimensions, schema/table versions, tokenizer/model asset pins and current activation. Inspect before trusting any ADR status. |
| [embedding/onnx.rs](../../src-tauri/src/embedding/onnx.rs) | ONNX runtime, chunk preparation, prefixes, caching, batching/priority, pooling and session lifetime. |
| [embedding/chunking.rs](../../src-tauri/src/embedding/chunking.rs), [embedding/prefixes.rs](../../src-tauri/src/embedding/prefixes.rs), [embedding/admission.rs](../../src-tauri/src/embedding/admission.rs) | Chunk boundaries, model-specific query/document instructions and low-signal admission. |
| [memory_embedding_document.rs](../../src-tauri/src/memory_embedding_document.rs) | Canonical document text shared by capture and other memory writes. Changing it can require re-embedding. |
| `src-tauri/src/store/` embedding schemas/indexes; `src-tauri/src/memory_compaction/` vector merge/pooling | Dimensions and model/version compatibility, compaction and durable retrieval. Mixed vector spaces must not be compared as if equivalent. |
| [ADR 002 dimension versioning](../decisions/002-embedding-dimension.md), [ADR 019 model choice](../decisions/019-embedding-model.md), [ADR 008 parent-child chunk RAG](../decisions/008-parent-child-chunk-rag.md) | Current rationale, benchmark limits, hardware and policy tradeoffs. ADR 019 is marked Proposed; it records Gemma as a candidate, not current activation. |

As of the documented state, v4 MiniLM/384 is active, BGE v5/1024 is explicit-reindex only, and EmbeddingGemma v6 (768 or truncated 256) remains inactive. Review the model terms, measured recall/ranking, long-document cost, concurrent capture RAM, migration/reindex and rollback together. Do not change dimensions in-place in a live index.

### 3.4 Search and retrieval

- Query path: `src-tauri/src/search/{query_processor.rs,hybrid.rs,reranker.rs,memory_cards.rs}`; `src-tauri/src/context_runtime/` for shared retrieval runtime and routes.
- Search-quality contracts: [ADR 003 hybrid ranking](../decisions/003-hybrid-search-ranking.md), [ADR 008 parent-child chunk RAG](../decisions/008-parent-child-chunk-rag.md), [Memory Journey](memory-journey.md), [retrieval baseline evidence](../evidence/W02/retrieval-baseline-seeded.md), [embedding audit](../evidence/W03/embedding-audit.md).
- Tests/fixtures: `src-tauri/tests/{retrieve.rs,retrieval_routes.rs,search_flow.rs,search_relevance_eval.rs,chunk_retrieval_quality.rs,anti_overfitting.rs,embedding_audit.rs,embeddinggemma_reference.rs}`, `src-tauri/tests/fixtures/{search_eval_cases.json,extraction_cases.json,chunking/,sessions/}`.

## 4. Existing review and experimentation harnesses

These are useful ways to play with changes without immediately changing production behavior.

| Harness | Location | Purpose / benefit |
|---|---|---|
| Prompt/task inventory | [LLM task catalog](llm-task-catalog.md), [W01 model call inventory](../evidence/W01/MOD-02-llm-call-inventory.md) | Enumerate model calls, task outputs, requirements, and gaps; extend with prompt version, schema, owner, and golden cases. |
| Retrieval evaluation | `scripts/audit/{retrieval_check.py,embedding_bakeoff.py,embeddinggemma_reference.py}` and `src-tauri/tests/{search_relevance_eval.rs,chunk_retrieval_quality.rs,anti_overfitting.rs}` | Compare models/ranking against labeled queries, calculate retrieval metrics, and keep embedding parity separate from quality. |
| Model asset/bootstrap | `scripts/bootstrap/download-embedding-model.sh`, `download-minilm.sh`, `download-local-llm.sh`, `download-qwen3-vl-4b.sh`; `scripts/download_model.sh` | Reproducible local asset acquisition and model setup; check terms and pinned hashes before distribution. |
| Capture/replay fixtures | `src-tauri/tests/capture_fixtures.rs`, `src-tauri/tests/fixtures/screens/`, `synthetic_captures/`, `sessions/`, `chunking/` | Re-run noisy OCR, UI chrome, capture exclusion, multi-step sessions, and memory formation without real personal capture data. |
| Memory Journey and native QA | [Memory Journey](memory-journey.md), [QA prep](qa-prep.md), [CAP/MEM runbook](cap-05-mem-03-qa-runbook.md), [reopen QA matrix](reopen-qa-matrix.md), [manual QA run](../evidence/W03/manual-qa-2026-10-06.md) | Validate capture-to-memory-to-search-to-reopen and clearly separate automation from human/native evidence. |
| UI component checks | Tests alongside `src/app/`, `src/domains/`, and `src/shared/` (e.g. HomeHero, ResumeWork, Vault card, insight and search tests) | Protect presentation, accessibility, and behavior at the visible boundary. Add fixtures for raw title IDs, repeated pronouns, confidence, and low-evidence content. |
| Scoring and eval tooling | `scripts/audit/`, `scripts/bench/`, `scripts/model/`, `docs/skills-and-evals.md` | Reusable measures for retrieval, vault health, review scoring, and pipeline metrics. Check which are maintained before growing the harness. |

## 5. Consolidate, add, or retire: review candidates

No removal is performed by this index. These are candidates for your frontier-model review and experimentation.

### Consolidate or clarify

1. **One prompt/output contract, not one mega-prompt.** Keep task prompts task-specific, but centralize shared rules for evidence grounding, uncertainty, neutral memory voice, title quality, prohibited instruction leakage, and output schema. Benefit: fewer contradictory prompt copies and more consistent Home/Search/Vault content.
2. **A versioned prompt registry tied to task catalog.** Record task ID, prompt version/hash, model/quantization, schema version, and source evidence. Benefit: reproduce bad cards and compare prompt edits against a fixed corpus. Done in part: all local prompts now live in `inference/prompts.rs` with per-prompt fingerprints. `LLM_PROMPT_VERSION` (`v3`) is still one tag shared by the non-extraction prompts, and the Hermes, operator and MCP instructions are not versioned or traced.
3. **One authoritative “what is active” model matrix.** Align `model_config.rs`, ADR 002/019, model download docs, and the October plan. Benefit: reviewers can tell live model from prototype at a glance; prevents Gemma candidate code from being mistaken for production.
4. **Make v1/v2 instruction boundaries explicit.** Add a short scope header to v2 skill/architecture docs and link to the v1 entry point. Benefit: models stop importing the wrong product and architecture constraints.

### Add small experiments with high value

1. **Golden memory-card dataset and rubric.** Synthetic, privacy-safe examples for garbage titles (`app:title:app`), useful but uncertain matches, repeated person phrasing, instruction-shaped hallucinations, music/video titles, multi-app work, and empty evidence. Score title usefulness, factual support, context, confidence calibration, and actionability. Benefit: optimize quality rather than only absence of failures.
2. **Prompt ablation runner.** Run the same examples through current and candidate prompts/models; save structured outputs and rubric scores. Benefit: test prompt changes without updating persisted memories or reindexing.
3. **End-to-end capture-to-card eval IDs.** Track each synthetic fixture through OCR cleanup, extraction, embedding text, retrieval, and UI serialization. Benefit: localize whether a weak card came from capture, model wording, embedding, retrieval, or rendering.
4. **Memory display normalization experiment.** Since older text should be normalized at display time, compare deterministic display-only transforms with original stored data and retain a provenance/expand-original affordance. Benefit: improve old records without rewriting durable memories or triggering re-embedding.
5. **Whole-app resource benchmark.** Existing model benchmarks do not alone establish concurrent capture/search RAM on the 8 GB Mac. Add a reproducible capture + query + reindex profile with peak RSS, latency, and storage deltas before activating a larger model.

### Retire or consolidate after caller verification

1. **Dormant `MEMORY_SYNTHESIS_PROMPT`**: removed on 2026-10-06 with the rest of the dead model code (see section 0).
2. **Duplicate generated skill references** (`ALL_SKILLS_COMBINED.md` vs individual files): keep generated single-file compatibility only if a generator and drift check exist; otherwise remove the duplicate. Benefit: one editable authority.
3. **Historical `docs/superpowers/` plans/specs and old v2 review artifacts**: archive/index clearly by status rather than delete based on age alone. Benefit: frontier reviews focus on current contracts while preserving design history.
4. **Unreferenced or stale user-facing copy/prompt constants**: use `rg`/call graph and tests before deletion. Benefit: less dead code and fewer instructions that future edits accidentally revive.

## 6. Suggested frontier-review sequence

1. Confirm repository scope and read `README.md`, `docs/CONTEXT.md`, `docs/architecture/ARCHITECTURE.md`, `AGENTS.md`, and the current October plan.
2. Review this index’s Section 1 and decide which agent instruction files are canonical, duplicated, historical, or v2-only.
3. Review Section 3 against active source callers and mark every capture/model/embedding stage as active, optional, experimental, or dormant.
4. Review Section 2 using real UI examples but synthetic or redacted text; do not upload local databases, screenshots with private content, or ignored data.
5. Pick a small prompt/rubric experiment from Section 5 before changing production prompts, persisted memories, embedding dimensions, or model activation.
6. Require evaluation evidence and a rollback/migration plan for model, embedding, capture, and durable-data changes.

## 7. Completeness boundary

“Every file responsible” spans thousands of ordinary implementation files. This index catalogs the instruction entry points and the owning modules/contracts that materially shape frontend output, capture, embeddings, models, and retrieval. It does not list each component stylesheet, every store implementation, generated asset, or every historical design note. Those can be expanded as a second, domain-specific source map after you choose which area to review first.
