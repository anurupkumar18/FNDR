# 007: Insight-first memory and embedding text purity

## Context

FNDR stores screen-derived OCR in `MemoryRecord.text` / `clean_text` for provenance and debugging, while semantic search relies on `embedding_text` and related vectors. Product direction treats **human-facing insight layers** (what happened, why it mattered, what changed, thread context) as the durable story of a capture, not the raw transcript.

## Decision

1. **Embedding input** — `embedding_text` is composed **only** from structured insight and retrieval fields (`user_intent`, `project`, `topic`, `workflow`, `memory_context`, entities, aliases, decisions, errors, blockers, todos, results, files, URLs, commands). **Raw OCR / `clean_text` must not appear** in `embedding_text`. Salient OCR may still inform *derivation* of insight fields during capture/normalization; it does not get concatenated into the embedder payload.

2. **First-class insight columns** — Persisted LanceDB columns `insight_what_happened`, `insight_why_mattered`, `insight_what_changed`, `insight_context_thread`, `insight_spans_json` (debug: ranked salient spans + dropped noise), and `insight_card_confidence` (0..1 for UI gating) live beside existing summary fields. They are populated at **write time** in `normalize_record_for_index` when empty, using deterministic rules over existing structured fields and salience ranking.

3. **`memory_context` purity** — `memory_context` remains **human-readable prose** only. Structured continuation, reopen targets, and IDs stay in dedicated fields (`related_memory_ids`, `parent_id`, card-level `reopen_target` on synthesized search cards, etc.). Do not encode machine markers into `memory_context` going forward; legacy rows may still contain markers until backfill.

4. **Source statements** — New text extraction selects references into host-numbered source text. Rust resolves complete quotes with adjacent context and stores the source snapshot hash, selected line and quote in `raw_evidence.source_evidence`, with bounded original-snapshot history on merge. Quotes stay separate from `memory_context`, `internal_context` and embedding text. Human detail views and agent evidence packs may expose them as observations, with ownership and pending status explicitly unverified. Capture, merge, review and storage must not regenerate canonical intent or actions for these records. Legacy records retain their existing behavior until explicitly migrated; merely reading them does not certify their claims.

## Consequences

- **Migrations** — New nullable/default columns are added via `ensure_memory_schema_columns` for existing databases.
- **Recall** — Keyword / lexical recall continues to use `lexical_shadow` and hybrid search paths that may reference OCR-adjacent signals outside `embedding_text`.
- **Tests** — Golden records assert `embedding_text` does not contain leading substrings of `clean_text` (see quality diagnostics and lance_store tests).

## Status

Accepted — implemented in code alongside `memory_insight` module and schema updates.
