# PRD: Privacy-safe activity traces

## Problem

FNDR performs meaningful work across capture, retrieval, local models, speech,
downloads, Screen Guide, and agent workflows. A generic spinner or a fixed
"thinking" message does not tell a person which subsystem is active, what has
actually completed, whether FNDR is waiting, or where a failure occurred. It
also gives developers too little evidence when a native workflow behaves
differently from the visible UI.

Exposing raw logs or model reasoning would not solve that problem safely. Logs
can contain private desktop context and implementation noise; a model's private
reasoning is neither a reliable operational record nor appropriate UI.

## Goal

Show a concise, evidence-backed **activity trace** beside each meaningful
asynchronous or multi-stage workflow. The trace names the current operation and
owning subsystem, and can disclose a short history of steps that actually
occurred. It helps users understand FNDR's state and gives developers a shared
vocabulary for diagnosis without exposing captured content.

## Users / actors

- A person waiting for FNDR to capture, retrieve, transcribe, download, or
  answer.
- A developer or tester identifying the last confirmed boundary before a
  failure.
- Frontend workflow owners that observe local state and request/result
  boundaries.
- Native subsystems that emit typed status events or return bounded result
  metadata.

## Proposed behavior

- Place a compact trace next to the control or result whose work it explains.
  Show the current step, actor, status, and duration when available.
- Let an interactive surface disclose the ordered observed steps, their
  evidence origin, event time, and bounded non-content detail. Passive or
  click-through surfaces may show only the current step.
- Start a trace only when an operation has actually begun. Add or update a step
  only when its producing boundary is observed. Never prefill expected or
  future stages.
- Preserve terminal states long enough to explain the result, then replace or
  clear them when the owning workflow starts again or leaves its normal UI
  lifecycle.
- Use the states `running`, `waiting`, `completed`, `degraded`, `failed`, and
  `cancelled`. Failure detail is a bounded category or safe remediation, not a
  raw exception or log line.
- Present this as **Activity** or **What FNDR is doing**, not as a model's
  "thoughts." Expanded UI explicitly says that private model reasoning and
  captured content are not shown.

## Evidence origins

Every visible step identifies the observable boundary that produced it:

| Origin | Meaning |
| --- | --- |
| Backend event | A typed native event emitted at a real state change. |
| Backend snapshot | A real status value returned by an existing initial or user-requested read. |
| Frontend event | A renderer-owned transition that occurred, such as a debounce completing or a recorder stopping. |
| Request boundary | The frontend actually invoked an IPC operation. It does not claim that unreported backend internals ran. |
| Verified result | A returned result supplied safe metadata such as count, route, or terminal state. |

Request boundaries and frontend events may explain workflows whose internals do
not yet emit typed events, but they must not be dressed up as backend stages.

## Privacy contract

Allowed trace data is intentionally narrow:

- subsystem or process name;
- logical model identifier, never its filesystem location;
- lifecycle status and bounded reason category;
- event time and duration;
- aggregate counts and non-content route/mode names;
- evidence origin.

An activity trace must never contain:

- model chain-of-thought, hidden reasoning, or scratch work;
- prompts, questions, transcripts, OCR, captured text, or generated answers;
- window/document titles, URLs, filenames, or file paths;
- screenshots, microphone audio, memory contents, or retrieved snippets;
- API keys, tokens, credentials, or other secrets;
- raw backend logs, exception dumps, command output, or unbounded error text.

The trace is transient presentation state. It remains in memory, is bounded to
the current visible workflow, and is never written to the memory store,
analytics, telemetry, or a trace file. This is distinct from FNDR's internal
`llm_traces.jsonl` developer telemetry.

## Functional requirements

- FR1: Every meaningful asynchronous, long-running, background, model, or
  multi-stage user workflow has a colocated activity display.
- FR2: Every step includes a concise label, an actor/subsystem, a lifecycle
  status, an evidence origin, and an observed time.
- FR3: Repeated progress for the same stage updates a stable step rather than
  growing an unbounded event list.
- FR4: A trace contains observed steps only; speculative, predicted, or
  hardcoded future stages are prohibited.
