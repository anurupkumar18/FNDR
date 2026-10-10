# Home rebuild: one work-set engine for Home and Notch Do, checked in the browser preview only

Home now opens the places of a thread, a task due within 72 hours, a saved set or a routine through the same `resolve_work_set` and `open_work_set` that Notch Do uses (ADR 027). Nothing here has run in the packaged app or against real captures.

## What is on Home, top to bottom

| Part | File | Data | Rule it keeps |
| --- | --- | --- | --- |
| Arrange side by side | `src/app/ResumeWork.tsx` | none, per visit | Off by default. On: a set opens in one `open_work_set` call with a layout by count (1 maximize, 2 left and right, 3 thirds, 4 or more 2x2), so Stop cannot fall between items; the result says when windows stayed put. |
| Routine offer | `src/app/RoutineOfferCard.tsx` | `routineOffers`, `dismissRoutineOffer` | Shown only for an offer with `dueNow`; Start is a tap; Not now hides it for today. |
| Due soon | `src/app/HomeDeadlines.tsx` | `getTodos`, `resolveWorkSet(task.title)` | Open, accepted tasks (suggestions left out) due within 72 hours or missed by under a day, at most 3. A set counts only when it holds the task's own memories (`pickSet`); otherwise the card says nothing is saved to reopen. |
| Pick up where you left off | `src/app/ResumeWork.tsx` | `resumeWork`, `resolveWorkSet(thread.title)` | Open all, View latest source, Save as set per thread; the set must share memories with the thread's evidence. |
| What changed | `src/app/ThreadChanges.tsx` | `whatChangedSince(key)`, `markThreadSeen(key)` | One line ("3 new pages, 1 task since yesterday"), up to 5 names per kind when opened. Reading marks nothing; opening the list or Dismiss marks the thread seen once. |
| Your sets | `src/app/HomeSets.tsx`, `src/app/SaveAsSet.tsx` | `listNamedSets`, `saveNamedSet`, `deleteNamedSet` | Delete takes a second tap; a refused name shows the backend's sentence. |
| Open all | `src/app/WorkSetOpener.tsx` | `openWorkSet`, `completeTodo`, `reopenMemory` | Never on load. Up to 3 items open on the tap; more list every item and wait for "Open N items". Items open one call at a time so Stop works between them. Each row reads FNDR's typed outcome: Opened, Moved (opened from its new place), Needs permission (actions off, Private Mode, or a permission named in the detail), Not opened with the reason. After a task's set: Mark this task done. After any set with a source: Continue where you left off. |

Pure rules live in `src/app/homeWorkSet.ts` with their tests.

## Decision: no `upcoming_deadlines` command

The deadline card resolves each task's title with `resolve_work_set`, which already raises a thread whose task is due within three days and seeds a set from a task's memory when no recent thread holds it (`workset/rank.rs`). A new command would have been a second path to the same ranking. Cost: one `resolve_work_set` per card on Home load (at most 3, no model call, no opening).

## Audit finding 11, greeting and placeholder

`get_fun_greeting` names the part of the day on hours 4, 12, 16 and 20; `HomeHero` used 12, 17 and 21 for its own subtitle and placeholder, so 16:00 to 17:00 read "Good Evening" over "this afternoon", and the preview's greeting without "!" was used whole. Home now takes one daypart from `now` on the backend's hours and keeps the IPC greeting only when it names the same part. Finding 12 (preview could not load recent work) is closed by the `resume_work` preview stub.

## Checks

| Check | Kind | Result |
| --- | --- | --- |
| `npx vitest run src/app/homeWorkSet.test.ts WorkSetOpener ThreadChanges HomeDeadlines HomeSets ResumeWork HomeHero` | unit and component, IPC faked at `invoke` | pass |
| `ipcDrift.test.ts` | repo test | `markThreadSeen`, `whatChangedSince` and the five named-set wrappers left the baseline |
| Tab order, 1280 px light | real Chromium on the Vite preview | hero, arrange option, Start, Not now, Open all (deadline), Open Vault, Refresh, change line, Dismiss, Open all, View latest source, Save as set, View source, second thread, Open, Delete: the visual order |
| Focus ring | same | 2 px accent outline on every Home control (`after-keyboard-focus.jpg`) |
| Contrast of secondary text on cards | same, computed colors | 4.12:1 in light before, now about 7.3:1 light and 7.2:1 dark |
| States | same, `?home=empty|error|loading` | each renders its own copy (`after-empty.jpg`, `after-error-dark.jpg`, `after-loading.jpg`) |
| 390 px dark | same | no horizontal scroll, actions wrap (`after-dark-narrow.jpg`) |
| Reduced motion | same, `prefers-reduced-motion: reduce` | Home adds no motion; the hero already honors it |

Screenshots: `home-rebuild/before-light.jpg`, `before-dark-narrow.jpg` (commit e14cf77, recent work failing to load and "Good evening" over "tonight") and the `after-*.jpg` files.

## Critique notes kept open

- With nothing due, the Due soon card still takes a row. It says so plainly; collapsing it into a line is a later call.
- The arrange option applies to every set on Home for the visit. Remembering it is a settings question, not decided here.
- A focusable `g` from the hero's liquid surface takes one Tab stop before Speak. It predates this work.

## Not verified

- The packaged app: WKWebView rendering, real `open_work_set` openings, a layout actually arranging windows, and Accessibility permission prompts.
- Real data: whether `resolve_work_set(thread.title)` returns the set that holds the thread's memories on the owner's vault, and how often Home shows "nothing saved to reopen".

Open question: does resolving by title pick the right set often enough on real threads, or should Home pass the thread's evidence ids to the engine directly?
