# Native file finding implementation record

Files search uses its own storage and EmbeddingGemma 2 process, not FNDR's screen-memory embedding space. Kunj requested native file finding, separate indexing, onboarding opt-in, and a development default on 2026-11-17.

## Decisions and delivered slices

- Reused committed FNDR Files source from `a762a80`. The other local worktree's uncommitted changes were not copied. [ADR 039](../../decisions/039-native-file-recall-boundary.md) records rejected storage/runtime options and attribution.
- Integrated the native crawler, extraction reader, SQLite WAL/FTS5 stores, float16 vectors, file/code scopes, worker, settings, typed IPC, and helper build/release wiring.
- Added onboarding folder selection and explicit model download, Settings controls, Search results, Quick Find and notch integration, matching passages, and native source actions.
- Debug defaults apply only to absent settings and leave roots empty. Invalid settings fail closed. Saved disabled settings win. Downloads require explicit enabled, selected roots and verify pinned revision, size, and SHA-256.
- Applied live blocklist, current-root, exclusion, source-metadata and readability checks before disclosing cached results, passages or executing file actions. Removed roots are deleted; temporarily unavailable selected volumes are retained but hidden.
- Kept the existing memory embedding contract, capture pipeline, summaries, Vault and MCP memory retrieval unchanged.

## TDD summary

Behavior implemented: native file finding with separate consent, storage and models.

Tests added or updated cover setup failures and retries, code-only roots, declined onboarding, file/code scopes, PDF passages, result reasons, preview fixtures, helper bundle drift, live source visibility, indexing lifecycle and small-corpus semantic search.

Red-green evidence:

- Setup tests failed when saving consent failed but downloading/advancing continued, and when a failed download start disabled retry. They pass after save/download success became prerequisites.
- A PDF passage test failed because only document and code hits fetched passages. It passes after PDF hits use the same passage path.
- A native tiny-index regression failed because a two-sigma gate cannot admit the relevant one-vector result. The bounded absolute-cosine fallback passes the test and the real-model code/negative eval.
- The lifecycle stand-in embeds all text identically. Its semantic-empty assertion became invalid after small-corpus admission. The test now uses keyword visibility and independently requires physical row removal.
- That stronger test exposed a real FSEvents path mismatch: `/var` selected roots versus `/private/var` event paths. Mapping canonical event paths back to selected root spelling fixed the observed failure. No timeout increase or temporary logging was kept.
- Two protocol tests failed when a late startup and late reply succeeded despite a supplied deadline. Pipe I/O now runs on a dedicated thread with a bounded caller wait; both tests pass, and the reply test asserts that the timed-out child was reaped. Production startup and calls use a 120-second deadline, including blocked writes.
- Pre-publication keyboard browser testing exposed Enter on a filter opening the selected file. A focused unit test reproduced it. The results section now leaves keyboard activation to focused filters, action buttons and preview controls, while result rows retain navigation shortcuts. The unit test and both browser input modes pass.
- A typed-scope unit test failed because `code:` in the main search did not select the Code filter. Explicit `file:` and `code:` input now updates the filter, while ordinary queries preserve manual filter selection. Browser coverage also exercises both typed prefixes.

## Verification on 2026-11-17

