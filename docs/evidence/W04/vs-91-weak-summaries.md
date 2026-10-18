# VS-91: why summaries are still weak, and what fixes them

Date: 2026-10-07. Corpus: a copy of the owner vault taken after the 2026-10-06 re-review (147 memories). Tool: `cargo run --example vault_qa -- --data-dir <copy> --sample 40 --fast`, which now prints a counts-only `weak_summaries` breakdown. No memory text is recorded here.

## Where the 44 weak summaries came from

| Count | Kind | Why it was still weak |
|---|---|---|
| 23 | Narrated | The stored sentence opened with the person ("The user is ..."). Cards already showed a cleaned line; the stored text and its vector did not. Fixable with no model. |
| 3 | Narrated | The wording cleanup does not produce a usable sentence. |
| 12 | Placeholder | Visual-only capture, no text to review against, status `pending_visual_semantics`. The review skips these by design. |
| 6 | Placeholder | Reviewed, but the review returned no card line and kept the stored placeholder. |

Five of the narrated rows are also visual-only captures. Most weak rows hold under 200 characters of text.

## Fixes

1. `reword_narration` (in `memory_review::repair_truncated`) applies the display cleanup to the stored summary and refreshes the vectors. The repair scan runs it with the cut-token repair.
2. `summary_candidate` (in `memory_review::pipeline`) no longer keeps a placeholder: with no card line from the reviewer, the first sentence of the reviewed context is used. This takes effect when those rows are reviewed again.
3. The scorecard separates weak rows a person can see from rows already hidden as low signal.

## Result on the copy

| | Before | After repair |
|---|---|---|
| Narrated | 26 (18%) | 3 (2%) |
| Placeholder | 18 (12%) | 18 (12%) |
| Weak rows shown to the person | not measured | 7 of 121 shown (6%) |
| Weak rows hidden as low signal | not measured | 14 |
| Found in top five by new summary gist | 37 of 40 | 37 of 40 |
| Found in top five by the summary it had before | n/a | 37 of 40 |
| Found in top five by summary words | 35 of 40 | 34 of 40 |

The repair rewrote 23 rows and re-embedded all 23. Searching with the sentences the rows had before the repair still finds them, so no wording a person would remember was lost.

## Reading

- Counting only what a person can see, weak summaries are at 6 percent on the copy, under the 10 percent target. Counting every stored row they are at 14 percent, and the rest is the visual path: rows that never got a description.
- The reworded lines read "Reviewing ..." or "Opened ...". They are neutral but not always past tense.

## Applied to the real vault, 2026-10-07

With FNDR closed and the database backed up, `repair_truncated_summaries --apply --allow-real-profile` rewrote 30 rows: 23 reworded and re-embedded, 7 relabelled with their vectors kept. A second run changed nothing. Checked on a fresh copy afterwards:

| | Before | After |
|---|---|---|
| Memories | 147 | 147 |
| Narrated | 26 (18%) | 3 (2%) |
| Placeholder | 18 (12%) | 18 (12%) |
| Activity labels outside the list | 7 | 0 |
| Weak rows shown to the person | not measured | 7 of 121 |
| Top five by the summary each row had before | n/a | 37 of 40 |
| Top five by new summary gist | 37 of 40 | 37 of 40 |
| Top five by summary words | 35 of 40 | 34 of 40 |
| Rows with a zero vector | 0 | 0 |
| Unrelated queries marked strong | 3 of 8 | 0 of 8 |

The last row is the strong-match rule (`strong-match.md`), not this repair.

## Second review pass on the repaired copy, 2026-10-07

`review_preview --limit 40 --quiet` on the copy that already had the second repair, with the on-device model. It picked 33 weak rows: 6 were rewritten, 12 were refused by the guards, and 15 were skipped because they have no text to review.

| | Before | After |
|---|---|---|
| Narrated | 14 (9%) | 11 (7%) |
| Placeholder | 19 (12%) | 17 (11%) |
| Weak rows shown to the person | 18 | 14 |
| Weak rows hidden as low signal | 15 | 14 |

How the summaries a person sees open after the pass: 51 with a past-tense verb, 22 describe a thing ("The ...", "A ..."), 39 other, 14 placeholder or empty, 3 narrator, 2 with an -ing verb, 2 dangling.

Reading: a review pass moves the numbers a little. The guards refuse twice as many rewrites as they let through, which is the intended direction, and the rows that are left have no text for the model to work from.

## Not done

- The second repair and this review pass are on a copy only. The real vault has the first repair.
- The 15 rows with no text belong to the VS-90 backlog work.
- 22 shown summaries describe a thing instead of stating what happened.
