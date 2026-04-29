<a id="readme-top"></a>

# FNDR

[![Version][version-shield]][github-url]
[![Tauri][tauri-shield]][tauri-url]
[![React][react-shield]][react-url]
[![Rust][rust-shield]][rust-url]
[![macOS][macos-shield]][tauri-config]
[![License: MIT][license-shield]][cargo-manifest]

FNDR is a macOS desktop app for building a searchable local memory from screen context, meetings, tasks, downloads, and app activity. The app combines a React/Tauri UI with a Rust capture and search backend, LanceDB storage, local ONNX embeddings, and selectable local GGUF models.

## Table Of Contents

| Section | Description |
| --- | --- |
| [About](#about) | Current product scope and capabilities |
| [Architecture](#architecture) | Repository layout and major runtime components |
| [Getting Started](#getting-started) | Prerequisites, setup, and local launch |
| [Configuration](#configuration) | Environment variables and runtime settings |
| [Local Models](#local-models) | Model catalog used by onboarding and settings |
| [Privacy Controls](#privacy-controls) | Verified capture and data controls present in source |
| [Development](#development) | Test and verification commands |
| [Links](#links) | Repository remotes |

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## About

FNDR captures macOS screen context, extracts text and visual signals, stores memory records, and exposes search and reconstruction workflows in the desktop UI. The current codebase includes the following product areas:

| Area | Current implementation |
| --- | --- |
| Capture | macOS screen capture, OCR, adaptive sampling, perceptual deduplication, semantic deduplication, and batched memory writes |
| Search | Hybrid vector and keyword search, sentence-aware reranking, memory cards, timeline browsing, and raw result inspection |
| Summaries | Local model-backed memory summaries, daily summaries, daily briefings, and search-result synthesis |
| Tasks | Todo, reminder, and follow-up parsing with persisted task state |
| Meetings | Meeting detection heuristics, ffmpeg-based segmented audio capture, Whisper sidecar transcription, transcript search, and markdown/json export |
| Speech | Voice transcription and local text-to-speech command paths |
| Graph | Local graph store and graph visualization panel |
| Downloads | Downloads folder watcher that injects local file-arrival memory records |
| Autofill | Global shortcut-driven autofill retrieval and injection settings |

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Architecture

```text
fndr/
├── src/                 # React + TypeScript frontend
├── src-tauri/           # Rust backend for capture, search, storage, meetings, MCP, and Tauri commands
│   ├── src/
│   └── sidecar/         # Python sidecars for agent, embedding, transcription, VLM, and TTS workflows
├── docs/                # Design and intelligence-engine documentation
├── scripts/             # Local maintenance scripts
├── download_embedding_model.sh
├── Makefile
├── package.json
└── README.md
```

| Component | Primary paths |
| --- | --- |
| Frontend shell | `src/App.tsx`, `src/main.tsx`, `src/components/` |
| Tauri commands | `src-tauri/src/api/commands.rs` |
| Capture pipeline | `src-tauri/src/capture/` |
| Search and memory cards | `src-tauri/src/search/` |
| LanceDB store | `src-tauri/src/store/` |
| Model catalog | `src-tauri/src/models.rs` |
| Runtime config | `src-tauri/src/config.rs` |
| Privacy controls | `src-tauri/src/privacy/` |
| Meeting recorder | `src-tauri/src/meeting/`, `src-tauri/sidecar/whisper_gguf_runner.py` |

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Getting Started

| Requirement | Notes |
| --- | --- |
| macOS | macOS 13.0 or newer, matching `src-tauri/tauri.conf.json` |
| Xcode Command Line Tools | Required for native macOS and Rust builds |
| Node.js and npm | Runs the Vite/React frontend and Tauri CLI |
| Rust toolchain | Builds the Tauri backend |
| Python 3 | Runs optional sidecar workflows |
| ffmpeg | Required for meeting audio capture |

Install dependencies and launch the development app from the repository root:

```bash
make install
./download_embedding_model.sh
npm run tauri dev
```

Complete onboarding in the desktop app to grant macOS permissions and select/download the local model used for memory summaries, question answering, and screen understanding.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Configuration

Runtime app configuration is written through `src-tauri/src/config.rs`. The `.env.example` file documents optional environment variables used by experimental or sidecar features:

| Variable | Required | Purpose |
| --- | --- | --- |
| `ANTHROPIC_API_KEY` | No | Enables experimental Claude Agent SDK UI paths |
| `OPENAI_API_KEY` | No | Supports optional graph or external knowledge workflows |
| `NEO4J_URI` | No | Connects optional graph workflows to Neo4j |
| `NEO4J_USER` | No | Username for optional Neo4j graph workflows |
| `NEO4J_PASSWORD` | No | Password for optional Neo4j graph workflows |
| `VITE_EVAL_UI` | No | Hides selected feature panels when set to `true` for evaluation builds |
| `FNDR_MEETING_AUDIO_DEVICE` | No | Overrides macOS avfoundation meeting-recorder audio device selection |

Core runtime settings include capture cadence, dedupe threshold, retention days, app blocklist, screenshot retention, proactive surface behavior, and autofill behavior.

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Local Models

The onboarding and settings flows read the model catalog from `src-tauri/src/models.rs`:

| ID | Display name | Size | RAM | Role |
| --- | --- | --- | --- | --- |
| `qwen3-vl-4b` | Qwen3-VL · 4B | 2.5 GB | 6.0 GB | Recommended local model for summaries, Q&A, and screen understanding |
| `llama-3.2-1b` | Llama 3.2 · 1B | 770 MB | 2.0 GB | Minimal text model for basic summaries and search |
| `smolvlm-500m` | SmolVLM · 500M | 440 MB | 1.5 GB | Lightweight vision model for lower-RAM Macs |

The embedding bootstrap script downloads the local 1024-dimensional `bge-large-en-v1.5-quantized.onnx` embedding model and `tokenizer.json` into the default app-support models directory unless a custom target directory is supplied.

Validate the local embedding and LanceDB path with:

```bash
make diagnostic
```

If an older prototype database was created with a different vector dimension, back it up and let FNDR recreate the 1024-dimensional schema with:

```bash
make reset-lancedb
```

Generated Rust/Tauri artifacts can become large during repeated local builds. Clear only
build outputs with:

```bash
make clean-dev-cache
```

For a full local reset of generated build outputs, runtime memory data, backups, and
downloaded model blobs:

```bash
make clean-all-generated
```

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Privacy Controls

The controls below are implemented in source and exposed through Tauri commands or configuration. Optional environment variables can enable external services, so review `.env.example` before enabling experimental workflows.

| Control | Source-backed behavior |
| --- | --- |
| Pause and resume | `pause_capture` and `resume_capture` toggle capture state in `src-tauri/src/api/commands.rs` |
| App blocklist | `get_blocklist` and `set_blocklist` read/write blocked app names in runtime config |
| Default blocked apps | `1Password`, `Keychain Access`, `System Preferences`, and `System Settings` are seeded in `Config::default` |
| Sensitive-context alerts | `Blocklist::is_sensitive_context` detects selected banking and finance keywords for proactive alerts |
| Add site to blocklist | `add_to_blocklist` adds a site and attempts retroactive deletion for matching stored memories |
| Delete one memory | `delete_memory` deletes the memory record and its screenshot artifact when present |
| Delete older memories | `delete_older_than` removes memory records older than the requested day count |
| Delete all data | `delete_all_data` clears memory records, graph data, frames, screenshots, and meetings under the app data store |
| Retention | `retention_days` defaults to `7`; `screenshot_retention_days` defaults to `30` |

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Development

Run the full local test target from the repository root:

```bash
make test
```

The target runs TypeScript typechecking, Vitest, and Rust tests:

| Phase | Underlying command |
| --- | --- |
| TypeScript | `npm run typecheck` |
| Frontend tests | `npm test` |
| Rust tests | `cd src-tauri && cargo test` |

<p align="right">(<a href="#readme-top">back to top</a>)</p>

## Links

| Host | Remote |
| --- | --- |
| GitLab | `git@capstone.cs.utah.edu:fndr/fndr.git` |
| GitHub | `git@github.com:anurupkumar18/FNDR.git` |

<p align="right">(<a href="#readme-top">back to top</a>)</p>

[version-shield]: https://img.shields.io/badge/version-0.2.11-0f766e?style=for-the-badge
[tauri-shield]: https://img.shields.io/badge/Tauri-2-24C8DB?style=for-the-badge&logo=tauri&logoColor=white
[react-shield]: https://img.shields.io/badge/React-18-61DAFB?style=for-the-badge&logo=react&logoColor=111111
[rust-shield]: https://img.shields.io/badge/Rust-2021-000000?style=for-the-badge&logo=rust&logoColor=white
[macos-shield]: https://img.shields.io/badge/macOS-13%2B-111111?style=for-the-badge&logo=apple&logoColor=white
[license-shield]: https://img.shields.io/badge/License-MIT-yellow?style=for-the-badge
[github-url]: https://github.com/anurupkumar18/FNDR
[tauri-url]: https://tauri.app/
[react-url]: https://react.dev/
[rust-url]: https://www.rust-lang.org/
[tauri-config]: src-tauri/tauri.conf.json
[cargo-manifest]: src-tauri/Cargo.toml
