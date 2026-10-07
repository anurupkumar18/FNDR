# VS-23 evidence (cloud): the Vault reads like your work

**Status:** built and tested in the cloud session against synthetic browser-preview data. The "one screen per day on the seeded profile" check still needs the owner on the Mac (see the last section).

Branch `claude/train-c-ux`, one commit starting `VS-23:`.

## What changed

| Ticket ask | What the Vault does now | Where |
|---|---|---|
| Group by day, then by thread, newest first | Day sections in local time ("Today", "Yesterday", then e.g. "Monday, Sep 21"), each with a memory count. Inside a day, one thread per project; a memory with no project falls back to its app, which is what the capture session is keyed on (`build_session_id` uses app plus a 30 minute slot, and the card does not carry `session_id`). Threads are ordered by their newest memory; rows are newest first with an id tie-break so the order never depends on input order. | `src/domains/memory-vault/vaultGrouping.ts` (`groupVaultMemories`) |
| Collapse near-duplicates with a "3 similar" count | Inside one thread, memories with the same normalized title (case, punctuation and spacing ignored) and the same app fold under the newest one. The row shows an "N similar" button (`aria-expanded`) that unfolds them in place. If the Vault is opened focused on a folded memory, its group starts unfolded. | `vaultGrouping.ts`, `VaultDayList.tsx` |
| Row: human title, one-line summary, source icon, reopen action | The existing compact `MemoryCard` row keeps its title and one-line summary and gains a source icon before the app name: web page (URL or http reopen target), document (`file://` document path), download (the downloads tracker's Finder/Downloads shape), agent note (`source_type == "agent"`), otherwise screen capture. Rows with a `reopen_target` get an "Open source" button that calls the existing `reopenMemory` IPC (`reopen_memory`, Minh's RE work) through the panel's existing `handleReopen`; its accessible name reuses `reopenButtonLabel`, so PDF rows say the page. No new backend command. | `VaultDayList.tsx`, `MemoryCard.tsx` (new optional `sourceIcon` prop, `reopenButtonLabel` exported) |
| Graph strip behind a "Connections" toggle | A "Connections" toggle (`aria-pressed`) sits next to "Excluded captures". It is off by default; the Vault no longer asks for the graph until it is turned on. On shows the existing strip, with a loading state and an honest "No connections to show yet." when the graph is empty. | `MemoryCardsPanel.tsx` |
| Two clicks to any item | Visible row: one click opens the memory, one click on "Open source" reopens it. Folded duplicate: "N similar" then the row or its "Open source" (two clicks). Beyond the 300-row render batch: "Show 300 more" then the row (two clicks). | tests below |

Kept as before: the App, When and Activity filters, the 300-row render batch and "Show N more", the excluded-captures queue, the expanded memory modal and its actions, the activity trace.

Density: Vault rows drop the compact card's 96 px minimum height (about 62 px now) and hide the per-row day label, since the day heading carries it.

Preview data (`src/dev/previewIpc.ts`, dev-only): three existing synthetic memories got a `project`, five synthetic memories were added for "yesterday" (a near-duplicate trio, a web page, a download), and `get_full_graph` now returns a small graph derived from the preview cards so the Connections strip can be shown. Pre-existing em and en dashes in the files this commit touches were replaced with plain punctuation (comments and synthetic window titles only).

## Tests

Test first: the 14 grouping tests failed on the missing module, and the 4 new panel tests failed with `Unable to find role="region" and name "Today"`, `role="button" and name /^2 similar/`, `role="button" and name "Open source: Read the rubric"` and `role="button" and name "Connections"` before the implementation.

New and changed tests:

- `src/domains/memory-vault/__tests__/vaultGrouping.test.ts` (14, table-driven): local-day buckets and labels, project thread across apps, app fallback, blank project, thread order and id tie-break under reversed input, near-duplicate fold limited to the same thread, app and day, counts, empty input, and six source-kind cases. Timestamps are built with the local `Date` constructor, and the file also passed under `TZ=America/Los_Angeles`, `TZ=Asia/Kolkata` and `TZ=Pacific/Kiritimati`.
- `src/domains/memory-vault/MemoryCardsPanel.test.tsx` (+4): day regions and thread headings with source icons; "2 similar" folds and unfolds; reopen from a row in one click and a folded duplicate in two; the Connections toggle hides the strip and skips `getFullGraph` until pressed. The six existing panel tests pass unchanged.

Focused run:

```
$ npx vitest run src/domains/memory-vault/__tests__/vaultGrouping.test.ts src/domains/memory-vault/MemoryCardsPanel.test.tsx
 Test Files  2 passed (2)
      Tests  24 passed (24)
```

Full checks:

```
$ npm run typecheck
> fndr@0.3.0 typecheck
> tsc --noEmit
(exit 0)

$ npm test
 Test Files  74 passed (74)
      Tests  476 passed (476)
(exit 0)

$ npm run build
✓ built in 8.35s
(exit 0; the existing "chunks larger than 500 kB" warning is unchanged)
```

Baseline on `origin/main` before the change: `Test Files 73 passed (73)`, `Tests 458 passed (458)`.

## Screenshots

Browser preview (`ui-preview.html`, synthetic IPC only, clock pinned to the fixtures' anchor, 1280 x 900). The before images use the same new preview data with `origin/main`'s Vault code, so the only difference is this change.

| | Light | Dark |
|---|---|---|
| Before (flat list, fixture order, no grouping) | [before-light.png](VS-23/before-light.png) | [before-dark.png](VS-23/before-dark.png) |
| After (Today fits the window) | [after-light.png](VS-23/after-light.png) | [after-dark.png](VS-23/after-dark.png) |
| After, Yesterday with "2 similar" unfolded and the download icon | [after-yesterday-similar-light.png](VS-23/after-yesterday-similar-light.png) | |
| After, Connections on | [after-connections-light.png](VS-23/after-connections-light.png) | |

## Known limits and follow-ups

- **Agent note icon has no data yet.** The backend `MemoryRecord` has `source_type`, but the `MemoryCard` DTO (`src-tauri/src/search/memory_cards.rs`, `src/shared/ipc/tauri.ts`) does not carry it, so no real row can show the agent icon until a follow-up adds the field. The UI already reads it when present.
- **Thread fallback is the app, not the capture session.** The card does not carry `session_id`; adding it would let the fallback split one app's day into real sessions.
- **Graph node clicks in the strip** still go to the existing handler, whose detail panel only renders in the full graph view; unchanged from before.

## Left for the owner (real app, seeded profile)

1. `make qa-seed`, then `FNDR_DEMO_DIR="$HOME/Library/Application Support/com.fndr.app.qa" scripts/demo/run-demo.sh`, and open Memory Vault.
2. Check that each day fits one screen at your usual window size. The seeded corpus puts up to 8 memories in the last 24 hours (4 projects plus one memory without a project, so 5 threads), depending on the time of day you seed; the other days have 1 to 3. In the preview, 5 memories in 3 threads fit with room to spare at 1280 x 900.
3. Click any row (opens the memory) and its "Open source" (reopens it): two clicks. Try a PDF row (label says the page) and a web row.
4. Turn on Connections and confirm the strip loads from the real graph or shows "No connections to show yet."
5. Take the same before and after screenshots on the seeded profile and add them next to these.
