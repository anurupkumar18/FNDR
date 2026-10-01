# Cross-cutting reliability

Mission: close the verified gaps that cross the existing owner lanes without creating parallel capture, trace, or embedding systems. Each ticket reuses the current production path, proves the observable boundary, and keeps private screen or memory content out of committed evidence.

## GS-15 Stabilize Screen Guide on full-screen and Retina targets
- assignee: anurupkumar
- labels: area::command, type::bug, prio::p0
- milestone: W02-Measure
- estimate: 6h
- depends: none

**Why.** Screen Guide currently fails on a full-screen Chrome Space, can lose the intended window while FNDR hides, can pass a blank frame to OCR, and can report no readable text on a text-heavy Retina screen. The person needs one truthful answer from the screen they invoked it on, not a different fallback screen or a different excuse on each attempt.

**Today.** In `src-tauri/src/ipc/commands/screen_guide.rs`, `create_screen_guide_overlay_window` joins workspaces, but `configure_screen_guide_overlay_native_window` only calls `setCanHide(false)` and does not apply the `FullScreenAuxiliary` behavior already used by the notch window. `wait_for_screen_guide_capture_context` latches the first external app exposed after FNDR hides, and the capture section calls `capture::macos::capture_screen` without checking whether the returned pixels are blank. `src-tauri/src/capture/macos.rs` captures `CGMainDisplayID`. `src-tauri/src/ocr/vision.rs` uses the general `OcrConfig` minimum text height of `0.02`; a local one-off run on the reported 2856 by 1696 text-heavy Chrome image returned 54 characters at `0.02` and 1369 at `0.015`. The real user image must not be committed.

**Do.**
1. Add focused tests around the Screen Guide target latch, overlay collection behavior, blank-frame classifier, retry decision, and OCR profile before changing the path.
2. In `configure_screen_guide_overlay_native_window`, preserve the current click-through and content-protection behavior and add the AppKit collection behavior needed to display beside a full-screen app. Reuse the `NSWindowCollectionBehavior::FullScreenAuxiliary` setup in `src-tauri/src/ipc/commands/notch.rs`.
3. Track the most recent non-FNDR NSWorkspace activation while FNDR comes forward. At turn start, latch that external process only when the observation is recent and still belongs to the active Space; otherwise return a stable `no_target_candidate` failure before capture. After FNDR hides, accept only a converged Accessibility and NSWorkspace snapshot for the latched process. If macOS exposes a different app or no verified window, fail closed with one stable reason code and restore every hidden FNDR surface.
4. Inspect capture width, height, alpha, mean luminance, luminance spread, and byte count. Retry one blank or near-blank frame after a bounded settle, then stop before OCR or model inference if the second frame is still blank.
5. Add a dedicated `OcrConfig::screen_guide` in `src-tauri/src/ocr/vision.rs`. Prove its small-text recall and noise tradeoff with synthetic 1x and 2x fixtures instead of lowering the global capture threshold.
6. Keep this ticket on the current main-display capture path. Return an honest target-display limitation rather than silently capturing another display; GS-16 owns selecting and capturing the latched display.
7. Run native QA on a real Mac with Screen Recording and Accessibility allowed: Chrome windowed, Chrome full-screen in its own Space, FNDR initially frontmost over Chrome, one 1x external display, one 2x Retina display, permission denied, Private Mode, and a blocklisted page. Use public or synthetic content only.

**Done when.** `cd src-tauri && cargo test screen_guide` and `make test` pass; with permissions granted and a public main-display target, ten of ten repeated turns succeed in windowed Chrome and ten of ten succeed in full-screen Chrome; deliberate permission, Private Mode, blocklist, stale-target, and unsupported-display cases return their stable classified failures; blank frames never reach OCR or a model; the synthetic Retina fixture meets the recorded recall target without increasing the noise fixture; and every failure restores FNDR and capture state.

