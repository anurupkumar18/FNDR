# FNDR Design Direction

> A living document describing the architecture decisions, design principles, and future roadmap for FNDR.

---

## Vision

FNDR is a **local-first, privacy-focused memory assistant** for macOS. It continuously captures screen activity, extracts meaning using on-device AI, and builds a searchable knowledge graph—all without sending data to the cloud.

**Core Principle**: Your data stays on your machine. Intelligence happens locally.

---

## Architecture

```
┌─────────────────────────────────────────────┐
│                 React UI (Vite)              │
│  SearchBar │ Timeline │ GraphPanel │ Agents  │
├─────────────────────────────────────────────┤
│              Tauri IPC Bridge               │
├─────────────────────────────────────────────┤
│                Rust Core (Tauri)             │
│  ┌──────────┐ ┌──────────┐ ┌─────────────┐ │
│  │ Capture  │ │  Search  │ │ Inference   │ │
│  │ Pipeline │ │ (Hybrid) │ │ LLM + VLM   │ │
│  └──────────┘ └──────────┘ └─────────────┘ │
│  ┌──────────┐ ┌──────────┐ ┌─────────────┐ │
│  │  Graph   │ │   OCR    │ │  Privacy    │ │
│  │  Store   │ │ (Vision) │ │ (Blocklist) │ │
│  └──────────┘ └──────────┘ └─────────────┘ │
├─────────────────────────────────────────────┤
│          Python Sidecars (Optional)         │
│  agent_runner.py │ graphiti_service.py      │
│  cua_runner.py                              │
└─────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| App framework | Tauri (Rust) | Smaller binary, native Metal access, memory-safe |
| LLM runtime | llama-cpp-2 | Best macOS Metal performance for local inference |
| LLM model | Llama 3.2 1B Q4 | Balance of quality and speed on consumer hardware |
| VLM model | SmolVLM-500M/256M | Tiny footprint for on-device screen understanding |
| Search | Hybrid (semantic + keyword + RRF) | Better recall than either approach alone |
| Graph storage | JSON + optional Graphiti | Works offline, upgradeable to full graph DB |
| Agent SDK | Anthropic Messages API | Standard tool-use patterns, streaming support |
| Frontend | React + Vanilla CSS | Minimal dependencies, full control over design |

---

## UI/UX Design Principles

1. **Calm Interface**: Warm, muted palette. No aggressive colors. The app should fade into the background.
2. **Search-First**: Everything is accessible through the search bar. Power users can use filters.
3. **Progressive Disclosure**: Show summaries first, details on demand.
4. **Graph as Insight**: The knowledge graph is not just storage—it's a visualization tool for understanding your digital life patterns.
5. **Privacy Visible**: Capture status, blocklist controls, and incognito mode are always accessible.

### Color System

- **Background**: Warm white (#FAF9F6) with subtle grain
- **Text**: Deep brown (#3E2723) for warmth over cold grays
- **Accent**: Warm orange (#E65100) for primary actions
- **Graph nodes**: Blue (memories), Purple (entities), Orange (tasks), Green (URLs)

---

## Data Flow & Privacy Model

```
Screen Capture → Deduplication → OCR → VLM Analysis → LLM Summary
                                  ↓
                           Graph Ingestion
                                  ↓
                      Local JSON / Graphiti DB
```

### Privacy Guarantees

- **No network calls** for inference (all models run on-device via Metal)
- **Blocklist** prevents capture of specific apps
- **Incognito mode** pauses all capture
- **Data retention** configurable (auto-delete after N days)
- **API keys** only used for optional cloud agent features (Claude)

---

## Graphiti Integration Strategy

### Current State (v1)
- Custom Rust `GraphStore` using JSON persistence
- Nodes: MemoryChunk, Entity, Task, Url
- Edges: MentionedIn, Related, UsedIn, FollowedBy, ExtractedFrom, LinksTo

### Future State (v2 — Optional)
- Python sidecar running `graphiti-core` with FalkorDB
- Richer entity extraction using Anthropic LLM
- Temporal knowledge decay and community detection
- Graph-enhanced search (traverse relationships for better context)

### Migration Path
1. The local JSON graph remains the primary, always-available store
2. Graphiti sidecar is optional—starts only if FalkorDB is running
3. Both systems can coexist; Graphiti enriches but doesn't replace

---

## Agent System

### Architecture
- **agent_runner.py**: Uses Anthropic Messages API with tool-use loop
- **cua_runner.py**: Computer-use agent for GUI automation
- **Communication**: JSON over stdin/stdout from Tauri subprocess

### Available Tools
| Tool | Purpose | Risk |
|---|---|---|
| `read_file` | Read local files | Low |
| `write_file` | Create/edit files | Medium |
| `run_command` | Execute shell commands | High |
| `web_search` | Search the web | Low |
| `report_critical_point` | Human-in-the-loop gate | None |

### Safety Model
- **Critical Points**: Agent must stop and report before purchases, form submissions, emails, or data deletion
- **Timeout**: Commands have a 30-second timeout
- **Output Truncation**: All tool outputs are truncated to prevent context overflow

---

## Future Roadmap

### Near Term
- [ ] LanceDB vector store (replace JSON-based embeddings)
- [ ] Advanced idle detection (mouse/keyboard activity)
- [ ] Multi-monitor capture support
- [ ] Metal kernel pre-compilation (eliminate cold-start latency)

### Medium Term
- [ ] Semantic timeline (group by topic, not just time)
- [ ] Activity patterns and insights dashboard
- [ ] Smart notifications ("You left that email unfinished")
- [ ] Export/import knowledge graph

### Long Term
- [ ] Cross-device sync (encrypted, user-controlled)
- [ ] Plugin system for custom extractors
- [ ] Voice memo integration
- [ ] On-device fine-tuning for personal vocabulary

---

## Contributing

### Code Style
- **Rust**: Follow `rustfmt` defaults. Use `tracing` for logging.
- **TypeScript**: Use functional React components with hooks.
- **Python**: Follow PEP 8. Use type hints.

### Testing
- Rust tests: `cargo test` in `src-tauri/`
- Frontend: Manual testing via `npm run tauri dev`
- Python sidecars: Test independently with `python -m pytest`

### Adding a New Feature
1. Update this design document with the feature's rationale
2. Add Rust backend APIs in `src-tauri/src/api/commands.rs`
3. Add TypeScript bindings in `src/api/tauri.ts`
4. Create the React component in `src/components/`
5. Register the Tauri command in `src-tauri/src/main.rs`
