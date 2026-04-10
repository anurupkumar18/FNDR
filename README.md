# FNDR - Privacy-First Local Memory Assistant

FNDR is a local-only screen history search and AI assistant for macOS. It periodically captures your screen, runs OCR and VLM analysis, and uses a local Large Language Model (LLM) to summarize your activity and answer questions—all without your data ever leaving your machine.

## 🚀 App Functionality

FNDR sits in your background, recording snapshots of your workspace. It indexes these snapshots using semantic and keyword search, allowing you to instantly find "that thing I saw 3 hours ago." Beyond search, it acts as a proactive assistant that can summarize your day or extract todos from your screen captures.

## ✨ Features

### Current Features (What Works)
- **Local Screen Capture**: Periodically snapshots active windows with deduplication.
- **Apple Vision OCR**: High-speed, local text recognition.
- **Multimodal Understanding**: Uses **SmolVLM** (500M/256M) for intelligent screen understanding beyond raw text.
- **"Ask FNDR" (RAG)**: Retrieval-Augmented Generation using a local LLM to answer questions about your history.
- **AI Summaries**: Turns messy OCR fragments into clean, human-readable event descriptions.
- **Todo Extraction**: Automatically identifies potential tasks and reminders from your screen history.
- **URL Capture**: Automatically saves website URLs from browser windows for quick navigation back to sources.
- **Agent Task Execution**: Execute todos using Claude Agent SDK (requires API key).
- **MCP Server**: Built-in local Model Context Protocol server for connecting FNDR to external MCP clients.
- **Offline Meeting Recorder**: Local meeting session recording, segmented audio capture, transcript timeline, export, and agent handoff.
- **Privacy Controls**: Built-in blocklist to exclude sensitive applications and incognito mode.
- **High Performance**: Native Rust core with Metal acceleration for Apple Silicon.

### Experimental (optional / demo)
Meetings, knowledge graph, agent panel, MCP, and meeting recorder are powerful but can be **hidden** for grading builds — see **Evaluation UI** below.

### Planned Features (What's Next)
- **Advanced Idle Detection**: Smarter capture logic based on user activity.
- **Multi-Monitor Support**: Comprehensive capture across all connected displays.

## 🚦 Current Status

| Feature | Status | Notes |
| :--- | :--- | :--- |
| **Search** | ✅ Working | Hybrid search (Semantic + Keyword) is functional. |
| **Vector store** | ✅ Working | **LanceDB** (`src-tauri/src/store/lance_store.rs`). |
| **Inference** | ✅ Working | Llama 3.2 1B runs locally with Metal acceleration. |
| **VLM** | ✅ Optional | SmolVLM; can be disabled for a stable OCR+LLM path. |
| **OCR** | ✅ Working | Fast and accurate via Apple Vision Framework. |
| **URL Capture** | ✅ Working | Captures URLs from Safari, Chrome, Arc, Brave, Edge. |

## 🛠 How to Run

### Prerequisites
- **macOS 13.0+**
- **Apple Silicon (M1/M2/M3)** recommended for best AI performance.
- **Rust Toolchain**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Node.js & npm**: [LTS version recommended](https://nodejs.org/).
- **CMake**: Required for building AI inference libraries (`brew install cmake`).
- **ffmpeg**: Required for local meeting audio recording (`brew install ffmpeg`).

### Execution Steps
1. **Clone the repository**:
   ```bash
   git clone <your-repo-url>
   cd FNDR
   ```

2. **Download AI Models**:
   Run the helper script to fetch the Llama and SmolVLM models:
   ```bash
   chmod +x download_model.sh
   ./download_model.sh
   ```

3. **Install UI Dependencies**:
   ```bash
   npm install
   ```

4. **Launch Developer Mode**:
   ```bash
   npm run tauri dev
   ```


### Evaluation UI (TA / prototype review)

Build the frontend with **`VITE_EVAL_UI=true`** to hide Meetings, Graph, Agent, Todo modal, and reconstruction side panel — leaving search, timeline, readiness, and settings focused on the core pipeline.

```bash
VITE_EVAL_UI=true npm run tauri dev
```

### Demo grading mode

- **Settings → Demo grading**: *Seed demo dataset*, *Reset demo data*, *Inject test memory*, *Use demo data only* (pauses live capture indexing).
- Or launch the app with **`--demo-data-only`** (see `src-tauri/src/main.rs`) for a headless-friendly default.

## Documentation

| Doc | Purpose |
|-----|---------|
| [DEMO.md](DEMO.md) | Five-minute walkthrough script |
| [TESTING.md](TESTING.md) | Commands, CI, manual QA |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Branches, MRs, review |
| [docs/architecture.md](docs/architecture.md) | Pipeline diagram |

## 📁 Repository Structure

```text
FNDR/
├── src-tauri/          # Backend (Rust)
│   ├── src/
│   │   ├── api/        # Tauri command handlers
│   │   ├── capture/    # Screen recording & sampling
│   │   ├── inference/  # LLM (Llama) & VLM (SmolVLM) engines
│   │   ├── ocr/        # Apple Vision OCR integration
│   │   ├── store/      # Local storage & indexing
│   │   ├── demo/       # Seeded demo corpus for grading
│   │   └── tasks/      # Todo extraction logic
│   └── tauri.conf.json # App configuration
├── src/                # Frontend (React + TypeScript)
│   ├── components/     # UI components (MemoryCard, ControlPanel, etc.)
│   ├── hooks/          # Custom React hooks (useSearch, etc.)
│   └── main.tsx        # Application entry point
├── download_model.sh   # Utility script for model management
└── README.md           # You are here
```

## 🛡 Privacy Note
All processing happens **100% locally** on your machine. No text, images, or queries are ever sent to any cloud provider or external server.

## 🔌 MCP Connection
- FNDR starts a local MCP server at `http://127.0.0.1:8799/mcp` by default.
- In FNDR Settings, use the **MCP Server** section to start/stop the server and copy the link.
- Exposed tools:
  - `search_memories`
  - `ask_fndr`
  - `get_fndr_stats`
  - `start_meeting`
  - `stop_meeting`
  - `get_meeting_transcript`
  - `search_meeting_transcripts`

## 🎙️ Meeting Recorder Notes
- Open **Meetings** in the header to view live auto-captured meeting notes.
- FNDR auto-detects meeting sessions (Zoom / Meet / Teams / Webex signals) and starts/stops recording automatically.
- Audio chunks are stored under FNDR app data in `meetings/<meeting_id>/audio/`.
- Transcripts are persisted in local indexes and exported to `meetings/<meeting_id>/transcript.md` when a session stops.
- A Finder-visible copy is written to `~/Documents/FNDR Meetings/*.md`.
- On session end, FNDR also ingests the markdown transcript into unified FNDR memory (`app_name = FNDR Meetings`).
- Transcription backend priority:
  1. `FNDR_PARAKEET_COMMAND` (custom Parakeet runner command)
  2. bundled sidecar `src-tauri/sidecar/parakeet_runner.py` (requires `python3` + `faster-whisper`)
  3. `python3 -m whisper` fallback
- Install sidecar transcription dependencies with:
  - `pip install -r src-tauri/sidecar/requirements.txt`

## 📄 License
MIT