**Evidence.** `docs/evidence/W02/GS-15-screen-guide-stability.md` with the automated output, OCR fixture table, native matrix, reason-code counts, and content-free screenshots of the visible states. Do not attach real captured pixels, OCR text, browser URLs, window titles, or diagnostic bundles.

## GS-16 Capture the latched display with ScreenCaptureKit
- assignee: rathodkunj
- labels: area::command, type::feature, prio::p0
- milestone: W03-Build
- estimate: 8h
- depends: GS-15

**Why.** A person may invoke Screen Guide on a secondary display or inside a full-screen Space. Capturing `CGMainDisplayID` after verifying a window on another display can answer from the wrong pixels even when the app and window checks are correct.

**Today.** `screen_guide_main_display_signature` and `get_screen_guide_cursor_position` in `src-tauri/src/ipc/commands/screen_guide.rs` use `app.primary_monitor()`. `capture_screen` in `src-tauri/src/capture/macos.rs` calls `CGDisplayCreateImage(CGMainDisplayID())`. `FocusedWindowSnapshot` in `src-tauri/src/accessibility/mod.rs` contains title and document URL but not focused-window bounds. `src-tauri/Cargo.toml` already pins `objc2-screen-capture-kit` with `SCScreenshotManager` and `SCShareableContent`; reuse it instead of adding another capture dependency.

**Do.**
1. Extend the focused-window snapshot with the minimum AX position and size needed to select the display containing the intended window. Keep the existing expected-process-id checks so stale Accessibility data cannot select a display.
2. Resolve and latch one stable ScreenCaptureKit display identifier as part of the verified target context. Record origin, physical size, logical size, and scale factor in a typed value used by capture and point-cue conversion.
3. Add a narrow single-frame ScreenCaptureKit adapter in `src-tauri/src/capture/macos.rs`. Capture only the latched display, exclude FNDR surfaces, and keep the continuous memory capture runtime unchanged.
4. Check the safety gate and blocklist against the latched context before requesting pixels. If the display disappears, the target moves to another display, or the returned frame does not match the latched dimensions, stop without falling back to the main display.
5. Convert OCR boxes, cursor positions, and point cues from captured pixels to global logical coordinates using the latched display origin and scale. Add pure tests for negative origins, mixed 1x and 2x displays, and edge clamping.
6. Add an injected display inventory and capture adapter for deterministic tests. Native-only behavior still requires a real two-display recording and cannot be claimed from mocks.

**Done when.** Focused tests and `make test` pass; a target on either of two displays is captured from that exact display in windowed and full-screen modes; point cues land on the observed control at both scale factors; disconnecting or moving the target during capture fails closed; and Private Mode, blocklisted pages, and missing permission request no pixels and leave no files.

**Evidence.** `docs/evidence/W03/GS-16-latched-display.md` with the injected test output and a content-free two-display native matrix. Include display identifiers only as ephemeral test labels, not hardware serials, private window metadata, or captured screen content.

## GS-17 Save one explicit private Screen Guide diagnostic bundle
- assignee: anurupkumar
- labels: area::command, type::feature, prio::p0
- milestone: W02-Measure
- estimate: 5h
- depends: GS-15

**Why.** The current errors collapse several different failures into user-facing copy, while screenshots of the UI do not show which target, frame, OCR result, or phase failed. A one-turn local bundle lets the owner diagnose the next reproduction without turning normal Screen Guide use into surveillance.

**Today.** `ask_screen_guide` in `src-tauri/src/ipc/commands/screen_guide.rs` emits bounded tracing events, but normal logs do not retain the captured frame, OCR boxes, or every context-probe result. `src/domains/screen-guide/ScreenGuidePanel.tsx` has no explicit one-shot diagnostic control, and `src/shared/ipc/tauri.ts` has no diagnostic lifecycle contract. ADR-004 forbids normal screenshot persistence.

