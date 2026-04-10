# Five-minute TA demo (FNDR)

## Before the meeting

1. Build the evaluation UI: `VITE_EVAL_UI=true npm run build` then run the Tauri app, **or** use a branch where eval UI is default.
2. Optional: enable **Use demo data only** in **Settings → Demo grading** (or launch with `--demo-data-only`) so the demo does not depend on Screen Recording.
3. Click **Seed demo dataset** (or **Reset** then **Seed**) so 36 deterministic rows exist.

## Script (semantic retrieval)

1. **Launch** the app; confirm the **Readiness** panel shows search-ready (green or acceptable fixes).
2. Open a **browser** tab containing a distinctive phrase, e.g. `OAuth redirect URI mismatch on localhost 5174`, **or** click **Inject test memory** if you are in demo-only mode.
3. Wait briefly for capture/indexing **or** rely on seeded data.
4. In FNDR search, type a **paraphrase**, e.g. `that oauth localhost error` — results should match by meaning.
5. Use the **App** filter → choose **Safari** or **Google Chrome** to show **one filter** working.
6. Open **Settings** → pause **Resume/Pause capture** to show **pause**.
7. Under **Privacy → Blocked Apps**, add one app name to show **one privacy control**.
8. Optionally click a **timeline** card to show selection.

## Team code walkthrough (prepare in advance)

| Area            | Suggested file(s)              | One-liner focus                    |
|-----------------|--------------------------------|------------------------------------|
| UI + IPC        | `src/App.tsx`, `src/api/tauri.ts` | React shell, `invoke` commands  |
| Capture + OCR   | `src-tauri/src/capture/`       | Screen sampling, Vision OCR        |
| Embeddings + privacy | `src-tauri/src/embed/`, `privacy/` | Hash embeddings, blocklist    |
| Store + search  | `src-tauri/src/store/lance_store.rs`, `search/` | LanceDB, hybrid search |

Each teammate: **one file open**, **what problem**, **how it plugs in**, **what you wrote**, **~lines of non-boilerplate**.

## Freeze

Tag a release (e.g. `v0.1-demo-eval`) 24 hours before grading; use one machine + one backup; keep this checklist printed.
