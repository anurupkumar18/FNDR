# Anurup: Vault and search

Mission: a person finds anything they have seen by what it meant or by the exact words, gets the same answer from every FNDR surface, and trusts the Vault as the place their work lives. Judged every Friday by `make qa-retrieval` and `make vault-health`.

Measurements from 2026-09-23 on the owner's profile: 29 memories in 5 active days, median 129 characters of stored text, chunk tables empty, two retrieval stacks, Search drops results with under 15% word overlap, keyword search is an unranked `LIKE` scan.

## VS-01 Record the retrieval and vault baseline before changing search
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p0
- milestone: W02-Measure
- estimate: 3h
- depends: none

**Why.** Every later search ticket is judged against these numbers; without them "better" is an opinion.

**Today.** Phase 0 Tasks 5 and 6 in `docs/superpowers/plans/2026-09-23-user-first-qa-reset.md` add `make vault-health` and `make qa-retrieval`.

**Do.**
1. Land Phase 0 Tasks 5 and 6 if not already merged.
2. `make qa-seed && make qa-retrieval`, then `make vault-health` on your real profile.
3. Save both outputs under `docs/evidence/W02/` and add a JSON copy of the retrieval scores (`--json` output, see VS-04).

**Done when.** Both files are committed and the Friday scoreboard quotes them.

**Evidence.** The two evidence files.

## VS-02 Add an office-PM persona week and 20 queries
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p1
- milestone: W02-Measure
- estimate: 4h
- depends: VS-01

**Why.** One synthetic student week is too easy and too narrow; knowledge workers in offices use Slack, Sheets, email, calendars, and slides all day.

**Today.** `scripts/demo/knowledge-worker-week.json` (20 memories) and `knowledge-worker-queries.json` (22 queries).

**Do.**
1. Write `scripts/demo/office-pm-week.json`: 40 synthetic memories across three projects (launch plan, hiring loop, quarterly metrics) with realistic OCR text, including two near-duplicate documents and one thread that spans four days.
2. Write `scripts/demo/office-pm-queries.json`: 20 queries, at least 8 paraphrase, 4 with a person's name, 4 with a number or code (ticket ID, dollar figure).
3. Extend the corpus test in `src-tauri/examples/retrieval_qa.rs` to check every expected id exists.
4. Add `make qa-retrieval PERSONA=office-pm`.

**Done when.** `make qa-retrieval PERSONA=office-pm` prints a table; the example tests pass.

**Evidence.** Baseline table for the second persona.

## VS-03 Add time, app, and "nothing matches" queries to both query sets
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p1
- milestone: W03-Build
- estimate: 3h
- depends: VS-02

**Why.** "What was I reading in Slack yesterday" and "the thing I never actually looked at" are real queries; today we measure neither.

**Do.**
1. Add 6 queries per persona with time phrases (yesterday, this morning, last Tuesday) and 4 with app phrases (in Slack, in the PDF).
2. Add 4 negative queries per persona whose correct answer is "no strong match"; score them as correct when the top result's score is under the no-match threshold you define in VS-12.
3. Report these three kinds as separate rows in the retrieval report.

**Done when.** The report shows keyword, paraphrase, time, app, and negative rows.

**Evidence.** Updated report.

## VS-04 Make the retrieval report a merge gate for search changes
- assignee: anurupkumar
- labels: area::vault-search, type::chore, prio::p0
- milestone: W02-Measure
- estimate: 3h
- depends: VS-01

**Why.** Search quality silently regressed before because nothing checked it.

**Do.**
1. Add `--json <path>` to `retrieval_qa` that writes per-path Recall@5, MRR@10, per-kind recall, and per-query ranks.
2. Commit `docs/evidence/retrieval-baseline.json` as the reference.
3. Add `make qa-retrieval-check` that reruns and fails when any path's Recall@5 drops more than 0.05 or any previously found query becomes a miss.
4. Add one line to `docs/team/TEAM.md`: search, capture-text, chunking, and embedding merge requests paste this output.

**Done when.** Breaking the reranker on purpose makes `make qa-retrieval-check` fail with the query that regressed.

