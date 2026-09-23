# Minh: Reopen exactly, and embeddings on every memory

Mission: every memory opens the exact page, document (and page), or file it came from, from every FNDR surface; and every memory gets its vectors and chunks automatically when it is stored, never "once in a while" or through a manual reindex.

Measurements from 2026-09-23 on the owner's profile: 3 of 29 memories reopen a specific page, 26 open only the app, 0 open a file; downloads are stored but cannot be opened; the chunk tables (`memories_v5_bge_1024`, `memory_chunks_v1_bge_1024`) have 0 rows because chunking only runs from `reindex_memories_v5`, which nothing in the app calls.

How to work this lane: test first, then fix, then rerun the matrix. The two matrices (RE-01 and EM-11) are the "done" definition for the lane; every fix ticket names the matrix rows it turns green.

## RE-01 Build and run the reopen QA matrix on today's build
- assignee: minhpro001
- labels: area::reopen, type::qa, prio::p0
- milestone: W02-Measure
- estimate: 5h
- depends: none

**Why.** We need to know exactly which cases fail today, so every fix is aimed and provable.

**Do.**
1. Create `docs/product/reopen-qa-matrix.md` with the table below plus columns: Stored target (what the Vault row shows), Reopen result (exact, app only, wrong, error), Build, Date, Notes.
2. Run every row on today's build on your Mac with a fresh test profile (`FNDR_DATA_DIR="$HOME/Library/Application Support/com.fndr.app.reopenqa" npm run tauri dev`). Use public or synthetic pages and files only.
3. For each failure, note which code path produced the stored target (`memory/reopen.rs`, `capture/mod.rs` call sites, `downloads.rs`).

| Row | Scenario | Expected target | Expected reopen |
|---|---|---|---|
| R01 | Chrome tab, public article | https URL | Same page |
| R02 | Safari tab | https URL | Same page |
| R03 | Arc tab | https URL | Same page |
| R04 | Edge or Brave tab | https URL | Same page |
| R05 | Firefox tab | URL, or app only if Firefox exposes no URL (record which) | Same page or honest "app only" |
| R06 | Chrome incognito or Safari private window | Nothing stored | Not applicable |
| R07 | Two Chrome windows, switch between them | URL of the focused tab each time | Correct tab |
| R08 | Single-page app navigation (Gmail thread, Notion page) | URL after navigation, not the previous one | The later page |
| R09 | URL with `?token=` or `#access_token=` | URL with the secret stripped | Page opens (or login) |
| R10 | Login-walled page (Canvas course page) | https URL | Login, then page |
| R11 | PDF open in Chrome | URL plus page if known | PDF at that page (`#page=N`) |
| R12 | Google Docs or Sheets | Docs URL | Same doc |
| R13 | URL over 2,000 characters | Stored without breaking the row | Opens |
| R14 | `chrome://settings`, `javascript:`, `data:` on screen | Never a reopen target | Never opened |
| R15 | PDF in Preview on page 112 | File path plus page 112 | File opens (page if supported) |
| R16 | Pages, Keynote, or Numbers document | File path | File opens |
| R17 | Word, Excel, or PowerPoint document | File path | File opens |
| R18 | Unsaved TextEdit document | App only, labeled unsaved | App opens, honest label |
| R19 | VS Code file | File path (and `vscode://file/...:line` if derivable) | File opens in VS Code |
| R20 | Finder window | Folder path | Folder revealed |
| R21 | File moved after capture | Found again by name | Opens from new location, UI says moved |
| R22 | File deleted after capture | Stored path | Clear "no longer exists," memory still readable |
| R23 | File on an unmounted external drive | Stored path | Clear "drive not connected" |
| R24 | iCloud file evicted from the Mac | Stored path | Opens and downloads, or clear message |
| R25 | Path with spaces, accents, emoji | Stored exactly | Opens |
| R26 | Slack channel | App, or deep link if derivable | App or channel |
| R27 | Notion desktop page | Notion URL or `notion://` | Same page |
| R28 | Zoom call | App only | App, UI says no specific target |
| R29 | App uninstalled after capture | Stored bundle id | Clear error |
| R30 | Chrome download of a PDF | File path, source URL, link to the page memory | PDF opens |
| R31 | Safari download that auto-unzips | Record what lands in Downloads | The extracted item or the archive, documented |
| R32 | Download still in progress (`.crdownload`, `.download`, `.part`) | No memory until complete; exactly one after | Opens once complete |
| R33 | Second download with the same name (`report (1).pdf`) | Separate memory, its own path | Correct file |
| R34 | Downloaded file renamed in Finder | Found again | Opens the renamed file |
| R35 | `.dmg`, `.pkg`, `.app`, or `.command` download | File path | Revealed in Finder, never opened or run |
| R36 | Memory merged from frames with different targets | The most specific, newest target | That target |
| R37 | Memory captured before this change | Backfilled target or honest "app only" | As stored |
| R38 | Deleted memory | Nothing | Not reachable |
| R39 | Blocklisted site | Nothing stored | Not applicable |
| R40 | Document on the second display | Same as single display | Same |
| R41 | Reopen from Search, Vault, Resume, Quick Find, and MCP | One target | Identical everywhere |

