# 036. Verbatim memory: keep the captured text beside the summary

Status: Proposed (2026-10-22). Requested by Kunj. Not yet reviewed by the team.

## Context

A page that said "Phone: ..." and "Passport number: ..." was captured and retrieved, and the model was handed only a one-line summary. Nothing that fills a form, answers for an exact value or quotes a page can work from that.

The text is gone by design, not by accident:

- A capture record is built with the full cleaned text in `clean_text` (`capture/mod.rs`, `enriched_clean_text`, near line 4439).
- The flush calls `Store::add_batch_and_get_count`, which runs `normalize_record_for_index`, which runs `compact_memory_record_payload` (`memory_compaction.rs`, lines 53 to 75). For every row that is not a note or an import, that sets `text` to empty and `clean_text` to `compact_clean_text(...)`: the model's snippet cut to 360 characters when the summary came from an LLM or VLM, or the first 420 to 560 characters of the cleaned text otherwise.
- Nothing saves the text anywhere first. The one non-test writer of `memory_chunks_v1_bge_1024` is the explicit v5 reindex (`ipc/commands/maintenance.rs`, line 313), and it chunks the already compacted `clean_text`, so even a reindexed profile would hold summaries, not pages. `Store::insert_memory_chunk` (`lance_store/mod.rs`, line 1087) is misnamed: it writes a parent memory through `add_batch`. The `support_chunks=2` in the capture log (`capture/mod.rs`, line 4205) counts texts folded into the `support_embedding` centroid; they are never persisted. So `list_chunks_for_memory` returns 0 on a real profile.

## Decision

Add a small table, `memory_verbatim_v1`, with one row per memory: `memory_id`, `text`, `byte_len`, `captured_at`. No vector column, no full-text index. Capture writes the cleaned text there after the memory has landed in the memories table. Consumers read it by memory id through `verbatim::text_for_memory`.

