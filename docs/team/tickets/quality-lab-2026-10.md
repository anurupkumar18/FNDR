# Quality Lab: native QA and quality improvements

These tickets split the FNDR Quality Lab work into reviewable native QA, quality fixes, and longer-term evaluation improvements. The Lab implementation is already present in the current working tree; its native UI batch has not yet been run by a person. These tickets start in Ready so implementation/evidence can be reviewed before any ticket is moved to Evidence or closed.

## VS-84 Record the completed Quality Lab fixture-lane evidence
- assignee: anurupkumar
- labels: area::tests, type::docs, prio::p1
- milestone: W03-Build
- estimate: 1h
- depends: none

**Why.** The synthetic fixture lane now emits structured OCR, admission, dedupe, and test metrics, but the tracked QA note still points at an earlier report. A reviewer needs a concise record of what this completed lane proves and what remains native/manual.

**Today.** `python3 scripts/quality_lab.py fixtures` completed successfully for report `src-tauri/target/quality-lab/capture-fixtures/20261006-175853/`. The ignored report contains aggregate results and input hashes; it must not be copied wholesale into Git.

**Do.**
1. Update `docs/evidence/W03/core-quality-manual-qa-2026-10-06.md` with the latest fixture run ID and aggregate metrics.
2. State that the OCR CER values pass historical regression ceilings but are not quality targets.
3. Separate production hasher/admission unit evidence from live capture-loop, native UI, and permission evidence that remains unverified.
4. Include only sanitized aggregates; no raw captures, OCR, memory descriptions, database files, or private paths.

**Done when.** The tracked evidence points to the latest report, names its exact coverage and limitations, and does not imply that the native gold-case batch has run.

**Evidence.** Updated QA note and `git diff --check` output.

## VS-71 Run the native gold-case batch in the isolated profile
- assignee: anurupkumar
- labels: area::tests, type::qa, prio::p0
- milestone: W03-Build
- estimate: 1h
- depends: none

**Why.** The serial gold-case action is implemented, but no person has run it in the native app yet. This is the highest-value next step because it checks the experience the code-level tests cannot see.

**Today.** `docs/product/quality-lab.md` documents the isolated synthetic profile and **Run all gold cases** in Engine Diagnostics → Memory Journey. The current `npm run tauri dev` session is already using the Quality Lab profile; do not run the batch against the owner vault.

**Do.**
1. Confirm the app is using the `knowledge-worker` Quality Lab profile.
2. Run **Run all gold cases** once and wait for completion.
3. Record each case's completed, passed, not-run, and error counts; note any visual issue or confusing result.
4. Share a screenshot of the inspector summary with captured source text out of view.

**Done when.** All three labeled cases have an explicit outcome, partial failures are visible as partial, and the screenshot or sanitized notes are attached as local evidence. Do not score or restart the app while it is using this profile.

**Evidence.** A content-safe inspector screenshot and sanitized per-case outcome table in `docs/evidence/W04/`.

## VS-72 Review gold memory titles and descriptions against source facts
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p0
- milestone: W03-Build
- estimate: 2h
- depends: VS-71

**Why.** Mechanical Search/Ask checks can pass while a memory title or summary is vague, repetitive, or unsupported. A human review is needed before the gold cases can measure memory usefulness.

**Today.** `src-tauri/tests/fixtures/screens/quality-cases.json` has three synthetic gold cases and required facts. `docs/product/quality-lab.md` explicitly leaves summary faithfulness to human review.

**Do.**
1. For every imported case, compare the visible source facts, OCR text, persisted fields, title, and summary.
2. Mark each required fact as supported, missing, or contradicted; separately note narrator phrasing, app IDs, generic filler, and unsupported claims.
3. Record whether the memory is useful enough to keep and what a better title/description must communicate.
4. Keep screenshots and raw memory text local; commit only synthetic labels and aggregate results.

**Done when.** Every gold case has a source-grounded human judgment and at least one concrete quality observation, with no model-generated judgment treated as approval.

**Evidence.** A sanitized case-by-case review table under `docs/evidence/W04/`.

## VS-73 Review Search quality on exact, paraphrase, and weak matches
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p0
- milestone: W03-Build
- estimate: 2h
- depends: VS-71

