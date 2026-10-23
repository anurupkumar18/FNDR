# 038. Daily Summary narrative: a local model rewords checked evidence

Status: Proposed (2026-10-22). Requested by Kunj. Not yet reviewed by the team.

## Context

The Daily Summary was a one-sentence list of app names, a task count and one bullet per capture ("Dia: 1 capture at 5:57 PM around ..."). A normal reader gets nothing from it. The ask: paragraphs, action items and reminders written by a local model.

The rule in `AGENTS.md` stands: the on-device 2B model extracts, it does not synthesize. On 2026-10-07 a model asked to write a briefing invented advice, copied its notes and reported an open task as submitted (`docs/product/llm-task-catalog.md`, `daily_briefing`, removed). That prompt had no check after it. It was also the only author of the sentence.

## Decision

The narrative is a checked model output, not a free one.

1. **Evidence package, in code** (`src-tauri/src/daily_narrative/mod.rs`). Visible memories are read in `ipc/commands/stats.rs` through `memory_is_visible`, and only own-screen rows (`is_own_work`). Captures group by project, else app, and split after a 60 minute gap. Each group carries a time span, apps, window titles, notes (neutral-voice summaries), decisions, errors, files, open tasks and finished tasks. Groups with no note, task, decision or error are dropped; at most 10 groups and 6,000 characters are sent. Raw screen text is never sent. A field that reads like an instruction ("ignore previous instructions", "reply only with") is not sent.
2. **Model**: local only. The task `daily_summary_narrative` prefers Apple's on-device model (route table in `inference/foundation.rs`) and falls back to Qwen when Apple is off or errors. If Apple's reply leaves no checked paragraph and a GGUF is loaded, one retry runs on Qwen. Apple receives three typed string lists (paragraphs, action items, reminders), each string `ids | text`; Qwen receives the same instruction and answers in `KIND | ids | text` lines. This is a refinement of the JSON shape in the request: both wires produce the same structure, `{paragraphs, action_items, reminders}` with evidence ids. JSON did not survive Apple's single text field in the live run (`docs/evidence/W05/daily-summary-narrative.md`); the checker still accepts a JSON object.
3. **The checker** (`daily_narrative/check.rs`, `verify`). An item is kept only when:
   - it cites one to three group ids that exist;
   - every number (digit or spelled out from two), time, file, identifier, capitalized name, weekday and month in it is in a cited group, and any quoted text is too; a sentence-initial word is checked when it is the name of an app, site, file or project from another group;
   - a part of the day ("afternoon") fits a cited group's span;
   - at least 60 percent of its content words (not verbs in -ed or -ing, not stop words) appear in a cited group;
   - it has no narrator or reader wording (you, we, they, the user, the team), no advice wording (should, consider, need to), no instruction shape, and does not copy 14 or more consecutive words of a note;
   - a done claim (finished, sent, fixed ...) has a cited group that states completion, and does not reword an open task in a cited group;
   - an action item or reminder rewords a task already extracted in a cited group (all its content words come from the task; a reminder's task needs a due date, a reminder kind or a time word in its text), and is not a done claim. The model's due date is ignored; the date shown comes from the task.
4. **Fallback** (`daily_narrative/basic.rs`). With no checked paragraph, the model absent, or too little activity (under three kept captures), the summary is composed in code: a short paragraph per theme (time span, apps, project, up to two notes, a decision, an error), and the open tasks as items. It is labelled "Basic summary" with the reason. When a checked paragraph exists but no item does, the open tasks from the groups fill the item lists. An empty day makes no model call.
5. **UI.** "Your day", "Action items" (checkable; a checked item with a task row completes it), "Reminders", and the old per-capture list under a closed "All activity" disclosure, together with the open-task list. Each paragraph and item links to its memory. The old overview sentence ("You used ...", a narrator) and its command are deleted. The PDF prints the same sections.

## Options not taken

- **A free-form model summary.** Rejected by the 2026-10-07 incident and reproduced twice on Apple's model in the live run (open tasks reported as completed).
- **Code only.** It is the fallback and is safe, but reads as a list of notes joined by "Used Slack at ...". The model's job is to merge themes and drop the noise, and the checker lets us accept that without trusting it.
- **A second model call to judge the first.** A 2B judge shares the failure it would catch.
- **Free fields from the model for dates.** Dates come from the task row.

## Risks

- **A plausible sentence that passes.** The checks are lexical. A paragraph that reorders true facts ("Reviewed X after Y" when Y came first) or states a wrong relation between two groups' words passes. The content-word rule is a ratio, so one ungrounded noun in a sentence of grounded words passes (the first live run kept a clause naming "errors addressed" when the group listed errors without saying they were resolved). The lowercase invented noun is the open hole.
- **Thin output.** Strict checks drop sentences. The best Apple run kept one paragraph.
- **Latency.** 8 to 14 seconds per day with the model, shown behind a loading state.
- **Apple's schema.** The helper's typed lists carry `ids | text` as one string; a reply that omits the bar is dropped.

## Consequences

- New task row in the catalog, prompt v19, fingerprint recorded.
- `generate_daily_summary_narrative` replaces `get_daily_summary_overview`.
- Checker tests live beside the code: invented entity, uncited item, done claim without evidence, open task reported as done, injection in a window title, empty day, single-app day.
- Open question: whether sentence-level paraphrase checks (an entailment model) are worth their cost before this leaves the basic summary as the default.
