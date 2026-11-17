# Embedding coverage map (EM-01)

Every code path that writes a memory row, which vectors and chunk rows it writes, from which text, and whether it goes through the standard composer. Read from the code on `origin/main` at commit `2bd496f6`. File and line numbers are for that commit; function names are stable across edits. The "Verified" section at the end says which claims were run and which were only read.

Checked by `python3 scripts/audit/check_embedding_coverage.py`, which fails if a production file that writes memory or chunk rows is missing from the tables below.

## Vocabulary

- **Primary, snippet, support** are the three text vectors on the parent row (`embedding`, `snippet_embedding`, `support_embedding`, 384 dimensions, MiniLM). **Image** is `image_embedding` (CLIP, optional).
- **Composer** is `compose_memory_embedding_document` in `src-tauri/src/memory_embedding_document.rs:181`. It builds the primary, snippet, support, chunk-source, and visual-semantic texts from one `MemoryRecord`.
- **Chunk rows** are rows in `memory_chunks_v1_bge_1024`, written only by `upsert_memory_chunks` (BGE, 1024 dimensions). Their parent rows live in `memories_v5_bge_1024`, written by `add_v5_batch_preserving_ids`.
- **Chunk config** means the writer passes `Some(&chunking_config)` to the composer. Passing `None` means the composed document has no chunk plan.

## 1. Writers that create new memories

| # | Writer | Where | Primary / snippet / support | Image | Composer | Chunk rows | Failure behavior |
|---|---|---|---|---|---|---|---|
| W1 | Normal screen capture | `capture/mod.rs` seed `:4004`, composer `:4142`, record `:4443`, flush `:2176` | Embedded from composer inputs through `embed_text_inputs_with_memo` (`:4792`); primary is `weighted_primary_embedding` (`:1860`) | CLIP, zero vector on error (`:4252-4260`) | Yes, with chunk config | None | A capture gate blocks frames while the embedder is missing (`embedder_gate_*` tests, `:8692`). Past the gate, `embed_text_inputs_with_memo` still returns zero vectors when the backend is not real or an input is empty (`:4799-4806`, `:4830`), and the record is stored |
| W2 | Low-OCR visual capture | `capture/mod.rs` seed `:490`, composer `:519`, record `:591` | Same helper as W1 | From the captured frame | Yes, with chunk config | None | Same zero fallback as W1 (`:535-545`) |
| W3 | URL-only surface capture | `capture/mod.rs:2558` (`summary_source: "url_only"`, `source_type: "browser_url_only"`) | All three set to `vec![0.0; EMBEDDING_DIM]` at record build (`:2577-2582`) | Zero vector | No | None | Stored with zero vectors by design; no re-embed path found in this audit |
| W4 | Merge of a new frame into an existing memory | `merge_or_append_memory_record` `capture/mod.rs:4891`; seed `:5450`; composer `:5515`; writes `:4969`, `:5048`, `:5120` | Recomputed only when `recompute_embedding` and a real embedder (`:5517`); otherwise keeps the old vectors | Kept from existing or incoming | Yes, chunk config is `None` | None, and existing chunks are not touched | Text can change while vectors and chunks stay old |
| W5 | Downloads | `downloads.rs:411` (`build_download_record`), embed `:497-555`, write `:567` | Own helper texts (`compact_summary_embedding_text`, `support_embedding_texts_with_config`) embedded with `embed_batch_with_context`; mean-pooled support | Zero vector (`:443`) | No | None | Embed failure or missing embedder writes zero vectors for all three (`:530-552`) |
| W6 | Meetings | `meeting/mod.rs:2175`; vectors from `transcript_embeddings_for_embedder` (`:2028`) | Own path over the transcript, summary, and support texts | Zero vector | No | None | No embedder, or embed failure, gives `zero_transcript_embeddings` (`:2083`) |
| W7 | Meta glasses photo import | `ipc/commands/glasses_import.rs` composer `:365`, write `:576` | Composer inputs through `embed_batch_with_context`; embed error aborts with `embed: ...` | CLIP, checked non-zero at `:558` | Yes, with chunk config | None, although it asks for a chunk plan | Aborts the import, so no zero row |
| W8 | MCP `remember` (agent notes) | `mcp/remember.rs` embed `:331-362`, composer `:573`, write `:363` | Composer inputs; rejects the note when any vector is low-signal | Zero vector | Yes, chunk config `None` | None | Refuses the note when the real embedder is missing |
| W9 | Archive and app import | `import/runner.rs` composer `:65`, writes `:503` (new) and `:513` (changed) | Composer inputs; drops the row (`unembeddable`) when the embedder is missing or a vector is low-signal | Zero vector | Yes, chunk config `None` | None; for a changed row it first deletes its chunks, then replaces the row | Refuses to store; counts `no_usable_embedding` |
| W10 | Companion manual memory (iOS and other devices) | `companion/handlers/memories.rs` `build_manual_record` `:150`, write `:75` | **None set.** The record uses `..MemoryRecord::default()`, and the default is already 384 zeros, so the row is stored as an all-zero row (run, see Verified) | Zero vector | No | None | Healed only later by the maintenance pass R3 and R4 (below), not at write time |
| W11 | Archive restore | `privacy/export.rs` `restore_records` `:90` | Copies vectors from the archive as exported | As exported | n/a | None | Rows restored without chunks |