**Why.** Search must keep useful possible matches visible without making weak candidates look certain. Existing retrieval metrics do not replace checking the evidence and confidence a person sees.

**Today.** The gold-case batch runs exact and paraphrase queries through production Search. `VS-12` owns the no-good-match behavior and `VS-22` owns match explanations; this ticket evaluates those behaviors on the new synthetic cases without duplicating their implementation work.

**Do.**
1. Review each exact and paraphrase result's rank, confidence, matched evidence, and displayed description.
2. Record any plausible candidate that was hidden, any irrelevant result that looked certain, and any evidence snippet that failed to explain the match.
3. Add a small number of synthetic ambiguous and unrelated queries only when a real gap appears in this review.
4. Route implementation gaps to `VS-12` or `VS-22` when they fit; file a new focused issue only for uncovered behavior.

**Done when.** Every gold query has an explained useful/not-useful judgment and the review distinguishes candidate retrieval, confidence calibration, and presentation defects.

**Evidence.** Sanitized per-query rank/evidence notes and any newly approved query labels under `docs/evidence/W04/`.

## VS-74 Review Ask citations and unsupported-question handling
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p1
- milestone: W03-Build
- estimate: 2h
- depends: VS-71

**Why.** A grounded answer should cite the right memory, and a question beyond the stored evidence should not trigger a fabricated answer. The current mechanical result checks only citation presence and refusal.

**Today.** Each gold case has one grounded and one unsupported question. The inspector records answer, citation, and refusal outcomes, but does not decide whether the answer is factually complete or helpful.

**Do.**
1. Review the grounded answer against the required facts and the cited source.
2. Review the unsupported answer for invented detail, useful uncertainty, and an appropriate next step.
3. Separate an incorrect retrieval from a bad answer given correct evidence.
4. Record specific failures without adding real captures or private text to the repository.

**Done when.** All six Ask outcomes have human judgments for grounding, completeness, citation quality, and refusal usefulness.

**Evidence.** Sanitized Ask review table and focused regression cases for any confirmed defect.

## VS-75 Check Home and Resume context with reviewed synthetic memories
- assignee: anurupkumar
- labels: area::ui-polish, type::qa, prio::p1
- milestone: W03-Build
- estimate: 2h
- depends: VS-71

**Why.** The Home surface should help a person resume work using a useful topic, context, recent activity, and a supported next step. A title like `chatgpt:title:chatgpt` is not a useful handoff.

**Today.** The native app can open an isolated seeded Quality Lab profile and Memory Journey imports. Existing `VS-36` owns connecting extracted task candidates to Resume; this issue reviews the visible Home/Resume experience and reports concrete gaps without reimplementing that behavior.

**Do.**
1. Inspect Home and Resume using the synthetic profile and at least one imported gold memory.
2. Judge whether each visible item says what happened, provides specific context, and offers only a source-supported next step.
3. Check that recent activity is distinguished from a durable memory and that the empty state is honest.
4. Send a content-safe screenshot and list the exact fields that confused or helped you.

**Done when.** The review covers at least three visible Home/Resume items or the available smaller set, with specific observations linked to current data builders or existing tickets.

**Evidence.** Content-safe screenshot and concise field-level review under `docs/evidence/W04/`.

## VS-76 Save a frozen per-case Quality Lab baseline
- assignee: anurupkumar
- labels: area::tests, type::qa, prio::p1
- milestone: W04-Prove
- estimate: 2h
- depends: VS-71, VS-72, VS-73, VS-74

**Why.** The fixture runner now reports OCR, admission, dedupe, and Rust test metrics, while the native journey action reports per-query outcomes. The evidence is split across reports and human notes, so later quality changes are hard to compare.

**Today.** `scripts/quality_lab.py` writes run manifests under ignored build output. Memory Journey retains its normal bounded evidence bundles; the all-cases summary is currently an in-session view.

**Do.**
1. Record the exact checkout fingerprint, suite, fixture hashes, and relevant model/prompt contract for the reviewed run.
2. Save sanitized, non-sensitive per-case OCR, memory-fidelity, Search, and Ask measurements alongside the human judgments.
3. State which stages were observed, inferred, skipped, or not measured.
4. Do not overwrite the accepted report when later experiments run.

