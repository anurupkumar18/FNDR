# Retrieval baseline: knowledge-worker

Schema v2; 39 queries (14 keyword, 8 paraphrase, 8 time, 5 app, 4 negative).
Search is the pre-card-synthesis ranked retrieval path used by Search; Ask is the context_runtime card path.
Recall@5 is case-level: a case is recalled when at least one accepted relevant ID appears in its top five results.
Recall@5 and MRR@10 cover keyword and paraphrase cases; other kinds have their own columns, and negative cases are scored only in the no-match table.
Route time budgets are raised to their configured maximums so ranks do not depend on machine load; latency is still measured.

| Path | Recall@5 | MRR@10 | Keyword Recall@5 | Paraphrase Recall@5 | Time Recall@5 | App Recall@5 | p50 ms | p95 ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Search | 0.955 | 0.909 | 1.000 | 0.875 | 1.000 | 1.000 | 1107 | 1512 |
| Ask | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 2477 | 2920 |

| Path | Negative cases | Returned nothing | Median top score, negative | Median top score, positive |
|---|---:|---:|---:|---:|
| Search | 4 | 3 | 0.649 | 0.850 |
| Ask | 4 | 0 | 0.245 | 0.472 |

Top-1 agreement: 28/39 (0.718).

| Query | Kind | Search rank@10 | Ask rank@10 | Search top score | Ask top score |
|---|---|---:|---:|---:|---:|
| pandas import error fix | keyword | 1 | 1 | 0.940 | 0.465 |
| how did I get python to find the data library last time | paraphrase | 2 | 1 | 0.533 | 0.401 |
| what does Dana want for the survey readout | keyword | 1 | 1 | 0.850 | 0.543 |
| churn drivers due Thursday | keyword | 1 | 1 | 0.880 | 0.488 |
| what customers complained about during onboarding | paraphrase | 1 | 1 | 0.658 | 0.320 |
| single sign-on | keyword | 1 | 1 | 1.000 | 0.359 |
| essay rubric weights | keyword | 1 | 1 | 0.850 | 0.626 |
| how much of my history grade depends on sources | paraphrase | 1 | 1 | 0.760 | 0.420 |
| Field Order 15 | keyword | 1 | 1 | 1.000 | 0.306 |
| labor contracts passage page 112 | keyword | 1 | 1 | 0.950 | 0.623 |
| counterargument paragraph | keyword | 1 | 1 | 1.000 | 0.417 |
| weekday vs weekend ridership chart | keyword | 1 | 1 | 0.950 | 0.717 |
| which plotting library am I allowed to use | paraphrase | miss | 1 | 0.511 | 0.312 |
| who is recording the backup video | keyword | 1 | 1 | 0.850 | 0.485 |
| architecture slide | keyword | 2 | 1 | 1.000 | 0.438 |
| how long is the capstone demo script | paraphrase | 1 | 1 | 0.850 | 0.538 |
| spring enrollment | keyword | 1 | 1 | 1.000 | 0.444 |
| duplicate survey responses removed | keyword | 1 | 1 | 0.940 | 0.609 |
| food bank inventory app presentation | paraphrase | 1 | 1 | 0.800 | 0.423 |
| transit dataset assignment requirements | keyword | 1 | 1 | 0.880 | 0.522 |
| my thesis about why Reconstruction failed | paraphrase | 1 | 1 | 0.850 | 0.555 |
| the problem our capstone app solves | paraphrase | 1 | 1 | 0.850 | 0.448 |
| the essay draft I was working on earlier today | time | 1 | 1 | 0.800 | 0.619 |
| the pandas error I hit earlier today | time | 1 | 1 | 0.759 | 0.455 |
| the capstone standup two days ago | time | 1 | 1 | 0.800 | 0.539 |
| the ridership charts I worked on two days ago | time | 1 | 1 | 0.829 | 0.572 |
| the survey themes I coded yesterday | time | 1 | 1 | 0.880 | 0.604 |
| the course reader chapter I read yesterday | time | 1 | 1 | 0.900 | 0.445 |
| what Dana asked me to do in Slack | app | 1 | 1 | 0.900 | 0.472 |
| the labor contracts passage in the PDF | app | 1 | 1 | 0.940 | 0.600 |
| the survey data in Sheets | app | 1 | 1 | 0.850 | 0.575 |
| the pandas error in Terminal | app | 1 | 2 | 0.850 | 0.427 |
| the pandas error from five days ago | time | 1 | 1 | 0.786 | 0.389 |
| the weekday vs weekend chart from two days ago | time | 1 | 2 | 0.887 | 0.669 |
| the pandas page I had open in Chrome | app | 1 | 1 | 0.690 | 0.430 |
| flight confirmation to Denver | negative | miss | miss | none | 0.246 |
| organic chemistry lab report | negative | miss | miss | 0.649 | 0.351 |
| apartment lease renewal | negative | miss | miss | none | 0.235 |
| dentist appointment reminder | negative | miss | miss | none | 0.244 |
