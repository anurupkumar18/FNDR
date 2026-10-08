# Anurup: embeddings, intent, vector search and LanceDB, filed 2026-10-06

Filed after the instruction and model review (`docs/product/instruction-model-capture-review-index.md`) and a quality scorecard run on a copy of the owner vault with `cargo run --example vault_qa`. Numbers below come from that run: 147 memories, 40 sampled for search. No memory text is recorded here. The long-term goal these serve: every sentence FNDR shows and every vector it searches comes from one owned contract, and nothing ships without a repeatable local score.

## VS-85 Find out why a memory cannot be found from its own summary
- assignee: anurupkumar
- labels: area::vault-search, type::bug, prio::p0
- milestone: W03-Build
- estimate: 6h

**Why.** Searching with the first seven words of a memory's own summary returned that memory first for 24 of 40 and in the top five for 26 of 40. The vector branch alone managed 25 of 40 in the top five. For 2 of the 40 the top hit was another capture from the same session. A person who remembers the gist cannot rely on search.

**Today.** `examples/vault_qa.rs` reports `known_item_search`. The traced capture in that run had a good `display_summary` that its primary embedding text did not contain.

**Do.**
1. For each miss, record which route returned what (`production_retrieval` in the explained result) and what took the first five places.
2. Classify the misses: summary absent from the embedding text, near-duplicates crowding the top five, keyword route miss, or score under the bar.
3. Fix the largest class first, test-first, and rerun the scorecard.

**Found 2026-10-07, title search.** Title search on the owner vault was being carried by stale second vectors that equal the window title; refreshing them lowers "a memory with that title in the top five" from 10 to 7 of 11. No stored vector is a vector of the title, and putting the title in the second vector costs first-place hits on summary searches. Ranking now adds a fixed bonus when the query's words are a memory's window title, which restores 10 of 11 on the repaired copy with summary searches unchanged. Evidence: `docs/evidence/W04/second-vector.md` and `voice-and-fallback.md`.

**Done when.** The scorecard shows top-five at 36 of 40 or better on the same copy, the three seeded gates still pass, and the miss classes are written down.

**Evidence.** `docs/evidence/W04/vs-85-known-item-search.md` with the before and after scorecard, counts only.

## VS-86 Make the embedding text say what the memory is about
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W03-Build
- estimate: 6h
- depends: VS-85

**Why.** `compose_insight_embedding_text` builds the primary text from project, topic, workflow, context, the insight rows, entities and commands. In the traced capture the reviewed summary was missing, the insight rows were cut-off text, and one `commands` entry was several hundred characters of prose. The most descriptive sentence is the one thing the primary vector does not see.

**Do.**
1. Add `display_summary` to the primary text when it is not a placeholder (`is_placeholder_summary`).
2. Cap each list segment (commands, entities, files) so one long item cannot dominate.
3. Confirm no `context_thread: session` segment is written any more (2 of 147 older rows still carry one).
4. This changes the embedding document, so bump `EMBEDDING_DOCUMENT_VERSION`, update the Python mirror in `scripts/audit/embedding_bakeoff.py`, and plan the re-embed of existing rows.

**Done when.** Unit tests cover the composition, the three seeded gates pass, and the VS-85 scorecard improves on a re-embedded copy.

## VS-87 Retune hybrid fusion after the query prompt change
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 4h

**Why.** Removing the BGE instruction from MiniLM queries raised vector scores for real matches by 0.07 to 0.12. The fusion weights and the 0.25 strong-match bar were tuned against the old scores. Recall held, but MRR@10 moved 0.966 to 0.909 on knowledge-worker and 0.879 to 0.859 on software-engineer (`docs/evidence/W04/2026-10-06-query-prompt.md`).

**Result, 2026-10-06.** Swept and closed with no change; see `docs/evidence/W04/vs-87-fusion-retune.md`. The config weights named below are not read by the live path, which ranks with `FusionWeights` in `context_runtime`. No setting beat the defaults by more than one query. Follow-ups are listed in the evidence file.

**Found 2026-10-07.** The config weights belong to `HybridSearcher`, an older second search engine still used by the mobile companion, the legacy graph and the raw MCP search tool. Moving those callers onto `context_runtime` and deleting the old engine is a separate piece of work.

**Do (original).** Sweep `vector_weight`, `snippet_weight`, `keyword_weight` and the short-query weights on the three personas plus the vault scorecard. Hold out one persona when choosing, so the weights are not fitted to the test set (`tests/anti_overfitting.rs`). Re-record the references with the pinned tokenizer.

**Done when.** MRR@10 is at or above the pre-change value on all three personas with Recall@5 unchanged or better, and the references are re-recorded.

## VS-88 Align the BGE prefixes with the model card behind a rebuild guard
- assignee: anurupkumar
- labels: area::vault-search, type::bug, prio::p2
- milestone: W04-Prove
- estimate: 5h

