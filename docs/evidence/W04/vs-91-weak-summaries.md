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

## Not done

- Not applied to the real vault. It needs FNDR closed and a backup first.
- The six reviewed placeholders need another review pass to pick up fix 2.
- The visual-only rows belong to the VS-90 backlog work.
