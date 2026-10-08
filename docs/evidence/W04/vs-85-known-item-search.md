# VS-85: why a memory could not be found from its own summary, 2026-10-06

Measured with `cargo run --example vault_qa` on a copy of the owner vault: 147 memories, 40 sampled (the newest that search does not hide and that have a real summary). Counts only; no memory text is recorded here.

## Question

Search with the first eight words of a memory's own summary, narrator opener removed ("gist"). Is that memory in the top five?

## Cause

Capture embedded the composed memory text (project, topic, context, insight rows) through the screen-text chunker. That chunker is for OCR: it adds the window title as a line and applies screen-noise cleanup. On composed text it made memories from the same app resemble each other more than their own content.

Two smaller contributors were also found: the line shown on the card was not part of the embedded text, and 62 of 147 primary vectors no longer matched the text their memory holds now, because merges and reviews changed the text and review re-embedded with a different recipe than capture.

## Experiment

Every memory in a fresh copy was re-embedded with one recipe, then the same 40 were searched.

| Recipe | Gist: first | Gist: top five | Gist: vector only, top five | Window title: top five |
|---|---|---|---|---|
| Stored vectors, untouched | 28 of 40 | 30 of 40 | 30 of 40 | 9 of 10 |
| Screen-text path, summary first | 23 | 28 | 29 | not recorded |
| Screen-text path, no summary | 23 | 28 | 29 | not recorded |
| Plain path, no summary | 30 | 37 | 36 | not recorded |
| Plain path, summary first | 31 | 37 | 38 | not recorded |
| Plain path, summary and title in the text (shipped) | 30 | 37 | 38 | 7 of 10 |
| Primary plain, snippet with screen-text path | 25 | 31 | 38 | 9 of 10 |

## Decision

Shipped: both the primary and the snippet text are embedded plain, the card summary leads the primary text, the window title is a segment of it, and prose misfiled as a command is left out. Capture, review and repair now share one recipe (`memory_embedding_document::refresh_text_vectors`). `EMBEDDING_DOCUMENT_VERSION` is 2.

Gist search improves from 30 to 37 of 40. Search by window title drops from 9 to 7 of 10. Keeping the screen-text path on the snippet vector restores title search but gives back almost all of the gist gain in the fused result (31 of 40), although the vector branch alone still finds 38. That points at fusion, not at the vectors: the snippet branch outweighs the primary. It belongs to the fusion retune (VS-87).

## Remaining misses (3 of 40 with the shipped recipe)

Two are found by the vector branch alone in the top five and lost in fusion. One has a primary vector ranked beyond 20 for its own summary.

## Limits

- Forty memories from one vault. Each query is one memory, so a difference of one or two is not reliable; the gist change (7 queries) is the only large effect here.
- Existing memories keep their old vectors until they are re-embedded (`cargo run --example reembed_memories`). Nothing has been applied to the owner vault.
- The seeded retrieval gates embed their own text and are not affected by this change, so they cannot confirm or contradict it.