**Evidence.** The failing and passing outputs.

## VS-05 Remove the hard word-overlap cutoff from Search
- assignee: anurupkumar
- labels: area::vault-search, type::bug, prio::p0
- milestone: W02-Measure
- estimate: 2h
- depends: VS-04

**Why.** Search throws away results that share under 15% of the query's words, which removes exactly the paraphrase matches semantic search is for.

**Today.** `src-tauri/src/search/reranker.rs`: `HARD_COVERAGE_THRESHOLD = 0.15` excludes results; score is `0.7 * vector + 0.3 * coverage`.

**Do.**
1. Write a failing test: a result with high vector similarity and zero shared words must survive reranking.
2. Delete the exclusion; keep coverage as a soft score only.
3. Run `make qa-retrieval-check`.

**Done when.** The test passes and paraphrase Recall@5 on the Search path goes up with no keyword row getting worse.

**Evidence.** Before and after report rows for paraphrase queries.

## VS-06 Stop the keyword branch from returning the first rows it finds instead of the best
- assignee: anurupkumar
- labels: area::vault-search, type::bug, prio::p2
- milestone: W02-Measure
- estimate: 2h
- depends: VS-04

**Why.** With more memories, the right keyword match can be cut before it is ever scored.

**Today.** `storage/lance_store/mod.rs:1693` `keyword_search`: `LOWER(col) LIKE '%term%'` over seven columns, `.limit(per_term_limit)` before `lexical_keyword_score`.

**Do.**
1. Failing test with 500 rows where the best match is stored last: it must rank first.
2. Interim fix: fetch candidate ids and the scoring columns without the early limit (cap at a safe maximum), score, then take the top N. VS-07 replaces this with BM25.

**Done when.** The test passes; `storage_scale` latency does not regress past 500 ms p95 at 10,000 rows.

**Evidence.** Test output and the latency number.

## VS-07 Add a BM25 full-text index to the memory table
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W02-Measure
- estimate: 6h
- depends: VS-04

**Why.** Real keyword ranking (rare words count more, repeated words count less) is what makes exact recall of names, codes, and phrases reliable.

**Today.** No full-text index. The `lancedb` 0.27.2 crate in `Cargo.toml` has `Index::FTS(FtsIndexBuilder)` and full-text queries (see `~/.cargo/registry/src/*/lancedb-0.27.2/src/index.rs`).

**Do.**
1. Create an FTS index on a single searchable text column (add `search_text` = title + URL + cleaned text + snippet if a multi-column index is not supported) when the table opens; make creation idempotent.
2. Keep the index current: after `add_batch`, call the table's optimize or index update on a schedule (not per insert), and measure its cost.
3. Replace the `LIKE` scan in `keyword_search` with a full-text query returning BM25 scores.
4. Tests: exact-phrase query, rare-term query, stemming behavior you choose to support, and an empty result.

**Done when.** Keyword rows beat the VS-01 baseline on both personas with no keyword query lost; latency p95 under 300 ms at 10,000 rows.

**Evidence.** Report rows and the scale test output.

## VS-08 Fuse keyword and vector results with reciprocal rank fusion
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W02-Measure
- estimate: 4h
- depends: VS-07

**Why.** Raw vector and BM25 scores are on different scales; adding them rewards whichever scale is larger. Rank fusion is robust and needs no tuning to start.

**Today.** `context_runtime/fusion.rs` and `search/hybrid.rs` each fuse differently.

**Do.**
1. One pure function `fuse_rrf(lists: &[Vec<(id, score)>], k: f32) -> Vec<(id, fused)>` with unit tests (ties, one empty list, a doc in only one list).
2. Use it in the single retrieval path (VS-09).
3. Try k = 30, 60, 90 on both personas; pick by MRR@10 and record why.

**Done when.** Unit tests pass; the chosen k and its numbers are in the MR.

**Evidence.** The k comparison table.

## VS-09 Define one retrieval function every surface calls
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W02-Measure
- estimate: 5h
- depends: VS-08

**Why.** The Search screen and Ask/agents use different stacks today, so the same question can rank different memories.

