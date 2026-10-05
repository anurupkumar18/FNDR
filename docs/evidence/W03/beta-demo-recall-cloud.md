# Beta demo, 4:10 beat: Recall@5 before and after (cloud)

Chart data: `docs/evidence/W03/beta-demo-recall.csv` (one row per persona, path, and stage).

## Where the numbers come from

- **Before:** `docs/evidence/W03/retrieval-baseline-knowledge-worker.json` and `retrieval-baseline-office-pm.json`, recorded at aed1315 (VS-03, 2026-10-04). They use today's query sets and harness, before any retrieval change in train B.
- **After:** `scripts/demo/retrieval-reference/knowledge-worker.json` and `office-pm.json`, ratcheted at 7f4e673 (VS-25). The gate reproduced them on main (342e5a2) on 2026-10-05, with every per-query rank identical (`VS-33-cloud.md`).
- Both are synthetic personas seeded from `scripts/demo/`. No owner data.
- **Headline Recall@5 counts keyword and paraphrase queries:** 22 for knowledge-worker, 20 for office-PM. Time and app queries are reported by kind; no-match queries are reported separately.

## The numbers

| Persona | Path | Recall@5 before / after | MRR@10 before / after | Paraphrase Recall@5 before / after | p95 ms before / after |
|---|---|---|---|---|---|
| knowledge-worker | Search | 0.955 / 1.000 (21 / 22 to 22 / 22) | 0.909 / 0.966 | 0.875 / 1.000 | 1512 / 339 |
| knowledge-worker | Ask | 1.000 / 1.000 | 1.000 / 0.966 | 1.000 / 1.000 | 2920 / 917 |
| office-pm | Search | 0.700 / 0.900 (14 / 20 to 18 / 20) | 0.589 / 0.661 | 0.333 / 0.778 | 913 / 372 |
| office-pm | Ask | 0.900 / 0.900 | 0.657 / 0.661 | 0.778 / 0.778 | 3197 / 1390 |

- **Search and Ask now agree on the top result for every query:** 39 / 39 and 37 / 37, against 28 / 39 and 12 / 37 before. They run one `retrieve` since VS-10 and VS-11.
- **Time queries:** office-PM time Recall@5 went 0.875 to 1.000 on both paths (VS-13 time filters).
- **Latency:** p95 fell on every path, mostly from VS-07's BM25 replacing the keyword scan and VS-10's shared model. These are Linux debug-build numbers; the M1 numbers will differ.

## Say on stage only what this supports

- "On our synthetic office-PM week, Search now finds the right memory in the top five for 18 of 20 questions, up from 14, and Search and Ask always agree on the top result."
- **Do not say Ask improved on recall.** Ask's Recall@5 is unchanged. Its knowledge-worker MRR fell 1.000 to 0.966: "which plotting library am I allowed to use" went from rank 1 to 4 once Ask moved onto the shared ranking. Three office-PM paraphrases also ranked lower in Ask, for example "how much money are we losing to customers leaving" (1 to 5). The ablation (`retrieval-ablation-cloud.md`) traces these to the keyword route.
- **Chunks** (VS-18, flag off by default) raise office-PM Recall@5 to 0.950 and paraphrase to 0.889 on this set. That is not the shipped default, and each synthetic memory is one chunk, so it is not a claim about long documents.
- **The sets are small:** 76 labeled queries over 60 synthetic memories. A third persona and a real-vault spot check (VS-01) are the honest next steps before calling this general.

## Suggested charts

1. Recall@5, before and after, grouped by persona and path (four pairs of bars, from the CSV's `recall_at_5`).
2. Office-PM Recall@5 by kind, before and after (paraphrase is the visible change: 0.333 to 0.778 in Search).
