# Investigate: what was measured (2026-11-11 to 2026-11-13)

Evidence for ADR 039 and `docs/superpowers/specs/2026-11-11-investigate-prd.md`.
Every profile here is synthetic: the three persona corpora under `scripts/demo/`,
seeded on 2026-11-11 into scratch profiles with capture paused. The owner's
vault was not read.

## How each claim was checked

| Claim | Checked by |
| --- | --- |
| The loop takes the right step for each kind of case | unit tests with a fake `Tools` (`investigate::agent`, 13 tests) |
| The done rule, plan and case file | unit tests (`done` 8, `plan` 6, `case` 7) |
| The real tools on a real store, the visibility rule, events carry no memory text | `src-tauri/tests/investigate.rs` (3 tests, mock embedder, real LanceDB store) |
| The step event's wire shape, and Stop reaching only its run | unit tests in `ipc/commands/investigate.rs` (2) |
| The same path the app calls, on seeded profiles with the real embedding model | `cargo run --example investigate_run`, 29 runs, below |
| The panel | 15 component tests, and the browser preview (synthetic IPC) |
| The debug app: command registered, events reach the webview | **not run**, see "Not verified" |

## The real path on three seeded personas

`investigate::run` on a copy of each profile, default `Config`, debug build,
M1. Each request is one a person would type; none names a memory id.

| Persona | Request | Steps taken | Memories in the case | Of the project's | From another project | Recorded as done |
| --- | --- | --- | --- | --- | --- | --- |
| office-pm | Catch me up on the Smart Reminders launch | search, thread, to-dos, check | 16 | 16 | 0 | 3 |
| office-pm | What is still open for the Senior Product Designer hiring? | search, thread, to-dos, check | 9 | 9 | 0 | 0 |
| office-pm | what did we decide in the Q3 metrics review | search, thread, to-dos, check | 9 | 9 | 0 | 1 |
| office-pm | what went wrong with LL-1482 | search, thread, to-dos, check | 16 | 16 | 0 | 3 |
| office-pm | the spam bug | search, thread, to-dos, check | 16 | 16 | 0 | 3 |
| office-pm | the Mars lander budget spreadsheet | search (weak), widen (weak) | nothing found | | | |
| office-pm | the kitchen renovation quote | search (weak), widen (weak) | nothing found | | | |
| office-pm | what did I do yesterday | none | needs a topic | | | |
| knowledge-worker | what is left for CS 3500 Assignment 4 | search, thread, to-dos, check | 6 | 6 | 0 | 0 |
| knowledge-worker | catch me up on the Reconstruction essay | search, thread, probe, to-dos, check | 4 | 4 | 0 | 0 |
| knowledge-worker | the Q4 onboarding survey | search, thread, probe, to-dos, check | 4 | 4 | 0 | 1 |
| knowledge-worker | where am I on the Pantry app demo | search, thread, probe, to-dos, check | 4 | 4 | 0 | 0 |
| software-engineer | what happened with the INC-2291 latency incident | search, thread, to-dos, check | 6 | 6 | 0 | 0 |
| software-engineer | catch me up on idempotency keys for labels | search, thread, to-dos, check | 5 | 5 | 0 | 1 |
| software-engineer | partner webhook rate limiting decisions | search, thread, to-dos, check | 4 | 4 | 0 | 0 |
| software-engineer | the Postgres 16 upgrade | search, thread, probe, to-dos, check | 3 | 3 | 0 | 1 |

- 13 requests found their work: 102 memories in the case files, none from
  another project, and every case held every memory of its project.
- 2 requests about things not in memory ended as nothing found after a widened
  search. 1 request with no topic was told to name the work.
- A run took 415 to 531 ms end to end (4 or 5 steps). The first search is
  about 390 ms of that; reading a thread is 23 to 51 ms.

### The harder case: project labels removed

The corpora carry a `project` on every memory, which makes a thread easy to
find. A real vault often has none. The same requests on copies seeded with
every `project` emptied, so a thread is only the windows whose titles share a
name (ADR 031 addendum):