**Today.** Search: `ipc/commands/search.rs` (`HybridSearcher` plus reranker). Ask and MCP: `context_runtime::run_query` (`context_runtime/mod.rs:3059`). MCP `search_memories` uses `HybridSearcher::search` (`mcp/mod.rs:2394`).

**Do.**
1. Add `context_runtime::retrieve(state, &RetrieveRequest { query, time, app, limit }) -> RetrieveResult { hits: Vec<Hit { memory_id, chunk_id, score, why }> }` built on the fused routes.
2. `why` records which routes found the hit (keyword terms matched, vector, time, entity) for the UI.
3. Unit tests for the request and response shapes; `retrieval_qa` gains a third path column for `retrieve`.

**Done when.** `retrieve` exists, is tested, and its report row is at least as good as the better of the two current paths.

**Evidence.** Report with three paths.

## VS-10 Point the Search screen at the one retrieval function
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W03-Build
- estimate: 3h
- depends: VS-09

**Do.**
1. `search_memory_cards` calls `retrieve`, then card synthesis as today.
2. Keep `search_ranked_results` as a thin wrapper so the eval keeps measuring what users see.
3. Browser preview check of the Search flow (`make qa-preview`).

**Done when.** The report shows Search and Ask agreeing on the top result for every query.

**Evidence.** Agreement line from the report.

## VS-11 Point Ask, Resume, and every MCP search tool at the one retrieval function
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W03-Build
- estimate: 3h
- depends: VS-09

**Do.**
1. `run_query` gathers evidence through `retrieve`.
2. MCP `search_memories`, `memory.search_full_context`, and `fndr.search` call `retrieve` (coordinate names with the MCP cleanup ticket in `product-decisions.md`).
3. Contract test: the same query through IPC and MCP returns the same ordered ids.

**Done when.** The contract test passes.

**Evidence.** Test output.

## VS-12 Define and show "no good match"
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p1
- milestone: W03-Build
- estimate: 3h
- depends: VS-10

**Why.** Showing ten weak results for a query that matches nothing teaches people not to trust search.

**Do.**
1. From the report, pick a fused-score threshold under which results are "weak."
2. Search shows "No strong matches" with the weak results collapsed below.
3. Negative queries from VS-03 count as correct when nothing crosses the threshold.

**Done when.** Negative rows pass on both personas; no positive query drops out.

**Evidence.** Report and a screenshot.

## VS-13 Parse time and app phrases into filters
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p1
- milestone: W03-Build
- estimate: 4h
- depends: VS-09

**Today.** `context_runtime/query_plan.rs` and `temporal_route.rs` handle some time intent; the Search screen only filters through its dropdowns.

**Do.**
1. Table-driven parser for: today, yesterday, this morning, last night, last week, weekday names, "on Sep 30," and "in <known app>" using `get_app_names`.
2. Strip the phrase from the text query and pass filters to `retrieve`.
3. Tests for each phrase and for no false match ("Monday.com" is not a weekday).

**Done when.** Time and app rows in the report reach Recall@5 of 0.9.

**Evidence.** Report rows.

## VS-14 Find out why most real memories take the visual path with little text
- assignee: anurupkumar
- labels: area::vault-search, type::spike, prio::p0
- milestone: W02-Measure
- estimate: 4h
- depends: VS-01

**Why.** 24 of the owner's 29 memories are `screen_visual` with a median of 129 characters. Retrieval cannot find text that was never stored.

**Today.** Low-OCR visual branch in `capture/mod.rs` (around lines 272 and 483) and the `llm_ocr_grounded_visual_fallback` synthesis branch.

**Do.**
1. Run a 30-minute live session with `FNDR_METRICS_DUMP` on; log per frame the OCR character count, the branch taken, and the stored `clean_text` length (counts only).
2. Identify which gate sends text-heavy screens (docs, Slack, web pages) to the visual branch or truncates text.
3. Write the finding and a proposed fix in the MR; implement the fix only if it is under 20 lines, otherwise open a follow-up ticket.

**Done when.** A written answer with numbers: what share of frames lose text and at which step.

**Evidence.** The session summary.

## VS-15 Read on-screen text from the Accessibility tree first
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W03-Build
- estimate: 8h
- depends: VS-14

