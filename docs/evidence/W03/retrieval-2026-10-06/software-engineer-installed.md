# Retrieval check: software-engineer: PASS

Gate: Recall@5 may drop at most 0.05 on any path, and no query found in a path's top ten may become a miss.

| Path | Recall@5 ref | Recall@5 now | Delta | MRR@10 ref | MRR@10 now | Delta | p95 ms ref | p95 ms now |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| search | 1.000 | 1.000 | +0.000 | 0.831 | 0.879 | +0.048 | 369 | 181 |
| ask | 1.000 | 1.000 | +0.000 | 0.831 | 0.879 | +0.048 | 1214 | 621 |
| retrieve | 1.000 | 1.000 | +0.000 | 0.831 | 0.879 | +0.048 | 315 | 172 |

Top-1 agreement: 34/34 -> 34/34 (reported, not gated).

| Path | Kind | Recall@5 ref | Recall@5 now | Delta |
|---|---|---:|---:|---:|
| search | app | 1.000 | 1.000 | +0.000 |
| search | keyword | 1.000 | 1.000 | +0.000 |
| search | paraphrase | 1.000 | 1.000 | +0.000 |
| search | time | 1.000 | 1.000 | +0.000 |
| ask | app | 1.000 | 1.000 | +0.000 |
| ask | keyword | 1.000 | 1.000 | +0.000 |
| ask | paraphrase | 1.000 | 1.000 | +0.000 |
| ask | time | 1.000 | 1.000 | +0.000 |
| retrieve | app | 1.000 | 1.000 | +0.000 |
| retrieve | keyword | 1.000 | 1.000 | +0.000 |
| retrieve | paraphrase | 1.000 | 1.000 | +0.000 |
| retrieve | time | 1.000 | 1.000 | +0.000 |

No-match queries (reported, not gated). A negative is right when its best result is under the strong-match bar (VS-12); a positive under the bar would wrongly say "No strong matches".

| Path | Negative cases | Returned nothing | Bar | Negatives under the bar | Positives under the bar | Median top score, negative | Median top score, positive |
|---|---:|---:|---:|---:|---:|---:|---:|
| search | 4 | 0 | 0.25 | 3 | 0 | 0.146 | 0.436 |
| ask | 4 | 0 | 0.25 | 3 | 0 | 0.146 | 0.436 |
| retrieve | 4 | 0 | 0.25 | 3 | 0 | 0.146 | 0.436 |

## Rank changes

| Query | Kind | Path | Ref rank@10 | Now rank@10 |
|---|---|---|---:|---:|
| how many calls each partner is allowed to make | paraphrase | search | 2 | 1 |
| how many calls each partner is allowed to make | paraphrase | ask | 2 | 1 |
| how many calls each partner is allowed to make | paraphrase | retrieve | 2 | 1 |
| the fix for dropped database connections | paraphrase | search | 2 | 1 |
| the fix for dropped database connections | paraphrase | ask | 2 | 1 |
| the fix for dropped database connections | paraphrase | retrieve | 2 | 1 |
