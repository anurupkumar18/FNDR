# Handoff: FNDR Memory Journey QA

## Goal

Record six real capture-to-answer journeys before tuning capture, extraction, embeddings, or ranking. Case 1 is still blocked at deduplication.

## Current state

- Clean `main` at `ef9de069c66a4e34209dd481e16d1a644efa79b8`; GitLab and GitHub match.
- No open GitLab merge requests.
- QA app is stopped. Profile and metrics remain in `/private/tmp/fndr-memory-journey-qa`.
- VS-27 (#284) is `status::evidence`; VS-28 (#285) and VS-29 (#286) are `status::ready`.

Kunj's stacked MRs were retargeted and merged bottom-up after the combined full gate passed:

| MR | Result |
|---|---|
| [!26](https://capstone.cs.utah.edu/fndr/fndr/-/merge_requests/26) PD-06 actions policy | merged |
| [!27](https://capstone.cs.utah.edu/fndr/fndr/-/merge_requests/27) GS-01 command surface | merged |
| [!28](https://capstone.cs.utah.edu/fndr/fndr/-/merge_requests/28) GS-03 typed tool registry | merged |
| [!29](https://capstone.cs.utah.edu/fndr/fndr/-/merge_requests/29) GS-11 shared risk policy | merged |

## Decisions made

| Decision | Reason | Files/Docs |
|---|---|---|
| Memory Journey is debug-only, one-shot, bounded, and records observed pipeline boundaries. | Needed honest end-to-end evidence without changing release behavior. | MR !25, `src-tauri/src/memory_journey.rs`, `MemoryJourneyInspector.tsx` |
| Keep the durable OCR default on Apple Vision Fast, but fix the raw enum values and cap the text-height filter at 24 physical pixels. | Preserves the 30-screen OCR baseline while making 2880x1800 Retina text readable. | MR !30, `src-tauri/src/ocr/vision.rs` |
| Add an 8-second target handoff after arming. | Prevents FNDR/Codex from immediately consuming the journey. | MR !30, `memory_journey.rs`, `MemoryJourneyInspector.tsx` |
| Treat Case 1 retry as a dedupe bug, not an OCR failure. | The intended browser frame was captured, but `perceptual_duplicate` ended the journey before OCR. | `src-tauri/src/capture/mod.rs`, exported retry bundle |

## Files changed

| Area | Change | Commit/MR |
|---|---|---|
| Memory Journey recorder, inspector, query explanations, tests, docs | Initial feature and six-case contract | MR !25, merge `86f852c` |
| OCR and handoff | Correct Vision bridge, Retina-aware cutoff, backend handoff, UI guidance | MR !30, merge `ff9d98a` |
| Command surfaces | Policies, typed 12-tool registry, shared command/voice/MCP risk gate | MRs !26-!29, final main `ef9de06` |

## Files inspected but not changed

- `src-tauri/src/capture/mod.rs` around `begin_capture_attempt` and `PerceptualHasher::is_duplicate`.
- `src-tauri/src/capture/dedupe.rs`.
- `/Users/anurupkumar/Downloads/memory-journey-0c768b48-f63d-4149-a356-6e744d85d8ed.fndrjourney.zip`.
- `/private/tmp/fndr-memory-journey-qa/metrics.ndjson` and native app logs.

## Case 1 evidence

### Attempt 1: `af9cab22-d88c-49a1-ad51-f8bc4a860889`

- Captured Codex instead of the intended browser because the one-shot started immediately.
- OCR returned zero blocks due to reversed Apple Vision Fast/Accurate raw values plus the Retina-scaled height cutoff.
- The exact 2880x1800 artifact produces 48 blocks and 2,717 cleaned characters after MR !30.

### Retry: `0c768b48-f63d-4149-a356-6e744d85d8ed`

- Captured the correct SimBio “What is Mitosis?” page at 2880x1800.
- Stopped at `admission: perceptual_duplicate`; only the frame artifact exists.
- Apple Vision probe at FNDR's effective Fast/0.0133 profile finds 27 blocks and 1,107 characters, so OCR should succeed once the frame reaches that stage.
- Likely cause is confirmed by control flow: during the 8-second handoff, `begin_capture_attempt` returns `None`, but the ordinary pipeline continues and updates `PerceptualHasher`. The first armed attempt then sees the unchanged target as its own recent duplicate.

## Tests / verification

- MR !30: exact failed artifact probe, 30-screen OCR corpus, Memory Journey tests, inspector tests, and full `make test` passed.
- Kunj combined stack on current main: `CARGO_BUILD_JOBS=1 make test` passed — 70 frontend files / 439 tests, production build, 844 Rust unit tests plus integration suites.
- `git diff --check` passed before both merges.

## Known issues

1. Handoff frames still enter normal dedupe history before the armed journey starts.
2. The latest native run logged repeated `memory_review` failures: `review_memory_record returned no parseable JSON`, plus one structured-memory JSON parse error. This is separate from Case 1 but should become a later journey/eval case.
3. Earlier long-running QA reached roughly 2.3–2.4 GB physical footprint and high memory pressure; keep native retries short until measured separately.

## Next steps

1. Change the debug-only arm API to distinguish `inactive`, `handoff_pending`, and `started`. While `handoff_pending`, skip the ordinary capture tick so it cannot seed dedupe with the target frame.
2. Add a regression test proving frames observed during handoff cannot cause the first armed target to fail as a duplicate. Preserve ordinary production dedupe semantics after the journey starts.
3. Record dedupe evidence in the journey (`threshold`, match kind, hash/RGB distance) so future duplicate decisions are explainable.
4. Rerun Case 1 against the same public SimBio page. Expected: non-zero OCR/cleanup, extraction, embeddings, storage, then exact/paraphrase Search and grounded Ask.
5. Continue Cases 2–6 only after Case 1 completes and its bundle is reviewed.

## Risks / do not do

- Do not tune OCR, extraction prompts, embeddings, or ranking from these failed attempts; neither reached the complete pipeline.
- Keep real captures and `.fndrjourney.zip` bundles local. Commit only sanitized labels or public/synthetic fixtures.

## Useful context for next agent

- Launch: `FNDR_DATA_DIR=/private/tmp/fndr-memory-journey-qa FNDR_METRICS_DUMP=/private/tmp/fndr-memory-journey-qa/metrics.ndjson npm run tauri dev`.
- Bundles: `/private/tmp/fndr-memory-journey-qa/developer-memory-journeys/`.
- Canonical tickets: VS-27 #284, VS-28 #285, VS-29 #286.
- VS-28 already has a comment with MR !30 evidence and the pending Case 1 retry.
