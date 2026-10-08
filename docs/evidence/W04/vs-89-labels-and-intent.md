# VS-89: activity labels and intent text

Date: 2026-10-07. Corpus: a copy of the owner vault (147 memories, MiniLM v4, 384 dimensions). Tool: `cargo run --example vault_qa -- --data-dir <copy> --sample 40 --fast`.

## One label list

`CANONICAL_ACTIVITY_TYPES` accepted 19 labels; the prompts offered 17. `observing` and `screen_review` could be stored but never chosen by the model. They are retired: `normalize_activity_type` maps them to `unknown`, and `stored_activity_labels_are_exactly_the_ones_the_prompts_offer` fails if the two lists differ.

The vault still held 3 `observing` and 4 `screen_review` rows, because labels are normalized on write and those rows were written earlier. The repair scan (`repair_truncated_summaries`) now relabels them; the label is not part of the embedded text, so no vector changes.

Five gold rows expected the retired labels. Two meeting cases became `watching_or_listening`; two design-review cases and one textless screenshot became `unknown` (owner decision).

## Does intent text help search?

Three fresh copies were re-embedded: as shipped, without the `intent:` segment, and without `intent:` and `workflow:`. Each was scored on known-item search with the same 40 sampled memories. Cells are first place, then top five.

| Query | As shipped | No intent | No intent, no workflow |
|---|---|---|---|
| Summary gist, hybrid | 33, 37 of 40 | 33, 37 | 33, 37 |
| Summary words, hybrid | 32, 35 of 40 | 30, 35 | 32, 35 |
| Window title, hybrid | 5, 6 of 9 | 5, 7 | 5, 6 |
| Summary gist, vector only | 32, 39 of 40 | 31, 39 | 32, 38 |
| Summary words, vector only | 26, 35 of 40 | 25, 35 | 28, 34 |
| Window title, vector only | 4, 7 of 9 | 4, 7 | 3, 6 |

No variant differs from the shipped text by more than two queries in any cell, and the hybrid top-five on summary queries is identical. With 81 percent of memories carrying no intent, the segment is too rare to matter either way.

**Decision.** No change to the embedded text. Intent stays in when it exists and is already skipped when empty or `unknown`.

## Not done

- The six rules in `infer_intent_analysis` are unchanged. The measurement above says the label does not affect retrieval, so improving the rules would only matter for display, and the label is not shown on the card today.
- Not measured on the three seeded personas: their memories are seeded with one vector for both roles, so they cannot show an embedding-text effect (see `vs-87-fusion-retune.md`).

## The agent review label needs an agent on screen, 2026-10-08

On a copy of the vault, 57 of 158 memories carried `reviewing_agent_output`. The scorecard now breaks that label down by app: 41 were ChatGPT or Claude windows, where it is right, and 16 were Finder, Google Chrome, Spotify and System Settings.

`inference::activity_for_evidence` keeps the label only when the app or page is an AI assistant, or the window title or screen text names one. Otherwise the activity is unknown. Capture uses it when it checks the model's answer, and the repair tool uses it for stored rows.

| Label on the copy | Before | After the repair |
|---|---|---|
| `reviewing_agent_output` | 57 | 41 |
| of those, outside ChatGPT and Claude | 16 | 2 |
| `unknown` | 58 | 74 |

The repair changed 16 rows and kept all 16 vectors. The two left outside an assistant's app name an agent in their text. Two of the 16 were assistant windows whose summary was first written without a model: the older rule for those rows took the label off again after a review pass had set it. That is two rows and is noted, not fixed.

Not applied to the real vault.