**Done when.** Every row has a result for today's build.

**Evidence.** The filled matrix and a count: exact, app only, wrong, error.

## RE-02 Unit-test the reopen rules before changing them
- assignee: minhpro001
- labels: area::reopen, type::qa, prio::p0
- milestone: W02-Measure
- estimate: 3h
- depends: RE-01

**Today.** `src-tauri/src/memory/reopen.rs` (`build_reopen_target`, line 83) chooses URL, then file, then app. `src-tauri/src/ipc/commands/memory.rs` (`reopen_memory` line 77, `resolve_reopen_target` line 105) resolves and opens.

**Do.**
1. Table-driven tests for `build_reopen_target`: http and https accepted; `javascript:`, `data:`, `file:` from a browser, `chrome:` rejected; file path chosen over app; empty inputs give `Unknown`.
2. Tests for `resolve_reopen_target` with each `ReopenKind` and with missing fields.
3. Mark tests that encode today's wrong behavior with the matrix row they will flip.

**Done when.** `cd src-tauri && cargo test reopen` passes with at least 15 cases.

**Evidence.** Test output.

## RE-03 Store the document's file path for native document apps
- assignee: minhpro001
- labels: area::reopen, type::feature, prio::p0
- milestone: W02-Measure
- estimate: 4h
- depends: RE-02

**Why.** Preview, Pages, Keynote, Word, TextEdit, and editors tell macOS which file is open; FNDR ignores it during capture.

**Today.** `capture/macos.rs` `read_frontmost_app_info` (line 184) reads `document_url` from `accessibility::focused_window_snapshot` but keeps it only for browsers (`normalize_browser_document_url` accepts only http and https). The file path reaches a memory only if the local model lists one in `files_touched`.

**Do.**
1. Add `document_path: Option<String>` to `FrontmostAppContext`, set from a `file://` `document_url` for non-browser apps (percent-decoded).
2. Pass it as `first_file_path` to `build_reopen_target` at every capture call site (search `build_reopen_target(` in `capture/mod.rs`), ahead of `files_touched`.
3. Tests for decoding (`%20`, unicode) and for the browser case staying unchanged.

**Done when.** Matrix rows R15 to R17, R19, R25 pass.

**Evidence.** Matrix rows and test output.

## RE-04 Remember the PDF page
- assignee: minhpro001
- labels: area::reopen, type::feature, prio::p1
- milestone: W03-Build
- estimate: 3h
- depends: RE-03

**Do.**
1. Parse "(page N of M)" and localized variants you can confirm from Preview window titles; store `reopen_page`.
2. Show "page 112" on the Vault row.
3. For PDFs opened in a browser, append `#page=N` when reopening. For Preview, open the file and state the page in the UI if Preview cannot be sent to a page.

**Done when.** R11 and R15 pass (page shown; browser PDFs open on the page).

**Evidence.** Matrix rows and a screenshot.

