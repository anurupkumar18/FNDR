# Anurup: follow-ups filed 2026-10-04

Filed by the parallel-session campaign (`docs/superpowers/plans/2026-10-04-parallel-sessions/README.md`, Part 6) after grepping every ticket file and `docs/**` for the same mechanism. Each ticket names the nearest existing ticket and why it does not already cover this.

## VS-30 Stop handoff frames from seeding capture dedupe history
- assignee: anurupkumar
- labels: area::vault-search, type::bug, prio::p0
- milestone: W02-Measure
- estimate: 4h
- depends: VS-27

**Why.** Memory Journey Case 1 never reached OCR: frames seen during the 8 second target handoff seeded `PerceptualHasher`, so the armed target read as a duplicate of itself. Nothing downstream of VS-28 can run until this is fixed. VS-27 built the recorder; no ticket covers this defect.

**Today.** `begin_capture_attempt` in `src-tauri/src/capture/mod.rs` returns `None` during handoff, but the ordinary tick still updates the hasher. The arm API has only armed and not armed.

**Do.**
1. Replace the arm flag with a tri-state: `inactive`, `handoff_pending`, `started`.
2. While `handoff_pending`, skip the ordinary tick entirely.
3. Write a failing regression test first: frames observed during handoff must not make the first armed target a duplicate. Ordinary production dedupe is unchanged once the journey has started.
4. Record dedupe evidence in the journey bundle: threshold, match kind, hash or RGB distance.
5. Rerun Case 1 on the same public page and review the bundle.

**Done when.** The regression test fails before the change and passes after, `make test` passes, and the Case 1 bundle shows non-zero OCR text, extraction, embeddings, and storage.

**Evidence.** `docs/evidence/W04/vs-30-handoff-dedupe.md` with the test output and the sanitized Case 1 stage table.

## VS-31 Make memory review and structured-memory output parse reliably
- assignee: anurupkumar
- labels: area::local-models, type::bug, prio::p1
- milestone: W03-Build
- estimate: 4h
- depends: VS-30

**Why.** The latest native run logged repeated `review_memory_record returned no parseable JSON` and a structured-memory JSON parse error. Memories silently skip review. LM-05 constrains output to a schema going forward; this ticket finds why the current outputs fail and adds the case to the journey set. Kunj's LM lane may fold this into LM-05 if the cause is the same.

**Do.**
1. Reproduce from the journey metrics log and record the raw failure shape (counts and error class only, never memory text).
2. Find the cause: prompt, token budget, truncation, or fence handling.
3. Add a tolerant parse or one bounded retry, test-first, with a fixture for each failure shape.
4. Add one journey eval case that exercises review.

**Done when.** The fixture tests pass and a fresh journey run shows zero parse failures in the metrics dump.

**Evidence.** `docs/evidence/W04/vs-31-review-parse.md`.

## VS-32 Measure the memory footprint by component and set a budget
- assignee: anurupkumar
- labels: area::local-models, type::qa, prio::p1
- milestone: W03-Build
- estimate: 5h
- depends: none

**Why.** Long QA runs reached 2.3 to 2.4 GB physical footprint on the 8 GB reference Mac; the month plan target is 700 MB with the vision model unloaded. LM-10 budgets the model-loaded scenarios; nothing attributes the idle and capture footprint to components.

**Do.**
1. Sample physical footprint (not RSS) over a 20 minute capture session for the webview, Rust core, embedder, OCR, LanceDB, and any loaded model, using `FNDR_METRICS_DUMP` and `vmmap` summaries.
2. Write the table of components, MB, and what unloads when.
3. Set a budget per component and add a short regression check that fails above it.

**Done when.** `docs/evidence/W04/footprint-by-component.md` has measured rows and a check exists that can be run in under 5 minutes.

**Evidence.** The evidence file and the check output.

## VS-33 Drop the empty graph route from retrieval until it is persisted
- assignee: anurupkumar
- labels: area::vault-search, type::chore, prio::p1
- milestone: W03-Build
- estimate: 2h
- depends: VS-09

**Why.** The graph route searches an in-memory graph that is empty after a restart, yet the README presents the insight graph as stable. VS-26 only says to avoid claims; it does not remove the dead route.

