# MEM-01: post-capture live-path audit

**Repository and baseline:** FNDR v1, `origin/main` at `130f2dd` (2026-09-18).
**Policy read before the audit:** ADR-015, accepted on 2026-09-21 in commit `8e4996d`, establishes this repository as the Beta/Final product and FNDR-2.0 as read-only reference material.
**Scope:** static liveness and duplication review of the capture-to-Lance path, post-capture review, rebuild/repair composition, `memory/`, and the capture embedding-composer call sites. No production code, tests, or scripts were changed.

## Result

`compose_memory_embedding_document` is the canonical live composer. It is used by normal capture, low-OCR visual capture, continuity merge, import, normalization, maintenance/reindex, and retrieval test fixtures. There is no second live capture composer.

The audit found one production-dead legacy composer and two live wrappers that bypass the canonical document's fallback/provenance contract. The wrappers are the meaningful drift risk; they should be consolidated in a follow-up behavior-preserving cleanup slice, not changed by this audit.

## Live post-capture path

```text
run_capture_loop
  -> build MemoryRecord seed from OCR + structured fields
  -> derive_insight_for_record
  -> compose_memory_embedding_document
  -> primary/snippet/support text inputs -> MiniLM vectors
  -> merge_or_append_memory_record (when continuity applies)
       -> compose_memory_embedding_document before re-embedding a merged record
  -> Lance add_batch_and_get_count
  -> graph queue + memory-review queue
  -> memory_review worker
       -> currently bypasses the canonical document through compose_embedding_text
```

