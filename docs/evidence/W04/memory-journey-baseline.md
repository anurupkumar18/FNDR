# Memory Journey baseline

Status: automated foundation verified; six native journeys and human approval are pending.

This report intentionally contains no captured text, prompts, screenshots, memory
identifiers, absolute paths, or exported bundles. Real `.fndrjourney.zip` bundles
remain local and ignored. The baseline is report-only: no capture, extraction,
embedding, or ranking thresholds were tuned while adding the inspector.

## Automated evidence

| Contract | Evidence |
| --- | --- |
| Disarmed and one-shot lifecycle | Unit tests verify no disarmed files and exactly one attempt per arm. |
| Privacy and cancellation | Unit tests verify pre-capture skips save no content, post-OCR privacy rejection redacts content, and Delete all removes an active partial bundle. |
| Bundle safety | Unit tests verify owner-only permissions, relative hashed artifacts, atomic publication, restart cleanup, deterministic deletion, six-bundle retention, 24-hour configuration, and the 128 MiB cap. |
| Embedding contracts | Unit tests reject zero and non-finite vectors and verify that full float arrays are not serialized. |
| Scoped model evidence | Unit tests verify the selected task-local journey scope does not leak. |
| Retrieval parity | Normal and explained ranking return the same IDs and scores; the explained path records route latency/candidates, fusion inputs, rerank deltas, exclusions, final ranks, and embedding reasons. |
| Synthetic replay | Six authored records persist to temporary LanceDB, reconstruct with unavailable live stages marked honestly, and retrieve through the production Search path. |
| Frontend truthfulness | Component tests verify event-backed state, unavailable-stage rendering, artifact metadata, and production Search IPC. No timer advances a stage. |
| Release boundary | Production frontend output contains none of the Memory Journey UI, event, command, or export strings; Rust commands and recorder are guarded by `debug_assertions`. |

## Native six-journey worksheet

Use a separate QA profile. For each case, arm **Record next capture**, allow one
capture attempt, then run one exact query, one paraphrase query, and one grounded
Ask question. Case 6 also receives one unsupported question. Export the bundle
locally and record only sanitized conclusions here after review.

| Case | Capture outcome | Exact | Paraphrase | Grounded Ask | Human label | Agent label |
| --- | --- | --- | --- | --- | --- | --- |
| 1. Research article | Pending | Pending | Pending | Pending | Pending | Pending |
| 2. Editor plus terminal error/fix | Pending | Pending | Pending | Pending | Pending | Pending |
| 3. Project owner/deadline/decision/next step | Pending | Pending | Pending | Pending | Pending | Pending |
| 4. Conversation commitments/open questions | Pending | Pending | Pending | Pending | Pending | Pending |
| 5. Layout-sensitive dashboard/table | Pending | Pending | Pending | Pending | Pending | Pending |
| 6. Low-signal/protected/unsupported | Pending | N/A if skipped | N/A if skipped | Unsupported refusal pending | Pending | Pending |

## Approval boundary

A stronger review model may propose required facts, forbidden claims, relevant
IDs, and quality labels from a user-supplied bundle. They become gold only after
the user approves or corrects them. Until then:

- privacy, dimensions, finite/non-zero vectors, parse, storage, citations, and
  refusal are observed but are not promoted beyond their existing invariants;
- retrieval is not compared against a frozen real-memory baseline;
- performance numbers are advisory and machine-specific;
- VS-29 remains blocked on six approved cases rather than inventing thresholds.

## Native QA handoff

Manual action: open **Engine diagnostics → Memory Journey**, enter a safe label,
choose **Record next capture**, then foreground the authored test content until
the next capture attempt completes.

Expected result: the state changes only on backend events, one bundle appears,
observed stages show measured outcomes/durations, missing stages say
`unavailable`, and artifact metadata lists relative names, sizes, and short
hashes. Send a screenshot of the inspector with private text hidden and the
exported bundle only through an approved private channel.

Still unverified: native macOS capture/OCR/model behavior, real-memory Search and
Ask usefulness, bundle review labels, instrumentation overhead under a six-case
run, and the frozen regression baseline.
