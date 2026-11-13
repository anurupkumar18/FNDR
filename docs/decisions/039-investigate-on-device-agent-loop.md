# ADR 039: Investigate is an agent loop decided in code, on this Mac

## Status

Accepted, 2026-11-11. Owner decision, delegated (see `docs/team/decision-log.md`).

## Context

The course ranks features 1 to 3; rank 3 is the stretch tier. The demo needs one
more feature that shows FNDR acting as an agent, and it has to work on any Mac
that runs FNDR, with no account, key or helper to install.

Every agent surface FNDR has today needs something outside the app:

| Surface | Needs |
| --- | --- |
| Notch Do, beyond "open X" and "look up Y" | Codex on a ChatGPT plan |
| Hand-off | Codex, a ChatGPT plan, a 200 MB toolchain |
| Smart Fill | ChatGPT |
| Agent page (Hermes) | A provider sign-in |

What works with nothing installed is Search & Ask, and Ask is one retrieval
pass: one query, one ranked list, one answer. A question about a piece of work
("catch me up on the Smart Reminders launch, what is still open") is not one
query. Answering it by hand means searching, noticing the work spans several
apps, reading the whole thread, and checking which next steps were later done.

Read from the office-PM corpus before deciding: the Smart Reminders launch is
16 memories in 4 apps over 7 days. Ask returns at most 12 cards from
one ranked list and shows 5 sources. Three of the steps those memories record
as next steps are recorded as done by a later memory in the same thread
(`docs/evidence/W06/investigate.md`).

## Decision

FNDR gains **Investigate**: a bounded agent loop over the person's own
memories, inside Search & Ask.

1. **The loop is a controller in code, not a model.** `investigate::agent`
   holds the case so far and `next_step` picks the next move from what the
   last one found: search, widen a weak search, follow the work thread the
   hits belong to, probe a line of inquiry that has no findings yet, follow a
   stored link, read open to-dos, check. It stops at a fixed step budget.
   AGENTS.md records why a model does not plan here: the on-device 2B model
   extracts and does not synthesize, and ADR 018 allows a cloud model only on
   a surface where the person chose the provider.
2. **Tools are the paths every surface already uses.** Search is
   `context_runtime::retrieve`; a thread is `resume::build_resume_threads`;
   links are the stored `related_memory_ids`; places are work-set items. The
   one visibility rule applies to every memory read. No new index, table or
   prompt.
3. **The result is a case file composed from stored fields.** Each line is a
   summary, decision, next step or error already on a memory, with the memory
   it came from. A line with no memory is not written. Nothing is reworded.
4. **A step is marked done only with a later quote.** A next step is shown as
   "recorded as done" when one clause of a later memory in the same case
   holds every word of what the step acts on and a done word, and the row
   shows that clause and its memory. FNDR does not say the step is done; it
   shows what was recorded. The other steps are "next steps on record", not
   "still open": FNDR does not know they are open.
5. **A hit must name the work.** Search's strong-match bar passes a hit that
   is only close in meaning, which is right for a list a person scans and
   wrong for a loop that then reads that hit's whole thread. A hit enters a
   case only when the keyword route found at least half of the topic's words
   in it. A request in other words than the memories use finds nothing and
   says so.
6. **Nothing happens without a tap.** The case file offers to open its places
   (the work-set opener), add a step to To-dos, and copy the file. The run
   itself only reads.
7. **Live steps follow ADR 021.** The backend emits one typed event per step
   with counts and route names only. Findings appear in the case file, never
   in the trace.
8. **Not an MCP tool yet.** Outside agents keep `fndr.search` and
   `fndr.build_context_pack`. A tool is added when a client needs it, through
   the MCP audit.

## Options weighed

| Option | Why not |
| --- | --- |
| Let the local 2B model plan the steps | It invented advice and reported an open task as done when asked to sum up notes (2026-10-07). A planner that fails silently is worse than none in a live demo. |
| Plan with Codex or ChatGPT | Needs a sign-in, fails offline, and sends memory text off the Mac for a read-only question. The demo requirement is no setup. |
| Make Ask itself multi-step | Ask is measured by the retrieval gate as one pass. Changing it moves every number in `make qa-retrieval-check`. Investigate sits beside it and calls the same `retrieve`. |
| A scripted tour that looks like an agent | It would not be a feature. The loop here takes different steps on different stores, and the tests show that. |
| A standing agent that watches for a condition | Proactive signals and routines cover this, and a demo cannot wait for a trigger. |
| Drop next steps a later memory finishes | Hides the evidence. Showing the later sentence lets the person judge a wrong match. |

## Consequences

- A new top-level module `investigate/` (plan, agent, done rule, case file,
  tools) and two IPC commands. It calls no bulk store read of its own, so
  `src/dev/memoryReadDrift.test.ts` needs no new row: every memory comes
  through `retrieve`, `resume::build_resume_threads` or `related_memories`,
  and is checked again with `memory_is_visible` when read by id.
- The panel is a new activity trace producer. Its counts are listed in
  `src/shared/activity/activityTraceBoundaries.test.ts`.
- The controller is pure, so its tests run with a fake `Tools` and no store.
  One integration test runs the real tools on a seeded store.
- The "recorded as done" rule can be wrong. It is conservative (every object
  word, a done word, one clause, a later memory): 7 of 7 pairs right on the
  three persona corpora, recall not measured
  (`docs/evidence/W06/investigate.md`). A wrong match shows its clause.
- Investigate needs a topic. "What did I do yesterday" stays with FNDR Review.
- Imported rows and assistant notes are left out of a case file, as in Daily
  outputs: they are not the person's own work.
- Private Mode refuses a run.

## Verification

Unit tests for the plan, the controller's choices and the done rule; an
integration test on a real store; component tests for the panel; 29 runs of
the real path on seeded profiles of the three personas, with and without
project labels. Not run inside the debug app. Results and what is not
verified: `docs/evidence/W06/investigate.md`.
