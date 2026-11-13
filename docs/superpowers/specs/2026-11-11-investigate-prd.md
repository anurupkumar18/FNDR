# PRD: Investigate

Rank 3 (stretch) feature for the Final demo. Decision record: ADR 039.

## Problem

A person asks about a piece of work, not a page: "catch me up on the Smart
Reminders launch", "what is left for PA2", "what went wrong with the spam
bug". Ask answers from one ranked list. The work is spread over a document, a
chat thread, a ticket and a deck across a week, and some of what those pages
call a next step was finished later. Today the person runs several searches
and reconciles them by hand.

The demo has the same gap. Every agent surface FNDR shows needs an outside
account, so the part of the demo that runs on a clean Mac has no agent in it.

## Goal

One request, typed in Search & Ask, makes FNDR work through the question in
visible steps on this Mac and return a case file: what happened, what was
decided, what is still open, what went wrong, and the places to reopen, each
line citing its memory. Then one tap acts on it.

No account, key, download or setting beyond what Search already needs.

## Users / actors

- A knowledge worker returning to a piece of work.
- The presenter of the demo, on a seeded profile, offline.

## Current behavior

`fndr_answer` runs `retrieve` once, verifies, and composes one answer with up
to 5 sources shown. The activity trace has two steps: request and result.

## Proposed behavior

Search & Ask gains an **Investigate** button beside Search & Ask, and a
finished answer offers "Investigate this further". A run:

1. reads the request into a topic, an optional time filter and the lines of
   inquiry to focus on (decisions, open items, problems);
2. searches the topic; when the search is weak, widens it once;
3. finds the work thread most of the hits belong to and reads the whole
   thread, across apps;
4. probes a focused line of inquiry that still has no findings;
5. follows the stored links of the strongest memory;
6. reads the person's open to-dos tied to those memories;
7. checks: one citation per line, duplicates folded, and each next step
   compared with later memories for a sentence that records it as done;
8. returns the case file.