**Why.** `embedding/prefixes.rs` uses "Represent this question for searching relevant passages: " for queries and "Represent this sentence: " for documents. BGE v1.5 specifies "Represent this sentence for searching relevant passages: " for queries and no document prefix. The owner chose on 2026-10-06 to leave this until it can be measured. ADR 019 may retire BGE in favor of EmbeddingGemma, so check that first.

**Do.**
1. Install the BGE model in a scratch models folder and record `make qa-retrieval-check QA_CHUNKS=1` before.
2. Change the prefixes and route the two call sites (`prefix_query_for_search`, `prefix_document_for_index`) through `query_text_for` and `document_text_for`.
3. The v5 reindex skips memories that already have rows (`should_skip_v5_reindex`), so add a guard that forces a full rebuild when the prompt contract changes. Old and new vectors must never be searched together.
4. Record after. Ship only if the numbers hold.

**Result, 2026-10-08: deferred, with the cheap half done.** No profile holds BGE rows, the BGE model is not installed on the owner's Mac, and ADR 019 has not chosen the chunk model. Measuring would mean a 1.3 GB download and a full chunk index on an 8 GB machine to tune a path nothing runs, for a model that may be replaced. So: the prefixes stay as they are; both call sites now go through `query_text_for` and `document_text_for`, so there is one place to change; the constants carry the model-card difference and the rule that the v5 tables are cleared before a change; the existing test pins both strings. The rebuild guard and the before and after numbers move to whichever model ADR 019 picks.

**Done when.** Before and after numbers are in the evidence file and a test proves a prompt change invalidates existing BGE rows.

## VS-89 Make intent and activity labels worth storing
- assignee: anurupkumar
- labels: area::vault-search, type::bug, prio::p1
- milestone: W04-Prove
- estimate: 5h

**Why.** On the vault copy, intent was empty for 64 memories and `unknown` for 55, so 81 percent carry no intent. Activity was `unknown` for 64 and used labels outside the allowed list (`observing`, `screen_review`). Intent and workflow are written into the embedding text. Tied intent scores are now broken deterministically (2026-10-06), but the heuristic in `infer_intent_analysis` has six weak rules.

**Do.**
1. Decide whether intent belongs in the embedding text at all when it is unknown. Skipping `unknown` is already done; measure dropping the segment entirely.
2. Normalize stored activity labels to `ACTIVITY_TYPES` on read and on write.
3. Either improve the rules with evidence from the scorecard or remove the field from display and ranking.

**Result, 2026-10-07.** See `docs/evidence/W04/vs-89-labels-and-intent.md`. The stored label list now equals the list the prompts offer, guarded by a test; the repair scan relabels older rows. Removing the intent or workflow segment moved no cell by more than two queries on the vault copy, so the embedded text is unchanged. The intent rules were left alone because the label affects neither retrieval nor the card. The personas were not used: they cannot show an embedding-text effect.

**Found 2026-10-07, the fallback.** The summary written without a model (`build_low_ram_semantic_fusion`) set the activity to "reviewing" for every capture, which stores as `reviewing_agent_output`, and wrote an intent nothing on screen stated. It now leaves the activity unknown and writes no intent; the repair scan relabels the 10 such rows. The model itself still chooses `reviewing_agent_output` for 52 of 158 memories, which is the open half of this ticket.

**Done when.** The scorecard shows no activity label outside the list, and an evidence note says whether intent helps retrieval on the three personas.

## VS-90 Make memory rewrites safe and clear the review backlog
- assignee: anurupkumar
- labels: area::vault-search, type::bug, prio::p1
- milestone: W04-Prove
- estimate: 6h

**Why.** `replace_memory_preserving_chunks` deletes a row and then inserts it. A crash between the two loses the memory. Review, repair and merge all use it. Separately, the vault copy held 27 `pending`, 29 `pending_visual_semantics` and 13 `review_failed` rows, so about half the vault has never been through a successful review.

**Do.**
1. Replace delete-then-insert with a LanceDB merge-insert (upsert on `id`) or an insert-then-delete of the old version, test-first with a failure injected between the two steps.
2. Report the review backlog in the inspector and find why `pending_visual_semantics` rows stay pending.
3. Check which LanceDB indexes exist on the memories table and whether the vector search is a flat scan at this size; record it in `docs/evidence/W04/storage-indexes.md`.

**Result, 2026-10-07.** Parent memories and chunk rows now use merge-insert keyed by id. The chunk regression test asserts a replacement creates exactly one table version, so the previous delete plus append implementation fails it. The Memory Vault graph inspector now shows visible `pending`, `pending_visual_semantics`, and `review_failed` totals. `pending_visual_semantics` rows are visual-only or visual-metadata fallback records that `review_skip_reason` deliberately skips for lack of text evidence; the review pipeline persists that status to keep them out of ordinary text review. That code is in the parallel session's `memory_review` lane, so this lane records the cause and leaves the review policy untouched. No live profile counts were read or copied. Index inventory and the 10k timing evidence are in `docs/evidence/W04/storage-indexes.md`.

**Done when.** The injected-failure test passes, and the backlog numbers are visible and explained.

