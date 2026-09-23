# WS5 Post-Capture Memory Pipeline Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Commit after every step so Claude Code or Codex can resume from git alone (see `docs/team/agent-switch.md`, WS4 Task 6).

**Goal:** Break everything that happens after OCR text exists, until a memory is stored and finalized, into measurable stages, remove the duplicate and dead code that hides which behavior is real, improve embedding, dedup and merge, and local-model output quality with evidence, and state what a finalized memory must give search, recall, ranking, MCP, agents, and computer use.

**Architecture:** The same method as WS1: measure first, cut second, no parallel scoreboard. Extend `telemetry/runtime_metrics.rs` with `mem.*` stage timings and outcome counters, add replayable multi-frame sessions with gold "stories" so merge and dedup can be scored, write down one finalized-memory contract as tests, then optimize the costliest or riskiest stages in measured order. Decisions that are made by thresholds today become candidates for the bounded-decision layer in WS6.

**Tech Stack:** Rust (`src-tauri/src/capture/`, `memory_embedding_document.rs`, `embedding/`, `storage/`, `memory_review/`, `context_runtime/`, `search/`), Python 3 scoring scripts, LanceDB, Make.

**Spec:** `2026-09-21-beta-final-master-plan.md` sections 2, 7. Tickets: MEM-01 to MEM-09, RET-01, RET-02, plus Final epics in section 8. Owner and accountable DRI for every ticket: Anurup. Executors are named per ticket in the manifest.

**Follow-on execution detail:** P12 review scoring is decomposed in
`2026-09-23-s0-p12-execution.md`. It scores only human-reviewed labels and
reports draft fixtures as structural checks, not quality evidence.

## Global Constraints

- Strictly local models. No cloud LLM at runtime.
- Reference machine is Apple M1, 8 GB RAM. All budgets are for it.
- No real captures, database blobs, or tokens in git. Fixtures and replay sessions are synthetic or public content.
- Reuse before adding (`AGENTS.md` anti-bloat gate): extend `runtime_metrics`, `CapturePipelineStats`, `memory_quality.rs`, `agent/audit.rs`; delete code only after its behavior-defining tests are ported.
- No commits to `main`. Branch per ticket, merge request per ticket. No em dashes in code comments, docs, or commit messages.
- Verification: `make test`, plus the ticket's own verify command. Say what you ran.

---

## 1. The live path, and the duplicate paths (verified 2026-09-21)

Method: `scripts/audit/callers.py` (WS5 Task 1) counts non-test call sites per function. Scanning of each file stops at its first `#[cfg(test)]`. Results from this repository:

| Function | Defined in | Class | Where it is called |
|---|---|---|---|
| `compose_memory_embedding_document` | `memory_embedding_document.rs:180` | live | `capture/mod.rs:485, 3275, 4437` |
| `compose_primary_embedding_text` | `capture/mod.rs:1697` | dead outside tests | only tests at `capture/mod.rs:5829, 6022` |
| `distill_memory_from_record` | `memory/distill.rs` | debug-only | `ipc/commands/debug.rs` |
| `decide_memory` | `memory/validate.rs:99` | debug-only | `ipc/commands/debug.rs` |
| `build_embedding_document` | `memory/embed_doc.rs:23` | debug-only | `ipc/commands/debug.rs` |
| `quality_decision_for_record` | `memory/validate.rs:28` | self-only | called only by `memory/validate.rs` |
| `can_merge_into_continuity` | `memory/validate.rs:152` | dead | none |
| `should_queue_graph` | `memory/validate.rs:157` | dead | none |
| `clean_evidence_text` | `memory/evidence.rs:61` | dead | none |
| `best_support_embedding_texts` | `memory_compaction.rs:129` | dead | none |
| `merge_or_append_memory_record` | `capture/mod.rs:3925` | live | called from the loop in the same file |
| `weighted_primary_embedding` | `capture/mod.rs:1807` | live | called from the loop at `3303` and `509` |

Finding: there are **three** embedding-document composers (one live, one test-only, one debug-only) and a typed `memory/` distill, validate, decide pipeline that the capture loop does not use. Each carries passing tests, so it reads as real. This is the first thing to resolve (MEM-01, MEM-02): the live path is `capture/mod.rs` helpers plus `memory_insight::derive_insight_for_record` plus `memory_embedding_document.rs`.

Two caveats a reviewer must remember. First, "self-only" for a function in `capture/mod.rs` does not mean dead, because the capture loop itself lives in that file. Second, deleting `compose_primary_embedding_text` also deletes tests that assert real properties ("structured first", "excludes the raw OCR excerpt"); those assertions must be ported to the live composer before deletion.

## 2. Stage catalog (after OCR text exists)

Line numbers are from `capture/mod.rs` unless a file is named. "Now" is from the file map; anything marked verify is confirmed during MEM-01.