- **Switch.** `config.verbatim.enabled`, shown in Settings, Privacy, as "Keep the full text of what FNDR sees on this Mac". **Default on** (decided 2026-10-22 on the owner's instruction to pick defaults for a good everyday experience; the first draft shipped it off). It is bounded (6 KB a memory, 7 days, 128 MB), gated, local, and easy to turn off; leaving it off meant memory could still not answer for an exact value, which is the failure this ADR exists to fix. Keeping the page text of ordinary pages widens what memory holds, and the safety gate does not catch personal data such as a passport or phone number (that is the point of the feature). An existing profile must not start keeping more on upgrade. Turning it off deletes every stored text at once; memories and summaries stay.
- **What is stored.** Screen and browser captures only (not notes, not imports). Text is trimmed and cut to `max_bytes_per_memory` (6144 bytes, on a character boundary).
- **Gate at write time.** The switch; Private Mode (`is_incognito`, read at flush, because a batch built before Private Mode was switched on can still flush); internal FNDR windows; the current blocklist for app, URL and window title; and `safety_gate::evaluate` run again on the exact string stored, which must return `Allow`. A record the store dropped as a duplicate or low signal leaves no row (the write checks which ids landed).
- **Merges.** A new capture merged into a persisted memory deletes the old row (`delete_memory_by_id` removes the text), so `capture/mod.rs` reads the older text first and writes older text plus the new capture under the merged id, capped again.
- **Retention.** `retention_days` (default 7, matching the default memory retention) counts from the memory's capture time. A sweep at startup and in the daily retention job deletes older text; the memory and summary stay. The default memory retention is 7 days (`Config::retention_days`), so on a default profile the effective life of text is the shorter of the two: a deleted memory takes its text with it.
- **Budget.** `total_budget_mb` (default 128). The sweep deletes the oldest text first until the table fits.
- **Deletion.** `delete_memory_by_id`, `delete_memories_matching` (imports), `delete_memories_by_domain`, `delete_older_than` and `delete_all` all delete the row. This covers the vault delete, forget, range erase, blocklist back-deletion and the retention sweeps, which call those. A soft delete through `replace_memory_preserving_chunks` deletes it too. `compact_and_prune` (daily, and with zero days on erasure) now covers the table, so a deleted row does not survive in an older table version.
- **Reads.** `verbatim::text_for_memory(state, id, max_bytes)` loads the memory, applies `context_runtime::context_memory_is_visible` (soft delete, blocklist, quality gate, the `memory_is_visible` rule) with the current blocklist, refuses text older than the retention window even if no sweep has run, and caps the result. The raw reader `Store::verbatim_text_unchecked` is listed in `src/dev/memoryReadDrift.test.ts` so a new caller fails until it is explained.
- **First consumer.** `smart_fill/mod.rs::memory_notes` adds each retrieved memory's stored text to its note as `page text (untrusted)`. The base here has `memory_notes`, not the `note_for` of `kunj/smart-fill-forms`; there the same one call (`crate::verbatim::text_for_memory(state, &result.id, 3000)`) goes where the note text is built. The existing "untrusted" labelling of notes is kept; no prompt in `inference/prompts.rs` changed.

## Options not taken

- **Use `memory_chunks_v1_bge_1024`.** It cannot serve without changing ranking. Its `embedding` column is a required 1024-d BGE vector (`validate_memory_chunk_vectors`), and live capture only has 384-d MiniLM vectors; zero vectors would pass the length check and pollute `chunk_vector_search`. Rows in it are a ranking input: `chunk_keyword_search` runs BM25 over chunk text, and `has_chunk_retrieval_index` flips the chunk route on and raises the strong-match bar once rows exist (`context_runtime/chunk_route.rs`, `retrieve.rs`). And the v5 reindex deletes and rewrites chunks per memory from stored text (`maintenance.rs`, line 306), which would erase verbatim text. Embedding chunks at capture with BGE-large is also a different cost and a different contract (ADR 019).
- **Stop clearing `text` in `compact_memory_record_payload`.** Needs no migration, but every `replace_memory_preserving_chunks` (review, repair, import) runs the same compaction and would wipe it again, and every bulk reader and serializer of `MemoryRecord` would carry up to 6 KB per row past the visibility rules. A separate table gives one door.
- **Redact detected personal data before storing.** Not built: the point is that a form can be filled with the person's own phone number. The gate still refuses banking, health, auth and credential pages.
- **Default off, or on only for new profiles.** Considered and rejected on 2026-10-22: a feature that keeps nothing until someone finds a switch does not fix the design, and splitting new from existing profiles needs a flag the config cannot carry without a migration.

## Privacy consequences

- The page text of ordinary, allowed pages is now on disk for up to the retention window, in the LanceDB directory, readable by anything that can read it. It is not encrypted by FNDR (the memories table is not either); FileVault is the protection, as for the rest of memory. The Settings text says it stays on this Mac, how long, and what stops it.
- It cannot reach a model except through a consumer that calls `text_for_memory`. Today that is Smart Fill, which already sends memory notes to ChatGPT; with the switch on, those notes can now include page text. The Smart Fill setting text should say so.
- The safety gate matches strings (for example `password:`, `api_key`, token prefixes) and domains. It does not recognize a passport number, an address or a phone number; the first two are common on the pages this feature is for.
- Not covered: text of a memory that was captured before the switch was turned on does not exist and is not recovered.

## Storage estimate

Measured with `cargo test --lib verbatim::tests::measure_disk_growth -- --ignored --nocapture` (synthetic pseudo-random words, 2,000 rows written 8 at a time in 250 flushes, each row about 4.1 KB): 8,191,662 bytes of text; 11,247,164 bytes on disk before compaction (250 small versions, +37%); 8,310,501 bytes after `compact_and_prune` (+1.5%). Real text compresses no better than this, because Lance does not compress these strings.

So a row costs at most about 6.2 KB on disk after the daily compaction. Memories per day is an assumption, not measured on a real profile: at 500 a day, the cap gives at most 3 MB a day, 90 MB over 30 days. The 128 MB budget holds that with room. On a default profile (7-day memory retention) the ceiling is about 21 MB.

## Retrieval eval

Search, ranking and embedding text are not touched, so `make qa-retrieval-check` was not run. The new table has no vector and no full-text index, nothing in `context_runtime` routes reads it, `compact_memory_record_payload` and `inference/model_config.rs` are unchanged, and an existing memory is embedded and ranked exactly as before. What would invalidate this: any future change that lets a retrieval route read `memory_verbatim_v1`.

## Open questions for the team

- On upgrade, every profile starts keeping page text (the switch defaults on when the config has no `verbatim` block). Is a one-time notice in the app wanted?
- Should Smart Fill's own setting say that stored page text may go to ChatGPT with a request?
- Is a longer window than the 7 day memory retention ever wanted? It only matters for profiles that raised `retention_days`.