**Do.**
1. Confirm with a test that the graph route returns nothing on a fresh profile.
2. Remove it from fusion in `retrieve` and delete its dead wiring.
3. Correct the README and architecture doc wording.

**Done when.** `make qa-retrieval-check` shows no Recall@5 drop and `grep` finds no caller of the removed route.

**Evidence.** Test output and the doc diff.

## VS-34 Run the retrieval gate in GitHub Actions
- assignee: anurupkumar
- labels: area::vault-search, type::chore, prio::p1
- milestone: W03-Build
- estimate: 3h
- depends: VS-04

**Why.** VS-04 makes `make qa-retrieval-check` a local gate. Without CI it depends on someone remembering to run it.

**Do.**
1. Add a workflow job on `macos-14` that restores the embedder cache and runs the check on the synthetic corpora only.
2. Fail the job when Recall@5 drops more than 0.05 on any path.

**Done when.** A pull request that lowers recall fails the job, and one that does not passes it.

**Evidence.** Two CI run links.

## VS-35 Specify `fndr.remember` agent write-back
- assignee: anurupkumar
- labels: area::vault-search, type::spike, prio::p2
- milestone: W04-Prove
- estimate: 4h
- depends: none

**Why.** Month plan priority 6 describes assistants writing notes back into FNDR with provenance. It has no ticket, and an unspecified write path is a prompt-injection door.

**Do.**
1. Write `docs/product/fndr-remember-spec.md`: fields (`source_type = "agent"`, client, tool, time), redaction identical to screen text, rate limits, and the ADR-017 token requirement.
2. Build a small synthetic corpus of injected notes (instructions hidden in a note) and the expected handling.
3. List the tests an implementation must pass.

**Done when.** The spec and corpus are committed and reviewed by the owner.

**Evidence.** The two files.

## VS-36 Connect task candidates to Resume next steps
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p2
- milestone: W04-Prove
- estimate: 3h
- depends: VS-11

**Why.** `extract_task_candidates` has no production caller, so Resume cannot offer "next steps" even though the extraction exists.

**Do.**
1. Test-first: Resume for a synthetic session returns up to three candidate next steps with their source memories.
2. Call the extractor from the Resume path, or delete it if the test shows it cannot meet the bar (anti-bloat gate).

**Done when.** Either Resume shows cited next steps in a test, or the dead code is deleted with the reason recorded.

**Evidence.** Test output or the deletion diff.

## VS-37 Persist Privacy Activity and log MCP reads
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p2
- milestone: W04-Prove
- estimate: 4h
- depends: none

**Why.** Privacy Activity counters reset when the app quits and do not show what MCP clients read. "Trust" in the month plan depends on a person seeing which client read what.

**Do.**
1. Persist counters in the local profile.
2. Log each MCP read as client, tool, result count, and bytes, never content.
3. Show the log in Privacy Activity with a clear action.

**Done when.** Counters survive a restart and an MCP `tools/call` appears in the log in a test.

**Evidence.** Test output and one sanitized screenshot.

## VS-38 Decide the model weight diet from the embedding results
- assignee: anurupkumar
- labels: area::local-models, type::decision, prio::p2
- milestone: W04-Prove
- estimate: 2h
- depends: VS-17

**Why.** About 1.9 GB of model weights sit on an 8 GB Mac, and BGE is downloaded but unused. VS-17 picks the embedder; EM-09 migrates it; neither decides what to stop downloading.

**Do.**
1. List each downloaded model, size, and the code path that loads it.
2. After VS-17, mark each keep, lazy-download, or remove.
3. File or make the smallest change that stops downloading what is unused.

**Done when.** `docs/decisions/` has a short record and onboarding downloads only what is used.

**Evidence.** The record and the model list before and after.

## VS-39 Add a needs-human status to the board script
- assignee: anurupkumar
- labels: area::tests, type::chore, prio::p2
- milestone: W03-Build
- estimate: 2h
- depends: none

**Why.** The board docs list a Needs human column, but `gitlab_sync.py` only knows ready, doing, evidence, and closed, so it is a comment prefix today.

**Do.**
1. Add `status::needs-human` to the script and its label colors, test-first with the existing script tests.
2. Update `docs/team/gitlab-agent-instructions.md`.

**Done when.** `move <ID> needs-human` works and the script tests pass.

**Evidence.** Test output.
