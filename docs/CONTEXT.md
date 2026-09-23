# FNDR — shared context for agents

Use this file with **`AGENTS.md`** and the portable skills under `.agent-skills/portable-engineering/`.

## What FNDR is

A macOS Tauri application that builds a **searchable local memory** from screen context, meetings, tasks, downloads, and related signals. See `README.md` for product areas and user-facing capabilities.

Full documentation index (architecture, decisions, product notes, agent defaults): **`docs/README.md`**.

## Engineering vocabulary

- **Memory record** (`MemoryRecord`): persisted unit of captured context stored and indexed for search. This is the **parent** in the parent-child RAG model — the authoritative record for card synthesis, holding full OCR, insight fields, and metadata.
- **Memory chunk** (`MemoryChunkRecord`, to be added by Subagent 7): an overlapping text window derived from a parent `MemoryRecord`, carrying its own embedding and a `parent_id` foreign key. Used by the chunk-first retrieval path for higher-precision vector search. See ADR 008.
- **Embedding document** (`MemoryEmbeddingDocument`): the canonical in-memory retrieval document used to derive primary/search text, snippet text, support text, chunk text, visual semantic text, and graph-node text before vectors are written. Its provenance is stored additively under `raw_evidence.embedding_manifest`. See ADR 010.
- **Memory card**: UI-facing presentation of a search hit / browse item.
- **Memory Vault**: full-screen browse surface for all memories, the global insight graph (Louvain-clustered layout), and per-project graph scopes (`src/domains/memory-vault/MemoryCardsPanel` + sidebar entry).
- **Capture pipeline**: screen → OCR / text extraction → chunking → embedding → storage.
- **Hybrid search**: vector + keyword retrieval with reranking as implemented in Rust.
- **Parent-child RAG**: retrieval pattern where child chunks are searched first for precision, then matched chunks' parent records are fetched for full-context card synthesis. Governed by ADR 008.
- **Sidecar**: Python helpers under `src-tauri/sidecars/` for transcription, agent, graph, TTS, etc.
- **Status events**: backend → renderer push channels (`capture://status`, `privacy://alerts`, `meeting://status`, `proactive_suggestion`, model-download events). Always-on UI state subscribes via `useTauriEvent` after one initial fetch instead of polling. See ADR 011.
- **Screen Guide**: FNDR's opt-in, read-only assistant for asking about the current main display or explicitly locating a named local file. A guide turn uses on-device transcription and local speech, and is not written to memory. Screen questions may use ephemeral pixels and an OCR-grounded point cue; file questions use scoped metadata only. See ADR 014.
- **Ephemeral display capture**: a screen image held only in process memory for one explicit Screen Guide request. It never becomes a screenshot file or `MemoryRecord` attachment.
- **Point cue**: an optional, non-interactive overlay marker at the normalized center of a text line Apple Vision actually observed. It explains where to look; it does not click or control another app.
- **Scoped file lookup**: an explicit filename-only macOS metadata query limited to Documents, Desktop, and Downloads. It never reads or opens a match, scans the full home directory, or persists its query/results.
- **Notch status item**: FNDR's OS-managed menu-bar presence beside the notch. It exposes only fixed Screen Guide phases and never user questions, filenames, paths, screen text, answers, or detailed errors.
- **Companion API**: the existing local-network API for FNDR's iPhone and Watch clients. Do not use “Companion” as the product or code name for Screen Guide.

## Default quality bar

Prefer small diffs, tests at stable boundaries, and evidence-backed debugging — see `AGENTS.md` and the `diagnose` / `tdd` skills.
