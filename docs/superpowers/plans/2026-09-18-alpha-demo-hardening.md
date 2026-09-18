# FNDR Alpha Demo Hardening — Overnight Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (recommended for this plan — see "Execution model") or superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. After every task, append one line to the progress ledger (`docs/superpowers/plans/2026-09-18-alpha-demo-progress.md`) and push the branch.

**Goal:** By 07:00 on 2026-09-18, the `alpha-demo` branch of FNDR 1.0 runs on this 8 GB M1 against a seeded demo profile. It shows a professional, honest, working product: Home → Privacy → live capture → Search → Ask FNDR (cited answer + honest refusal) → Vault quality gate → Felipe's Daily Summary + Wrapped → Kunj's Screen Guide. Every non-demo surface is hidden or deleted.

**Architecture:** One shared read-side admission policy (`memory_quality::low_signal_reason`) is applied at every read boundary: search cards, Vault list, Home recent, and Ask FNDR/`fndr_answer`. Low-signal captures stay stored and appear only in a labeled Vault "Needs more signal" list. The demo profile is a separate data directory, selected through one `FNDR_DATA_DIR` resolver. It is seeded through the real `Store::add_batch_preserving_ids` path, so the normal quality gate classifies it, and the real on-device review worker reviews it. The UI surface is curated through `SIDEBAR_GROUPS` + `DEMO_COMMAND_IDS` + `AppPanels` mounts.

**Tech Stack:** Tauri 2 + Rust (LanceDB, ONNX MiniLM 384-d, Qwen3-VL-2B GGUF), React 18 + TypeScript + Vite + Vitest, zustand, framer-motion.

**Spec:** The "Owner decisions (binding spec)" section below, plus the owner's handoff prompt in the originating Claude session. These decisions were made by the owner (Anurup) interactively on 2026-09-18 00:00–00:40 and override the handoff wherever they differ.

## Owner decisions (binding spec)

