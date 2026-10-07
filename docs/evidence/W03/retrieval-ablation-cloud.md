# Retrieval ablation: keyword only, vector only, fused, and chunks (cloud)

Part 9 stretch item. Four configurations of the same code, on one seed per persona.

## Setup

- Code: train F at 98045d1 (main plus VS-33, VS-36, VS-34, and the VS-13 deadline follow-up).
- Both personas were seeded once under America/Denver time on 2026-10-05. Every configuration ran `make qa-retrieval-check QA_SKIP_SEED=1` on that seed.
- Configurations differ only in the planner's base routes (`route_selection` in `src-tauri/src/context_runtime/query_plan.rs`, edited for the run and restored) or the chunk flag:
  - **fused:** vector and keyword (the shipped default; the chunk route is planned but empty without the flag);
  - **no keyword:** vector only;
  - **no vector:** keyword (BM25) only;
  - **chunks:** fused plus the chunk route with BGE-large and chunk BM25 (`QA_CHUNKS=1`, VS-18).
- In every configuration the temporal and entity routes still run when the planner asks for them (the entity route finds nothing; see VS-33).
- Search, Ask, and `retrieve` gave the same Recall@5 in every configuration, so the table shows `retrieve`.

## Results

| Persona | Config | Recall@5 | MRR@10 | Keyword | Paraphrase | Time | App | Negatives under the bar | Positives under the bar | p50 ms | p95 ms |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| knowledge-worker | fused | 1.000 | 0.966 | 1.000 | 1.000 | 1.000 | 1.000 | 2 / 4 | 0 | 201 | 336 |
| knowledge-worker | chunks | 1.000 | 0.970 | 1.000 | 1.000 | 1.000 | 1.000 | 2 / 4 | 0 | 327 | 471 |
| knowledge-worker | no keyword | 1.000 | 0.947 | 1.000 | 1.000 | 1.000 | 1.000 | 4 / 4 | 14 | 129 | 252 |
| knowledge-worker | no vector | 0.955 | 0.827 | 1.000 | 0.875 | 1.000 | 1.000 | 4 / 4 | 32 | 167 | 315 |
| office-pm | fused | 0.900 | 0.661 | 1.000 | 0.778 | 1.000 | 1.000 | 1 / 4 | 0 | 201 | 348 |
| office-pm | chunks | 0.950 | 0.699 | 1.000 | 0.889 | 1.000 | 1.000 | 1 / 4 | 0 | 347 | 496 |
| office-pm | no keyword | 0.950 | 0.648 | 1.000 | 0.889 | 0.875 | 1.000 | 4 / 4 | 18 | 136 | 229 |
| office-pm | no vector | 0.750 | 0.604 | 1.000 | 0.444 | 1.000 | 1.000 | 4 / 4 | 27 | 153 | 329 |
| software-engineer | fused | 1.000 | 0.831 | 1.000 | 1.000 | 1.000 | 1.000 | 3 / 4 | 0 | 183 | 306 |
| software-engineer | no keyword | 0.952 | 0.814 | 1.000 | 0.889 | 1.000 | 1.000 | 4 / 4 | 19 | 122 | 223 |
| software-engineer | no vector | 0.905 | 0.756 | 1.000 | 0.778 | 1.000 | 1.000 | 4 / 4 | 25 | 169 | 294 |

The software-engineer rows come from the third persona (`persona-software-engineer-cloud.md`). Its queries were written before any run. It ran after the other two, on its own seed, and had no chunks run.

## What it shows

1. **The vector route carries paraphrase.** Without it, paraphrase Recall@5 falls to 0.875 and 0.444, and five office-PM paraphrases leave the top ten. This is the change VS-34's negative control used.
2. **BM25 helps exact words and hurts some paraphrases, and on balance helps.**
   - *Without it,* exact-identifier, time, and app queries rank lower:
     - "Rafael O. staging retest 2.1%" 1 to 3;
     - "Field Order 15" 1 to 3;
     - "the product requirements doc I drafted last week" 2 to 8;
     - "the LL-1482 thread in Slack" 1 to 3.
   - *But three office-PM paraphrases rank higher:*
     - "how much money are we losing to customers leaving" 5 to 1;
     - "why were our payment nudges ending up in junk mail" 6 to 2;
     - "would clients recommend us, and did that improve" 9 to 3.
   - *Net:* MRR@10 falls (0.966 to 0.947, 0.661 to 0.648) while office-PM Recall@5 rises (0.900 to 0.950). Eleven queries move for the worse and four for the better.
   - These three paraphrases are the ones that moved down in Ask between the VS-03 baseline and today (`beta-demo-recall-cloud.md`). The cause is literal-word matches from BM25 outranking the right memory in fusion.
3. **The no-match bar is calibrated to the fused mix.** Without either route, 14 to 32 real queries fall under VS-12's 0.25 bar, because fused scores drop. Any change to routes or weights must recheck the bar (the report already counts "positives under the bar").
4. **Chunks are the only configuration that gains without a trade.** Office-PM Recall@5 rises to 0.950 and MRR@10 to 0.699; knowledge-worker MRR@10 rises to 0.970; no query ranks lower. The cost is about 130 ms at p50 for the BGE-large query embedding (CPU, debug build). These are VS-18's numbers again, from an independent seed.
5. **The keyword route costs about 70 ms at p50** in this build (201 against 129 ms).

## Out of sample: the third persona does not repeat finding 2

On the software-engineer persona, removing BM25 makes nothing better and four queries worse:

- two paraphrases: "the fix for dropped database connections" 2 to 5, and "what broke in the build for the labels change" 5 to 7;
- one time query and one app query, each 1 to 2.

Its Recall@5 falls 1.000 to 0.952. Here the paraphrases share words with their memories ("database", "build", "labels"), so BM25 helps them.

So "BM25 hurts paraphrases" is a property of three office-PM queries, not of paraphrase queries. Removing the vector route still hurts paraphrase on all three personas (finding 1 holds out of sample).

## What it suggests

- **Keep the fused default.** It has the best MRR@10 on all three personas.
- **Do not lower the keyword weight for paraphrase-like queries.** I drafted that as a proposal from the two original personas. The third persona, run afterwards, contradicts it, so it was withdrawn before filing. This is the overfitting a third persona exists to catch.
- **Chunks remain the measured way to lift paraphrase** (finding 4).

## Caveats

- **Small synthetic corpora** (20 and 40 memories) favor the vector route: each memory is about something different. A real vault has many near-duplicates, where exact words matter more. Do not read "no keyword is fine" into this.
- **One seed and one run per configuration.** Ranks have been stable across seeds since VS-21; latency is single-run, debug build, Linux.
- The chunk configuration has one chunk per memory (short synthetic memories), as in VS-18.
