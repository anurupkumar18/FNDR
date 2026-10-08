# VS-13 follow-up: a date or day after a deadline word is not a capture filter (cloud)

## The bug

Local's merge gate fixed weekdays (b1a1776): "churn drivers due Thursday" no longer filters to memories captured last Thursday. The same reading still applied to the other two time phrases VS-13 parses:

- **Dates.** "the grant report due October 9", asked on October 3, filtered to memories captured on October 9 *of last year*, because a date later than today is read as last year's. For anyone with captures from that day, the results are limited to it.
- **Day phrases.** "the invoice due today" filtered to memories captured today, hiding the invoice captured last week that says it is due today.

**When it shows (correction to the first version of this note).** `retrieve` searches again without the filter when a parsed filter matches nothing (`retrieve_with_fused`). So the date case changes results only when the misread day has captures. That is the case for a profile more than a year old, and always for "due today". On a profile younger than a year, "due October 9" falls back to an unfiltered search and is unaffected. The third persona's "Postgres 16 runbook due October 9" query ranks the same with and without this change for that reason (`persona-software-engineer-cloud.md`).

## The change

`find_time` (`src-tauri/src/context_runtime/query_filters.rs`) now skips a date or day phrase that follows a deadline word, as it already did for weekdays. The deadline words are local's list: due, by, until, before, till, and next. When the first match follows a deadline word, a later date in the same query can still be a filter: the date search now looks at every match, not only the first.

## Test (failed first)

`look_alikes_are_not_filters` gains "the grant report due October 9" and "the invoice due today". On the code before this change it failed:

```
assertion `left == right` failed: the grant report due October 9
  left: Some(TimeRange { start_ms: 1759968000000, end_ms: 1760054400000 })
 right: None
```

That range is October 9, 2025, a year before the deadline. With the change, `query_filters` passes 6 of 6 and `cargo test --lib` passes 910.

## Retrieval gate, before and after, same seed

Both personas were seeded once under America/Denver time on 2026-10-05, then the gate ran on 3e9c16c (train F) and on this change. All four runs pass. No labeled query has a deadline word before a date or day phrase, so this checks that nothing else moved.

| Persona | Path | Recall@5 before / after | MRR@10 before / after | p50 ms before / after | p95 ms before / after |
|---|---|---|---|---|---|
| knowledge-worker | search | 1.000 / 1.000 | 0.966 / 0.966 | 223 / 242 | 388 / 380 |
| knowledge-worker | ask | 1.000 / 1.000 | 0.966 / 0.966 | 880 / 926 | 969 / 1025 |
| knowledge-worker | retrieve | 1.000 / 1.000 | 0.966 / 0.966 | 201 / 208 | 336 / 369 |
| office-pm | search | 0.900 / 0.900 | 0.661 / 0.661 | 222 / 228 | 367 / 421 |
| office-pm | ask | 0.900 / 0.900 | 0.661 / 0.661 | 1375 / 1318 | 1551 / 1482 |
| office-pm | retrieve | 0.900 / 0.900 | 0.661 / 0.661 | 201 / 199 | 330 / 357 |

Every per-query rank is identical on all three paths; top scores differ by at most 0.0002 (recency between runs). Latency moves both ways within noise.

## Not covered

The labeled sets have no deadline-date query, so the gate cannot show the user-facing gain. A third persona (Part 9 stretch) should include one, for example "the grant report due October 9".
