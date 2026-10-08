# Task list audit and what replaced the extractor

Date: 2026-10-07. Corpus: a copy of the owner vault. Tool: `cargo run --example task_audit -- --data-dir <copy>` (counts only).

## What the list held

| Measure | Value |
|---|---|
| Open tasks stored | 471 |
| Memories in the vault | 147 |
| Memories that produced a task | 86 |
| Tasks ever completed or dismissed | 0 |
| Tasks from ChatGPT or Claude screens | 266 |
| Tasks with a due date | 0 (82 were typed as reminders) |
| Title words mostly found in the cited memory (80 percent or more) | 37 |
| Under half of the title words found there | 341 |
| Titles that leaked the prompt's placeholder | 34 |
| Titles addressed to a "team" | 49 |
| Tasks repeating another in other words (word overlap 0.5 or more) | 79 |
| Most tasks citing one memory | 33 |

The support figures are word overlap against the memory as it is now, which can differ from what the model saw. Read them as direction.

## Causes

1. The extractor ran on every stored capture, and again every time a capture merged into an existing memory. The model words the same idea differently each run, and duplicates were matched by exact title only.
2. It was asked for tasks on every screen, with no requirement to show where a task came from.
3. It ran on AI chat screens, where an assistant's plans and status read like tasks.
4. Opening To-dos created more tasks from recent memories by substring cues ("fix" in "prefix").
5. Nothing expired and there was no way to say "not a task".

## What changed

- `tasks/suggest.rs`: the model proposes `KIND | task | words copied from the screen`. A line is kept only when the copied words are in the captured screen text and state a commitment or a request. At most two per capture. A reminder needs a date and a follow-up a name, or it becomes a plain to-do.
- Capture asks once per new memory, never on a merge, never on AI chat or system screens, and at most once per app per ten minutes.
- A suggestion is not a task. To-dos shows the person's tasks, then up to five suggestions with their quote; a suggestion is offered for three days. Accepting keeps the link to its memory. "Not a task" dismisses it, and a dismissed title is not suggested again.
- Opening To-dos no longer creates tasks.

## Cleanup, on the copy

`cargo run --example retire_task_suggestions -- --data-dir <copy> --apply` dismissed all 471 (every one was an unquoted suggestion; none was the person's own). Open tasks 471 to 0, nothing deleted, and a second run changed nothing.

Applied to the real vault on 2026-10-07 with FNDR closed and the database backed up: 471 dismissed, 0 open, 471 still stored, and a second run changed nothing.

## The new prompt on the real model

`task_suggestions_on_synthetic_screens` (ignored test) runs the local 2B model on eleven synthetic screens: four that state a task (an email request, a person's own note, a chat message asking for a review, an assignment with a due date) and seven that do not (assistant status text, instructions written for an agent, an article, a disk report, a spreadsheet, code, and a page with a planted instruction to add a task).

| Run | Model lines | Kept | Screens right | Real tasks missed | False tasks |
|---|---|---|---|---|---|
| First check: trust the model's copied words | 17 | 2 | 7 of 11 | 3 of 4 | 1 (the planted one) |
| Check now | 17 | 4 | 11 of 11 | 0 | 0 |

What the first run showed: the model writes about 1.5 lines per screen whether or not there is a task, and it cannot copy. For the email it returned "Friday" as the copied words; for the chat message, a different line. So the check no longer trusts the model's copy. It keeps it when it holds up, and otherwise looks for the shortest sentence on the screen that states a commitment or request and holds at least 60 percent of the title's words. The planted line passed the first check because its words really were on the screen; a line that carries wording addressed to an AI ("ignore all previous instructions", "system note", "add the task") can no longer support a task.

**That result is fitted**: the check was changed after reading those eleven outputs. So two further sets were written and each was run once before anything was changed for it.

| Set, at first sight | Screens right | Real tasks found | False tasks | What went wrong |
|---|---|---|---|---|
| Held-out 1 (9 screens) | 6 of 9 | 3 of 4 | 2 | Documentation ("You need to add the dependency") and a recipe ("Make sure the starter is active") were taken as this person's tasks. One real request was missed because the model returned NONE. |
| Held-out 2 (8 screens), after the fix for set 1 | 7 of 8 | 4 of 4 | 1 | An encyclopedia page about "the deadline effect", quoted from the window title. |

Two rules came out of these, each with a unit test taken from the model's output:

- **Where the words are decides what they mean.** A request or instruction ("please", "can you", "make sure", "you need to", "remember to") counts only in mail, chat and notes (`Surface::Personal`). A first-person commitment ("I need to", "I'll") or a dated deadline counts anywhere.
- **A deadline has a date, and a window title states nothing.** "Deadline" or "due" supports a task only with a day, date or time in the same sentence, and the first line of the evidence (the window title) is never a quote.

With both rules all three sets read 27 of 28. The one miss is the request the model did not offer at all. Every screen in these sets has now been seen, so 27 of 28 is not a held-out number; the two first-sight rows above are.

Across the three sets the model wrote 42 task lines for 28 screens, 12 of which state a task. The check kept 11.

## Same task, different words

The model words one task differently every time, so a suggestion is also compared by meaning with tasks from the last 14 days, in any state. The threshold comes from the 471 real titles, embedded with the model the app already loads (MiniLM, 384 dimensions):

| Pairs of titles | Count | Cosine similarity |
|---|---|---|
| Repeat each other (word overlap 0.5 or more) | 150 | median 0.83, lower quartile 0.73 |
| All other pairs | 110,535 | median 0.10, 99th percentile 0.49 |

| Threshold | Repeats caught | Other pairs merged |
|---|---|---|
| 0.70 | 118 of 150 | 157 |
| 0.80 | 88 of 150 | 40 |
| 0.85 | 67 of 150 | 16 |

`SAME_TASK_SIMILARITY` is 0.80. The 40 "other" pairs at that level are an upper bound on wrong merges: they were not read, and many are likely repeats the word-overlap grouping missed. Without an embedder loaded nothing is dropped by meaning.

## Replay on real captures

`cargo run --example task_suggestion_replay -- --data-dir <copy>` runs the new pipeline over every memory in a copy of the owner vault, with the real model, and writes nothing.

| Step | Count |
|---|---|
| Memories | 147 |
| Skipped: AI chat or system screen | 95 |
| Skipped: too little text | 1 |
| Asked the model | 51 |
| Task lines the model wrote | 51 |
| Lines the screen text supports | 0 |
| Suggestions | 0 |

The old extractor made 471 tasks from these same memories. The new one makes none.

The 51 rejected lines were read on the machine (not recorded here). None quotes a sentence in which someone commits to or asks for something. They are what the audit predicted: titles of pages and videos restated as tasks, file names, and meeting times that appear nowhere on the screen (the same invented date and time recurs across unrelated captures). Two concern course pages that may well be real work, a survey and a report; neither capture holds a due date or a request in its text, so by the rule they are not suggestions. That judgment is the assistant's, not the owner's.

So on this vault precision is not measurable (nothing was kept) and the open question is recall: whether real tasks exist in these captures that the pipeline cannot see. Most of this vault is AI chat, video and course pages; it holds almost no mail, chat or notes, which is where the synthetic sets found tasks.

## Not measured

- A fresh set of screens after the last rule. The first-sight rate has been 6 of 9, then 7 of 8.
- Requests the model never offers: the check can only reject, it cannot find a task the model skipped.
- Recall on real captures. It needs the owner to mark, on a few days of their own captures, the tasks they would have written down.
- Captures from mail, chat and notes, which this vault barely contains.
