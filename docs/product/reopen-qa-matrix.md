# Reopen QA matrix (RE-01)

Baseline of which memories reopen to the exact page, file, or app on today's
build. Later RE tickets name the rows they turn green. Do not copy memory
text, credentials, or personal paths into this file.

| Field | Value |
|---|---|
| Date | 2026-09-28 |
| Build | `123cd75` `tauri dev` debug |
| Profile | `$HOME/Library/Application Support/com.fndr.app.reopenqa` |
| Machine | Chrome, Safari, Brave; Preview, TextEdit, Finder; Downloads watcher on `~/Downloads` |
| Permissions | Screen Recording worked (OCR ran). Window titles fell back to bundle ids, so Accessibility `AXDocument` / `AXTitle` for this **dev** binary did not yield http URLs. |

## Counts

14 memories in the isolated profile at end of run (7 capture + 7 download-tracker). Zero stored `reopen_url`. Zero absolute `reopen_file_path`.

| Outcome | Count |
|---|---|
| exact | 0 |
| app only | 14 |
| wrong | 6 |
| error | 3 |
| n/a | 6 |
| not available | 12 |
| **total rows** | **41** |

## How we inspected

1. Isolated profile: `FNDR_DATA_DIR="$HOME/Library/Application Support/com.fndr.app.reopenqa" npm run tauri dev` (log: `FNDR_DATA_DIR override active`).
2. After each stimulus, wait ~80s for OCR + local LLM + flush.
3. **Stored target:** Lance columns `reopen_kind`, `reopen_url`, `reopen_file_path`, `reopen_app_bundle_id` from `lancedb/memories_v4_minilm_384` (no memory body in this doc). Vault **Open source** is shown only when `parse_reopen_target` finds a string; `app_bundle` becomes `app-bundle://…`.
4. **Reopen result:** inferred from stored kind plus [`open_reopen_target`](../../src-tauri/src/ipc/commands/memory.rs) (`open -b` for app bundle; `open` for file; error if path missing).
5. Public or synthetic pages and files only.

## RE-03 follow-up (2026-10-01)

Code on `a569e48` plus this change: native `file://` `AXDocument` is percent-decoded into `FrontmostAppContext.document_path` and passed to `build_reopen_target` ahead of `files_touched`. Browser `file://` stays dropped.

Unit tests: `cd src-tauri && cargo test --lib native_document` and `cargo test --lib reopen` pass (decode `%20` / unicode / `file://localhost`, reject non-file schemes, prefer AX path over LLM junk).

Two causes kept the window title and `AXDocument` empty in RE-01 and in the first RE-03 check:

1. Accessibility was not granted. `tauri dev` started from Cursor's terminal inherits Cursor as the responsible app; `AXIsProcessTrusted()` was false until Cursor was added under System Settings → Privacy & Security → Accessibility.
2. Even with Accessibility granted, the system-wide `AXFocusedApplication` lookup in `accessibility::focused_window_snapshot` failed with `-25204` (`kAXErrorCannotComplete`). Asking the frontmost app directly by PID (`AXUIElementCreateApplication`) returned the title and document. `focused_window_snapshot` now queries by PID when the caller passes one, which `read_frontmost_app_info` always does.

Live re-check (2026-10-01, Accessibility granted, PID fix in): opened one synthetic fixture per row, read `get_frontmost_app_info` the way capture does, then ran `open` on the decoded path.

| Row | Fixture | Window title (verified) | `document_path` | Path exists, `open` works |
|---|---|---|---|---|
| R15 | `re03-preview.pdf` in Preview | `re03-preview.pdf – 1 page` | `…/re03-fixtures/re03-preview.pdf` | yes |
| R25 | `café 📁/report (1) ✨.pdf` in Preview | `report (1) ✨.pdf – 1 page` | `…/re03-fixtures/café 📁/report (1) ✨.pdf` (decomposed `e` + U+0301, as macOS reports it) | yes |
| R16 | `re03-pages.pages` (saved by Pages) | `re03-pages.pages` | `…/re03-fixtures/re03-pages.pages` | yes |
| R17 | `re03-sheet.xlsx` in Excel | `re03-sheet` | `…/re03-fixtures/re03-sheet.xlsx` | yes |
| R19 | `re03-notes.txt` in VS Code | `re03-notes.txt` | none: VS Code exposes no `AXDocument` | n/a |

