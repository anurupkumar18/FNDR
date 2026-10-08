# ADR 021: Privacy-safe activity traces from observed events

## Status

Accepted

## Context

FNDR has asynchronous work across capture, hybrid retrieval, local inference,
speech, model downloads, Screen Guide, meetings, and agent workflows. Many
surfaces currently reduce that work to a spinner or a generic "thinking"
message. That makes slow or failed native boundaries hard for users to
understand and hard for developers to diagnose.

Raw logs are not an acceptable UI substitute: they are noisy, unstable, and
may contain prompts, OCR, window titles, URLs, paths, or other private desktop
context. Model chain-of-thought is not an operational contract and must not be
exposed. A separate product-wide polling or telemetry system would also
duplicate the event-driven direction established by ADR 011.

## Decision

FNDR uses a shared **activity trace** presentation contract for meaningful
asynchronous, long-running, background, model, and multi-stage work.

- A trace is a bounded, ephemeral sequence of steps for one visible workflow.
  It lives in renderer memory only, is replaced or cleared with that workflow's
  lifecycle, and is never persisted as memory, analytics, or trace telemetry.
- A step is recorded only after its boundary is observed. Producers never add
  predicted stages, hardcoded future progress, or timer-driven claims.
- Each step names a concise operation, the actor/subsystem that owns it, its
  lifecycle status, observed time, and one evidence origin:
  `backend-event`, `backend-snapshot`, `frontend-event`, `ipc-boundary`, or
  `result-metadata`.
- Stable step identifiers update repeated progress instead of appending an
  unbounded stream. The shared states are `running`, `waiting`, `completed`,
  `degraded`, `failed`, and `cancelled`.
- Typed backend events are authoritative for backend phases. Existing initial
  status reads may provide a backend snapshot. Where no typed internal event
  exists, the renderer may show only transitions it actually observed, the IPC
  request boundary, and safe metadata from the returned result. It must not
  infer or invent native stages.
- Traces reuse existing event streams and local workflow state. They add no
  fixed-interval polling. New always-on status follows ADR 011 and emits at
  native change points.
- The primary UI is a compact, colocated display beside the operation it
  explains, with optional ordered details on interactive surfaces. We do not
  introduce a global activity center, central event bus, or durable activity
  history in this decision.
- Shared presentation code owns status/evidence vocabulary and accessible
  rendering. The workflow that owns the facts maps its typed events and result
  metadata into steps; a generic layer does not parse logs or guess semantics.

### Privacy boundary

Allowed metadata is limited to process/subsystem names, logical model IDs,
durations/times, aggregate counts, non-content mode/route names, lifecycle
state, and bounded reason categories.

Prompts, questions, transcripts, OCR, captured text, generated answers, memory
content, titles, URLs, filenames, file paths, screenshots, audio, credentials,
raw backend logs, exception dumps, and model chain-of-thought are prohibited.
Model identifiers must be logical names, not filesystem locations. Failure
copy is producer-authored from an enumerated safe category rather than copied
from arbitrary error text.

The UI describes this as operational **Activity** or **What FNDR is doing**,
not as access to a model's thoughts or private reasoning.

## Consequences

- Users can see the last confirmed operation, responsible subsystem, and real
  outcome without reading developer logs.
- Developers can distinguish a frontend transition, IPC invocation, native
  event, backend snapshot, and verified result when reproducing a failure.
- A trace can be less detailed when a backend lacks typed progress events. That
  limitation is truthful; adding typed native events is the path to greater
  detail.
- Workflow owners must review every label and detail value for content leakage.
  Shared UI types help consistency but cannot sanitize arbitrary strings.
- The renderer retains only small per-workflow state and receives no new idle
  traffic. Stable-id replacement prevents repeated progress from growing a
  history without bound.
- Renderer tests prove presentation and mapping only. Native QA remains
  necessary to prove that a real subsystem emitted the event and performed the
  work.

### Debug-build Memory Journey exception

The Memory Journey inspector is deliberately outside the normal activity-trace
privacy contract. After an explicit one-shot developer arm, a debug build may
persist raw content artifacts and scoped model prompt/output data for one
capture attempt so observed pipeline boundaries can be correlated. Its UI must
label the surface as private developer evidence, not model "thinking" or a
normal user activity history.

The exception is limited by the versioned contract in
`docs/product/memory-journey.md`: owner-only local storage, one active journey,
six bundles, 24 hours, 128 MiB, explicit export/delete, no automatic upload,
and no ingestion into Memory, embeddings, model context, analytics, or normal
activity traces. A release build contains no raw Memory Journey commands or UI.
This exception does not permit captured content in the always-available
privacy-safe activity trace.

## Rejected alternatives

- **Show model chain-of-thought:** misleading as an execution trace and
  incompatible with the privacy boundary.
- **Render raw application logs:** unstable, noisy, and likely to leak private
  context or paths.
- **Animate a hardcoded stage list:** looks informative but claims work that may
  never have occurred.
- **Poll every subsystem for progress:** duplicates backend work and conflicts
  with ADR 011.
- **Persist a cross-session activity history:** creates a new sensitive data
  surface without being necessary for inline feedback.
- **Build a global event framework first:** increases coupling and scope; local
  workflow mapping plus shared presentation satisfies the current need.

## Verification

- Shared unit tests cover observed-step insertion, stable-id replacement,
  terminal states, bounds, safe time/duration formatting, and empty traces.
- Component tests cover compact and expanded states, evidence labels, keyboard
  disclosure, live-region behavior, and privacy copy.
- Each adopted workflow tests its typed event/state/result mapping, including
  failure and cancellation, without asserting invented intermediate stages.
- Review fixtures include sensitive prompt, OCR, title, URL, path, key, and raw
  error values and prove that none reaches rendered trace data.
- Native QA captures the real emitted sequence for event-backed workflows and
  keeps that evidence distinct from mocked renderer tests.
