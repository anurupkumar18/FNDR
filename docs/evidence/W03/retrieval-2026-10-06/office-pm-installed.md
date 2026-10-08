# Retrieval check: office-pm: FAIL

Gate: Recall@5 may drop at most 0.05 on any path, and no query found in a path's top ten may become a miss.

| Path | Recall@5 ref | Recall@5 now | Delta | MRR@10 ref | MRR@10 now | Delta | p95 ms ref | p95 ms now |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| search | 0.900 | 0.900 | +0.000 | 0.661 | 0.663 | +0.002 | 372 | 188 |
| ask | 0.900 | 0.900 | +0.000 | 0.661 | 0.663 | +0.002 | 1390 | 839 |
| retrieve | 0.900 | 0.900 | +0.000 | 0.661 | 0.663 | +0.002 | 334 | 181 |

Top-1 agreement: 37/37 -> 37/37 (reported, not gated).

| Path | Kind | Recall@5 ref | Recall@5 now | Delta |
|---|---|---:|---:|---:|
| search | app | 1.000 | 1.000 | +0.000 |
| search | keyword | 1.000 | 1.000 | +0.000 |
| search | paraphrase | 0.778 | 0.778 | +0.000 |
| search | time | 1.000 | 1.000 | +0.000 |
| ask | app | 1.000 | 1.000 | +0.000 |
| ask | keyword | 1.000 | 1.000 | +0.000 |
| ask | paraphrase | 0.778 | 0.778 | +0.000 |
| ask | time | 1.000 | 1.000 | +0.000 |
| retrieve | app | 1.000 | 1.000 | +0.000 |
| retrieve | keyword | 1.000 | 1.000 | +0.000 |
| retrieve | paraphrase | 0.778 | 0.778 | +0.000 |
| retrieve | time | 1.000 | 1.000 | +0.000 |

No-match queries (reported, not gated). A negative is right when its best result is under the strong-match bar (VS-12); a positive under the bar would wrongly say "No strong matches".

| Path | Negative cases | Returned nothing | Bar | Negatives under the bar | Positives under the bar | Median top score, negative | Median top score, positive |
|---|---:|---:|---:|---:|---:|---:|---:|
| search | 4 | 0 | 0.25 | 1 | 0 | 0.302 | 0.456 |
| ask | 4 | 0 | 0.25 | 1 | 0 | 0.302 | 0.456 |
| retrieve | 4 | 0 | 0.25 | 1 | 0 | 0.302 | 0.456 |

## Regressions

- search lost 'would clients recommend us, and did that improve' (paraphrase): rank 9 -> miss
- ask lost 'would clients recommend us, and did that improve' (paraphrase): rank 9 -> miss
- retrieve lost 'would clients recommend us, and did that improve' (paraphrase): rank 9 -> miss

## Rank changes

| Query | Kind | Path | Ref rank@10 | Now rank@10 |
|---|---|---|---:|---:|
| why were our payment nudges ending up in junk mail | paraphrase | search | 6 | 7 |
| why were our payment nudges ending up in junk mail | paraphrase | ask | 6 | 7 |
| why were our payment nudges ending up in junk mail | paraphrase | retrieve | 6 | 7 |
| what pay range did leadership sign off on for the design hire | paraphrase | search | 4 | 1 |
| what pay range did leadership sign off on for the design hire | paraphrase | ask | 4 | 1 |
| what pay range did leadership sign off on for the design hire | paraphrase | retrieve | 4 | 1 |
| which applicant came out on top for the design role | paraphrase | search | 3 | 4 |
| which applicant came out on top for the design role | paraphrase | ask | 3 | 4 |
| which applicant came out on top for the design role | paraphrase | retrieve | 3 | 4 |
| what share of new accounts got up and running quickly last quarter | paraphrase | search | 1 | 2 |
| what share of new accounts got up and running quickly last quarter | paraphrase | ask | 1 | 2 |
| what share of new accounts got up and running quickly last quarter | paraphrase | retrieve | 1 | 2 |
| would clients recommend us, and did that improve | paraphrase | search | 9 | miss |
| would clients recommend us, and did that improve | paraphrase | ask | 9 | miss |
| would clients recommend us, and did that improve | paraphrase | retrieve | 9 | miss |
| LL-1482 spam placement 1.8% | keyword | search | 2 | 1 |
| LL-1482 spam placement 1.8% | keyword | ask | 2 | 1 |
| LL-1482 spam placement 1.8% | keyword | retrieve | 2 | 1 |
| Rafael O. staging retest 2.1% | keyword | search | 1 | 2 |
| Rafael O. staging retest 2.1% | keyword | ask | 1 | 2 |
| Rafael O. staging retest 2.1% | keyword | retrieve | 1 | 2 |
| the product requirements doc I drafted last week | time | search | 2 | 4 |
| the product requirements doc I drafted last week | time | ask | 2 | 4 |
| the product requirements doc I drafted last week | time | retrieve | 2 | 4 |
