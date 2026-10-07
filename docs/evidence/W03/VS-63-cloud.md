# VS-63 the retrieval gate tolerates near-ties at the bottom of the top ten (cloud)

## Change

`scripts/audit/retrieval_check.py`:

- A query that a path ranked 8, 9, or 10 in the reference and now misses is a **warning**: listed under "Warnings", and the verdict reads "PASS with N warnings".
- A query lost from ranks 1 to 7 still fails, as do a Recall@5 drop over 0.05 and a missing path or query. Ranks 8 to 10 never count toward Recall@5, so the recall rule is unchanged.
- The rule is one constant, `NEAR_TIE_FROM_RANK = 8`. The gate sentence in every report states it, and the Makefile and the CI workflow comments say the same.

## Tests (written first; three failed on the old comparator)

- `test_a_miss_from_the_bottom_of_the_top_ten_is_a_warning`: ranks 8 and 10 lost give two warnings and no failure.
- `test_a_miss_from_rank_seven_still_fails`.
- `test_main_passes_with_a_warning_and_lists_it`: exit 0, "PASS with 1 warning", and the query is listed.
- All 29 comparator tests pass.

## The M1 case, replayed

Local's office-PM run on the M1 lost "would clients recommend us, and did that improve", which the reference ranks 9th on every path (status report, 2026-10-05). The cloud replayed that loss against the committed reference:

```
old comparator: # Retrieval check: office-pm: FAIL
new comparator: # Retrieval check: office-pm: PASS with 3 warnings
- search lost 'would clients recommend us, and did that improve' (paraphrase): rank 9 -> miss
- ask lost ...
- retrieve lost ...
```

On the latest real runs (three personas, train F head), old and new comparators give the same verdict: PASS, no warnings.

## Open for local, and a caution on the cause

- **Regenerating the office-PM reference on the M1 is local's half of the ticket.** With this change it is no longer needed for the gate to pass, but it would record the M1 numbers.
- **The cause may not be the platform.** GitHub's Apple-silicon `macos-26` runner reproduced every Linux-recorded rank, on PR #31 and on the negative control #32 (`VS-34-cloud.md`). Other candidates:
  - the hour of the seed: the office-PM corpus mixes clock-time and minutes-ago entries, and recency moves near-ties;
  - a different MiniLM file (the pinned sha256 is `759c3cd2...`);
  - production route budgets, if the run did not go through `retrieval_qa`, which lifts them.
- **A warning still deserves a look.** Several warnings on one run, or the same query warning on every run, point at a real loss, not a near-tie.
