# ADR 033: Files search, DigUp's engine ported into FNDR

## Status

Proposed 2026-10-19. The four choices below were made in a 2026-10-19 session Q&A by the person who asked for the integration; the decision-log row is added when the ADR is accepted under the two-day rule. Building happens on `feat/files-search` behind a setting that is off by default, so Beta ships untouched.

- Parts and gates: [work breakdown](../superpowers/plans/2026-10-19-files-search-work-breakdown.md)
- Source: [ARahim3/DigUp](https://github.com/ARahim3/DigUp) (MIT), read at its default branch on 2026-10-19.

## Question

DigUp searches the files on a computer by what is in them: pictures by what they show, PDFs by page, recordings and videos by moment, documents, and code. It does this with EmbeddingGemma 2, one multimodal embedding model that puts text, images, audio and video in one 768-dimensional space, plus an exact-word index. How does every DigUp capability become a native FNDR feature, reachable from anywhere on the Mac and from the notch, without a second app?

## What is true today

- FNDR's search is over memories (screen captures, meetings, imports) in LanceDB with MiniLM 384-d text vectors and a tantivy keyword index (ADR 003, ADR 007). Files reach memory only three ways: the Downloads watcher, the `file` import connector (text files up to 5 MB, recent ones), and photo import with a 512-d CLIP vector used only for image-to-image.
- ADR 019 measured EmbeddingGemma (v1, 300M, text only) as the best text model on FNDR's sets and left two things open: the M1 8 GB budget with capture running, and the Gemma Terms of Use. The `V6EmbeddingGemma` contract exists in `model_config.rs` but is not active.
- FNDR links llama.cpp through `llama-cpp-2` 0.1 with `metal` and `mtmd` (used for Qwen3-VL). DigUp needs llama.cpp tag b11452 or later for EmbeddingGemma 2 (text, vision, audio) plus a one-line patch that stops an embedding context reserving logits.
- Quick Find (Alt+Space, `omnibar.html`) and the notch already search memories from any app. ADR 023 fixes five destinations.

## Decision

1. **Engine: a Rust port, run in a helper process.** DigUp's crawler, file rules, readers, index and searcher are rewritten in Rust under `src-tauri/src/files/`. EmbeddingGemma 2 runs through FNDR's own `llama-cpp-2` in a child process started from the FNDR binary (the pattern `voice` and `inference::foundation` already use), which loads the media encoder only while indexing and quits when idle. Queries load only the 310 MB text model. A Metal fault in the helper cannot take the app down, and Qwen and EmbeddingGemma 2 never share a process.
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
- **llama.cpp.** `llama-cpp-2` must reach a llama.cpp build with EmbeddingGemma 2, and the no-logits patch must be carried (vendored `llama-cpp-sys-2` patch or an upstream fix). Qwen3-VL extraction must be re-checked on the bumped build before merge.
- **Surfaces.** Quick Find, the notch and Search and Ask gain a file result type and file actions. `ipcDrift.test.ts` covers the new commands; `memoryReadDrift.test.ts` is unaffected because files are not memories.
- **Cost.** The first pass is the heavy part and runs under FNDR's existing pressure gate, paused while capture extraction holds the model lock, and within the indexing hours the person picks.

## Open before acceptance

1. **License.** DigUp's README says EmbeddingGemma 2 is Apache 2.0, unlike EmbeddingGemma 300M's Gemma Terms. This was not verifiable from the build container (Hugging Face blocked); confirm on the model card before the model download slice.
2. **llama.cpp version.** Which llama.cpp commit `llama-cpp-sys-2` 0.1.143 vendors, and the first crate release at or after b11452.
3. **MCP.** Whether outside agents may search files through MCP. Proposed default: no, until a separate decision.