**Done when.** A reviewer can identify the baseline inputs, reproduce the automated run, and compare every labeled case without access to owner data or an active database.

**Evidence.** A compact versioned report and reproduction commands under `docs/evidence/W04/`.

## VS-77 Improve memory fallbacks from reviewed failure examples
- assignee: anurupkumar
- labels: area::local-models, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 3h
- depends: VS-72

**Why.** Thin evidence still needs a useful factual fallback. It should not become narrator-heavy boilerplate, expose internal identifiers, or invent why a screen mattered.

**Today.** `src-tauri/src/inference/prompts.rs`, `src-tauri/src/summariser/narration_filter.rs`, and `src-tauri/src/memory_insight/` contribute to generated and deterministic memory text. The user-facing FNDR voice has no narrator; prompt versions and catalog rows must move with prompt changes.

**Do.**
1. Trace reviewed title/description defects to extraction, validation, narration filtering, or deterministic fallback before editing.
2. Add failing fixtures for the exact reviewed defects and keep source text explicitly untrusted.
3. Reuse or simplify the existing prompt/fallback path; do not add a second summarizer layer.
4. If a prompt string changes, bump its prompt version and update `docs/product/llm-task-catalog.md` in the same change.

**Done when.** Reviewed defects have focused regressions, unsupported claims are withheld, weak evidence still produces a factual fallback, and the existing prompt fingerprint checks pass.

**Evidence.** Before/after synthetic examples, focused test output, and the updated prompt catalog row when applicable.

## VS-78 Add human-reviewed fact coverage to gold-case reports
- assignee: anurupkumar
- labels: area::tests, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 3h
- depends: VS-72

**Why.** OCR character error and retrieval rank cannot tell whether a memory retained the decision or next step that made the source useful. Fact coverage needs a separate, reviewed measure.

**Today.** `quality-cases.json` contains required facts, but generated summary coverage is deliberately not auto-approved by a model judge. The runner reports OCR CER as a regression ceiling, not an OCR quality target.

**Do.**
1. Add a versioned human-review section keyed by stable gold case ID, with supported, missing, and contradicted labels per required fact.
2. Keep unreviewed/draft labels out of blocking scores.
3. Report coverage, unsupported-claim count, and review state separately from CER and Search/Ask results.
4. Test missing labels, unknown case IDs, and unreviewed labels.

**Done when.** The report cannot present draft labels as approved, and reviewers can compare factual coverage without collapsing it into one blended score.

**Evidence.** Synthetic manifest examples and passing parser/report tests.

## VS-79 Expand the gold set with ambiguous and unrelated questions
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p2
- milestone: W04-Prove
- estimate: 3h
- depends: VS-73

**Why.** Exact and paraphrase positives alone reward returning something for every query. FNDR should surface plausible candidates with honest uncertainty and make clearly unrelated queries look weak.

**Today.** The current gold manifest has three positive cases with four production Search/Ask checks each. Search evaluation has separate positive recall and negative calibration measures, but the Memory Journey gold batch is small.

**Do.**
1. Add a few synthetic ambiguous and unrelated queries with an explicit expected candidate set and evidence rationale.
2. Include cases where a related candidate should remain visible at low confidence and cases where none is relevant.
3. Keep all imported images positive/store fixtures; privacy-negative fixtures must never enter the import path.
4. Add the query set to the report hashes and test stable manifest validation.

**Done when.** The expanded set catches both hidden plausible candidates and overconfident irrelevant results without requiring a stricter result cutoff.

**Evidence.** Reviewed synthetic query labels, manifest test output, and per-query report.

## VS-80 Replay synthetic frames through the isolated capture loop
- assignee: anurupkumar
- labels: area::tests, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 5h
- depends: VS-76

**Why.** Current fixture checks exercise production admission logic and dedupe primitives, while image import exercises the downstream pipeline. Neither proves capture-loop orchestration end to end.

**Today.** `docs/product/quality-lab.md` identifies capture-loop fixture replay as the next high-value build slice. The existing fixture image importer accepts only positive synthetic cases and bypasses live capture admission.

