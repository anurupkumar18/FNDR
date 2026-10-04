# CLOUD-OUTBOX

Cloud session (Linux container, GitHub only) to local session. Updated after every ticket and at least every two hours. Newest entries first under each heading.

Last update: 2026-10-04 23:45 UTC. Cloud has read LOCAL-OUTBOX at 9b05bbf (main at b948dfd).

## NEEDS HUMAN (open)

None.

## Read first (local)

1. **Security fix, merge early:** VS-41 on `claude/train-f-new` (PR #26). In Local mode a loopback JSON-RPC **batch** that starts with `initialize` skipped the token check for every item, so `[initialize, tools/call]` ran any tool without the token (against ADR-017). Fixed and covered by the existing HTTP auth test; it touches only `jsonrpc_method_hint` in `src-tauri/src/mcp/mod.rs`. Please also check whether a follow-up task was created for it in the Claude app (a sub-agent tried and timed out); VS-41 is that follow-up, file it from `docs/team/tickets/proposed/cloud-proposals.md` once I add it there (next docs push).
2. **GitHub Rust CI has been red on main since at least 2026-09-30.** The Swift speech helper uses `SpeechAnalyzer` (macOS 26 SDK only) and `test.yml` runs on `macos-14`, so `build.rs:25` panics before any test. Draft PR #24 moves the job to `macos-26`: the build then passes and 885 of 886 lib tests pass. The one failure, `memory_journey::tests::six_synthetic_journeys_reconstruct_from_temporary_storage_and_search`, is load-dependent: all embeddings in that test are equal, so the result rests on the keyword route, whose 320 ms per-variant budget drops hits on a slow runner (same root cause as below). BM25 (VS-07) makes the keyword route fast; train B (#25) carries the ported `macos-26` change so its CI will show whether that test passes there. `release.yml` still uses `macos-14` and will hit the same Swift error.
3. **Retrieval timing bug (product):** route time budgets (`keyword_variant_timeout_ms` 320, `keyword_timeout_ms` 900) silently drop keyword hits when a `LIKE` scan is slow, so Ask results can change with CPU load. The eval now lifts budgets (VS-04 second commit) so the gate is deterministic; VS-07 replaces the scan with BM25 (Search p95 1357 to 288 ms on the knowledge-worker set).
4. **Ask depends on Search (product, for VS-21):** the same Ask query ranks the target card 2nd in a fresh process and 11th when Search ran earlier in the same process (office-PM profile, "the product requirements doc I drafted last week"). First suspect: the text-keyed embedding cache in `src-tauri/src/embedding/onnx.rs`. The gate is unaffected (fixed order), but users would see it.
5. Evidence runs must reseed: recency scores age, so a profile seeded hours earlier drifts (office-PM Ask MRR 0.636 old seed versus 0.661 fresh). `make qa-retrieval-check` reseeds by default.

## Merge queue for local (in this order)

| # | Branch | Draft PR | Head | Tickets | Rust CI | Notes |
|---|---|---|---|---|---|---|
| 1 | `claude/train-f-new` | #26 | 8c96a07 | VS-41 (security fix, 2 commits), VS-35 spec | pending | Independent of A and B. Carries the ported `macos-26` CI commit; drop it if #24 lands first. |
| 2 | `claude/train-a-measure` | #21 | aed1315 | VS-04 (2 commits), VS-02, VS-03 | red on main's Swift issue | Train B contains all of A, so #25's CI covers this code. |
| 3 | `claude/train-b-retrieval` | #25 | 74963fa | (A) + VS-05, VS-07 (2 commits; closes VS-06) | pending on macos-26 | Stacked on A. Carries the ported `macos-26` CI commit. |
| 4 | `claude/train-c-ux` | #22 | 82792e2 | VS-23 | red on main's Swift issue (no Rust in diff) | Frontend CI green. |
| 5 | `claude/train-e-docs` | #23 | ca9c383 | PD-03, PD-17 draft, PD-01 draft (3), PD-02, PD-04, PD-18 guide, PD-05, proposals (VS-40), session log | red on main's Swift issue (no Rust in diff) | |
| 6 | `claude/ci-macos-26` | #24 | 1 commit | CI runner fix | build passes, 1 test fails (see Read first 2) | `.github/workflows/**` is yours to merge. |
| 7 | `claude/train-d-chunks` | (not yet) | | VS-17 and VS-19 harness (sub-agent running) | | Based on train A. |

Merge trials: `git merge-tree` shows A with E and A with C merge cleanly; B is A plus commits; F touches `src-tauri/src/mcp/mod.rs`, which no other train touches. Every file each train touches is free of em and en dashes (pre-existing dashes in touched files were replaced, so your whole-file dash scan passes).

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

### VS-41 (proposed ID) MCP auth for every item of a batch: delivered (security)

- `claude/train-f-new`, b8a2412 (fix) and f27ab97 (dash cleanup in the touched file), PR #26. Evidence `docs/evidence/W03/VS-41-cloud.md`.
- Comment to post (after filing the ticket):

> Cloud fixed on `claude/train-f-new` (b8a2412, PR https://github.com/anurupkumar18/FNDR/pull/26): in Local mode the loopback handshake exemption took its method from the first item of a JSON-RPC batch, so `[initialize, tools/call]` ran a tool with no token. A batch is now exempt only when every item is a handshake method. The existing HTTP auth test gains a batch case that returned 200 on the old code and returns 401 now, plus a handshake-only batch that stays 200. `cargo test --lib` 883 passed, three runs (Linux). Evidence: `docs/evidence/W03/VS-41-cloud.md`.

### VS-35 fndr.remember spec and injected-note corpus: delivered (owner decisions open)

- `claude/train-f-new`, 3792c46, PR #26. Evidence `docs/evidence/W03/VS-35-cloud.md` (10 open questions with recommended defaults).
- Comment to post:

> Spec and corpus on `claude/train-f-new` (3792c46): `docs/product/fndr-remember-spec.md` and `src-tauri/tests/fixtures/agent_notes/injected-notes.json` (30 synthetic cases, 10 benign); evidence in `docs/evidence/W03/VS-35-cloud.md`. Capture skips secrets rather than redacting them, so `fndr.remember` refuses them with `sensitive_content` using the same detector, without echoing the text. Notes are leaf records (`source_type = "agent"`, client from the MCP session, no schema migration, never merged or fed into project context, graph, or memory review); 39 named tests mapped to the corpus. The batch-auth gap the spec found is fixed as VS-41 in the same PR. Needs the owner: 10 questions with recommended defaults in the evidence file.

### VS-07 BM25 keyword search: delivered (pending local gate); VS-06: closed by VS-07

- `claude/train-b-retrieval`, 42a4c98 and fd54578, PR #25. Evidence `docs/evidence/W03/VS-07-cloud.md`.
- Comments to post:

**VS-07**
> Cloud delivered on `claude/train-b-retrieval` (42a4c98, PR https://github.com/anurupkumar18/FNDR/pull/25): `keyword_search` now runs one LanceDB full-text BM25 query over seven text columns (inverted index per column, created on first use; rows written later are found by a flat scan and folded into the index in the background after 256 rows). Scores use bm25/(bm25+2) because normalizing to the best hit inflated weak matches and cost Ask paraphrase recall (measured, dropped). Three new tests fail on the old LIKE code and pass now (rare term over common, best match stored last among 500, no match returns nothing); 891 lib tests pass. At 10,000 rows (Linux, unoptimized build): p95 290 ms; folding 200 new rows 189 ms; one-time index build 2.3 s. Retrieval check (both personas reseeded): no Recall@5 change; Search p95 1357 to 288 ms and 827 to 374 ms; office-PM Ask time Recall@5 0.875 to 1.000. Please rerun the 10k test on the M1: `cargo test --test storage_scale keyword_search_latency_at_10k_rows -- --ignored --nocapture`. Evidence: `docs/evidence/W03/VS-07-cloud.md`.

**VS-06**
> Superseded by VS-07 (BM25), as the plan allows: the VS-06 test (best match stored last among 500 rows) fails on the old keyword scan and passes on `claude/train-b-retrieval` (42a4c98); there is no early limit before scoring any more. Evidence: `docs/evidence/W03/VS-07-cloud.md`, section VS-06.

### VS-05 remove the hard word-overlap cutoff: delivered (pending local gate)

- `claude/train-b-retrieval`, 7c60577, PR #25. Evidence `docs/evidence/W03/VS-05-cloud.md`.
- Comment to post:

> Cloud delivered on `claude/train-b-retrieval` (7c60577): Search's reranker no longer drops results sharing under 15% of the query's words; coverage is a soft 0.3 weight. Two tests fail on the old code and pass now. Retrieval check: office-PM Search Recall@5 0.700 to 0.800, paraphrase 0.333 to 0.556, MRR@10 0.589 to 0.664; keyword rows and the knowledge-worker set unchanged; no query got worse. Removing the hybrid relevance gate's overlap rules too gave no further gain and was reverted (written up). References ratcheted. Evidence: `docs/evidence/W03/VS-05-cloud.md`.

### VS-03 time, app, and no-match queries: delivered (pending local gate)

- `claude/train-a-measure`, aed1315, PR #21. Evidence `docs/evidence/W03/VS-03-cloud.md`.
- Comment to post:

> Cloud delivered on `claude/train-a-measure` (aed1315): each persona gains 8 time, 5 app, and 4 negative queries (relative time phrases only, so answers do not depend on the seeding weekday). The report moves to schema v2 (open kind maps, each path's top score per query, a no-match block); the headline Recall@5 still covers keyword and paraphrase only, so it stays comparable with VS-01. The gate compares v1 and v2. Finding: most time and app queries are answerable by topic alone (Recall@5 near 1.0 with no parser), so three discriminating queries per persona were added and VS-13 should be judged on those. Negative queries: Search "returns nothing" only because of its overlap cutoff, so VS-12 needs a score threshold. Evidence: `docs/evidence/W03/VS-03-cloud.md`.

### VS-02 office-PM persona and 20 queries: delivered (pending local gate)

- `claude/train-a-measure`, 28ce9b4, PR #21. Evidence `docs/evidence/W03/VS-02-cloud.md`, baseline `docs/evidence/W03/retrieval-baseline-office-pm.md`.
- Comment to post:

> Cloud delivered on `claude/train-a-measure` (28ce9b4): `scripts/demo/office-pm-week.json` (40 synthetic memories: launch plan, hiring loop, quarterly metrics; two near-duplicate documents; one four-day thread) and `office-pm-queries.json` (20 queries: 11 keyword, 9 paraphrase; 5 with a name, 5 with a number or code). `make qa-seed PERSONA=office-pm`, `make qa-retrieval PERSONA=office-pm`, and `make qa-retrieval-check PERSONA=office-pm` use their own profile. Baseline: Search Recall@5 0.700 (paraphrase 0.333), Ask 0.900 (paraphrase 0.778), top-1 agreement 3/20; the knowledge-worker set was too easy. Example tests: 11 passed. Evidence: `docs/evidence/W03/VS-02-cloud.md`.

### PD-05 Friday scoreboard: delivered (two Friday posts open)

- `claude/train-e-docs`, a2f7df1, PR #23. Evidence `docs/evidence/W03/PD-05-cloud.md`.
- Comment to post:

> Scoreboard on `claude/train-e-docs` (a2f7df1): `make scoreboard` prints one Markdown page from the retrieval reports (schema v1 and v2, one row per persona and path), vault health, and linked voice and session files, marking the month-plan Beta targets met or not met; anything missing shows "not measured". Tests: 24 (23 pass, 1 skipped in the cloud without numpy; all pass with numpy). First run on W02 evidence: knowledge-worker Search Recall@5 0.955 (met), paraphrase 0.875 (met), same top result 17/22 (not met); vault median 129 chars, project and next steps 0.0%, 0 chunk rows, exact reopen 10.3% (all not met). Done when posted on Oct 9 and Oct 16. Evidence: `docs/evidence/W03/PD-05-cloud.md`.


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

- **VS-40 (N10)** Make the Rust crate build and test on Linux: in `docs/team/tickets/proposed/cloud-proposals.md` on `claude/train-e-docs` (ca9c383 parent c7fef59). The files are local-owned, so this is a contract note; the clean version is about 2 hours and lets CI run the retrieval gate on `ubuntu-latest`.
- **VS-41** MCP batch auth (above): fix already on train F; ticket text to be added to the proposals file.

## In progress (cloud)

- Train B next: VS-08 (rank fusion), then VS-09 (`retrieve`), VS-13, VS-10, VS-11, VS-12, VS-21, VS-25.
- Sub-agent: VS-17 embedding bake-off and VS-19 cross-encoder spike harness on `claude/train-d-chunks` (based on train A).
