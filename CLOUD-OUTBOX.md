# CLOUD-OUTBOX

Cloud session (Linux container, GitHub only) to local session. Updated after every ticket and at least every two hours. Newest entries first under each heading.

Last update: 2026-10-04 21:35 UTC. Cloud has read LOCAL-OUTBOX at 9b05bbf (main at b948dfd).

## NEEDS HUMAN (open)

None.

## Merge queue for local (in this order)

| # | Branch | Draft PR | Head | Tickets | CI at last check |
|---|---|---|---|---|---|
| 1 | `claude/train-a-measure` | https://github.com/anurupkumar18/FNDR/pull/21 | 5e2610d | VS-04 (critical path; VS-02 and VS-03 will follow on this branch) | running |
| 2 | `claude/train-c-ux` | https://github.com/anurupkumar18/FNDR/pull/22 | 82792e2 | VS-23 | running |
| 3 | `claude/train-e-docs` | https://github.com/anurupkumar18/FNDR/pull/23 | c91b18f | PD-03, PD-17 draft, PD-01 draft (3 commits), PD-02, PD-04, PD-18 guide | running |

All three are based on b948dfd or d2f07e8 and touch no file the others touch, except `docs/team/TEAM.md` (train A adds one line under "Definition of done"; train E rewrites "Weekly rhythm" and adds two sections). `git merge-tree` shows clean merges for A with E and A with C (E and C share no files).

## Gate 0 capability probe (2026-10-04)

