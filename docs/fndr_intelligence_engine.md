# fndr Intelligence Engine (Implementation Spec)

This document maps the requested intelligence engine design to the current local-first implementation.

## 1. Data Ingestion and Apple Permissions

### Capture
- Active module: `src-tauri/src/capture/mod.rs`
- Platform capture adapter: `src-tauri/src/capture/macos.rs`
- Current behavior:
  - Captures screen frames locally.
  - Extracts active app name, bundle ID, and front window title.
  - Captures browser URL (Safari, Chrome, Arc, Brave, Edge).
  - Persists frame image files under app data (`frames/YYYYMMDD/*.png`).

### Extraction Pipeline
- OCR: Apple Vision integration in `src-tauri/src/ocr/vision.rs`.
- Text embeddings: local embedder (`src-tauri/src/embed/onnx.rs`).
- Image embeddings: local CLIP-style embedding interface (`src-tauri/src/embed/clip.rs`).

### Event Context
Every memory event now stores:
- `app_name`
- `bundle_id`
- `window_title`
- `session_id`
- `url`
- `screenshot_path`
- `embedding` (text)
- `image_embedding` (visual)

Schema: `src-tauri/src/store/schema.rs`

## 2. Graphiti-Style Temporal RAG Architecture

### Graph Schema
- Module: `src-tauri/src/graph/mod.rs`
- Persistence: `memory_graph.json` in app data directory.

Node types:
- `MemoryChunk`
- `Entity` (session entity today; extensible to person/project entities)
- `Task`
- `Url`

Edge types:
- `PART_OF_SESSION`
- `REFERENCE_FOR_TASK`
- `OCCURRED_AT`

### Hybrid Search
- Semantic + keyword hybrid retrieval via `src-tauri/src/search/hybrid.rs`.
- Structural traversal via graph relationships (e.g., task -> memory -> URL).

## 3. Agentic Task Management

### Task Nodes and URL Ground Truth
- Task schema: `src-tauri/src/tasks/mod.rs`
- Tasks now include:
  - `linked_urls`
  - `linked_memory_ids`
- Command `get_todos` links extracted tasks to recent URLs and source memories, then writes graph edges.
- Command `execute_todo` keeps linked context attached to returned tasks.

### Agent Execution Context
- Command: `start_agent_task` in `src-tauri/src/api/commands.rs`
- Agent prompt now receives:
  - task title
  - graph-linked URLs
  - graph notes

## 4. Memory Reconstruction Interface

### Backend Contract
- Command: `reconstruct_memory`
- Returns:
  - `answer` (synthesized language response)
  - `cards` (memory artifacts: screenshot path, snippet, URL, app metadata)
  - `structural_context` (graph traversal notes)

### Frontend Artifact View
- Component: `src/components/MemoryReconstructionPanel.tsx`
- Styles: `src/components/MemoryReconstructionPanel.css`
- Integrated into app layout in `src/App.tsx`.
- Side-panel behavior:
  - runs reconstruction on active query
  - renders synthesized response + evidence cards
  - shows screenshot previews via Tauri `convertFileSrc`

## 5. Notes and Hardening Gaps

- Capture backend currently uses local macOS capture path; if strict `ScreenCaptureKit` API usage is required, the capture adapter can be swapped behind `capture/macos.rs` without changing graph/search/task APIs.
- Image embeddings currently use a local CLIP-compatible interface with deterministic vectors for offline reliability; replace internals with full CLIP runtime for production accuracy.
- `Entity` extraction (person/project nodes) is schema-ready but currently session-first; NER/entity linking can be layered during ingestion.
