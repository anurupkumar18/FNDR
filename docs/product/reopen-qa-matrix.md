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

## RE-05 follow-up (2026-10-05)

Capture stores `reopen_text_anchor`: 8 to 12 words from the sentence under the middle of the browser viewport, and only when that sentence is already in the stored text. Reopen of an http(s) memory with no fragment appends `#:~:text=<percent-encoded anchor>`. A stored PDF page still wins. Google Docs and `.pdf` URLs are left unchanged.

Live check on the reopen QA profile, Accessibility granted:

| App | Page | Stored `reopen_kind` | Stored `reopen_url` | `reopen_text_anchor` |
|---|---|---|---|---|
| Chrome | Nitrogen, scrolled to the isotopes section | `browser_url` | `https://en.wikipedia.org/wiki/Nitrogen` | `of this, access to the primary coolant piping in a pressurised water` |
| Safari | Helium | `app_bundle` | empty | none (no URL to attach it to) |
| Arc | | | | not installed |

Opening that Chrome URL with `#:~:text=` scrolls and highlights the passage in Chrome and in Safari. Recording: [re05-chrome-text-fragment.mp4](re05-chrome-text-fragment.mp4). Stills: [re05-chrome-text-fragment.png](re05-chrome-text-fragment.png), [re05-safari-text-fragment.png](re05-safari-text-fragment.png). The Safari still is the fragment URL opened directly. FNDR's Safari memory is still app only, so FNDR cannot build that URL yet (RE-15).

Unit tests: `cd src-tauri && cargo test --lib reopen` and `cargo test --lib semantic`.

## RE-06 follow-up (2026-10-05)

A merge now keeps one whole reopen target instead of merging each field on its own, so the stored kind and its fields always come from the same capture. The higher rank wins. On a tie the newer `reopen_captured_at_ms` wins, and equal times go to the incoming record. The winner keeps only its own kind's fields plus app name and metadata, so a stray file path cannot ride along on an app target.

| Rank | Target |
|---|---|
| 7 | Absolute file path with a page |
| 6 | Absolute file path |
| 5 | http(s) URL with a passage or page |
| 4 | http(s) URL |
| 3 | App deep link |
| 2 | App bundle id |
| 1 | Relative file path |
| 0 | Unknown, or a kind missing its own field |

The R36 card (`app_bundle` with a stray `en.wikipedia.org/wiki/Nitrogen` file path) merged with a Chrome `browser_url` capture now stores `browser_url` and no file path. A Preview file at page 112 stays a file target when a newer app-only frame merges in. Two captures of the same URL keep the existing passage when the incoming one has none.

Unit tests: `cd src-tauri && cargo test --lib reopen` (44 passed) and `cargo test --lib merge` (18 passed). `make test`: typecheck, 353 frontend tests, and 818 Rust lib tests pass. The only failure is `capture_fixtures::ocr_plus_cleanup_stays_within_each_fixtures_cer_budget`, an OCR accuracy budget unrelated to reopen that also failed before this change.

## RE-07 follow-up (2026-10-05)

`reopen_memory` now returns a tagged `ReopenOutcome` instead of `bool` / a thrown string. Before opening, `plan_reopen` checks the stored target:

| Outcome | When |
|---|---|
| `opened` | http(s) URL, allowed deep link, or a file that is still at the stored path (including an iCloud placeholder) |
| `opened_moved` | Stored file is gone; Spotlight `mdfind -name` found an exact-name match (Trash skipped; longest shared folder prefix, then newest mtime) |
| `missing` | File gone, no usable Spotlight hit, or a relative path |
| `drive_not_connected` | Path is under `/Volumes/<name>` and that volume folder is gone |
| `app_only` | Bundle id is installed (`NSWorkspace`); FNDR still opens the app |
| `app_missing` | Bundle id is not installed; nothing is launched |
| `blocked` | `javascript:`, `data:`, `file:`, `about:`, `chrome:`, `edge:`, `brave:` |
| `no_target` | Nothing to resolve |

The Vault expanded card shows that one line under **Open source**. The stored path is not rewritten on `opened_moved`.

Live checks (synthetic files only; no memory text):

| Row | Stimulus | Result |
|---|---|---|
| R21 | Unique `fndr-re07-*.txt` on Desktop, then moved to Documents | Spotlight indexed the Desktop copy, then the Documents copy after the move. Exact-name pick would open the Documents file and report `opened_moved`. |
| R22 | Temp file deleted | Path does not exist; `plan_reopen` returns `missing` with no lookup success. |
| R23 | 5 MB APFS disk image volume `FndrRe07Vol`, file on it, then `hdiutil detach` | `/Volumes/FndrRe07Vol` gone; `drive_not_connected` with volume `FndrRe07Vol`. |
| R24 | New file in iCloud Drive, `brctl evict` | brctl refused (`NSFileProviderErrorDomain -2008`, file cannot be evicted). File still existed on disk, so reopen is `opened` (same as a placeholder: `exists()` is true and `open` can materialize). |
| R29 | `NSWorkspace` lookup | `com.apple.Preview` installed; `com.fndr.re07.missingapp` not installed → `app_missing`. |