Each step appears as it finishes, with counts only ("Followed the work
thread, 11 more memories"). Stop ends the run between steps.

The case file shows a header (title, memories read, apps, date span), then
Next steps on record, Recorded as done, Decisions, Problems seen, What
happened and Places. Every line opens its memory. Actions: Open places, Add
to To-dos (per step on record), Copy case file.

## Non-goals

- No model plans, writes or rewords anything in a case file.
- No cloud request. No MCP tool in this slice.
- No new persistence: a case file lives in the panel and is gone when it
  closes. Copy is the way to keep one.
- No voice or notch entry point in this slice.
- No questions without a topic ("what did I do yesterday"): FNDR Review owns
  those. Investigate says so.
- Ask's pipeline and its retrieval gate do not change.

## User workflows

1. **Catch up.** Type "catch me up on the smart reminders launch", press
   Investigate. Steps tick by. The case file lists seven days of work across
   four apps, the decisions, the next steps on record and three recorded as
   done with the clause that says so. Press Open places; the PRD, the ticket,
   the board and the calendar invite open.
2. **From an answer.** Ask "what was the spam bug". The answer cites one
   ticket. Press "Investigate this further" for the whole arc.
3. **Nothing there.** Investigate "the Mars lander budget". The search is
   weak, the widened search is weak, and the panel says nothing in memory
   matches, with the two searches it tried as counts.
4. **Stop.** Press Stop during a run; the panel returns to idle.

## Functional requirements

- FR1: `investigate::plan::plan(goal)` returns the topic words, a time
  filter and the focused lines of inquiry. A request with no topic words
  returns `NeedsTopic`.
- FR2: `investigate::agent::next_step(case)` is pure and returns exactly one
  step or `Finish`. Its order: search, widen (only after a weak search, only
  once), follow thread (only when a thread holds at least two of the kept
  hits, or the best hit), probe (at most two, only for a focused line with
  no findings), follow links (once, only when the best memory has stored
  links), read to-dos (once), check (once), finish.
- FR3: a run never takes more than 10 steps.
- FR4: search hits are kept only from a strong match, only when the keyword
  route found at least half of the topic's words in them, and only those
  scoring at least 0.6 of the best such hit, at most 8. A search with no such
  hit counts as weak.
- FR5: a followed thread adds its members, newest first, at most 24 in the
  case.
- FR6: every memory entering a case passes `memory_is_visible`; imported
  rows and assistant notes are skipped.
- FR7: the done rule marks a next step only when a later memory of the case
  has one clause holding every word of what the step acts on, identifiers
  such as LL-1482 included, and a done word (the step's own verb in the
  past, or closed, done, fixed, live, resolved, shipped, merged, completed),
  and no word that says otherwise (not, still, open, pending, will). A step
  that names one plain word is never marked.
- FR8: a line with neither a memory id nor a to-do id is never returned.
- FR9: the command emits `investigate://step` once per finished step with
  kind, status, counts, route names and duration. No memory text, query or
  title is in an event.
- FR10: `investigate_stop` ends the run before its next step; the command
  returns a cancelled outcome.
- FR11: in Private Mode a run is refused with a sentence that says why.
- FR12: Places are work-set items from the case's memories, one per target,
  at most 6, ordered by reopen precision then recency. Open places calls the
  existing `open_work_set`.
- FR13: Add to To-dos calls the existing `add_todo` with the step's text.
  (`add_todo` takes no source memory; the to-do is the person's own.)

## Non-functional requirements

- Performance: a run on the 40-memory persona profile finishes in under 6
  seconds on the M1 8 GB. Each step is one retrieval or one store read.
- Reliability: a tool error ends that step as degraded and the run goes on;
  a case with no memories ends as nothing found, never an error screen.
- Security/privacy: read-only until a tap. One visibility rule. Events carry
  counts only (ADR 021). Private Mode refuses.
- Accessibility: steps are a polite live region while running; the case file
  is headed sections with real buttons; reduced motion shows rows at once.
- Maintainability: the controller has no store and no Tauri type in it.

## Domain language

| Term | Meaning | Existing code/docs |
|---|---|---|
| Investigation | One run of the loop for one request | new, `investigate/` |
| Case file | The cited result of an investigation | new, `investigate::case` |
| Line of inquiry | Decisions, open items or problems, read from the request's words | new, `investigate::plan` |
| Work thread | A Resume thread: a work identity or a named thread | `resume`, ADR 031 |
| Place | A work-set item: where a memory reopens | `workset::WorkItem`, ADR 027 |
| Strong match | The bar under which Search says "No strong matches" | `RetrieveResult.strong_match` |
| Activity trace | Counts-only steps beside an operation | ADR 021 |

## Affected modules and interfaces

| Module | Change | Interface impact | Tests |
|---|---|---|---|
| `src-tauri/src/investigate/` | new: `plan`, `agent`, `case`, `done`, `tools` | `investigate::run(state, goal, stop, on_step)` | unit tests per file; `tests/investigate.rs` on a real store |
| `src-tauri/src/ipc/commands/investigate.rs` | new: `investigate`, `investigate_stop` | two commands, one event | covered by the integration test and ipcDrift |
| `src-tauri/src/lib.rs` | register module and commands | none | build |
| `src-tauri/src/workset/mod.rs` | `item_of`, `eligible` and `time_filter` shared in the crate | none | existing |
| `src/shared/ipc/tauri.ts`, `src/dev/previewIpc.ts` | wrappers and a preview stub | new types | ipcDrift, previewIpc test |
| `src/domains/ask/` | `Investigation.tsx`, wired into `AskPanel` | none | component tests |
| `src/shared/activity/activityTraceBoundaries.test.ts` | the new producer's counts | none | itself |

## Data flow

```
request -> plan -> agent loop -> tools -> retrieve / resume threads /
             ^        |                   stored links / to-dos
             |        v
             +-- case so far -> check (cite, fold, done rule) -> case file
                      |
                      +-> investigate://step (counts only) -> panel
case file -> tap -> open_work_set / add_todo / clipboard
```

## Acceptance criteria

Checked 2026-11-13; how each was checked is in `docs/evidence/W06/investigate.md`.

- [x] On the seeded office-PM profile, "catch me up on the smart reminders
      launch" returns a case file that reads at least 12 memories from at
      least 3 apps, with at least 5 decisions, each citing a memory of the
      Smart Reminders launch and none from another project. (16 memories, 4
      apps, 13 decisions; the example on the seeded profile.)
- [x] That case file marks "Close LL-1482" and "Book the go/no-go meeting"
      as recorded as done, each with a clause from a later memory.
- [x] A request about something absent returns nothing found after a widened
      search, with no lines. (It first returned another project's case; see
      the evidence.)
- [x] A request with no topic is told to name the work.
- [x] The panel shows each step as its event arrives and the case file after.
      (Component tests and the browser preview; not the debug app.)
- [x] Open places calls `open_work_set` with the case's place ids.
      (Component test; no real place was opened.)
- [x] No event payload contains memory text (integration test).
- [x] `npm run typecheck`, `npm test`, and the focused Rust tests pass.
- [ ] Seen in the debug app on a seeded profile.

## Test plan

- Unit, `plan`: topic words, time phrases, focus words, no topic.
- Unit, `agent`: with a fake `Tools`, the step sequence for a strong search
  with a thread; a weak search that widens then finds; nothing found; no
  thread; a probe only when a focused line is empty; the 10-step budget;
  stop between steps.
- Unit, `done`: the corpus's true pairs, and near misses that must not match
  (same verb, other object; same object, no done word; an earlier memory).
- Unit, `case`: citations enforced, duplicates folded, places capped and
  ordered.
- Integration, `tests/investigate.rs`: a real store with two projects; the
  case file holds one project only; a blocked app's memory never enters.
- Component: steps render from events; case file sections; Open places and
  Add to To-dos call their commands; nothing found; stop; error.
- Real path: an example runs the loop on the seeded profile and prints the
  case file and the done pairs for all three personas; the pairs are judged
  by hand and the count recorded in `docs/evidence/W06/investigate.md`.
- Preview and native: the panel in the browser preview; the debug app on a
  seeded profile with capture paused.

## Rollout / migration plan

Ships on in Search & Ask. No stored data, so nothing to migrate and nothing
to roll back beyond the code.

## Risks

- The done rule matches a sentence about something else. Held down by the
  same-sentence rule and identifiers; the sentence is always shown.
- Thread following pulls in a neighbouring project. A thread is followed
  only when the kept hits sit in it; the integration test holds this.
- A request worded differently from the memories finds nothing, because a
  hit must hold the topic's words. The panel says nothing matched.
- Without work identities a case can be short: 71 of 86 memories on the
  label-free personas.
- Real vaults have fewer `decisions` and `next_steps` than the seeded
  personas, so a case file there can be mostly timeline. The header says how
  many memories were read; empty sections are not shown.

## Open questions

None blocking. A voice entry from the notch and an MCP tool are follow-ups.

## Suggested issues

1. Plan and controller with a fake tool set.
2. Done rule with the persona pairs.
3. Real tools and the integration test.
4. Commands, events and wrappers.
5. Panel, preview stub and component tests.
6. Evidence run on three personas; docs.