| Stage | Where | Now | Problem or unknown | Metric | Best practice or reference | Candidates | Ticket |
|---|---|---|---|---|---|---|---|
| P0 Evidence normalization | `normalize_evidence_text` 663, `compute_window_title_hash` 674, `lightweight_entities_from_text` 690, `extract_file_references` 828 | Deterministic cleanup and entity and file hints | Quality on fixtures unmeasured | CER, entity recall on fixtures | Deterministic extraction before any model call (cheaper, auditable) | Tests on CAP-03 corpus | MEM-05 |
| P1 Local-model structuring | `capture_pixel_vlm_route` 193, `compose_visual_capture_record` 273, `build_structured_from_browser_semantics` 734, `build_low_ram_semantic_fusion` 889, `apply_semantic_fusion` 1102 | VLM or LLM fills `StructuredMemoryExtraction`; a no-model fallback exists for low RAM | On 8 GB the model cannot always run; fallback share and time to enriched unmeasured | enriched share, time to enriched p95 | Queue with priority and load shedding (v2 `model_worker`) | Scheduling policy (Task 6) | MEM-06, WS2 |
| P2 Validation and grounding | `validate_structured_memory_extraction` 1197, `field_supported_by_evidence` 1146, `strip_unsupported_values` 1164, `memory_quality.rs` | Drops unsupported fields; gate on stacked issues | The 2026-05-17 bug silently dropped every frame | grounding rate on gold set; unexplained drops = 0 | Grounding checks plus counters that make silent failure loud | Gold set (MOD-04) | MEM-07 |
| P3 Context composition | `build_grounded_memory_context` 1640, `build_continuation_footer` 1361, `pad_with_structured` 1387, `memory_insight/derive.rs`, `memory_insight/fluff.rs` | Builds the readable memory text and insight fields | Fluff and meta narration filtered by rules; leakage unmeasured | meta-narration rate on gold set | Filter at write time and at display (memory_insight/fluff.rs, MemoryCard.tsx) | Extend banned patterns with tests | MEM-07 |
| P4 Embedding document | `memory_embedding_document.rs:180`, ADR 010 | One document with roles, vector spaces, provenance manifest | Three composers exist (section 1) | one live composer, zero dead | One contract, one function | Delete or adopt | MEM-01, MEM-02 |
| P5 Chunking | `embedding/chunking.rs` (917 lines), ADR 008 | Overlapping chunks under a parent record | Chunk count, size, duplicate share unmeasured | chunks per memory, mean tokens, duplicate share | Parent-child RAG; dedupe near-identical chunks | Audit and two fixes | MEM-05 |
| P6 Embedding generation | `embedding/onnx.rs` (1,264 lines), `embed_text_inputs_with_memo` 3846, `embedder_gate_action` 4835 | MiniLM 384-d live; BGE-large 1024-d for chunks; memoized batches; zero-vector gate | Two contracts; BGE-large is heavy for 8 GB; never ablated | Recall@5, MRR@10, ms per chunk, RAM | Qwen3-Embedding-0.6B (v2's pick), static Model2Vec baseline | Ablation | MEM-09 |
| P7 Vector combination | `weighted_primary_embedding` 1807 | Weighted mix of primary, snippet, support vectors | Weights are fixed by hand | Recall@5 by weight setting | Ablate against labeled questions | Weight sweep | MEM-09, RET-01 |
| P8 Similar-memory search for merge | `best_batch_merge_target` 4137, `best_persisted_merge_target` 4189, lexical variants 4161 and 4256, `score_search_candidate` 4906, `passes_lexical_merge_threshold` 5053, cross-app rules 5074 to 5108, `continuity_anchor` 5161, session keys 5481 and 5505 | Thresholded cosine and lexical scores plus rules | Precision and recall of merge unknown; many thresholds | pairwise precision, recall, false merges | Pairwise clustering metrics; selective classification with abstain | Bounded decision pilot | MEM-04, DEC-03 |
| P9 Merge write | `merge_or_append_memory_record` 3925, `merge_string_lists` 4791, `choose_story_title` 4849, `merge_story_text` 4865, re-embed at 4437 to 4527 | Merges fields, recomputes embeddings | Cost of re-embedding on every merge unmeasured | `mem.merge_write_ms`, outcome mix | Merge only fields that changed | Counters first | MEM-03, MEM-04 |
| P10 Persistence | `storage/lance_store/`, `memory_compaction.rs`, `memory_quality.rs` | Batched Lance writes to parent and chunk tables | v2's audit found no index on any table; version and fragment growth unknown | rows per day, MB per day, query ms, versions | Scalar, FTS, vector indexes; compaction schedule (v2 T-203, T-204, spike T-208) | Index and compact | MEM-08 |
| P11 Side effects | `graph.auto_link_to_task` 3790, `maybe_create_tasks_from_memory` 5360, `graph/` (3,725 lines) | Graph links and task extraction after write | Contend with capture; quality unmeasured | queue depth, task precision | Async, pressure-gated | Later | Final epic |
| P12 Post-capture review | `memory_review/` (worker, pipeline 1,021 lines, daily 721, backfill) | Async local-model review with validated patch, lifecycle statuses | Regression risk of rewrites; unmeasured | patch acceptance rate, `review_failed` share | Judge with gold set; keep original on failure | Score review outputs | Final epic, DEC-05 |
| P13 Finalization | `memory_quality::classify_storage_outcome`, lifecycle chips (MemoryCard.tsx) | A record ends as developed, pending, raw, review_failed, or visual_failed | Rules scattered; no single contract | invariant tests pass | One contract as tests | MEM-07 | MEM-07 |

## 3. What happens from there: consumers of a finalized memory

| Consumer | Needs from the memory | Existing code | Consideration or gap | Ticket |
|---|---|---|---|---|
| Search | Clean text for keyword search, a live-contract vector, provenance for surfacing reasons | `search/hybrid.rs` (2,520 lines), `search/memory_cards.rs`, `SearchConfig` weights | Fusion weights and relevance floors are hand-set | RET-01 |
| Recall and RAG | Chunk vectors tied to a parent id, time, entity, and graph links | `context_runtime/` (16 files, 7,419 lines): `query_plan.rs` (`PlannerIntent`, `Route`), routes (keyword, vector, chunk, entity, temporal, graph), `fusion.rs` (`fuse`, `SurfacingReason`), `verifier.rs`, `composer.rs` | `refine_plan_with_llm` (`query_plan.rs:195`) calls the LLM per query; a bounded decision could replace it | DEC-01, DEC-06 |
| Ranking | Signals: recency, score, anchor coverage, feedback | `search/reranker.rs` (anchor coverage), `embedding_retrieval_adjustment` (`memory_embedding_document.rs:374`), fusion signals | No ablation; feedback exists but is not used for ranking | RET-01, Final epic |
| Feedback | Ratings tied to a retrieval | `agent/audit.rs`: `RetrievalFeedbackRating` (Useful, Irrelevant, Wrong, Stale, MissingContext), `append_feedback`, MCP `agent.rate_result` | WS2's preference-pair pipeline should read this existing store, not create a second one | WS2 Task 8 |
| MCP | Stable ids, citations, token-budgeted packs, raw text gated | `mcp/mod.rs`: 51 tools (from `memory.search_full_context` to `fndr.open_target`) | Overlapping names, mixed read and execute powers, auth default (SEC-01) | RET-02, SEC-01 |
| Agentic use | Read-only context for agents; audited write-back | `agent/`: `AgentMode` (Ask, Plan, Act, Learn), `PermissionScope`, `RiskLevel`, `policy_for_action`, approvals by action status, `audit.rs`, `execution.rs` with a command allowlist | Captured screen text is untrusted: a web page can contain instructions. Context packs must wrap evidence as data | FEA-02, WS3 |
| Computer use | A memory to target, and a safe way to reach it | `memory/reopen.rs` (`ReopenTarget`, `ReopenKind`, `build_reopen_target`), `accessibility/mod.rs` (AXUIElement bindings, text injection), `ipc/commands/autofill.rs` (enigo, osascript, pbcopy paste), MCP `fndr.open_target` | `policy_for_action` currently allows `OpenUrl` and `OpenFile` in Act mode without approval, which conflicts with WS3's one-approval-per-action rule | WS3 E-F4 |

Design consequences to carry into every ticket here:

1. **A memory is data, never instructions.** Anything that reaches an agent (context pack, MCP result, Screen Guide answer) carries its evidence inside delimiters and a label saying it came from screen content. Add an injection fixture to the WS3 agent tests.
2. **Every downstream claim needs a memory id.** Citations depend on stable ids across merges (MEM-07 invariant 8).
3. **Ranking improvements are only real if they move `make eval` retrieval rows** (RET-01).

## 4. The finalized-memory contract (MEM-07)

A stored memory satisfies all of these. Each becomes one test named `memory_contract_<n>_<name>`, reusing existing helpers where they exist.

| # | Invariant | Reuse |
|---|---|---|
| 1 | Provenance: app, window title, timestamp, session key, and an evidence hash; no raw screenshot bytes | `raw_screenshot_stored` default false (ADR 004) |
| 2 | A non-zero primary vector of the live contract dimension and an `embedding_manifest` | `embedder_gate_action`, `memory_embedding_document` manifest |
| 3 | A lifecycle state from `""`, `pending`, `reviewed_local`, `reviewed_daily`, `review_failed`; a `visual_semantics_failed` record never carries enriched fields | `memory_quality::is_visual_semantics_failed_record` |
| 4 | Every non-empty structured field is supported by the evidence or cleared | `field_supported_by_evidence` |
| 5 | No meta narration in summary or display fields | banned patterns in `memory_review` and `MemoryCard.tsx` |
| 6 | `activity_type` is one of `CANONICAL_ACTIVITY_TYPES` | `normalize_activity_type` |
| 7 | URL fields carry no credentials in the query string (add the check if missing) | new |
| 8 | Merging keeps the union of source ids, so every earlier citation still resolves | new |
| 9 | Reprocessing the same frame twice does not create a second memory | `deterministic_dedup_fingerprint` |
| 10 | Deleting by id, time range, or domain removes the memory, its chunks, its vectors, and its graph nodes | verify in v1; port v2 T-206 tests if missing |

## 5. Tasks

### Task 1: Trace the live path (MEM-01)

**Files:**
- Create: `scripts/audit/callers.py`, `scripts/audit/test_callers.py` (copied from the tested assets)
- Create: `docs/product/post-capture-live-path.md`

**Interfaces:**
- Produces: `callers.py <src root> <symbol>...` printing a markdown table with class (`live`, `debug-only`, `self-only`, `dead`), call count, and top callers. `scan(root, symbol, debug_marker)` and `classify(def_files, calls, debug_marker)`.

The script and its 6 tests were written test-first and run before this plan was written.

- [ ] **Step 1: Copy the tested files**

```bash
mkdir -p scripts/audit
cp docs/superpowers/plans/assets/2026-09-21/ws5/callers.py docs/superpowers/plans/assets/2026-09-21/ws5/test_callers.py scripts/audit/
```

- [ ] **Step 2: Run the tests**

Run: `python3 scripts/audit/test_callers.py`
Expected: `Ran 6 tests ... OK`.

- [ ] **Step 3: Run it on the post-capture functions**

```bash
python3 scripts/audit/callers.py src-tauri/src \
  compose_memory_embedding_document compose_primary_embedding_text distill_memory_from_record decide_memory \
  build_embedding_document quality_decision_for_record can_merge_into_continuity should_queue_graph \
  clean_evidence_text best_support_embedding_texts best_embedding_text merge_or_append_memory_record \
  weighted_primary_embedding validate_structured_memory_extraction build_grounded_memory_context \
  embed_text_inputs_with_memo classify_storage_outcome deterministic_dedup_fingerprint
```

Expected: a table whose rows for the first eleven symbols match section 1. If a row differs, the code moved; trust the run and update section 1.

- [ ] **Step 4: Hand-check every `self-only` and `live` row in `capture/mod.rs`**

Open the caller line and confirm it is reached from `run_capture_loop` (line 1872) or a function it calls. A caller inside `#[cfg(test)]` is excluded automatically; a caller inside a function that nothing calls is not, so read the caller.

- [ ] **Step 5: Write `docs/product/post-capture-live-path.md`**

```markdown
# Post-capture live path

| Stage | Live function (file:line) | Duplicate or dead alternatives | Verdict |
|---|---|---|---|
| Embedding document | compose_memory_embedding_document (memory_embedding_document.rs:180) | compose_primary_embedding_text (capture/mod.rs:1697, tests only); memory::embed_doc::build_embedding_document (debug only) | keep the first, delete or adopt the others (MEM-02) |
```

Add one row for each of stages P0 to P13 in section 2, naming the single live function and what to do about any alternative.

- [ ] **Step 6: Commit**

```bash
git checkout -b docs/mem-01-live-path
git add scripts/audit docs/product/post-capture-live-path.md
git commit -m "docs(audit): trace the live post-capture path and flag duplicate code"
```

### Task 2: Delete or adopt the duplicate code (MEM-02)

**Files:**
- Modify: `src-tauri/src/capture/mod.rs` (remove `compose_primary_embedding_text` at line 1697 and its two tests)
- Modify: `src-tauri/src/memory_embedding_document.rs` (receive the ported assertions as tests)
- Modify or delete: `src-tauri/src/memory/{distill,validate,embed_doc,evidence}.rs`, `src-tauri/src/ipc/commands/debug.rs` (the only caller)

**Interfaces:**
- Consumes: the verdicts in `docs/product/post-capture-live-path.md`.
- Produces: one embedding-document composer and no function that only tests exercise. No change in stored memories.

- [ ] **Step 1: Capture the baseline**

Run: `make test 2>&1 | tail -20` and record the pass counts for Rust and Vitest in the merge request.

- [ ] **Step 2: Port the behavior tests before deleting the function**

The two tests that call `compose_primary_embedding_text` are `fused_primary_embedding_text_excludes_raw_ocr_excerpt` (`capture/mod.rs:5809`) and `primary_embedding_text_is_structured_first_with_capped_evidence` (`capture/mod.rs:6011`). Rewrite each to build a record, call `compose_memory_embedding_document(&record, Some(&chunking_config))`, and assert the same property on `primary_text`. Run them against the live composer.

Run: `cd src-tauri && cargo test primary_text`
Expected: PASS. If a property does not hold for the live composer, that is a real finding (the live path does not guarantee it): open a ticket instead of deleting the test.

- [ ] **Step 2b: PR 1, delete the test-only composer**

Delete `compose_primary_embedding_text` and its two old tests. Run `make test`.
Expected: PASS, with a test count that is exactly two lower and two higher (ported), so equal overall.

```bash
git checkout -b refactor/mem-02a-remove-test-only-composer
git add src-tauri
git commit -m "refactor(capture): remove the test-only embedding composer, keep its property tests on the live one"
```

- [ ] **Step 3: PR 2, decide the `memory/` typed path**

Read `memory/validate.rs` (`quality_decision_for_record`, `decide_memory`) beside the live rules (`validate_structured_memory_extraction`, `memory_quality::classify_storage_outcome`). If the typed path expresses rules the live path lacks, list them as invariants for MEM-07 and delete the module anyway. If it is strictly weaker, delete it. Either way remove the debug command that called it. Record the decision in `docs/decisions/018-single-post-capture-path.md`.

- [ ] **Step 4: Verify nothing else changed**

Run: `git grep -n "compose_primary_embedding_text\|distill_memory_from_record\|build_embedding_document" src-tauri`
Expected: no hits. Run `make test` and compare with Step 1.

- [ ] **Step 5: Commit**

```bash
git checkout -b refactor/mem-02b-remove-debug-only-memory-path
git add src-tauri docs/decisions/018-single-post-capture-path.md
git commit -m "refactor(memory): remove the debug-only typed memory path (ADR-018)"
```

### Task 3: Post-capture stage timings and outcome counters (MEM-03)

**Files:**
- Modify: `src-tauri/src/capture/mod.rs` (stage boundaries and the merge decision)
- Test: existing `runtime_metrics` tests plus one counter test

**Interfaces:**
- Consumes: `runtime_metrics::since_ms(op, started)` and percentiles from WS1 Task 1; `runtime_metrics::bump(counter: &'static str)`.
- Produces: op names `mem.structure_ms`, `mem.validate_ms`, `mem.compose_ms`, `mem.embed_ms`, `mem.merge_search_ms`, `mem.merge_write_ms` and counters `mem.outcome.new`, `mem.outcome.merged_persisted`, `mem.outcome.merged_batch`. The WS1 report script already prints `mem.` stage rows and a "Memory outcomes" table.

- [ ] **Step 1: Locate the boundaries**

```bash
grep -n "validate_structured_memory_extraction(\|compose_memory_embedding_document(\|embed_text_inputs_with_memo(\|best_batch_merge_target(\|best_persisted_merge_target(\|merge_or_append_memory_record(" src-tauri/src/capture/mod.rs
```

Read each hit in context. The structuring call is the VLM or LLM route (`capture_pixel_vlm_route` at line 193 and the text path in the loop).

- [ ] **Step 2: Wrap each boundary with the two-line pattern from WS1 Task 1 Step 6**

```rust
let stage_started = std::time::Instant::now();
let validated = /* the existing call, unchanged */;
runtime_metrics::since_ms("mem.validate_ms", stage_started);
```

Use the op names listed above.

- [ ] **Step 3: Count outcomes where the merge is decided**

Inside `merge_or_append_memory_record` (line 3925), at each return that decides the outcome, add one of:

```rust
crate::telemetry::runtime_metrics::bump("mem.outcome.new");
crate::telemetry::runtime_metrics::bump("mem.outcome.merged_persisted");
crate::telemetry::runtime_metrics::bump("mem.outcome.merged_batch");
```

- [ ] **Step 4: Write and run a counter test**

Add a test beside the existing merge tests (`merge_preserves_v2_metadata_when_incoming_is_sparse`, `capture/mod.rs:5859`) that merges a sparse record into an existing one and asserts the `mem.outcome.merged_persisted` counter is one higher after the call. `snapshot_inner` is private to `runtime_metrics.rs`, so first add this accessor next to `bump` (test-driven: the test below fails to compile without it):

```rust
pub fn counter(name: &str) -> u64 {
    global().inner.lock().counters.get(name).copied().unwrap_or(0)
}
```

Because the counter is global, compare the value before and after the call rather than to zero.

Run: `cd src-tauri && cargo test merge_ && cargo test runtime_metrics`
Expected: PASS.

- [ ] **Step 5: Verify in the report**

Run a 30-minute release-build session with `FNDR_METRICS_DUMP` set (WS1 Task 2 Step 10), then `make capture-baseline`.
Expected: the report shows `mem.*` rows and a Memory outcomes table with `new` and a merged count.

- [ ] **Step 6: Commit**

```bash
git checkout -b feat/mem-03-post-capture-timings
git add src-tauri
git commit -m "feat(telemetry): time post-capture stages and count memory outcomes"
```

### Task 4: Replay sessions and the merge and dedup baseline (MEM-04)

**Files:**
- Create: `scripts/model/score_merge.py`, `scripts/model/test_score_merge.py` (tested assets)
- Create: `src-tauri/tests/fixtures/sessions/*.json` (6 sessions)
- Create: `src-tauri/tests/merge_replay.rs`
- Create (by running): `docs/evidence/W03/merge-baseline.md`

**Interfaces:**
- Consumes: `score_merge.score(frames) -> dict` with keys `frames, stories, memories, precision, recall, false_merges, fragmentation`; input lines `{"frame_id", "story_id", "memory_id"}`.
- Produces: the baseline that any merge-threshold change (and the DEC-03 pilot) must beat.

The scorer and its 6 tests were run before this plan was written. Example: two frames of one story stored in two memories give recall 1/3 with three frames, and a false merge lowers precision to 0.

- [ ] **Step 1: Copy the tested scorer**

```bash
cp docs/superpowers/plans/assets/2026-09-21/ws5/score_merge.py docs/superpowers/plans/assets/2026-09-21/ws5/test_score_merge.py scripts/model/
python3 scripts/model/test_score_merge.py
```

Expected: `Ran 6 tests ... OK`.

- [ ] **Step 2: Author six sessions of eight synthetic frames**

Each file is `{"session_id": "...", "frames": [{"frame_id", "story_id", "app", "window_title", "url", "evidence"}]}`. A story is "the same piece of work, so a person would expect one memory". Content is synthetic or public.

| Session | Construction | Expected stories |
|---|---|---|
| `coding_cross_app` | 8 frames alternating editor and terminal on one repo within 40 minutes | 1 |
| `two_articles_alternating` | article A, B, A, B, A, B, A, B in one browser | 2 |
| `revisit_same_day` | article A at frame 1 to 3, other work 4 to 6, article A again 7 to 8 | 2 (A merged once it returns, other work separate) |
| `chat_thread_growth` | one chat thread where each frame adds two lines | 1 |
| `terminal_test_loop` | `cargo test` output repeated with different pass counts | 1 |
| `similar_but_different` | two documents that share boilerplate but cover different topics | 2 |

- [ ] **Step 3: Write the replay test**

`src-tauri/tests/merge_replay.rs` follows the setup in `capture/mod.rs` tests (`merge_test_record` at line 5591, the store setup at 5859): build a record per frame, call the merge decision in order against a fresh temporary store, and write one line per frame to `.eval-tmp/merge-<session>.jsonl` with the memory id the frame ended in. It asserts only that every frame maps to some memory; the score is computed by the script.

Run: `cd src-tauri && cargo test --test merge_replay`
Expected: PASS and six output files.

- [ ] **Step 4: Score and record the baseline**

```bash
cat .eval-tmp/merge-*.jsonl > .eval-tmp/merge-all.jsonl
python3 scripts/model/score_merge.py .eval-tmp/merge-all.jsonl --name "baseline thresholds" --out docs/evidence/W03/merge-baseline.md
```

Expected: a report with pairwise precision, recall, F1, false merges, and fragmentation. Read the false merges first: they lose information and cannot be undone.

- [ ] **Step 5: Commit**

```bash
git checkout -b test/mem-04-merge-replay
git add scripts/model src-tauri/tests docs/evidence/W03
git commit -m "test(merge): replay sessions with gold stories and a merge and dedup baseline"
```

### Task 5: Embedding text and chunk audit (MEM-05)

**Files:**
- Create: `src-tauri/tests/embedding_audit.rs`
- Create (by running): `docs/evidence/W03/embedding-audit.md`

**Interfaces:**
- Consumes: `compose_memory_embedding_document(&record, Some(&chunking_config))` and the CAP-03 fixtures and MEM-04 sessions as input.
- Produces: per-input statistics and at most two targeted fixes.

- [ ] **Step 1: Write the audit test**

For every fixture screen and session frame build a record, compose the embedding document, and print: number of chunks, mean and maximum tokens per chunk (four characters per token), share of chunks whose normalized text equals another chunk's, and share of lines matching known noise (reuse the patterns in `text_cleanup.rs`). Write results as a markdown table to stdout and to the evidence file.

Run: `cd src-tauri && cargo test --test embedding_audit -- --nocapture`
Expected: a table for 30 screens and 48 frames.

- [ ] **Step 2: Decide the fixes from the numbers**

Fix only what the table shows: for example a duplicate share above 10 percent gets chunk deduplication in `embedding/chunking.rs`; a mean chunk size far from the embedder's sweet spot gets a window change in `ChunkingConfig`. Each fix is a separate commit with a test that fails before it.

- [ ] **Step 3: Re-run and record before and after**

Save the before and after tables in `docs/evidence/W03/embedding-audit.md`. Run `make eval` before and after and record Recall@5 so a "cleaner" chunking that hurts retrieval is caught.

- [ ] **Step 4: Commit**

```bash
git checkout -b feat/mem-05-embedding-audit
git add src-tauri docs/evidence/W03
git commit -m "test(embedding): audit chunking and embedding text on fixtures and sessions"
```

### Task 6: Enrichment scheduling (MEM-06)

**Files:**
- Create: `src-tauri/src/capture/enrich_policy.rs`
- Modify: `src-tauri/src/capture/mod.rs` (call the policy where the VLM or LLM route is chosen; count outcomes)

**Interfaces:**
- Consumes: `telemetry::system_metrics::pressure_recommends_skipping_heavy_models() -> (bool, &'static str)`, queue depth of the review queue, battery state.
- Produces: `pub enum EnrichNow { Yes, Defer(&'static str), Skip(&'static str) }`, `pub struct EnrichInputs { host_memory_high, on_battery, queue_depth, text_chars, is_duplicate_story }`, `pub fn should_enrich_now(i: &EnrichInputs, max_queue: usize) -> EnrichNow`, and counters `mem.enrich.yes`, `mem.enrich.defer.<reason>`, `mem.enrich.skip.<reason>`. The WS6 pilot DEC-04 may later replace the body with a calibrated decision while keeping this signature.

- [ ] **Step 1: Write the failing tests in `enrich_policy.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> EnrichInputs {
        EnrichInputs { host_memory_high: false, on_battery: false, queue_depth: 0, text_chars: 500, is_duplicate_story: false }
    }

    #[test]
    fn healthy_valuable_frame_is_enriched() {
        assert_eq!(should_enrich_now(&base(), 4), EnrichNow::Yes);
    }

    #[test]
    fn memory_pressure_defers_even_a_duplicate() {
        let i = EnrichInputs { host_memory_high: true, is_duplicate_story: true, ..base() };
        assert_eq!(should_enrich_now(&i, 4), EnrichNow::Defer("host_memory_high"));
    }

    #[test]
    fn duplicates_and_thin_frames_are_skipped_not_deferred() {
        assert_eq!(should_enrich_now(&EnrichInputs { is_duplicate_story: true, ..base() }, 4), EnrichNow::Skip("duplicate_story"));
        assert_eq!(should_enrich_now(&EnrichInputs { text_chars: 39, ..base() }, 4), EnrichNow::Skip("too_little_text"));
        assert_eq!(should_enrich_now(&EnrichInputs { text_chars: 40, ..base() }, 4), EnrichNow::Yes);
    }

    #[test]
    fn battery_with_a_backlog_and_a_full_queue_both_defer() {
        assert_eq!(should_enrich_now(&EnrichInputs { on_battery: true, queue_depth: 1, ..base() }, 4), EnrichNow::Defer("battery_backlog"));
        assert_eq!(should_enrich_now(&EnrichInputs { on_battery: true, queue_depth: 0, ..base() }, 4), EnrichNow::Yes);
        assert_eq!(should_enrich_now(&EnrichInputs { queue_depth: 4, ..base() }, 4), EnrichNow::Defer("queue_full"));
        assert_eq!(should_enrich_now(&EnrichInputs { queue_depth: 3, ..base() }, 4), EnrichNow::Yes);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Declare `pub mod enrich_policy;` in `capture/mod.rs`, then run: `cd src-tauri && cargo test enrich_policy`
Expected: compile FAIL, "cannot find type `EnrichInputs`".

- [ ] **Step 3: Implement above the tests**

```rust
/// What to do with the local model for one captured frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnrichNow {
    Yes,
    /// Keep the frame in the queue and try later.
    Defer(&'static str),
    /// Do not spend model time on this frame; store the deterministic fallback.
    Skip(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub struct EnrichInputs {
    pub host_memory_high: bool,
    pub on_battery: bool,
    pub queue_depth: usize,
    pub text_chars: usize,
    pub is_duplicate_story: bool,
}

pub const MIN_TEXT_CHARS: usize = 40;

/// Order matters: system health first, then value, then backlog.
pub fn should_enrich_now(i: &EnrichInputs, max_queue: usize) -> EnrichNow {
    if i.host_memory_high {
        return EnrichNow::Defer("host_memory_high");
    }
    if i.is_duplicate_story {
        return EnrichNow::Skip("duplicate_story");
    }
    if i.text_chars < MIN_TEXT_CHARS {
        return EnrichNow::Skip("too_little_text");
    }
    if i.on_battery && i.queue_depth > 0 {
        return EnrichNow::Defer("battery_backlog");
    }
    if i.queue_depth >= max_queue {
        return EnrichNow::Defer("queue_full");
    }
    EnrichNow::Yes
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd src-tauri && cargo test enrich_policy`
Expected: PASS (4 tests; the same logic and tests ran in an isolated crate before this plan was written).

- [ ] **Step 5: Wire it in and count outcomes**

Where the loop currently decides to call the VLM or LLM (near `capture_pixel_vlm_route`, line 193), build `EnrichInputs` from `pressure_recommends_skipping_heavy_models()`, the review queue depth, and the frame, call `should_enrich_now`, bump the matching `mem.enrich.*` counter, and on `Defer` or `Skip` take the existing low-RAM fallback path (`build_low_ram_semantic_fusion`, line 889) instead of failing.

- [ ] **Step 6: Measure**

Two-hour release-build run. Record enriched share, fallback share by reason, and time to enriched p50 and p95 (from the review queue timestamps) in `docs/evidence/W04/enrichment-scheduling.md`. Compare quality of fallback versus enriched records with the WS2 gold set (`make eval`) so "deferred" does not silently mean "worse".

- [ ] **Step 7: Commit**

```bash
git checkout -b feat/mem-06-enrichment-scheduling
git add src-tauri docs/evidence/W04
git commit -m "feat(capture): explicit enrichment scheduling policy with counters"
```

### Task 7: Finalization contract tests (MEM-07)

**Files:**
- Create: `src-tauri/tests/memory_contract.rs`
- Modify: helper reuse from `memory_quality.rs` (no new helper unless an invariant has none)

**Interfaces:**
- Consumes: the ten invariants in section 4.
- Produces: `assert_memory_contract(&MemoryRecord)` used by the replay and audit tests, and one test per invariant named `memory_contract_<n>_<name>`.

- [ ] **Step 1: Write one failing test per invariant, starting with the ones that already have helpers (3, 4, 6, 9)**

Each test builds a valid record and a violating record and asserts the contract helper accepts the first and rejects the second with a message naming the invariant. Invariants 7, 8, and 10 have no existing helper: their tests are written first and fail, then the checks are added.

- [ ] **Step 2: Run to verify the right ones fail**

Run: `cd src-tauri && cargo test --test memory_contract`
Expected: FAIL for 7, 8, and 10 until implemented; the others pass if the live rules already hold. A passing test that was expected to fail means the rule already existed; note it.

- [ ] **Step 3: Implement the missing checks**

Invariant 7: strip or reject query-string credentials in stored URLs. Invariant 8: on merge, keep the union of source ids. Invariant 10: if deletion by id, time range, or domain leaves chunks or graph nodes behind, fix the deletion path or file a bug with the failing test attached.

- [ ] **Step 4: Run the contract over the fixtures**

Call `assert_memory_contract` on every record produced by `merge_replay` and `embedding_audit`. Expected: zero violations; each violation becomes a bug ticket.

- [ ] **Step 5: Verify and commit**

Run: `cd src-tauri && cargo test memory_contract && cd .. && make test`

```bash
git checkout -b test/mem-07-memory-contract
git add src-tauri
git commit -m "test(memory): finalized-memory contract as executable invariants"
```

### Task 8: Storage indexes and compaction (MEM-08)

**Files:**
- Modify: `src-tauri/src/storage/lance_store/` (index creation, compaction job)
- Create: `src-tauri/tests/storage_scale.rs` (100,000-row fixture)
- Create (by running): `docs/evidence/W04/storage-indexes.md`

Read first: `~/FNDR-2.0/docs/spikes/T-208-lance-findings.md` (measured BM25, prefilter, hybrid, and index maintenance behavior of the same LanceDB family on 100,000 rows). Do not copy code; take the measurements as the hypothesis to check.

- [ ] **Step 1: Measure the current state**

In `storage_scale.rs`, insert 100,000 synthetic rows into a temporary table using the real schema and record: time for a vector query, a keyword query, and a point lookup by id; number of dataset versions; number of fragments; bytes on disk. Print a table.

Run: `cd src-tauri && cargo test --release --test storage_scale -- --nocapture`
Expected: a "before" table.

- [ ] **Step 2: Add indexes one at a time, measuring after each**

A scalar index on the id and timestamp columns, a full-text index on the keyword text column, and a vector index on the live vector column, in that order. Keep each only if it helps by the margin you set beforehand (at least 3 times faster for its query class), and note the RAM and build cost.

- [ ] **Step 3: Add compaction and version pruning on a schedule**

A background job that compacts small fragments and prunes old versions, gated by the same pressure check as the review worker. Test: a simulated day of batched writes keeps versions and fragments under bounds you state in the test.

- [ ] **Step 4: Verify and commit**

Run: `cd src-tauri && cargo test storage_scale --release && cd .. && make test`

```bash
git checkout -b feat/mem-08-storage-indexes
git add src-tauri docs/evidence/W04
git commit -m "feat(storage): indexes and compaction for the Lance tables, measured on 100k rows"
```

### Task 9: Embedding ablation (MEM-09)

**Files:**
- Create (by running): `docs/evidence/W04/embedding-ablation.md`, `docs/decisions/019-embedding-contract.md`

**Interfaces:**
- Consumes: `make eval` retrieval rows and the WS2 scorer (`score.py --json-out`).

Candidates: MiniLM 384-d (current live), BGE-large 1024-d (present as quantized ONNX, `BGE_V5_MODEL_FILENAME`), Qwen3-Embedding-0.6B (v2's pick; GGUF through llama.cpp), and a static Model2Vec-style baseline as a speed floor. Check licenses on the day (v2 excluded EmbeddingGemma for its terms).

- [ ] **Step 1: Run each candidate three times on the same gold questions**

Use the retrieval part of `make eval` with each embedder selected through the embedding contract (`inference/model_config.rs`). Record Recall@5, MRR@10 with the confidence interval, milliseconds per chunk, peak RSS, and storage per 100,000 chunks (dimension times four bytes).

- [ ] **Step 2: Apply the decision rule**

Pick the smallest candidate whose Recall@5 is within its own interval of the best and whose peak RSS fits the 8 GB budget with the VLM unloaded. If two tie inside the intervals, say the data cannot separate them and keep the current contract (a migration must be justified by a measured win).

- [ ] **Step 3: Write ADR-019 and commit**

```bash
git checkout -b docs/mem-09-embedding-ablation
git add docs/evidence/W04 docs/decisions/019-embedding-contract.md
git commit -m "docs(embedding): ablation on the gold questions and ADR-019"
```

### Task 10: Ranking and recall audit (RET-01)

**Files:**
- Create (by running): `docs/evidence/W04/ranking-audit.md`
- Modify: config switches only (`SearchConfig`, fusion weights in `context_runtime/fusion.rs`), no new ranking code

- [ ] **Step 1: Define the configurations**

Keyword only; vector only; hybrid with default weights; hybrid without the reranker (`search/reranker.rs`); hybrid without the chunk-first route; hybrid with each fusion weight halved and doubled in turn.

- [ ] **Step 2: Run the retrieval rows of `make eval` per configuration**

Record Recall@5, MRR@10 with intervals, and p95 latency. Read differences against the interval: with 30 questions, only a large gap is real.

- [ ] **Step 3: Write the table and the recommendation**

`docs/evidence/W04/ranking-audit.md` lists each configuration with its numbers and one sentence on what it shows. Change a default only if a configuration beats the default outside the interval; otherwise record "no evidence to change".

- [ ] **Step 4: Commit**

```bash
git checkout -b docs/ret-01-ranking-audit
git add docs/evidence/W04
git commit -m "docs(retrieval): ranking and recall ablations on the gold questions"
```

### Task 11: MCP tool surface audit (RET-02)

**Files:**
- Create: `docs/product/mcp-tool-audit.md`

- [ ] **Step 1: Extract the tool list from the source**

```bash
python3 - <<'PY'
import re
s = open("src-tauri/src/mcp/mod.rs").read()
start = s.index("fn tools_list_result()")
end = s.index("\nfn ", start + 10)
names = re.findall(r'"name":\s*"([^"]+)"', s[start:end])
print(len(names))
print("\n".join(names))
PY
```

Expected on 2026-09-21: `51`, from `memory.search_full_context` to `fndr.open_target`. If the count differs, the surface changed; use the new count.

- [ ] **Step 2: Classify every tool**

Table columns: tool, read, write, or execute; releases raw captured text (yes or no); side effects (meeting start and stop, `agent.run`); overlaps with another tool (for example `fndr.search`, `search_memories`, `memory.search_full_context`, `memory.search_raw`; `fndr.privacy_status` and `agent.privacy_status`; `fndr.build_context_pack`, `agent.build_context_pack`, `memory.get_context_pack`); v2 equivalent among its 14 tools; verdict keep, merge, remove, or gate behind approval.

- [ ] **Step 3: Write the recommendation**

A proposed target surface (v2 settled on 14 read-and-write-context tools plus one write tool) and the order to migrate, with `agent.run`, `start_meeting`, and `stop_meeting` moved behind the approval flow in `agent/`. No code change in this ticket.

- [ ] **Step 4: Commit**

```bash
git checkout -b docs/ret-02-mcp-tool-audit
git add docs/product/mcp-tool-audit.md
git commit -m "docs(mcp): classify all 51 tools by risk and overlap"
```

## 6. Final phase epics (W5 to W12), decomposed at the Beta retro

| Epic | Owner | Outcome and acceptance |
|---|---|---|
| E-F9 Memory consolidation | Anurup | Merged stories are periodically consolidated and stale duplicates removed. Acceptance: fragmentation at or below 1.2 on the replay sessions with zero new false merges |
| E-F10 Retrieval feedback loop | Anurup | The existing `RetrievalFeedbackRating` store feeds ranking and the WS2 preference pairs. Acceptance: a ranking change justified by feedback improves `make eval` outside the interval, and is reversible |
| E-F11 Temporal and entity recall | Anurup | Use `temporal_route.rs` and `entity_route.rs` on gold questions with time and entity constraints. Acceptance: Recall@5 on that question class improves outside the interval |
| E-F12 Reranker ablation | Minh | Qwen3-Reranker or a cross-encoder against the anchor-coverage reranker. Acceptance: a win on Recall@5 at acceptable latency, or a recorded no |
| E-F13 Deletion everywhere | Anurup | Contract invariant 10 holds for id, time range, domain, and all, verified through search, MCP, and graph. Acceptance: the contract test plus a UI-driven deletion recording |
| E-F14 Agent write-back with audit | Anurup with Kunj | Agents can add notes and decisions only through audited actions; reads return evidence wrapped as untrusted data. Acceptance: injection fixtures produce no proposals; every write appears in the audit log |
| E-F15 Bounded-decision pilots | Anurup | WS6 pilots DEC-03 to DEC-06 land behind the `Decider` cascade with calibration and a decision ledger |

## Self-review

**Spec coverage (the new ask, part 1):** stage-by-stage teardown of everything after capture with emphasis on embeddings (P4, P5, P6, P7, Tasks 5 and 9), RAG (P5, section 3, RET-01), similar-memory addition and dedup (P8, P9, Task 4), and making local models produce better outputs (P1 to P3, Tasks 3 and 6 here, and WS2); and thoughts on what happens next for search, recall, ranking, MCP, agentic use, and computer use (section 3 and RET-02).

**Placeholder scan:** the only bodies left to write against real signatures are in Task 4 Step 3 (the replay test follows the existing merge tests, whose setup the step names) and Task 8 Step 1 (Lance calls follow the current `lance_store` API). Both are tied to code that the step tells the implementer to read first.

**Type consistency:** op names and counters in Task 3 match what the WS1 report prints (`mem.` prefix and `mem.outcome.` counters). `EnrichNow`, `EnrichInputs`, and `should_enrich_now` in Task 6 match the tested source. `score_merge.score` keys match the report renderer. The MEM-01 table reproduces the results of the run on 2026-09-21.

## Sources

- Screenpipe (event-driven capture, accessibility first, local SQLite, agent access): https://docs.screenpipe.com/architecture
- Cascades and selective prediction background: https://arxiv.org/html/2603.04445v2
- Agent memory benchmark vocabulary (vendor blog): https://mem0.ai/blog/state-of-ai-agent-memory-2026
- Prompt injection against computer-use agents (why memory text is untrusted): https://arxiv.org/pdf/2507.05445
