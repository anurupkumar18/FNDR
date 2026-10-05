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

## VS-40 Make the Rust crate build and test on Linux
- assignee: anurupkumar
- labels: area::vault-search, type::chore, prio::p2
- milestone: W03-Build
- estimate: 3h
- depends: none

**Why.** Cloud sessions and cheaper Linux CI cannot build the crate, so pure retrieval logic can only be verified on a Mac or a `macos-14` runner. The retrieval gate and the 883 lib tests run fine on Linux once five macOS-only call sites are gated. This would also let VS-34 run the gate on `ubuntu-latest`.

**Today.**
- `objc2-screen-capture-kit` is an unconditional dependency in `src-tauri/Cargo.toml` (line 105, above the existing `[target.'cfg(target_os = "macos")'.dependencies]` table), and `objc2` refuses non-Apple targets.
- With that moved, 15 compile errors remain in `src-tauri/src/accessibility/mod.rs`, `src-tauri/src/ocr/vision.rs`, `src-tauri/src/capture/macos.rs`, `src-tauri/src/ipc/commands/notch.rs` (line ~199, `MainThreadMarker`), and `src-tauri/src/ipc/commands/screen_guide.rs` (line ~2641, `app.hide()`).
- Six ungated `#[link(name = ..., kind = "framework")]` attributes (`accessibility/mod.rs` 3, `capture/macos.rs` 2, `ocr/vision.rs` 1) need Apple frameworks at link time.
- Tauri's `externalBin` and the `FNDR Speech Helper.app` resource in `src-tauri/tauri.conf.json` must exist at build time, and `src-tauri/build.rs` builds them with Swift.

**Do.**
1. Move `objc2-screen-capture-kit` under `[target.'cfg(target_os = "macos")'.dependencies]`.
2. `cfg`-gate the five call sites with non-mac stubs: `None`, an "Unknown" context, and `OcrError::InitializationError`.
3. `cfg_attr` the framework links and give the extern functions non-mac stubs, so no linker flag is needed.
4. In `build.rs`, write placeholders for the helper binaries on non-Apple targets instead of building them.
5. Add a Linux CI job (`ubuntu-latest`) that runs `cargo test --locked --lib` and the retrieval gate (`make qa-retrieval-check`).

**Done when.** `cargo test --locked --lib` passes on `ubuntu-latest` in CI with no linker flags, and the macOS build is unchanged.

**Evidence.** The CI run link.

**Note.** These files are owned by the local session (plan Part 2 rule 7), so the cloud did not change them. The cloud measured the fix with a never-committed 55-line shim: lib tests 883 passed, 0 failed, 10 ignored, and `make qa-retrieval-check` ranked identically to the M1 reference.

## VS-41 Check MCP auth for every item of a JSON-RPC batch
- assignee: anurupkumar
- labels: area::vault-search, type::bug, prio::p0
- milestone: W03-Build
- estimate: 1h
- depends: none

**Why.** ADR-017 says `tools/call` needs the bearer token. In Local mode a loopback batch that started with `initialize` skipped the token for every item, so a local process could run any MCP tool without it.

**Today.** `jsonrpc_method_hint` in `src-tauri/src/mcp/mod.rs` returned the first item's method for a batch, and `should_bypass_http_auth` exempts `initialize` and `tools/list` from loopback peers.

**Do.**
1. Failing test first: a loopback batch `[initialize, tools/call]` with no token must get 401; a batch of handshake methods only stays 200.
2. Exempt a batch only when every item is a handshake method.

**Done when.** The test fails on the old code and passes; `cargo test --lib` passes.

