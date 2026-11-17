# ADR 039: Native file finding with its own index and model

## Status

Accepted for implementation by Kunj on 2026-11-17 in the direct request: add native file finding, keep it separate from the main pipeline, offer EmbeddingGemma 2 as an onboarding opt-in, and enable the feature by default in development.

## Decision

Reuse the committed FNDR Files implementation from `a762a80`, adapting its integration to the current product. Files use their own SQLite WAL stores, FTS5 indexes, 768-dimensional float16 vectors, settings, worker, and model helper. They never become `MemoryRecord` rows and do not enter capture, review, Vault, summaries, or MCP memory tools.

The native Rust engine lives in `src-tauri/src/files/`. A Swift reader uses PDFKit, Vision, ImageIO, and AVFoundation for extraction and previews. A Rust model helper links pinned llama.cpp b11461 in a separate process. This is required because the app's current llama.cpp cannot load the `gemma-embedding2` architecture. The helper loads image and audio encoders only for indexing; searches use the text model.

## Consent and defaults

- Release builds begin disabled. Onboarding offers selected folders and a separate EmbeddingGemma 2 download from Hugging Face, about 865 MB. Declining leaves ordinary memory setup usable.
- Debug builds begin enabled, with no folders selected. This enables development access without silently crawling personal directories or downloading model weights.
- Explicitly saved settings take precedence, including a saved disabled state. Invalid settings fail closed.
- Settings retains folder selection, exclusions, indexing schedule, power policy, pause, retry, and index removal controls.
- Models stay under FNDR's own models directory. Both Q8_0 assets have pinned repository revision, byte size, and SHA-256 checks. The older EmbeddingGemma 300M contract is unrelated.

## Indexing and retrieval

The crawler reconciles additions, edits, moves, and removals in selected roots. Files have independent searchable segments for names, text chunks, PDF pages or rendered pages, screenshots, images, audio windows, video frames, and code stretches. Code has a separate store and explicit `code:` query scope.

The searcher scores segment cosine similarities, normalizes by modality, keeps each file's best semantic segment, and combines that score with query-word coverage from FTS5. Exact identifier lookups favor words. Results preserve page, timestamp, or line evidence and distinguish weaker matches.

For an index with at most five vectors, a cosine of at least 0.75 can admit a semantic result without a two-sigma outlier. Such a tiny distribution cannot reliably establish that outlier. This is a bounded heuristic, measured on the three-file code fixture: relevant paraphrases scored 0.785 and 0.849, while three unrelated queries stayed below 0.695. Results retain the measured z-score and expose semantic admission separately rather than fabricating a z-score.

Current folder and exclusion policy applies before results and file actions. File text, OCR, and media are untrusted evidence. A result never authorizes a subsequent action by its contents.

## Options considered

| Option | Result |
| --- | --- |
| Put the corpus in `import:file` or the active memory table | Rejected: continuously synchronized files have different identity, retention, visibility, and embedding contracts. |
| Build new LanceDB file tables before reusing the available implementation | Rejected: duplicates an existing FNDR engine and delays real parity and evaluation. |
| Load EmbeddingGemma 2 in the main app's llama.cpp | Rejected: unsupported architecture in the linked version; also couples model residency and failures to capture. |
| Reuse the existing native Rust Files engine, separate SQLite indexes, and pinned model helper | Chosen: preserves the tested multimodal contract and isolates it from memory. |

The initial discovery draft proposed new LanceDB tables and deferred media. Discovery of the existing FNDR implementation and the explicit model choice superseded that draft.

## Evidence and verification

- Installed DigUp 0.6.2 was inspected with REA. Mach-O evidence ID: `ev_4dc48682fb90e4cd4ed02f3346b90c7c122747f234ad1ec2af81b56a5979f56b`.
- The installed schema and runtime confirm separate ordinary-file/code indexes, FTS5, segment evidence, and multimodal vector search. Source from DigUp's newer public checkout explains mechanisms but does not establish exact installed-version equivalence.
- The reused FNDR source is commit `a762a80`, including its MIT attribution in `THIRD_PARTY_NOTICES.md`. Its earlier measurements are historical; this integration is verified again.
- On 2026-11-17 the helper built in this checkout passed all ten real reference comparisons at cosine 1.00000, using the SHA-256-verified pinned text and media assets.
- Onboarding regression tests require consent to save successfully before downloading and require a failed download start to remain retryable.
- Native ranking and source visibility checks are recorded in the [implementation plan](../superpowers/plans/2026-11-17-native-file-finding.md).

## Consequences

The build ships two additional native helpers. The model helper has its own pinned llama.cpp build and reference tests. Any change that alters its vector output requires a vector-contract version bump and a file-only reindex. FNDR's screen-memory embedding model and storage contract stay independent.

The model helper currently targets macOS 14, while the app and reader target 13. Files compatibility on macOS 13 and Intel remains unverified. Helper startup and request/reply operations have a 120-second deadline, including blocked pipe writes; a timeout kills and reaps the child. Query failures fall back to words and retry with a fresh helper next time.
