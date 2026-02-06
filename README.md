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
- **Privacy Controls**: Built-in blocklist to exclude sensitive applications and incognito mode.
- **High Performance**: Native Rust core with Metal acceleration for Apple Silicon.

### Planned Features (What's Next)
- **CUA Agent Execution**: Actually executing the todos it finds (e.g., "Send that email").
- **Enhanced Vector Store**: Migration to LanceDB for scalable, persistent vector search.
- **Advanced Idle Detection**: Smarter capture logic based on user activity.
- **Multi-Monitor Support**: Comprehensive capture across all connected displays.

## 🚦 Current Status

| Feature | Status | Notes |
| :--- | :--- | :--- |
| **Search** | ✅ Working | Hybrid search (Semantic + Keyword) is functional. |
| **Inference** | ✅ Working | Llama 3.2 1B runs locally with Metal acceleration. |
| **VLM** | ✅ Working | SmolVLM integration is operational on M-series chips. |
| **OCR** | ✅ Working | Fast and accurate via Apple Vision Framework. |
| **Todo Extraction** | 🟡 Partial | Extraction works; execution agent is a prototype. |
| **Persistence** | 🟡 Partial | Using JSON storage; migration to DB planned. |

## 🛠 How to Run

### Prerequisites
- **macOS 13.0+**
- **Apple Silicon (M1/M2/M3)** recommended for best AI performance.
- **Rust Toolchain**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Node.js & npm**: [LTS version recommended](https://nodejs.org/).
- **CMake**: Required for building AI inference libraries (`brew install cmake`).

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

## 📄 License
MIT