**Evidence.** Already done by the cloud on `claude/train-f-new` (b8a2412, PR #26): `docs/evidence/W03/VS-41-cloud.md`.

## VS-42 Get web page text from Chromium browsers through Accessibility
- assignee: anurupkumar
- labels: area::vault-search, type::spike, prio::p0
- milestone: W03-Build
- estimate: 6h
- depends: VS-15

**Why.** Chrome shows only browser chrome (toolbar, tabs) to the Accessibility walker, so the most common work surface still falls back to OCR or the browser semantic path.

**Today.** `accessibility::focused_text` reads Electron apps once AXManualAccessibility and AXEnhancedUserInterface are set, but a Chrome probe returned no AXWebArea and about 20 characters.

**Do.**
1. Probe Chrome, Arc, Edge, Brave, and Safari with `ax_text_probe`, with and without the opt-in attributes, waiting for the tree to fill (record counts per read).
2. Find the working recipe (attributes, timing, a second read, or Chrome's own accessibility flag), or conclude it cannot work and document the browser semantic path as the web source.
3. Implement the recipe in `focused_text` with a test for the decision logic.

**Done when.** Per browser, a table of AXWebArea found, characters, and milliseconds exists, and capture uses the best source for each.

**Evidence.** `docs/evidence/W04/vs-42-chromium-ax.md` (counts only).

## VS-43 Keep Accessibility reads inside the capture budget
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W03-Build
- estimate: 5h
- depends: VS-16

**Why.** A 50 ms cap truncates Electron apps on the first reads, and a read on every tick could raise capture CPU, which VS-16 must not do.

**Today.** `focused_text` walks the whole tree every capture with a 50 ms and 4,000 node cap.

**Do.**
1. Measure `capture.ax_ms` and capture CPU against OCR over a 20 minute session.
2. Add incremental reads (skip unchanged subtrees by role and value hash) or a per-app cache, and tune the caps from the measurements.
3. Skip the read when the frame is a perceptual duplicate, as OCR does.

**Done when.** Median `capture.ax_ms` is below the median `capture.ocr_ms` and capture CPU does not rise versus the VS-16 baseline.

**Evidence.** Metrics comparison in `docs/evidence/W04/vs-43-ax-budget.md`.

## VS-44 Capture text from apps that expose none to Accessibility
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W03-Build
- estimate: 6h
- depends: VS-16

**Why.** Preview PDFs, Keynote, and canvas apps give OCR only, which is noisy and capped to what is visible.

**Today.** Screen PDFs are OCR'd; `docs/team/tickets/minh-reopen-embeddings.md` EM-08 covers file text from downloads, not the page on screen.

**Do.**
1. List which of the 12 matrix apps return under 200 characters.
2. For Preview and other PDF viewers read the document path and extract the visible page text with PDFKit, within the privacy gates.
3. For the rest keep OCR and record the fallback share.

**Done when.** Preview PDFs store the page text, not OCR of it, and the matrix shows the remaining OCR-only apps.

**Evidence.** Per-app table in `docs/evidence/W04/vs-44-no-ax-apps.md`.

## VS-45 Store where each memory's text came from
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W03-Build
- estimate: 4h
- depends: VS-16

**Why.** VS-16 chooses Accessibility or OCR per frame but only the debug journey and metrics record it; the stored memory does not, so search and QA cannot slice by source.

**Today.** `source_kind` ('ax', 'ocr', 'browser_semantic') exists in the capture loop and the journey `text_source` stage.

**Do.**
1. Add `text_source` to the memory record with a migration that backfills 'ocr' (or 'unknown') for existing rows.
2. Write it at capture and keep it through merge and review.
3. Expose it on the card data for the UI.

**Done when.** New memories carry `text_source`, old ones backfill, and the migration test passes.

**Evidence.** Test output and the schema diff.

## VS-46 Report text source and text length together in vault health
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p1
- milestone: W03-Build
- estimate: 3h
- depends: VS-45

**Why.** The headline claim is median stored text 129 to 800 or more; it must be shown per source or an average can hide a regression.

**Today.** `make vault-health` reports median text but not by source.

**Do.**
1. Add rows: share of memories per `text_source`, median characters per source, and share under 200 characters.
2. Include the rows in the Friday scoreboard (`make scoreboard`).

**Done when.** `make vault-health` prints the per-source table on a seeded profile.

**Evidence.** The output on the owner profile before and after VS-16.

## VS-47 Make EmbeddingGemma embeddings correct in the ONNX embedder
- assignee: anurupkumar
- labels: area::embeddings, type::feature, prio::p0
- milestone: W03-Build
- estimate: 5h
- depends: VS-17

**Why.** The bake-off winner needs mean pooling plus its dense layers; the current embedder would pool the hidden state and skip the dense layers, which gives wrong vectors and would erase the measured gain.

**Today.** `src-tauri/src/embedding/onnx.rs` mean-pools the hidden state for MiniLM and BGE.

**Do.**
1. Add reference vectors for 20 sentences from the model's reference implementation.
2. Failing test first, then implement pooling, dense layers, normalization, and the 768 and 256 dimension options.
3. Prefix handling for queries and documents as the model card specifies.

**Done when.** Cosine similarity to the reference vectors is at least 0.999 on all 20 sentences.

**Evidence.** Test output.

## VS-48 Decide EmbeddingGemma on license, latency, and RAM
- assignee: anurupkumar
- labels: area::embeddings, type::decision, prio::p0
- milestone: W03-Build
- estimate: 3h
- depends: VS-47

**Why.** The cloud numbers are on shared Linux CPUs; the 8 GB M1 decides whether we can ship it.

**Today.** ADR 019 is Proposed; the M1 latency and peak RSS columns are open.

**Do.**
1. Check the license terms for redistribution in an app download.
2. Run the bake-off latency and peak RSS on the M1 for 768 and 256 dimensions.
3. Record the decision and the dimension in ADR 019.

**Done when.** ADR 019 is Accepted or Rejected with the measured M1 numbers.

**Evidence.** `docs/decisions/019-embedding-model.md`.

## VS-49 Migrate records and chunks to one embedding model
- assignee: anurupkumar
- labels: area::embeddings, type::feature, prio::p0
- milestone: W03-Build
- estimate: 8h
- depends: VS-48

**Why.** Three vector families (384, 512, 1024) cost disk, RAM, and two migrations; one text model for records, snippets, supports, and chunks removes the BGE family and the dimension mismatch risk. Minh owns EM-09 (versioned model switch); this ticket depends on it and must not duplicate it.

**Today.** Record vectors are MiniLM (384) in the main table; chunk vectors are BGE-large (1024) in a separate v5 table; `normalize_embed_migrate.rs` migrates text vectors.

**Do.**
1. Comment on EM-09 with this ticket's needs and agree who writes which part (no reassignment).
2. Add a model version to the manifest; re-embed resumably in the background, reversible until the new vectors validate.
3. Point `vector_route` and `chunk_route` at the single model and delete the second query embedder.

**Done when.** A seeded profile migrates and `make qa-retrieval-check` passes with no query worse; killing the app mid-migration resumes cleanly.

**Evidence.** Migration test output and the check report.

## VS-50 Stop downloading and loading the retired embedding models
- assignee: anurupkumar
- labels: area::embeddings, type::chore, prio::p0
- milestone: W04-Prove
- estimate: 3h
- depends: VS-49

**Why.** MiniLM and BGE-large stay on disk and in onboarding after the migration, wasting about 1.9 GB.

**Today.** Onboarding downloads and `models/` hold both; VS-38 decides the diet.

**Do.**
1. Remove the downloads and loaders, and delete the weights safely on update.
2. Measure footprint and first-run download size before and after.

**Done when.** Onboarding downloads only the chosen embedder and footprint drops by the measured amount.

**Evidence.** Before and after sizes.

## VS-51 Re-embed once after Accessibility text lands and compare
- assignee: anurupkumar
- labels: area::embeddings, type::qa, prio::p0
- milestone: W04-Prove
- estimate: 3h
- depends: VS-49, VS-16

**Why.** Vectors should see the longer, exact text, so we re-embed once after capture quality improves and measure the real gain.

**Today.** No before and after on a real vault exists.

**Do.**
1. After VS-16 is validated live, run the VS-49 migration on the owner profile.
2. Run `make vault-health` and the retrieval gate before and after (counts and ranks only).

**Done when.** A before and after table of text length, chunk coverage, and retrieval ranks exists.

**Evidence.** `docs/evidence/W04/vs-51-reembed.md`.

## VS-52 Spike: can CLIP find a screenshot from words
- assignee: anurupkumar
- labels: area::vault-search, type::spike, prio::p1
- milestone: W03-Build
- estimate: 5h
- depends: none

**Why.** The 512-dimension image vector is stored on every memory but never searched, because only the CLIP vision tower is on disk, so a typed query cannot be embedded into the same space.

**Today.** `similar_by_image_embedding` has no production caller; the image vector only feeds the capture novelty tracker.

**Do.**
1. Write the go or no-go bar first: Recall@5 and p95 latency on 20 text queries.
2. Download the CLIP text tower (check license, pin checksum), embed the queries, and score 50 of your own screenshots locally (counts and ranks only).
3. Record RAM for loading the text tower.

**Done when.** A written go or no-go with numbers against the bar set beforehand.

**Evidence.** `docs/evidence/W04/vs-52-clip-spike.md`.

## VS-53 Index and backfill the image vector
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 4h
- depends: VS-52

**Why.** Without an index and without non-zero vectors, image search is slow or empty.

**Today.** The image column has no ANN index and legacy rows are zero vectors.

**Do.**
1. Create an ANN index on the image column.
2. Backfill zero vectors in the background, bounded and resumable (EM-05 pattern).

**Done when.** An image query over 10,000 rows is under 300 ms p95 and no row is zero after the backfill.

**Evidence.** Latency and coverage output.

## VS-54 Add an image route to retrieval behind a flag
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 5h
- depends: VS-53

**Why.** A third signal helps low-text frames but can hurt precision, so it must be flagged, time-bounded, and lightly weighted.

**Today.** `retrieve` fuses keyword, vector, snippet, temporal, and chunk routes.

**Do.**
1. Add `ImageRoute` with its own timeout and a low fusion weight, default off.
2. Report its hits in `why.routes`.

**Done when.** With the flag on, no query in the existing sets gets worse and image-only cases improve.

**Evidence.** `make qa-retrieval-check` report.

## VS-55 Test image search with synthetic screenshots
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p1
- milestone: W04-Prove
- estimate: 5h
- depends: VS-54

**Why.** The current corpora have no screenshots, so image search cannot regress or improve in the gate.

**Today.** `make qa-retrieval-check` runs on text-only synthetic memories.

**Do.**
1. Generate charts, slides, and diagrams with known descriptions and add them with queries to a persona set.
2. Include the cases in the gate with the flag on.

**Done when.** The gate reports image cases separately and fails if they regress.

**Evidence.** The report.

## VS-56 Show a Looks-like chip and More like this
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 4h
- depends: VS-54

**Why.** People remember what a screen looked like; the UI should say a result matched visually and offer similar screens.

**Today.** `similar_by_image_embedding` exists but no UI calls it.

**Do.**
1. Add a Looks-like chip on image-matched results.
2. Add More like this in the Vault using `similar_by_image_embedding`.

**Done when.** Both appear on a seeded profile and tests cover the empty and zero-vector cases.

**Evidence.** Test output and one screenshot.

## VS-57 Unload the image model when idle and budget its RAM
- assignee: anurupkumar
- labels: area::local-models, type::chore, prio::p1
- milestone: W04-Prove
- estimate: 3h
- depends: VS-32

**Why.** The vision and text towers add RAM on an 8 GB Mac.

**Today.** The CLIP vision session stays loaded for the life of capture.

**Do.**
1. Load lazily and unload after a configurable idle period.
2. Add the measured RAM to the VS-32 footprint table.

**Done when.** RAM with both towers idle is back to the baseline within 60 seconds.

**Evidence.** Footprint table row.

## VS-58 Count every send off the Mac in Privacy Activity
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W03-Build
- estimate: 5h
- depends: VS-37

**Why.** Screen Guide's ChatGPT path and Hermes with a cloud provider send text off the Mac and neither is counted by `record_egress`.

**Today.** Privacy Activity counts only some local events (cloud ADR-018 draft, 'What is true today').

**Do.**
1. Find every network send of user content (grep the HTTP clients and providers).
2. Route each through one `record_egress` with client, destination class, and bytes, never content.
3. Show the log in Privacy Activity.

**Done when.** A test per path proves an entry appears; no entry contains captured text.

**Evidence.** Test output and a sanitized screenshot.

## VS-59 Store provider credentials in the Keychain
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W03-Build
- estimate: 4h
- depends: none

**Why.** Provider keys are stored as files in the data directory.

**Today.** Hermes and Screen Guide providers read key files (ADR-018 draft, 'What is true today').

**Do.**
1. Move reads and writes to the Keychain, keeping a one-time migration from the file that deletes the file.
2. Test the migration with a fake store.

**Done when.** No key file remains after the migration and providers still authenticate.

**Evidence.** Test output.

## VS-60 Decide and implement: redact secrets or skip frames that contain them
- assignee: anurupkumar
- labels: area::vault-search, type::decision, prio::p0
- milestone: W03-Build
- estimate: 5h
- depends: none

**Why.** Capture skips any frame matching a secret pattern, which silently drops the rest of a useful screen, while some docs say 'redaction'.

**Today.** `fndr.remember` refuses secrets with the same detector; capture does not redact.

**Do.**
1. Measure how many real frames are skipped for secrets (counts only).
2. Decide: redact the match and keep the frame, or keep skipping; write it in a short decision record.
3. Implement it with tests and fix the docs.

**Done when.** Behavior and docs agree and tests cover both a secret frame and a clean frame.

**Evidence.** Decision record and test output.

## VS-61 Sweep every MCP route for auth gaps
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p0
- milestone: W03-Build
- estimate: 4h
- depends: VS-41

**Why.** VS-41 found a batch bypass in the loopback exemption; other shapes may exist.

**Today.** `should_bypass_http_auth` exempts initialize and tools/list for loopback peers.

**Do.**
1. Enumerate every HTTP route and JSON-RPC method.
2. Add negative tests: batches, notifications, unknown methods, wrong content type, and non-loopback peers.
3. Fix anything that returns data without the token.

**Done when.** A table of route by auth result exists and every negative test returns 401 or 403.

**Evidence.** Test output.

## VS-62 Run the journey cases without the owner
- assignee: anurupkumar
- labels: area::tests, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 8h
- depends: VS-30

**Why.** The six journey cases need foregrounded real pages and you at the keyboard.

**Today.** The journey recorder arms a one-shot capture of whatever is in front.

**Do.**
1. Render public-style pages and documents in a scratch window on its own Space with known text.
2. Drive arm, handoff, and capture from a script and read the bundle.
3. Add cases that use Accessibility, OCR, and semantic sources.

**Done when.** Case 1 to 6 run unattended and the bundle review is automatic for counts.

**Evidence.** Script and one report.

## VS-63 Make the retrieval gate tolerate cross-platform near-ties
- assignee: anurupkumar
- labels: area::tests, type::chore, prio::p1
- milestone: W03-Build
- estimate: 3h
- depends: VS-04

**Why.** The office-pm check fails on M1 because one paraphrase query moves from rank 9 to a miss; the reference came from Linux.

**Today.** `scripts/audit/retrieval_check.py` fails on any query found in the top ten that becomes a miss.

**Do.**
1. Allow a miss only when the reference rank is 8 or worse and report it as a warning.
2. Regenerate the office-pm reference on the M1 and document the platform.

**Done when.** The check passes on both platforms with identical code.

**Evidence.** Two passing reports.

## VS-64 Run the heavy Rust tests on the Linux CI job
- assignee: anurupkumar
- labels: area::tests, type::chore, prio::p1
- milestone: W04-Prove
- estimate: 4h
- depends: VS-40

**Why.** Full `make test` takes minutes and gigabytes on the 8 GB reference Mac.

**Today.** CI runs macOS only.

**Do.**
1. After VS-40, add an Ubuntu job for lib tests and the retrieval gate.
2. Mark native-only tests so they are skipped on Linux.

**Done when.** The Ubuntu job is green and local runs can skip it.

**Evidence.** CI run link.

## VS-65 Add a lighter local test profile
- assignee: anurupkumar
- labels: area::tests, type::chore, prio::p1
- milestone: W04-Prove
- estimate: 3h
- depends: VS-64

**Why.** The M1 hit critical memory pressure and a hook blocked tool calls during a build and test.

**Today.** `make test` runs everything with one job but still peaks high.

**Do.**
1. Add `make test-native` for macOS-only and integration tests only.
2. Record time and peak memory.

**Done when.** `make test-native` completes under 5 minutes and under 3 GB.

**Evidence.** Output of the run.

## VS-66 Generate the before and after recall chart from the evidence
- assignee: anurupkumar
- labels: area::product, type::docs, prio::p1
- milestone: W04-Prove
- estimate: 3h
- depends: VS-63

**Why.** The 4:10 demo beat needs a chart that is rebuilt from committed numbers.

**Today.** Evidence JSON and baselines exist but no chart.

**Do.**
1. Write one command that reads the baseline and current reports and emits chart data and a PNG.
2. Use only committed evidence.

**Done when.** One command regenerates the chart.

**Evidence.** The PNG and the command.

## VS-67 Verify reopen to the exact page and passage on public documents
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p1
- milestone: W04-Prove
- estimate: 4h
- depends: none

**Why.** The 'opens the PDF on page 112' beat is weak because Preview has no page-jump API.

**Today.** RE-04 stores the PDF page; RE-05 plans passage reopen. Both are Minh's lane.

**Do.**
1. Comment on RE-04 and RE-05 with the demo requirement (no reassignment).
2. Choose the supported viewer and verify page and passage reopen end to end on public PDFs and pages.
3. Write the demo path.

**Done when.** The chosen demo path works ten of ten times on a clean profile.

**Evidence.** Run log and the path doc.

## VS-68 Ship a safe slice of agent write-back
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 8h
- depends: VS-35, VS-61

**Why.** Assistants writing notes back into FNDR is a priority beat but is unsafe until auth and injection tests pass.

**Today.** VS-35 specifies `fndr.remember` and a 30 case injected-note corpus; nothing is implemented.

**Do.**
1. Implement the spec's smallest slice: token auth, secret refusal, rate limit, leaf record with `source_type = agent`.
2. Pass the corpus tests.
3. Demo it from Claude Code.

**Done when.** The corpus passes and a note written from Claude Code is findable and never merged or fed to context.

**Evidence.** Test output and a run log.
