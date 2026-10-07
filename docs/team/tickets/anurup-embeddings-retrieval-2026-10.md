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

**Done when.** The injected-failure test passes, and the backlog numbers are visible and explained.

## VS-91 Re-review weak summaries on the owner vault
- assignee: anurupkumar
- labels: area::local-models, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 4h
- depends: VS-90

**Why.** 55 of 147 summaries narrate ("The user is viewing...") and 17 are placeholders. A preview on a copy (`cargo run --example review_preview`) rewrote 9 of 12 weak rows into neutral summaries; 3 were refused by the grounding and narration guards and kept their old text. The evidence-based repair (`repair_truncated_summaries`) found 6 rows cut inside a token.

**Do.** After VS-90, back up the vault, run the repair with `--apply --allow-real-profile`, then queue the weak rows through the review worker. Compare the scorecard before and after. The reviewed summaries are short (24 words), so check that detail a person would search for is not lost.

**Done when.** The scorecard shows narrated and placeholder summaries under 10 percent combined and known-item search no worse.

## VS-92 Write one session memory from a session's moments
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p2
- milestone: W05-Retro
- estimate: 8h
- depends: VS-91

**Why.** The Vault now groups captures no more than 30 minutes apart into one row for display only. The row still shows the newest moment's sentence. Eleven captures of one piece of work are one memory with eleven pieces of evidence.

**Do.** When a session goes quiet, write one session record from its moments: what was worked on, what was decided, where it stopped. Keep the moments as evidence beneath it. Stop adding a new card when a new moment adds no new fact. Link sessions across apps that share a project within minutes.

**Done when.** The Vault session row shows the session's own summary with counts of decisions, next steps and errors, and the moments still open individually.

## VS-93 Stop prose landing in the commands field, and explain the short clean text
- assignee: anurupkumar
- labels: area::vault-search, type::bug, prio::p2
- milestone: W04-Prove
- estimate: 3h

**Why.** In the traced capture a `commands` entry was a long natural-language request, not a command, and it went into the embedding text. The same record held 67 characters of `clean_text` beside a long `memory_context` after a merge.

**Do.** Validate `commands` entries (length cap, must look like a command) in `extraction_evidence::finalize_extraction`. Trace what the merge path does to `clean_text` and decide whether that is intended.

**Done when.** A test rejects a prose `commands` entry, and the merge behavior is documented or fixed.