## 2. Writers that rewrite existing memories

| # | Writer | Where | What it touches | Chunk rows |
|---|---|---|---|---|
| R1 | Memory review | `memory_review/pipeline.rs` `review_one_memory_with_mode` `:217`, writes `:239`, `:314`; `persist_failure` `:422` | Recomposes `embedding_text` (composer, chunk config `None`) but replaces only the primary vector (`EM-13`); snippet and support are not refreshed | `replace_memory_preserving_chunks` deliberately leaves chunks alone, so they go stale |
| R2 | Truncated-memory repair | `memory_review/repair_truncated.rs:278` | Rewrites the row | Same as R1 |
| R3 | Memory repair backfill | `ipc/commands/maintenance.rs` `run_memory_repair_backfill_for_state` `:880`; composer with chunk config `:1226`; `delete_all` then `add_batch_preserving_ids` `:1341` | Re-embeds a vector only when it is zero or low-signal (`is_low_signal_embedding`, `:1227`, `:1243`, `:1259`), so it heals zero rows such as W3 and W10 | `delete_all` (`lance_store/mod.rs:2543`) clears the chunk table, and the re-add writes none, so a repair run drops every chunk. It runs automatically: `reclaim_memory_storage_silent` (`main.rs:339`) starts 60 seconds after launch and repeats every 6 hours, and calls this repair first (`maintenance.rs:1794`) |
| R4 | Storage reclaim and compaction | `ipc/commands/maintenance.rs` `reclaim_memory_storage_for_state` `:1755`; composer with chunk config `:1880`; write `:2139` | Re-embeds primary and support only when zero or low-signal, and always re-embeds the snippet. A text change alone does not re-embed primary or support | Preserves existing chunks only |
| R5 | Context and insight rebuild | `ipc/commands/quality.rs` `rebuild_memory_context_for_range` `:922`, write `:1004`; `backfill_insight_layers_for_range` `:1159`, write `:1205` | Rewrites `embedding_text` from the composer (chunk config `None`) but does not re-embed | Preserved, may be stale |
| R6 | Reopen target backfill | `memory/reopen_backfill.rs:151` | Rewrites reopen fields only | Preserved, unaffected |

## 3. The one writer of chunk rows

| Writer | Where | Notes |
|---|---|---|
| `reindex_memories_v5` | `ipc/commands/maintenance.rs:153`; v5 parent write `:295`; chunk write `:313` | Composer with chunk config; embeds parent roles and chunk rows with BGE. Nothing in the app calls it, so on real profiles the chunk tables stay empty. |

## 4. Store-level behavior that affects every writer

| # | Behavior | Where | Effect |
|---|---|---|---|
| S1 | A vector whose length is not the expected dimension is replaced with all zeros and a warning | `storage/lance_store/normalize_embed_migrate.rs` `normalize_vector_dim` `:1179`, applied by `normalize_record_for_index` `:104` on every `add_batch*` | A vector of the wrong size silently becomes a zero-vector row. The store never rejects a missing, wrong-sized, or zero vector. |
| S2 | Rows with fewer than 6 alphanumeric characters of text are dropped | `is_indexable_memory_record` `:1160` | Not a vector rule; low-text rows are never stored |
| S3 | Zero vectors are detected, not blocked at the store | `memory_compaction.rs` `is_low_signal_embedding`, used by W8, W9, and the maintenance paths R3 and R4 (`maintenance.rs:1227-1920`) | W1 to W6 and W10 do not run the check before writing |
| S4 | Deleting a memory removes its chunk rows | `delete_memory_by_id` and `delete_all` (`lance_store/mod.rs:2543`) | Covered by EM-10 tests |

