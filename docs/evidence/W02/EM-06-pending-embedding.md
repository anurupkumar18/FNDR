# EM-06 never store a memory with a zero or missing vector

Build: branch `minh/em-06-no-zero-vector-rows`, on `origin/main` at `2bd496f6`. Synthetic data only; no owner vault was read or changed.

## What changed

| Change | Where |
|---|---|
| New embedding status `pending_embedding` (`EmbeddingStatus::PendingEmbedding`). A zero or wrong-sized primary vector now maps to it. Before, it mapped to `unavailable`. | `src-tauri/src/memory_embedding_document.rs` (`text_embedding_status`, `status_label`, `status_severity`, retrieval multiplier 0.0) |
| Vault health reports `pending_embedding_pct` and `unmarked_zero_vectors_pct` per parent table, beside the unchanged `zero_or_missing_vectors_pct`. | `scripts/audit/vault_health.py` |

## Why no per-writer change

The store already rebuilds the embedding manifest on every write from the vector it is given: `normalize_record_for_index` (`storage/lance_store/normalize_embed_migrate.rs`) calls `text_embedding_status(&normalized.embedding)`. So every writer in `docs/product/embedding-coverage.md` (W1 to W10, and the rewriters R1 to R6) gets the marker with no change of its own, including future writers. A rewrite with a real vector rebuilds the manifest, so the marker clears with no extra code.

## How "0%" is measured

The ticket asks for zero vectors at 0% after a forced embedder failure. A row cannot be stored without a vector column, so a failed row still holds zeros. The measurable meaning is: **no zero vector is unmarked**. `unmarked_zero_vectors_pct` is the share of rows with a zero or missing primary vector and no `pending_embedding` marker. `pending_embedding_pct` is the share that are marked. `zero_or_missing_vectors_pct` is unchanged, so existing comparisons still hold.

## Search

| Claim | Status | How |
|---|---|---|
| Semantic search never returns a pending memory | Already true before this change; now guarded by a test | With cosine distance, a zero vector gives a NaN distance and the store drops it. A probe on a store holding one real and one zero-vector row returned only the real one for both `vector_search` and `snippet_vector_search`. Test: `vector_route_skips_memories_pending_an_embedding`. |
| Keyword search still finds a pending memory | Already covered | `keyword_route_returns_seeded_hits` stores a zero-vector row and finds it. |

This answers open question 3 in `embedding-coverage.md`: rows written with zero vectors are excluded from semantic search while they wait, and found by keyword search.

## Red, then green

| Step | Result |
|---|---|
| New store tests written first | `cargo test --lib pending_embedding` failed to compile: `no variant named PendingEmbedding` |
| Variant and mapping added | 3 of 3 targeted tests pass |
| Python tests written first | 4 new tests errored on `KeyError: 'unmarked_zero_vectors_pct'` / `'pending_embedding_pct'`; 12 existing passed |
| Metrics added | 119 of 119 audit script tests pass (1 skipped, existing) |
| Vector-route test | Passed on first run, so it is a regression guard, not a red-green test (see Search) |

Tests added:

- `storage::lance_store::tests`: `zero_primary_vector_is_marked_pending_embedding`, `wrong_sized_primary_vector_is_marked_pending_embedding`, `real_primary_vector_is_not_marked_pending`, `pending_marker_keeps_an_existing_manifest_and_is_cleared_by_a_real_vector`, `pending_marker_preserves_other_raw_evidence_keys`, `embedder_failure_then_success_moves_a_stored_memory_from_pending_to_ready`
- `context_runtime::vector_route::tests::vector_route_skips_memories_pending_an_embedding`
- `scripts/audit/test_vault_health.py`: `test_pending_embedding_rows_are_not_counted_as_unmarked_zero_vectors`, `test_zero_vector_without_a_pending_marker_is_reported_as_unmarked`, `test_pending_marker_on_a_row_with_a_real_vector_is_not_unmarked`, `test_report_and_render_show_pending_and_unmarked_shares`

## End to end, with the real writer and the real report

A throwaway integration test (deleted afterward) wrote three rows to a temporary store through `add_batch_preserving_ids`: one with a real vector; one with zero vectors, as a writer produces when its embedder fails; and one that failed and was then replaced by a retry with a real vector. `scripts/audit/vault_health.py` ran on that store:

| table | rows | zero/missing vectors | pending embedding | unmarked zero |
|---|---:|---:|---:|---:|
| `memories_v4_minilm_384` | 3 | 33.3% | 33.3% | **0.0%** |

The retried row is not counted as zero or pending, so the marker cleared on rewrite. The Rust writer and the Python reader agree on the manifest JSON shape.

## Other test runs

| Command | Result |
|---|---|
| `cargo test --lib` | 2067 passed, 5 failed (see below), 40 ignored |
| `cargo test --test embedding_audit --test retrieval_routes --test search_flow --test store_graph_regressions --test chunk_retrieval_quality --test memory_browse --test merge_replay --test low_signal_surface` | all pass |
| Python: `unittest discover` over `scripts/audit` | 119 passed, 1 skipped |

The 5 failures are `import::sources::mail::tests::*`. They are not caused by this change: the fixture email is dated 14 Oct 2026 and the importer scans a 30-day window, so they began failing on 13 Nov 2026. Not fixed here.

## Retrieval gate

`make qa-retrieval-check PERSONA=knowledge-worker` (reseeded synthetic profile): **PASS**. Recall@5 is 1.000 on search, ask, and retrieve, unchanged from the reference, and no query that a path found in its top ten became a miss. MRR@10 is 0.909 against a reference of 0.966 (not gated). The reference was recorded on a smaller case set (39 cases; 43 now), so that MRR difference is not attributed to this change, and I did not run the gate on the base commit to separate the two.

## Not done

- Writers still store zero vectors for the failure cases. This ticket makes them visible and keeps them out of semantic search; EM-05 backfills them, and EM-07 stops URL-only captures and downloads from needing to fail.
- The BGE v5 parent table has its own normalization and was not changed.
- Image vectors still fall back to zero without a pending state (G9 in `embedding-coverage.md`).