| Check | Result and scope |
| --- | --- |
| `npm test` | 134 test files, 1,041 tests passed, including IPC/documentation drift, keyboard activation and typed-scope regressions |
| `npm run build` | TypeScript and Vite production build passed; existing large-chunk warning remains |
| `cargo test --lib files::` | 107 passed, 0 failed; includes real SQLite and subprocess protocol fakes, not real model inference |
| `cargo build --bin fndr` | Full debug application binary built with the native command registrations |
| Files helper build and stage | Passed using the existing matching pinned llama.cpp library through `FNDR_LLAMA_DIR`; did not rebuild upstream llama.cpp from scratch |
| Helper reference check | Real text/media assets, 10 comparisons passed at cosine 1.00000, including multilingual text, image, audio and batch cases |
| `files_eval` | Real macOS readers and model: 42 synthetic files, 0 extraction failures, 46 vectors; all 20 expected files ranked first, including asserted PDF pages; R@1/R@5/MRR all 1.00 |
| Separate real code eval | Exact identifier and paraphrase both ranked `retry.py` first; all 3 unrelated queries returned no hits |
| `make qa-retrieval-check PERSONA=software-engineer` | Passed on an isolated freshly seeded profile; all three main-memory paths retained Recall@5 1.000 |
| Chrome UI preview | Real browser with synthetic IPC: Settings, file search, accept/decline onboarding, Quick Find PDF passage; not native end-to-end evidence |
| Files and workspace browser journeys | 18 passed in light/pointer and dark/keyboard modes, including 6 new Files checks for opt-in/decline, PDF filtering/passage and code scope |
| Full browser suite | 24 passed, 2 Review checks failed expecting the removed `Summary for` heading; both failures reproduced on untouched main `64e45609` |
| Full `make test` | Script checks, typecheck, frontend tests and build passed; native library finished with 2,158 passed, 5 Mail-import failures, 40 ignored |
| Native integration targets, run separately | 23 suites, 128 passed, 0 failed, 4 ignored |
| Static checks | Changed native Files files pass rustfmt; plist lint, shell syntax, and `git diff --check` pass |

Rust commands used `CARGO_BUILD_JOBS=2`, one build at a time. The full gate required Python 3.12 and the pinned `scripts/audit/requirements.txt`, installed into a disposable virtual environment. The default Python 3.11 failed on existing f-string syntax; plain Python 3.12 lacked the audit dependencies.

All five Mail failures reproduce on an exported, untouched main `64e45609`: `default_mailbox_scope_bulk_filter_and_discovery`, `emlx_mbox_html_only_base64_and_non_utf8`, `imports_eml_with_mime_decoding_and_strips_reply_and_signature`, `malformed_empty_huge_and_permission_denied`, and `multiple_mbox_messages_and_html_quotes`. Their fixed October 14, 2026 messages are outside the 30-day window on November 17. The Mail implementation and fixtures were not changed in this feature. Full-suite green is not claimed.

A signed/notarized release was not run. No owner vault rewrite, personal-folder indexing, or model download into the owner's profile was performed. Existing verified model files were used for native evaluation.

## Publication checklist

- Feature defaults and consent checked: release off, debug on with empty roots, explicit downloads only.
- No screen-memory migration is required; Files uses new independent stores and model assets.
- Baseline failures are recorded above and must not be hidden by changing thresholds or unrelated tests.
- Publish as a draft PR targeting main rather than merging while repository-wide gates are red. CI and review remain required.
- Rollback is to disable Files in Settings; forgetting the independent index and model is separately explicit. Reverting the feature does not require rewriting memory vectors.

## Anti-bloat review

Verdict: approve with bounded operational follow-ups. The substantial engine is reused FNDR code rather than a second new storage architecture. The native reader owns macOS extraction; the separate helper is necessary because the main app's linked llama.cpp cannot load `gemma-embedding2`. No generic retrieval framework or memory adapter was added.

The bundle overlay avoids requiring a model-helper build for ordinary Cargo tests. It must remain the base external-binary list plus the model helper; a drift test enforces that relationship. Local packaged builds must explicitly use the overlay as documented in the README.

## Remaining limits

- Native model/extraction verification is on the current Apple Silicon Mac, not macOS 13, Intel, a fresh-machine install, or a signed distributable. The model helper targets macOS 14.
- The shared helper deadline is 120 seconds. Slow CPU inference can time out; the appropriate per-operation limits need measurement beyond the current Metal fixtures.
- Source metadata and policy are rechecked, but replacement between validation and an external helper opening a path remains a narrow filesystem race.
- The tiny-corpus cosine threshold is supported by the committed regression fixtures, not calibrated on a broad personal corpus.
- Installed DigUp findings are partial reverse-engineering evidence. Source-guided reconstruction and passing FNDR fixtures do not establish complete equivalence with every installed DigUp feature.

The open compatibility question is whether to support the model helper on macOS 13 or explicitly require macOS 14 for Files.
