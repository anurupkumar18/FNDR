# VS-71 native gold-case run, 2026-10-13

## Setup and scope

- Ran the current checkout's debug Tauri binary in an FNDR.app window with the Vite dev frontend. The native process had `FNDR_DATA_DIR` set to the isolated `com.fndr.app.quality-lab/knowledge-worker` profile.
- Reseeded the knowledge-worker profile from the committed synthetic corpus before the final run. Paused live capture in that profile, and the app confirmed the pause persists after relaunch.
- Used Engine diagnostics > Memory Journey > Run all 3 gold cases once after the evaluator fix. This exercised the real native image-import, OCR, extraction fallback, storage, Search, and Ask paths with the installed local models. It did not test live screen-capture admission or the owner profile.
- Visually inspected the completed native summary with captured source text collapsed. The content-safe table below is the retained evidence; no private journey export or raw capture is committed.

## Final native outcome

The inspector reported 3/3 fixtures finished, 9 passed, 3 needing review, 0 not run, and 0 fixture errors.

| Synthetic fixture | Exact Search | Paraphrase Search | Grounded Ask | Unsupported Ask | Total |
| --- | --- | --- | --- | --- | --- |
| `browser_article-04` | Pass, target rank 1 | Pass, target rank 1 | Pass, target cited and answered | Pass, refused | 4/4 |
| `chat_mock-01` | Pass, target rank 1 | Pass, target rank 3 | Review, target cited but refused | Pass, refused | 3/4 |
| `pdf_paper-04` | Pass, target rank 1 | Review, target rank 7 | Review, target cited but refused | Pass, refused | 2/4 |

The initial native run, before fixing refusal classification, displayed 6/12 passed, 6 needing review, and 0 fixture errors. The displayed final unsupported answer used the model's exact not-found response, but the inspector recorded `refusal: false` because it checked only the earlier verifier outcome. It also counted a cited grounded result as passed even when the final answer refused. The second run used the corrected evaluator and a freshly seeded profile, so the change from 6/12 to 9/12 is not a controlled retrieval improvement comparison.

## Source-grounded observations

- The browser article's stored summary retained faster search indexing and memory-leak repair, but did not retain the labeled background-worker qualifier.
- The chat image's visible snippet was only `import process`. Its longer stored context mentioned a bug fix and a PR review after lunch, but omitted the labeled names Priya and Sam. Grounded Ask stopped at the verifier with `all hits below confidence threshold`; the target appeared among evidence citations but not in the returned cards.
- The paper image's stored summary said top-five accuracy improved, but omitted the labeled twelve-percent amount and the comparison against either signal alone on a held-out set. Its paraphrase target ranked seventh. Grounded Ask reached composition with a partial-answer verifier outcome, then the model returned the exact not-found response.
- All three imports recorded `vlm_route: fallback_ocr_only` and `extraction_failure_reason: system_pressure`. The stored image memory was built from the local LLM-on-OCR fallback, so this run does not measure the pixel-VLM path. The exact OCR text and private journey artifacts remain in the local lab profile only.
- Ask currently renders each regular memory to the model from `record.snippet`. That is a plausible cause of the paper answer's missing amount, since its snippet omitted the amount. It does not explain the chat refusal, which occurred earlier at the verifier. A separate behavior change and native rerun are needed to confirm either remedy.

## Verification

- Native UI: completed 3/3 gold fixtures in the isolated FNDR.app window; inspected the full per-check summary and capture-pause state through accessibility and screenshot observation.
- Unit checks: `npm test -- --run src/domains/workspace/MemoryJourneyInspector.test.tsx` passed 7 tests; `CARGO_BUILD_JOBS=2 cargo test --lib ipc::commands::memory_journey::tests::refusal_reflects_the_final_answer_when_verifier_found_candidates` passed 1 test.
- Static checks: `npm run typecheck`, `rustfmt --edition 2021 --check src-tauri/src/ipc/commands/memory_journey.rs`, and `git diff --check` passed.

## Next investigation

Keep VS-72 through VS-74 open for source review and Search/Ask quality. Investigate the chat verifier threshold, the paper's snippet-only Ask context, and the paper paraphrase rank as separate leads. Do not treat the passing unsupported checks as proof that grounded questions work.

## Accurate OCR trial for explicit image imports