## VS-91 Re-review weak summaries on the owner vault
- assignee: anurupkumar
- labels: area::local-models, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 4h
- depends: VS-90

**Why.** 55 of 147 summaries narrate ("The user is viewing...") and 17 are placeholders. A preview on a copy (`cargo run --example review_preview`) rewrote 9 of 12 weak rows into neutral summaries; 3 were refused by the grounding and narration guards and kept their old text. The evidence-based repair (`repair_truncated_summaries`) found 6 rows cut inside a token.

**Do.** After VS-90, back up the vault, run the repair with `--apply --allow-real-profile`, then queue the weak rows through the review worker. Compare the scorecard before and after. The reviewed summaries are short (24 words), so check that detail a person would search for is not lost.

**Result, 2026-10-06.** Applied to the owner vault with a backup: 34 reviewed, 5 refused by the guards, 17 skipped. Narrated summaries went from 55 to 26, cut summaries from 3 to 0, placeholders stayed at 18, and known-item search held at 37 of 40. The target is not met (30 percent).

**Found 2026-10-07.** Every skip comes from `review_skip_reason`: the row is a visual-only capture with no text for the review to check against. The counts line up with the placeholder rows (17 skips, 18 placeholders) and with the rows VS-90 reports as stuck in `pending_visual_semantics`; confirm row by row before relying on it. If it holds, the placeholder half of the target cannot move until the visual path is fixed.

**Result on a copy, 2026-10-07.** See `docs/evidence/W04/vs-91-weak-summaries.md`. 23 of the 26 narrated rows only needed the wording cleanup the cards already apply; the repair scan now does that to the stored text and re-embeds. Narrated went 26 to 3, and search by the earlier sentences still finds 37 of 40. Of the remaining weak rows, 7 are shown to the person (6 percent of shown rows) and 14 are already hidden as low signal. Applied to the real vault on 2026-10-07 with a backup; the same numbers hold there, and activity labels outside the list went from 7 to 0.

**Found 2026-10-07, how summaries open.** Only 17 of 132 visible summaries opened with a past-tense verb; 35 opened "Reviewing ...". The display cleanup and the repair scan now put a leading activity verb into the past tense and store the summary as shown: 50 of 132 on a repaired copy. Sentences about the window, the capture or the OCR fall back to the title. A second repair pass (56 rows on the copy) is ready and waits for the owner, with FNDR closed and a backup. Evidence: `docs/evidence/W04/voice-and-fallback.md`.

**Done when.** The scorecard shows narrated and placeholder summaries under 10 percent combined and known-item search no worse.

## VS-92 Write one session memory from a session's moments
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p2
- milestone: W05-Retro
- estimate: 8h
- depends: VS-91

**Why.** The Vault now groups captures no more than 30 minutes apart into one row for display only. The row still shows the newest moment's sentence. Eleven captures of one piece of work are one memory with eleven pieces of evidence.

**Do.** When a session goes quiet, write one session record from its moments: what was worked on, what was decided, where it stopped. Keep the moments as evidence beneath it. Stop adding a new card when a new moment adds no new fact. Link sessions across apps that share a project within minutes.

**Progress, 2026-10-08.** Decided: the session summary is composed from the session's own moments and no model writes it. The on-device model was measured on the same kind of job for the briefing and invented advice, copied its notes, and reported an open task as done. `sessionDigest` (`src/domains/memory-vault/sessionDigest.ts`) gives a session row its moments, its length in minutes, its distinct files, and the most detailed earlier sentence when that says something the row's own line does not. Nothing is stored, so it covers every past session and can never disagree with its moments. Still to do: counts of decisions, next steps and errors (the card does not carry them yet), and linking sessions across apps.

**Done when.** The Vault session row shows the session's own summary with counts of decisions, next steps and errors, and the moments still open individually.

## VS-93 Stop prose landing in the commands field, and explain the short clean text
- assignee: anurupkumar
- labels: area::vault-search, type::bug, prio::p2
- milestone: W04-Prove
- estimate: 3h

**Why.** In the traced capture a `commands` entry was a long natural-language request, not a command, and it went into the embedding text. The same record held 67 characters of `clean_text` beside a long `memory_context` after a merge.

**Do.** Validate `commands` entries (length cap, must look like a command) in `extraction_evidence::finalize_extraction`. Trace what the merge path does to `clean_text` and decide whether that is intended.

**Result, 2026-10-07.** `extraction_evidence::is_command_like` decides: one line, at most 160 characters, does not open like a sentence, and is either short or carries a flag, path or assignment. The capture validator drops other entries and records `commands_not_command_like`; the embedded text applies the same check, so rows stored earlier are cleaned the next time they are re-embedded. On the merge question: `merge_story_text` only keeps or extends `clean_text`, it never shortens it. A short `clean_text` beside a long `memory_context` therefore comes from a capture with little readable text whose context was written from the image, which is intended. This was read from the code, not re-checked on the traced record.

**Done when.** A test rejects a prose `commands` entry, and the merge behavior is documented or fixed.
