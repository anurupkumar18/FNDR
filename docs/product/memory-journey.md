# Memory Journey quality program

## Purpose

Memory Journey is a debug-build-only inspector for following one capture attempt
through FNDR's real capture, extraction, embedding, storage, retrieval, and
presentation paths. It exists to make pipeline quality measurable before any
capture threshold, prompt, embedding, or ranking change is attempted.

It is not a second capture pipeline, a user activity history, model
chain-of-thought, analytics, or a new memory source. A normal release retains
privacy-safe activity traces but contains no raw Memory Journey commands or UI.

## Shared understanding

- One explicit **Record next capture** arm is consumed by exactly one capture
  attempt, including an attempt that is skipped by privacy or admission policy.
- **Inspect existing memory** reconstructs only evidence that is persisted on
  the selected memory. It marks every other stage `unavailable`; it never
  presents an inferred stage as observed.
- Search and Ask evaluations call the production paths. Explained and normal
  execution must return identical result IDs, scores, cards, citations, and
  refusal decisions.
- Operational prompts, outputs, validators, timings, and process boundaries may
  be recorded. Hidden model reasoning and chain-of-thought are unavailable and
  must never be requested or represented.
- MiniLM 384-d, BGE 1024-d, and CLIP 512-d vectors are intentionally separate
  spaces. The inspector verifies each vector's declared role, model, space,
  dimension, source hash, norm, finite/zero state, and vector hash; it does not
  force dimensions to match.
- The first six journeys are a report-only baseline. No quality tuning or new
  merge gate is justified until their expected facts, forbidden claims,
  relevant IDs, and usefulness labels are reviewed and approved.

## Developer workflow

The Engine Diagnostics panel exposes the following controls in debug builds:

1. **Record next capture** arms one attempt, optionally with a non-sensitive
   label.
2. **Inspect existing memory** creates a reconstructed journey for an explicitly
   selected memory.
3. The inspector renders only observed stage records, durations, outcomes, and
   artifact availability.
4. A tester can add an exact query, a paraphrase query, and a grounded question,
   then run Search or Ask through the production path.
5. A tester can export one `.fndrjourney.zip` or explicitly delete one or all
   local journeys.

At most one journey may be armed or recording. Completed bundles are retained
for no more than 24 hours, with at most six bundles and 128 MiB across partial
and complete data.

## Manifest contract

`MemoryJourneyManifestV1` is the canonical, versioned manifest. It contains:

- schema version `1`;
- `mode`: `live` or `reconstructed`;
- `state`: `armed`, `capturing`, `stored`, `querying`, `complete`, `failed`, or
  `expired`;
- opaque journey and optional memory identifiers;
- ordered stage records with status, duration, safe outcome, and artifact refs;
- relative artifact references with hashes, sizes, and availability;
- production query runs and human/agent scorecard drafts.

Exported JSON contains no absolute filesystem paths or automatic upload
destination. Artifacts use relative paths under a backend-owned journey
directory. Directories use owner-only permissions, writes are made to partial
data and atomically published, and Reveal/Export/Delete are backend-owned.

Bundles and their contents never enter LanceDB, Memory, embeddings, analytics,
model context, or an automatic upload path. Real bundles, screenshots, and
recognized text remain ignored local artifacts. Only approved synthetic/public
fixtures or sanitized derived labels may be committed.

## Recorded stages

| Stage | Evidence recorded when observed |
| --- | --- |
| Admission | Target app class; privacy/surface decision; dedupe decision. |
| Frame | One PNG; dimensions; blank-frame metrics; pixel hash. |
| Text source | Accessibility, browser-semantic, OCR, or visual path selected. |
| OCR | Positioned lines; confidence; block count; latency; raw recognized text. |
| Cleanup | Cleaned text; dropped-line categories; preservation ratio; diff. |
| Extraction | Model/task/version; prompt; output; token counts; latency; parse and validator results; unsupported-field warnings; fallback. |
| Embedding document | Primary, snippet, support, chunk-source, and visual-semantic inputs. |
| Vector contracts | Role; model; vector space; dimension; source hash; norm; zero/non-finite counts; vector hash; pairwise similarity. Full float arrays are not rendered. |
| Storage | Memory ID; table; insert/merge/skip outcome; manifest; chunk count; review status; latency. |
| Retrieval | Query plan and expansions; route candidates and latency; fusion inputs; rerank deltas; exclusions; final ranks; embedding penalties; top-1 stability. |
| Presentation | Memory-card grouping; surfaceability; title/summary source; evidence IDs; Ask citations; refusal; answer latency. |

