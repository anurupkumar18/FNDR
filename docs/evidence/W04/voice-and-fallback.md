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
