# Third persona: a backend engineer's week (cloud)

Part 9 stretch item: a third synthetic persona, so retrieval changes are not tuned to two.

## The corpus

`scripts/demo/software-engineer-week.json` holds 26 memories over seven days, one of them low-signal (a Finder desktop). The threads:

- a production latency incident (INC-2291): page, dashboard, incident channel, `kubectl` rollback, postmortem, follow-up ticket;
- an idempotency pull request (#4187): opening, CI failure, fix in VS Code, review, merge;
- a Postgres 16 upgrade runbook, due October 9: outline, DBA thread choosing logical replication, rewrite;
- a rate limiting design: draft, review, v2;
- onboarding a new teammate and reviewing her pull request;
- a 1:1 about promotion.

Two reading memories (tail latency, a Redis rate limiter) share words with the incident and design threads as distractors. Apps: Chrome, Slack, Notion, Terminal, Visual Studio Code, Zoom.

`scripts/demo/software-engineer-queries.json` holds 34 queries: 12 keyword, 9 paraphrase, 5 time, 4 app, 4 no-match.

- **Written before any run.** No query or label was changed after seeing results.
- **Time queries use whole days only** ("yesterday", "two days ago" to "five days ago"), so the gate does not depend on the hour it runs. There are no weekday names, which is the VS-13 lesson.
- **Keyword queries include identifier shapes:** `INC-2291`, `ENG-912`, `labels_idempotency_key_key`, `Retry-After`, `test_track_webhook_retry`. One is a deadline date: "Postgres 16 runbook due October 9".

## First runs (Linux, debug build, America/Denver)

| Path | Recall@5 | MRR@10 | Keyword | Paraphrase | Time | App | Negatives under the bar | Positives under the bar | p50 ms | p95 ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| search | 1.000 | 0.831 | 1.000 | 1.000 | 1.000 | 1.000 | 3 / 4 | 0 | 223 | 369 |
| ask | 1.000 | 0.831 | 1.000 | 1.000 | 1.000 | 1.000 | 3 / 4 | 0 | 1024 | 1214 |
| retrieve | 1.000 | 0.831 | 1.000 | 1.000 | 1.000 | 1.000 | 3 / 4 | 0 | 203 | 315 |

- **Every labeled query is in the top five, and the room is in rank.** Six queries are not first:
  - "UniqueViolation labels_idempotency_key_key" is 4th. An exact identifier loses to memories that say "idempotency key" many times.
  - "what broke in the build for the labels change" is 5th.
  - Four more are 2nd.
- **Stable across seeds.** A second, fresh seed reproduced every per-query rank ("No per-query rank changed"). Search and Ask agree on the top result for 34 of 34.
- **No-match.** 3 of 4 no-match queries fall under VS-12's bar, against 2 of 4 and 1 of 4 on the other personas, and no real query does.
- **The deadline-date query does not reach the VS-13 bug at this level.** It ranks the same with and without the deadline fix (98045d1). No memory falls on October 9 of last year, so the misread filter matched nothing and `retrieve` fell back to an unfiltered search. The unit test covers the parse.

The first run is the reference: `scripts/demo/retrieval-reference/software-engineer.json`.

## In the gate

- `make qa-retrieval-check PERSONA=software-engineer` runs it locally.
- The VS-34 workflow now runs it as a third step.
- Out of sample, it contradicted one finding from the two original personas (`retrieval-ablation-cloud.md`). Without BM25 its paraphrases rank lower, not higher, so the proposed keyword-weight change (VS-43) was withdrawn.