Unit tests: `cd src-tauri && cargo test --lib reopen`; frontend `npx vitest run src/shared/reopenOutcome.test.ts` and `ExpandedMemoryCard.test.tsx`.

## RE-08 follow-up (2026-10-07)

Download memories now store a file reopen target, a credential-stripped source URL from `kMDItemWhereFroms`, and a link to the browser memory from the two minutes before the file settled. The watcher no longer uses a 10 s debounce. A `DownloadSettler` waits until the size is stable for 3 s, no `.crdownload` / `.part` / `.download` sibling remains, and `flush_interval_secs + 15 s` has passed so the page memory is likely flushed. Partial names and zero-byte placeholders never produce a row. `report.pdf` and `report (1).pdf` are separate paths, so each gets its own memory.

Source URL rule: use `kMDItemWhereFroms[1]` (referring page) when it is http(s); otherwise use `[0]` with its query and fragment dropped, then `strip_url_credentials`. Copied files with no WhereFroms stay a file target with no page link.

`normalize_record_for_index` keeps `FilePath` even when `url` is set, so reopen opens the file rather than the page.

Live check (2026-10-07, reopen QA profile, `tauri dev`, Chrome): a synthetic local page (`http://127.0.0.1:8765/notes.html`) auto-downloaded a ~320 KB PDF twice, streamed over ~12 s each time. Results:

- Chrome wrote `Unconfirmed NNNNNN.crdownload` (not `<name>.crdownload`) during the transfer, then renamed it. No row was created for the temp name.
- Each final file was ingested once, 45 s after it settled: `fndr-re08-live.pdf` and `fndr-re08-live (1).pdf` got separate `tracker` rows with `reopen_kind = file_path`.
- Both rows store `url` = the referring page (not the PDF URL) and `related_memory_ids` = the Chrome page memory, which was captured 74 s before the first ingest.
- The Open file / Open source buttons were not clicked in the UI; `plan_reopen` covers that path in unit tests.

Unit tests: `cd src-tauri && cargo test --lib downloads` (18 passed) and `cargo test --lib reopen` / `cargo test --lib plan_reopen`. `make test`: typecheck, 355 frontend tests, and 854 Rust lib tests pass. The only failure is `capture_fixtures::ocr_plus_cleanup_stays_within_each_fixtures_cer_budget`, an OCR accuracy budget unrelated to reopen that also failed before this change.

R31 (Safari auto-unzip) and R34 (rename in Finder) are unchanged and out of scope.

## RE-09 follow-up (2026-10-07)

Landed on the same branch as RE-08 so a `.pkg` / `.dmg` / `.app` / `.command` download is never passed to `open` without `-R`. `should_reveal_in_finder` is true for those extensions (any case) and for a quarantined unix-executable with no installer extension. A quarantined PDF still opens. `open_path_with_system` uses `open -R`. MCP `fndr.open_target` still does not launch; it now returns `reveal_only` so the same rule is visible there until RE-12 routes through `reopen_memory`.

Unit tests: `should_reveal_installer_and_script_extensions_r35`, `should_reveal_quarantined_executable_but_not_quarantined_pdf`, `plan_reopen_existing_pkg_stays_opened_and_is_reveal_only_r35`.

## RE-10 follow-up (2026-10-08)

Capture now builds an `AppDeepLink` for VS Code, VS Code Insiders, and Cursor when an absolute path is available, and stores it ahead of `files_touched` so a relative LLM path cannot win. Web Notion, Figma, and Slack https URLs stay `BrowserUrl` (higher rank). No desktop Notion or Slack deep-link parser was added: the live probe found no URL or team/channel ids.

Path sources, in order:

1. Window title, when it already contains an absolute path (for example `window.title` includes `${activeEditorLong}`). A leading dirty marker (`●` / `•`) is stripped. A trailing `:line` or `:line:column` keeps the line.
2. Otherwise Accessibility `document_path` (decoded `file://` `AXDocument`), when it is absolute.

VS Code is not installed on this Mac. R19 was run with Cursor (`com.todesktop.230313mzl4w4u92`, scheme `cursor`). VS Code would use the same title rule with scheme `vscode`.

Live Accessibility probe (2026-10-08):

