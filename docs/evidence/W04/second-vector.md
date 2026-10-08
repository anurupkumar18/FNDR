# What the second stored vector should hold

Date: 2026-10-07. Each memory stores a primary vector (the composed text) and a second, "snippet" vector that search queries as its own branch. Three options for the second vector were measured on copies of the owner vault after the summary repair, with every row re-embedded (158 memories, 40 sampled, 11 of them with a usable window title). Cells are first place, then top five.

| Second vector holds | Summary gist | Summary words | Window title | A memory with that title in top five |
|---|---|---|---|---|
| The summary (as shipped) | 31, 36 of 40 | 29, 36 of 40 | 6, 7 of 11 | 7 of 11 |
| Title, then the summary | 30, 36 | 27, 34 | 8, 8 | 9 |
| The title alone | 28, 36 | 25, 34 | 9, 10 | 10 |

No option wins. The title alone recovers title search and costs three first-place hits on gist queries and four on word queries. Unrelated queries marked strong stayed at 0 of 8 in all three.

Two vectors cannot serve both needs. A third vector is already stored per memory (`support_embedding`) and search does not query it. The next step is a title signal of its own, either that third vector as a title branch or a title match in the keyword route, measured against the same table.

Until then the summary repair is not applied to the real vault: it would lower title search from 10 to 7 of 11 by replacing stale second vectors that happen to equal the title (`voice-and-fallback.md`).
