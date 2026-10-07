# VS-02 office-PM persona and 20 queries (cloud)

## What was added

- `scripts/demo/office-pm-week.json`: 40 synthetic memories from one fictional PM week at a fictional company ("Lumen Ledger"). Three projects: Smart Reminders launch (16), Senior Product Designer hiring (9), Q3 metrics review (9), plus 5 distractors and 1 low-signal control. Apps: Google Chrome (Docs, Sheets, Gmail, Calendar, a fictional tracker and ATS) 20, Slack 9, Notion 3, Keynote 3, zoom.us 2, Microsoft Excel 1, Preview 1, Finder 1. Two near-duplicate documents (`pm-launch-checklist-v3` and `-v4`, identical except the title and one checked box) and one Slack thread revisited on four days (`pm-launch-thread-d1` to `-d4`). All names, companies, and URLs are fictional (`*.example`, `*.lumenledger.example`, fake document ids).
- `scripts/demo/office-pm-queries.json`: 20 queries, 11 keyword and 9 paraphrase; 5 name a person, 5 carry a number or code (LL-1482, REQ-2207, $1.24M, 3.9%, 2.1%). Paraphrase queries share at most one incidental content word with their targets.
- `retrieval_qa` now validates any case set (non-empty, known kind, unique query, at least one relevant id) instead of the knowledge-worker counts, and reports kind counts from the cases. Fixture tests check both personas: corpus size, unique ids, exact kind mix, every relevant id exists, and no relevant id is the low-signal control.
- `make qa-seed PERSONA=office-pm`, `make qa-retrieval PERSONA=office-pm`, and `make qa-retrieval-check PERSONA=office-pm` use their own profile (`com.fndr.app.qa-office-pm`) and write the baseline to `docs/evidence/W03/retrieval-baseline-office-pm.{md,json}`. Without `PERSONA` nothing changes.
- The baseline JSON is promoted to `scripts/demo/retrieval-reference/office-pm.json`, so the VS-04 gate covers both personas.

## Baseline (Linux cloud container, seeded 2026-10-04)

Full table: `retrieval-baseline-office-pm.md`.

| Path | Recall@5 | MRR@10 | Keyword Recall@5 | Paraphrase Recall@5 |
|---|---:|---:|---:|---:|
| Search | 0.700 | 0.589 | 1.000 | 0.333 |
| Ask | 0.900 | 0.657 | 1.000 | 0.778 |

Top-1 agreement: 3/20.

What it says:

- The knowledge-worker set was too easy. On office-PM, Search finds 3 of 9 paraphrase queries in its top five and misses 6 of them entirely in its top ten. Ask finds 7 of 9. This is the gap VS-05 (word-overlap cutoff) and VS-07/VS-08 (BM25 plus fusion) have to close.
- Search and Ask agree on the top result for only 3 of 20 queries. Even where both find the answer they rank different memories first, which is the case for one retrieval path (VS-09 to VS-11).
- Keyword queries with names and codes are solved by both paths (Recall@5 1.000).
- Ask misses "why were our payment nudges ending up in junk mail" outright while Search has it at rank 4.

Stability: three runs on one profile and a run after reseeding gave identical ranks (requires the second VS-04 commit, which lifts route time budgets in the harness).

## Verification

- `cargo test --example retrieval_qa`: 11 passed. The new fixture and validation tests failed first (`cannot find function kind_counts`) and pass after.
- `make qa-seed PERSONA=office-pm`: stored 40, surfaced 39, needs-signal 1.
- `make qa-retrieval PERSONA=office-pm` and `make qa-retrieval-check PERSONA=office-pm`: output above; the check passes against the promoted reference.
- Run on Linux with the local build shim (N10). Local should rerun on the M1 at the gate; ranks are expected to match, as they did for the knowledge-worker set.
