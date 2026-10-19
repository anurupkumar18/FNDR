# ADR 033: Files search, DigUp's engine ported into FNDR

## Status

Proposed 2026-10-19. The four choices below were made in a 2026-10-19 session Q&A by the person who asked for the integration; the decision-log row is added when the ADR is accepted under the two-day rule. Building happens on `feat/files-search` behind a setting that is off by default, so Beta ships untouched.

- Parts and gates: [work breakdown](../superpowers/plans/2026-10-19-files-search-work-breakdown.md)
- Source: [ARahim3/DigUp](https://github.com/ARahim3/DigUp) (MIT), read at its default branch on 2026-10-19.

## Ownership

Files search is Kunj's rank 3 feature (owner's rule, 2026-10-19). Only Kunj writes its code. Teammates test it, critique it and comment on anything: the UI, how it works, the experience, speed, results. They do that through `docs/team/files-search-feedback.md`, issues or merge request comments, not commits. A teammate who finds a bug reports it with steps; Kunj fixes it. Paths this covers:

- `src-tauri/src/files/`, `src-tauri/src/ipc/commands/files.rs`, the `SearchFiles` step in `src-tauri/src/ipc/onboarding.rs`
- `src-tauri/helpers/fndr-files/`, `src-tauri/helpers/fndr-files-reader/`, `src-tauri/examples/files_eval.rs`, `src-tauri/tauri.files.conf.json`
- `scripts/files/`, `evals/files/`, the `files-*` Makefile targets
- `src/shared/files/`, `src/shared/ipc/files.ts`, `src/domains/setup/FilesSetup.*`, `src/domains/search/FilesResults.*`, and the file rows in `src/domains/omnibar/` and `src/domains/notch/`

## Question

DigUp searches the files on a computer by what is in them: pictures by what they show, PDFs by page, recordings and videos by moment, documents, and code. It does this with EmbeddingGemma 2, one multimodal embedding model that puts text, images, audio and video in one 768-dimensional space, plus an exact-word index. How does every DigUp capability become a native FNDR feature, reachable from anywhere on the Mac and from the notch, without a second app?

## What is true today

- FNDR's search is over memories (screen captures, meetings, imports) in LanceDB with MiniLM 384-d text vectors and a tantivy keyword index (ADR 003, ADR 007). Files reach memory only three ways: the Downloads watcher, the `file` import connector (text files up to 5 MB, recent ones), and photo import with a 512-d CLIP vector used only for image-to-image.
- ADR 019 measured EmbeddingGemma (v1, 300M, text only) as the best text model on FNDR's sets and left two things open: the M1 8 GB budget with capture running, and the Gemma Terms of Use. The `V6EmbeddingGemma` contract exists in `model_config.rs` but is not active.
- FNDR links llama.cpp through `llama-cpp-2` 0.1 with `metal` and `mtmd` (used for Qwen3-VL). DigUp needs llama.cpp tag b11452 or later for EmbeddingGemma 2 (text, vision, audio) plus a one-line patch that stops an embedding context reserving logits.
- Quick Find (Alt+Space, `omnibar.html`) and the notch already search memories from any app. ADR 023 fixes five destinations.

## Decision

1. **Engine: a Rust port, with the model in a helper process.** DigUp's crawler, file rules, readers, index and searcher are rewritten in Rust under `src-tauri/src/files/`. EmbeddingGemma 2 runs in `fndr-files-helper` (`src-tauri/helpers/fndr-files`), a separate executable beside `fndr-speech` and `fndr-foundation`, which links its own llama.cpp at DigUp's pinned tag (b11461, commit `4d756bc`) with DigUp's no-logits patch (`scripts/files/build-llama.sh`). It loads the media encoder only while indexing and quits when the app closes its input. Queries load only the 310 MB text model. A Metal fault in the helper cannot take the app down, and Qwen and EmbeddingGemma 2 never share a process or a llama.cpp build.

2. **Data: a separate Files index, results merged at the surface.** Files live in their own SQLite file (FTS5 for words, 768 fp16 vectors in a BLOB column, brute-force cosine as DigUp does) under FNDR's app data, plus a second one for code. Files are never `MemoryRecord`s: they do not enter the Vault, Daily Brief, Resume, review, or the MCP memory tools. Memories stay on their current contract, so ADR 019 stays independent of this ADR.
3. **Entry points: Quick Find and the notch, no new hotkey.** Quick Find and the notch query memories and files in parallel and show one list; a file row says File and its kind. `file:` and `code:` narrow to files or code. Enter opens at the page, moment or line; Space previews there; Cmd+Enter reveals in Finder; Cmd+C copies the file. Search and Ask gets a Files filter with DigUp's picture grid. Notch Do opens or reveals a found file through its existing typed actions.
4. **Timing: a branch and a setting, aimed at Final.** Slices land on `feat/files-search` in the order of the work breakdown. Onboarding gains an opt-in "Search your files" step: choose folders, see the measured first-pass time for each, and download the 865 MB model only after that consent.

## Options rejected