Missing stages in reconstructed mode are explicitly `unavailable`.

## Evaluation scorecards

Three scorecards remain separate so one audience cannot hide regressions for
another.

### Pipeline integrity

- privacy/admission result;
- OCR character error rate and confidence;
- cleanup preservation and noise removal;
- extraction parse and validator result;
- embedding dimension, norm, freshness, and provenance;
- exactly-one storage outcome and correct merge/skip behavior;
- stage latency, memory pressure, and instrumentation overhead.

### Human usefulness

- whether the screen should become a memory;
- understandable title, summary, topic, and timeline;
- important facts preserved without invented claims;
- ability to reopen or recover the work;
- exact and paraphrase retrieval;
- acceptable delay and failure explanations.

### Agent grounding

- required-fact recall and unsupported-fact rate;
- structured-field precision and coverage;
- Recall@5, MRR@10, nDCG@10, and top-1 stability;
- citation precision and evidence completeness;
- correct refusal for unsupported questions;
- stable IDs, schemas, and machine-readable provenance.

After six cases are approved, privacy, dimension, non-zero-vector, parse,
storage-integrity, citation, and refusal invariants may become blocking.
Retrieval becomes blocking only relative to the frozen approved baseline.
Performance remains advisory until repeated runs establish stable thresholds
for the test machine.

## Initial six-case baseline

1. Research article with headings and a distinctive fact.
2. Code editor plus terminal error and fix.
3. Project document with owner, deadline, decision, and next step.
4. Conversation with names, commitments, and unresolved questions.
5. Visual dashboard or table where layout matters.
6. Low-signal, protected, or unsupported content that should skip or refuse.

Each successful memory receives one exact query, one paraphrase query, and one
grounded question. Case six also receives one unsupported question. A stronger
review model may draft labels, but a person must approve or correct them before
they become gold.

## Verification contract

- Disarmed recording creates no files and cannot change capture results.
- One arm consumes one attempt.
- Privacy/pre-capture skips save no pixels or OCR.
- Cancellation removes partial data.
- Reconstructed journeys never claim unavailable evidence was observed.
- Scoped model tracing records content only for the selected journey.
- Explained Search/Ask ranks, scores, and evidence describe the exact results
  returned by that same call. Separate requests may differ if the active model
  backend changes after a runtime failure.
- Vector dimension, zero, non-finite, and stale-source mismatches are detected.
- Retention, size cap, export, restart cleanup, and deletion are deterministic.
- Production builds exclude raw Memory Journey commands and UI.
- Frontend stages are event-backed, accessible, and never advance on timers.
- Synthetic journeys use temporary storage and the production search path.
- Existing capture, retrieval, vault-health, frontend, Rust, and `make test`
  gates remain green.

Native QA reports what changed, why, automated evidence, one manual action, the
expected result, the screenshot or bundle to return, and what remains
unverified. Live recording uses a separate QA profile; reconstructed inspection
may read an explicitly selected normal-profile record.

## Delivery slices

- **VS-27 — Record one memory journey through the real pipeline:** debug
  recorder, lifecycle, capture/storage stages, inspector UI.
- **VS-28 — Establish the six-journey human and agent baseline:** production
  Search/Ask explanations, manual review, approved labels, baseline report.
- **VS-29 — Stage approved pipeline quality checks as merge gates:**
  deterministic gates, frozen-baseline comparison, regression report.

VS-14 consumes live capture findings, EM-11 consumes vector-contract findings,
PX-07 verifies frontend traces at the same boundaries, and VS-04 consumes the
approved retrieval gate. These links are dependencies, not duplicate work.