- The continuous capture owner is `run_capture_loop` at [src-tauri/src/capture/mod.rs:L1927](../../src-tauri/src/capture/mod.rs#L1927). Its normal OCR path derives insight and composes the document at [capture/mod.rs:L3158](../../src-tauri/src/capture/mod.rs#L3158) and [capture/mod.rs:L3291](../../src-tauri/src/capture/mod.rs#L3291). The returned `primary_text` becomes the persisted `embedding_text` at [capture/mod.rs:L3725](../../src-tauri/src/capture/mod.rs#L3725).
- The low-OCR visual branch separately constructs a record but invokes the same canonical composer at [capture/mod.rs:L272](../../src-tauri/src/capture/mod.rs#L272) and [capture/mod.rs:L483](../../src-tauri/src/capture/mod.rs#L483).
- Continuity merge recomposes from the merged seed before re-embedding at [capture/mod.rs:L4312](../../src-tauri/src/capture/mod.rs#L4312) and [capture/mod.rs:L4454](../../src-tauri/src/capture/mod.rs#L4454). This is a legitimate second *call path*, not a second composer.
- The capture batch is durable only after `add_batch_and_get_count`; successful flushes enqueue graph and review follow-up work at [capture/mod.rs:L1998](../../src-tauri/src/capture/mod.rs#L1998) and [capture/mod.rs:L2030](../../src-tauri/src/capture/mod.rs#L2030).
- The application starts the pressure-gated review worker at [src-tauri/src/main.rs:L267](../../src-tauri/src/main.rs#L267). The review pipeline currently recomputes `embedding_text` through a wrapper at [src-tauri/src/memory_review/pipeline.rs:L303](../../src-tauri/src/memory_review/pipeline.rs#L303).

## Composer inventory

| Function | Liveness | Finding | Disposition |
| --- | --- | --- | --- |
| `compose_memory_embedding_document` | Live canonical | Produces primary, snippet, support, chunk-source, and visual-semantic text; `primary_text` uses insight-first text with a safe fallback. | Keep as the sole `MemoryRecord` composition API. [memory_embedding_document.rs:L180](../../src-tauri/src/memory_embedding_document.rs#L180) |
| `compose_insight_embedding_text` | Live subordinate | Pure insight-text builder used by the canonical document. It is not a competing document composer. | Keep, but consume it through the canonical document outside the document module. [memory_insight/embedding_text.rs:L20](../../src-tauri/src/memory_insight/embedding_text.rs#L20) |
| `compose_primary_embedding_text` | **Production-dead** | Private legacy composer; exactly two callers, both unit tests. It ignores `clean_text` and `lexical_shadow` explicitly, and its output shape predates the document/manifest contract. | Delete with its two legacy-format tests, replacing assertions with `compose_memory_embedding_document(...).primary_text`. [capture/mod.rs:L1753](../../src-tauri/src/capture/mod.rs#L1753), [capture/mod.rs:L5847](../../src-tauri/src/capture/mod.rs#L5847), [capture/mod.rs:L6040](../../src-tauri/src/capture/mod.rs#L6040) |
| `storage::compose_embedding_text` | Live but redundant | One-line forwarding wrapper around `compose_insight_embedding_text`; it is used by the review worker and therefore bypasses document fallback, snippet/support sources, and manifest alignment. If structured fields are empty, it can produce an empty primary input where the canonical document selects a fallback. | Replace the review use with `compose_memory_embedding_document(&record, ...).primary_text`, then remove this wrapper. [normalize_embed_migrate.rs:L782](../../src-tauri/src/storage/lance_store/normalize_embed_migrate.rs#L782), [memory_review/pipeline.rs:L305](../../src-tauri/src/memory_review/pipeline.rs#L305) |
| `compose_rebuild_embedding_text` | Live but redundant | One-line quality-rebuild wrapper around the same insight-only function, called by both rebuild modes. It has the same fallback/provenance drift risk as review. | Replace both calls with canonical `primary_text`, then delete the wrapper. [quality.rs:L329](../../src-tauri/src/ipc/commands/quality.rs#L329), [quality.rs:L846](../../src-tauri/src/ipc/commands/quality.rs#L846), [quality.rs:L1072](../../src-tauri/src/ipc/commands/quality.rs#L1072) |
| `memory::embed_doc::build_embedding_document` | Debug-only, not a capture writer | Builds a separate `ValidatedMemory` inspection payload for the registered debug command. It does not write a vector, but its name can imply it reproduces live composition. | Keep only if the inspector must show legacy validation output; otherwise make the inspector display the canonical document. Rename on the next debug-API change to avoid a second source-of-truth implication. [memory/embed_doc.rs:L23](../../src-tauri/src/memory/embed_doc.rs#L23), [ipc/commands/debug.rs:L44](../../src-tauri/src/ipc/commands/debug.rs#L44), [debug.rs:L77](../../src-tauri/src/ipc/commands/debug.rs#L77) |
| `compose_graph_node_embedding_text` | Live, distinct contract | Constructs graph-node vectors from a graph node plus optional source memory; it is not a `MemoryRecord` primary-text substitute. | Keep separate. [memory_embedding_document.rs:L426](../../src-tauri/src/memory_embedding_document.rs#L426), [ipc/commands/graph.rs:L173](../../src-tauri/src/ipc/commands/graph.rs#L173) |

## Redundant work inside the canonical path

`normalize_record_for_index` composes a `canonical_document` and, without changing the source record, immediately composes a second `final_document`. The second composition is only needed as a mutable copy when preserving an already-vectorized legacy `embedding_text`. Reusing `canonical_document` and cloning only for that mismatch branch is behavior-preserving and removes one pure composition per normalization.

Evidence: [src-tauri/src/storage/lance_store/normalize_embed_migrate.rs:L413](../../src-tauri/src/storage/lance_store/normalize_embed_migrate.rs#L413) through [normalize_embed_migrate.rs:L428](../../src-tauri/src/storage/lance_store/normalize_embed_migrate.rs#L428).

## Boundaries that are intentionally separate

- Import, explicit v5 reindex, maintenance compaction, and migration normalization use the canonical composer because they write or repair real retrieval vectors. For example, v5 batching composes every source record once before role-specific embedding at [src-tauri/src/ipc/commands/maintenance.rs:L462](../../src-tauri/src/ipc/commands/maintenance.rs#L462) and [maintenance.rs:L475](../../src-tauri/src/ipc/commands/maintenance.rs#L475).
- A graph-node composer must remain separate because it feeds a different graph vector contract, not the memory primary vector.
- The document's `chunk_source_text` may contain cleaned OCR while `primary_text` remains insight-first; that split is deliberate under ADR-007 and ADR-010, not duplication. [docs/decisions/007-insight-first-memory-and-embedding-text.md:L7](../decisions/007-insight-first-memory-and-embedding-text.md#L7), [docs/decisions/010-embedding-document-manifest.md:L18](../decisions/010-embedding-document-manifest.md#L18).

## Recommended follow-up: one bounded cleanup slice

1. Add a parity test covering an insight-empty record: capture/review/quality rebuild must select the same canonical `primary_text` and preserve its manifest source hash.
2. Route review and quality rebuild through `compose_memory_embedding_document`.
3. Remove `compose_embedding_text`, `compose_rebuild_embedding_text`, and production-dead `compose_primary_embedding_text`; migrate the two legacy tests to the canonical composer.
4. Reuse `canonical_document` in normalization when no mismatch mutation is required.
5. Do not change the graph composer or the debug inspection payload in this slice unless its contract is explicitly revised.

## Audit limits and verification

- Static in-tree call-site inventory: `compose_primary_embedding_text` has three references (definition plus two tests); `compose_embedding_text` two; `compose_rebuild_embedding_text` three; `build_embedding_document` six; canonical `compose_memory_embedding_document` seventeen.
- `scripts/audit/` does not exist on the `origin/main` baseline. No script was added because this ticket is documentation-only; a future liveness script must state its static-analysis limits rather than claim compiler-grade reachability.
- Validation for this documentation change: call-site searches above and `git diff --check`. No behavior changed, so no build or test was run.