- 12 requests found their work: 71 memories, none from another project.
- Coverage fell to 71 of 86 of the projects' memories. The smallest cases:
  CS 3500 Assignment 4 held 2 of 6, the Pantry app demo 2 of 4, INC-2291 4 of
  6. The loop reads what the named thread joins and no more.
- The absent request still ended as nothing found.

Precision held and recall did not. A case file on a vault without work
identities is correct and can be short; its header says how many memories it
read.

## The done rule

Every pair the rule produced on the 13 found runs, judged by reading both
sentences:

| Next step | Later clause shown | Right |
| --- | --- | --- |
| Book the go/no-go meeting | "the go/no-go meeting is now booked" | yes |
| Close LL-1482 | "confirmed the LL-1482 fix is live in production with 1.8% spam placement and closed the ticket" | yes |
| Fill in the recommendation slide after LL-1482 closes | "Filled in the launch review recommendation slide" | yes |
| Build the Q3 metrics sheet | "Built the first pass of the Q3 metrics sheet" | yes |
| Clean the survey export | "Cleaned the survey export" | yes |
| Merge PR #4187 after CI | "Merged PR #4187" | yes |
| Rewrite the runbook for logical replication | "Rewrote the Postgres 16 upgrade runbook around logical replication" | yes |

7 of 7 distinct pairs are right. Recall was not measured: the corpora do not
label which steps were finished. Known misses, left on record as next steps:
"Rafael to fix DKIM alignment on the reminders subdomain" (the later memory
says the fix is live, in other words) and "Finish the recommendation slide
before the meeting" (the later memory says "Filled in").

## What the real run changed

Three faults passed the unit tests and showed only on the seeded store.

1. **An absent topic got a case file.** "the Mars lander budget spreadsheet"
   returned the Q3 metrics review: Search's strong-match bar passes a hit
   that is only close in meaning (0.383, no query word in it), and the loop
   then followed that hit's thread. A hit now enters a case only when the
   keyword route found at least half of the topic's words in it.
   `a_strong_score_without_the_topics_words_is_not_the_work` holds it.
2. **The done rule was too loose.** Asking only for the words before a
   preposition marked "Retest spam placement on staging after DNS propagates"
   from a clause about the production fix, and "Update the launch checklist
   with the rollout plan" from a clause that only said the checklist was
   updated. First pass: 7 pairs on the launch, 3 of them not supported by the
   clause shown. Every word of what the step acts on is required now; the
   launch gives 3 pairs, all right.
3. **Lines lost their punctuation.** A card's summary is clipped to about 18
   words with colons and commas removed, which also defeated the clause
   split in the done rule. A case line is now the whole stored sentence, taken
   the way the briefing quotes one (`briefing::quotable`).

## Preview

In the browser preview (`ui-preview.html`, synthetic IPC, a scripted run
standing in for the backend loop): steps appear one at a time while the run
works; the case file replaces the open trace when it ends; Open places asks
once more above three places and lists what opened; Add to To-dos turns to
Added; the counts fold to two columns at 390 px. Dark theme only.

## Not verified

- **The debug app.** The command and its event were not exercised inside a
  running FNDR window. The loop, its tools and its events ran through
  `investigate::run` on real stores, and the panel ran against the preview
  stub. Of the Tauri layer between them (`ipc/commands/investigate.rs`) the
  event shape and the stop registry have unit tests; Tauri delivering the
  call and the events is checked only by compiling.
- **Opening real places and adding a real to-do from a case file.** Both go
  through commands other surfaces already use (`open_work_set`, `add_todo`).
- **A real vault.** No count from the owner's vault. Real memories carry
  fewer `decisions` and `next_steps` than the seeded personas, so expect more
  timeline and fewer findings.
- **Stored links.** No persona memory has `related_memory_ids`, so the links
  step ran only against the fake.
- **Light theme, keyboard-only use and a screen reader** in the panel.
- **Latency on a large store.** The largest profile here has 40 memories.
  Thread reading scans 30 days of memories.
