# Home rebuild: one work-set engine for Home and Notch Do

Home opens the places of a thread, a task due within 72 hours, a saved set or a routine through the same `resolve_work_set` and `open_work_set` that Notch Do uses (ADR 027). The original W04 checks below used the browser preview, not real captures.

## October 14 Home follow-up

The landing card is compact enough to show recent work on the first desktop screen. Recent work now precedes deadlines, a routine offer and saved sets. Continue reopens a thread or task's latest source directly; Open all remains a separate action. FNDR shows the typed reopen outcome, including when there is no target. The arrangement option is at the end of Home. The scroll reveal component and its blur were removed because they hid actionable content.

| Check | Result and limit |
| --- | --- |
| HomeHero, ResumeWork, WorkSetOpener and HomeDeadlines component tests | 43 passed. IPC was faked at the wrapper, so the test proves the actions and displayed results, not macOS reopening. |
| Full frontend suite, run alone | 108 files and 843 tests passed. An earlier run overlapped typecheck and build and had two unrelated 5 second timeouts; both files then passed alone, followed by this clean full run. |
| Typecheck and production frontend build | Passed. The build still reports its existing large main-chunk warning. |
| Browser preview at 1280 by 720 in light and dark themes | The first recent-work card, Continue and its separate Open all action were visible on the first screen. A preview no-target reopen showed its explicit message. |
| Browser preview at 390 by 844 | Speak retained its visible label, typed Search and Submit fit in the query field, actions wrapped, and document scroll width equaled viewport width. |
| First native app attempt | The current debug binary started with `FNDR_DATA_DIR` pointed at the paused synthetic office-pm profile. Computer use then attached to an older bundled FNDR app instead of that process. The bundle used the default owner profile and briefly captured screen context before it was quit. Its window was covered by macOS authentication at first; no Home or reopen result from the current binary was verified. The owner vault was not edited or cleaned up. |
| Isolated native bundle before the follow-up fix | `FNDR Home QA.app` had the distinct `com.fndr.app.qa-home-resume` identifier; the single running process path and startup log confirmed the paused synthetic office-pm profile. The packaged `tauri://localhost` Home showed recent work and Continue on the first native window. Clicking Continue on the synthetic Product team thread returned `zoom.us is no longer installed.` from real `reopen_memory`. Its line was in the accessibility tree but below the visible edge, prompting the inline-feedback change. On quit, this debug process exited with a `ggml-metal-device.m` assertion; the process stopped. This run preceded the open-time visibility fix. |
| Rebuilt isolated native bundle | The bundle identifier, sole process path, and `FNDR_DATA_DIR` log again confirmed the synthetic office-pm profile. On packaged `tauri://localhost`, Continue on Product team returned `zoom.us is no longer installed.` from the real backend. The line appeared beside Continue within the visible native window and in the accessibility tree. On quit, this debug process again exited with the `ggml-metal-device.m` assertion (exit 134). The process stopped; shutdown stability remains open. |
| Native success path | A second isolated run selected Continue on the synthetic Q3 metrics review thread. The real backend returned `opened`, and Home's accessibility tree showed `Opened the latest source` beside Continue. Google Chrome opened; the destination page's content was not checked. The debug app again exited with the same ggml Metal assertion on quit. |
| Open-time visibility and UI feedback | A real-store Rust test with a fake Mac opener left a quarantined record out before calling the opener. Frontend tests cover the typed `unavailable` message and its alert placement beside Continue. Native refusal through the UI is not yet verified. |
| Office PM retrieval gate | `make qa-retrieval-check PERSONA=office-pm` reseeded its synthetic profile and passed. Search, ask and retrieve Recall@5 were each 0.950, up from the 0.900 reference. The report still counts one of eight no-match queries as strong and four of 33 real queries as weak; those counts are reported, not gated. |
| After integration with current main | `npm test` and `npm run typecheck` passed after the latest merge. `cargo test --lib workset::` passed 36 tests with one ignored; `cargo test --lib reopen` passed 71 tests. An earlier combined tree passed 108 files and 847 frontend tests. |

Do not treat the browser fixture as proof of the native reopen path.

## What is on Home, top to bottom

| Part | File | Data | Rule it keeps |
| --- | --- | --- | --- |
| Pick up where you left off | `src/app/ResumeWork.tsx` | `resumeWork`, `resolveWorkSet(thread.title)` | Continue reopens the latest source directly. Open all, View latest source and Save as set are separate actions; the set must share memories with the thread's evidence. |
| Due soon | `src/app/HomeDeadlines.tsx` | `getTodos`, `resolveWorkSet(task.title)` | Open, accepted tasks (suggestions left out) due within 72 hours or missed by under a day, at most 3. A set counts only when it holds the task's own memories (`pickSet`); otherwise the card says nothing is saved to reopen. |
| Routine offer | `src/app/RoutineOfferCard.tsx` | `routineOffers`, `dismissRoutineOffer` | Shown only for an offer with `dueNow`; Start is a tap; Not now hides it for today. |
| What changed | `src/app/ThreadChanges.tsx` | `whatChangedSince(key)`, `markThreadSeen(key)` | One line ("3 new pages, 1 task since yesterday"), up to 5 names per kind when opened. Reading marks nothing; opening the list or Dismiss marks the thread seen once. |
| Your sets | `src/app/HomeSets.tsx`, `src/app/SaveAsSet.tsx` | `listNamedSets`, `saveNamedSet`, `deleteNamedSet` | Delete takes a second tap; a refused name shows the backend's sentence. |
| Open all | `src/app/WorkSetOpener.tsx` | `openWorkSet`, `completeTodo`, `reopenMemory` | Never on load. Up to 3 items open on the tap; more list every item and wait for "Open N items". Items open one call at a time so Stop works between them. Each row reads FNDR's typed outcome: Opened, Moved (opened from its new place), Needs permission (actions off, Private Mode, or a permission named in the detail), Not opened with the reason. After a task's set: Mark this task done. Continue is available before Open all when a source exists. |
| Open places side by side | `src/app/ResumeWork.tsx` | none, per visit | Off by default. On: a set opens in one `open_work_set` call with a layout by count (1 maximize, 2 left and right, 3 thirds, 4 or more 2x2), so Stop cannot fall between items; the result says when windows stayed put. |

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
| Tab order, 1280 px light | real Chromium on the Vite preview at the original W04 check | historical order before the October 14 follow-up |
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

- The packaged app's `open_work_set` path, a layout actually arranging windows, and Accessibility permission prompts. WKWebView Home and real `reopen_memory` outcomes were checked with the isolated synthetic QA bundle.
- Real data: whether `resolve_work_set(thread.title)` returns the set that holds the thread's memories on the owner's vault, and how often Home shows "nothing saved to reopen".

Open question: does resolving by title pick the right set often enough on real threads, or should Home pass the thread's evidence ids to the engine directly?