**Why.** Accessibility text is exact, costs less than OCR, and keeps paragraph order; OCR on documents and chats is noisy (CER about 0.10 to 0.36 on our fixtures).

**Today.** `accessibility/mod.rs` has AXUIElement bindings (`focused_window_snapshot` at line 552) used for titles, URLs, and Autofill.

**Do.**
1. `accessibility::focused_text(pid, max_chars)`: walk the focused window's tree (AXWebArea for browsers, AXTextArea and AXStaticText elsewhere), in reading order, with a node and time budget (stop at 50 ms or 4,000 nodes).
2. Fixture-free unit tests for ordering and budget logic; a manual matrix for Chrome, Safari, Arc, Google Docs, Word, Pages, Keynote, Preview, Slack, Notion, Mail, VS Code (record chars and quality per app).
3. Secure text fields and password managers are never read; privacy gates still run before any read.

**Done when.** The matrix shows usable text for at least 9 of the 12 apps.

**Evidence.** The per-app matrix (counts and pass or fail only).

## VS-16 Use Accessibility text in capture, with OCR as the fallback
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W03-Build
- estimate: 6h
- depends: VS-15

**Do.**
1. In the capture loop, prefer `focused_text` when it returns at least 200 characters; otherwise OCR as today.
2. Record `text_source = ax | ocr | both` on the memory; count each in runtime metrics.
3. Store the full cleaned text (bounded, for example 20,000 characters) for chunking; keep the short summary for display.
4. Run a two-hour live session and `make vault-health`.

**Done when.** Median stored text is 800 characters or more on a live day, and capture CPU does not rise (compare `capture.ocr_ms` and CPU in the metrics dump).

**Evidence.** Vault health before and after, and the metrics comparison.

## VS-17 Choose the embedding model by measurement
- assignee: anurupkumar
- labels: area::vault-search, type::spike, prio::p1
- milestone: W03-Build
- estimate: 8h
- depends: VS-04, VS-16

**Why.** MiniLM-L6 (2019, 384 dimensions) is the weakest embedder in common use; BGE-large is downloaded but unused. We should pick once, by data, for an 8 GB Mac.

**Do.**
1. Candidates: all-MiniLM-L6-v2 (today), bge-small-en-v1.5, EmbeddingGemma-300M, Qwen3-Embedding-0.6B (ONNX builds; check license and pin checksums as `model_config.rs` does for MiniLM).
2. For each: re-embed both seeded personas in a scratch profile, run `qa-retrieval`, and record Recall@5, MRR@10, milliseconds per chunk on the M1, and peak memory.
3. Write `docs/decisions/019-embedding-model.md` with the table and the choice. Minh's embedding tickets implement the switch.

**Done when.** The ADR is merged with numbers for all four.

**Evidence.** The ADR table.

## VS-18 Retrieve over chunks and roll hits up to their memory
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p0
- milestone: W03-Build
- estimate: 6h
- depends: VS-09, EM-03

**Why.** Matching one short summary per memory misses the sentence you actually remember; chunks carry the real text.