| Area | Option | Why not |
|---|---|---|
| Engine | Bundle DigUp's Swift engine as a sidecar CLI | Fastest, but a second engine, a second llama.cpp build and Swift in FNDR's build; integrated, not native. |
| Engine | Rust port inside the main process | 865 MB beside Qwen on an 8 GB M1, where ADR 019 already measured contention, and a Metal fault ends the app. |
| Data | Move all of FNDR to EmbeddingGemma 2 | Cleanest end state, but it decides ADR 019 by the back door and needs a full memory reindex with retrieval-gate evidence. Kept as a possible later ADR once Files has run on real profiles. |
| Data | Files become memory records | Thousands of pages and frames would flood the Vault, Daily Brief and Resume with rows that are not activity, and it needs the migration above too. |
| Entry | Keep DigUp's Shift+Cmd+Space | One more global shortcut to own for muscle memory FNDR's users do not have. |
| Entry | A separate Files panel and window | Two search bars, and a sixth destination against ADR 023. |
| Timing | A thin slice into Beta (Oct 21) | A llama.cpp bump two days before a release. |

## Consequences

- **Privacy.** Files search reads only folders the person chose, and DigUp's never-read rules carry over unchanged: keys, certificates, password and credential files, hidden folders, `~/Library`, apps, caches, dataset-like folders, cloud-only files. Inside a code project only screenshots are read unless code search is on for it. The FNDR blocklist applies to paths. Nothing leaves the Mac; the only network use is the one-time pinned model download (SHA-256 checked). Files are only read, never changed, moved or uploaded. Indexed text is evidence and never reaches a prompt as an instruction.
- **Models.** `model_config.rs` gains a Files contract (EmbeddingGemma 2 Q8_0 GGUF from `ggml-org/embeddinggemma-2-GGUF` at revision `bfcd2987`, text 309,855,456 bytes, media 554,821,024 bytes, with DigUp's pinned SHA-256s). The index stores the vector version that made it; an engine update that changes vectors forces a rebuild, guarded by reference-vector tests as DigUp does.
- **llama.cpp.** Two builds, never in one binary: Qwen stays on `llama-cpp-2` as on `main`; the Files helper has the pinned, patched b11461. A tag bump ships only when `make files-check` passes (DigUp's nine reference vectors at cosine 0.999), or the helper's `VECTOR_VERSION` is bumped and every Files index embeds again.
- **Surfaces.** Quick Find, the notch and Search and Ask gain a file result type and file actions. `ipcDrift.test.ts` covers the new commands; `memoryReadDrift.test.ts` is unaffected because files are not memories.
- **Cost.** The first pass is the heavy part and runs under FNDR's existing pressure gate, paused while capture extraction holds the model lock, and within the indexing hours the person picks.

## Measured

On Kunj's Mac (macOS 27.2, Apple Silicon, 16 GB), 2026-10-19:

- `make files-check`: all nine DigUp reference vectors (three queries in English, Bengali and Arabic, two documents, a screenshot at 280 tokens, a photo at 140 and 70, 30 s of speech) and a batched-texts row agree at cosine 1.00000 with llama.cpp b11461 on Metal.
- A query takes about 9 ms. Loading the text model takes 0.6 s; text plus media encoders 9 s.
- Peak physical footprint: 178 MB for the text-only helper that answers searches, 858 MB for the full helper that runs only while indexing.
- `cargo test --lib files::` in the app crate passes (73 tests at that commit), and `fndr-files-reader` reads real PDFs, pictures and recordings.
- `files_eval` on the synthetic testbed (42 files: documents, a docx, PDFs, screenshots, recordings, Bengali and Arabic) at `d8b7eb2`: R@1 1.00, R@5 1.00, MRR 1.000 over 20 queries; indexed in 9.2 s with the model warm. The first run scored R@1 0.90: Vision read the testbed's transparent PNG screenshots as black on black, so they are now flattened on white before OCR.
- At the same commit, `cargo check` of the whole app crate is clean and `cargo test --lib files::` passes 85 tests.
- The debug app (`ea1c352` with the UI pass), on a scratch profile with Files turned on for the testbed: 42 files read in about 60 s once indexing started; the full helper peaked near 1.05 GB resident and quit when the work was done; a new file was searchable within 5 s. A deleted file stayed for about a minute and the first pass waited about a minute to start, both fixed in `3020ea7`. The reader returned a whole PNG thumbnail and a 5 s AAC clip; `cargo test --lib files::` passed 94 tests.

## Open before acceptance

1. **License: settled 2026-10-19.** The `google/embeddinggemma-2` model card says `license: apache-2.0`, ungated (Hugging Face API read on the Mac, and confirmed by Kunj). Unlike EmbeddingGemma 300M, no Gemma Terms notice is owed. THIRD_PARTY_NOTICES.md names it.
2. **llama.cpp version: settled 2026-10-19.** The newest `llama-cpp-sys-2` (0.1.160) builds on the Mac but cannot load the model (`unknown model architecture: 'gemma-embedding2'`). Its llama.cpp predates b11452, and it offers no way to point at another source. Hence the separate helper with its own llama.cpp in decision 1. Bumping the app's `llama-cpp-2` was rejected: it would not have loaded the model, and it would have changed Qwen's runtime two days before Beta.
3. **MCP.** Whether outside agents may search files through MCP. Proposed default: no, until a separate decision. Files search registers no MCP tool.