**Do.**
1. Add a deterministic frame source to the existing capture loop, with output constrained to the marked Quality Lab profile.
2. Exercise admission, frame dedupe, OCR/cleanup, extraction, embeddings, storage, and presentation through their existing production boundaries.
3. Keep privacy-negative frames out of the image import route and verify capture admission rejects them before OCR/model work.
4. Mark every stage observed, skipped, failed, or unavailable; never infer a successful stage from downstream output.

**Done when.** A synthetic replay creates only isolated Lab records, reports each actual stage and timing, rejects protected fixtures before content processing, and cannot target the owner vault.

**Evidence.** Focused automated replay output plus a native operator run on a disposable Quality Lab profile.

## VS-81 Persist per-case journey results for before/after comparison
- assignee: anurupkumar
- labels: area::tests, type::feature, prio::p1
- milestone: W05-Retro
- estimate: 4h
- depends: VS-76, VS-78

**Why.** A batch summary is useful during a session, but improving the product requires comparing the same reviewed cases across commits and model/prompt contracts.

**Today.** Memory Journey persists bounded individual evidence bundles; its batch summary is in-session. The suite scorer compares frozen persona-level Search/Ask/Retrieve queries but does not include reviewed fact coverage for imported fixture memories.

**Do.**
1. Store a compact local JSON result keyed by case and query, including checkout/source fingerprint, suite, fixture hash, model/prompt contract, observed stages, ranks, citations, refusals, and human labels.
2. Add a compare command that requires matching case/query IDs and shows per-case deltas without blending them into one score.
3. Retain previous reports and label contract changes that invalidate direct comparisons.
4. Keep screenshots, raw OCR, memory text, and owner data out of committed reports.

**Done when.** Two local runs of the same reviewed set produce a readable per-case comparison, and changed inputs/contracts are disclosed instead of silently averaged.

**Evidence.** Synthetic before/after report pair and comparison tests.

## VS-82 Show baseline and candidate quality in native diagnostics
- assignee: anurupkumar
- labels: area::ui-polish, type::feature, prio::p2
- milestone: W05-Retro
- estimate: 4h
- depends: VS-81

**Why.** Product quality is easier to improve when the reviewed example, evidence, and before/after outcome are visible in the same native surface used for manual QA.

**Today.** Engine Diagnostics contains Memory Journey and the fixture batch action, while suite comparison is a local command-line report. This issue adds a view to the existing diagnostics surface, not a separate browser dashboard.

**Do.**
1. Reuse the existing diagnostics navigation and run report types.
2. Show per-case baseline and candidate outcomes for fidelity, Search, Ask, and observed pipeline stages.
3. Show evidence snippets and click through to the same Vault/Search/Ask presentation.
4. Make missing, changed-contract, and unreviewed labels explicit; keep raw screenshots and OCR local.

**Done when.** A reviewer can inspect a paired case in FNDR and see its evidence, human-review state, and metric deltas without treating missing data as a pass.

**Evidence.** Native screenshots from a synthetic profile and focused UI tests.

## VS-83 Measure Quality Lab resource cost on the reference Mac
- assignee: anurupkumar
- labels: area::local-models, type::qa, prio::p2
- milestone: W05-Retro
- estimate: 3h
- depends: VS-32, VS-80

**Why.** Better memory quality must fit the device's RAM, storage, latency, and thermal budget. Fixture unit tests do not measure the full native capture-to-search workload.

**Today.** `VS-32` measures component footprints and sets budgets. Quality Lab already avoids a duplicate model download by sharing the owner's model directory, so the Lab profile's disk size must not count those linked model bytes twice.

**Do.**
1. Run the same fixed synthetic capture/retrieval workload sequentially with the approved active model.
2. Record cold/warm latency, peak physical footprint, model load/unload behavior, and actual Lab profile disk delta.
3. Compare against the existing component budgets and retain the run fingerprint.
4. Only after the MiniLM baseline is frozen, run any candidate model as a separate vector-space experiment; do not mix vectors or truncate dimensions.

**Done when.** The report attributes measured resource changes to the workload and model state, fits the reference device budget or identifies the exact failing component, and preserves a rollback-safe baseline.

**Evidence.** Sanitized resource table, run manifests, and comparison output under `docs/evidence/W05/`.
