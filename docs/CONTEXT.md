# FNDR — shared context for agents

Use this file with **`AGENTS.md`** and the portable skills under `.agent-skills/portable-engineering/`.

## What FNDR is

A macOS Tauri application that builds a **searchable local memory** from screen context, meetings, tasks, downloads, and related signals. See `README.md` for product areas and user-facing capabilities.

## Where truth lives

| Topic | Location |
| --- | --- |
| Product + setup + dev commands | `README.md` |
| Shared vocabulary (this file) | `docs/CONTEXT.md` |
| Pipeline and components | `docs/architecture/ARCHITECTURE.md` |
| Architecture decisions | `docs/decisions/` |
| UX / visual direction | `docs/product/DESIGN_DIRECTION.md` |
| Intelligence engine notes | `docs/product/intelligence-engine.md` |
| Insight graph (Lance schema + policies) | `docs/architecture/graph-schema.md` |
| Agent defaults + skill map | `AGENTS.md` |

## Engineering vocabulary

- **Memory record**: persisted unit of captured context stored and indexed for search.
- **Memory card**: UI-facing presentation of a search hit / browse item.
- **Memory Vault**: full-screen browse surface for all memories, the global insight graph (Louvain-clustered layout), and per-project graph scopes (`src/domains/memory-vault/MemoryCardsPanel` + sidebar entry).
- **Capture pipeline**: screen → OCR / text extraction → chunking → embedding → storage.
- **Hybrid search**: vector + keyword retrieval with reranking as implemented in Rust.
- **Sidecar**: Python helpers under `src-tauri/sidecars/` for transcription, agent, graph, TTS, etc.

## Default quality bar

Prefer small diffs, tests at stable boundaries, and evidence-backed debugging — see `AGENTS.md` and the `diagnose` / `tdd` skills.