The manual image-import path was changed to Apple's existing accurate OCR profile. The continuous capture loop kept its fast profile. A focused native OCR fixture test confirmed that the accurate profile reads both short chat speaker names, the article's background-worker phrase, and the paper's twelve-percent amount and held-out comparison. The prior fast profile missed the names and the article's final word. The native app still used `fallback_ocr_only` under system pressure, so this trial did not test pixel-VLM extraction.

The final native rerun used a newly seeded knowledge-worker profile with `capture.user_paused` persisted before the app launched. Engine Diagnostics reported 0 captured frames before and after the batch. All three synthetic imports completed, with 10/12 checks passed, 2 needing review, 0 not run, and 0 fixture errors.

| Synthetic fixture | Exact Search | Paraphrase Search | Grounded Ask | Unsupported Ask | Total |
| --- | --- | --- | --- | --- | --- |
| `browser_article-04` | Pass, rank 1 | Pass, rank 1 | Pass | Pass, refused | 4/4 |
| `chat_mock-01` | Pass, rank 1 | Pass, rank 2 | Review, verifier refused | Pass, refused | 3/4 |
| `pdf_paper-04` | Pass, rank 1 | Pass, rank 2 | Review, answer refused | Pass, refused | 3/4 |

The paper paraphrase target moved from rank 7 in the earlier native run to rank 2 here. The runs used separately reseeded profiles and local model output can vary, so this is observed improvement, not a controlled causal estimate. Source inspection still found a quality problem: the chat memory's longer context attributed the planned review to "the user" rather than the labeled speaker. The paper's stored summary omitted the twelve-percent amount even though accurate OCR recognized it. Grounded Ask therefore remains open for both cases. The import OCR change is limited to better source collection and does not claim to repair model extraction or answer composition.

Verification for this trial: `CARGO_BUILD_JOBS=2 cargo test --test capture_fixtures manual_import_ocr_reads_gold_small_labels_and_amounts -- --nocapture` passed 1 test; `CARGO_BUILD_JOBS=2 python3 scripts/quality_lab.py fixtures` passed the capture fixture suite, including all 7 integration tests and the admission check. `rustfmt --edition 2021 --check` on the two changed Rust files and `git diff --check` passed. The native run above used the rebuilt debug binary and installed local models.

## Follow-up memory fidelity review

A fresh, capture-paused knowledge-worker Lab profile ran the same native gold batch again after the paused-seed change. The inspector completed all 3 cases with 10/12 mechanical checks passed, 2 grounded Ask refusals, 0 not run, and 0 fixture errors. The saved `persisted-memory.json` artifacts were compared directly with the committed `expected_text` and `required_facts` for each synthetic fixture. This review covers the accurate OCR import and the local LLM-on-OCR fallback under system pressure; it does not cover pixel-VLM extraction or owner data.

| Fixture | Required fact 1 in saved memory | Required fact 2 in saved memory | Visible memory judgment |
| --- | --- | --- | --- |
| `browser_article-04` | Faster search indexing was retained. | Memory-leak repair was retained in the longer context, but the background-worker qualifier was missing. | The card was cut midword after `memory l` and narrated the image and OCR process. It was not a useful complete release summary. |
| `chat_mock-01` | The export bug fix was mentioned, but Priya finishing it was missing; the context said a PR had opened while the source said Priya was opening one. | Review after lunch was mentioned, but attributed to "the user" instead of Sam. | The card title `Team Standup and PR review` used the source window title and topic, but hid the actor and status errors. Do not treat the generated context as a reliable next step. |
| `pdf_paper-04` | The improvement was described without the twelve-percent amount or top-five measure. | The comparison against either signal alone on a held-out set was missing. | The card said only `results analysis`; the longer context described a generic experiment. It was not useful for answering the labeled question. |

All three records stored `vision_semantic_extraction_failed` as an import diagnostic. Home rendered that machine marker as a suggested `Fix:` task for the chat and paper threads. That display failure is separate from the source-fidelity gaps and is addressed by filtering legacy import diagnostics from Resume suggestions and keeping future import diagnostics in `raw_evidence`. With the rebuilt native backend, Home showed no `Fix: vision_semantic_extraction_failed` suggestion for those legacy records or a newly imported chat fixture. Home also grouped the article and paper imports by the shared file suffix `04 png`; that naming issue remains open for work-identity review. A 10/12 mechanical retrieval score does not imply the saved memory text is faithful or useful.
