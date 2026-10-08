# What the second stored vector should hold

Date: 2026-10-07. Each memory stores a primary vector (the composed text) and a second, "snippet" vector that search queries as its own branch. Three options for the second vector were measured on copies of the owner vault after the summary repair, with every row re-embedded (158 memories, 40 sampled, 11 of them with a usable window title). Cells are first place, then top five.

| Second vector holds | Summary gist | Summary words | Window title | A memory with that title in top five |
|---|---|---|---|---|
| The summary (as shipped) | 31, 36 of 40 | 29, 36 of 40 | 6, 7 of 11 | 7 of 11 |
| Title, then the summary | 30, 36 | 27, 34 | 8, 8 | 9 |
| The title alone | 28, 36 | 25, 34 | 9, 10 | 10 |

No option wins. The title alone recovers title search and costs three first-place hits on gist queries and four on word queries. Unrelated queries marked strong stayed at 0 of 8 in all three.

Two vectors cannot serve both needs. A third vector is already stored per memory (`support_embedding`) and search does not query it. The next step is a title signal of its own, either that third vector as a title branch or a title match in the keyword route, measured against the same table.

## A title signal in ranking

`context_runtime::fusion` now adds `TITLE_MATCH_BONUS` (0.15) to a memory when at least 80 percent of the query's words are in its window title and the query has two words or more. The second vector stays the summary. On the repaired copy, same 40 memories:

| | Repaired, no title signal | Repaired, with the title signal | Before the repair (stale vectors) |
|---|---|---|---|
| Window title, top five | 7 of 11 | 9 of 11 | 10 of 11 |
| A memory with that title in top five | 7 of 11 | 10 of 11 | 10 of 11 |
| Summary gist, top five | 34 of 40 | 34 of 40 | 34 of 40 |
| The summary each row had before, top five | 34 of 40 | 33 of 40 | n/a |
| Summary words, top five | 33 of 40 | 33 of 40 | 32 of 40 |
| Unrelated queries marked strong | 0 of 8 | 0 of 8 | 0 of 8 |

Title search is back where it was, now on purpose. The exact row is found for 9 of 11 where it was 10: the same page captured at several times shares a title, and the bonus lifts all of them equally, so which one comes first is decided by the other signals. One earlier-summary query moved out of the top five; with one run each, a difference of one is within what this corpus shows between runs.

The labeled retrieval gate passes on all three personas with the title signal, and the strong-match figures did not move: 3 of 24 no-match queries marked strong and 6 of 98 real queries marked weak, as before it.

The summary repair has not been applied to the real vault. It should be applied only with the title signal in the build.