- FR5: Backend-owned phases use typed events when available. Frontend-only
  workflows may show actual local transitions, IPC boundaries, and verified
  result metadata without claiming unseen backend work.
- FR6: Existing event streams and workflow state are reused. Trace adoption
  adds no fixed-interval polling.
- FR7: The current step remains understandable without opening details, and
  status is conveyed with text rather than color or animation alone.
- FR8: Failure and degraded states name a safe category and useful next action
  when one is known, without copying raw logs.
- FR9: Trace state is bounded, in memory, and scoped to its owning workflow; it
  is not persisted or combined into a global user-history feed.
- FR10: Producers enforce the privacy contract before passing labels or detail
  into shared presentation code.

## Placement and scope

The primary pattern is inline and colocated:

- retrieval activity sits beside the active search or answer;
- capture/model health sits with capture and model controls;
- Screen Guide activity appears in its panel or transient overlay;
- download, speech, meeting, and agent activity appears within the surface that
  initiated or owns that work.

Static data, instantaneous controls, and dashboards that already present their
final state do not need a decorative trace. A single global activity center,
cross-session history, raw-log viewer, and user-configurable trace pipeline are
out of scope for this slice.

## Data flow

```text
typed backend event / existing backend snapshot
                    OR
observed frontend transition -> IPC request -> verified result metadata
                              |
                              v
             privacy-safe step mapping in owning workflow
                              |
                              v
        bounded in-memory trace -> colocated summary + optional details
                              |
                              v
              replaced/cleared with workflow lifecycle
```

## Non-functional requirements

- Privacy: trace payloads contain no captured or user-authored content and do
  not create a new persistence or egress path.
- Reliability: the UI distinguishes waiting, degraded, failed, cancelled, and
  completed work, and never claims an unobserved phase completed.
- Performance: reuse push events and current state; do not add polling or copy
  high-volume raw logs into React state.
- Accessibility: use a polite live status for the latest step; details use an
  ordered list and an explicit disclosure control; honor Reduce Motion.
- Maintainability: share the display vocabulary and component, while keeping
  process-specific event mapping with the workflow that owns the facts.

## Acceptance criteria

- [ ] A trace displays the last real observed step, responsible subsystem,
      status, evidence origin, and safe elapsed time when available.
- [ ] Expanded details contain only steps produced by real events, snapshots,
      frontend transitions, IPC calls, or result metadata.
- [ ] No loading state predicts future stages or advances on a timer.
- [ ] Search, capture, model, voice, Screen Guide, agent, meeting, and download
      owners use the shared contract as their workflows are adopted.
- [ ] A request boundary never masquerades as proof of unreported backend work.
- [ ] Tests reject representative sensitive values and prove terminal,
      degraded, failed, cancelled, replacement, and stable-step update states.
- [ ] Screen-reader output announces useful state without replaying the entire
      trace on each update.
- [ ] Trace adoption introduces no new polling, persistence, network egress, or
      raw-log rendering.

## Test plan

- Unit-test trace creation, stable-step replacement, terminal states, bounds,
  duration formatting, and invalid/missing times.
- Component-test collapsed and expanded presentation, text alternatives,
  disclosure behavior, and privacy copy.
- At each adopted workflow, test the mapping from its real event/state/result
  contract to safe steps, including error and cancellation.
- Keep native QA separate from renderer evidence: a mocked event proves the UI
  mapping, while a native run proves that the producing subsystem emitted it.
- Review rendered labels/details against a sensitive-value fixture containing
  prompts, OCR, titles, URLs, paths, secrets, and raw errors.

## Rollout

Adopt the contract incrementally at active product surfaces, starting with the
workflows where hidden native work most affects trust and debugging. Existing
spinners may remain only when they accompany, rather than contradict, the real
trace. No storage migration is required.

## Risks

- A frontend may accidentally infer backend progress from a generic loading
  flag. Evidence-origin labels and workflow tests mitigate that risk.
- Safe-looking exception or model output can still contain user content. Only
  enumerated reason categories and producer-authored labels are accepted.
- Product-wide adoption could become visual noise. Colocation, compact default
  summaries, and the meaningful-work threshold keep the pattern focused.
- Adding a central event bus or durable history would increase complexity and
  privacy risk. This contract deliberately does neither.
