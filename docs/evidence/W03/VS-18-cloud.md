# VS-18 retrieve over chunks and roll hits up to their memory (cloud)

Behind the existing flag `search.use_chunk_first_retrieval` (default off). Chunk rows still come only from the v5 reindex until EM-03 writes them at capture, so on real profiles the chunk route finds nothing and steps aside.

## What changed

- **Chunk BM25.** `Store::chunk_keyword_search` searches a full-text index on chunk text, built on first use and folded with the memory indexes by `optimize_fts_indexes`. Scores are bm25/(bm25+2), as for memories.
- **The chunk route runs both searches.** It takes BM25 over chunk text and the existing BGE chunk vectors, and keeps each chunk's better score. BM25 needs no model, so the words a person remembers still find their chunk when the BGE model is missing.
- **Roll-up to the memory.** A memory scores its best chunk, plus 0.01 for each other chunk within 0.05 of it, at most 3 (the VS-17 bake-off's rule).
- **`retrieve` names the chunk.** `RetrieveHit.chunk_id` and a new `matched_text` carry the chunk the chunk route matched. Search rows carry the same chunk in `matched_chunk_ids` and `chunk_evidence`, so cards can show the sentence.
- **Report rows for chunks on and off.** `retrieval_qa --chunks` (`make qa-retrieval-check QA_CHUNKS=1`) runs the existing v5 reindex on the evaluation copy, then measures with the chunk route on. The report states which mode it ran in, and the gate's check says when the run's mode differs from the reference. `reindex_memories_v5_for_state` became public for this.
- **The no-match bar follows the route.** With chunks on, every score rises (the chunk route's weight is 0.40), and VS-12's bar of 0.25 stopped catching anything. A hit the chunk route found uses `STRONG_MATCH_SCORE_WITH_CHUNKS` = 0.45 instead. On these sets that catches the same no-match queries, and no real query falls under it. It is provisional until a real vault has chunks.

## Tests (each failed or did not compile first)

- `lance_store::tests::chunk_keyword_search_finds_the_chunk_that_holds_the_words`.
- `chunk_route::tests::several_matching_chunks_lift_their_memory`: three close chunks at 0.91, 0.89, and 0.88 beat a single 0.92 chunk. It failed on the old best-chunk-only roll-up.
- `chunk_route::tests::chunk_route_finds_words_even_without_the_chunk_model`: it returned nothing on the old code when BGE was missing.
- `retrieve::tests::the_chunk_route_raises_the_strong_match_bar`.
- `tests/retrieve.rs`:
  - `retrieve_names_the_chunk_that_matched`: with v5 parents and chunks written, `retrieve` returns `chunk_id` "vendor-1" and its text, hits without the chunk route carry none, and Search rows carry the chunk.
  - The JSON shape test now includes `matched_text`.
- Report and gate: the report tests cover the chunk-route block and its markdown line (16 passed); the comparator tests cover the mode line and the Bar column (26 OK).
- Results:
  - `cargo test --lib`: 910 passed.
  - `--test retrieve`: 13 passed.
  - All other integration targets pass.

## Report rows: chunk route off and on, same seed

Both personas were seeded once under America/Denver time. The gate ran once normally and once with `QA_CHUNKS=1` and the BGE model (`BAAI/bge-large-en-v1.5`, quantized, from the pinned URL in `scripts/bootstrap/download-embedding-model.sh`, kept in a separate folder). All four runs exit 0. Each synthetic memory is short, so each became exactly one chunk: 20 chunks and 40 chunks.

| Persona | Path | Recall@5 off / on | MRR@10 off / on | Paraphrase Recall@5 off / on | p50 ms off / on | p95 ms off / on |
|---|---|---|---|---|---|---|
| knowledge-worker | search | 1.000 / 1.000 | 0.966 / 0.970 | 1.000 / 1.000 | 231 / 391 | 360 / 530 |
| knowledge-worker | ask | 1.000 / 1.000 | 0.966 / 0.970 | 1.000 / 1.000 | 904 / 1022 | 958 / 1129 |
| knowledge-worker | retrieve | 1.000 / 1.000 | 0.966 / 0.970 | 1.000 / 1.000 | 200 / 326 | 331 / 482 |
| office-pm | search | 0.900 / 0.950 | 0.661 / 0.699 | 0.778 / 0.889 | 233 / 396 | 403 / 529 |
| office-pm | ask | 0.900 / 0.950 | 0.661 / 0.699 | 0.778 / 0.889 | 1333 / 1493 | 1443 / 1570 |
| office-pm | retrieve | 0.900 / 0.950 | 0.661 / 0.699 | 0.778 / 0.889 | 200 / 331 | 335 / 458 |

- **Done when ("improves paraphrase Recall@5 on both personas, or explain why not with numbers").**
  - Office-PM paraphrase Recall@5 rises from 0.778 to 0.889.
  - Knowledge-worker is already at 1.000, so it cannot rise; its MRR@10 rises 0.966 to 0.970 ("which plotting library am I allowed to use", 4 to 3).
- **Rank changes.** Office-PM ranks improve on four queries:
  - "how much money are we losing to customers leaving", 5 to 4;
  - "why were our payment nudges ending up in junk mail", 6 to 5;
  - "LL-1482 spam placement 1.8%", 2 to 1;
  - "launch checklist with the go/no-go meeting checked off", 3 to 2.

  No query got worse on any path. Time and app recall stay 1.000.
- **No-match.**
  - Chunks off: 2 of 4 and 1 of 4 negatives fall under the 0.25 bar, as in VS-12.
  - Chunks on: the same 2 and 1 fall under the 0.45 bar.
  - No positive falls under the bar in either mode.
- **Caveat: what the gain measures.** Because each memory is one chunk, these rows mostly measure adding BGE-large similarity and chunk-text BM25 as a fourth signal, not long-document chunking. Real captures are longer, and the chunk table is empty until EM-03, so the real test is a live profile after EM-03.
- **Cost.** Turning chunks on adds about 120 to 165 ms at p50: the BGE-large query embedding runs on CPU in a debug build. Search p95 goes from 360 to 530 ms and from 403 to 529 ms. VS-20 measures this at 10,000 memories and 60,000 chunks on the M1.
- **Not separated here.** The gain is not split between chunk BM25 and chunk vectors: the report indexes and searches in one process, with the model loaded.

References are unchanged: the default run (chunks off) reproduces them.
