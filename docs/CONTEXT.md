# FNDR — shared context for agents

Use this file with **`AGENTS.md`** and the portable skills under `.agent-skills/portable-engineering/`.

## What FNDR is

A macOS Tauri application that builds a **searchable local memory** from screen context, meetings, tasks, downloads, and related signals. See `README.md` for product areas and user-facing capabilities.

Full documentation index (architecture, decisions, product notes, agent defaults): **`docs/README.md`**.

## Engineering vocabulary

- **Memory record** (`MemoryRecord`): persisted unit of captured context stored and indexed for search. This is the **parent** in the parent-child RAG model — the authoritative record for card synthesis, holding full OCR, insight fields, and metadata.
- **Source statement**: an exact quote resolved by Rust from a model-selected line and its adjacent lines in the bounded extraction input. Its snapshot hash and line identify where it appeared; they do not establish speaker ownership, pending status, truth, or permission to act. New text extractions retain these observations in `raw_evidence.source_evidence` instead of generating canonical intent or tasks. Generated descriptive context remains unverified.
- **Memory chunk** (`MemoryChunkRecord`, to be added by Subagent 7): an overlapping text window derived from a parent `MemoryRecord`, carrying its own embedding and a `parent_id` foreign key. Used by the chunk-first retrieval path for higher-precision vector search. See ADR 008.
- **Embedding document** (`MemoryEmbeddingDocument`): the canonical in-memory retrieval document used to derive primary/search text, snippet text, support text, chunk text, visual semantic text, and graph-node text before vectors are written. Its provenance is stored additively under `raw_evidence.embedding_manifest`. See ADR 010.
- **Memory card**: UI-facing presentation of a search hit / browse item.
- **Memory Vault**: full-screen browse surface for all memories, the global insight graph (Louvain-clustered layout), and per-project graph scopes (`src/domains/memory-vault/MemoryCardsPanel` + sidebar entry).
- **Capture pipeline**: screen → OCR / text extraction → chunking → embedding → storage.
- **Hybrid search**: vector + keyword retrieval with reranking as implemented in Rust.
- **Parent-child RAG**: retrieval pattern where child chunks are searched first for precision, then matched chunks' parent records are fetched for full-context card synthesis. Governed by ADR 008.
- **Sidecar**: Python helpers under `src-tauri/sidecars/` for transcription, agent, graph, TTS, etc.
- **Status events**: backend → renderer push channels (`capture://status`, `privacy://alerts`, `meeting://status`, `proactive_suggestion`, model-download events). Always-on UI state subscribes via `useTauriEvent` after one initial fetch instead of polling. See ADR 011.
- **Activity trace**: a bounded, in-memory sequence of observed workflow steps shown beside the operation it explains. Each step names its actor, status, time, and evidence origin; traces never contain captured content, raw logs, or model chain-of-thought, never predict future stages, and are not persisted. See ADR 021.
- **Memory Journey**: a debug-build-only, explicitly armed evidence bundle that correlates one capture attempt through the real capture, extraction, embedding, storage, retrieval, and presentation paths. Unlike a normal activity trace it may persist raw local artifacts under the bounded developer contract in `docs/product/memory-journey.md`; it never enters Memory, model context, analytics, or automatic upload.
- **Live journey**: a Memory Journey whose one-shot arm is consumed by a real capture attempt. Its manifest contains only stages actually observed during that attempt.
- **Reconstructed journey**: a Memory Journey assembled from one explicitly selected persisted memory. It records persisted evidence and labels every non-reconstructable stage `unavailable` rather than implying it ran.
- **Approved gold case**: a public, synthetic, or sanitized journey whose expected facts, forbidden claims, relevant IDs, and quality labels have been reviewed and approved by a person. A model-generated draft is not gold.
- **Human usefulness**: the separately scored ability of a memory to be understandable, faithful, recoverable, and findable for a person; it is not inferred from retrieval metrics alone.
- **Agent grounding**: the separately scored ability of an agent to recover required facts with complete evidence, precise citations, stable provenance, and correct refusal of unsupported claims.
- **Screen Guide**: FNDR's opt-in, read-only assistant for asking about the current main display or explicitly locating a named local file. A guide turn uses on-device transcription and local speech, and is not written to memory. Screen questions may use ephemeral pixels and an OCR-grounded point cue; file questions use scoped metadata only. See ADR 014.
- **Ephemeral display capture**: a screen image held only in process memory for one explicit Screen Guide request. It never becomes a screenshot file or `MemoryRecord` attachment.
- **Point cue**: an optional, non-interactive overlay marker at the normalized center of a text line Apple Vision actually observed. It explains where to look; it does not click or control another app.
- **Scoped file lookup**: an explicit filename-only macOS metadata query limited to Documents, Desktop, and Downloads. It never reads or opens a match, scans the full home directory, or persists its query/results.
- **Notch status item**: FNDR's OS-managed menu-bar presence beside the notch. It exposes only fixed Screen Guide phases and never user questions, filenames, paths, screen text, answers, or detailed errors.
- **Companion API**: the existing local-network API for FNDR's iPhone and Watch clients. Do not use “Companion” as the product or code name for Screen Guide.
- **Document path**: POSIX path decoded from a native app’s Accessibility `AXDocument` `file://` URL; used as the reopen file target ahead of LLM `files_touched`. Browsers keep http(s) URLs only.

## Default quality bar

Prefer small diffs, tests at stable boundaries, and evidence-backed debugging — see `AGENTS.md` and the `diagnose` / `tdd` skills.