Full capture run (2026-10-01, 17:54–18:04): `FNDR_DATA_DIR=…/com.fndr.app.reopenqa npm run tauri dev`, each fixture held frontmost ~95 s, then read `memories_v4_minilm_384` and ran `open` on the stored `reopen_file_path`.

| Row | App | Stored `reopen_kind` | Stored `reopen_file_path` | Exists, `open` works |
|---|---|---|---|---|
| R15 | Preview | `file_path` | `…/re03-fixtures/re03-preview.pdf` | yes |
| R16 | Pages | `file_path` | `…/re03-fixtures/re03-pages.pages` | yes |
| R17 | Microsoft Excel | `file_path` | `…/re03-fixtures/re03-sheet-rich.xlsx` | yes |
| R25 | Preview | `file_path` | `…/re03-fixtures/café 📁/report (2) ✨.pdf` | yes |

The first Excel fixture (one cell) and a second blank PDF were not stored: capture skipped them as near-empty or duplicate frames. Text-rich fixtures (`re03-sheet-rich.xlsx`, `report (2) ✨.pdf`) were stored. Page 112 remains RE-04. VS Code is RE-10 (title has only the file name, no folder).

## RE-04 follow-up (2026-10-02)

Capture now stores `reopen_page` (nullable Int64 Lance column). Detection:

- Preview / native file: window title `WORD N CONNECTOR M` after a dash, or `(page N of M)`.
- Browser PDF: keep `#page=N` on the URL; otherwise the first OCR line that is just `N / M` or `N of M`, only when the URL path ends in `.pdf`.
- Reopen: browser URLs get `#page=N`. Preview files open as the file; the Vault row and **Open source (page N)** button state the page because Preview has no supported page-jump API.

Live Preview titles on a 150-page synthetic PDF at page 112 (Accessibility / System Events window name):

| Locale | Window title |
|---|---|
| en | `re04-preview-150.pdf – Page 112 of 150` |
| fr | `re04-preview-150.pdf – Page 112 sur 150` |
| de | `re04-preview-150.pdf – Seite 112 von 150` |
| es | `re04-preview-150.pdf – Página 112 de 150` |

A one-page Preview title (`x.pdf – 1 page`) still has no current page. Screenshot: [re04-preview-page-112.png](re04-preview-page-112.png).

Unit tests: `cd src-tauri && cargo test --lib reopen` and `cargo test --lib memory_cards`; Vault `npm test -- MemoryCard`. Browser `#page=N` reopen is covered by `resolve_reopen_target_appends_pdf_page_to_browser_url`. Live Chrome PDF capture was not re-run in this session (still depends on storing the http URL, which RE-01 recorded as app-only).

## Matrix

