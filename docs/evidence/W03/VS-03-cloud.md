# VS-03 time, app, and "nothing matches" queries (cloud)

## What was added

- Both query sets gain 8 time queries, 5 app queries, and 4 negative queries (knowledge-worker: 22 to 39 queries; office-PM: 20 to 37). Time phrases are relative only ("yesterday", "this morning", "earlier today", "two days ago", "last week", "three/four/five days ago"), never weekday names or dates, so the right answer does not depend on which weekday the profile is seeded. Negative queries have `"relevant_ids": []` and were checked against the corpus for absent key terms.
- The report moves to schema v2:
  - kinds are open maps (`case_count_by_kind`, `recall_at_5_by_kind`);
  - every query row carries each path's top score (`search_top_score`, `ask_top_score`; null when the path returned nothing);
  - each path gets a `no_match` block (negative cases, how many returned nothing, median top score on negative versus positive queries).
- The headline Recall@5 and MRR@10 still cover keyword and paraphrase only, so they stay comparable with VS-01 and the Beta target; time and app get their own columns; negative queries never count as misses. Verified: against the v1 references every existing query kept its exact rank on both personas, and the new queries were reported as new.
- `retrieval_check.py` accepts v1 and v2, compares a v1 reference with a v2 report, and prints the no-match table (reported, not gated).
- Both references are promoted to v2 (`scripts/demo/retrieval-reference/*.json`), and full baselines are in `docs/evidence/W03/retrieval-baseline-{knowledge-worker,office-pm}.md`. The W02 VS-01 files are untouched.

## Rows (Linux cloud container, seeded 2026-10-04)

| Persona | Path | Recall@5 (kw+para) | Keyword | Paraphrase | Time | App | Negative: returned nothing | Median top score, negative | Median top score, positive |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| knowledge-worker | search | 0.955 | 1.000 | 0.875 | 1.000 | 1.000 | 3/4 | 0.649 | 0.850 |
| knowledge-worker | ask | 1.000 | 1.000 | 1.000 | 1.000 | 1.000 | 0/4 | 0.245 | 0.472 |
| office-pm | search | 0.700 | 1.000 | 0.333 | 0.875 | 1.000 | 2/4 | 0.522 | 0.880 |
| office-pm | ask | 0.900 | 1.000 | 0.778 | 0.875 | 1.000 | 0/4 | 0.233 | 0.474 |

## What the numbers say, honestly

1. Most time and app queries name a unique topic, so they reach rank 1 with no time or app understanding at all (the Search screen gets no filters from the text today). I added three discriminating queries per persona that target the older of two versions, or one app among several; two of them separate the paths: "the launch checklist from three days ago" (Search rank 3, the newer v4 ranks above it) and "the LL-1482 spam discussion from four days ago" (Search rank 7, Ask rank 3). Recency, not the phrase, is what answers "yesterday" and "earlier today" queries today. **VS-13's "Done when" (time and app rows at Recall@5 0.9) is already nearly met without a parser on these sets, so VS-13 should be judged on the discriminating rows (rank 1 for the older version) rather than on Recall@5.**
2. Negative queries: the Search path returns nothing for 3 of 4 knowledge-worker negatives and 2 of 4 office-PM negatives because of its word-overlap cutoff. The same cutoff also returns nothing for 4 of 9 office-PM paraphrase queries, so "returned nothing" cannot be the no-match signal (VS-05 removes the cutoff). Ask always returns something; its median top score is about 0.24 on negatives against about 0.48 on positive queries, but seven of the nine office-PM paraphrase queries have an Ask top score between 0.265 and 0.336, overlapping the negatives (up to 0.288), so a single Ask threshold today would hide correct paraphrase answers. VS-12 should pick its threshold on the fused `retrieve` score with these rows, not on today's scores.

## Verification

- `cargo test --example retrieval_qa`: 15 passed (new: kind columns, no-match scoring, negative validation, schema v2 keys, markdown table). Written first; the fixture tests failed with "time case count changed" before the queries were added.
- `python3 scripts/audit/test_retrieval_check.py`: 24 OK (new: v2 validity, v1 reference against v2 report, negative queries never lost, no-match rendering).
- `make qa-retrieval-check` and `make qa-retrieval-check PERSONA=office-pm` (both reseeded): PASS against the v1 references with "No per-query rank changed", then against the promoted v2 references.
- Run on Linux with the local build shim (N10); local should rerun on the M1 at the gate.
