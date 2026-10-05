# CLOUD-OUTBOX

Cloud session (Linux container, GitHub only) to local session. Updated after every ticket and at least every two hours. Newest entries first under each heading.

Last update: 2026-10-05 03:50 UTC. Cloud has read LOCAL-OUTBOX at 9b05bbf (main at b948dfd).

## NEEDS HUMAN (open)

None.

## Read first (local)

1. **Security fix, merge early:** VS-41 on `claude/train-f-new` (PR #26). In Local mode a loopback JSON-RPC **batch** that starts with `initialize` skipped the token check for every item, so `[initialize, tools/call]` ran any tool without the token (against ADR-017). Fixed and covered by the existing HTTP auth test; it touches only `jsonrpc_method_hint` in `src-tauri/src/mcp/mod.rs`. Please also check whether a follow-up task was created for it in the Claude app (a sub-agent tried and timed out); VS-41 is that follow-up, file it from `docs/team/tickets/proposed/cloud-proposals.md` once I add it there (next docs push).
2. **GitHub Rust CI has been red on main since at least 2026-09-30.** The Swift speech helper uses `SpeechAnalyzer` (macOS 26 SDK only) and `test.yml` runs on `macos-14`, so `build.rs:25` panics before any test. Draft PR #24 moves the job to `macos-26`: the build then passes and 885 of 886 lib tests pass. The one failure, `memory_journey::tests::six_synthetic_journeys_reconstruct_from_temporary_storage_and_search`, depends on runner load: all embeddings in that test are equal, so the result rests on the keyword route, whose 320 ms per-variant budget drops hits on a slow runner. Correction to my earlier note: it is intermittent, not certain. Train D (#27) passed it on macos-26 without BM25, while #24 and train F (#26) failed it. Train B (#25) passes it on every push since VS-07 (BM25), and is green on macos-26 at 483b6e1. Simplest order: merge #24, then train B. `release.yml` still uses `macos-14` and will hit the same Swift error.
3. **Retrieval timing bug (product):** route time budgets (`keyword_variant_timeout_ms` 320, `keyword_timeout_ms` 900) silently drop keyword hits when a `LIKE` scan is slow, so Ask results can change with CPU load. The eval now lifts budgets (VS-04 second commit) so the gate is deterministic; VS-07 replaces the scan with BM25 (Search p95 1357 to 288 ms on the knowledge-worker set).
4. **Same query, different results (product, VS-21, fixed on train B at fd03afa):** three causes, each measured. (a) The temporal route gave a placeholder score of 1.0 to every memory in the asked-about window and kept a `HashMap`-ordered `limit` of them, so a random subset got the time bonus on each call. (b) `QueryProfile.number_terms` was a `HashSet` joined into the text the vector route embeds, so queries with two or more numbers ("LL-1482 spam placement 1.8%") embedded different text on each call. (c) Every route sized its candidate pool from the caller's page size, so Ask (10) and Search (20) ranked differently. The earlier "Ask depends on Search" observation is (a) and (b), not the embedding cache.
5. Evidence runs must reseed: recency scores age, so a profile seeded hours earlier drifts (office-PM Ask MRR 0.636 old seed versus 0.661 fresh). `make qa-retrieval-check` reseeds by default.

## Merge queue for local (in this order)

| # | Branch | Draft PR | Head | Tickets | Rust CI | Notes |
|---|---|---|---|---|---|---|
| 1 | `claude/train-f-new` | #26 | 8c96a07 | VS-41 (security fix, 2 commits), VS-35 spec | macos-26: 885/886, the intermittent memory_journey test (see Read first 2) | Independent of A and B. Carries the ported `macos-26` CI commit; drop it if #24 lands first. |
| 2 | `claude/train-a-measure` | #21 | aed1315 | VS-04 (2 commits), VS-02, VS-03 | red on main's Swift issue | Train B contains all of A, so #25's CI covers this code. |
| 3 | `claude/train-b-retrieval` | #25 | 7f4e673 | (A) + VS-05, VS-07 (3 commits; closes VS-06), VS-08 (evidence only), VS-09, VS-13, VS-10, VS-11, VS-21, VS-12, VS-25: **train complete** | green on macos-26 at 483b6e1; later heads running | Stacked on A. Carries the ported `macos-26` CI commit (74963fa). VS-10 makes Search show weak results for no-match queries until VS-12; merge the train whole. |
| 4 | `claude/train-c-ux` | #22 | 82792e2 | VS-23 | red on main's Swift issue (no Rust in diff) | Frontend CI green. |
| 5 | `claude/train-e-docs` | #23 | ca9c383 | PD-03, PD-17 draft, PD-01 draft (3), PD-02, PD-04, PD-18 guide, PD-05, proposals (VS-40), session log | red on main's Swift issue (no Rust in diff) | |
| 6 | `claude/ci-macos-26` | #24 | c6cf285 | CI runner fix | build passes, the one memory_journey test fails until VS-07 lands | `.github/workflows/**` is yours to merge. |
| 7 | `claude/train-d-chunks` | #27 | 94747d6 | VS-17 and VS-19 harnesses (Python and docs only), ADR 019 Proposed | **green on macos-26** | Stacked on A. Carries the `macos-26` CI commit. |

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

### VS-25 retired search code: delivered (pending local gate); contract notes for five callers

- `claude/train-b-retrieval`, 7f4e673, PR #25. Evidence `docs/evidence/W03/VS-25-cloud.md`.
- Comment to post:

> Cloud delivered on `claude/train-b-retrieval` (7f4e673): the raw `search`/`search_raw_results` commands (Pipeline Inspector), autofill, and quality checks rank through `retrieve`; the LLM query expansion for short abstract queries moved into `retrieve` (it fixes a VS-10 regression: Search lost the expansion whenever a local model was loaded; untested here, no local model in the cloud). Removed `HybridSearcher::search_with_expansion(_explained)`, `fuse_and_rerank`, the explain plumbing, `reranker::rerank_results`, and the IPC module's own hybrid search. Lib tests 907 to 906 (two `rerank_results` tests go with it, one coverage test added). Same seed: every rank and metric identical. Five callers outside this lane keep the engine: `src/companion/handlers/search.rs`, `src/graph/legacy.rs:388`, MCP `memory.search_raw` (PD-11), `tests/search_relevance_eval.rs`, `examples/fndr_diagnostic.rs`; proposals in the evidence. Evidence: `docs/evidence/W03/VS-25-cloud.md`.

### VS-12 "No strong matches": delivered, done-when NOT met (negative result)

- `claude/train-b-retrieval`, f44f0a1, PR #25. Evidence `docs/evidence/W03/VS-12-cloud.md`.
- Comment to post:

> Cloud built on `claude/train-b-retrieval` (f44f0a1): `retrieve` reports `strong_match` (best hit at or above 0.25, or containing every query word; the second rule keeps exact matches strong with no embedding model, where a perfect match fuses to 0.20); Search marks cards `weak_match` and shows "No strong matches" with the weak results behind "Show N weaker matches"; the report and gate add negatives and positives under the bar. Result: 3 of 8 no-match queries (2/4 knowledge-worker, 1/4 office-PM) and 0 real queries under the bar. The done-when (all negatives pass) is not met: on these sets no-match queries reach 0.351 while real ones go down to 0.290, and vector similarity, word coverage, and score shape do not separate them either; hiding weak results one by one would hide two right answers ranked below a wrong one at 0.215. Next candidate signal: chunk scores (VS-18). Also fixed: the gate's reference test, broken since VS-09. Evidence: `docs/evidence/W03/VS-12-cloud.md`.

### VS-21 same query, same results: delivered (pending local gate)

- `claude/train-b-retrieval`, fd03afa, PR #25. Evidence `docs/evidence/W03/VS-21-cloud.md`. Done ahead of VS-12, whose threshold needs stable scores.
- Comment to post:

> Cloud delivered on `claude/train-b-retrieval` (fd03afa, PR https://github.com/anurupkumar18/FNDR/pull/25). Three measured causes: (1) the temporal route gave every in-window memory the range query's placeholder score 1.0 and kept a `HashMap`-ordered `limit` of them, so a random subset got the time bonus (now scored by recency in the window); (2) `QueryProfile.number_terms` was a `HashSet` joined into the text the vector route embeds, so queries with two or more numbers embedded different text per call (now a `BTreeSet`); (3) routes sized their candidate pools from the page size, so Ask (10) and Search (20) ranked differently (now a fixed pool of 50). Ties break by score, then newest, then id in routes, fusion, and card grouping. Tests: five runs through `retrieve`, Search, and Ask return identical ids; a short page is the start of a long one; number order is fixed; each failed before. Seeded profiles, five runs of every labeled query: 39/39 and 37/37 on all three paths in a second pass (one office-PM query differed once in the first pass and did not reproduce in 100+ later calls; minute-granular recency can swap a near-tie across a minute boundary). Same seed: Recall@5 and MRR@10 unchanged; every query has the same rank on all three paths; Search/Ask top-1 39/39 and 37/37. Evidence: `docs/evidence/W03/VS-21-cloud.md`.

### VS-11 Ask, agents, and every MCP search tool through `retrieve`: delivered (pending local gate)

- `claude/train-b-retrieval`, 483b6e1, PR #25. Evidence `docs/evidence/W03/VS-11-cloud.md`.
- Comment to post:

> Cloud delivered on `claude/train-b-retrieval` (483b6e1, PR https://github.com/anurupkumar18/FNDR/pull/25): `run_query` (Ask, `fndr_search`/`fndr_answer`, MCP `fndr.search`/`fndr.answer`) starts from `retrieve`, so Ask reads time and app phrases like Search; MCP `search_memories`, `memory.search_full_context` (`keyword_matches` is the keyword-route subset of the same list), `ask_fndr`'s sources, and `build_context_pack`'s candidates come from `retrieve` instead of their own `HybridSearcher` calls (which also loaded the model per call). Tool names unchanged pending PD-11; `memory.search_raw` keeps its raw lists. Resume Work takes no query, so nothing to move. Contract test `mcp::tests::search_tools_return_the_search_screens_ids_in_its_order`: Search screen ids equal all three MCP tools' ids in order for four queries (one with an app phrase); failed on the old code. Same seed: Recall@5 and MRR@10 unchanged; Search/Ask top-1 agreement 39/39 and 36/37. `cargo test --lib` 904 passed. The MCP file's dash fixes match train F's f27ab97 line for line. Evidence: `docs/evidence/W03/VS-11-cloud.md`.

### VS-10 the Search screen ranks through `retrieve`: delivered (pending local gate)

- `claude/train-b-retrieval`, 5341c0c, PR #25. Evidence `docs/evidence/W03/VS-10-cloud.md`.
- Comment to post:

> Cloud delivered on `claude/train-b-retrieval` (5341c0c): `search_ranked_results` is a thin wrapper over `retrieve` (plus each hit's stored row from the lookup it already makes), so the Search screen, Ask, and agents rank the same way and the report measures what users see; card synthesis unchanged. `retrieve` now drops FNDR's own windows as Search did; Ask and `retrieve` reuse the loaded embedding model (loading it cost 171 to 263 ms per query here). Fusion ties break by id and recency counts whole minutes so repeated searches are identical (the Memory Journey test needs it). Same seed: Search Recall@5 0.955 to 1.000 and 0.800 to 0.900 (office-PM paraphrase 0.556 to 0.778); Ask ranks unchanged; `retrieve` p50 about 470 to 200 ms. Top-1 agreement 28/39 to 37/39 and 16/37 to 35/37 (the rest closed by VS-11). Known until VS-12: Search shows weak results for no-match queries (it returned nothing for 3 of 4 and 1 of 4 before). `make qa-preview` cannot exercise this (IPC is mocked in `src/dev/previewIpc.ts`); no frontend change. Evidence: `docs/evidence/W03/VS-10-cloud.md`.

### VS-13 query phrases become filters: delivered (pending local gate)

- `claude/train-b-retrieval`, 3dade40, PR #25. Evidence `docs/evidence/W03/VS-13-cloud.md`.
- Comment to post:

> Cloud delivered on `claude/train-b-retrieval` (3dade40, PR https://github.com/anurupkumar18/FNDR/pull/25): `retrieve` reads today, yesterday, this morning/afternoon/evening, last night, last week, N days ago, weekday names, dates, and "in/on/from/using <app>" (only apps stored in the vault, via aliases such as Excel, Chrome, Zoom, VS Code) into a time range and an app filter applied to every route; explicit filters win; a phrase-read filter that finds nothing falls back to the whole vault. Look-alikes stay text ("Monday.com", "the today show", "in the PDF"). The ticket said to strip the phrase from the text; measured, that cost one office-PM query (rank 1 to 4) because the planner loses its time intent, so the full text is still searched and the phrase-free text only drives `matched_terms`. Retrieve, both personas reseeded (America/Denver): time and app Recall@5 and MRR@10 1.000 (from 0.938/0.900 and 0.917/1.000 MRR); keyword and paraphrase unchanged. Tests: parser 6 (four time zones), `tests/retrieve.rs` 7, lib context_runtime and lance_store 94. Also: `get_app_names` reads one column instead of every column. Evidence: `docs/evidence/W03/VS-13-cloud.md`.

### VS-09 one `retrieve` function: delivered (pending local gate)

- `claude/train-b-retrieval`, 1b5d623, PR #25. Evidence `docs/evidence/W03/VS-09-cloud.md`.
- Comment to post:

> Cloud delivered on `claude/train-b-retrieval` (1b5d623): `context_runtime::retrieve(state, RetrieveRequest { query, time, app, limit }) -> RetrieveResult { hits: [{ memory_id, chunk_id, score, why: { routes, matched_terms } }] }`, sharing one front half (`retrieve_fused`: plan, routes, fusion, low-signal drop) with Ask, so both rank the same memories in the same order (tested on three queries). Types derive serde and specta for VS-10 and VS-11. Found on the way: the low-signal drop looked up each hit separately (about 1.1 s per office-PM query); one batched `get_memories_by_ids` brings `retrieve` to 0.43 to 0.64 s and Ask from about 3.0 to 2.0 s. Report gains a third path: retrieve Recall@5 1.000 and 0.900, equal to the better path on both personas; MRR equal to Ask; p95 575 and 515 ms, about twice Search, so VS-20 matters before VS-10 ships. Evidence: `docs/evidence/W03/VS-09-cloud.md`.

### VS-08 rank fusion: measured, not adopted (negative result)

- `claude/train-b-retrieval`, 05edde9 (evidence only, no code change), PR #25. Evidence `docs/evidence/W03/VS-08-cloud.md`.
- Comment to post:

> Measured and rejected by a rule set before the last run (adopt only if Recall@5 does not drop on either persona and MRR@10 does not drop). RRF at k = 30, 60, 90 gives identical numbers; it loses one paraphrase query per persona from the top five (Ask Recall@5 1.000 to 0.955 and 0.900 to 0.850) because every weak one-word keyword hit gets a full rank vote, while gaining office-PM MRR (0.661 to 0.685; a score-weighted variant 0.705). Ask keeps weighted score fusion. The tested function is in the evidence file; rerun when VS-18 adds chunk routes. Evidence: `docs/evidence/W03/VS-08-cloud.md`.

### VS-17 embedding bake-off: harness and cloud numbers delivered (M1 columns open)

- `claude/train-d-chunks`, 617dcf6, PR #27. Evidence `docs/evidence/W03/VS-17-bakeoff-cloud.md`; ADR `docs/decisions/019-embedding-model.md` (Proposed).
- Comment to post:

> Harness `scripts/audit/embedding_bakeoff.py` on `claude/train-d-chunks` (617dcf6, PR https://github.com/anurupkumar18/FNDR/pull/27) scores MiniLM-L6, bge-small-en-v1.5, EmbeddingGemma-300m (768 and a 256 cut), and Qwen3-Embedding-0.6B by pure vector retrieval on both personas, record text and ~300-token chunks. Cloud: EmbeddingGemma recalls 41/42 and 42/42 against MiniLM's 39/42 and 40/42 (not significant) and lifts MRR@10 0.771 to 0.913 and 0.751 to 0.889 (sign test p = 0.039 and 0.013); bge-small and Qwen3 are indistinguishable from MiniLM. ADR 019 drafted as Proposed. Open for local: the M1 latency and peak-RSS columns, and EmbeddingGemma's license check. Note: `embedding/onnx.rs` would mean-pool EmbeddingGemma's hidden state and skip its dense layers, so adopting it needs that code change first. Evidence: `docs/evidence/W03/VS-17-bakeoff-cloud.md`.

### VS-19 cross-encoder rerank spike: rejected for EmbeddingGemma; needs M1 only if MiniLM stays

- `claude/train-d-chunks`, 8cea73a, PR #27. Evidence `docs/evidence/W03/VS-19-spike-cloud.md`.
- Comment to post:

> `--rerank` on the bake-off harness (8cea73a) rescores the top 30 with ms-marco-MiniLM-L-6-v2. Reranked MRR@10 lands near 0.83 whatever the embedder: it lowers EmbeddingGemma (0.889 to 0.829), so rejected for the recommended embedder; over MiniLM it gains +0.080 (mostly office-PM) but still ranks below EmbeddingGemma alone, and p95 at 30 pairs is 345 to 1252 ms on this shared host against a 150 ms budget. If MiniLM stays, the M1 must show at least a 2.3x speedup. Evidence: `docs/evidence/W03/VS-19-spike-cloud.md`.

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

- Train B complete (PR #25 body rewritten with one Promise / How / What can go wrong / How we would know block and a per-ticket table).
- Now train D: VS-18 (chunk routes behind a flag against the EM-03 chunk table, synthetic chunk fixture), then VS-26 (README and walkthrough), VS-20 (10,000-memory harness).
