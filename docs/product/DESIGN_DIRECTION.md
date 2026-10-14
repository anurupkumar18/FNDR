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
│  whisper_gguf_runner.py │ orpheus_tts.py    │
└─────────────────────────────────────────────┘
```

### Key Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| App framework | Tauri (Rust) | Smaller binary, native Metal access, memory-safe |
| LLM runtime | llama-cpp-2 | Best macOS Metal performance for local inference |
| LLM model | Llama 3.2 1B Q4 (default) | Fast summaries on consumer hardware; optional Qwen3-VL 4B for advanced users |
| Vision import | CLIP ViT-B/32 vision ONNX | Tiny encoder for Meta glasses / photo imports; hybrid search remains OCR+BGE-first |
| Search | Hybrid semantic + keyword ranking | Better recall than either approach alone |
| Graph storage | LanceDB + local graph tables | Fast local retrieval with structured node/edge persistence |
| Agent SDK | Anthropic Messages API | Standard tool-use patterns, streaming support |
| Frontend | React + Vanilla CSS | Minimal dependencies, full control over design |

---

## UI/UX Design Principles

1. **Calm Interface**: Warm, muted palette. No aggressive colors. The app should fade into the background.
2. **Resume and Search First**: Home makes recent work and a memory query available together. Filters belong in Search and Ask.
3. **Progressive Disclosure**: Show summaries first, details on demand.
4. **Graph as Insight**: The knowledge graph is not just storage—it's a visualization tool for understanding your digital life patterns.
5. **Privacy Visible**: Capture status, blocklist controls, and incognito mode are always accessible.

### Home and Resume design contract, October 14, 2026

**Purpose.** A returning person should find prior work, reopen the latest source when one exists, and see an evidenced next step. Home is the entry point for this task. The current October plan measures the time it takes to recover prior work correctly.

**Hierarchy.** Search stays prominent, and the first recent-work card begins on the initial desktop screen. The vertical order is search, recent work, due soon, a due routine, saved sets, then the optional window arrangement control. A section that is loading, empty or unavailable states that plainly and keeps a route to Search or Vault. Cards are readable as soon as they enter view; content is never blurred or disabled to create a scroll effect.

**Type and layout.** Use `--film-font-display` for the greeting and `--film-font-ui` for the input, controls, cards and status. The greeting caps at 48 CSS px, section headings are 19 px, item titles are 16 px, and body, supporting and action text are at least 14 px. The content column caps at 720 px so the eye can move between the query and work cards. The search placeholder stays short and stable across times of day, and Speak keeps a visible word at narrow widths. Use the shared `--s-*` spacing scale for the top offset and card padding. Text actions have at least 40 px of height, visible focus and room to wrap on a narrow window. At 200% zoom, content must reflow without clipping actions or meaning.

**Color and motion.** Foreground cards use semantic `--surface`, `--surface-elevated`, `--text-primary`, `--text-secondary`, `--border` and `--accent` tokens over the wallpaper. The accent marks the primary continuation action and a due routine; status has words as well as color. Verify contrast against the composited surface in light and dark modes. The app's motion-off state and the operating system's Reduce Motion setting remove Home's entry animation; no reveal layer hides work.

**Actions and outcomes.** A thread's Continue action calls `reopen_memory` for its latest cited record without resolving or opening the whole set. Open all is a separate, secondary action through `resolve_work_set` and `open_work_set`; it requires another tap for more than three items. The actions cannot run concurrently. Reopen checks current visibility and action rules, then shows the typed outcome beside Continue, including missing, blocked, no-target and unavailable cases. View latest source remains available to inspect the memory in FNDR when an external place cannot reopen. Suggested steps link to their cited memory. No work opens on page load.

**Ownership and checks.** Rust `resume_work` owns grouping, ordering and current visibility rules. `WorkSetOpener` owns the shared foreground reopen state; Home composes the existing sections and IPC wrappers. This change adds no storage, command or model path. Check the Home component tests and typecheck, inspect the first screen and action states in the running browser UI in both themes, then inspect the native Tauri window with a synthetic profile whose capture is paused. Browser IPC fixtures establish presentation only; real reopen behavior needs a native result from an actual supported source.

### Screen Guide overlay

Screen Guide should feel like a small native FNDR affordance, not a second app.
An OS-managed status item keeps FNDR quietly present beside the notch/menu bar
and uses only a bounded set of micro-states: ready, listening, on-device
processing, finding, found, or attention needed. It must never display a user's
question, screen text, filename, path, answer, or detailed error, and it must
not draw over the physical notch or depend on private placement APIs.
Transient listening and answer feedback appears just below the menu bar so it
remains visible on both notched and non-notched displays.

The panel uses the existing warm surfaces and typography; its transient point
cue uses a restrained blue marker so it remains distinct from orange primary
actions. Holding the shortcut should make listening feedback immediate, and the
answer is always available as text even when optional local speech is muted or
interrupted. The overlay never intercepts clicks, and Reduce Motion replaces
travel animation with a simple appearance at the target.

### Color System

Product surfaces use semantic light and dark tokens defined in `src/shared/theme/film-paper.css` and the active cinematic palette. Component CSS uses `--surface`, `--text-primary`, `--text-secondary`, `--border`, `--focus-ring` and `--accent` so a palette change preserves hierarchy and contrast. Graph colors carry a label or shape in addition to hue.

---

## Data Flow & Privacy Model

```
Screen Capture → Deduplication → OCR → VLM Analysis → LLM Summary
                                  ↓
                           Graph Ingestion
                                  ↓
                    Local LanceDB + graph tables
```

### Privacy Guarantees

- **No network calls** for inference (all models run on-device via Metal)
- **Blocklist** prevents capture of specific apps
- **Incognito mode** pauses all capture
- **Data retention** configurable (auto-delete after N days)
- **API keys** only used for optional cloud agent features (Claude)

---

## Graph Integration Strategy

### Current State (v1)
- Local Rust `GraphStore` persisted through LanceDB-backed tables
- Nodes: MemoryChunk, Entity, Task, Url
- Edges: PartOfSession, ReferenceForTask, OccurredAt

### Future State (v2 — Optional)
- Richer entity extraction and temporal graph analytics
- Temporal knowledge decay and community detection
- Graph-enhanced search that traverses relationships for better context

### Migration Path
1. Local LanceDB graph remains the primary, always-available store
2. Any future graph enrichment runtime must remain optional and additive.

---

## Agent System

### Architecture
- **Hermes runtime**: the one agent path, surfaced on the Agent page. FNDR runs a pinned Hermes gateway as a child process and talks to it over a local port.
- The legacy `agent_runner.py` subprocess and its commands were removed on 2026-10-08.

### Available Tools
Hermes started by FNDR gets FNDR's read-only memory tools over MCP and its own `todo` tool, and nothing else: no terminal, file or browser tools. See ADR 024.

---

## Future Roadmap

### Near Term
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
2. Add Rust backend APIs in `src-tauri/src/ipc/commands/` (new file + `mod.rs` registration) or extend an existing command module.
3. Add TypeScript bindings in `src/shared/ipc/tauri.ts`.
4. Add UI under `src/domains/<domain>/` (see `src/domains/README.md`) and wire it from `src/app/AppPanels.tsx` or `src/app/App.tsx` as appropriate.
5. Register the Tauri command in `src-tauri/src/main.rs` (or the library builder) alongside existing invokes.
