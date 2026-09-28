# Retrieval baseline: knowledge-worker

Schema v1; 22 queries (14 keyword, 8 paraphrase).
Search is the pre-card-synthesis ranked retrieval path used by Search; Ask is the context_runtime card path.
Recall@5 is case-level: a case is recalled when at least one accepted relevant ID appears in its top five results.

| Path | Recall@5 | MRR@10 | Keyword Recall@5 | Paraphrase Recall@5 | p50 ms | p95 ms |
|---|---:|---:|---:|---:|---:|---:|
| Search | 0.955 | 0.909 | 1.000 | 0.875 | 595 | 764 |
| Ask | 1.000 | 1.000 | 1.000 | 1.000 | 1234 | 1418 |

Top-1 agreement: 17/22 (0.773).

| Query | Kind | Search rank@10 | Ask rank@10 |
|---|---|---:|---:|
| pandas import error fix | keyword | 1 | 1 |
| how did I get python to find the data library last time | paraphrase | 2 | 1 |
| what does Dana want for the survey readout | keyword | 1 | 1 |
| churn drivers due Thursday | keyword | 1 | 1 |
| what customers complained about during onboarding | paraphrase | 1 | 1 |
| single sign-on | keyword | 1 | 1 |
| essay rubric weights | keyword | 1 | 1 |
| how much of my history grade depends on sources | paraphrase | 1 | 1 |
| Field Order 15 | keyword | 1 | 1 |
| labor contracts passage page 112 | keyword | 1 | 1 |
| counterargument paragraph | keyword | 1 | 1 |
| weekday vs weekend ridership chart | keyword | 1 | 1 |
| which plotting library am I allowed to use | paraphrase | miss | 1 |
| who is recording the backup video | keyword | 1 | 1 |
| architecture slide | keyword | 2 | 1 |
| how long is the capstone demo script | paraphrase | 1 | 1 |
| spring enrollment | keyword | 1 | 1 |
| duplicate survey responses removed | keyword | 1 | 1 |
| food bank inventory app presentation | paraphrase | 1 | 1 |
| transit dataset assignment requirements | keyword | 1 | 1 |
| my thesis about why Reconstruction failed | paraphrase | 1 | 1 |
| the problem our capstone app solves | paraphrase | 1 | 1 |
