# Retrieval baseline: office-pm

Schema v1; 20 queries (11 keyword, 9 paraphrase).
Search is the pre-card-synthesis ranked retrieval path used by Search; Ask is the context_runtime card path.
Recall@5 is case-level: a case is recalled when at least one accepted relevant ID appears in its top five results.
Route time budgets are raised to their configured maximums so ranks do not depend on machine load; latency is still measured.

| Path | Recall@5 | MRR@10 | Keyword Recall@5 | Paraphrase Recall@5 | p50 ms | p95 ms |
|---|---:|---:|---:|---:|---:|---:|
| Search | 0.700 | 0.589 | 1.000 | 0.333 | 772 | 911 |
| Ask | 0.900 | 0.657 | 1.000 | 0.778 | 3045 | 3160 |

Top-1 agreement: 3/20 (0.150).

| Query | Kind | Search rank@10 | Ask rank@10 |
|---|---|---:|---:|
| how much money are we losing to customers leaving | paraphrase | miss | 1 |
| why were our payment nudges ending up in junk mail | paraphrase | 4 | miss |
| what pay range did leadership sign off on for the design hire | paraphrase | 5 | 7 |
| which applicant came out on top for the design role | paraphrase | miss | 3 |
| what share of new accounts got up and running quickly last quarter | paraphrase | 3 | 2 |
| would clients recommend us, and did that improve | paraphrase | miss | 3 |
| the applicant's past project about getting tradespeople paid on time | paraphrase | miss | 3 |
| is there a way to stop the nudges for just one bill | paraphrase | miss | 2 |
| who is filling in during my end-of-year vacation | paraphrase | miss | 2 |
| SMB churn 3.9% by segment | keyword | 1 | 1 |
| LL-1482 spam placement 1.8% | keyword | 1 | 2 |
| Rafael O. staging retest 2.1% | keyword | 1 | 1 |
| Tobias W. interview feedback | keyword | 1 | 1 |
| net new ARR $1.24M | keyword | 1 | 1 |
| REQ-2207 pipeline stages | keyword | 1 | 1 |
| Smart Reminders launch checklist | keyword | 1 | 1 |
| launch checklist with the go/no-go meeting checked off | keyword | 1 | 2 |
| Hannah K. customer announcement email | keyword | 1 | 1 |
| Owen P. churn excluding trial cancels | keyword | 1 | 1 |
| Victor S. Q3 metrics summary email | keyword | 1 | 2 |