No production demo seeder exists. `FNDR_DEMO_REVIEW_BACKFILL` (`main.rs:415`) only queues stored memories for review.

## 5. Gaps and the ticket that fixes each

| Gap | Evidence | Ticket |
|---|---|---|
| G1. No writer except `reindex_memories_v5` writes chunk rows; nothing calls it | Section 3; W1 to W11 all write none | EM-03, EM-05 |
| G2. Zero vectors are stored: W3 always, W1, W2, W4, W5, W6 on embedder failure, W10 through S1 | Failure columns above; S1, S3 | EM-06 |
| G3. W3, W5, W6 build their own vectors and skip the composer | Composer column | EM-07 |
| G4. W4, W8, W9 call the composer with `None`, so they never plan chunks | Composer column | EM-03 (chunking happens in the worker, not at compose) |
| G5. Text changes (merge W4, review R1, R5) do not invalidate chunks, and no hash of the chunked text is stored | R1, R5, W4 | EM-04 |
| G6. Review replaces only the primary vector (R1), and the review and rebuild paths do not share one document | R1; EM-13 | EM-13 |
| G7. The repair backfill (R3) wipes all chunk rows and does not restore them, and it runs automatically every 6 hours | R3, Verified V2 | EM-03, EM-05 (re-enqueue after repair, or stop the wipe), EM-10 |
| G8. Companion manual memories (W10) are stored as all-zero rows (the `MemoryRecord` default) and stay that way until a maintenance pass runs, up to 6 hours after launch | W10, Verified V1 | EM-06, EM-07 |
| G9. Image vectors are zero when CLIP fails (W1, logged and stored with status `error:...`) or by design (W3, W5, W6), and no pending state exists | W1 `:4252-4260`; W5 `:443` | Out of scope here; note for EM-06 |

## 6. Verified

How each uncertain claim was checked.

| # | Claim | Status | How |
|---|---|---|---|
| V1 | A companion-style record (`MemoryRecord::default()` vectors) is stored as three all-zero vectors | **Run** | Scratch integration test on a temporary store: default `embedding` length is 384, `add_batch_and_get_count` inserts it, and the stored primary, snippet, and support vectors are all zero. The test was deleted afterward. |
| V2 | `delete_all` followed by a re-add leaves no chunk rows | **Run** | Same scratch test: one chunk row before `delete_all`, zero after `delete_all` and `add_batch_preserving_ids` of the same memory |
| V3 | The repair backfill runs `delete_all` on every maintenance pass | **Read, not run** | `run_memory_repair_backfill_for_state` has early returns only for "already running" and "no memories"; `delete_all` follows unconditionally. Running it needs a real embedder, because it begins with `shared_real_embedder()?` |
| V4 | The maintenance pass runs automatically | **Read, not run** | `main.rs:339` calls `reclaim_memory_storage_silent` after `MAINTENANCE_FIRST_DELAY` (60 s) and every `STORAGE_RECLAIM_INTERVAL` (6 h); `reclaim_memory_storage_for_state` calls the repair first |
| V5 | R4 re-embeds only zero primary and support vectors, plus every snippet | **Read, not run** | `maintenance.rs:1894-1925` |
| V6 | W3 (URL-only) and W10 rows have no re-embed path at write time | **Read, not run** | No embed call in either writer. R3 and R4 heal them later, if the real embedder is loaded |

## 7. Open questions for the next tickets

1. Confirm V3 on a seeded profile before EM-03 designs around it: a profile with chunk rows, then one maintenance pass, then a chunk count.
2. W3 is stored with zero vectors deliberately. EM-06 must decide between embedding the title and URL at write time (EM-11 row E03 expects this) and storing as `pending_embedding`.
3. Between a zero-vector write and the next maintenance pass (up to 6 hours), do search and Ask return these rows with a zero score, or exclude them? EM-06 needs this answer for its "search excludes pending memories" step.