| App | Window title | `AXDocument` / document path | Deep link |
|---|---|---|---|
| Cursor (default title) | `re10-notes.txt — fndr` or `….plan.md — fndr` | `file:///private/tmp/re10-fixtures/re10-notes.txt` (and plans path) | `cursor://file/private/tmp/re10-fixtures/re10-notes.txt` from document path |
| Cursor with workspace `window.title` = `${dirty}${activeEditorLong}${separator}${rootName}` | Still short in this probe (`re10-notes.txt — fndr`) | Same `file://` when an editor is focused | Same; title did not expose the long path in this run |
| Notion desktop | `new grad job application` | none | none → app only |
| Slack desktop | `Slack` | none | none → app only |

`open cursor://file/private/tmp/re10-fixtures/re10-notes.txt` launched Cursor on the fixture. Unit tests: `app_deep_link_for_maps_editor_titles_and_document_paths_r19`, `build_reopen_target_orders_url_then_deep_link_then_file_then_app`, `resolve_and_plan_vscode_deep_link_opens_r19`. `cd src-tauri && cargo test --lib reopen` and `cargo test --lib plan_reopen` pass.

## Matrix

| Row | Scenario | Expected target | Expected reopen | Stored target | Reopen result | Build | Date | Notes |
|---|---|---|---|---|---|---|---|---|
| R01 | Chrome tab, public article | https URL | Same page | `browser_url` `https://en.wikipedia.org/wiki/Nitrogen` plus `reopen_text_anchor` (12 words from the isotopes section) | exact: Chrome scrolls to and highlights the passage | RE-05 | 2026-10-05 | AX http URL is stored. Reopen appends `#:~:text=`. See RE-05 section. |
| R02 | Safari tab | https URL | Same page | `app_bundle`; title `Helium - Wikipedia`; `reopen_url` empty | blocked: no URL stored, so no passage | RE-05 | 2026-10-05 | Safari still has no http `AXDocument`. Fragment works when the URL is opened directly (still in RE-05 section). Follow-up: RE-15. |
| R03 | Arc tab | https URL | Same page | | not available | RE-05 | 2026-10-05 | Arc not installed |
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
| R19 | VS Code file | File path (and `vscode://file/...:line` if derivable) | File opens in VS Code | Cursor substitute: `app_deep_link` `cursor://file/private/tmp/re10-fixtures/re10-notes.txt` from `AXDocument` (default title has file name only) | exact: `open` of the deep link opens Cursor on the file | RE-10 | 2026-10-08 | VS Code not installed; Cursor used. Cursor exposes `AXDocument`; VS Code still needs a title with `${activeEditorLong}` or stays app only. See RE-10 section. |
| R20 | Finder window | Folder path | Folder revealed | `file_path` same relative plan filename, not the folder | wrong | `123cd75` | 2026-09-28 | Snippet mentioned the fixtures folder; stored reopen path did not. |
| R21 | File moved after capture | Found again by name | Opens from new location, UI says moved | Absolute path that is then moved | opened_moved | RE-07 | 2026-10-05 | Spotlight exact-name lookup; UI: "File was moved. Opened it from …". See RE-07 section. |
| R22 | File deleted after capture | Stored path | Clear "no longer exists," memory still readable | Absolute path then deleted | missing | RE-07 | 2026-10-05 | Outcome `missing`; Vault status line; memory is not deleted. Relative paths are also `missing` and skip Spotlight. |
| R23 | File on an unmounted external drive | Stored path | Clear "drive not connected" | `/Volumes/FndrRe07Vol/doc.pdf` after detach | drive_not_connected | RE-07 | 2026-10-05 | Synthetic APFS disk image stood in for an external drive. UI: "Connect the drive FndrRe07Vol to open this file." |
| R24 | iCloud file evicted from the Mac | Stored path | Opens and downloads, or clear message | iCloud Drive file still on disk | opened (placeholder / unevictable new file) | RE-07 | 2026-10-05 | `brctl evict` refused a brand-new file. Present iCloud/placeholder files `exists()` and reopen as `opened`. |
| R25 | Path with spaces, accents, emoji | Stored exactly | Opens | `file_path` `…/re03-fixtures/café 📁/report (2) ✨.pdf` (decomposed accent, as macOS reports it) | exact | `a569e48`+RE-03 | 2026-10-01 | Preview. Stored path exists and opens. Full `tauri dev` capture, see RE-03 section. |
| R26 | Slack channel | App, or deep link if derivable | App or channel | Desktop: title `Slack`, no `AXDocument` → would store `app_bundle`. Web `app.slack.com/client/T…/C…` stays `browser_url` (unit-tested) | desktop: app only (honest); web: exact URL when AX/browser URL is stored | RE-10 | 2026-10-08 | Desktop exposes no team/channel id. No `slack://` mapping added. See RE-10 section. |
| R27 | Notion desktop page | Notion URL or `notion://` | Same page | Desktop: title is the page name only, no `AXDocument` → `app_bundle`. Web `www.notion.so/…` stays `browser_url` (unit-tested) | desktop: app only (honest); web: exact URL when stored | RE-10 | 2026-10-08 | No `notion://` mapping added. Figma desktop not installed; web `www.figma.com` covered by the same BrowserUrl order test. See RE-10 section. |
| R28 | Zoom call | App only | App, UI says no specific target | | not available | `123cd75` | 2026-09-28 | Zoom not in this run's target list |
| R29 | App uninstalled after capture | Stored bundle id | Clear error | Bundle id with `NSWorkspace` check | app_missing when gone | RE-07 | 2026-10-05 | Preview is installed → `app_only`. Fake `com.fndr.re07.missingapp` → `app_missing`, UI: "<app> is no longer installed." Nothing launched. |
| R30 | Chrome download of a PDF | File path, source URL, link to the page memory | PDF opens | `file_path` plus `url` from WhereFroms page; `related_memory_ids` when a browser memory matches the host in the prior 2 min | exact file (`plan_reopen` → `opened`) | RE-08 | 2026-10-07 | `build_download_record` sets `FilePath` and keeps it through `normalize_record_for_index`. xattr roundtrip stores the referring page, not the signed CDN URL. Live Chrome download (synthetic local page): `file_path` row, `url` = referring page, linked to the Chrome memory; see RE-08 section. |
| R31 | Safari download that auto-unzips | Record what lands in Downloads | The extracted item or the archive, documented | Tracker memory for `fndr-re01-safari-unzip.zip` (archive), `app_bundle` Finder | app only | `123cd75` | 2026-09-28 | Copied zip into Downloads; Safari auto-unzip not triggered. Watcher stored the archive, not an extracted item. |
| R32 | Download still in progress (`.crdownload`, `.download`, `.part`) | No memory until complete; exactly one after | Opens once complete | One `file_path` memory after rename; partials never ingested | exact once complete | RE-08 | 2026-10-07 | Settler: temp extensions ignored; zero-byte + `.part` sibling never ready; growing file waits for stable size; ingest-once per path. Live: Chrome's `Unconfirmed NNNNNN.crdownload` produced no row; one row per finished file. See RE-08 section. |
| R33 | Second download with the same name (`report (1).pdf`) | Separate memory, its own path | Correct file | Separate `file_path` rows for `report.pdf` and `report (1).pdf` | exact, each path | RE-08 | 2026-10-07 | Ingested set is keyed by the full path. Covered by `settler_duplicate_names_are_separate_paths`. Live: `fndr-re08-live.pdf` and `fndr-re08-live (1).pdf` got separate rows. |
| R34 | Downloaded file renamed in Finder | Found again | Opens the renamed file | Extra tracker row `fndr-re01-report-renamed.pdf`; old name row remains | wrong | `123cd75` | 2026-09-28 | Watcher treated rename as a new file. No “found again” / `OpenedMoved`. |
| R35 | `.dmg`, `.pkg`, `.app`, or `.command` download | File path | Revealed in Finder, never opened or run | `file_path` for the installer/script | reveal in Finder (`open -R`); never launched | RE-09 | 2026-10-07 | Extension list plus quarantined unix-executable. Quarantined PDF still opens. MCP returns `reveal_only` and does not launch. See RE-09 section. |
| R36 | Memory merged from frames with different targets | The most specific, newest target | That target | Merge of the RE-01 Chrome card (`app_bundle` plus a stray file path) with a `browser_url` capture: `browser_url` `https://en.wikipedia.org/wiki/Nitrogen`, no file path | pass: reopens the URL | RE-06 | 2026-10-05 | Whole-target merge by rank (see RE-06 section). Covered by `merge_keeps_browser_target_over_app_with_stray_file_r36`. Unit-test verified, not a live capture. |
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
| Editor stays app only | VS Code default title has no folder; set `window.title` to include `${activeEditorLong}`, or use Cursor which exposes `AXDocument`. Notion/Slack desktop titles have no URL or ids (RE-10). |
| PDF page not stored | Preview title is total-only (`– 1 page`) or browser URL is not `.pdf`; page parsers live in `memory/reopen.rs` |
| Download cannot reopen the file | Fixed in RE-08: `build_download_record` sets `FilePath`; settler delay is `flush_interval_secs + 15 s` |
| Kind vs file disagree after merge | `capture/mod.rs` ~4795 field-wise `or` |
| Missing file is a thrown string, not typed UI | Fixed in RE-07: `plan_reopen` + Vault status line |
| Surfaces disagree | Vault vs Timeline vs omnibar vs MCP as in R41 |
| Installer download is launched | Fixed in RE-09: `should_reveal_in_finder` + `open -R` |
