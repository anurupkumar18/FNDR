# PX-07 evidence: activity traces follow real process boundaries

Date: 2026-10-05. Scope of this file: the automated, renderer-only part of
PX-07. The native run is pending the owner (see the last section).

## Call-site count

`grep -rn "<ActivityTrace\b" src --include='*.tsx'` over non-test files:

- **34 mounted call sites in 23 files.**
- Every one has a row in `docs/product/activity-trace-boundaries.md` with step
  id, visible label, actor, evidence kind, source boundary, completion
  boundary, cancellation or supersession behavior, failure behavior, and the
  privacy fields allowed.
- The ticket's example list is out of date: `src/app/HomeHero.tsx` no longer
  mounts a trace, and fifteen further files do.

## Violations found and fixed

| # | Where | Violation | Fix |
| --- | --- | --- | --- |
| 1 | `src/shared/hooks/useSearch.ts` | A renderer timeout was recorded as evidence `ipc-boundary` with actor "FNDR search service" although the backend had not answered. A backend error whose message contained "timed out" was reported as "Client timeout". | Dedicated error class for the renderer timer. Timeout is evidence `frontend-event`, actor "FNDR search". Other rejections are backend failures. Failing tests were written first (two failed, six passed), then the fix. |
| 2 | `src/domains/ask/AskPanel.tsx` | Same mislabel for the 60 second renderer timer, matched by the exact message "timeout". | Dedicated error class, evidence `frontend-event`, actor "Ask FNDR". Test written first (one failed, four passed), then the fix. |

No violation was found for invented progress, future stages, percentages, or
model thinking. No producer put query text, transcripts, OCR, prompts, answers,
titles, URLs, paths, or raw errors into a step; `activityTraceBoundaries.test.ts`
now enforces that for every producer.

## New automated tests

| File | Tests | Proves |
| --- | --- | --- |
| `src/shared/hooks/useSearchActivityBoundaries.test.tsx` | 8 | Debounce stays `waiting` and no backend call is made until it settles; retrieval stays `running` while the deferred call is pending; success, failure, and renderer timeout update the same `retrieval` step; supersession ignores a stale answer; a cleared query drops the trace; no query, title, snippet, or raw error text appears. |
| `src/domains/ask/AskPanelActivityBoundaries.test.tsx` | 5 | Request stays running while pending; timeout and backend failure update the same step with the right evidence; stale answer ignored; no question, answer, or raw error text. |
| `src/shared/activity/voiceActivityTraceBoundaries.test.ts` | 8 | Each voice step stays running until its own event; success, degraded, and failed outcomes settle the same request step; bounded and detail-free. |
| `src/domains/screen-guide/__tests__/ScreenGuideActivityBoundaries.test.ts` | 9 | Stage stays running until a later backend stage; failure and cancellation update the same stable step; supersession by generation; frontend voice boundaries; no target app, title, URL, or path. |
| `src/shared/activity/activityTraceBoundaries.test.ts` | 5 | Source scan of every producer: only counts, logical identifiers, and fixed copy reach a label or detail. |

Existing per-surface tests (Notch, Quick Find, Autofill, Agent, Codex,
Onboarding, model download, biometric lock, Screen Guide overlay) were left
as they were and still pass.

## Command output

`npm run typecheck`:

```
> fndr@0.3.0 typecheck
> tsc --noEmit
(exit 0, no diagnostics)
```

`npm test -- activityTrace useSearch voiceActivityTrace ScreenGuide`:

```
 Test Files  11 passed (11)
      Tests  92 passed (92)
   Duration  2.04s
```

Files in that run: ScreenGuidePanel (19), ScreenGuideOverlay (18),
ScreenGuideActivityBoundaries (9), useSearchActivityBoundaries (8),
voiceActivityTraceBoundaries (8), screenGuideState (8), ActivityTrace
component (7), activityTraceBoundaries (5), activityTrace (5),
voiceActivityTrace (3), useSearch (2). The Ask boundary test is matched by
the full run, not by that filter.

`npm test` (full):

```
 Test Files  79 passed (79)
      Tests  513 passed (513)
   Duration  9.99s
```

`make test` was not run because it includes `cargo test`, which this task was
told not to run. The renderer half of it (`npm run typecheck`, `npm test`) is
above.

## Native QA matrix

Pending owner (search, voice turn, Screen Guide turn, model download,
cancellation).

| Flow | What to confirm | Result |
| --- | --- | --- |
| One search | Steps appear in order; retrieval stays running until results appear; no step before the request | pending owner |
| One voice turn | Microphone request, recording, and transcription steps match the real prompts and recorder; no step ahead of the recorder | pending owner |
| One Screen Guide turn | Each stage appears only when its backend event fires; actors match the real subsystem (Screen Recording, Vision OCR, on-device model or ChatGPT) | pending owner |
| One model download | Percent and bytes match the backend status event; activation starts only after completion | pending owner |
| One cancellation | Interrupted or cleared work leaves no step stuck in running | pending owner |

Screenshots, if taken, may show only synthetic labels and counts.