## RE-05 Reopen web pages at the passage, not just the page
- assignee: minhpro001
- labels: area::reopen, type::feature, prio::p1
- milestone: W03-Build
- estimate: 4h
- depends: RE-02

**Why.** "Where I left off" on a long article means the paragraph, not the top of the page.

**Do.**
1. At capture, pick one distinctive visible sentence (8 to 12 words, from the stored text) and store it as `reopen_text_anchor`.
2. On reopen of an http or https target, append `#:~:text=<percent-encoded anchor>` unless the URL already has a fragment. Skip for domains known not to support it (Google Docs).
3. Tests: encoding of commas, dashes, and quotes; the anchor never contains text from a blocklisted or sensitive context (it is taken only from already-stored text).

**Done when.** R01 to R03 open scrolled to the passage in Chrome, Arc, and Safari.

**Evidence.** A short recording.

## RE-06 When memories merge, keep the most specific target
- assignee: minhpro001
- labels: area::reopen, type::bug, prio::p0
- milestone: W02-Measure
- estimate: 2h
- depends: RE-02

**Today.** `capture/mod.rs` around line 4747 merges `reopen_file_path` as `incoming.or(existing)` field by field, so kind and fields can disagree.

**Do.**
1. Rank targets: file with page, file, URL with anchor, URL, deep link, app, unknown; on merge keep the higher rank, and the newer one on a tie.
2. Merge the whole target, not individual fields.
3. Tests for each pair.

**Done when.** R36 passes; tests pass.

**Evidence.** Test output.

## RE-07 Check the target before opening and say what happened
- assignee: minhpro001
- labels: area::reopen, type::feature, prio::p0
- milestone: W03-Build
- estimate: 5h
- depends: RE-03

**Do.**
1. `reopen_memory` returns a typed result: `Opened`, `OpenedMoved { new_path }`, `Missing`, `DriveNotConnected`, `AppMissing`, `AppOnly`, `Blocked`.
2. For a missing file, look it up with Spotlight by file name (`mdfind -name`), prefer the same extension and closest size; open it and report `OpenedMoved`.
3. The UI shows a one-line result for each case.
4. Tests with temporary files for moved and missing.

**Done when.** R21 to R24 and R29 pass.

**Evidence.** Matrix rows.

## RE-08 Make downloads openable and connected to where they came from
- assignee: minhpro001
- labels: area::reopen, type::feature, prio::p0
- milestone: W02-Measure
- estimate: 5h
- depends: RE-02

**Today.** `src-tauri/src/downloads.rs` `inject_download_memory` (line 140) stores text "File downloaded locally ... <path>" with no reopen target, no source URL, and no link to the page.

**Do.**
1. Set `reopen_kind = FilePath` and `reopen_file_path` on download memories.
2. Read the source URL from the `com.apple.metadata:kMDItemWhereFroms` extended attribute; store it (credential-stripped like capture URLs).
3. Link to the browser memory captured within two minutes before the download (`related_memory_ids`).
4. Ignore partial files (`.crdownload`, `.download`, `.part`) and create exactly one memory when the final file appears; handle `name (1).ext`.
5. Tests with temporary files and a fake xattr reader.

**Done when.** R30, R32, R33 pass.

**Evidence.** Matrix rows.

## RE-09 Never execute a download from FNDR
- assignee: minhpro001
- labels: area::reopen, type::bug, prio::p0
- milestone: W02-Measure
- estimate: 2h
- depends: RE-08

**Why.** Reopening a `.dmg`, `.pkg`, `.app`, or script from a memory must not install or run anything.

**Do.**
1. For executable and installer types (extension list plus the quarantine attribute), reopen means "reveal in Finder" only.
2. Same rule for `fndr.open_target` over MCP.
3. Tests for each extension.

**Done when.** R35 passes.

**Evidence.** Test output.

## RE-10 Deep links for apps that support them
- assignee: minhpro001
- labels: area::reopen, type::feature, prio::p2
- milestone: W04-Prove
- estimate: 3h
- depends: RE-03

