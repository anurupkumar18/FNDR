# How summaries open, and the summary written without a model

Date: 2026-10-07. Corpus: a copy of the owner vault (158 memories, 132 visible). Tool: `cargo run --example vault_qa -- --data-dir <copy>`, section `voice`. No memory text is recorded here.

## What the card line opened with

The voice rule is a finished past-tense sentence ("Reviewed the PR"). Classified by first word, visible memories only:

| Opens with | Stored, before | Shown, before | Stored, after the repair on the copy |
|---|---|---|---|
| A past-tense verb | 12 | 17 | 50 |
| An -ing word ("Reviewing ...") | 35 | 35 | 2 |
| "The", "A", "In": a sentence about a thing or the screen | 35 | 32 | 32 |
| A narrator ("The user", "You") | 7 | 3 | 3 |
| A dangling verb ("Has completed ...") | 2 | 2 | 2 |
| Something else (a title, a noun phrase) | 33 | 35 | 35 |
| A placeholder | 8 | 8 | 8 |

"Shown, before" is the display cleanup as it stood this morning. After today's changes the shown line matches the stored one, and sentences about the window, the capture or the OCR fall back to a title-based line.

## Where the -ing openings came from

Not only from the model. `build_low_ram_semantic_fusion` in `capture/mod.rs` writes the summary when the model is unavailable or its output is weakly grounded. It wrote "You were reviewing {subject} on {app}", where the subject could be a line of body text, set the activity to "reviewing" for every capture (which normalizes to `reviewing_agent_output`) and wrote an intent such as "reviewing visible screen context" that nothing on the screen stated. It also added "The visible context was about implementation status, docs, or roadmap items" on a keyword match.

It now writes "Viewed {files or title} in {app}." (or "Used {app}." when the title is the app's name), leaves the activity `unknown` unless error text sits beside file names, writes no intent, and quotes the strongest line of screen text in a later sentence so the memory can still be found.

## Other changes measured here

- A leading activity verb goes to the past tense even with no narrator ("Reviewing the grades" becomes "Reviewed the grades"), for a fixed list of verbs. A category label ("Debugging: ...") is left alone.
- "The Finder window displays ...", "A screen capture of ...", "The OCR text ..." count as narration.
- The repair scan stores any summary as it is shown, and relabels `reviewing_agent_output` as `unknown` on rows whose summary was written without a model.

## Repair on the copy

`repair_truncated_summaries --apply`: 56 rows rewritten, 49 reworded and re-embedded, 10 relabelled. Known-item search, same 40 sampled memories:

| Query | Before | After |
|---|---|---|
| Summary gist, top five | 34 of 40 | 34 of 40 |
| The summary each row had before, top five | n/a | 34 of 40 |
| Summary words, top five | 32 of 40 | 33 of 40 |
| Window title, top five | 10 of 11 | 7 of 11 |

## The title regression, and its cause

The drop on title queries is real and stable across reruns. Three rows were traced by shape, not text. Before the repair, their second stored vector (the "snippet" vector) matched the window title almost exactly (cosine 1.00, 0.69, 0.44); after it, 0.03, 0.37, 0.28.

The second vector is meant to be the embedding of the summary text. On these rows it was older than the text: it had been written at first capture, when the summary was the title, and later merges and reviews changed the text without refreshing it. So title search on this vault has been working partly by accident, through stale vectors. The repair makes each vector match its text, which is correct, and removes the accident. This also explains the title drop noted after the full re-embed on 2026-10-06.

Whether the second vector should carry the title on purpose is measured in `second-vector.md`.

## Not done

- Not applied to the real vault.
- 32 visible summaries still open with "The" or "A" and 35 with a title or noun phrase. Those are the model's sentences; the display path falls back to the title for the ones that describe the screen.
- 52 memories are labelled `reviewing_agent_output` by the model itself, not by the fallback. That label is offered in the prompt and the model overuses it.

## A summary that is only the window title, 2026-10-08

Some stored summaries are a label, not a sentence: the name of the page or the app with a full stop after it. When every word of a short summary (eight words or fewer, no leading past-tense verb) is in the window title, the card now shows "Viewed {label}." The stored summary is not changed.

On a copy of the vault, summaries a person sees that open with a past-tense verb went from 51 to 55 of 131, and "other" openings from 39 to 36. The rest of the "other" and "describes a thing" openings are the model's own sentences about the state of a screen ("The ... is ..."). No rule can restate those safely; they need the summary prompt, which means runs with the on-device model while FNDR is closed.

## Decision: a statement of what the screen held is in voice, 2026-10-08

About 22 of the summaries a person sees say what a screen held, in the present tense. They come from the extraction prompt's "Describe what is visible", which is there so the model does not guess what the person meant to do.

Options weighed:

1. Ask the model for a past-tense action in every summary. A screen often shows no action. The 2B model then supplies one, and the briefing work on 2026-10-07 showed it doing exactly that (a task reported as submitted that was not).
2. Rewrite the sentences by rule. A rule can change a leading verb. It cannot turn "The PDF is inaccessible" into an action without making one up.
3. Show no model sentence at all and compose every card line from the title. Every line would be true and would say much less, and search by what a memory was about would lose its best text.
4. Accept the sentence as it is.

Decided: option 4. A card line is in voice when it is a past-tense statement of what happened, or a present-tense statement of what the screen held. A narrator ("the user", "you"), an instruction, a sentence about the capture itself, or a guessed intention is never in voice. A true description is worth more than a uniform tense.

After the second repair of the real vault, of 132 visible lines: 76 in voice (54 past tense, 22 screen statements), 17 placeholders, 7 out of voice (narrator, dangling verb, -ing word), and 32 other, which are mostly titles and noun phrases the title rule does not reach.

## Second repair applied to the real vault, 2026-10-08

With FNDR closed and the database backed up (`com.fndr.app.lancedb-backup-20261008-172306-before-second-repair`), the repair changed 62 of 158 rows: 49 reworded and re-embedded, 22 relabelled, 13 with their vectors kept. A second run found nothing to change. Checked on a fresh copy afterwards:

| | After |
|---|---|
| Rows with a zero vector | 0 |
| `reviewing_agent_output` | 40 (was 57) |
| Unrelated queries marked strong | 0 of 8 |
| Search by window title: a memory with that title in the top five | 10 of 11 |
| Search by summary gist: a memory with the same title in the top five | 37 of 40 |
| Search by summary gist: the exact memory in the top five | 34 of 40 |