| # | Decision | Value |
|---|---|---|
| D1 | Visible demo surface | Home, Memory Vault, **Ask FNDR**, Daily Summary (Felipe), FNDR Wrapped (Felipe), Screen Guide (Kunj), Cmd+K palette. Everything else hidden or deleted. |
| D2 | Git flow | New branch `alpha-demo` from `origin/main` + cherry-pick `0465dac`. Push **only `alpha-demo`** to GitLab after each verified task. **Never push `main`.** Owner merges after the morning rehearsal. |
| D3 | Low-signal captures | Kept in storage and excluded from Search, Home, Vault default list, and Ask FNDR. Visible only in the Vault "Needs more signal (N)" toggle, with a plain-language reason. Never deleted. |
| D4 | Ask FNDR engine | `fndr_answer` (grounded Qwen answer, citation-validated, honest "not enough evidence" refusal) + clickable cited sources. It replaces the templated AgentPanel Context mode. |
| D5 | Demo data | Seeded demo profile (7 prior days, realistic CS-capstone student building FNDR, fictional third parties) + live capture during the day. Separate data dir; the real profile is untouched. |
| D6 | Demo machine | This 8 GB M1 (afternoon demo). No teammate-Mac setup work. |
| D7 | Rubric artifacts (wiki, issues, slides, CI) | **Out of scope.** Someone else owns them. Product polish only. |
| D8 | Auth | Mention only, don't show. Demo profile has `biometric_enabled: false`. The Touch ID code stays intact. |
| D9 | Seed story | "Student building FNDR": the owner's profile name, Cursor/GitLab/Canvas/Docs/paper/Slack, fictional names only (Jordan Lee, Prof. Rivera, Sam Patel). Never invent words for real teammates. |
| D10 | Settings | One clean sheet with no tabs: Profile · Capture (pause/resume + Stored/Skipped/Needs-signal) · Privacy (alerts + blocklist) · Local models (read-only "Ready" list). Hidden: Updates, Indexing/retention, Auto-Fill, MCP, model download/delete, Danger Zone. |
| D11 | Look | The moon button becomes a plain dark/light toggle. The palette is locked to the current one. **Static gradient background (no WebGL).** Shader/palette code is retained on disk. |
| D12 | Home | Greeting + search + live capture chip ("● Capturing · N memories today") + "Recent" row (5 latest good memories → open in Vault). Sidebar gets an explicit "Home" item. Remove "SCROLL TO EXPLORE". |
| D13 | Delete entirely | **Agentic experiments** (Hermes bridge, full Agent panel, Automation, Quick Skills, Research) and **Debug/experimental UI** (Engine Metrics panel, Pipeline Inspector, Glasses photo import, Screen Auto-Fill overlay). The backend is deleted only where it's isolated; Rust `agent/` stays (MCP uses it), and backend telemetry stays. |
| D14 | Hide (keep compiled) | Focus Mode, Focus Session, Time Tracking, Search History, Meetings, Knowledge Graph, To-dos, Stats. |
| D15 | Cards | Clean by default (distinct title, one-line summary, What happened / Why it mattered with empty rows hidden, app + time, plain status chip) + a collapsed **"How FNDR built this"** disclosure (confidence, pipeline path in plain words, evidence). No FRAME ids, anchor %, raw snake_case tags, "— not yet extracted", or "0 nodes · 0 edges". |
| D16 | Overnight machine policy | `caffeinate`. Stop the live `tauri dev` during heavy Rust builds/tests (`CARGO_BUILD_JOBS=2`). Relaunch on the demo profile at checkpoints. Leave it running and warmed at the end. |
| D17 | Deadline | **QA-ready by 07:00** (seeded profile, app running, morning QA packet written). |
| D18 | Extra surfaces | **Not visible:** Alt+Space Omnibar (don't register hotkey), voice/mic buttons, MCP toggle, "Find similar screens". |
| D19 | Judge takeaway | "FNDR remembers what you did on your Mac, privately and on-device, and answers questions about it with cited evidence." |
| D20 | Seed disclosure | Verbal only. No UI badge. (Internally `synthesis_branch = "demo_seed"` so the "How FNDR built this" disclosure stays truthful.) |
| D21 | Teammate code | **Full ownership tonight.** Edit Daily Summary / Wrapped / Screen Guide / stats.rs freely for bugs and polish. List every touched file in the morning packet. |
| D22 | Broken teammate feature | **Always keep visible**, flag it in the morning packet with exact repro. |
| D23 | Demo flow | 1 Home → 2 Privacy (pause/resume, block "Messages") → 3 live capture of a safe page (~20s) → 4 Search `E0502` then `that rust borrowing error` → 5 Ask FNDR: `What chunk size did the chunking paper recommend?` (cited) then `What's my bank balance?` (refusal) → 6 Vault "Needs more signal" → 7 Daily Summary (yesterday) + Wrapped (week) → 8 Screen Guide. |

## Global Constraints

- Repo: `/Users/anurupkumar/FNDR` (GitLab `git@capstone.cs.utah.edu:fndr/fndr.git`). **Never touch `/Users/anurupkumar/FNDR-2.0`.**
- Branch: `alpha-demo`. Never push or force-push `main`. `git push origin alpha-demo` after each verified task.
- Real user data dir `~/Library/Application Support/com.fndr.app` is **read-only for this plan**: never write, delete, reset, or seed into it. The only allowed read is `onboarding.json` → `display_name`. Legacy `~/Library/Application Support/com.fndr.FNDR`: never touch.
- Demo data dir: `~/Library/Application Support/com.fndr.app.demo`. Resetting it means moving it to `~/.Trash/` (never `rm -rf`).
- Never persist or display raw screenshots/pixels. `raw_screenshot_stored` stays false. Seeded records carry zero image vectors.
- Never make "AI understanding" claims when evidence is insufficient. User-facing copy is plain English. Default UI never shows snake_case identifiers, FRAME ids, or internal branch names.
- Build policy: `export CARGO_BUILD_JOBS=2`. Stop `tauri dev` before `cargo test`/`make test`. The cargo target dir is shared (`~/.cache/cargo-target-shared`), so don't `cargo clean`.
- Kill only FNDR processes: `pkill -f "cargo-target-shared/debug/fndr"; pkill -f "node .*FNDR/node_modules/.bin/tauri dev"; pkill -f "FNDR/node_modules/.bin/vite --host 127.0.0.1 --port 1420"`. **Never kill** the unrelated Vite on port 5173 (`~/Downloads/AI in EDU`).
- Commits: small, one per task (or per step group), conventional prefix (`fix:`, `feat:`, `chore:`, `demo:`), ending with:
  ```
  Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
  ```
- TypeScript `noUnusedLocals`/`noUnusedParameters` are ON. Removing JSX means removing the now-unused state/handlers/imports too.
- Copy constants (use verbatim):
  - Product takeaway (D19): `FNDR remembers what you did on your Mac, privately and on-device, and answers questions about it with cited evidence.`
  - Ask FNDR badge: `On-device · read-only`
  - Low-signal reasons: see Task 1 `LowSignalReason::user_message`.
- Time gates: **05:45** stop starting new Tier-2 tasks. **06:00** begin Task 15 regardless. **06:50** app running on the demo profile, warmed, packet written.
- Laptop must stay **plugged in** (the review worker is battery-gated).

## Execution model (read first)

1. **Order is by demo value, and Rust changes are batched first.** Tasks 1–4 are all Rust/data work, done while the app is stopped. At the end of Task 4, one full `cargo test`, then launch the demo profile with review backfill. Tasks 5–11 are frontend-only and verified with Vite HMR in the running app plus Vitest. Task 13 (dead-code deletion) needs one more Rust rebuild.
2. **Self-QA without the owner:** `screencapture -x -o -l <windowid>` or plain `screencapture -x /tmp/fndr-qa-N.png` captures the real screen (Screen Recording is granted to this process — verified 00:33). Read the PNG to verify layout. Bring FNDR to front with `osascript -e 'tell application "System Events" to set frontmost of (first process whose name is "fndr") to true'`. If System Events keystrokes are refused (no Accessibility permission), don't retry. Add the check to the morning packet instead.
3. **Usage-limit resilience:** the owner's weekly usage limit is close (banner seen at 00:33). After every task: commit → push `alpha-demo` → append to the progress ledger. If the session dies, any agent (Claude or Codex) resumes by reading the ledger + `git log --oneline origin/main..alpha-demo` and starting the first unchecked task. Prefer inline execution over subagent-per-task to conserve budget.
4. **Tiers:**
   - Tier 1 (must): Tasks 0–10.
   - Tier 2 (should): Tasks 11–14.
   - Tier 3 (always, from 06:00): Task 15.
   If Tier 1 slips past 05:00, skip Task 11 and do Task 12 → 15.

## File structure (what changes where)

| File | Responsibility | Task |
|---|---|---|
| `src-tauri/src/memory_quality.rs` | + `LowSignalReason`, `SurfaceSignals`, `low_signal_reason`, `record_low_signal_reason`, `result_low_signal_reason`, `partition_surfaceable` | 1 |
| `src-tauri/tests/low_signal_surface.rs` (new) | Store-backed regression: filename-only, visual-metadata, duplicate, high-signal, reviewed, review-failed | 1 |
| `src-tauri/src/ipc/commands/search.rs` | Apply policy in `search_memory_cards` + `list_memory_cards`; new `list_needs_signal_memory_cards` + `NeedsSignalCard` | 2 |
| `src-tauri/src/ipc/commands/mod.rs`, `src-tauri/src/main.rs` | Re-export + register new command | 2 |
| `src-tauri/src/context_runtime/mod.rs` | `drop_low_signal_hits` after `fusion::fuse` in `run_query` | 2 |
| `src/shared/ipc/tauri.ts` | `NeedsSignalCard`, `listNeedsSignalMemoryCards` | 2 |
| `src-tauri/src/config.rs` | `fndr_app_data_dir(paths)` resolver honoring `FNDR_DATA_DIR` | 3 |
| `src-tauri/src/main.rs`, `ipc/onboarding.rs`, `ipc/commands/{maintenance,screen_guide,stats}.rs` | Route every `app.path().app_data_dir()` through the resolver; demo review-backfill hook; stop registering the Omnibar/Auto-Fill windows and shortcuts | 3 |
| `src-tauri/examples/seed_demo.rs` (new) | Build/embed/insert the seed corpus; quality-gate assertion | 4 |
| `scripts/demo/demo-week.json` (new) | Seed corpus (~110 records + 3 low-signal) | 4 |
| `scripts/demo/seed-demo-profile.sh`, `scripts/demo/run-demo.sh` (new) | One-command seed / launch | 4 |
| `src/domains/ask/AskPanel.tsx`, `.css`, `.test.tsx` (new) | Ask FNDR panel on `fndrAnswer` | 5 |
| `src/app/App.tsx`, `src/app/AppPanels.tsx`, `src/domains/command-palette/CommandPalette.tsx` (+ test) | Curated sidebar, palette, and mounts | 6 |
| `src/domains/memory-vault/MemoryCardsPanel.tsx` (+ test) | "Needs more signal (N)" toggle | 7 |
| `src/domains/memory-vault/{InsightLayers,MemoryCard,ExpandedMemoryCard,MemoryProvenanceStrip}.tsx`, `src/domains/timeline/Timeline.tsx`, `src/domains/search/SearchBar.tsx`, `src/app/HomeHero.tsx`, `src/shared/utils/config.ts` (+ tests) | Clean cards, disclosure, no mic, no similar/delete in results | 8 |
| `src/domains/workspace/ControlPanel.tsx` (+ test) | One-sheet Settings + theme toggle | 9, 11 |
| `src/app/HomeRecent.tsx` (new, + test), `src/app/App.tsx`, `src/app/HomeHero.tsx`, `src/app/styles/App.css` | Capture chip, Recent row, Home item, scrim | 10 |
| `src/app/AppShell.tsx` (+ test), `src/app/styles/wallpaper.css` | Static background | 11 |
| Teammate files (`DailySummaryPanel*`, `FndrWrappedPanel*`, `ScreenGuidePanel*`, `stats.rs`) | Verify + polish on the demo profile | 12 |
| Deleted panels, wrappers, `hermes_agent.rs`, `glasses_import.rs`, autofill entry points | Dead-code removal | 13 |
| `docs/superpowers/plans/2026-09-18-alpha-demo-{progress,morning}.md` | Ledger + morning QA packet | 0, 15 |

---

## Task 0: Branch, machine prep, baseline (Tier 1, ~15 min)

**Files:**
- Create: `docs/superpowers/plans/2026-09-18-alpha-demo-progress.md`
- Commit: this plan file

- [ ] **Step 1: Keep the Mac awake for the night** (run as a background command; it lives until killed)

```bash
caffeinate -dims
```

- [ ] **Step 2: Stop the running FNDR dev app (only FNDR processes)**

```bash
pkill -f "cargo-target-shared/debug/fndr" ; pkill -f "node .*FNDR/node_modules/.bin/tauri dev" ; pkill -f "FNDR/node_modules/.bin/vite --host 127.0.0.1 --port 1420" ; sleep 2 ; ps aux | grep -E "fndr|1420" | grep -v grep || echo "FNDR stopped"
```
Expected: `FNDR stopped` (the 5173 Vite may still be listed; that's fine).

- [ ] **Step 3: Create the branch from the teammates' latest main and carry 0465dac over**

```bash
cd /Users/anurupkumar/FNDR
git stash list | head -3   # expect nothing important; working tree must be clean except this plan (untracked)
git fetch origin
git switch -c alpha-demo origin/main
git cherry-pick 0465dac || true
git status --short
```
Expected: conflict only in `src/app/AppPanels.tsx`.

- [ ] **Step 4: Resolve the conflict by taking 0465dac's reduced AppPanels** (Task 6 rewrites this file with the final mounts, including Felipe's `onOpenMemoryById` prop for Daily Summary)

```bash
git checkout --theirs src/app/AppPanels.tsx
git add src/app/AppPanels.tsx
GIT_EDITOR=true git cherry-pick --continue
git log --oneline -3
```
Expected: top commit `demo: harden alpha-only local flow` on top of `715cc8c`.

- [ ] **Step 5: Fast baseline (frontend only; Rust baseline happens in Task 1)**

```bash
npm ci --silent && npm run typecheck && npm test 2>&1 | tail -5
```
Expected: typecheck clean and all Vitest tests pass. If anything fails **before any change of ours**, record the failure verbatim in the ledger and continue. Don't fix pre-existing failures unless a later task touches that file.

- [ ] **Step 6: Create the progress ledger**

`docs/superpowers/plans/2026-09-18-alpha-demo-progress.md`:
```markdown
# Alpha demo progress ledger (append-only)

Resume rule: read this file, run `git log --oneline origin/main..alpha-demo`, start the first unchecked task in 2026-09-18-alpha-demo-hardening.md.

| time | task | commit | verification | notes |
|---|---|---|---|---|
| HH:MM | T0 branch + baseline | <sha> | typecheck ✅ vitest ✅/❌(details) | cherry-picked 0465dac; conflict resolved with --theirs |
```

- [ ] **Step 7: Commit plan + ledger, push the branch**

```bash
git add docs/superpowers/plans/2026-09-18-alpha-demo-hardening.md docs/superpowers/plans/2026-09-18-alpha-demo-progress.md
git commit -m "docs: alpha demo overnight plan and progress ledger

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push -u origin alpha-demo
```

---

## Task 1: Shared read-side surface policy (Tier 1, ~40 min incl. first compile)

**Files:**
- Modify: `src-tauri/src/memory_quality.rs` (add after `quality_gate_reason`, ~line 97; tests in the existing `mod tests`)
- Create: `src-tauri/tests/low_signal_surface.rs`

**Interfaces:**
- Produces (used by Tasks 2, 4):
  - `pub enum LowSignalReason { VisualSemanticsFailed, ImageOnly, Ungrounded }` with `fn code(self) -> &'static str` and `fn user_message(self) -> &'static str`
  - `pub fn result_low_signal_reason(r: &SearchResult) -> Option<LowSignalReason>`
  - `pub fn record_low_signal_reason(r: &MemoryRecord) -> Option<LowSignalReason>`
  - `pub fn partition_surfaceable(results: Vec<SearchResult>) -> (Vec<SearchResult>, Vec<(SearchResult, LowSignalReason)>)`

Why a new policy instead of reusing `storage_outcome == "low_quality_evidence"`: `classify_storage_outcome` also assigns `low_quality_evidence` to ordinary text memories with `evidence_confidence >= 0.45`. Filtering on that label would hide real memories. The new policy targets only unreadable/visual-only/ungrounded records, and uses a text-signal check so records whose "OCR" is just the app name plus an image filename are caught (the rehearsal's `ChatGPT_1789709739566.png` card).

- [ ] **Step 1: Write the failing unit tests** (append inside `#[cfg(test)] mod tests` in `memory_quality.rs`; the module already has `use super::*;`, so verify with `grep -n "use super" src-tauri/src/memory_quality.rs`)

```rust
    fn surface_result(
        storage_outcome: &str,
        enrichment_status: &str,
        synthesis_branch: &str,
        ocr_block_count: u32,
        ocr_confidence: f32,
        clean_text: &str,
        display_summary: &str,
    ) -> crate::storage::SearchResult {
        crate::storage::SearchResult {
            id: "m-1".to_string(),
            app_name: "ChatGPT".to_string(),
            storage_outcome: storage_outcome.to_string(),
            enrichment_status: enrichment_status.to_string(),
            synthesis_branch: synthesis_branch.to_string(),
            ocr_block_count,
            ocr_confidence,
            clean_text: clean_text.to_string(),
            display_summary: display_summary.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn low_signal_flags_filename_only_visual_fallback_from_rehearsal() {
        let r = surface_result(
            "low_quality_evidence",
            "",
            "llm_ocr_grounded_visual_fallback",
            1,
            0.62,
            "ChatGPT:",
            "Screen capture (visual) ChatGPT_1789709739566.png. ChatGPT. Also Screen capture (visual) ChatGPT_1789709551930.png. ChatGPT",
        );
        assert_eq!(result_low_signal_reason(&r), Some(LowSignalReason::ImageOnly));
    }

    #[test]
    fn low_signal_flags_visual_metadata_fallback_without_text() {
        let r = surface_result("low_quality_evidence", "visual_metadata_fallback", "visual_metadata_fallback", 0, 0.0, "", "");
        assert_eq!(result_low_signal_reason(&r), Some(LowSignalReason::ImageOnly));
    }

    #[test]
    fn low_signal_flags_visual_semantics_failed_and_quarantine() {
        let failed = surface_result(VISUAL_SEMANTICS_FAILED_OUTCOME, "", "", 0, 0.0, "", "");
        assert_eq!(result_low_signal_reason(&failed), Some(LowSignalReason::VisualSemanticsFailed));
        let quarantined = surface_result("quarantine_low_grounding", "", "llm_ocr_grounded", 6, 0.9, "error[E0502]: cannot borrow `self.buffer` as mutable because it is also borrowed as immutable", "Fixed borrow error");
        assert_eq!(result_low_signal_reason(&quarantined), Some(LowSignalReason::Ungrounded));
    }

    #[test]
    fn low_signal_keeps_high_signal_ocr_memory() {
        let r = surface_result(
            "primary_memory_card",
            "",
            "llm_ocr_grounded",
            9,
            0.94,
            "error[E0502]: cannot borrow `self.frame_buffer` as mutable because it is also borrowed as immutable --> src/capture/mod.rs:412:9",
            "Debugged E0502 borrow error in capture/mod.rs",
        );
        assert_eq!(result_low_signal_reason(&r), None);
    }

    #[test]
    fn low_signal_keeps_ocr_grounded_visual_fallback_with_real_text() {
        let r = surface_result(
            "low_quality_evidence",
            "",
            "llm_ocr_grounded_visual_fallback",
            7,
            0.88,
            "Parent-child chunking with 512-token parents and 128-token children gave the best recall at 10 in our experiments",
            "Read chunking results",
        );
        assert_eq!(result_low_signal_reason(&r), None);
    }

    #[test]
    fn low_signal_keeps_reviewed_and_review_failed_text_memories() {
        let reviewed = surface_result("primary_memory_card", "reviewed_local", "llm_ocr_grounded", 5, 0.9, "Canvas CS 4500 Alpha Release rubric: most rank 1 features complete, framework for rank 2", "Read the Alpha rubric");
        assert_eq!(result_low_signal_reason(&reviewed), None);
        let failed_review = surface_result("enriched_memory_card", "review_failed", "llm_ocr_grounded", 5, 0.9, "Canvas CS 4500 Alpha Release rubric: most rank 1 features complete, framework for rank 2", "Read the Alpha rubric");
        assert_eq!(result_low_signal_reason(&failed_review), None);
    }

    #[test]
    fn low_signal_does_not_hide_plain_low_quality_text_memory() {
        let r = surface_result("low_quality_evidence", "", "llm_ocr_grounded", 4, 0.8, "Jordan Lee: can we move the alpha dry run to Thursday at 4pm in the MEB lab? I booked room 3147", "Dry run moved to Thursday");
        assert_eq!(result_low_signal_reason(&r), None);
    }

    #[test]
    fn partition_surfaceable_splits_and_preserves_order() {
        let good = surface_result("primary_memory_card", "", "llm_ocr_grounded", 9, 0.94, "error[E0502]: cannot borrow `self.frame_buffer` as mutable because it is also borrowed as immutable", "E0502");
        let bad = surface_result("", "visual_metadata_fallback", "visual_metadata_fallback", 0, 0.0, "", "");
        let (kept, hidden) = partition_surfaceable(vec![bad.clone(), good.clone(), bad]);
        assert_eq!(kept.len(), 1);
        assert_eq!(hidden.len(), 2);
        assert!(hidden.iter().all(|(_, reason)| *reason == LowSignalReason::ImageOnly));
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cd /Users/anurupkumar/FNDR/src-tauri && CARGO_BUILD_JOBS=2 cargo test --lib memory_quality::tests::low_signal 2>&1 | tail -15`
Expected: compile error `cannot find function result_low_signal_reason` / `LowSignalReason`. (The first compile of the lib test target can take 10–20 min on this machine. Do not interrupt it.)

- [ ] **Step 3: Implement the policy** (insert after `quality_gate_reason` in `memory_quality.rs`; add `SearchResult` to the existing `use crate::storage::MemoryRecord;` → `use crate::storage::{MemoryRecord, SearchResult};`)

```rust
/// Minimum alphanumeric characters of on-screen text (excluding the app name,
/// image filenames, and "Screen capture (visual)" boilerplate) for a visual
/// fallback capture to count as a real memory.
pub const LOW_SIGNAL_TEXT_MIN_CHARS: usize = 40;

/// Why a stored memory is kept out of Search, Home, the default Vault list,
/// and Ask FNDR. The record stays stored and is listed only in the Vault's
/// "Needs more signal" view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LowSignalReason {
    VisualSemanticsFailed,
    ImageOnly,
    Ungrounded,
}

impl LowSignalReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::VisualSemanticsFailed => "visual_semantics_failed",
            Self::ImageOnly => "image_only",
            Self::Ungrounded => "ungrounded_summary",
        }
    }

    /// Plain-language reason shown in the Vault "Needs more signal" list.
    pub fn user_message(self) -> &'static str {
        match self {
            Self::VisualSemanticsFailed => {
                "FNDR couldn't read this screen, so it was kept out of search."
            }
            Self::ImageOnly => {
                "Image only: no readable text was on screen, so it was kept out of search."
            }
            Self::Ungrounded => {
                "The summary couldn't be matched to on-screen text, so it was kept out of search."
            }
        }
    }
}

/// Read-side fields shared by `MemoryRecord` and `SearchResult`.
pub struct SurfaceSignals<'a> {
    pub storage_outcome: &'a str,
    pub enrichment_status: &'a str,
    pub synthesis_branch: &'a str,
    pub ocr_block_count: u32,
    pub ocr_confidence: f32,
    pub clean_text: &'a str,
    pub display_summary: &'a str,
    pub app_name: &'a str,
}

impl<'a> SurfaceSignals<'a> {
    pub fn from_record(r: &'a MemoryRecord) -> Self {
        Self {
            storage_outcome: &r.storage_outcome,
            enrichment_status: &r.enrichment_status,
            synthesis_branch: &r.synthesis_branch,
            ocr_block_count: r.ocr_block_count,
            ocr_confidence: r.ocr_confidence,
            clean_text: &r.clean_text,
            display_summary: &r.display_summary,
            app_name: &r.app_name,
        }
    }

    pub fn from_result(r: &'a SearchResult) -> Self {
        Self {
            storage_outcome: &r.storage_outcome,
            enrichment_status: &r.enrichment_status,
            synthesis_branch: &r.synthesis_branch,
            ocr_block_count: r.ocr_block_count,
            ocr_confidence: r.ocr_confidence,
            clean_text: &r.clean_text,
            display_summary: &r.display_summary,
            app_name: &r.app_name,
        }
    }
}

fn is_image_filename_token(token: &str) -> bool {
    const EXTS: [&str; 6] = [".png", ".jpg", ".jpeg", ".heic", ".gif", ".webp"];
    EXTS.iter().any(|ext| token.ends_with(ext))
        || (!token.is_empty() && token.chars().all(|c| c.is_ascii_digit() || c == '_'))
}

/// Counts alphanumeric characters that carry meaning beyond the app name,
/// image filenames, and FNDR's own visual-fallback boilerplate.
pub fn meaningful_text_chars(text: &str, app_name: &str) -> usize {
    let app_tokens: Vec<String> = app_name
        .split_whitespace()
        .map(|t| t.to_lowercase())
        .collect();
    text.split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric() && c != '.' && c != '_')
                .trim_end_matches('.')
                .to_lowercase()
        })
        .filter(|t| !t.is_empty())
        .filter(|t| !is_image_filename_token(t))
        .filter(|t| !app_tokens.iter().any(|a| a == t))
        .filter(|t| !matches!(t.as_str(), "screen" | "capture" | "visual" | "also"))
        .map(|t| t.chars().filter(|c| c.is_alphanumeric()).count())
        .sum()
}

/// The single read-side admission policy for Search, Home, Vault, and Ask FNDR.
pub fn low_signal_reason(s: &SurfaceSignals<'_>) -> Option<LowSignalReason> {
    let outcome = s.storage_outcome.trim();
    if outcome.eq_ignore_ascii_case(VISUAL_SEMANTICS_FAILED_OUTCOME) {
        return Some(LowSignalReason::VisualSemanticsFailed);
    }
    if outcome.starts_with("quarantine_") {
        return Some(LowSignalReason::Ungrounded);
    }

    let thin_text = s.ocr_block_count == 0
        || s.ocr_confidence <= 0.01
        || meaningful_text_chars(s.clean_text, s.app_name) < LOW_SIGNAL_TEXT_MIN_CHARS;
    let branch = s.synthesis_branch.trim();
    let visual_fallback = s
        .enrichment_status
        .trim()
        .eq_ignore_ascii_case("visual_metadata_fallback")
        || branch.eq_ignore_ascii_case("visual_metadata_fallback")
        || branch.eq_ignore_ascii_case("llm_ocr_grounded_visual_fallback");
    let filename_summary = s
        .display_summary
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("screen capture (visual)");

    if (visual_fallback || filename_summary) && thin_text {
        return Some(LowSignalReason::ImageOnly);
    }
    None
}

pub fn result_low_signal_reason(r: &SearchResult) -> Option<LowSignalReason> {
    low_signal_reason(&SurfaceSignals::from_result(r))
}

/// Record variant: also consults `raw_evidence`-based detectors that
/// `SearchResult` can't see.
pub fn record_low_signal_reason(r: &MemoryRecord) -> Option<LowSignalReason> {
    if let Some(reason) = low_signal_reason(&SurfaceSignals::from_record(r)) {
        return Some(reason);
    }
    if is_visual_semantics_failed_record(r) {
        return Some(LowSignalReason::VisualSemanticsFailed);
    }
    let thin_text =
        meaningful_text_chars(&r.clean_text, &r.app_name) < LOW_SIGNAL_TEXT_MIN_CHARS;
    if is_low_evidence_visual_fallback_record(r)
        || (is_visual_metadata_fallback_record(r) && thin_text)
    {
        return Some(LowSignalReason::ImageOnly);
    }
    None
}

/// Splits results into (surfaceable, low-signal-with-reason), preserving order.
pub fn partition_surfaceable(
    results: Vec<SearchResult>,
) -> (Vec<SearchResult>, Vec<(SearchResult, LowSignalReason)>) {
    let mut kept = Vec::with_capacity(results.len());
    let mut hidden = Vec::new();
    for result in results {
        match result_low_signal_reason(&result) {
            Some(reason) => hidden.push((result, reason)),
            None => kept.push(result),
        }
    }
    (kept, hidden)
}
```

- [ ] **Step 4: Run unit tests to verify they pass**

Run: `cd /Users/anurupkumar/FNDR/src-tauri && CARGO_BUILD_JOBS=2 cargo test --lib memory_quality 2>&1 | tail -8`
Expected: all `memory_quality::tests::*` pass (existing + 8 new).

- [ ] **Step 5: Write the store-backed regression test** `src-tauri/tests/low_signal_surface.rs`

```rust
//! Store-backed regression for the read-side surface policy: fixture records go
//! through `Store::add_batch` normalization, come back via `list_recent_results`,
//! and are partitioned exactly as Search/Vault/Home will partition them.

use fndr_lib::config::DEFAULT_IMAGE_EMBEDDING_DIM;
use fndr_lib::embedding::EMBEDDING_DIM;
use fndr_lib::memory_quality::{partition_surfaceable, LowSignalReason};
use fndr_lib::storage::{MemoryRecord, Store};

fn base(id: &str, ts: i64, app: &str, title: &str, text: &str) -> MemoryRecord {
    MemoryRecord {
        id: id.to_string(),
        timestamp: ts,
        day_bucket: chrono::Local::now().format("%Y-%m-%d").to_string(),
        app_name: app.to_string(),
        window_title: title.to_string(),
        session_id: format!("session-{id}"),
        text: text.to_string(),
        clean_text: text.to_string(),
        ocr_confidence: 0.93,
        ocr_block_count: 6,
        snippet: text.chars().take(160).collect(),
        summary_source: "fixture".to_string(),
        noise_score: 0.02,
        session_key: format!("fixture-{id}"),
        embedding: vec![0.0; EMBEDDING_DIM],
        image_embedding: vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM],
        snippet_embedding: vec![0.0; EMBEDDING_DIM],
        support_embedding: vec![0.0; EMBEDDING_DIM],
        decay_score: 1.0,
        specificity_score: 0.8,
        intent_score: 0.7,
        entity_score: 0.6,
        agent_usefulness_score: 0.75,
        evidence_confidence: 0.85,
        retrieval_value_score: 0.7,
        ocr_noise_score: 0.05,
        ..Default::default()
    }
}

#[test]
fn surface_policy_over_real_store_round_trip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = Store::new(dir.path()).expect("store");
    let now = chrono::Utc::now().timestamp_millis();

    let high_signal = base(
        "fx-high-signal",
        now - 1_000,
        "Cursor",
        "capture/mod.rs — fndr",
        "error[E0502]: cannot borrow `self.frame_buffer` as mutable because it is also borrowed as immutable --> src/capture/mod.rs:412:9",
    );

    let mut reviewed = base(
        "fx-reviewed",
        now - 2_000,
        "Google Chrome",
        "CS 4500 Alpha Release — Canvas",
        "Alpha Release Expectations: most rank 1 features complete, framework for rank 2, components talk to each other",
    );
    reviewed.enrichment_status = "reviewed_local".to_string();
    reviewed.reviewed_at_ms = now - 500;
    reviewed.insight_what_happened = "Read the CS 4500 Alpha rubric.".to_string();

    let mut review_failed = base(
        "fx-review-failed",
        now - 3_000,
        "Slack",
        "#fndr-team — Slack",
        "Jordan Lee: can we move the alpha dry run to Thursday at 4pm in the MEB lab? I booked room 3147",
    );
    review_failed.enrichment_status = "review_failed".to_string();

    let mut filename_only = base(
        "fx-filename-only",
        now - 4_000,
        "ChatGPT",
        "ChatGPT",
        "ChatGPT:",
    );
    filename_only.ocr_block_count = 1;
    filename_only.synthesis_branch = "llm_ocr_grounded_visual_fallback".to_string();
    filename_only.display_summary =
        "Screen capture (visual) ChatGPT_1789709739566.png. ChatGPT".to_string();

    let mut visual_metadata = base("fx-visual-metadata", now - 5_000, "Preview", "IMG_4471.HEIC", "");
    visual_metadata.ocr_block_count = 0;
    visual_metadata.ocr_confidence = 0.0;
    visual_metadata.enrichment_status = "visual_metadata_fallback".to_string();
    visual_metadata.synthesis_branch = "visual_metadata_fallback".to_string();

    // Two near-identical captures of the same screen in one batch.
    let dup_a = base("fx-dup-a", now - 6_000, "Cursor", "reranker.rs — fndr", "fn rerank_results(query: &QueryContext, results: Vec<SearchResult>) -> (Vec<SearchResult>, RerankStats)");
    let mut dup_b = dup_a.clone();
    dup_b.id = "fx-dup-b".to_string();
    dup_b.timestamp = now - 5_500;

    let rt = tokio::runtime::Runtime::new().expect("runtime");
    rt.block_on(async {
        store
            .add_batch(&[
                high_signal,
                reviewed,
                review_failed,
                filename_only,
                visual_metadata,
                dup_a,
                dup_b,
            ])
            .await
            .expect("add_batch");
    });

    let listed = rt
        .block_on(async { store.list_recent_results(100, None).await })
        .expect("list");
    let dup_count = listed.iter().filter(|r| r.id.starts_with("fx-dup-")).count();
    assert_eq!(dup_count, 1, "near-identical captures in one batch must collapse to one record");

    let (kept, hidden) = partition_surfaceable(listed);
    let kept_ids: Vec<&str> = kept.iter().map(|r| r.id.as_str()).collect();
    assert!(kept_ids.contains(&"fx-high-signal"));
    assert!(kept_ids.contains(&"fx-reviewed"));
    assert!(kept_ids.contains(&"fx-review-failed"));
    assert!(!kept_ids.contains(&"fx-filename-only"));
    assert!(!kept_ids.contains(&"fx-visual-metadata"));

    let hidden_ids: Vec<(&str, LowSignalReason)> =
        hidden.iter().map(|(r, why)| (r.id.as_str(), *why)).collect();
    assert!(hidden_ids.contains(&("fx-filename-only", LowSignalReason::ImageOnly)));
    assert!(hidden_ids.contains(&("fx-visual-metadata", LowSignalReason::ImageOnly)));
}
```

- [ ] **Step 6: Run it**

Run: `cd /Users/anurupkumar/FNDR/src-tauri && CARGO_BUILD_JOBS=2 cargo test --test low_signal_surface 2>&1 | tail -15`
Expected: PASS. Two known failure modes, handled without touching prod code:
- **Dup assertion fails** (dedup key didn't match): read `record_insert_dedup_key` in `src-tauri/src/storage/lance_store/normalize_embed_migrate.rs` and set whichever field it keys on (e.g. `content_hash` / `dedup_fingerprint`) identically on `dup_a`/`dup_b`, then re-run.
- **A "kept" record is hidden** because normalization rewrote `storage_outcome` to `quarantine_*`: print `storage_outcome`/`quality_gate_reason` for it (`eprintln!`), then raise that fixture's scores or text length (fixtures must represent good captures). Don't weaken the policy.

- [ ] **Step 7: Commit + push + ledger**

```bash
cd /Users/anurupkumar/FNDR
git add src-tauri/src/memory_quality.rs src-tauri/tests/low_signal_surface.rs
git commit -m "fix(quality): shared read-side low-signal surface policy

Filename-only / visual-fallback / ungrounded captures stay stored but are
classified for exclusion from Search, Home, Vault and Ask FNDR.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push origin alpha-demo
```

---

## Task 2: Apply the policy at every read boundary + review-queue command (Tier 1, ~40 min)

**Files:**
- Modify: `src-tauri/src/ipc/commands/search.rs` (`search_memory_cards` ~line 472, `list_memory_cards` ~line 588)
- Modify: `src-tauri/src/ipc/commands/mod.rs` (the explicit `pub use search::{ ... }` list at line 6)
- Modify: `src-tauri/src/main.rs` (`invoke_handler` list, next to `ipc::commands::list_memory_cards`)
- Modify: `src-tauri/src/context_runtime/mod.rs` (`run_query`, ~line 3074; tests module)
- Modify: `src/shared/ipc/tauri.ts` (after `listMemoryCards`, ~line 1108)

**Interfaces:**
- Consumes: `partition_surfaceable`, `result_low_signal_reason`, `record_low_signal_reason`, `LowSignalReason` (Task 1)
- Produces:
  - Rust `#[tauri::command] list_needs_signal_memory_cards(limit: Option<usize>) -> Result<Vec<NeedsSignalCard>, String>`
  - Rust `pub struct NeedsSignalCard { pub card: MemoryCard, pub reason_code: String, pub reason: String }`
  - TS `export interface NeedsSignalCard { card: MemoryCard; reason_code: string; reason: string }`
  - TS `export async function listNeedsSignalMemoryCards(limit?: number): Promise<NeedsSignalCard[]>`

- [ ] **Step 1: Write the failing context-runtime test** (inside `context_runtime/mod.rs`'s existing `#[cfg(test)] mod tests`. If there's none, add one at file end with `use super::*;`)

```rust
    #[tokio::test]
    async fn drop_low_signal_hits_removes_visual_fallback_memories() {
        use crate::storage::{MemoryRecord, Store};
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Store::new(dir.path()).expect("store");
        let now = chrono::Utc::now().timestamp_millis();
        let good = MemoryRecord {
            id: "good".into(),
            timestamp: now,
            app_name: "Cursor".into(),
            window_title: "capture/mod.rs".into(),
            text: "error[E0502]: cannot borrow `self.frame_buffer` as mutable because it is also borrowed as immutable".into(),
            clean_text: "error[E0502]: cannot borrow `self.frame_buffer` as mutable because it is also borrowed as immutable".into(),
            ocr_block_count: 6,
            ocr_confidence: 0.9,
            specificity_score: 0.8,
            intent_score: 0.7,
            agent_usefulness_score: 0.75,
            evidence_confidence: 0.85,
            embedding: vec![0.0; crate::embedding::EMBEDDING_DIM],
            snippet_embedding: vec![0.0; crate::embedding::EMBEDDING_DIM],
            support_embedding: vec![0.0; crate::embedding::EMBEDDING_DIM],
            image_embedding: vec![0.0; crate::config::DEFAULT_IMAGE_EMBEDDING_DIM],
            ..Default::default()
        };
        let mut junk = good.clone();
        junk.id = "junk".into();
        junk.app_name = "Preview".into();
        junk.window_title = "IMG_4471.HEIC".into();
        junk.text = String::new();
        junk.clean_text = String::new();
        junk.ocr_block_count = 0;
        junk.ocr_confidence = 0.0;
        junk.enrichment_status = "visual_metadata_fallback".into();
        junk.synthesis_branch = "visual_metadata_fallback".into();
        store.add_batch_preserving_ids(&[good, junk]).await.expect("insert");

        let hit = |id: &str| FusedHit {
            memory_id: id.to_string(),
            score: 0.9,
            signals: Default::default(),
            surfacing_reason: Default::default(),
            contributing_routes: Vec::new(),
        };
        let kept = drop_low_signal_hits(vec![hit("good"), hit("junk"), hit("missing")], &store).await;
        let ids: Vec<&str> = kept.iter().map(|h| h.memory_id.as_str()).collect();
        assert_eq!(ids, vec!["good", "missing"]);
    }
```
If `FusionSignals`/`SurfacingReason` don't implement `Default`, build them the way the nearest existing test in `context_runtime/fusion.rs` or `verifier.rs` does. Check with `grep -n "FusionSignals {" -r src-tauri/src/context_runtime | head -3`. If `add_batch_preserving_ids` has a different signature, check `grep -n "pub async fn add_batch_preserving_ids" -A4 src-tauri/src/storage/lance_store/mod.rs`.

- [ ] **Step 2: Run to verify it fails**

Run: `cd /Users/anurupkumar/FNDR/src-tauri && CARGO_BUILD_JOBS=2 cargo test --lib drop_low_signal_hits 2>&1 | tail -8`
Expected: `cannot find function drop_low_signal_hits`.

- [ ] **Step 3: Implement `drop_low_signal_hits` and call it in `run_query`** (`context_runtime/mod.rs`)

Add near `run_query`:
```rust
/// Ask FNDR must never cite captures the read-side policy keeps out of search.
/// Unknown ids are kept; downstream evidence collection already tolerates them.
pub(crate) async fn drop_low_signal_hits(
    fused: Vec<FusedHit>,
    store: &crate::storage::Store,
) -> Vec<FusedHit> {
    let mut kept = Vec::with_capacity(fused.len());
    for hit in fused {
        match store.get_memory_by_id(&hit.memory_id).await {
            Ok(Some(record))
                if crate::memory_quality::record_low_signal_reason(&record).is_some() =>
            {
                tracing::debug!(memory_id = %hit.memory_id, "context_runtime:drop_low_signal_hit");
            }
            _ => kept.push(hit),
        }
    }
    kept
}
```
In `run_query`, change:
```rust
    let fused = fusion::fuse(&plan, route_hits.clone(), &weights);
```
to:
```rust
    let fused = fusion::fuse(&plan, route_hits.clone(), &weights);
    let fused = drop_low_signal_hits(fused, &state.store).await;
```

- [ ] **Step 4: Apply the policy in `search_memory_cards` and `list_memory_cards`** (`ipc/commands/search.rs`)

Add to imports: `use crate::memory_quality::{partition_surfaceable, LowSignalReason};` and `use serde::Serialize;` (skip if already imported).

In `search_memory_cards`, directly after the first `raw_results.truncate(raw_limit);` (the one right after `run_search_query(...).await?;`):
```rust
    let (raw_results, low_signal) = partition_surfaceable(raw_results);
    if !low_signal.is_empty() {
        tracing::info!(hidden = low_signal.len(), "search_memory_cards:low_signal_hidden");
    }
```
(Rebinding shadows the `let mut raw_results`, so keep `mut` on the new binding if later code mutates it: `let (mut raw_results, low_signal) = ...` if the compiler asks.)

Replace the body of `list_memory_cards` after fetching `results`:
```rust
    let (surfaced, _low_signal) = partition_surfaceable(strip_internal_fndr_results(results));
    let mut cards: Vec<MemoryCard> = surfaced.into_iter().map(memory_card_from_result).collect();
    refine_memory_card_titles(&mut cards);
    enrich_insight_kg_node_counts(state.store.clone(), &mut cards).await;
    Ok(cards)
```

Add the review-queue command after `list_memory_cards`:
```rust
/// A stored capture kept out of Search/Home/Vault/Ask because it lacks readable
/// signal. Listed only in the Vault's "Needs more signal" view.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct NeedsSignalCard {
    pub card: MemoryCard,
    pub reason_code: String,
    pub reason: String,
}

#[tauri::command]
pub async fn list_needs_signal_memory_cards(
    state: State<'_, Arc<AppState>>,
    limit: Option<usize>,
) -> Result<Vec<NeedsSignalCard>, String> {
    let limit = limit.unwrap_or(200).clamp(1, 1_000);
    let results = state
        .inner()
        .store
        .list_recent_results(MEMORY_GRAPH_LIMIT.max(limit), None)
        .await
        .map_err(|e| e.to_string())?;
    let (_surfaced, low_signal) = partition_surfaceable(strip_internal_fndr_results(results));
    Ok(low_signal
        .into_iter()
        .take(limit)
        .map(|(result, reason): (SearchResult, LowSignalReason)| NeedsSignalCard {
            card: memory_card_from_result(result),
            reason_code: reason.code().to_string(),
            reason: reason.user_message().to_string(),
        })
        .collect())
}
```
If `specta` isn't a direct dependency of this file's crate path (compile error), drop `specta::Type` from the derive. `MemoryCard` derives it, so it normally resolves.

- [ ] **Step 5: Export + register**

`ipc/commands/mod.rs`: add `list_needs_signal_memory_cards` and `NeedsSignalCard` to the `pub use search::{ ... }` list.
`main.rs`: in `tauri::generate_handler![ ... ]`, add `ipc::commands::list_needs_signal_memory_cards,` on the line after `ipc::commands::list_memory_cards,`.

- [ ] **Step 6: Add the TS wrapper** (`src/shared/ipc/tauri.ts`, directly after `listMemoryCards`)

```ts
export interface NeedsSignalCard {
    card: MemoryCard;
    reason_code: string;
    reason: string;
}

export async function listNeedsSignalMemoryCards(limit = 200): Promise<NeedsSignalCard[]> {
    return invoke<NeedsSignalCard[]>("list_needs_signal_memory_cards", { limit });
}
```

- [ ] **Step 7: Verify**

```bash
cd /Users/anurupkumar/FNDR/src-tauri && CARGO_BUILD_JOBS=2 cargo test --lib drop_low_signal_hits 2>&1 | tail -5 && CARGO_BUILD_JOBS=2 cargo test --lib search 2>&1 | tail -5 && CARGO_BUILD_JOBS=2 cargo check --bin fndr 2>&1 | tail -3
cd /Users/anurupkumar/FNDR && npm run typecheck
```
Expected: tests pass, `cargo check` clean, typecheck clean.

- [ ] **Step 8: Commit + push + ledger**

```bash
git add src-tauri/src/ipc/commands/search.rs src-tauri/src/ipc/commands/mod.rs src-tauri/src/main.rs src-tauri/src/context_runtime/mod.rs src/shared/ipc/tauri.ts
git commit -m "fix(search): keep low-signal captures out of search, vault, home and ask

Adds list_needs_signal_memory_cards for the Vault review queue.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push origin alpha-demo
```

---

## Task 3: Demo-profile backend hooks + stop non-demo global hotkeys (Tier 1, ~35 min)

**Files:**
- Modify: `src-tauri/src/config.rs` (append resolver)
- Modify every `app.path().app_data_dir()` call site. As of 00:39 they are: `src-tauri/src/main.rs:115`; `src-tauri/src/ipc/onboarding.rs:110,195,384,592,645,683,751,955,972`; `src-tauri/src/ipc/commands/maintenance.rs:1887,1924`; `src-tauri/src/ipc/commands/screen_guide.rs:1109`; `src-tauri/src/ipc/commands/stats.rs:223`. Lines 384/1887/1924 are split across two lines.
- Modify: `src-tauri/src/main.rs` (review backfill hook; remove Omnibar + Auto-Fill window/shortcut registration at ~lines 622–647 and the retry at ~667)

**Interfaces:**
- Produces: `pub fn fndr_app_data_dir<R: tauri::Runtime>(paths: &tauri::path::PathResolver<R>) -> tauri::Result<std::path::PathBuf>`; env vars `FNDR_DATA_DIR`, `FNDR_DEMO_REVIEW_BACKFILL=1`.

- [ ] **Step 1: Failing unit test for the override** (append to `config.rs`'s test module, or create `#[cfg(test)] mod data_dir_tests { use super::*; ... }` at file end)

```rust
    #[test]
    fn data_dir_override_reads_env_and_ignores_blank() {
        std::env::set_var("FNDR_DATA_DIR", "   ");
        assert_eq!(data_dir_override(), None);
        std::env::set_var("FNDR_DATA_DIR", "/tmp/fndr-demo-test");
        assert_eq!(
            data_dir_override(),
            Some(std::path::PathBuf::from("/tmp/fndr-demo-test"))
        );
        std::env::remove_var("FNDR_DATA_DIR");
        assert_eq!(data_dir_override(), None);
    }
```
Run: `cargo test --lib data_dir_override 2>&1 | tail -5`. Expected: fails (`data_dir_override` not found).

- [ ] **Step 2: Implement the resolver** (append to `config.rs`)

```rust
/// Optional data-directory override used for the seeded demo profile.
/// Blank values are ignored so a stray `FNDR_DATA_DIR=` never redirects data.
pub fn data_dir_override() -> Option<std::path::PathBuf> {
    std::env::var("FNDR_DATA_DIR")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
}

/// Single resolver for FNDR's app data directory. Every Tauri call site must use
/// this instead of `app.path().app_data_dir()` so a demo profile can't
/// split-brain (store in one dir, onboarding/models in another).
pub fn fndr_app_data_dir<R: tauri::Runtime>(
    paths: &tauri::path::PathResolver<R>,
) -> tauri::Result<std::path::PathBuf> {
    if let Some(dir) = data_dir_override() {
        std::fs::create_dir_all(&dir)?;
        return Ok(dir);
    }
    paths.app_data_dir()
}
```
Run the test again. Expected: PASS.

- [ ] **Step 3: Route every call site through the resolver**

```bash
cd /Users/anurupkumar/FNDR/src-tauri
grep -rn "app_data_dir()" src | grep -v "fn fndr_app_data_dir\|paths.app_data_dir()" 
```
For each hit, replace the expression `<x>.path().app_data_dir()` with `crate::config::fndr_app_data_dir(<x>.path())`. In `main.rs` (a bin, not the lib) use `fndr_lib::config::fndr_app_data_dir(app.path())`. For the split-line sites (`.path()` on one line, `.app_data_dir()` on the next), edit by hand. Re-run the grep. Expected: no remaining hits except the resolver itself. Keep existing error mapping (`.map_err(|e| e.to_string())?` still works because the return type is `tauri::Result`).

- [ ] **Step 4: Log the override at startup** (`main.rs`, directly after `let data_dir = ...;` at ~line 115)

```rust
            if fndr_lib::config::data_dir_override().is_some() {
                tracing::warn!(path = %data_dir.display(), "FNDR_DATA_DIR override active (demo profile)");
            }
```

- [ ] **Step 5: Demo review-backfill hook** (`main.rs`, in `setup` after the memory-review worker/daily scheduler is spawned. Find it with `grep -n "spawn_daily_scheduler\|memory_review" src/main.rs`. Use the same `Arc<AppState>` variable that call uses, named `state` below)

```rust
            if std::env::var("FNDR_DEMO_REVIEW_BACKFILL").ok().as_deref() == Some("1") {
                let backfill_state = state.clone();
                tauri::async_runtime::spawn(async move {
                    // Let the model finish loading before queueing review work.
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    let now_ms = chrono::Utc::now().timestamp_millis();
                    match fndr_lib::memory_review::backfill_memory_review_in_range(
                        &backfill_state,
                        &backfill_state.store,
                        0,
                        now_ms,
                        now_ms,
                        false,
                    )
                    .await
                    {
                        Ok(_) => tracing::info!("demo review backfill queued"),
                        Err(err) => tracing::warn!("demo review backfill failed: {err}"),
                    }
                });
            }
```

- [ ] **Step 6: Stop registering non-demo global surfaces** (`main.rs` ~lines 622–647 and ~667 retry)

Delete these blocks (the functions stay in the lib until Task 13):
- `ipc::commands::create_autofill_overlay_window(app.handle());`
- the `if let Err(err) = ipc::commands::register_autofill_shortcut(...) { ... } else { ... }` block and its retry block (~line 667)
- `ipc::commands::create_omnibar_window(app.handle());`
- the `if let Err(err) = ipc::commands::register_omnibar_shortcut(app.handle()) { ... } else { ... }` block

Keep: the Screen Guide overlay window + shortcut (Kunj's feature, D1).

- [ ] **Step 7: Verify**

```bash
cd /Users/anurupkumar/FNDR/src-tauri && CARGO_BUILD_JOBS=2 cargo check --bin fndr 2>&1 | grep -E "^(error|warning: unused)" | head; CARGO_BUILD_JOBS=2 cargo test --lib data_dir_override onboarding 2>&1 | tail -5
```
Expected: no errors. Fix any `unused import` warnings introduced by removed blocks.

- [ ] **Step 8: Commit + push + ledger**

```bash
git add -A src-tauri/src
git commit -m "feat(demo): FNDR_DATA_DIR profile resolver, review backfill hook, no omnibar/autofill hotkeys

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push origin alpha-demo
```

---

## Task 4: Seed corpus, seeder, one-command scripts, full Rust test, launch (Tier 1, ~75 min)

**Files:**
- Create: `scripts/demo/demo-week.json`
- Create: `src-tauri/examples/seed_demo.rs`
- Create: `scripts/demo/seed-demo-profile.sh`, `scripts/demo/run-demo.sh` (both `chmod +x`)

**Interfaces:**
- Consumes: `Store::new`, `Store::add_batch_preserving_ids`, `Store::list_recent_results`, `Embedder::new`, `Embedder::embed_batch`, `memory_quality::partition_surfaceable` (Task 1), `FNDR_DATA_DIR` (Task 3)
- Produces: demo profile at `~/Library/Application Support/com.fndr.app.demo` containing `lancedb/`, `onboarding.json`, symlinks `models`, `speech_models`.

### Corpus rules (`scripts/demo/demo-week.json`)

JSON array. Each entry:
```json
{
  "id": "demo-tue-rust-e0502",
  "day_offset": -3,
  "time": "14:12",
  "app_name": "Cursor",
  "bundle_id": "com.todesktop.230313mzl4w4u92",
  "window_title": "mod.rs — capture — fndr",
  "url": null,
  "summary": "Hit Rust error E0502 in the capture loop: frame_buffer borrowed immutably while pushing a new frame.",
  "ocr_text": "error[E0502]: cannot borrow `self.frame_buffer` as mutable because it is also borrowed as immutable\n  --> src/capture/mod.rs:412:9\n   |\n409 |         let recent = self.frame_buffer.as_slice();\n   |                      ------------------------ immutable borrow occurs here\n...\n412 |         self.frame_buffer.push(frame);\n   |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ mutable borrow occurs here\n413 |         self.dedup.compare(recent, &frame);\n   |                            ------ immutable borrow later used here",
  "session": "fndr-capture-borrow-fix",
  "low_signal": false
}
```
- `day_offset` ∈ −7…−1 **only**. Today's memories come from real live capture (D5); never seed today.
- Size: **~110 normal entries + exactly 3 `"low_signal": true` entries.** Distribution: day −1 (Thu): 22, −2 (Wed): 20, −3 (Tue): 20, −4 (Mon): 18, −5 (Sun): 6, −6 (Sat): 6, −7 (Fri): 18. Times are realistic (08:30–23:30, clustered into 30–90 min sessions of 2–5 captures sharing a `session`).
- Persona (D9): the owner (a CS capstone student at the University of Utah) building FNDR. Apps: Cursor, Google Chrome, Slack, Google Docs (in Chrome), Terminal, Figma, Notion, Zoom, Preview, Mail, Spotify (1–2 only). Domains: `capstone.cs.utah.edu` (GitLab issues/MRs), `utah.instructure.com` (Canvas CS 4500), `docs.google.com`, `github.com`, `stackoverflow.com`, `developer.apple.com`, `docs.rs`, `lancedb.github.io`, `figma.com`, `delta.com` (one trip booking).
- People: **fictional only**: Jordan Lee (teammate-like study partner on Slack), Prof. Rivera (instructor), Sam Patel (TA). Never write messages attributed to Felipe, Kunj, Minh, or any real person.
- **No finance/banking content at all** (the D23 refusal depends on it), no passwords, emails, phone numbers, or addresses. The Delta booking shows only the route/time/confirmation code `HX7Q2P`.
- `ocr_text` must look like real screen text (code, error output, doc paragraphs, chat lines, page chrome), 150–900 chars, never meta-narration ("The screen shows…"). `summary` is one plain sentence (≤ 160 chars) and must differ from `window_title`.

**Required anchor entries (write these exactly; filler goes around them):**

| id | day/time | app | window_title / url | summary | ocr_text must contain |
|---|---|---|---|---|---|
| `demo-tue-rust-e0502` | −3 14:12 | Cursor | `mod.rs — capture — fndr` | (as example above) | the full E0502 block above |
| `demo-tue-rust-e0502-fix` | −3 14:41 | Cursor | `mod.rs — capture — fndr` | Fixed E0502 by copying the recent frames into a Vec before pushing; capture tests pass. | `let recent: Vec<Frame> = self.frame_buffer.iter().rev().take(4).cloned().collect();` and `test result: ok. 41 passed; 0 failed` |
| `demo-tue-so-borrow` | −3 14:20 | Google Chrome | `rust - cannot borrow as mutable because it is also borrowed as immutable - Stack Overflow` / `https://stackoverflow.com/questions/47618823` | Read a Stack Overflow answer on mutable/immutable borrow conflicts. | `Clone the data you need before taking the mutable borrow, or restructure so the immutable borrow ends first.` |
| `demo-wed-chunking-paper` | −2 10:05 | Preview | `Moreno2026_ChunkingForRAG.pdf (page 7 of 14)` | Read the chunking paper's results: 512-token parents with 128-token children and 32-token overlap gave the best recall. | `Table 3: Parent–child chunking with 512-token parents, 128-token children and a 32-token overlap achieved the highest recall@10 (0.81), outperforming fixed 256-token chunks (0.69).` and `We therefore recommend 512/128 parent–child chunking for personal-memory retrieval.` |
| `demo-wed-design-doc` | −2 11:30 | Google Chrome | `FNDR Alpha Design Doc — Retrieval - Google Docs` / `https://docs.google.com/document/d/fndr-alpha-design/edit` | Updated the design doc to adopt 512/128 parent–child chunking from the Moreno paper. | `Decision: adopt 512-token parent / 128-token child chunks (Moreno et al. 2026, Table 3). Child chunks embed with BGE (1024-d); parents stay in the MiniLM 384-d table.` |
| `demo-thu-rubric` | −1 09:15 | Google Chrome | `Alpha Release Progress: CS 4500 — Canvas` / `https://utah.instructure.com/courses/1234/assignments/5678` | Reviewed the CS 4500 Alpha Release rubric and what the demo must show. | `Most/all rank 1 features complete; Framework for Rank 2` and `identify what has changed between the prototype and the current state` |
| `demo-thu-issue-36` | −1 13:40 | Google Chrome | `Change card headers and summaries to not be the same thing (#36) · Issues · FNDR · GitLab` / `https://capstone.cs.utah.edu/fndr/fndr/-/issues/36` | Worked on GitLab issue #36: card titles and summaries were duplicating each other. | `Change card headers and summaries to not be the same thing or even vague in some cases` |
| `demo-thu-dryrun-slack` | −1 16:05 | Slack | `#fndr-team — FNDR Capstone — Slack` | Jordan moved the alpha dry run to Thursday 4pm in MEB 3147. | `Jordan Lee  4:02 PM` / `can we move the alpha dry run to thursday 4pm? I booked MEB 3147` / `👍 works for me` |
| `demo-mon-delta` | −4 20:10 | Google Chrome | `Trip Confirmation - Delta Air Lines` / `https://www.delta.com/mytrips/` | Booked a Delta flight SLC → SFO for the career fair; confirmation HX7Q2P. | `Confirmation #: HX7Q2P` and `SLC → SFO  Fri Oct 2  7:05 AM – 8:21 AM` |
| `demo-low-img-1` | −1 22:14 | Preview | `IMG_4471.HEIC` | "" | `"ocr_text": ""`, `"low_signal": true` |
| `demo-low-img-2` | −2 21:03 | ChatGPT | `ChatGPT` | "" | `"ocr_text": "ChatGPT:"`, `"low_signal": true` |
| `demo-low-img-3` | −4 18:47 | Photos | `Photos` | "" | `"ocr_text": ""`, `"low_signal": true` |

Filler themes per day (vary apps and sessions; each has concrete, searchable details):
- **−7 Fri:** sprint kickoff notes in Notion; GitLab board triage (issues #30–#39 titles); LanceDB docs on vector index (`IVF_PQ`, `num_partitions`); Figma home screen iteration.
- **−6 Sat / −5 Sun (light):** Spotify playlist, reading docs.rs `tokio::sync::Mutex`, a recipe page, planning career fair resume in Google Docs, a YouTube talk "Local-first software".
- **−4 Mon:** ONNX MiniLM tokenizer bug (`token_type_ids` missing), `cargo test` output, Zoom standup with Prof. Rivera's feedback (fictional quotes about scope), Delta booking (anchor).
- **−3 Tue:** E0502 session (anchors), hybrid search reranker tuning (`rerank_results`, RRF k=60), Canvas quiz on distributed systems.
- **−2 Wed:** chunking paper + design doc (anchors), BGE embedding latency notes (`412 ms cold, 38 ms warm`), Terminal `tauri dev` build output, Slack with Sam Patel about lab hours.
- **−1 Thu:** rubric (anchor), issue #36 (anchor), dry-run Slack (anchor), Vitest run output (`Test Files 33 passed`), Figma card redesign, privacy blocklist code review, evening Apple docs on ScreenCaptureKit.

- [ ] **Step 1: Write the corpus**, then validate it

```bash
cd /Users/anurupkumar/FNDR && python3 - <<'PY'
import json, collections
d = json.load(open("scripts/demo/demo-week.json"))
ids = [e["id"] for e in d]
assert len(ids) == len(set(ids)), "duplicate ids"
assert all(-7 <= e["day_offset"] <= -1 for e in d), "day_offset out of range"
low = [e for e in d if e.get("low_signal")]
assert len(low) == 3, f"expected 3 low_signal, got {len(low)}"
for anchor in ["demo-tue-rust-e0502","demo-tue-rust-e0502-fix","demo-tue-so-borrow","demo-wed-chunking-paper","demo-wed-design-doc","demo-thu-rubric","demo-thu-issue-36","demo-thu-dryrun-slack","demo-mon-delta"]:
    assert anchor in ids, anchor
banned = ["bank", "balance", "chase", "wells fargo", "venmo", "password", "felipe", "kunj", "minh", "the screen shows", "the ocr"]
for e in d:
    blob = (e["summary"] + " " + e["ocr_text"] + " " + e["window_title"]).lower()
    for b in banned:
        assert b not in blob, f"{e['id']} contains banned '{b}'"
    if not e.get("low_signal"):
        assert 150 <= len(e["ocr_text"]) <= 900, f"{e['id']} ocr_text length {len(e['ocr_text'])}"
        assert e["summary"].strip() and e["summary"].strip() != e["window_title"].strip(), e["id"]
print(len(d), "entries;", collections.Counter(e["day_offset"] for e in d))
PY
```
Expected: `113 entries; Counter(...)` (±5) with no assertion errors.

- [ ] **Step 2: Write the seeder** `src-tauri/examples/seed_demo.rs`

`source_type` literals match the live capture path (`src-tauri/src/capture/mod.rs:3149`: `"browser"` when a URL is present, else `"screen"`). Quarantine hard gates only fire on `raw_evidence` extraction issues, so seeded records with empty `raw_evidence` classify normally.

```rust
//! Seeds the FNDR alpha demo profile from scripts/demo/demo-week.json through the
//! real Store insert path (normalization + storage-outcome classification).
//! Usage: cargo run --example seed_demo -- --data-dir <dir> --corpus <json>
//! Refuses to write into the real profile (com.fndr.app).

use chrono::{Duration as ChronoDuration, Local, NaiveTime, TimeZone};
use fndr_lib::config::DEFAULT_IMAGE_EMBEDDING_DIM;
use fndr_lib::embedding::{Embedder, EMBEDDING_DIM};
use fndr_lib::memory_quality::partition_surfaceable;
use fndr_lib::storage::{MemoryRecord, Store};
use serde::Deserialize;
use std::path::PathBuf;

const SOURCE_TYPE_URL: &str = "browser";
const SOURCE_TYPE_SCREEN: &str = "screen";

#[derive(Deserialize)]
struct SeedEntry {
    id: String,
    day_offset: i64,
    time: String,
    app_name: String,
    #[serde(default)]
    bundle_id: Option<String>,
    window_title: String,
    #[serde(default)]
    url: Option<String>,
    summary: String,
    ocr_text: String,
    session: String,
    #[serde(default)]
    low_signal: bool,
}

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn timestamp_ms(day_offset: i64, time: &str) -> i64 {
    let t = NaiveTime::parse_from_str(time, "%H:%M").expect("time HH:MM");
    let date = Local::now().date_naive() + ChronoDuration::days(day_offset);
    Local
        .from_local_datetime(&date.and_time(t))
        .single()
        .expect("unambiguous local time")
        .timestamp_millis()
}

fn record(entry: &SeedEntry, embedding: Vec<f32>) -> MemoryRecord {
    let ts = timestamp_ms(entry.day_offset, &entry.time);
    let day_bucket = Local
        .timestamp_millis_opt(ts)
        .single()
        .expect("ts")
        .format("%Y-%m-%d")
        .to_string();
    let lines = entry.ocr_text.lines().filter(|l| !l.trim().is_empty()).count() as u32;
    let mut r = MemoryRecord {
        id: entry.id.clone(),
        timestamp: ts,
        timestamp_start: ts,
        timestamp_end: ts + 45_000,
        day_bucket,
        app_name: entry.app_name.clone(),
        bundle_id: entry.bundle_id.clone(),
        window_title: entry.window_title.clone(),
        session_id: entry.session.clone(),
        session_key: entry.session.clone(),
        text: entry.ocr_text.clone(),
        clean_text: entry.ocr_text.clone(),
        snippet: entry.summary.clone(),
        display_summary: entry.summary.clone(),
        memory_context: entry.summary.clone(),
        summary_source: "demo_seed".to_string(),
        synthesis_branch: "demo_seed".to_string(),
        source_type: if entry.url.is_some() { SOURCE_TYPE_URL } else { SOURCE_TYPE_SCREEN }.to_string(),
        url: entry.url.clone(),
        ocr_confidence: 0.93,
        ocr_block_count: lines.max(1),
        noise_score: 0.04,
        embedding: embedding.clone(),
        snippet_embedding: embedding.clone(),
        support_embedding: embedding,
        image_embedding: vec![0.0; DEFAULT_IMAGE_EMBEDDING_DIM],
        decay_score: 1.0,
        specificity_score: 0.78,
        intent_score: 0.68,
        entity_score: 0.62,
        agent_usefulness_score: 0.72,
        evidence_confidence: 0.84,
        retrieval_value_score: 0.74,
        ocr_noise_score: 0.05,
        enrichment_status: "pending".to_string(),
        raw_screenshot_stored: false,
        ..Default::default()
    };
    if entry.low_signal {
        r.snippet = String::new();
        r.display_summary = String::new();
        r.memory_context = String::new();
        r.ocr_block_count = if entry.ocr_text.trim().is_empty() { 0 } else { 1 };
        r.ocr_confidence = if entry.ocr_text.trim().is_empty() { 0.0 } else { 0.4 };
        r.enrichment_status = "visual_metadata_fallback".to_string();
        r.synthesis_branch = "visual_metadata_fallback".to_string();
        r.embedding = vec![0.0; EMBEDDING_DIM];
        r.snippet_embedding = vec![0.0; EMBEDDING_DIM];
        r.support_embedding = vec![0.0; EMBEDDING_DIM];
    }
    r
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(arg("--data-dir").expect("--data-dir required"));
    let corpus = PathBuf::from(arg("--corpus").expect("--corpus required"));
    let real = dirs::data_dir().map(|d| d.join("com.fndr.app"));
    if Some(data_dir.canonicalize().unwrap_or(data_dir.clone())) == real.as_ref().map(|r| r.canonicalize().unwrap_or(r.clone())) {
        return Err("refusing to seed the real FNDR profile".into());
    }
    std::fs::create_dir_all(&data_dir)?;

    let entries: Vec<SeedEntry> = serde_json::from_slice(&std::fs::read(&corpus)?)?;
    let expected_low = entries.iter().filter(|e| e.low_signal).count();
    let embedder = Embedder::new()?;

    let mut records = Vec::with_capacity(entries.len());
    for chunk in entries.chunks(16) {
        let texts: Vec<String> = chunk
            .iter()
            .map(|e| format!("{}\n{}\n{}", e.window_title, e.summary, e.ocr_text))
            .collect();
        let vectors = embedder.embed_batch(&texts)?;
        for (entry, vector) in chunk.iter().zip(vectors) {
            records.push(record(entry, vector));
        }
    }

    let store = Store::new(&data_dir)?;
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async { store.add_batch_preserving_ids(&records).await })?;
    let listed = rt.block_on(async { store.list_recent_results(1_000, None).await })?;
    let (surfaced, hidden) = partition_surfaceable(listed);
    println!(
        "seeded {} records into {} — surfaced {}, needs-signal {}",
        records.len(),
        data_dir.display(),
        surfaced.len(),
        hidden.len()
    );
    if hidden.len() != expected_low {
        return Err(format!(
            "quality gate mismatch: expected {expected_low} needs-signal records, got {}",
            hidden.len()
        )
        .into());
    }
    Ok(())
}
```
Adjust to reality if the compiler disagrees: `embed_batch` may return `Result<_, String>` (map with `.map_err(|e| e.into())` or `?` on a `String` via `Box<dyn Error>` works), `add_batch_preserving_ids` may return a count (ignore it with `.map(|_| ())`), and `dirs` must be a dependency (it is: `models.rs` uses `dirs::data_dir()`). If `surfaced.len() + hidden.len() < records.len()`, insert-time dedup merged entries. Make those entries' `ocr_text` more distinct and re-seed.

- [ ] **Step 3: Write the scripts**

`scripts/demo/seed-demo-profile.sh`:
```bash
#!/usr/bin/env bash
# Builds the seeded alpha-demo profile. Never touches the real com.fndr.app data.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
REAL="$HOME/Library/Application Support/com.fndr.app"
DEMO="${FNDR_DEMO_DIR:-$HOME/Library/Application Support/com.fndr.app.demo}"

if [[ "${1:-}" == "--reset" && -d "$DEMO" ]]; then
  mv "$DEMO" "$HOME/.Trash/com.fndr.app.demo.$(date +%Y%m%d-%H%M%S)"
elif [[ -d "$DEMO/lancedb" ]]; then
  echo "Demo profile already exists at $DEMO (use --reset to rebuild)"; exit 0
fi

mkdir -p "$DEMO"
[[ -d "$REAL/models" ]] || { echo "Missing $REAL/models — install models via the app first"; exit 1; }
ln -sfn "$REAL/models" "$DEMO/models"
[[ -d "$REAL/speech_models" ]] && ln -sfn "$REAL/speech_models" "$DEMO/speech_models"

NAME="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("display_name") or "Anurup")' "$REAL/onboarding.json" 2>/dev/null || echo Anurup)"
cat > "$DEMO/onboarding.json" <<JSON
{
  "step": "complete",
  "biometric_enabled": false,
  "screen_permission": true,
  "accessibility_permission": true,
  "model_downloaded": true,
  "model_id": "qwen3-vl-2b",
  "display_name": "$NAME"
}
JSON

cd "$REPO/src-tauri"
CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo run --example seed_demo -- \
  --data-dir "$DEMO" --corpus "$REPO/scripts/demo/demo-week.json"
echo "Demo profile ready: $DEMO"
```

`scripts/demo/run-demo.sh`:
```bash
#!/usr/bin/env bash
# Launches FNDR on the seeded demo profile. Set FNDR_DEMO_REVIEW_BACKFILL=1 once
# (overnight) so the on-device review worker reviews the seeded week.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/../.." && pwd)"
DEMO="${FNDR_DEMO_DIR:-$HOME/Library/Application Support/com.fndr.app.demo}"
[[ -d "$DEMO/lancedb" ]] || "$REPO/scripts/demo/seed-demo-profile.sh"
cd "$REPO"
export FNDR_DATA_DIR="$DEMO"
export FNDR_DEMO_REVIEW_BACKFILL="${FNDR_DEMO_REVIEW_BACKFILL:-0}"
exec npm run tauri dev
```

- [ ] **Step 4: Seed**

```bash
cd /Users/anurupkumar/FNDR && chmod +x scripts/demo/*.sh && ./scripts/demo/seed-demo-profile.sh --reset 2>&1 | tail -5
ls -la "$HOME/Library/Application Support/com.fndr.app.demo"
```
Expected: `seeded 113 records … surfaced 110, needs-signal 3`, plus a listing with `lancedb`, `onboarding.json`, and the `models ->` symlink. **Verify the real profile is unchanged:** `ls -la "$HOME/Library/Application Support/com.fndr.app/lancedb" | head -3` shows timestamps ≤ Sep 17 23:03.

- [ ] **Step 5: Full Rust + frontend suite** (app is stopped)

```bash
cd /Users/anurupkumar/FNDR && CARGO_BUILD_JOBS=2 make test 2>&1 | tail -25
```
Expected: typecheck ✅, Vitest ✅, `cargo test` ✅ (known-ignored tests stay ignored). Record counts in the ledger. If a failure is in code this plan touched, fix it before continuing. If it's pre-existing and unrelated, log it verbatim and continue.

- [ ] **Step 6: Commit + push**

```bash
git add scripts/demo src-tauri/examples/seed_demo.rs
git commit -m "feat(demo): seeded alpha demo week, seeder with quality-gate check, run scripts

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push origin alpha-demo
```

- [ ] **Step 7: Launch the demo profile with review backfill** (background, keep running for Tasks 5–11)

```bash
cd /Users/anurupkumar/FNDR && FNDR_DEMO_REVIEW_BACKFILL=1 ./scripts/demo/run-demo.sh > /tmp/fndr-demo-dev.log 2>&1
```
(Run with `run_in_background: true`.) Wait until the log shows the app booted (`grep -m1 "Consolidated store initialized" /tmp/fndr-demo-dev.log`; the path must be `…com.fndr.app.demo`), then `grep "FNDR_DATA_DIR override active\|demo review backfill" /tmp/fndr-demo-dev.log`.
Self-QA: `screencapture -x /tmp/fndr-qa-t4.png` → Read it. Expected: Home with greeting and no Touch ID lock. If the onboarding screen appears instead, the resolver missed a call site. Go back to Task 3 Step 3.

---

## Task 5: Ask FNDR panel on `fndr_answer` (Tier 1, ~35 min)

**Files:**
- Create: `src/domains/ask/AskPanel.tsx`, `src/domains/ask/AskPanel.css`, `src/domains/ask/AskPanel.test.tsx`

**Interfaces:**
- Consumes: `fndrAnswer(query: string, limit?: number): Promise<ComposedAnswer>`, `ComposedAnswer`, `MemoryCard` from `@/shared/ipc/tauri`
- Produces: `export function AskPanel(props: { isVisible: boolean; onClose: () => void; onOpenMemoryById: (id: string) => void }): JSX.Element | null`

- [ ] **Step 1: Write the failing test** `src/domains/ask/AskPanel.test.tsx`

```tsx
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { AskPanel } from "./AskPanel";
import { fndrAnswer, type ComposedAnswer, type MemoryCard } from "@/shared/ipc/tauri";

vi.mock("@/shared/ipc/tauri", () => ({ fndrAnswer: vi.fn() }));

const source: MemoryCard = {
    id: "demo-wed-chunking-paper",
    title: "Chunking paper results",
    summary: "512-token parents with 128-token children gave the best recall.",
    action: "",
    context: [],
    timestamp: Date.now() - 86_400_000,
    app_name: "Preview",
    window_title: "Moreno2026_ChunkingForRAG.pdf",
    score: 0.9,
    source_count: 1,
    raw_snippets: [],
};

function answer(overrides: Partial<ComposedAnswer>): ComposedAnswer {
    return {
        query: "q",
        answer: "The paper recommends 512-token parents with 128-token children.",
        evidence: { files: [], commands: [], errors: [], decisions: [], todos: [], urls: [] } as unknown as ComposedAnswer["evidence"],
        cards: [source],
        verify_outcome: { kind: "grounded", confidence: 0.82 },
        surfacing_reasons: [],
        ...overrides,
    };
}

afterEach(() => {
    cleanup();
    vi.clearAllMocks();
});

describe("AskPanel", () => {
    it("shows a grounded answer with clickable cited sources", async () => {
        vi.mocked(fndrAnswer).mockResolvedValue(answer({}));
        const onOpen = vi.fn();
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={onOpen} />);
        fireEvent.change(screen.getByLabelText("Ask FNDR a question"), {
            target: { value: "What chunk size did the chunking paper recommend?" },
        });
        fireEvent.click(screen.getByRole("button", { name: "Ask" }));
        expect(await screen.findByText(/512-token parents with 128-token children\./)).toBeInTheDocument();
        expect(screen.getByText("Grounded in 1 memory")).toBeInTheDocument();
        fireEvent.click(screen.getByRole("button", { name: /Chunking paper results/ }));
        expect(onOpen).toHaveBeenCalledWith("demo-wed-chunking-paper");
    });

    it("shows an honest refusal when evidence is missing", async () => {
        vi.mocked(fndrAnswer).mockResolvedValue(
            answer({
                answer: "I don't have enough grounded evidence to answer that yet.",
                cards: [],
                verify_outcome: { kind: "not_enough_evidence", reason: "no hits" },
            })
        );
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);
        fireEvent.change(screen.getByLabelText("Ask FNDR a question"), {
            target: { value: "What's my bank balance?" },
        });
        fireEvent.click(screen.getByRole("button", { name: "Ask" }));
        expect(await screen.findByText("Not in your memories")).toBeInTheDocument();
        expect(screen.getByText(/only answers from what it captured on this Mac/)).toBeInTheDocument();
    });

    it("does not call the backend for a blank question", () => {
        render(<AskPanel isVisible onClose={() => {}} onOpenMemoryById={() => {}} />);
        fireEvent.click(screen.getByRole("button", { name: "Ask" }));
        expect(fndrAnswer).not.toHaveBeenCalled();
    });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd /Users/anurupkumar/FNDR && npx vitest run src/domains/ask/AskPanel.test.tsx 2>&1 | tail -6`
Expected: FAIL (`Failed to resolve import "./AskPanel"`).

- [ ] **Step 3: Implement** `src/domains/ask/AskPanel.tsx`

```tsx
import { useEffect, useRef, useState } from "react";
import { fndrAnswer, type ComposedAnswer, type MemoryCard } from "@/shared/ipc/tauri";
import "./AskPanel.css";

const ANSWER_TIMEOUT_MS = 60_000;
const TAKEAWAY =
    "FNDR remembers what you did on your Mac, privately and on-device, and answers questions about it with cited evidence.";
const EXAMPLES = [
    "What was the Rust borrow error I fixed this week?",
    "What chunk size did the chunking paper recommend?",
    "When is the alpha dry run?",
];

type AskState =
    | { kind: "idle" }
    | { kind: "asking"; question: string }
    | { kind: "answer"; question: string; answer: ComposedAnswer }
    | { kind: "error"; question: string; message: string };

interface AskPanelProps {
    isVisible: boolean;
    onClose: () => void;
    onOpenMemoryById: (id: string) => void;
}

function outcomeLabel(answer: ComposedAnswer): string {
    const outcome = answer.verify_outcome;
    if (outcome.kind === "not_enough_evidence") return "Not in your memories";
    if (outcome.kind === "partial_answer") return "Partial answer";
    const n = answer.cards.length;
    return `Grounded in ${n} ${n === 1 ? "memory" : "memories"}`;
}

function sourceTime(card: MemoryCard): string {
    const d = new Date(card.timestamp);
    return `${d.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" })} · ${d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}`;
}

export function AskPanel({ isVisible, onClose, onOpenMemoryById }: AskPanelProps) {
    const [draft, setDraft] = useState("");
    const [state, setState] = useState<AskState>({ kind: "idle" });
    const seq = useRef(0);
    const inputRef = useRef<HTMLTextAreaElement>(null);

    useEffect(() => {
        if (isVisible) window.setTimeout(() => inputRef.current?.focus(), 30);
    }, [isVisible]);

    if (!isVisible) return null;

    const ask = async (text: string) => {
        const question = text.trim();
        if (!question) return;
        const id = ++seq.current;
        setState({ kind: "asking", question });
        try {
            const result = await Promise.race([
                fndrAnswer(question),
                new Promise<never>((_, reject) =>
                    window.setTimeout(() => reject(new Error("timeout")), ANSWER_TIMEOUT_MS)
                ),
            ]);
            if (id === seq.current) setState({ kind: "answer", question, answer: result });
        } catch (err) {
            if (id !== seq.current) return;
            const timedOut = err instanceof Error && err.message === "timeout";
            setState({
                kind: "error",
                question,
                message: timedOut
                    ? "FNDR took too long to answer. Try a shorter question."
                    : "FNDR couldn't answer right now. Try again in a moment.",
            });
        }
    };

    return (
        <div className="ask-page" role="dialog" aria-label="Ask FNDR">
            <header className="ask-header">
                <div className="ask-header-title">
                    <h2>Ask FNDR</h2>
                    <span className="ask-badge">On-device · read-only</span>
                </div>
                <button type="button" className="ui-action-btn ask-close-btn" onClick={onClose} aria-label="Close Ask FNDR">
                    ×
                </button>
            </header>

            <main className="ask-body">
                <p className="ask-takeaway">{TAKEAWAY}</p>

                <form
                    className="ask-form"
                    onSubmit={(event) => {
                        event.preventDefault();
                        void ask(draft);
                    }}
                >
                    <textarea
                        ref={inputRef}
                        className="ask-input"
                        aria-label="Ask FNDR a question"
                        placeholder="Ask about anything you've worked on…"
                        value={draft}
                        rows={2}
                        onChange={(event) => setDraft(event.target.value)}
                        onKeyDown={(event) => {
                            if (event.key === "Enter" && !event.shiftKey) {
                                event.preventDefault();
                                void ask(draft);
                            }
                        }}
                    />
                    <button type="submit" className="ui-action-btn btn-primary ask-submit" disabled={state.kind === "asking"}>
                        Ask
                    </button>
                </form>

                {state.kind === "idle" && (
                    <div className="ask-examples" aria-label="Example questions">
                        {EXAMPLES.map((example) => (
                            <button
                                key={example}
                                type="button"
                                className="ask-example"
                                onClick={() => {
                                    setDraft(example);
                                    void ask(example);
                                }}
                            >
                                {example}
                            </button>
                        ))}
                    </div>
                )}

                {state.kind === "asking" && (
                    <div className="ask-status" role="status">
                        <div className="thinking-loader" aria-hidden="true" />
                        <span>Searching your memories…</span>
                    </div>
                )}

                {state.kind === "error" && (
                    <div className="ask-result ask-result--error" role="alert">
                        <p>{state.message}</p>
                    </div>
                )}

                {state.kind === "answer" && (
                    <section className="ask-result" aria-live="polite">
                        <div className={`ask-outcome ask-outcome--${state.answer.verify_outcome.kind}`}>
                            {outcomeLabel(state.answer)}
                        </div>
                        <p className="ask-answer">{state.answer.answer}</p>
                        {state.answer.verify_outcome.kind === "not_enough_evidence" && (
                            <p className="ask-hint">FNDR only answers from what it captured on this Mac.</p>
                        )}
                        {state.answer.cards.length > 0 && (
                            <div className="ask-sources">
                                <h3>Sources</h3>
                                <ul>
                                    {state.answer.cards.slice(0, 5).map((card) => (
                                        <li key={card.id}>
                                            <button
                                                type="button"
                                                className="ask-source"
                                                onClick={() => onOpenMemoryById(card.id)}
                                            >
                                                <span className="ask-source-title">{card.title}</span>
                                                <span className="ask-source-meta">
                                                    {card.app_name} · {sourceTime(card)}
                                                </span>
                                            </button>
                                        </li>
                                    ))}
                                </ul>
                            </div>
                        )}
                    </section>
                )}
            </main>
        </div>
    );
}
```

- [ ] **Step 4: Styles** `src/domains/ask/AskPanel.css`. This mirrors the Daily Summary overlay shell and uses only existing tokens.

```css
.ask-page {
    position: fixed;
    inset: 0;
    background: var(--panel-bg);
    backdrop-filter: var(--panel-backdrop);
    -webkit-backdrop-filter: var(--panel-backdrop);
    z-index: 1050;
    display: flex;
    flex-direction: column;
    animation: panel-slide-in 0.3s cubic-bezier(0.16, 1, 0.3, 1);
}
.ask-header {
    flex-shrink: 0;
    padding: 14px 16px;
    border-bottom: var(--header-border);
    display: flex;
    align-items: center;
    justify-content: space-between;
}
.ask-header-title { display: flex; align-items: center; gap: 10px; }
.ask-header h2 { margin: 0; font-size: 16px; color: var(--text-primary); }
.ask-badge {
    font-size: 11px;
    letter-spacing: 0.06em;
    padding: 3px 10px;
    border-radius: var(--film-radius-pill);
    border: 1px solid var(--border);
    color: var(--text-secondary);
}
.ask-close-btn {
    padding: 6px 12px; background: transparent; border: none; cursor: pointer;
    color: var(--text-secondary); border-radius: var(--film-radius-md); font-size: 18px;
}
.ask-close-btn:hover { color: var(--text-primary); background: var(--surface-hover); }
.ask-body {
    flex: 1; overflow-y: auto; width: min(760px, 100% - 32px); margin: 0 auto;
    padding: 40px 0 64px; display: flex; flex-direction: column; gap: 18px;
}
.ask-takeaway { margin: 0; color: var(--text-secondary); font-size: 15px; line-height: 1.5; }
.ask-form { display: flex; gap: 10px; align-items: stretch; }
.ask-input {
    flex: 1; resize: none; padding: 14px 16px; font: inherit; font-size: 16px;
    color: var(--text-primary); background: var(--surface-translucent);
    border: 1px solid var(--border); border-radius: var(--film-radius-lg);
}
.ask-input:focus { outline: none; border-color: var(--accent); }
.ask-submit { min-width: 88px; border-radius: var(--film-radius-lg); }
.ask-examples { display: flex; flex-wrap: wrap; gap: 8px; }
.ask-example {
    padding: 8px 14px; font: inherit; font-size: 13px; cursor: pointer;
    color: var(--text-secondary); background: transparent;
    border: 1px solid var(--border); border-radius: var(--film-radius-pill);
}
.ask-example:hover { color: var(--text-primary); border-color: var(--border-strong); background: var(--surface-hover); }
.ask-status { display: flex; align-items: center; gap: 12px; color: var(--text-secondary); }
.ask-result {
    padding: 20px 22px; background: var(--surface-elevated);
    border: 1px solid var(--border); border-radius: var(--film-radius-lg);
    display: flex; flex-direction: column; gap: 12px;
}
.ask-result--error { color: var(--danger); }
.ask-outcome {
    align-self: flex-start; font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase;
    padding: 3px 10px; border-radius: var(--film-radius-pill); border: 1px solid var(--accent); color: var(--accent);
}
.ask-outcome--not_enough_evidence, .ask-outcome--partial_answer { border-color: var(--border-strong); color: var(--text-secondary); }
.ask-answer { margin: 0; font-size: 17px; line-height: 1.6; color: var(--text-primary); white-space: pre-wrap; }
.ask-hint { margin: 0; font-size: 13px; color: var(--text-secondary); }
.ask-sources h3 { margin: 4px 0 8px; font-size: 12px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-secondary); }
.ask-sources ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
.ask-source {
    width: 100%; text-align: left; display: flex; flex-direction: column; gap: 2px; cursor: pointer;
    padding: 10px 12px; font: inherit; background: transparent; color: var(--text-primary);
    border: 1px solid var(--border); border-radius: var(--film-radius-md);
}
.ask-source:hover { background: var(--surface-hover); border-color: var(--border-strong); }
.ask-source-title { font-size: 14px; }
.ask-source-meta { font-size: 12px; color: var(--text-secondary); }
```

- [ ] **Step 5: Run tests**

Run: `npx vitest run src/domains/ask/AskPanel.test.tsx && npm run typecheck`
Expected: 3 passed; typecheck clean.

- [ ] **Step 6: Commit + push**

```bash
git add src/domains/ask
git commit -m "feat(ask): Ask FNDR panel with grounded answer, cited sources and honest refusal

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push origin alpha-demo
```

---

## Task 6: Curated demo surface: sidebar, palette, panel mounts (Tier 1, ~40 min)

**Files:**
- Modify: `src/app/App.tsx` (`SIDEBAR_GROUPS` ~line 45, sidebar render ~line 530, `AppPanels` props ~line 657; remove `useAutomationScheduler`)
- Rewrite: `src/app/AppPanels.tsx`
- Modify: `src/domains/command-palette/CommandPalette.tsx` + `CommandPalette.test.ts`

**Interfaces:**
- Consumes: `AskPanel` (Task 5); `DailySummaryPanel({ isVisible, onClose, onOpenMemoryById })` (Felipe, origin/main); `FndrWrappedPanel({ isVisible, onClose })`; `ScreenGuidePanel({ isVisible, onClose })`
- Produces: `PanelKey` without `"agent" | "engineMetrics" | "pipeline" | "glassesImport" | "quickSkills" | "automation" | "research"`, plus `"ask"`. `CommandContext` gains `onGoHome: () => void` and loses `onResearch`. `DEMO_COMMAND_IDS = ["go-home","memory-cards","ask-fndr","daily-summary","wrapped","screen-guide","pause-capture","resume-capture"]`.

- [ ] **Step 1: Update the failing palette test** (`src/domains/command-palette/CommandPalette.test.ts`)

```ts
import { describe, expect, it } from "vitest";
import { DEMO_COMMAND_IDS, isDemoCommand } from "./CommandPalette";

describe("demo command palette", () => {
    it("exposes exactly the alpha demo destinations and capture controls", () => {
        expect(DEMO_COMMAND_IDS).toEqual([
            "go-home",
            "memory-cards",
            "ask-fndr",
            "daily-summary",
            "wrapped",
            "screen-guide",
            "pause-capture",
            "resume-capture",
        ]);
        expect(isDemoCommand("engine-metrics")).toBe(false);
        expect(isDemoCommand("local-context")).toBe(false);
        expect(isDemoCommand("automation")).toBe(false);
    });
});
```
Run: `npx vitest run src/domains/command-palette` → FAIL.

- [ ] **Step 2: Update `CommandPalette.tsx`**
  1. `PanelKey`: remove `"pipeline" | "engineMetrics" | "glassesImport" | "quickSkills" | "agent" | "automation" | "research"` and add `"ask"`.
  2. `CommandContext`: remove `onResearch`, add `onGoHome: () => void`.
  3. `DEMO_COMMAND_IDS`: the list from Step 1.
  4. `COMMANDS`: **remove** the objects with ids `quick-skills`, `automation`, `pipeline`, `engine-metrics`, `glasses-photo-import`, `import-meta-glasses-photo`, `research-memory`, `local-context`, `agent-analyze`. Remove the `startAgentTask` import. **Add** at the top of the Navigate group:
  ```ts
    {
        id: "go-home",
        label: "Home",
        description: "Back to your greeting, capture status and recent memories",
        category: "navigate",
        keywords: ["home", "start", "timeline", "recent"],
        run: ({ onGoHome }) => onGoHome(),
    },
    {
        id: "ask-fndr",
        label: "Ask FNDR",
        description: "Get an answer with cited evidence from your local memories",
        category: "navigate",
        keywords: ["ask", "question", "answer", "context"],
        run: ({ onOpenPanel }) => onOpenPanel("ask"),
    },
    {
        id: "wrapped",
        label: "FNDR Wrapped",
        description: "Your week in review",
        category: "navigate",
        keywords: ["week", "recap", "wrapped", "review"],
        run: ({ onOpenPanel }) => onOpenPanel("wrapped"),
    },
  ```
  Confirm the existing `memory-cards`, `daily-summary`, and `screen-guide` commands call `onOpenPanel("memoryCards" | "dailySummary" | "screenGuide")`.
  5. Input placeholder: find `find a memory` and change it to `Jump to a view or command…`. Find the footer text `LOCAL ONLY` and keep it.

- [ ] **Step 3: Rewrite `src/app/AppPanels.tsx`**

```tsx
import type { MemoryCard } from "@/shared/ipc/tauri";
import { AskPanel } from "@/domains/ask/AskPanel";
import { CommandPalette, type PanelKey } from "@/domains/command-palette/CommandPalette";
import { MemoryCardsPanel } from "@/domains/memory-vault/MemoryCardsPanel";
import { ScreenGuidePanel } from "@/domains/screen-guide/ScreenGuidePanel";
import { DailySummaryPanel } from "@/domains/workspace/DailySummaryPanel";
import { FndrWrappedPanel } from "@/domains/workspace/FndrWrappedPanel";
import { AppToasts } from "./AppToasts";
import { PanelErrorBoundary } from "./PanelErrorBoundary";
import type { AppToast } from "./types";

interface AppPanelsProps {
    activePanel: PanelKey | null;
    appNames: string[];
    appToasts: AppToast[];
    isCapturing: boolean;
    query: string;
    selectedResult: MemoryCard | null;
    showCommandPalette: boolean;
    memoryVaultFocusId: string | null;
    onClearSearch: () => void;
    onCloseCommandPalette: () => void;
    onClosePanel: () => void;
    onDeleteMemory: (memoryId: string) => void;
    onDismissToast: (toastId: string) => void;
    onGoHome: () => void;
    onMemoryDeleted: (memoryId: string) => void;
    onOpenPanel: (panel: PanelKey) => void;
    onRunQuery: (query: string) => void;
    onSearchApp: (appName: string) => void;
    onToastAction: (toast: AppToast) => void;
    onOpenMemoryById: (memoryId: string) => void;
}

/** Alpha demo surface (D1): Vault, Ask FNDR, Daily Summary, Wrapped, Screen Guide.
 *  Hidden rank-2 panels stay compiled under src/domains but are not mounted. */
export function AppPanels({
    activePanel,
    appNames,
    appToasts,
    isCapturing,
    query,
    selectedResult,
    showCommandPalette,
    memoryVaultFocusId,
    onClearSearch,
    onCloseCommandPalette,
    onClosePanel,
    onDeleteMemory,
    onDismissToast,
    onGoHome,
    onMemoryDeleted,
    onOpenPanel,
    onRunQuery,
    onSearchApp,
    onToastAction,
    onOpenMemoryById,
}: AppPanelsProps) {
    return (
        <>
            <PanelErrorBoundary panelName="Ask FNDR">
                <AskPanel isVisible={activePanel === "ask"} onClose={onClosePanel} onOpenMemoryById={onOpenMemoryById} />
            </PanelErrorBoundary>
            <MemoryCardsPanel
                isVisible={activePanel === "memoryCards"}
                onClose={onClosePanel}
                appNames={appNames}
                onMemoryDeleted={onMemoryDeleted}
                feature="vault"
                focusMemoryId={memoryVaultFocusId}
                onOpenMemoryById={onOpenMemoryById}
            />
            <PanelErrorBoundary panelName="Daily Summary">
                <DailySummaryPanel
                    isVisible={activePanel === "dailySummary"}
                    onClose={onClosePanel}
                    onOpenMemoryById={onOpenMemoryById}
                />
            </PanelErrorBoundary>
            <PanelErrorBoundary panelName="FNDR Wrapped">
                <FndrWrappedPanel isVisible={activePanel === "wrapped"} onClose={onClosePanel} />
            </PanelErrorBoundary>
            <PanelErrorBoundary panelName="Screen Guide">
                <ScreenGuidePanel isVisible={activePanel === "screenGuide"} onClose={onClosePanel} />
            </PanelErrorBoundary>
            <CommandPalette
                isOpen={showCommandPalette}
                onClose={onCloseCommandPalette}
                selectedMemory={selectedResult}
                demoOnly
                context={{
                    query,
                    onOpenPanel,
                    onGoHome,
                    onSearch: onRunQuery,
                    onSearchApp,
                    onClearSearch,
                    onDeleteMemory,
                    isCapturing,
                }}
            />
            <AppToasts toasts={appToasts} onAction={onToastAction} onDismiss={onDismissToast} />
        </>
    );
}
```
If `PanelErrorBoundary`'s prop name differs, check `sed -n 1,30p src/app/PanelErrorBoundary.tsx`.

- [ ] **Step 4: Update `App.tsx`**
  1. Replace `SIDEBAR_GROUPS` with:
  ```ts
  const SIDEBAR_GROUPS = [
      {
          label: "Memory",
          items: [
              { key: "memoryCards", text: "Memory Vault" },
              { key: "ask", text: "Ask FNDR" },
          ],
      },
      {
          label: "Reflect",
          items: [
              { key: "dailySummary", text: "Daily Summary" },
              { key: "wrapped", text: "FNDR Wrapped" },
          ],
      },
      {
          label: "Assist",
          items: [{ key: "screenGuide", text: "Screen Guide" }],
      },
  ] as const satisfies ReadonlyArray<{
      label: string;
      items: ReadonlyArray<{ key: PanelKey; text: string }>;
  }>;
  ```
  2. Add a `goHome` callback near the other handlers:
  ```ts
      const goHome = useCallback(() => {
          setActivePanel(null);
          setShowCommandPalette(false);
          setIsSidebarOpen(false);
          setQuery("");
          setQueryDraft("");
          setTimeFilter(null);
          setAppFilter(null);
      }, []);
  ```
  3. In the sidebar, directly after `<div className="sidebar-brand"></div>`, insert:
  ```tsx
                      <div className="sidebar-group sidebar-actions">
                          <button
                              className={`ui-action-btn ${activePanel === null && !query.trim() ? "active" : ""}`}
                              onClick={goHome}
                          >
                              Home
                          </button>
                      </div>
  ```
  4. Pass `onGoHome={goHome}` to `<AppPanels … />`.
  5. Remove `import { useAutomationScheduler } …` and the `useAutomationScheduler();` call (+ its comment).
  6. `handleToastAction` / any other remaining `"agent"` or `"research"` references → `grep -n '"agent"\|"research"\|engineMetrics\|pipeline' src/app/App.tsx` and change them to `"ask"` or remove them.
  7. The `SearchBar` props `onSetMeetingPanelOpen` / `onSetKnowledgeGraphPanelOpen` may set hidden panels. Leave them; the keys still exist, and those panels aren't mounted, so nothing renders.

- [ ] **Step 5: Verify**

```bash
npm run typecheck && npx vitest run src/domains/command-palette src/app 2>&1 | tail -8
```
Expected: clean. **Allowed collateral:** `src/domains/workspace/AgentPanel.test.tsx` still imports the (now unmounted) AgentPanel and passes. Leave it until Task 13. If `AppShell.test.tsx` asserts old sidebar labels, update the expected labels to the new ones.

Self-QA (HMR is live): open the sidebar and Cmd+K, then `screencapture -x /tmp/fndr-qa-t6.png` → Read. Expected sidebar: Home / Memory: Memory Vault, Ask FNDR / Reflect: Daily Summary, FNDR Wrapped / Assist: Screen Guide / Commands: Cmd+K Palette. No "Engine metrics".

- [ ] **Step 6: Commit + push**

```bash
git add src/app src/domains/command-palette
git commit -m "demo: curated alpha surface with Home, Ask FNDR and teammates' features

Removes Engine Metrics and Context-pack entry points from nav and Cmd+K.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push origin alpha-demo
```

---

## Task 7: Vault "Needs more signal" review queue (Tier 1, ~30 min)

**Files:**
- Modify: `src/domains/memory-vault/MemoryCardsPanel.tsx` (load effect ~line 319; header count ~line 548; card list ~line 694)
- Modify: `src/domains/memory-vault/MemoryCardsPanel.test.tsx`
- Modify: `src/domains/memory-vault/MemoryCardsPanel.css` (append)

**Interfaces:**
- Consumes: `listNeedsSignalMemoryCards`, `NeedsSignalCard` (Task 2)

- [ ] **Step 1: Extend the test** (`MemoryCardsPanel.test.tsx`). Add `listNeedsSignalMemoryCards: vi.fn().mockResolvedValue([])` to the `vi.mock` factory, change the existing expectation `"1500 cards"` → `"1500 memories"`, and add:

```tsx
import { fireEvent } from "@testing-library/react";
import { listNeedsSignalMemoryCards } from "@/shared/ipc/tauri";

    it("lists low-signal captures only behind the Needs more signal toggle", async () => {
        vi.mocked(listMemoryCards).mockResolvedValue([card(1)]);
        vi.mocked(listNeedsSignalMemoryCards).mockResolvedValue([
            {
                card: { ...card(99), app_name: "ChatGPT", title: "Screen capture (visual) ChatGPT_1789709739566.png" },
                reason_code: "image_only",
                reason: "Image only: no readable text was on screen, so it was kept out of search.",
            },
        ]);
        render(<MemoryCardsPanel isVisible={true} onClose={() => {}} appNames={[]} feature="vault" />);
        const toggle = await screen.findByRole("button", { name: "Needs more signal (1)" });
        expect(screen.queryByText(/Image only/)).not.toBeInTheDocument();
        fireEvent.click(toggle);
        expect(await screen.findByText(/Image only: no readable text/)).toBeInTheDocument();
        expect(screen.queryByText(/\.png/)).not.toBeInTheDocument();
    });
```
Run: `npx vitest run src/domains/memory-vault/MemoryCardsPanel.test.tsx` → FAIL.

- [ ] **Step 2: Implement in `MemoryCardsPanel.tsx`**
  1. Import `listNeedsSignalMemoryCards, type NeedsSignalCard` from `@/shared/ipc/tauri`.
  2. State: `const [needsSignal, setNeedsSignal] = useState<NeedsSignalCard[]>([]);` and `const [showNeedsSignal, setShowNeedsSignal] = useState(false);`
  3. Load: in the same effect that calls `listMemoryCards(1500, selectedApp)` (only when `feature === "vault"`), also call:
  ```ts
            void listNeedsSignalMemoryCards(200)
                .then((items) => setNeedsSignal(items))
                .catch(() => setNeedsSignal([]));
  ```
  4. Header: change `{filteredCards.length} cards` → `{filteredCards.length} memories` and, when `feature === "vault" && needsSignal.length > 0`, render next to it:
  ```tsx
                        <button
                            type="button"
                            className={`ui-action-btn memory-needs-signal-toggle${showNeedsSignal ? " is-active" : ""}`}
                            aria-pressed={showNeedsSignal}
                            onClick={() => setShowNeedsSignal((v) => !v)}
                        >
                            {`Needs more signal (${needsSignal.length})`}
                        </button>
  ```
  5. Body: when `showNeedsSignal` is true, render this **instead of** the card grid:
  ```tsx
                    <section className="memory-needs-signal" aria-label="Captures that need more signal">
                        <p className="memory-needs-signal-intro">
                            These captures had too little readable content to become memories. They stay on this Mac but are kept out of search and answers.
                        </p>
                        <ul>
                            {needsSignal.map(({ card, reason }) => (
                                <li key={card.id} className="memory-needs-signal-row">
                                    <span className="memory-needs-signal-app">{card.app_name}</span>
                                    <span className="memory-needs-signal-time">
                                        {new Date(card.timestamp).toLocaleString(undefined, { weekday: "short", hour: "numeric", minute: "2-digit" })}
                                    </span>
                                    <span className="memory-needs-signal-reason">{reason}</span>
                                </li>
                            ))}
                        </ul>
                    </section>
  ```
  (Never render `card.title`/`card.summary` here, because they may contain filenames.)
  6. CSS (append to `MemoryCardsPanel.css`):
  ```css
  .memory-needs-signal-toggle { font-size: 12px; padding: 4px 12px; border-radius: var(--film-radius-pill); }
  .memory-needs-signal-toggle.is-active { border-color: var(--accent); color: var(--accent); }
  .memory-needs-signal { padding: 8px 4px 32px; }
  .memory-needs-signal-intro { color: var(--text-secondary); font-size: 14px; margin: 0 0 14px; }
  .memory-needs-signal ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
  .memory-needs-signal-row {
      display: grid; grid-template-columns: 140px 150px 1fr; gap: 12px; align-items: baseline;
      padding: 12px 14px; border: 1px dashed var(--border-strong); border-radius: var(--film-radius-md);
  }
  .memory-needs-signal-app { color: var(--text-primary); font-weight: 600; }
  .memory-needs-signal-time { color: var(--text-secondary); font-size: 13px; }
  .memory-needs-signal-reason { color: var(--text-secondary); font-size: 14px; }
  ```

- [ ] **Step 3: Verify**

`npx vitest run src/domains/memory-vault && npm run typecheck` → PASS. Self-QA: open Vault → `screencapture` → expect "113-ish memories" and "Needs more signal (3)". Toggle → 3 rows (Preview, ChatGPT, Photos) with reasons, no filenames.

- [ ] **Step 4: Commit + push**

```bash
git add src/domains/memory-vault
git commit -m "feat(vault): Needs more signal review queue for low-signal captures

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push origin alpha-demo
```

---

## Task 8: Clean card presentation + "How FNDR built this" (Tier 1, ~50 min)

**Files:**
- Modify: `src/domains/memory-vault/InsightLayers.tsx`
- Modify: `src/domains/memory-vault/MemoryCard.tsx` (STAMP_META ~line 313; strip/frame ~lines 110 & 175; preview duplication)
- Modify: `src/domains/memory-vault/ExpandedMemoryCard.tsx`
- Modify: `src/domains/memory-vault/MemoryProvenanceStrip.tsx`
- Modify: `src/domains/memory-vault/MemoryCardsPanel.tsx` (stop passing `similarSlot`/`debugSlot`/`onResearch` to the expanded card)
- Modify: `src/domains/timeline/Timeline.tsx` (result card)
- Modify: `src/shared/utils/config.ts`, `src/app/HomeHero.tsx`, `src/domains/search/SearchBar.tsx` (voice mic hidden, D18)
- Tests: `src/domains/memory-vault/__tests__/MemoryCard.lifecycle.test.tsx` (update labels), new `src/domains/memory-vault/__tests__/InsightLayers.test.tsx`, `src/domains/timeline/Timeline.test.tsx` (extend), `src/app/HomeHero.test.tsx` (update if it asserts the voice button)

**Interfaces:**
- Produces: `export function describeBuildPath(card: MemoryCard): string` (in `MemoryCard.tsx`); `export const VOICE_INPUT_ENABLED = false` (in `src/shared/utils/config.ts`)

- [ ] **Step 1: Write failing tests**

`src/domains/memory-vault/__tests__/InsightLayers.test.tsx`:
```tsx
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { InsightLayers } from "../InsightLayers";
import type { MemoryCard } from "@/shared/ipc/tauri";

const base: MemoryCard = {
    id: "m", title: "t", summary: "s", action: "", context: [], timestamp: 0,
    app_name: "Cursor", window_title: "w", score: 1, source_count: 1, raw_snippets: [],
};

describe("InsightLayers (clean)", () => {
    it("hides empty rows and never shows raw branch tags", () => {
        render(
            <InsightLayers
                card={{ ...base, insight_what_happened: "Fixed E0502 in the capture loop.", synthesis_branch: "llm_ocr_grounded_visual_fallback" }}
            />
        );
        expect(screen.getByText("Fixed E0502 in the capture loop.")).toBeInTheDocument();
        expect(screen.queryByText(/not yet extracted/)).not.toBeInTheDocument();
        expect(screen.queryByText("What changed")).not.toBeInTheDocument();
        expect(screen.queryByText(/llm_ocr_grounded/)).not.toBeInTheDocument();
    });

    it("renders nothing when every insight is empty", () => {
        const { container } = render(<InsightLayers card={base} />);
        expect(container).toBeEmptyDOMElement();
    });
});
```
In `MemoryCard.lifecycle.test.tsx`, change the asserted labels to the new ones: `DEVELOPED→REVIEWED`, `PENDING→AWAITING REVIEW`, `RAW→CAPTURED`, `REVIEW FAILED→REVIEW INCOMPLETE`, `VISUAL FAILED→UNREADABLE`. Keep the `data-lifecycle` values (`DEVELOPED` etc.) unchanged, since they're internal. Add an assertion that `screen.queryByText(/^FRAME /)` is null.
In `Timeline.test.tsx`, add a case: a result whose `title` equals its `display_summary` renders that text once, and there's no button named "Delete this memory".
Run: `npx vitest run src/domains/memory-vault src/domains/timeline` → FAIL.

- [ ] **Step 2: `InsightLayers.tsx`**. Keep `SLOTS`. Rewrite the component body:

```tsx
/** Renders only the insight rows FNDR actually has. Internal pipeline names
 *  live in the expanded card's "How FNDR built this" disclosure, not here. */
export function InsightLayers({ card, evalUi = false }: { card: MemoryCard; evalUi?: boolean }) {
    const ic = card.insight_card_confidence ?? 0;
    const low = ic > 0 && ic < 0.4;
    const rows = SLOTS.map((slot) => {
        const raw = (card as unknown as Record<string, unknown>)[slot.field];
        const trimmed = typeof raw === "string" ? raw.trim() : "";
        return { label: slot.label, value: trimmed && !isMetaOcrNarration(trimmed) ? trimmed : "" };
    }).filter((row) => row.value.length > 0);
    const categories = card.topic_categories?.filter((c) => c.trim()) ?? [];

    if (rows.length === 0 && categories.length === 0) return null;

    return (
        <div className={`insight-layers${low ? " insight-layers--low-conf" : ""}`}>
            {low && (
                <div className="insight-meta-row">
                    <div className="insight-low-badge">Limited detail</div>
                </div>
            )}
            {rows.map((row) => (
                <div className="insight-row" key={row.label}>
                    <span className="insight-label">{row.label}</span>
                    <p className="insight-value">{row.value}</p>
                </div>
            ))}
            {categories.length > 0 && (
                <div className="insight-categories">
                    {categories.slice(0, 6).map((c) => (
                        <span className="insight-category-chip" key={c}>{c}</span>
                    ))}
                </div>
            )}
            {evalUi && card.insight_spans_json?.trim() && (
                <details className="insight-spans-debug" onClick={(e) => e.stopPropagation()}>
                    <summary>Salience spans (debug)</summary>
                    <pre className="insight-spans-pre">{card.insight_spans_json}</pre>
                </details>
            )}
        </div>
    );
}
```
Update the file's top doc comment to match (no placeholders).

- [ ] **Step 3: `MemoryCard.tsx`**
  1. `STAMP_META` labels: `DEVELOPED: "REVIEWED"`, `PENDING: "AWAITING REVIEW"`, `RAW: "CAPTURED"`, `REVIEW_FAILED: "REVIEW INCOMPLETE"`, `VISUAL_FAILED: "UNREADABLE"` (tones unchanged).
  2. Delete both `FRAME {frameId}` spans (compact `fndr-mc-c-frame` and strip `fndr-mc-frame-no`), the `frameId` memo, and `deriveFrameId`.
  3. Preview duplication: import `tokenOverlap` from `@/shared/utils/cardCleanup`, and change `const previewText = pickPreviewText(card);` to:
  ```ts
      const rawPreview = pickPreviewText(card);
      const previewText =
          rawPreview && tokenOverlap(rawPreview, card.title) < 0.8 ? rawPreview : "";
  ```
  (Check `tokenOverlap`'s signature with `grep -n "export function tokenOverlap" src/shared/utils/cardCleanup.ts`; it returns 0..1.)
  4. Add and export:
  ```ts
  /** Plain-language description of how a memory was produced, for the
   *  "How FNDR built this" disclosure. Never shown in the default card view. */
  export function describeBuildPath(card: MemoryCardData): string {
      const branch = (card.synthesis_branch ?? "").toLowerCase();
      const source =
          branch === "demo_seed"
              ? "Seeded demo week (text written for the demo, embedded and stored through FNDR's normal pipeline)"
              : branch.includes("visual")
                ? "Screen image with little readable text"
                : "On-screen text (Apple Vision OCR) summarized on this Mac";
      const review =
          card.enrichment_status === "reviewed_local" || card.enrichment_status === "reviewed_daily"
              ? "Reviewed by the local Qwen3-VL-2B model"
              : card.enrichment_status === "review_failed"
                ? "Local review didn't finish; original capture kept"
                : "Waiting for local review";
      return `${source}. ${review}.`;
  }
  ```

- [ ] **Step 4: `ExpandedMemoryCard.tsx`**
  1. Delete the subgraph state, the `fndrGetMemorySubgraph` call and import, and `subgraphNode`.
  2. Wrap evidence in a disclosure. Replace `combinedEvidence` with:
  ```tsx
      const confidencePct = Math.round(((card.confidence ?? card.score) || 0) * 100);
      const combinedEvidence = (
          <>
              <details className="fndr-emc-build" onClick={(e) => e.stopPropagation()}>
                  <summary>How FNDR built this</summary>
                  <p className="fndr-emc-meta">{describeBuildPath(card)}</p>
                  {confidencePct > 0 && <p className="fndr-emc-meta">Match confidence: {confidencePct}%</p>}
                  {chunkEvidenceNode}
                  {evidenceNode}
              </details>
              {relatedNode}
          </>
      );
  ```
  Import `describeBuildPath` from `./MemoryCard`. If `card.confidence` doesn't exist on the TS type, use `card.score` only.
  3. Remove `CopyForAgentButton` from `relatedSlot` (agent handoff isn't in the demo). Pass `relatedSlot={null}`. Leave the `CopyForAgentButton.tsx` file.
- In `MemoryCardsPanel.tsx`, stop passing `debugSlot`, `similarSlot`, and `onResearch` to `ExpandedMemoryCard` and remove the now-unused state/handlers (`findVisuallySimilarMemories`, `getMemoryDebugInspector` usages for the expanded card) that `noUnusedLocals` flags.

- [ ] **Step 5: `MemoryProvenanceStrip.tsx`**: read the file, then remove the **Anchor** and **Status** cells (the stamp already shows status). Keep Captured / Source / Window / Confidence.

- [ ] **Step 6: `Timeline.tsx` result card** (~lines 112–210)
  1. Remove the Delete button block (`onDeleteMemory && (…)`) from `result-meta-actions`. Keep the `onDeleteMemory` prop in the interface but prefix the destructured name with `_` if `noUnusedParameters` complains, or remove the prop and its pass-through in `App.tsx`.
  2. Duplication: compute
  ```ts
                      const showPrimary =
                          !isLowSignalPreview(primaryText, result.app_name) &&
                          tokenOverlap(primaryText, displayTitle) < 0.8;
  ```
  and render `<p className="result-primary">{primaryText}</p>` only when `showPrimary`.
  3. Hide the internal quality label chip when it's a raw status (keep match-reason chips like "Exact phrase match" and "Direct match").

- [ ] **Step 7: Hide voice input (D18)**
  - `src/shared/utils/config.ts`: add `export const VOICE_INPUT_ENABLED = false;`
  - `HomeHero.tsx`: wrap the voice `<button …home-hero__voice-btn…>` and the voice-status `<p>` in `{VOICE_INPUT_ENABLED && (…)}`. Delete the `SCROLL TO EXPLORE` indicator `motion.div` and its now-unused `scrollX/scrollY` transforms.
  - `SearchBar.tsx`: wrap its mic/voice button in `{VOICE_INPUT_ENABLED && (…)}`.
  - Fix any now-unused locals the compiler reports (keep the voice hook code if still referenced behind the flag).

- [ ] **Step 8: Verify**

```bash
npm run typecheck && npx vitest run src/domains/memory-vault src/domains/timeline src/app src/domains/search 2>&1 | tail -8
```
Self-QA: search `E0502` in the running app → `screencapture` → expect: CURSOR · Tue · 2:12 PM, title once, one summary line, no Delete, no snake_case chip. Open it in the Vault (expanded) → no FRAME, no Anchor, no "0 nodes", and a collapsed "How FNDR built this".

- [ ] **Step 9: Commit + push**

```bash
git add src
git commit -m "fix(cards): clean memory cards with How FNDR built this disclosure

Hides empty insight rows, raw pipeline tags, frame ids, anchor %, subgraph
counts, duplicate title/summary, delete-in-results, and voice input.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push origin alpha-demo
```

---

## Task 9: Settings as one clean sheet (Tier 1, ~50 min)

**Files:**
- Modify: `src/domains/workspace/ControlPanel.tsx` (settings `<aside>` ~lines 683–1275; tab state ~line 82/101)
- Modify: `src/domains/workspace/ControlPanel.test.tsx`

**Interfaces:**
- Consumes: existing ControlPanel state/handlers for profile, capture toggle, blocklist add/remove, privacy alerts (`PrivacyPanel` section at ~line 1146), models list (`loadModels`), and `status` prop.

- [ ] **Step 1: Failing test + update the tab-based tests** (`ControlPanel.test.tsx`). Add `listNeedsSignalMemoryCards: vi.fn().mockResolvedValue([])` to the `vi.mock("@/shared/ipc/tauri", …)` factory. The existing tests rely on tabs and Updates, so update them:
  - `"exposes privacy alerts inside settings privacy"` (line ~109): delete the `fireEvent.click(screen.getByRole("button", { name: /privacy/i }))` line. The alerts are on the single sheet.
  - `"warns that capture is paused when the embedder is unavailable"` (~121): delete the `/model/i` tab click. The warning must render inside the **Local models** section (keep the same expected text).
  - `"reports up to date after checking for updates"` and `"offers install and restart when an update is available"` (~132, ~142): **delete both tests** and the `@tauri-apps/plugin-updater` / `plugin-process` mocks if nothing else uses them (Updates is hidden, D10).
  Then add:

```tsx
    it("shows one demo-safe settings sheet without destructive or advanced controls", async () => {
        render(<ControlPanel status={null} compact={true} />);
        fireEvent.click(screen.getByRole("button", { name: /open settings/i }));
        expect(await screen.findByText("Profile")).toBeInTheDocument();
        expect(screen.getByText("Capture")).toBeInTheDocument();
        expect(screen.getByText("Blocked Apps & Sites")).toBeInTheDocument();
        expect(screen.getByText("Local models")).toBeInTheDocument();
        for (const hidden of [/Danger Zone/i, /Delete all data/i, /Check for updates/i, /Screen Auto-Fill/i, /MCP Server/i, /Reclaim storage/i, /continuity repair/i]) {
            expect(screen.queryByText(hidden)).not.toBeInTheDocument();
        }
        expect(screen.queryByRole("tab")).not.toBeInTheDocument();
        expect(screen.queryByRole("button", { name: /^Delete$/ })).not.toBeInTheDocument();
    });
```
Run: `npx vitest run src/domains/workspace/ControlPanel.test.tsx` → FAIL.

- [ ] **Step 2: Restructure the settings `<aside>`**. Order and content:
  1. Header unchanged ("FNDR Settings" / "Private, local, always in your control.").
  2. **Delete the tab bar** (`ui-action-btn tab` buttons) and the `activeTab` state/type. Render all sections in one scroll container.
  3. `<section><h3>Profile</h3>…` (the existing profile block, unchanged).
  4. `<section><h3>Capture</h3>`: the existing pause/resume button, then a counts line merging the old Capture Status and Memory Quality numbers: `Stored {stored} · Skipped {skipped} · Needs signal {needsSignalCount}`. Get `needsSignalCount` via `listNeedsSignalMemoryCards(500).then(r => setNeedsSignalCount(r.length))` when the sheet opens. Keep the "FNDR never records its own window…" hint. Drop the skip-reason chips (`this app (FNDR): 51`, etc.) and "Last skip".
  5. `<section><h3>Privacy</h3>`: the existing privacy-alerts content (Felipe's bank alert) + the existing "Blocked Apps & Sites" block with add/remove, unchanged.
  6. `<section><h3>Local models</h3>`: read-only. For each model from the existing models list whose installed/downloaded flag is true, render `✓ {displayName} — {role}` where role maps: Qwen → `memory + answers`, MiniLM → `search (384-d)`, otherwise the model's own description. Add one line: `Everything runs on this Mac. Nothing is uploaded.` Move the existing embedder-unavailable "capture is paused" warning (currently in the Model tab) into this section unchanged. **No** download/delete buttons, no RAM/disk rows.
  7. **Delete** from render: Updates, Memory Quality (merged above), Indexing, Screen Auto-Fill, MCP Server, AI Model manage block, Danger Zone.
  8. Remove every handler/state/import that is now unused (`noUnusedLocals`): retention, clean dev cache, autofill settings + save, MCP toggle/copy link, model download/delete + confirm, repair backfill, reclaim storage, delete all + confirm, `onOpenPanel` prop if unused (keep the prop in the interface only if `App.tsx` still passes it; otherwise remove it from both). The IPC wrappers stay in `tauri.ts`, and the backend stays.

- [ ] **Step 3: Verify**

`npm run typecheck && npx vitest run src/domains/workspace/ControlPanel.test.tsx` → PASS. Self-QA: open Settings → `screencapture` → one sheet with Profile / Capture / Privacy / Local models, nothing else.

- [ ] **Step 4: Commit + push**

```bash
git add src/domains/workspace/ControlPanel.tsx src/domains/workspace/ControlPanel.test.tsx
git commit -m "demo: single Settings sheet with capture, privacy and read-only local models

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push origin alpha-demo
```

---

## Task 10: Home: capture chip, Recent row, Home affordance, lighter scrim (Tier 1, ~40 min)

**Files:**
- Create: `src/app/HomeRecent.tsx`, `src/app/HomeRecent.test.tsx`
- Modify: `src/app/App.tsx` (render under `<HomeHero>` in `home-hero-stage`), `src/app/HomeHero.css` (append), `src/app/styles/App.css` (`.sidebar-scrim`)

**Interfaces:**
- Consumes: `listMemoryCards(limit, appFilter)` (already low-signal filtered, Task 2), `getStats(): Promise<Stats>` with `today_count` (check the TS name via `grep -n "export async function getStats" src/shared/ipc/tauri.ts`), `CaptureStatus` (`is_capturing`, `is_paused`), `usePolling(fn, ms, enabled)`
- Produces: `export function HomeRecent(props: { status: CaptureStatus | null; onOpenMemory: (id: string) => void }): JSX.Element`

- [ ] **Step 1: Failing test** `src/app/HomeRecent.test.tsx`

```tsx
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { HomeRecent } from "./HomeRecent";
import { getStats, listMemoryCards, type MemoryCard } from "@/shared/ipc/tauri";

vi.mock("@/shared/ipc/tauri", () => ({ listMemoryCards: vi.fn(), getStats: vi.fn() }));

const mk = (i: number): MemoryCard => ({
    id: `m${i}`, title: `Fixed E0502 ${i}`, summary: "", action: "", context: [],
    timestamp: Date.now() - i * 60_000, app_name: "Cursor", window_title: "", score: 1,
    source_count: 1, raw_snippets: [],
});

afterEach(() => { cleanup(); vi.clearAllMocks(); });

describe("HomeRecent", () => {
    it("shows live capture status with today's count and opens a recent memory", async () => {
        vi.mocked(listMemoryCards).mockResolvedValue([mk(1), mk(2)]);
        vi.mocked(getStats).mockResolvedValue({ today_count: 23 } as never);
        const onOpen = vi.fn();
        render(<HomeRecent status={{ is_capturing: true, is_paused: false } as never} onOpenMemory={onOpen} />);
        expect(await screen.findByText("Capturing · 23 memories today")).toBeInTheDocument();
        fireEvent.click(await screen.findByRole("button", { name: /Fixed E0502 1/ }));
        expect(onOpen).toHaveBeenCalledWith("m1");
    });

    it("shows paused state and an empty hint", async () => {
        vi.mocked(listMemoryCards).mockResolvedValue([]);
        vi.mocked(getStats).mockResolvedValue({ today_count: 0 } as never);
        render(<HomeRecent status={{ is_capturing: false, is_paused: true } as never} onOpenMemory={() => {}} />);
        expect(await screen.findByText("Capture paused")).toBeInTheDocument();
        expect(screen.getByText(/FNDR builds your memory as you work/)).toBeInTheDocument();
    });
});
```
Run → FAIL.

- [ ] **Step 2: Implement** `src/app/HomeRecent.tsx`

```tsx
import { useCallback, useState } from "react";
import { getStats, listMemoryCards, type CaptureStatus, type MemoryCard } from "@/shared/ipc/tauri";
import { usePolling } from "@/shared/hooks/usePolling";

interface HomeRecentProps {
    status: CaptureStatus | null;
    onOpenMemory: (id: string) => void;
}

function relativeTime(ts: number): string {
    const mins = Math.max(0, Math.round((Date.now() - ts) / 60_000));
    if (mins < 1) return "just now";
    if (mins < 60) return `${mins}m ago`;
    const hours = Math.round(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    return new Date(ts).toLocaleDateString(undefined, { weekday: "short" });
}

export function HomeRecent({ status, onOpenMemory }: HomeRecentProps) {
    const [recent, setRecent] = useState<MemoryCard[]>([]);
    const [today, setToday] = useState<number | null>(null);

    const refresh = useCallback(async () => {
        const [cards, stats] = await Promise.all([
            listMemoryCards(5, null).catch(() => [] as MemoryCard[]),
            getStats().catch(() => null),
        ]);
        setRecent(cards.slice(0, 5));
        setToday(stats ? stats.today_count : null);
    }, []);
    usePolling(refresh, 15_000, true);

    const chip = status?.is_paused
        ? { tone: "paused", text: "Capture paused" }
        : status?.is_capturing
          ? { tone: "live", text: `Capturing · ${today ?? 0} memories today` }
          : { tone: "off", text: "Not capturing" };

    return (
        <section className="home-recent" aria-label="Recent memories">
            <div className={`home-capture-chip home-capture-chip--${chip.tone}`} role="status">
                <span className="home-capture-dot" aria-hidden="true" />
                {chip.text}
            </div>
            <h2 className="home-recent-label">Recent</h2>
            {recent.length === 0 ? (
                <p className="home-recent-empty">No memories yet. FNDR builds your memory as you work.</p>
            ) : (
                <div className="home-recent-row">
                    {recent.map((card) => (
                        <button key={card.id} type="button" className="home-recent-tile" onClick={() => onOpenMemory(card.id)}>
                            <span className="home-recent-app">{card.app_name}</span>
                            <span className="home-recent-title">{card.title}</span>
                            <span className="home-recent-time">{relativeTime(card.timestamp)}</span>
                        </button>
                    ))}
                </div>
            )}
        </section>
    );
}
```
Check `usePolling`'s signature (`sed -n 1,30p src/shared/hooks/usePolling.ts`); if it expects `(fn: (isMounted) => …)`, adapt the callback.

- [ ] **Step 3: Mount in `App.tsx`**. Inside `<div className="home-hero-stage">`, directly after `<HomeHero … />` and before the `{query.trim() && (…)}` block:
```tsx
                        {!EVAL_UI && <HomeRecent status={status} onOpenMemory={handleOpenMemoryById} />}
```
Import `HomeRecent` from `./HomeRecent`. `handleOpenMemoryById` already exists and opens the Vault focused on an id. If it isn't defined before this JSX, it is still in scope (it's a function-level const). Verify with `grep -n "handleOpenMemoryById" src/app/App.tsx`.

- [ ] **Step 4: Styles** (append to `src/app/HomeHero.css`)

```css
.home-recent { width: min(860px, 100% - 48px); margin: 8px auto 48px; display: flex; flex-direction: column; align-items: center; gap: 14px; }
.home-capture-chip { display: inline-flex; align-items: center; gap: 8px; font-size: 13px; padding: 5px 14px; border-radius: var(--film-radius-pill); border: 1px solid var(--border); color: var(--text-secondary); background: var(--surface-translucent); }
.home-capture-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--text-secondary); }
.home-capture-chip--live .home-capture-dot { background: var(--accent); box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 25%, transparent); }
.home-capture-chip--paused .home-capture-dot { background: var(--danger); }
.home-recent-label { align-self: flex-start; margin: 12px 0 0; font-size: 11px; letter-spacing: 0.14em; text-transform: uppercase; color: var(--text-secondary); font-weight: 500; }
.home-recent-row { width: 100%; display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 10px; }
.home-recent-tile { display: flex; flex-direction: column; gap: 4px; text-align: left; padding: 12px 14px; cursor: pointer; font: inherit; color: var(--text-primary); background: var(--surface-translucent); border: 1px solid var(--border); border-radius: var(--film-radius-md); transition: border-color 0.2s, background 0.2s; }
.home-recent-tile:hover { border-color: var(--border-strong); background: var(--surface-hover); }
.home-recent-app { font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-secondary); }
.home-recent-title { font-size: 14px; line-height: 1.35; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
.home-recent-time { font-size: 12px; color: var(--text-secondary); }
.home-recent-empty { color: var(--text-secondary); font-size: 14px; }
```

- [ ] **Step 5: Lighter sidebar scrim**. Find `.sidebar-scrim` in `src/app/styles/App.css` (`grep -n "sidebar-scrim" -A8 src/app/styles/App.css`). Set its background to `rgba(0, 0, 0, 0.28)` and any `backdrop-filter` blur to `blur(2px)`, so the home behind the sidebar stays legible (screenshot #2 looked disabled).

- [ ] **Step 6: Verify**

`npm run typecheck && npx vitest run src/app` → PASS. Self-QA: `screencapture` of Home. Expected: greeting, date, search pill, "● Capturing · N memories today" (N may be 0 at night; the seeded days aren't "today"), and a Recent row with the latest seeded memories (Thursday items). Open the sidebar → the home is dimmed, not blacked out.

- [ ] **Step 7: Commit + push**

```bash
git add src/app
git commit -m "feat(home): live capture chip, Recent memories row, Home nav, lighter scrim

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push origin alpha-demo
```

**Tier 1 checkpoint:** append a ledger line `T1 complete` with the time. If it's past 05:00, skip Task 11.

---

## Task 11: Theme toggle + static background (Tier 2, ~30 min)

**Files:**
- Modify: `src/domains/workspace/ControlPanel.tsx` (moon button ~line 651; Appearance `<aside>` ~lines 1276–1380)
- Modify: `src/app/AppShell.tsx`, `src/app/styles/wallpaper.css`, `src/app/AppShell.test.tsx`

- [ ] **Step 1: Failing test** (`src/app/AppShell.test.tsx`: add a case, keeping the file's existing mocks)

```tsx
    it("renders a static background layer instead of the WebGL wallpaper", () => {
        const { container } = render(<AppShell />);
        expect(container.querySelector(".fndr-wallpaper-static")).not.toBeNull();
        expect(container.querySelector("canvas")).toBeNull();
    });
```
Run → FAIL.

- [ ] **Step 2: `AppShell.tsx`**

```tsx
import { useActiveCinematicPalette } from "@/shared/hooks/useActiveCinematicPalette";
import { WorkModeShell } from "./WorkModeShell";
import "./styles/wallpaper.css";

type Rgb = readonly [number, number, number];

/** Palette triples are shader-space floats (0..1) or 0..255 ints; normalize to CSS. */
function css([r, g, b]: Rgb, alpha = 1): string {
    const scale = Math.max(r, g, b) <= 1 ? 255 : 1;
    return `rgba(${Math.round(r * scale)}, ${Math.round(g * scale)}, ${Math.round(b * scale)}, ${alpha})`;
}

/**
 * Root shell: static palette gradient (no WebGL; keeps GPU/RAM free for the
 * local model on 8 GB Macs) + the main productive UI. MotionWallpaper stays on
 * disk for a later release.
 */
export function AppShell() {
    const { aurora } = useActiveCinematicPalette();
    const background = [
        `radial-gradient(120% 80% at 50% -10%, ${css(aurora.mid, 0.55)} 0%, transparent 60%)`,
        `radial-gradient(90% 60% at 85% 110%, ${css(aurora.acc, 0.18)} 0%, transparent 55%)`,
        css(aurora.bg),
    ].join(", ");

    return (
        <>
            <div className="fndr-wallpaper-layer fndr-wallpaper-static" aria-hidden style={{ background }} />
            <div className="fndr-app-chrome">
                <WorkModeShell />
            </div>
        </>
    );
}

export default AppShell;
```
Add to `wallpaper.css`: `.fndr-wallpaper-static { pointer-events: none; }`. Remove any test/mocks in `AppShell.test.tsx` that assert `MotionWallpaper`.

- [ ] **Step 3: Moon button → theme toggle (`ControlPanel.tsx`)**
  - Replace the moon button's `onClick` with `onClick={() => selectAppearance(paletteKey, theme === "dark" ? "light" : "dark")}`. Set `aria-label`/`title` to ``theme === "dark" ? "Switch to light mode" : "Switch to dark mode"``. Show the moon icon in dark mode and a sun icon in light mode (sun: `<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>`).
  - Delete the Appearance `<aside>` and the `isAppearanceOpen` state and references (backdrop condition becomes `isOpen` only), `selectWallpaper`, and wallpaper state if unused. Keep `STORAGE_KEYS.wallpaper` writes only if something else still reads them. The palette/wallpaper registries and `MotionWallpaper.tsx` stay on disk.

- [ ] **Step 4: Verify + self-QA** (`npm run typecheck && npx vitest run src/app src/domains/workspace`). Take a screenshot in dark mode, click-toggle to light via the button if System Events is allowed, otherwise check the light theme through the unit test only, and add "toggle light mode" to the morning checklist. Check with `top -l 1 -o cpu | head -15` that the FNDR WebView's CPU dropped vs. the ~87% seen at 00:30.

- [ ] **Step 5: Commit + push**

```bash
git add src/app src/domains/workspace/ControlPanel.tsx
git commit -m "demo: plain dark/light toggle and static palette background (no WebGL)

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push origin alpha-demo
```

---

## Task 12: Teammates' features on the demo profile (Tier 2, ~45 min; D21 full ownership, D22 keep visible)

**Files (as needed):** `src/domains/workspace/DailySummaryPanel.{tsx,css}`, `src/domains/workspace/FndrWrappedPanel.{tsx,css}`, `src/domains/screen-guide/*`, `src-tauri/src/ipc/commands/stats.rs`

- [ ] **Step 1: Daily Summary (Felipe).** Open it via the sidebar → pick yesterday (Thu) → generate. `screencapture`. Pass criteria:
  - (a) renders within 60s
  - (b) mentions the rubric / issue #36 / dry run, and no filenames
  - (c) follow-ups are actionable and clicking one opens the Vault (`onOpenMemoryById`)
  - (d) no raw snake_case
  
  If summary generation is LLM-bound and slow, note the time. If >60s, add a visible "Summarizing on this Mac…" state if missing.
- [ ] **Step 2: FNDR Wrapped (Felipe).** Open → current week. Pass criteria:
  - top apps (Cursor, Chrome, Slack…) and websites (capstone.cs.utah.edu, utah.instructure.com, docs.google.com…) are populated
  - no low-signal apps dominate
  - no empty-state text shows while data exists
  
  If the week selection excludes Fri −7, that's fine.
- [ ] **Step 3: Screen Guide (Kunj).** Open the panel → verify it explains the shortcut and its enabled state, and that the overlay window exists. If System Events is allowed, trigger the shortcut shown in the panel and `screencapture` the overlay. Don't grant any permission. If a permission prompt appears, stop and log it for the morning.
- [ ] **Step 4: Visual consistency pass.** For each of the three panels, make sure: header matches `ask-header`/`daily-summary-header` (title + one-line subtitle + `×` close), no leftover debug text, CSS uses theme tokens (works in light mode), and there are no `console.error`s in `/tmp/fndr-demo-dev.log`.
- [ ] **Step 5: Fix what fails with minimal diffs + a regression test for any logic fix** (Vitest for UI logic, `cargo test --lib stats` for backend). Anything not fixed within the time box stays visible (D22) and goes into the morning packet with exact repro steps.
- [ ] **Step 6: Commit + push** (`fix(daily-summary): …`, `fix(wrapped): …`, `fix(screen-guide): …` as separate commits).

---

## Task 13: Dead-code deletion (Tier 2, ~40 min; D13)

**Files — delete (frontend):**
`src/domains/workspace/AgentPanel.tsx`, `AgentPanel.css`, `AgentPanel.test.tsx`, `AutomationPanel.tsx`, `AutomationPanel.css`, `QuickSkillsPanel.tsx`, `QuickSkillsPanel.css`, `ResearchPanel.tsx`, `ResearchPanel.css`, `EngineMetricsPanel.tsx`, `EngineMetricsCard.tsx`, `PipelineInspectorPanel.tsx`, `PipelineInspectorPanel.css`, `GlassesImportPanel.tsx`, `AutofillOverlay.tsx`, `AutofillOverlay.test.tsx`; `src/app/autofill-entry.tsx`; `autofill.html`.
**Modify:** `vite.config.ts` (remove the `autofill:` input line); `src/shared/ipc/tauri.ts` (remove wrappers that lose all references); `src/domains/workspace/README.md` (remove the deleted panels' rows if listed).
**Files — delete (backend, only where isolated):** `src-tauri/src/ipc/commands/hermes_agent.rs` (+ `mod hermes_agent; pub use hermes_agent::*;` in `ipc/commands/mod.rs` + every `ipc::commands::<fn>` from that file in `main.rs`'s handler list); `src-tauri/src/ipc/commands/glasses_import.rs` (+ `mod`/`pub use` + `import_meta_glasses_photo` registration); `src-tauri/src/ipc/commands/autofill.rs` **only if** `cargo check` passes without it.

- [ ] **Step 1: Stop the app** (Global Constraints kill line).
- [ ] **Step 2: Delete the frontend files + vite input, then list orphaned IPC wrappers**

```bash
cd /Users/anurupkumar/FNDR
git rm -q src/domains/workspace/{AgentPanel.tsx,AgentPanel.css,AgentPanel.test.tsx,AutomationPanel.tsx,AutomationPanel.css,QuickSkillsPanel.tsx,QuickSkillsPanel.css,ResearchPanel.tsx,ResearchPanel.css,EngineMetricsPanel.tsx,EngineMetricsCard.tsx,PipelineInspectorPanel.tsx,PipelineInspectorPanel.css,GlassesImportPanel.tsx,AutofillOverlay.tsx,AutofillOverlay.test.tsx} src/app/autofill-entry.tsx autofill.html
sed -i '' '/autofill: resolve(__dirname, "autofill.html"),/d' vite.config.ts
node -e '
const fs=require("fs"),cp=require("child_process");
const src=fs.readFileSync("src/shared/ipc/tauri.ts","utf8");
const names=[...src.matchAll(/export (?:async )?function (\w+)/g)].map(m=>m[1]);
const pat=/hermes|agent|autofill|glasses|runtimeMetrics|pipeline|automation|skill|research/i;
for (const n of names.filter(n=>pat.test(n))) {
  const hits=cp.execSync(`grep -rlw ${n} src --include=*.ts --include=*.tsx || true`).toString().trim().split("\n").filter(f=>f&&f!=="src/shared/ipc/tauri.ts");
  if (hits.length===0) console.log(n);
}'
```
Delete each printed function from `tauri.ts`, then delete any interfaces/types that `npm run typecheck` now reports as unused or that `grep -rw` shows are unreferenced.

- [ ] **Step 3: Frontend verify**: `npm run typecheck && npm test 2>&1 | tail -5 && npm run build 2>&1 | tail -3` → all green.
- [ ] **Step 4: Backend deletions**

```bash
cd /Users/anurupkumar/FNDR/src-tauri
grep -n "pub async fn\|pub fn" src/ipc/commands/hermes_agent.rs | sed 's/.*fn \([a-z_0-9]*\).*/\1/' > /tmp/hermes_fns.txt; cat /tmp/hermes_fns.txt
```
Remove each listed name from `main.rs`'s `generate_handler!` list (only those present there), delete the file, and remove its `mod`/`pub use` lines. Do the same for `glasses_import.rs`. Run `CARGO_BUILD_JOBS=2 cargo check --bin fndr 2>&1 | grep -E "^error" | head`. If errors come from *other* modules calling into a deleted module, restore that one file (`git checkout -- <file>` plus its mod lines) and keep only the unregistration. Then try removing `autofill.rs` + its registrations (`get_autofill_settings`, `set_autofill_settings`, `set_autofill_overlay_ready`, `take_pending_autofill_payload`, `resolve_autofill`, `dismiss_autofill`, and any others from the file). If `omnibar.rs`/`screen_guide.rs`/`common.rs`/`accessibility` need its items, restore it and log "autofill backend retained: shared helpers".
- [ ] **Step 5: Full verification**: `cd /Users/anurupkumar/FNDR && CARGO_BUILD_JOBS=2 make test 2>&1 | tail -20` → green.
- [ ] **Step 6: Commit + push**

```bash
git add -A
git commit -m "chore: delete agentic experiments and debug/experimental UI

Removes Agent/Hermes, Automation, Quick Skills, Research, Engine Metrics,
Pipeline Inspector, Glasses import and Screen Auto-Fill entry points.
Rust agent/ (MCP) and runtime telemetry are kept.

Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>"
git push origin alpha-demo
```
- [ ] **Step 7: Relaunch the demo profile** (without backfill now): `./scripts/demo/run-demo.sh > /tmp/fndr-demo-dev.log 2>&1` (background).

---

## Task 14: Runtime + answer latency check on 8 GB (Tier 2, ~20 min)

- [ ] **Step 1: Review progress.** `grep -c "reviewed_local\|review.*ok\|memory_review" /tmp/fndr-demo-dev.log` and check in the Vault (screencapture) how many cards show REVIEWED vs. AWAITING REVIEW. Both are honest (D15). Log the ratio.
- [ ] **Step 2: Memory headroom.** `memory_pressure | tail -1`; `ps -o rss=,comm= -p $(pgrep -f cargo-target-shared/debug/fndr) | awk '{print $1/1024 " MiB"}'`. Log both.
- [ ] **Step 3: Ask FNDR latency.** If System Events keystrokes work: Cmd+K → "Ask FNDR" → type `What chunk size did the chunking paper recommend?` ↵ → time it until the answer renders (poll `screencapture` every 5s, max 90s). Then `What's my bank balance?` → expect "Not in your memories". If keystrokes are blocked, skip and put both in the morning checklist.
- [ ] **Step 4: If the answer takes >30s or times out**, `grep -n "SYNTHESIS_TIMEOUT\|fn answer" src-tauri/src/inference/*.rs src-tauri/src/context_runtime/composer.rs | head`. If Qwen `answer` generation has a max-tokens setting, lower it (e.g. to 220) behind the existing config, rebuild once, and re-test. Don't change retrieval. Commit as `perf(ask): …` if changed.
- [ ] **Step 5: If the bank question returns a grounded answer** (a false positive), inspect which card was cited, then either reword that seed entry to remove the accidental overlap (reseed with `--reset`) or record it for the morning. Never special-case the query in code.

---

## Task 15: Final verification, push, morning QA packet (Tier 3, starts by 06:00, done by 06:50)

**Files:**
- Create: `docs/superpowers/plans/2026-09-18-alpha-demo-morning.md`

- [ ] **Step 1: Stop the app, then run the full suite.** `CARGO_BUILD_JOBS=2 make test 2>&1 | tail -25`. It must be green. Record exact counts.
- [ ] **Step 2: Push.** `git push origin alpha-demo && git log --oneline origin/main..alpha-demo`.
- [ ] **Step 3: Relaunch on the demo profile and warm it up.** `./scripts/demo/run-demo.sh > /tmp/fndr-demo-dev.log 2>&1` (background). Once booted, do one Ask FNDR query (via keystrokes if allowed) so Qwen is loaded. Final `screencapture` of Home.
- [ ] **Step 4: Write the morning packet** `docs/superpowers/plans/2026-09-18-alpha-demo-morning.md` with exactly these sections:
  1. **Status line:** branch, head sha, pushed yes/no, `make test` counts, app running yes/no (pid), demo profile path.
  2. **What changed tonight:** one bullet per commit, in plain English.
  3. **Verified automatically vs. needs a human:** two lists. Human-only for sure: Screen Recording permission for live capture, Touch ID (not shown), anything System Events couldn't drive.
  4. **Demo script (D23)** with the exact inputs and expected results:
     - Step 2: pause → chip shows "Capture paused" → resume; add `Messages` to the blocklist.
     - Step 3: open a safe page such as `https://en.wikipedia.org/wiki/Retrieval-augmented_generation` for ~20s, return to FNDR, and it appears in Recent within ~1 min.
     - Step 4: `E0502`, then `that rust borrowing error`.
     - Step 5: `What chunk size did the chunking paper recommend?` → a 512/128 answer citing the Preview PDF + design doc; `What's my bank balance?` → "Not in your memories".
     - Step 6: Vault → "Needs more signal (3)".
     - Step 7: Daily Summary → yesterday; Wrapped → this week.
     - Step 8: Screen Guide.
  5. **Screenshot checklist for live QA** (numbered; the owner sends these in order): Home; sidebar open; Cmd+K; Settings sheet (scrolled top + bottom); search `E0502`; expanded card with "How FNDR built this" open; Ask FNDR grounded; Ask FNDR refusal; Vault default; Vault "Needs more signal"; Daily Summary; Wrapped; Screen Guide; light mode Home.
  6. **Known risks + fallbacks:** e.g. Qwen latency (fallback: ask the grounded question first while talking), live capture delay (fallback: the seeded week carries the story), a teammate feature issue (D22: visible, with the owner's talking point).
  7. **Teammate files touched (D21):** the exact list from `git diff --stat origin/main..alpha-demo -- src/domains/workspace/DailySummaryPanel* src/domains/workspace/FndrWrappedPanel* src/domains/screen-guide src-tauri/src/ipc/commands/stats.rs`.
  8. **How to run / roll back:** `./scripts/demo/run-demo.sh` (demo) vs. `npm run tauri dev` (real profile, untouched), re-seed with `./scripts/demo/seed-demo-profile.sh --reset`, merge path: owner reviews → `git switch main && git merge --ff-only alpha-demo` **only after the rehearsal passes** (a teammate may need to rebase if main moved).
- [ ] **Step 5: Commit + push the packet and final ledger line.**
- [ ] **Step 6: Notify the owner** (PushNotification if available): "FNDR alpha demo build is QA-ready: app running on the demo profile, packet at docs/superpowers/plans/2026-09-18-alpha-demo-morning.md". Then send the packet file with SendUserFile.

## Morning live-QA protocol (07:00 → demo)

For each screenshot the owner sends:
1. **Verdict:** ✅ ready / ⚠️ fix / ❌ blocker.
2. The single most important issue, with file:line.
3. The exact fix, applied immediately with HMR for UI (stop/rebuild only for Rust), plus a test when it's logic.
4. The next screenshot to send.

Cut-offs: no Rust changes after 2 hours before the demo, and no UI changes after 45 minutes before it, except blocker fixes.