| Probe | Result |
|---|---|
| `origin/main` at or after d6e7188 | Yes. Now b948dfd (VS-30). |
| Push `claude/probe` | Push works. **Branch deletion is refused by the environment's git proxy (HTTP 403)**, so `claude/probe` (9d58233, identical to an old main) is still on GitHub. Housekeeping for local: `git push gh --delete claude/probe` when convenient. |
| Toolchains | node v22.22.0, npm 10.9.4, Python 3.11.15, rustc 1.97.0, cargo 1.97.0. `protoc` was missing; installed `protobuf-compiler` (libprotoc 3.21.12) plus Tauri's Linux packages via apt. |
| `npm ci`, `npm run typecheck`, `npm test` | All pass on origin/main: 73 files, 458 tests. |
| `cargo check --locked --lib` on Linux | **Fails as-is**: `objc2-screen-capture-kit` is an unconditional dependency and `objc2` refuses non-Apple targets; then 15 errors in 5 macOS-only files (`accessibility/mod.rs`, `ocr/vision.rs`, `capture/macos.rs`, `ipc/commands/notch.rs`, `ipc/commands/screen_guide.rs`) plus missing `binaries/fndr-*` placeholders. |
| Linux build with a local shim | **Works.** A 55-line, never-committed shim (cfg-gates those call sites, stubs Vision's CGRect, links with `--unresolved-symbols=ignore-all`) builds `cargo test --no-run --lib --example retrieval_qa --example seed_demo` in 7.5 minutes. I keep the shim as uncommitted changes and stage files by explicit path only. |
| CI visibility | Yes: GitHub MCP tools read check runs and job logs on the draft PRs. |
| MiniLM download | Yes, from the pinned URL in `model_config.rs`; both sha256 pins match. |
| `make qa-seed` and `make qa-retrieval` on Linux | **Yes.** Seeding: 20 stored, 19 surfaced, 1 needs-signal. Retrieval: every per-query rank@10 identical to the M1 reference for both paths. Top-1 agreement 18/22 here versus 17/22 on the M1 (one non-relevant top result differs by platform; noted for VS-21). Latency is slower (Search p95 about 1000 ms, Ask about 2500 ms; debug build, no Metal) and is not gated. |
| Git identity | `anurupkumar18 <81anurup@gmail.com>` set in repo config; commits are signed by the environment. No trailers. |

### What this changes in the plan

1. Cloud runs `make qa-retrieval-check` itself on Linux and attaches the output to every search, chunking, and embedding change. Local still reruns on the M1 at the merge gate, but should expect identical ranks.
2. Rust verification: I run the relevant `cargo test` targets locally on Linux before pushing; macOS CI is the second check, not the only one.
3. N10 is worth filing as a small ticket (proposal below). With it, CI could also run the retrieval gate on `ubuntu-latest` (cheaper than macos-14 for VS-34), since ranks match across platforms.
4. Branch policy: I only rebase (with `--force-with-lease`) commits that local has not merged. If you are mid-gate on a branch, say so in LOCAL-OUTBOX and I will stack on it instead.

## Tickets

### VS-04 Retrieval report as a merge gate: delivered (pending local gate)

- Branch `claude/train-a-measure`, commit 5e2610d, draft PR #21.
- Evidence: `docs/evidence/W03/VS-04-cloud.md`.
- Comment to post:

> Cloud delivered VS-04 on `claude/train-a-measure` (5e2610d, draft PR https://github.com/anurupkumar18/FNDR/pull/21). `make qa-retrieval-check` reseeds the QA profile (`QA_SKIP_SEED=1` skips), reruns `retrieval_qa` into `src-tauri/target/qa-retrieval-check/`, and compares with `scripts/demo/retrieval-reference/knowledge-worker.json` (the VS-01 JSON, promoted unchanged). It fails on a Recall@5 drop over 0.05 on any path, or when a query a path found in its top ten becomes a miss; MRR@10, per-kind recall, latency, and top-1 agreement are reported, not gated. TEAM.md "Definition of done" now asks search, capture-text, chunking, and embedding MRs to paste it.
> Done-when check: raising `HARD_COVERAGE_THRESHOLD` from 0.15 to 0.34 on purpose made the check exit with `Error 1` and name the four paraphrase queries Search lost (Search Recall@5 0.955 to 0.773). The unmodified code passes with identical ranks.
> Comparator tests: `python3 scripts/audit/test_retrieval_check.py` 20 OK. Ran on Linux (cloud); please rerun the pass case on the M1 at the gate.
> Evidence: `docs/evidence/W03/VS-04-cloud.md`.

### VS-23 Vault reads like your work: delivered (owner check open)

- Branch `claude/train-c-ux`, commit 82792e2, draft PR #22.
- Evidence: `docs/evidence/W03/VS-23-cloud.md` plus six preview screenshots in `docs/evidence/W03/VS-23/`.
- Comment to post:

> VS-23 built on `claude/train-c-ux` (82792e2, draft PR https://github.com/anurupkumar18/FNDR/pull/22): the Vault groups by day, then project (or app) thread, newest first, with near-duplicates folded behind "N similar". Rows show a source icon and an "Open source" button that reuses `reopen_memory`, so any item opens in two clicks or fewer. The graph strip sits behind a "Connections" toggle and only loads when it is on.
> Tests: typecheck exit 0, `npm test` 74 files and 476 tests passed (18 new), `npm run build` passed. Browser-preview before and after screenshots (synthetic data) in `docs/evidence/W03/VS-23/`.
> Needs owner: open the Vault on the seeded profile in the real app, confirm one screen per day, and add real-app screenshots.
> Follow-up (cloud, train C): add `source_type` and `session_id` to the card data so agent notes get their icon and threads follow real sessions.

### PD-03, PD-17, PD-01, PD-02, PD-04, PD-18: drafts delivered (owner decisions open)

- Branch `claude/train-e-docs`, draft PR #23. Commits: PD-03 f41d6f2; PD-17 f4c7a03; PD-01 c6aa4f2, 7c242f1, c91b18f (take all three); PD-02 b0dd965; PD-04 35a8313; PD-18 0497b6a.
- Evidence: `docs/evidence/W03/PD-{01,02,03,04,17,18}-cloud.md`.
- Comments to post:

**PD-03**
> Cloud draft on `claude/train-e-docs` (f41d6f2): `docs/team/decision-log.md` with the process and 10 rows seeded only from decisions already recorded as accepted (ADRs 014, 015, 017, 020 to 023; master plan D-9; month plan section 12 rows 2 and 3), plus an Open table. TEAM.md has the weekly rhythm as a table with post templates and the response norm (reply within one working day; blocked over a day means ping the lane owner and the lead). Needs the owner: post the first Monday plan with the template and link it here. Evidence: `docs/evidence/W03/PD-03-cloud.md`.

**PD-17**
> Draft charter in `docs/team/TEAM.md`, "Team charter (draft, pending owner approval)" (f4c7a03): decision rights by kind and by lane, the disagreement rule (written options, 48 hours, lead decides and records), and handoff when away. Marked not approved and listed under Open in the decision log. Needs: owner edits and approval, then acknowledgements from Kunj, Minh, and Felipe recorded as a decision-log row. Evidence: `docs/evidence/W03/PD-17-cloud.md`.

**PD-01**
> `docs/decisions/018-reasoning-tier.md`, Status Proposed (c6aa4f2, 7c242f1, c91b18f). Three options, each with what leaves the Mac, Privacy Activity logging, Keychain key storage, how quality is measured, and risks. A code read shows what already leaves today: the Screen Guide ChatGPT path and the Hermes cloud provider, neither counted in Privacy Activity, both with file-stored credentials. Placeholder for PD-08's table; exact one-line edits to make on acceptance. No code wired. Needs: PD-08 from Kunj, then the owner's choice. Evidence: `docs/evidence/W03/PD-01-cloud.md`.

**PD-02**
> `docs/product/positioning.md` (b0dd965): who it is for, the problem in their words (marked as hypothesis until PD-18 and PD-13), the one sentence, three proof points tied to committed evidence with their limits, what we are not, and a six-product teardown with sources accessed 2026-10-04 (vendor claims marked [V]). Needs: the team adopts the sentence; real quotes replace the hypothesis lines. Evidence: `docs/evidence/W03/PD-02-cloud.md`.

**PD-04**
> `docs/product/qa-prep.md` (35a8313): what the panel rewards (labeled assumptions; no rubric in the repo), every section 9 beat mapped to its evidence file or to "missing" plus the ticket that produces it, ten hard questions answered from committed evidence, and a dry-run checklist and notes template. Needs a human: one dry run with someone outside the team, under five minutes, notes as `docs/evidence/W04/PD-04-dry-run-<date>.md`; and the real rubric. Evidence: `docs/evidence/W03/PD-04-cloud.md`.

**PD-18**
> `docs/research/conversations.md` (0497b6a): public-repo rules, a consent note, the 20-minute guide (finding things, getting back in after an interruption, AI tools, demo only at the end), a note template with no identifying fields, and two empty entries. Needs the owner: hold two conversations and fill both entries. Evidence: `docs/evidence/W03/PD-18-cloud.md`.

### Findings from train E worth the owner's attention

1. "Strictly local" is already partly untrue: Screen Guide's opt-in ChatGPT path and Hermes with a cloud provider send text off the Mac, neither is counted by `record_egress`, and both store credentials as files rather than in the Keychain (details in ADR-018 "What is true today"; VS-37 is the nearest ticket).
2. The capture privacy gate skips frames that match a secret pattern; it does not redact them. Docs that say "redaction" overstate it.
3. `make eval` is referenced by the master plan and the gold-set README but does not exist in the Makefile.
4. The "opens the PDF on page 112" demo beat is weak for Preview (no page-jump API, RE-04 note).

## Proposed new work

- **N10** Make the crate build and test on Linux (written up below; will move into `docs/team/tickets/proposed/cloud-proposals.md` on `claude/train-e-docs`). The files are local-owned, so this is a contract note: I will not edit them.

### N10 contract note (local-owned files)

The only blockers to `cargo test --lib` on Linux are:

1. `objc2-screen-capture-kit` sits in `[dependencies]`; move it to `[target.'cfg(target_os = "macos")'.dependencies]` (no lockfile change).
2. `#[link(name = ..., kind = "framework")]` in `accessibility/mod.rs` (3), `capture/macos.rs` (2), `ocr/vision.rs` (1): make them `#[cfg_attr(target_os = "macos", link(...))]` and give the extern functions non-mac stubs (or gate their callers) so no linker flag is needed.
3. `use objc2_app_kit::NSWorkspace` in `accessibility/mod.rs` and `capture/macos.rs`; `objc2`/`objc2_foundation` imports and `class!` in `ocr/vision.rs`; `objc2_foundation::MainThreadMarker` in `ipc/commands/notch.rs:199`; `app.hide()` in `ipc/commands/screen_guide.rs:2641`: cfg-gate, with non-mac stubs returning `None`, an "Unknown" context, or `OcrError::InitializationError`.
4. `tauri.conf.json` `externalBin` and the `FNDR Speech Helper.app` resource must exist at build time; CI or a `build.rs` branch can create empty placeholders for non-Apple targets.

My shim does exactly this in 55 lines, but uses a linker flag instead of stubs; the clean version is about 2 hours. Payoff: Linux CI (cheaper, faster) for Rust tests and the retrieval gate (VS-34), and cloud sessions that verify Rust before pushing.
