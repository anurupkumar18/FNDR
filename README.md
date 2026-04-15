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
- Local model-backed memory summarization when a model is available.
- Task extraction (Todo / Reminder / Follow-up) from recent memories.
- Local graph store + graph visualization panel.
- Meeting recorder with:
  - automatic meeting detection heuristics,
  - ffmpeg-based segmented audio capture,
  - local Whisper GGUF transcription,
  - transcript search + markdown/json export.
- Voice input transcription and local TTS.
- Built-in MCP server (HTTPS + token + SSE transport).
- Privacy controls (pause/resume, blocklist, retention, delete memory, delete all data).

## Optional / External Pieces

- Agent panel is optional and requires `ANTHROPIC_API_KEY`.
- `ffmpeg` is only required for meeting recording.
- `python3` is required for sidecar-powered features (embeddings/speech/transcription).
- `VITE_EVAL_UI=true` hides advanced panels for evaluation/demo-style builds.

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
- Optional: `ffmpeg` (`brew install ffmpeg`)

### Setup

```bash
git clone <repo-url>
cd fndr
npm install
npm run tauri dev
```

Then complete onboarding in-app (permissions + model download).

## Environment Variables

Use `.env` (see `.env.example`).

- `ANTHROPIC_API_KEY` - enable optional agent tasks.
- `FNDR_WHISPER_GGUF_COMMAND` - override voice transcription command.
- `FNDR_MEETING_TRANSCRIBE_COMMAND` - override meeting transcription command.
- `FNDR_PARAKEET_COMMAND` - legacy alias for meeting transcription override.
- `FNDR_ORPHEUS_COMMAND` - override TTS command.
- `VITE_EVAL_UI=true` - hide advanced UI panels.

## MCP Server

FNDR can start/stop an MCP server from settings.

Current behavior:

- HTTPS endpoint with self-signed TLS.
- Bearer-token auth.
- Dynamic host/port (not a fixed hardcoded localhost URL).
- Discovery file written to `~/.fndr/mcp.json`.

Exposed MCP tools:

- `search_memories`
- `ask_fndr`
- `get_fndr_stats`
- `start_meeting`
- `stop_meeting`
- `get_meeting_transcript`
- `search_meeting_transcripts`

## Data Locations (High Level)

- Core app data (memories/frames/graph/meetings/tasks): Tauri app data directory.
- MCP discovery: `~/.fndr/mcp.json`
- Meeting transcript exports: `~/Documents/FNDR Meetings/`
- Speech venv (on-demand): `~/Documents/FNDR Speech/venv`

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

Core capture/search/indexing runs locally on-device. If you enable the optional agent panel, that flow uses Anthropic APIs via your API key.

## Documentation

- `docs/DESIGN_DIRECTION.md`
- `docs/fndr_intelligence_engine.md`

## License

MIT
