# Retrieval check: knowledge-worker: PASS

Gate: Recall@5 may drop at most 0.05 on any path, and no query found in a path's top ten may become a miss.

| Path | Recall@5 ref | Recall@5 now | Delta | MRR@10 ref | MRR@10 now | Delta | p95 ms ref | p95 ms now |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| search | 1.000 | 1.000 | +0.000 | 0.966 | 0.966 | +0.000 | 339 | 178 |
| ask | 1.000 | 1.000 | +0.000 | 0.966 | 0.966 | +0.000 | 917 | 579 |
| retrieve | 1.000 | 1.000 | +0.000 | 0.966 | 0.966 | +0.000 | 320 | 180 |

Top-1 agreement: 39/39 -> 39/39 (reported, not gated).

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
| search | 4 | 0 | 0.25 | 2 | 0 | 0.249 | 0.464 |
| ask | 4 | 0 | 0.25 | 2 | 0 | 0.249 | 0.464 |
| retrieve | 4 | 0 | 0.25 | 2 | 0 | 0.249 | 0.464 |

No per-query rank changed.