**Do.**
1. Table-driven mapping: VS Code (`vscode://file/<path>:<line>` when the title shows a line), Notion (keep the https URL), Figma (https URL), Slack (only when a workspace and channel id are visible in the URL).
2. Tests per mapping; unsupported apps stay app only.

**Done when.** R19, R26, R27 pass or are documented as app only.

**Evidence.** Matrix rows.

## RE-11 Backfill reopen targets for memories captured before these fixes
- assignee: minhpro001
- labels: area::reopen, type::chore, prio::p1
- milestone: W03-Build
- estimate: 3h
- depends: RE-03, RE-06

**Do.**
1. A resumable maintenance job that upgrades targets for existing memories from stored `url`, window title, and `files_touched`.
2. Run on idle; count upgraded memories.

**Done when.** R37 passes; vault health shows the new reopen share.

**Evidence.** Vault health before and after.

## RE-12 One reopen path from every surface
- assignee: minhpro001
- labels: area::reopen, type::feature, prio::p0
- milestone: W03-Build
- estimate: 3h
- depends: RE-07

**Do.**
1. Search results, Vault rows, Resume cards, Quick Find, and MCP `fndr.open_target` all call `reopen_memory` and show its typed result.
2. MCP reopen requires the same approval as other actions (coordinate with Kunj's risk policy ticket GS-11).
3. Test: the same memory from IPC and MCP resolves to the same target.

**Done when.** R41 passes.

**Evidence.** Matrix row and test output.

## RE-13 Count reopen outcomes
- assignee: minhpro001
- labels: area::reopen, type::feature, prio::p1
- milestone: W03-Build
- estimate: 2h
- depends: RE-07

**Do.**
1. Runtime counters per typed result and per target kind.
2. Add "reopen specific share" and outcome counts to `scripts/audit/vault_health.py` (stored share) and Engine diagnostics (session outcomes).

**Done when.** Both views show the numbers.

**Evidence.** Screenshot and report.

## RE-14 Rerun the reopen matrix and a live day
- assignee: minhpro001
- labels: area::reopen, type::qa, prio::p0
- milestone: W04-Prove
- estimate: 4h
- depends: RE-03, RE-06, RE-07, RE-08, RE-09, RE-12

**Do.**
1. Rerun every matrix row on the freeze candidate build.
2. Use FNDR for a full working day; run `make vault-health`.

**Done when.** At least 36 of 41 rows exact or honestly labeled, zero executions from R35, and 90% of the live day's memories reopen to a page or file.

**Evidence.** The matrix and vault health.

## EM-01 Map every way a memory is written and the vectors each writes
- assignee: minhpro001
- labels: area::embeddings, type::spike, prio::p0
- milestone: W02-Measure
- estimate: 4h
- depends: none

**Why.** "Embeddings on every memory" is only checkable once we list every writer.

**Do.**
1. List every constructor and writer of `MemoryRecord`: normal capture, low-OCR visual branch, URL-only surface capture (`capture/mod.rs` around 2197), downloads (`downloads.rs`), merge (`merge_or_append_memory_record`), memory review (`memory_review/pipeline.rs`, which says chunks are not touched), import, meetings, the demo seeder, and any agent writer.
2. For each: which of `embedding`, `snippet_embedding`, `support_embedding`, `image_embedding`, and chunk rows it writes, from which text, and whether it goes through `compose_memory_embedding_document`.
3. Write `docs/product/embedding-coverage.md` with the table and the gaps.

**Done when.** The doc lists every writer with file and line.

**Evidence.** The doc.

## EM-02 Report coverage by source in vault health
- assignee: minhpro001
- labels: area::embeddings, type::feature, prio::p0
- milestone: W02-Measure
- estimate: 2h
- depends: EM-01

**Do.**
1. In `scripts/audit/vault_health.py`, per `source_type` and `summary_source`: count, share with non-zero primary vector, share with at least one chunk row (join on memory id), stale chunk share (chunk text hash differs from the memory's text hash, once EM-04 stores it).
2. Extend `scripts/audit/test_vault_health.py`.

**Done when.** The report shows coverage per source; tests pass.

**Evidence.** Report on the owner's profile.

## EM-03 Chunk and embed every new memory automatically
- assignee: minhpro001
- labels: area::embeddings, type::feature, prio::p0
- milestone: W03-Build
- estimate: 8h
- depends: EM-01

**Why.** Retrieval over chunks (Anurup's VS-18) needs chunk rows for every memory, written as memories are stored.

**Today.** Only `reindex_memories_v5` (`ipc/commands/maintenance.rs:153`) writes chunks, with batch helpers `embed_v5_text_queue` (line 358) and `build_v5_reindex_batch_with` (line 462). Nothing in the app calls it.

**Do.**
1. After `add_batch_and_get_count` succeeds in capture, enqueue the new memory ids to a chunk queue (like `pending_memory_reviews`).
2. A background worker drains the queue in batches of 8 memories using the existing v5 helpers, respecting memory pressure and battery (defer, never drop).
3. Chunk from the stored cleaned text (Anurup's VS-16 later makes this the full screen text; no change needed here when it lands), about 300 tokens with 50 overlap, using the embedding model chosen in ADR-019 (BGE until then).
4. Tests: a stored memory produces chunk rows; the queue survives a pressure deferral; idempotent on re-enqueue (reuse `v5_reindex_identity`).

**Done when.** A two-hour live session shows chunk rows for 100% of new memories within 5 minutes of capture.

**Evidence.** Vault health coverage rows.

## EM-04 Re-chunk when a memory's text changes
- assignee: minhpro001
- labels: area::embeddings, type::bug, prio::p0
- milestone: W03-Build
- estimate: 4h
- depends: EM-03

**Today.** Merge rewrites text; memory review uses `replace_memory_preserving_chunks` (`memory_review/pipeline.rs`), so chunks go stale.

**Do.**
1. Store a hash of the chunked text per memory.
2. On merge or review, compare hashes and re-enqueue the memory when they differ.
3. Tests for merge and review.

**Done when.** No stale chunks in the EM-02 report after a live day.

**Evidence.** Report row.

## EM-05 Backfill missing vectors and chunks without a manual step
- assignee: minhpro001
- labels: area::embeddings, type::feature, prio::p0
- milestone: W03-Build
- estimate: 5h
- depends: EM-03

**Do.**
1. On launch and when idle and on power, find memories with no chunk rows or a zero or missing primary vector, and enqueue them.
2. Resumable across restarts; rate-limited so capture is never slowed.
3. Settings shows "Search index: 97% ready (40 left)" while it runs.
4. Retire any user-facing need to run `reindex_memories_v5`.

**Done when.** On the owner's profile, coverage reaches 100% within one idle hour.

**Evidence.** Vault health before and after.

## EM-06 Never store a memory with a zero or missing vector
- assignee: minhpro001
- labels: area::embeddings, type::bug, prio::p0
- milestone: W02-Measure
- estimate: 3h
- depends: EM-01

**Today.** Capture pauses when the embedder is missing (the FR6 gate), but `downloads.rs` writes zero vectors when embedding fails or the embedder is absent.

**Do.**
1. Any writer that cannot embed stores the memory as `pending_embedding` and enqueues it for EM-05 instead of writing zeros.
2. Search excludes pending memories until embedded (keyword search may still find them).
3. Tests with an embedder that fails once.

**Done when.** Zero vectors are 0% in vault health after a session with a forced embedder failure.

**Evidence.** Test output and report.

## EM-07 Send downloads and URL-only captures through the standard composer
- assignee: minhpro001
- labels: area::embeddings, type::chore, prio::p1
- milestone: W03-Build
- estimate: 3h
- depends: EM-01, RE-08

**Do.**
1. Replace the ad hoc vector building in `downloads.rs` with `compose_memory_embedding_document` and the shared embed path.
2. Same for the URL-only surface capture record.
3. Tests that both produce the same document shape as normal capture.

**Done when.** EM-01's table shows every writer on the standard composer.

**Evidence.** Updated coverage doc.

## EM-08 Index what is inside downloaded files
- assignee: minhpro001
- labels: area::embeddings, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 6h
- depends: EM-03, EM-07

**Why.** People remember a download by what was in it, not its file name.

**Do.**
1. Extract text for PDF (PDFKit through `objc2`) and for `.docx`, `.rtf`, `.txt`, `.html` (the built-in `textutil -convert txt`), capped at 50 pages or 100,000 characters.
2. Run through the same privacy checks as screen text (secret redaction; skip if the source URL is blocklisted).
3. Chunk and embed through EM-03.

**Done when.** A phrase that appears only inside a downloaded PDF finds it in Search.

**Evidence.** A recording.

## EM-09 Switch the embedding model with a versioned migration
- assignee: minhpro001
- labels: area::embeddings, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 6h
- depends: VS-17, EM-05

**Do.**
1. Implement ADR-019's model behind the existing model registry (pinned URL and SHA-256 like MiniLM in `inference/model_config.rs`).
2. New memories use the new model; EM-05's backfill re-embeds old ones into a versioned table; search uses the new table once coverage passes 95%, the old one before that.
3. Remove the old model download from onboarding when migration completes.

**Done when.** `make qa-retrieval` runs on the new model and matches ADR-019's numbers.

**Evidence.** Report.

## EM-10 Deleting a memory removes all of its vectors, everywhere
- assignee: minhpro001
- labels: area::embeddings, type::qa, prio::p0
- milestone: W02-Measure
- estimate: 2h
- depends: none

**Today.** `delete_memory_by_id` (`storage/lance_store/mod.rs:2256`) deletes the row and calls `delete_chunks_for_memory`.

**Do.**
1. Tests: delete by id, delete older than N days, delete all, and adding a site to the blocklist with "remove existing" each leave zero chunk rows and zero versioned-table rows for the affected memories.
2. Fix any path that misses.

**Done when.** Tests pass.

**Evidence.** Test output.

## EM-11 Build and run the embedding QA matrix
- assignee: minhpro001
- labels: area::embeddings, type::qa, prio::p0
- milestone: W04-Prove
- estimate: 4h
- depends: EM-03, EM-04, EM-05, EM-06

**Do.** Create `docs/product/embedding-qa-matrix.md` and run each row with a fresh test profile, checking vault health after each.

| Row | Scenario | Expected |
|---|---|---|
| E01 | Normal capture of a text-heavy page | Three non-zero vectors; chunk rows within 5 minutes |
| E02 | Low-text visual capture (an image-heavy page) | Vectors from title and visual text; chunks for any text |
| E03 | URL-only surface capture | Vectors from title and URL |
| E04 | Download of a PDF | Vectors and content chunks |
| E05 | Two frames merged into one memory | Chunks rebuilt once |
| E06 | Memory changed by memory review | Chunks rebuilt |
| E07 | Embedding model file removed at launch | Capture paused with a visible warning; nothing stored with zero vectors |
| E08 | Embedding fails once mid-batch | Memory pending, then embedded on retry |
| E09 | Quit FNDR during backfill | Backfill resumes on next launch |
| E10 | 1,000 memories needing backfill | Completes within the EM-12 budget; capture unaffected |
| E11 | On battery below 40%, or memory pressure high | Backfill defers; capture continues |
| E12 | Delete one memory, delete all | No orphan chunk rows |
| E13 | Model switch (EM-09) mid-use | Search keeps working throughout |

**Done when.** All rows pass on the freeze candidate.

**Evidence.** The matrix.

## EM-12 Keep chunking within the always-on budget
- assignee: minhpro001
- labels: area::embeddings, type::qa, prio::p1
- milestone: W04-Prove
- estimate: 3h
- depends: EM-03

**Do.**
1. With `FNDR_METRICS_DUMP`, measure CPU, memory, and milliseconds per chunk during a two-hour session and during a 1,000-memory backfill on the M1 8 GB.
2. Tune batch size and worker interval until average CPU stays at 3% or lower during normal capture.

**Done when.** Numbers recorded and within budget, or the MR states the gap.

**Evidence.** The metrics summary.
