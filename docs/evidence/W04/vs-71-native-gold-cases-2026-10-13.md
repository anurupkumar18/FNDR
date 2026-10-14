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

- The browser article's stored summary retained both labeled changes: faster search indexing and a background-worker memory-leak repair.
- The chat image's visible snippet was only `import process`. Its longer stored context mentioned a bug fix and a PR review after lunch, but omitted the labeled names Priya and Sam. The grounded Ask refusal is a real usefulness gap despite the target citation.
- The paper image's stored summary said top-five accuracy improved, but omitted the labeled twelve-percent amount and the comparison against either signal alone on a held-out set. Its paraphrase target ranked seventh, and the grounded Ask refused.
- All three imports recorded `vlm_route: fallback_ocr_only` and `extraction_failure_reason: system_pressure`. The stored image memory was built from the local LLM-on-OCR fallback, so this run does not measure the pixel-VLM path. The exact OCR text and private journey artifacts remain in the local lab profile only.
- Ask currently renders each regular memory to the model from `record.snippet`. This explains why a target citation can coexist with a refusal when the snippet omits a required fact. The synthetic profile and manifest demonstrate the symptom; a separate behavior change and native rerun are needed to confirm a remedy.

## Verification

- Native UI: completed 3/3 gold fixtures in the isolated FNDR.app window; inspected the full per-check summary and capture-pause state through accessibility and screenshot observation.
- Unit checks: `npm test -- --run src/domains/workspace/MemoryJourneyInspector.test.tsx` passed 7 tests; `CARGO_BUILD_JOBS=2 cargo test --lib ipc::commands::memory_journey::tests::refusal_reflects_the_final_answer_when_verifier_found_candidates` passed 1 test.
- Static checks: `npm run typecheck`, `rustfmt --edition 2021 --check src-tauri/src/ipc/commands/memory_journey.rs`, and `git diff --check` passed.

## Next investigation

Keep VS-72 through VS-74 open for source review and Search/Ask quality. The immediate code lead is the Ask context builder's snippet-only input for regular memories. The paper paraphrase rank also needs retrieval trace review. Do not treat the passing unsupported checks as proof that grounded questions work.