**Do.**
1. Add a clearly labeled `Save next turn` control with current state, local retention text, and Delete. Arming applies to one display-reading turn only and auto-disarms on success, failure, cancellation, quit, or timeout. Filename-only lookup must not consume the arm.
2. Add a small sibling module beside `screen_guide.rs` that owns the bundle lifecycle. Use a random directory under the FNDR app-data directory with owner-only permissions and an atomic partial-to-complete rename.
3. Save only what is needed to reproduce capture and OCR: a schema-versioned manifest, privacy-safe stage timings and reason codes, bounded context-probe facts, latched display geometry and scale, the single PNG returned by capture, OCR text and boxes, the OCR profile, and the terminal outcome. Exclude the question, history, audio, transcript, model prompt, model answer, file paths, and browser URL.
4. Never upload, index, embed, add to Memory, or include the bundle in model context. Keep at most the two newest completed bundles and remove them after 24 hours or when the person presses Delete.
5. Private Mode must disarm and delete partial data. A blocklist or pre-capture privacy rejection writes no pixels. If OCR discovers protected or secret content, delete raw frame and OCR artifacts immediately and keep only a content-free terminal reason if deletion succeeds. Do not claim physical secure erasure on APFS or SSD storage.
6. Amend ADR-004 and ADR-014 to define this narrow, explicit diagnostic exception, including consent, owner-only permissions, bounded retention, deletion, and the prohibition on upload, indexing, embedding, model context, and automatic export.
7. Test arm consumption, cancellation, partial-write cleanup, TTL cleanup, delete, blocked context, protected OCR, write failure, and application restart. Use a synthetic image fixture only.

**Done when.** One explicit arm creates exactly one complete local bundle, the next turn creates nothing, status and Delete work, restart and TTL cleanup are deterministic, a private or protected turn leaves no raw artifact, and repository search confirms the diagnostic path has no Memory, embedding, model-context, network, or automatic-export caller.

**Evidence.** `docs/evidence/W02/GS-17-private-diagnostic.md` with unit and frontend test output plus a manifest from a synthetic public screen. Do not commit the PNG, OCR text, a real diagnostic directory, or any user-profile path.

## PX-07 Prove activity traces follow real process boundaries
- assignee: anurupkumar
- labels: area::ui-polish, type::qa, prio::p1
- milestone: W03-Build
- estimate: 5h
- depends: none

**Why.** Concise activity displays are useful only when every visible step corresponds to work that actually started, completed, failed, or was cancelled. A polished timer-driven story that gets ahead of the backend would mislead users and developers.

**Today.** `src/shared/activity/activityTrace.ts` defines evidence kinds and says future stages must not be inserted. `src/shared/components/ActivityTrace.tsx` renders the shared disclosure. Current call sites include `src/shared/hooks/useSearch.ts`, `src/domains/search/SearchBar.tsx`, `src/app/HomeHero.tsx`, `src/domains/screen-guide/screenGuideState.ts`, `src/domains/omnibar/OmnibarApp.tsx`, `src/domains/workspace/AgentWorkspace.tsx`, `src/app/BiometricLockScreen.tsx`, and `src/shared/hooks/useModelDownloadStatus.ts`. There is no single QA artifact proving that their labels, actors, evidence kinds, and terminal states match the actual request, backend event, snapshot, or result boundary.

**Do.**
1. Create `docs/product/activity-trace-boundaries.md`. Inventory every mounted `ActivityTrace` call site with step id, visible label, actor, evidence kind, source event or request boundary, completion boundary, cancellation behavior, failure behavior, and privacy fields allowed.
2. Add deterministic tests with deferred promises or controlled backend events. A running step must remain running until its real boundary fires; completion, failure, timeout, supersession, and cancellation must update the same stable step id.
3. Reject or correct labels that claim a specific model, service, or phase without evidence from that subsystem. Frontend timers may debounce or enforce a timeout, but may not invent backend progress, percentages, model thinking, or future stages.
4. Assert that trace detail never contains query text, transcript text, OCR, prompts, answers, titles, URLs, paths, raw errors, or private model reasoning. Keep the existing bounded step count and accessible live-region behavior.
5. Run native QA for one search, one voice turn, one Screen Guide turn, one model download, and one cancellation. Browser tests prove rendering and boundary wiring only; the native run is required for microphone, Screen Recording, model, and OS events.

