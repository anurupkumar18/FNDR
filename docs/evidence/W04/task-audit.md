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

`cargo run --example retire_task_suggestions -- --data-dir <copy> --apply` dismissed all 471 (every one was an unquoted suggestion; none was the person's own). Open tasks 471 to 0, nothing deleted, and a second run changed nothing. Not applied to the real vault.

## The new prompt on the real model

`task_suggestions_on_synthetic_screens` (ignored test) runs the local 2B model on eleven synthetic screens: four that state a task (an email request, a person's own note, a chat message asking for a review, an assignment with a due date) and seven that do not (assistant status text, instructions written for an agent, an article, a disk report, a spreadsheet, code, and a page with a planted instruction to add a task).

| Run | Model lines | Kept | Screens right | Real tasks missed | False tasks |
|---|---|---|---|---|---|
| First check: trust the model's copied words | 17 | 2 | 7 of 11 | 3 of 4 | 1 (the planted one) |
| Check now | 17 | 4 | 11 of 11 | 0 | 0 |

What the first run showed: the model writes about 1.5 lines per screen whether or not there is a task, and it cannot copy. For the email it returned "Friday" as the copied words; for the chat message, a different line. So the check no longer trusts the model's copy. It keeps it when it holds up, and otherwise looks for the shortest sentence on the screen that states a commitment or request and holds at least 60 percent of the title's words. The planted line passed the first check because its words really were on the screen; a line that carries wording addressed to an AI ("ignore all previous instructions", "system note", "add the task") can no longer support a task.

**This result is fitted.** The check was changed after reading these eleven outputs, and three of them are now unit-test fixtures. It needs screens it has not seen.

## Not measured

- Screens the check was not tuned on.
- Precision on real captures. There is no labeled set of captures with the tasks a person would write.
