# MEM-07: finalized-memory contract test output

`assert_memory_contract` (`src-tauri/src/memory_quality.rs`) checks invariants 1
through 7 from the WS5 plan section 4 on a single `MemoryRecord`. Invariants 8,
9, and 10 are process invariants (merge, reprocessing, deletion) and are
covered by their own tests against the real merge, replay, and delete paths
instead, since they cannot be observed on one record snapshot.

## Invariants 1-7: `memory_quality::tests::contract_*`

```
test memory_quality::tests::contract_accepts_a_fully_valid_record ... ok
test memory_quality::tests::contract_rejects_empty_app_name ... ok
test memory_quality::tests::contract_rejects_raw_screenshot_path ... ok
test memory_quality::tests::contract_rejects_wrong_dimension_embedding ... ok
test memory_quality::tests::contract_rejects_all_zero_embedding ... ok
test memory_quality::tests::contract_rejects_missing_embedding_manifest ... ok
test memory_quality::tests::contract_rejects_invalid_lifecycle_state ... ok
test memory_quality::tests::contract_accepts_capped_visual_semantics_failed_scores ... ok
test memory_quality::tests::contract_rejects_uncapped_visual_semantics_failed_scores ... ok
test memory_quality::tests::contract_rejects_unsupported_low_confidence_field ... ok
test memory_quality::tests::contract_allows_unsupported_field_at_high_confidence ... ok
test memory_quality::tests::contract_rejects_meta_narration_in_memory_context ... ok
test memory_quality::tests::contract_rejects_non_canonical_activity_type ... ok
test memory_quality::tests::contract_rejects_url_with_credentials ... ok
test memory_quality::tests::contract_rejects_reopen_url_with_credentials ... ok
```

Invariant 7 needed a real fix, not just a check: `capture::strip_url_credentials`
now strips userinfo and credential-looking query/fragment parameters from the
captured browser URL at the point it enters the pipeline
(`capture/mod.rs`, where `browser_url` is read), closing a gap that the
existing storage-time `canonicalize_index_url` (which strips the whole query
string) does not reach: `reopen_url` is derived from the raw url earlier in
the capture loop and, once `reopen_kind` is resolved, is never
re-derived from the sanitized value at storage time.

```
test capture::tests::strip_url_credentials_removes_credential_query_params_keeps_others ... ok
test capture::tests::strip_url_credentials_drops_bare_query_string_when_only_credential_params ... ok
test capture::tests::strip_url_credentials_strips_userinfo ... ok
test capture::tests::strip_url_credentials_keeps_at_sign_in_path_untouched ... ok
test capture::tests::strip_url_credentials_strips_credential_fragment ... ok
test capture::tests::strip_url_credentials_is_a_no_op_on_a_clean_url ... ok
test capture::tests::url_has_credential_leak_detects_unstripped_urls ... ok
```

## Invariant 8: merging keeps the union of source ids

Real bug found and fixed. `merge_memory_records_with_policy` always kept
`existing.id` and silently dropped `incoming.id` with nothing recording where
it went. Any citation already holding the dropped id (a search result, an
agent's cited `memory_id`) would 404 forever after a merge. Fixed by
appending the dropped id to `consolidated_from`, and by adding a fallback in
`Store::get_memory_by_id`: on a primary-key miss, it now also queries
`array_contains(consolidated_from, '<id>')`.

```
test capture::tests::merge_keeps_incoming_id_in_consolidated_from_for_citation_redirect ... ok
test capture::tests::merge_of_already_consolidated_records_does_not_lose_earlier_ids ... ok
test storage::lance_store::tests::get_memory_by_id_redirects_through_consolidated_from_after_a_merge ... ok
```

## Invariant 9: reprocessing a frame twice does not create a second memory

```
test reprocessing_the_same_frame_twice_does_not_create_a_second_memory ... ok
```
(`src-tauri/tests/merge_replay.rs`, run via `cargo test --test merge_replay`.)

## Invariant 10: deletion removes chunks, vectors, and graph nodes

Real bug found and fixed. `legacy::GraphStore` (the graph store actually wired
into `AppState`, distinct from the unused `graph::graph_store::GraphStore`)
had no way to delete a single node: `delete_memory` removed the row and its
chunks but left the memory's own graph node (`memory:<id>`) and its edges
behind. Added `Store::delete_graph_nodes` and
`GraphStore::delete_memory_node`, which delete only the memory's own node
(`memory:<id>`) and edges touching it, leaving shared nodes (its session, a
visited url) alone since other memories may still reference them. Wired into
both `delete_memory` (single id) and `add_to_blocklist`'s retroactive
`delete_memories_by_domain` path.

```
test ipc::commands::memory::tests::delete_memory_logic_removes_the_memory_graph_node_but_keeps_the_shared_session_node ... ok
```

## Full suite

```
$ cd src-tauri && cargo test --lib
test result: ok. 696 passed; 0 failed; 7 ignored; 0 measured; 0 filtered out; finished in 2.57s

$ cargo test --test merge_replay
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.13s

$ cargo build --bins
Finished `dev` profile [unoptimized] target(s) in 28.34s
```

`make test` was not run in full for this evidence: the frontend suite has
unrelated, pre-existing failures from concurrent in-progress UI work on
`main` (`MemoryCard.compact.test.tsx`, accessible-name assertions), not
touched by this ticket. All files this ticket changed are under `src-tauri/`.
