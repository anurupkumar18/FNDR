# Retrieval baseline: office-pm

Schema v2; 37 queries (11 keyword, 9 paraphrase, 8 time, 5 app, 4 negative).
Search is the pre-card-synthesis ranked retrieval path used by Search; Ask is the context_runtime card path.
Recall@5 is case-level: a case is recalled when at least one accepted relevant ID appears in its top five results.
Recall@5 and MRR@10 cover keyword and paraphrase cases; other kinds have their own columns, and negative cases are scored only in the no-match table.
Route time budgets are raised to their configured maximums so ranks do not depend on machine load; latency is still measured.

| Path | Recall@5 | MRR@10 | Keyword Recall@5 | Paraphrase Recall@5 | Time Recall@5 | App Recall@5 | p50 ms | p95 ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Search | 0.700 | 0.589 | 1.000 | 0.333 | 0.875 | 1.000 | 724 | 913 |
| Ask | 0.900 | 0.657 | 1.000 | 0.778 | 0.875 | 1.000 | 3048 | 3197 |

| Path | Negative cases | Returned nothing | Median top score, negative | Median top score, positive |
|---|---:|---:|---:|---:|
| Search | 4 | 2 | 0.522 | 0.880 |
| Ask | 4 | 0 | 0.233 | 0.474 |

Top-1 agreement: 12/37 (0.324).

| Query | Kind | Search rank@10 | Ask rank@10 | Search top score | Ask top score |
|---|---|---:|---:|---:|---:|
| how much money are we losing to customers leaving | paraphrase | miss | 1 | none | 0.272 |
| why were our payment nudges ending up in junk mail | paraphrase | 4 | miss | 0.601 | 0.336 |
| what pay range did leadership sign off on for the design hire | paraphrase | 5 | 7 | 0.788 | 0.460 |
| which applicant came out on top for the design role | paraphrase | miss | 3 | 0.782 | 0.452 |
| what share of new accounts got up and running quickly last quarter | paraphrase | 3 | 2 | 0.502 | 0.293 |
| would clients recommend us, and did that improve | paraphrase | miss | 3 | none | 0.292 |
| the applicant's past project about getting tradespeople paid on time | paraphrase | miss | 3 | none | 0.305 |
| is there a way to stop the nudges for just one bill | paraphrase | miss | 2 | none | 0.325 |
| who is filling in during my end-of-year vacation | paraphrase | miss | 2 | 0.703 | 0.265 |
| SMB churn 3.9% by segment | keyword | 1 | 1 | 0.940 | 0.617 |
| LL-1482 spam placement 1.8% | keyword | 1 | 2 | 0.940 | 0.505 |
| Rafael O. staging retest 2.1% | keyword | 1 | 1 | 0.925 | 0.388 |
| Tobias W. interview feedback | keyword | 1 | 1 | 0.850 | 0.541 |
| net new ARR $1.24M | keyword | 1 | 1 | 1.000 | 0.493 |
| REQ-2207 pipeline stages | keyword | 1 | 1 | 0.880 | 0.488 |
| Smart Reminders launch checklist | keyword | 1 | 1 | 1.000 | 0.590 |
| launch checklist with the go/no-go meeting checked off | keyword | 1 | 2 | 0.914 | 0.491 |
| Hannah K. customer announcement email | keyword | 1 | 1 | 0.940 | 0.592 |
| Owen P. churn excluding trial cancels | keyword | 1 | 1 | 0.950 | 0.554 |
| Victor S. Q3 metrics summary email | keyword | 1 | 2 | 0.950 | 0.619 |
| the hiring debrief from yesterday | time | 1 | 1 | 0.850 | 0.588 |
| the Q3 numbers email I sent this morning | time | 1 | 1 | 0.900 | 0.407 |
| the launch checklist I updated yesterday | time | 1 | 1 | 0.880 | 0.582 |
| the launch review deck from two days ago | time | 1 | 1 | 0.871 | 0.436 |
| the product requirements doc I drafted last week | time | 2 | 10 | 0.786 | 0.435 |
| what Rafael said about the spam fix earlier today | time | 1 | 1 | 0.812 | 0.509 |
| the LL-1482 thread in Slack | app | 1 | 1 | 0.940 | 0.474 |
| the candidate's portfolio case study in the PDF | app | 1 | 1 | 0.900 | 0.455 |
| Q3 churn numbers in Sheets | app | 2 | 1 | 0.880 | 0.601 |
| the offsite venue vote on Zoom | app | 1 | 1 | 0.940 | 0.496 |
| the launch checklist from three days ago | time | 3 | 1 | 0.850 | 0.481 |
| the LL-1482 spam discussion from four days ago | time | 7 | 3 | 0.850 | 0.394 |
| the churn breakdown in Excel | app | 1 | 1 | 0.775 | 0.463 |
| rental car reservation | negative | miss | miss | none | 0.288 |
| SOC 2 audit evidence request | negative | miss | miss | 0.516 | 0.237 |
| Android app crash report | negative | miss | miss | 0.528 | 0.230 |
| GDPR data deletion request from a European customer | negative | miss | miss | none | 0.222 |
