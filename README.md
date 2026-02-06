# FNDR - Privacy-First Local Memory Assistant

FNDR is a local-only screen history search and AI assistant for macOS. It captures your screen, runs OCR, and uses a local Large Language Model (LLM) to summarize your activity and answer questions—all without your data ever leaving your machine.

## Features

- **Local Capture**: Periodically snapshots your screen.
- **Privacy First**: Built-in blocklist to ignore sensitive apps.
- **AI Summaries**: Turns messy OCR fragments into clean, human-readable events.
- **"Ask FNDR"**: Chat with your local history using a Retrieval-Augmented Generation (RAG) pipeline.
- **High Performance**: Optimized for Apple Silicon using Metal acceleration.

## Prerequisites

- **macOS 13.0+**
- **Apple Silicon (M1/M2/M3)** (Intel Mac support is limited)
- **Rust Toolchain**
- **Node.js & npm**
- **CMake** (required for building `llama.cpp`)

## Setup

1. **Clone the repository**:
   ```bash
   git clone <your-repo-url>
   cd fndr
   ```

2. **Download the AI Model**:
   We use the Llama 3.2 1B Instruct model. Run the helper script to fetch it:
   ```bash
   chmod +x download_model.sh
   ./download_model.sh
   ```

3. **Install dependencies**:
   ```bash
   npm install
   ```

4. **Run the application**:
   ```bash
   npm run tauri dev
   ```

## Technical Architecture

- **Frontend**: React + Vite + Vanilla CSS
- **Backend**: Rust + Tauri
- **AI Engine**: `llama-cpp-2` (GGUF inference)
  - **Text LLM**: Llama 3.2 1B for summarization and RAG
  - **Vision LLM**: SmolVLM 500M/256M for intelligent screen understanding
- **OCR**: Apple Vision Framework
- **Storage**: JSON-based vector store (SimpleStore)

## Privacy Note
All processing happens 100% locally on your machine. No text, images, or queries are sent to any cloud provider.

## License
MIT