| Row | Scenario | Expected target | Expected reopen | Stored target | Reopen result | Build | Date | Notes |
|---|---|---|---|---|---|---|---|---|
| R01 | Chrome tab, public article | https URL | Same page | `app_bundle` `com.google.Chrome`; `reopen_url` empty; title is bundle id | app only | `123cd75` | 2026-09-28 | `macos.rs` `normalize_browser_document_url` never saw http `AXDocument`. `build_reopen_target` (`reopen.rs:83`) fell through to app. `capture/mod.rs` ~3180. |
| R02 | Safari tab | https URL | Same page | `app_bundle` `com.apple.Safari`; `reopen_url` empty | app only | `123cd75` | 2026-09-28 | Same path as R01. Safari AppleScript semantic content exists (`macos.rs` `get_browser_semantic_content`) but reopen still uses AX http URL, which was missing. |
| R03 | Arc tab | https URL | Same page | | not available | `123cd75` | 2026-09-28 | Arc not installed |
| R04 | Edge or Brave tab | https URL | Same page | `app_bundle` `com.brave.Browser`; `reopen_url` empty | app only | `123cd75` | 2026-09-28 | Brave. Same AX URL miss as R01. |
| R05 | Firefox tab | URL, or app only if Firefox exposes no URL | Same page or honest app only | | not available | `123cd75` | 2026-09-28 | Firefox not installed |
| R06 | Chrome incognito or Safari private window | Nothing stored | Not applicable | No Helium / incognito row | n/a | `123cd75` | 2026-09-28 | Count bump during this wait was a late Brave flush, not an incognito row. Matches `privacy/safety_gate.rs` skip on title containing `incognito`. |
| R07 | Two Chrome windows, switch between them | URL of the focused tab each time | Correct tab | One Chrome memory, `app_bundle`; later `files_touched`-like file `en.wikipedia.org/wiki/Nitrogen` | app only | `123cd75` | 2026-09-28 | Continuity merge kept one card. No per-window URL. `merge_or_append_memory_record`. |
| R08 | Single-page app navigation | URL after navigation, not the previous one | The later page | Same Chrome `app_bundle` card; file field later Wikipedia Nitrogen path | app only | `123cd75` | 2026-09-28 | Used Wikipedia article switch (public). Later snippet is Nitrogen, but kind stayed `app_bundle` with no `reopen_url`. |
| R09 | URL with `?token=` or `#access_token=` | URL with the secret stripped | Page opens (or login) | No `reopen_url`; Chrome `app_bundle` only | app only | `123cd75` | 2026-09-28 | `strip_url_credentials` (`capture/mod.rs`) never ran on a stored URL because AX URL was empty. Secret did not land in `reopen_url`. |
| R10 | Login-walled page (Canvas course page) | https URL | Login, then page | | not available | `123cd75` | 2026-09-28 | No Canvas / login-walled fixture on this run |
| R11 | PDF open in Chrome | URL plus page if known | PDF at that page (`#page=N`) | Typed path: `browser_url` + `reopen_page`; live RE-01 row was Chrome `app_bundle` with no URL | code: appends `#page=N` when URL+page stored; live capture still app-only until URL lands | RE-04 | 2026-10-02 | `url_with_pdf_page` + PDF-toolbar OCR (`N / M`) on `.pdf` URLs. Live Chrome not re-captured this session. |
| R12 | Google Docs or Sheets | Docs URL | Same doc | Merged into Chrome `app_bundle` | app only | `123cd75` | 2026-09-28 | Public docs about page; no `docs.google.com` URL stored. |
| R13 | URL over 2,000 characters | Stored without breaking the row | Opens | Profile still readable (14 rows); Chrome `app_bundle`, URL not stored | app only | `123cd75` | 2026-09-28 | Row did not break Lance. Did not store the long URL. |
| R14 | `chrome://settings`, `javascript:`, `data:` on screen | Never a reopen target | Never opened | No `reopen_url` of `chrome:` / `javascript:` / `data:` | n/a | `123cd75` | 2026-09-28 | `normalize_browser_document_url` only accepts http/https. Met “never a URL target.” Existing Chrome card stayed `app_bundle`. |
| R15 | PDF in Preview on page 112 | File path plus page 112 | File opens (page if supported) | `file_path` plus `reopen_page=112` from title `… – Page 112 of 150` | exact file; page shown in Vault; Preview opens the file (no page jump) | RE-04 | 2026-10-02 | Live title confirmed en/fr/de/es. UI: `page 112` on the source line; file button `Open source (page 112)`. Screenshot in RE-04 section. |
| R16 | Pages, Keynote, or Numbers document | File path | File opens | `file_path` `…/re03-fixtures/re03-pages.pages` | exact | `a569e48`+RE-03 | 2026-10-01 | Pages. Full `tauri dev` capture, see RE-03 section. |
| R17 | Word, Excel, or PowerPoint document | File path | File opens | `file_path` `…/re03-fixtures/re03-sheet-rich.xlsx` | exact | `a569e48`+RE-03 | 2026-10-01 | Excel (Word and PowerPoint not installed). Full `tauri dev` capture, see RE-03 section. |
| R18 | Unsaved TextEdit document | App only, labeled unsaved | App opens, honest label | `file_path` `re-01_reopen_qa_matrix_….plan.md` (relative, not the unsaved doc); no unsaved label | wrong | `123cd75` | 2026-09-28 | LLM `files_touched` beat an honest app-only/unsaved target. `build_reopen_target` then `open` would hit `canonicalize_relaxed` “no longer exists.” |
| R19 | VS Code file | File path (and `vscode://file/...:line` if derivable) | File opens in VS Code | Capture context: title `re03-notes.txt`, no `AXDocument` | app only | `a569e48`+RE-03 | 2026-10-01 | VS Code does not expose `AXDocument`; title has the file name only. Moved to RE-10 (not an RE-03 row). |
| R20 | Finder window | Folder path | Folder revealed | `file_path` same relative plan filename, not the folder | wrong | `123cd75` | 2026-09-28 | Snippet mentioned the fixtures folder; stored reopen path did not. |
| R21 | File moved after capture | Found again by name | Opens from new location, UI says moved | No durable absolute path to move | error | `123cd75` | 2026-09-28 | `reopen_memory` has no Spotlight lookup. Missing path → `File path no longer exists` (`memory.rs` `canonicalize_relaxed`). |
| R22 | File deleted after capture | Stored path | Clear "no longer exists," memory still readable | Relative junk path | error | `123cd75` | 2026-09-28 | IPC can error; UI `handleReopen` only `console.warn`. Memory row would remain. |
| R23 | File on an unmounted external drive | Stored path | Clear "drive not connected" | | not available | `123cd75` | 2026-09-28 | No external drive in this run |
| R24 | iCloud file evicted from the Mac | Stored path | Opens and downloads, or clear message | | not available | `123cd75` | 2026-09-28 | No iCloud eviction fixture |
| R25 | Path with spaces, accents, emoji | Stored exactly | Opens | `file_path` `…/re03-fixtures/café 📁/report (2) ✨.pdf` (decomposed accent, as macOS reports it) | exact | `a569e48`+RE-03 | 2026-10-01 | Preview. Stored path exists and opens. Full `tauri dev` capture, see RE-03 section. |
| R26 | Slack channel | App, or deep link if derivable | App or channel | | not available | `123cd75` | 2026-09-28 | Slack not in this run's target list |
| R27 | Notion desktop page | Notion URL or `notion://` | Same page | | not available | `123cd75` | 2026-09-28 | Notion desktop not in this run's target list |
| R28 | Zoom call | App only | App, UI says no specific target | | not available | `123cd75` | 2026-09-28 | Zoom not in this run's target list |
| R29 | App uninstalled after capture | Stored bundle id | Clear error | Not uninstalled | n/a | `123cd75` | 2026-09-28 | Did not uninstall an app. `open_app_bundle` would `open -b` and error if the bundle is gone; UI still has no typed `AppMissing`. |
| R30 | Chrome download of a PDF | File path, source URL, link to the page memory | PDF opens | `summary_source=tracker` Finder `app_bundle` `com.apple.finder`; snippet `Downloaded: fndr-re01-sample.pdf`; no `reopen_file_path`, no source URL, no related id | app only | `123cd75` | 2026-09-28 | `downloads.rs` `inject_download_memory` (~141) never sets `reopen_*`. Watcher used a copied PDF (same inject path as a Chrome download). |
| R31 | Safari download that auto-unzips | Record what lands in Downloads | The extracted item or the archive, documented | Tracker memory for `fndr-re01-safari-unzip.zip` (archive), `app_bundle` Finder | app only | `123cd75` | 2026-09-28 | Copied zip into Downloads; Safari auto-unzip not triggered. Watcher stored the archive, not an extracted item. |
| R32 | Download still in progress (`.crdownload`, `.download`, `.part`) | No memory until complete; exactly one after | Opens once complete | No `.crdownload` row; one tracker row after rename to `.pdf` | app only | `123cd75` | 2026-09-28 | `is_temp_file` in `downloads.rs` ignored the partial. Reopen still Finder app only. |
| R33 | Second download with the same name (`report (1).pdf`) | Separate memory, its own path | Correct file | Separate tracker rows for `fndr-re01-report.pdf` and `fndr-re01-report (1).pdf`; neither has a file reopen path | app only | `123cd75` | 2026-09-28 | Separate memories yes; reopen cannot pick the file. |
| R34 | Downloaded file renamed in Finder | Found again | Opens the renamed file | Extra tracker row `fndr-re01-report-renamed.pdf`; old name row remains | wrong | `123cd75` | 2026-09-28 | Watcher treated rename as a new file. No “found again” / `OpenedMoved`. |
| R35 | `.dmg`, `.pkg`, `.app`, or `.command` download | File path | Revealed in Finder, never opened or run | Tracker `Downloaded: fndr-re01-dummy.pkg`, `app_bundle` Finder | app only | `123cd75` | 2026-09-28 | Did not execute the pkg (`open -b com.apple.finder`). Did not reveal the file. `downloads.rs` has no installer special case. |
| R36 | Memory merged from frames with different targets | The most specific, newest target | That target | Chrome card: `reopen_kind=app_bundle` **and** `reopen_file_path=en.wikipedia.org/wiki/Nitrogen` | wrong | `123cd75` | 2026-09-28 | Field-wise merge `incoming.or(existing)` at `capture/mod.rs` ~4795. Kind and file disagree. `resolve_reopen_target` follows kind → opens Chrome, ignores the file field. |
| R37 | Memory captured before this change | Backfilled target or honest "app only" | As stored | | n/a | `123cd75` | 2026-09-28 | Fresh profile has no pre-change rows. Read path: `normalize_embed_migrate.rs` backfills `Unknown` from `url` / `files_touched` / bundle. |
| R38 | Deleted memory | Nothing | Not reachable | Not deleted in the UI | n/a | `123cd75` | 2026-09-28 | Not exercised live. `reopen_memory` errors `Memory not found` if the id is gone. |
| R39 | Blocklisted site | Nothing stored | Not applicable | Blocklist not changed in Settings | n/a | `123cd75` | 2026-09-28 | Default blocklist is apps (1Password, Keychain, System Settings). Skip path: `capture_context_skip_reason` + `Blocklist`. |
| R40 | Document on the second display | Same as single display | Same | | not available | `123cd75` | 2026-09-28 | No second display in this run |
| R41 | Reopen from Search, Vault, Resume, Quick Find, and MCP | One target | Identical everywhere | Same stored kind per memory | wrong | `123cd75` | 2026-09-28 | Only Vault expanded **Open source** calls `reopen_memory` (`MemoryCardsPanel.tsx`). Search Timeline only selects a card. Quick Find `omnibar_open_memory` focuses Vault. Home has no `resume_work` binding. MCP `fndr.open_target` returns JSON fields and does not open (`mcp/mod.rs` `run_fndr_namespace_open_target`). |

## Attribution cheat sheet

| Symptom | Code path |
|---|---|
| Browser memory is app only, no http URL | `capture/macos.rs` `read_frontmost_app_info` + `normalize_browser_document_url`; `build_reopen_target` |
| Native doc has no file path | Accessibility not granted to the launching app, or the app exposes no `AXDocument` (VS Code); then `files_touched[0]` |
| PDF page not stored | Preview title is total-only (`– 1 page`) or browser URL is not `.pdf`; page parsers live in `memory/reopen.rs` |
| Download cannot reopen the file | `downloads.rs` `inject_download_memory` leaves `reopen_*` default |
| Kind vs file disagree after merge | `capture/mod.rs` ~4795 field-wise `or` |
| Missing file is a thrown string, not typed UI | `ipc/commands/memory.rs` `canonicalize_relaxed` + Vault `console.warn` |
| Surfaces disagree | Vault vs Timeline vs omnibar vs MCP as in R41 |