**Done when.** Every mounted trace has a boundary-matrix row and an automated success, failure, and cancellation or supersession check where that state exists; no trace advances before its source event; actor names match the observed backend; privacy assertions pass; `npm test -- activityTrace useSearch voiceActivityTrace ScreenGuide` and `make test` pass; and the five-flow native matrix has no false step.

**Evidence.** `docs/evidence/W03/PX-07-activity-trace-boundaries.md` with the call-site count, test output, and native pass or fail matrix. Screenshots may show only synthetic labels and counts; do not capture prompts, memories, transcripts, OCR, URLs, paths, or model reasoning.

## EM-13 Keep review and rebuild on one embedding document
- assignee: minhpro001
- labels: area::embeddings, type::bug, prio::p0
- milestone: W03-Build
- estimate: 5h
- depends: EM-01

**Why.** A reviewed memory can change the text that retrieval should represent. Search becomes less trustworthy if review updates the row while one or more parent vectors, source hashes, or rebuild inputs still describe the pre-review record.

**Today.** `compose_memory_embedding_document` in `src-tauri/src/memory_embedding_document.rs` is the canonical source for primary, snippet, support, chunk, and visual-semantic text. `review_one_memory_with_mode` in `src-tauri/src/memory_review/pipeline.rs` recomposes `embedding_text` but only attempts to replace the primary vector, then calls `replace_memory_preserving_chunks`; its success test checks primary text equality only. `build_v5_reindex_batch_with` in `src-tauri/src/ipc/commands/maintenance.rs` recomposes the full document with chunking configuration and embeds primary, snippet, support, and chunk roles. The two paths do not prove the same post-review document hashes or stale-state contract.

**Do.**
1. Extend `src-tauri/tests/embedding_audit.rs` with synthetic before-review and after-review records. Compare every canonical document role and source hash used by the review write-back and the V5 rebuild preparation.
2. Make review compose the post-review document once through `compose_memory_embedding_document` with the same explicit chunking configuration used by rebuild. Reuse the existing composer and vector contracts; do not add another text builder, table, or embedding dimension.
3. Refresh each parent text-vector role whose source hash changed and update the embedding manifest. Reuse EM-06's existing embed-failure and pending-state contract when an embedder fails or returns the wrong dimension; never label an old vector current for new text.
4. When `chunk_source_text` changes, emit and verify the existing stale-source marker without comparing or re-enqueueing individual chunk rows here. EM-03 owns draining rebuild work, and EM-04 owns chunk hash comparison and stale-chunk re-enqueue. Do not create another chunk worker in this ticket.
5. Add a contract test that runs a record through review, then through rebuild preparation, and asserts identical canonical text, document version, role hashes, model id, dimensions, and stale or current status. Cover no-op review, changed summary, changed structured fields, embed failure, wrong dimension, and dry-run review.
6. Use only stored, privacy-filtered `MemoryRecord` fields. Tests and reports use synthetic records and aggregate mismatch counts, never memory text or user database rows.

**Done when.** `cd src-tauri && cargo test --test embedding_audit`, `cd src-tauri && cargo test memory_review`, `cd src-tauri && cargo test maintenance`, and `make test` pass; review and rebuild produce identical canonical document roles and hashes for every fixture; no changed role keeps a stale vector marked current; dimensions remain 384 for the existing MiniLM parent fields and 1024 for BGE V5 rows; and no new composer or vector table exists.

**Evidence.** `docs/evidence/W03/EM-13-embedding-document-parity.md` with the fixture matrix, aggregate parity counts, model contracts, and test output. Include no raw memory text, paths, URLs, database blobs, or real-profile excerpts.