**Today.** `context_runtime/chunk_route.rs` exists but the chunk table is empty on real profiles (Minh's EM-03 fills it at capture).

**Do.**
1. `retrieve` queries chunk vectors and chunk BM25, then groups hits by memory (best chunk score, plus a small bonus for several matching chunks).
2. `Hit.chunk_id` and a matched snippet let the UI show the sentence that matched.
3. Report: add chunk on and off as two rows.

**Done when.** Chunk retrieval improves paraphrase Recall@5 on both personas, or the MR explains why not with numbers.

**Evidence.** Report rows.

## VS-19 Try a small cross-encoder reranker on the top 30
- assignee: anurupkumar
- labels: area::vault-search, type::spike, prio::p2
- milestone: W04-Prove
- estimate: 5h
- depends: VS-18

**Do.**
1. Load `cross-encoder/ms-marco-MiniLM-L-6-v2` (ONNX) through the existing `ort` runtime; score (query, chunk) pairs for the top 30.
2. Keep it only if MRR@10 improves by 0.03 or more and adds under 150 ms p95.

**Done when.** Kept or rejected with numbers in the MR.

**Evidence.** Comparison rows.

## VS-20 Keep search fast at 10,000 memories
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p1
- milestone: W04-Prove
- estimate: 4h
- depends: VS-18

**Today.** `src-tauri/tests/storage_scale.rs` and the MEM-08 index work in `docs/evidence/W04/storage-indexes.md`.

**Do.**
1. Extend the scale test to 10,000 memories and 60,000 chunks with synthetic text.
2. Measure `retrieve` p50 and p95; add a vector index only if flat search exceeds the budget.

**Done when.** p95 is 500 ms or lower on the M1.

**Evidence.** Scale test output.

## VS-21 Same query, same results, every time
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p1
- milestone: W03-Build
- estimate: 2h
- depends: VS-10

**Why.** Results that shuffle between identical queries feel broken even when they are relevant.

**Do.**
1. Deterministic tie-breaking (score, then newest timestamp, then id) in fusion and in card grouping.
2. Test: run the same query five times against the seeded profile; the ordered ids must be identical.

**Done when.** The test passes.

**Evidence.** Test output.

## VS-22 Show why each result matched
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 4h
- depends: VS-18

**Why.** "Matched the words 'labor contracts'" or "similar meaning" plus the matched sentence is how people learn to trust semantic search.

**Do.**
1. Carry `Hit.why` and the matched snippet into `MemoryCard`.
2. In the Search results and Ask citations, show the matched sentence with matched words highlighted, and a small "words" or "meaning" tag.
3. Component tests for both tags.

**Done when.** Seeded profile screenshots show the reason on every result.

**Evidence.** Screenshots in light and dark.

## VS-23 Make the Vault read like your work, not a log
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 5h
- depends: none

**Today.** `src/domains/memory-vault/MemoryCardsPanel.tsx` lists memories with app, time, and source filters and an embedded graph strip.

**Do.**
1. Group by day, then by thread (project or session), newest first; collapse near-duplicates with a "3 similar" count.
2. Each row: human title, one-line summary, source icon (page, document, download, agent note), and the reopen action from Minh's work.
3. Move the graph strip behind a "Connections" toggle.

**Done when.** On the seeded profile the Vault fits one screen per day and you can open any item in two clicks.

**Evidence.** Before and after screenshots.

## VS-24 Answer in Ask with cited sentences, not whole memories
- assignee: anurupkumar
- labels: area::vault-search, type::feature, prio::p1
- milestone: W04-Prove
- estimate: 5h
- depends: VS-18, PD-01

**Do.**
1. Build Ask's evidence from chunk hits; each citation points at a memory and a chunk.
2. The composer (`context_runtime/composer.rs`) keeps its rule of falling back to a partial answer when citations are invalid.
3. Answer set: 15 questions across both personas with expected answers; mark correct, partly, wrong, for the local model and, if PD-01 allows, the cloud tier.

**Done when.** The answer table is in `docs/evidence/`.

**Evidence.** The answer table.

## VS-25 Delete the retired search code
- assignee: anurupkumar
- labels: area::vault-search, type::chore, prio::p2
- milestone: W04-Prove
- estimate: 3h
- depends: VS-10, VS-11

**Do.**
1. Remove `HybridSearcher` paths and the coverage reranker that nothing calls after VS-10 and VS-11; keep behavior tests by porting them to `retrieve`.
2. `git grep` shows no callers; `make test` passes; explain the test count change in the MR.

**Done when.** Merged with the grep output.

**Evidence.** Grep and test counts.

## VS-26 Make the README and walkthrough describe search as it really works
- assignee: anurupkumar
- labels: area::vault-search, type::docs, prio::p1
- milestone: W04-Prove
- estimate: 2h
- depends: VS-18

**Do.**
1. Update the README feature table and architecture diagram (one retrieval path, BM25, chunks, chosen embedder; no claim about the graph route until it has data).
2. Update card 5 and card 6 in `docs/product/qa-walkthrough.md`.

**Done when.** Every claim in the README search rows maps to code.

**Evidence.** The diff.
