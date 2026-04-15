# FNDR

FNDR is a privacy-first local memory assistant for macOS. It captures on-screen activity, indexes it locally, and helps you search and reconstruct recent context.

## What FNDR Does Today

- Background capture loop with:
  - screenshot capture,
  - OCR extraction,
  - adaptive sampling,
  - perceptual deduplication.
- Local memory storage in LanceDB (text/image embeddings + metadata).
- Hybrid search pipeline (vector + keyword + sentence-aware reranking).
- Memory card retrieval/synthesis for timeline and card views.
- Local model-backed memory summarization.
- Task extraction (Todo / Reminder / Follow-up) from recent memories.
- Local graph store + graph visualization panel.
- Meeting recorder with:
  - automatic meeting detection heuristics,
  - ffmpeg-based segmented audio capture,
  - local Whisper GGUF transcription,
  - transcript search + markdown/json export.
- Voice input transcription and local TTS.
- Privacy controls (pause/resume, blocklist, retention, delete memory, delete all data).


## Local Models (Current Catalog)

From `src-tauri/src/models.rs`:

- `qwen3-vl-4b` (recommended)
- `llama-3.2-1b`
- `smolvlm-500m`

Models are selected/downloaded in onboarding/settings.

## Run FNDR (Dev)

### Prerequisites

- macOS 13+
- Xcode Command Line Tools
- Node.js + npm
- Rust toolchain
- Python 3
- ffmpeg (`brew install ffmpeg`) for meetings

### Setup

```bash
git clone <repo-url>
cd fndr
npm install
npm run tauri dev
```

Then complete onboarding in-app (permissions + model download).


## Repository Layout

```text
fndr/
├── src/                 # React + TypeScript frontend
├── src-tauri/           # Rust backend (capture, search, store, meetings, mcp, etc.)
│   ├── src/
│   └── sidecar/         # Python sidecars (whisper, embedder, agent, tts)
├── docs/
└── README.md
```

## Notes on Privacy

Core capture/search/indexing runs locally on-device, unlike anything you've seen before.