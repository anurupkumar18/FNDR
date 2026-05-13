# `src-tauri/src` — Rust crate layout

Modules follow **product and infrastructure boundaries**. Prefer extending an existing module over adding a parallel top-level crate.

## Ingestion & signal

| Module | Role |
| --- | --- |
| `capture/` | Screen sampling, OCR pipeline hooks, memory assembly. |
| `ocr/` | Apple Vision integration. |
| `embedding/` | Chunking and local embedding (ONNX / backends). |
| `downloads/` | Downloads-folder watcher. |
| `accessibility/` | macOS accessibility text capture. |

## Persistence & retrieval

| Module | Role |
| --- | --- |
| `storage/` | LanceDB schemas, migrations, `graph_store`, compaction. |
| `search/` | Hybrid retrieval, memory card projection, ranking. |
| `memory_compaction/` | Memory compaction helpers. |
| `memory_quality/` | Quality gates and deduplication policy. |
| `memory_insight/` | Insight fields and embedding text derived from finalized memory. |
| `memory/` | Memory-centric graph (`memory/graph/`: schema, Louvain, traversal, legacy bridge). |

## Intelligence & runtime

| Module | Role |
| --- | --- |
| `inference/` | Local LLM / VLM engines. |
| `summariser/` | Summaries and briefings. |
| `timeline/` | Action classification for timeline buckets (`classify`, `classify_rules`). |
| `wiki/` | Wiki synthesis stubs / policies. |
| `context_runtime/` | MCP and context packs. |
| `mcp/` | MCP server surface. |

## Platform & I/O

| Module | Role |
| --- | --- |
| `ipc/` | Tauri **IPC** command handlers (`ipc/commands/` split modules). |
| `config/` | Central configuration (`config.rs` at crate root). |
| `privacy/` | Privacy classes and exclusions. |
| `http_util/` | Bounded HTTP clients. |
| `speech/` | Speech I/O and **sidecar** script resolution (`../sidecars/` in dev). |
| `meeting/` | Meeting recorder and segments. |
| `tasks/` | Task persistence. |
| `telemetry/` | Telemetry hooks. |
| `system_resources/` | Idle job gating (battery, CPU). |

`lib.rs` holds **`AppState`** and process-wide singletons; keep new shared state there or behind a small submodule rather than new globals.
