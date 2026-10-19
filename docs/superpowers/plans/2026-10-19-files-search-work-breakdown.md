# Files search work breakdown

Decision: [ADR 033](../../decisions/033-files-search-from-digup.md). Branch: `feat/files-search`. Everything ships behind the `files_search_enabled` setting, off until part 9.

Status on 2026-10-19: parts 1 to 10 are built on the branch and the eval passes on a Mac (R@1 1.00 over 20 queries; ADR 033, Measured). The surfaces have not been seen in the running app yet.

A parity pass against DigUp's app (the same evening) added what the first build lacked: in Quick Find, word matches while typing, results grouped by kind with Show all, the weaker matches folded behind a row, Quick Look thumbnails, Space for Quick Look, Cmd+O to the bigger window, and notes on incomplete results; in Search and Ask, filters with counts and the keyboard; a preview pane that plays from the moment and shows the passage or code lines; the power policy (Low Power Mode, a warm Mac, battery) and any indexing hours; per-folder progress, reach and subfolders in Settings; code folders offered in onboarding; model files downloaded elsewhere. Not ported: dragging a result out (needs a native drag plugin, open with Kunj) and DigUp's empty-window example chips (Search and Ask has its own empty state).

Each part is one vertical slice with its own gate. A part is done when its gate passes and a check against the real program (the model, Finder, QuickTime, an editor) has run once on a Mac where it depends on one.

## DigUp capability map

Every DigUp capability, and the part that brings it into FNDR.

| DigUp capability | DigUp source | FNDR part |
|---|---|---|
| File rules: what is read, what is never read, code projects, datasets | `Rules.swift` | 1 |
| Text normalization for exact words (NFKC, Arabic, Bengali, inflecting scripts) | `SearchText.swift` | 1 |
| One index file: files, segments, fp16 vectors, FTS5 | `IndexStore.swift`, `Database.swift` | 1 |
| Hybrid search: per-modality z-scored meaning, word coverage, lookups, kind hints, strong-match fold | `Searcher.swift` | 1 |
| Crawl chosen folders, include and exclude subfolders, cloud-only skip | `Crawler.swift`, `FolderSelection.swift` | 1 |
| Documents: txt, md, html, docx, odt, rtf; long ones in two passes | `Extractors.swift`, `Docs+Portable.swift` | 1 |
| EmbeddingGemma 2 on llama.cpp, text model only for queries | `LlamaEmbedder.swift`, `LlamaModels.swift` | 2 |
| Helper process that quits when done, watchdog, parent watch | `digup/WorkerService.swift`, `Watchdog.swift`, `ParentWatch.swift` | 2 |
| Pinned model download with resume and SHA-256 | `ModelFiles.swift`, `ModelDownload.swift` | 2 |
| Reference-vector test against every llama.cpp update | `ReferenceVectorTests.swift` | 2 |
| PDFs by page, scans included, first 30 pages then the rest | `Extractors+Apple.swift`, `Indexer.swift` | 3 |
| Pictures by what they show (JPEG, PNG, HEIC, WebP, AVIF, RAW on Mac), EXIF date | `MediaExtractors.swift`, `ImageSizing.swift` | 4 |
| Screenshots by their text (Apple Vision), screenshot marker | `Extractors+Apple.swift`, `Rules.isScreenshot` | 4 |
| Audio of any length in overlapping 30 s windows, match at the moment | `MediaExtractors.swift` | 5 |
| Video: a frame per shot, plus the soundtrack | `MediaExtractors.swift`, `Media+Apple.swift` | 5 |
| Folder watcher: a new file searchable seconds after it lands | `FolderWatcher.swift` | 6 |
| Indexing hours, power policy, measured cost estimates per folder | `IndexHours.swift`, `PowerPolicy.swift`, `Estimator.swift`, `MeasuredCosts.swift` | 6 |
| Quick Find and notch results, `file:` prefix, keyboard actions | `SearchPanel*.swift` | 7 |
| Open at the page (Preview with a landmark word), at the moment (QuickTime, IINA) | `Moments.swift`, `Landmark` | 7 |
| Big preview at the page or moment, thumbnails | `Previews.swift`, `Thumbnails.swift`, `PreviewImages.swift` | 7 |
| Copy the file, reveal in Finder | `SearchPanel.swift` | 7 |
| Bigger window with picture grid and filters | `SearchWindow.swift` | 7 (Search and Ask Files filter) |
| Code search: separate index, `code:` prefix, definitions over call sites, notebooks | `CodeExtractor.swift`, `CodeFolders.swift` | 8 |
| Open code in the chosen editor at the line | `CodeEditors.swift` | 8 |
| Onboarding: folder choice with time per folder while the model downloads | `Onboarding.swift`, `FolderChoices.swift`, `AddFolders.swift` | 9 |
| Settings: folders, skipped folders and types, code, hours | `SettingsWindow.swift` | 9 |
| Status in the menu bar, pause | `StatusMenu.swift` | 9 (status and pause in the Setup Center; the menu bar item is not ported) |
| CLI for indexing, search and evals | `digup/main.swift` | 10 (`cargo run --example files_eval`) |
| Not ported: Sparkle and DigUp's update feed (FNDR's updater covers it), Windows and Linux shell | `Updates.swift`, `shell/` | none |

## Parts

### 1. Engine core, words first

`src-tauri/src/files/`: `rules`, `search_text`, `store`, `searcher`, `crawl`, `extract` (documents). Meaning search runs through an `Embedder` trait so the searcher is tested with a fake; real vectors come in part 2.

Gate: `cargo test --lib files::` covers the never-read rules, project and dataset detection, normalization, FTS5 round trips, the meaning scale, lookups, kind hints and the strong-match count, each against DigUp's documented examples.

### 2. EmbeddingGemma 2 helper

`fndr-files-helper` (`src-tauri/helpers/fndr-files`), a separate executable with its own llama.cpp b11461 and the no-logits patch (`scripts/files/build-llama.sh`), answering JSON lines (text, image, audio); `files::helper` and `files::process` drive it; `files::model` pins and downloads the two files; `--check` holds DigUp's reference vectors.

Gate: reference vectors match within DigUp's tolerance on the Mac; Qwen3-VL extraction suite unchanged on the bumped build.

### 3. PDFs

Page text through PDFKit (macOS) with the glyph-order and lost-letter checks, a rendered page image when the text layer fails, first 30 pages then the rest.

### 4. Pictures and screenshots

### 5. Audio and video moments

### 6. Live index: watcher, hours, pressure gate, estimates

`files::service`, `files::settings`, `files::estimate`; the app's hold is memory pressure and Qwen's model lock.

### 7. Surfaces: Quick Find, notch, Search and Ask, previews, open at place

Gate: `ipcDrift.test.ts` green with the new commands; Quick Find test for merged memory and file rows; a native check of open at page and at moment.

### 8. Code search

### 9. Onboarding and Settings

An opt-in onboarding step after imports, and a Files section in the Setup Center. Files search stays off until the person turns it on.

### 10. Evals

DigUp's eval method (R@1 over labeled queries) as `files_eval`, on a synthetic testbed built by `scripts/files/make-testbed.sh` (no real files in the repo), queries in `evals/files/queries.json`, `make files-eval`, and a row in the AGENTS.md gate table.
