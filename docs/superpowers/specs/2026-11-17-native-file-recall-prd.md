# Native file finding

Selected local files are searchable by their contents or meaning through a separate index and EmbeddingGemma 2 runtime. The model download is optional during onboarding. Development builds enable the feature by default; folders remain explicitly selected.

## User workflows

1. During onboarding, choose ordinary file folders and optional code folders, inspect the first-pass estimate, and accept the approximately 865 MB model download from Hugging Face. "Not now" completes setup without the feature.
2. Change these choices later in Settings, including exclusions, schedule, battery policy, pause/resume, download retry, and forgetting the index.
3. Search through Quick Find, the notch, or the Files section of Search. Use `file:` for files and `code:` for code.
4. Inspect matching text, a PDF page, image, video frame, or recording moment. Open the source, reveal it in Finder, or use the matching code line.

## Requirements

- Files have their own stores, vector contract, settings, and worker. No file parent or segment becomes a screen memory.
- Release default is off. Debug default is on with empty roots. A saved disabled choice is respected, and invalid settings fail closed.
- No model download begins until an explicit onboarding or Settings action. Downloading uses pinned URLs and SHA-256 checks, supports partial downloads, and reports errors.
- Selected roots and exclusions are checked while indexing and before returning source content or executing file actions.
- Folder changes reconcile additions, edits, moves, and removals. Missing volumes are distinguished from deleted files.
- Segments preserve page, timestamp, or line locations. Search rolls hits up to a file result and folds weaker matches.
- Text, PDFs, images, audio, video, and code use the reused native Files implementation. Extraction remains local.
- The indexing helper yields to pressure, schedule, and power policy. A stopped helper releases its model; queries use a smaller text-only helper.
- File content and indexed OCR are evidence and never instructions or action authority.
- Core onboarding remains usable after declining optional file search or when its helpers are unavailable.

## Implementation boundaries

| Path | Responsibility |
| --- | --- |
| `src-tauri/src/files/` | File rules, crawling, extraction, SQLite/FTS/vector storage, search, settings, and service |
| `src-tauri/helpers/fndr-files/` | Isolated pinned EmbeddingGemma 2 inference |
| `src-tauri/helpers/fndr-files-reader/` | Native macOS extraction, previews, and file operations |
| `src-tauri/src/ipc/commands/files.rs` | Typed native capability boundary |
| `src/shared/ipc/files.ts`, `src/shared/files/` | IPC, result models, search state, previews, and actions |
| `src/domains/setup/FilesSetup.tsx` | Onboarding opt-in and Settings controls |
| `src/domains/search/FilesResults.tsx` | Kind filters, file-level hits, and matching evidence |

## Verification

Use focused Rust Files tests, frontend setup/search/preview tests, IPC and documentation drift checks, typecheck, production frontend build, the real helper reference check, and a real macOS extraction/retrieval evaluation on synthetic fixtures. Screen-memory retrieval gates are required only if a change touches that separate path.

See [ADR 039](../../decisions/039-native-file-recall-boundary.md) and the [implementation record](../plans/2026-11-17-native-file-finding.md) for the current verification evidence.
