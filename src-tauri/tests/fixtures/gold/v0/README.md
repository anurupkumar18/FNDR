# Gold set v0 (MOD-04)

**Status: draft, awaiting owner review.** Every case here was labeled by
Claude, not a second independent human labeler, so there is no real
inter-labeler agreement percentage yet. This is a starting point sized and
shaped to unblock MOD-05/06/08/15, MEM-09, RET-01, and DEC-02, not a
finished, agreement-verified gold set. Review before trusting any number
`make eval` produces from it.

## What's here

- `extraction.jsonl` (50 lines): 25 grounded in the real CAP-03 30-screen
  fixture corpus (`src-tauri/tests/fixtures/screens/manifest.json`), using
  its `expected_text` verbatim as `evidence`. 25 more are original synthetic
  cases covering activity types the CAP-03 corpus doesn't exercise
  (debugging, reviewing_agent_output, watching_or_listening,
  job_or_career_work, travel_or_logistics, entertainment_or_personal_interest,
  observing, screen_review, unknown). No scraped or copyrighted content;
  all synthetic app names, people, and text.
- `questions.jsonl` (30 lines): retrieval questions, each pointing at the
  `extraction.jsonl` id(s) that should be the relevant memory.
- `guard.jsonl` (20 lines): same shape as `extraction.jsonl`, distinct
  content, held out from any tuning. Its only job is catching a regression;
  never look at it while iterating on a prompt or schema.
- `review-draft.jsonl` (1 line): a synthetic contract fixture for the P12
  review scorer. Its label is intentionally draft, so a scorer run must
  withhold all quality claims while still reporting structural counts.
- Splits inside `extraction.jsonl`: `dev` (use freely while iterating) and
  `test` (only look at when reporting a final number).

## Labeling rules used

- `activity_type` is one of the 19 values in `CANONICAL_ACTIVITY_TYPES`
  (`src-tauri/src/inference/mod.rs:646`). Every value in this set was checked
  against that list programmatically.
- `topic` is at most 12 words and states what the user was doing. Also
  checked programmatically; none exceed 12 words.
- `must_mention`: facts that must appear in a correct extraction, taken
  directly from `evidence`.
- `must_not`: traps a model that hallucinates content from a *different*
  gold case falls into (usually a near-neighbor case's distinctive term).
- Nothing in `expected` states a fact absent from `evidence`.

## What still needs a human pass

1. **Spot-check the activity_type and topic labels**, especially the
   ambiguous ones: `g-005` (README usage docs, labeled `studying`),
   `g-009` (`git status`, labeled `organizing_information` rather than
   `coding`), `g-018`/`g-020` (personal chat, labeled `communication` rather
   than `entertainment_or_personal_interest`). These are judgment calls a
   second labeler may disagree with.
2. **Re-label 10 cases blind and compute agreement** on `activity_type`,
   per the original ticket's Step 3. If agreement comes out below 80
   percent, the label definitions need tightening, not the labels
   themselves.
3. **Decide whether 25 CAP-03-grounded + 25 synthetic is the right ratio**,
   or whether more should come from real captures (synthetic-only, per the
   Global Constraints — no real personal captures in git).
4. Once satisfied, remove this section and record the real agreement result
   in its place.

## Verify the files parse and the counts are right

```bash
python3 - <<'PY'
import json
d = "src-tauri/tests/fixtures/gold/v0/"
counts = {f: sum(1 for l in open(d + f) if l.strip()) for f in ["extraction.jsonl", "questions.jsonl", "guard.jsonl"]}
print(counts)
assert counts == {"extraction.jsonl": 50, "questions.jsonl": 30, "guard.jsonl": 20}
for f in counts:
    for l in open(d + f):
        if l.strip():
            json.loads(l)
print("ok")
PY
```

## Score post-capture review output

The scorer only consumes saved synthetic/public JSONL. It never calls a model,
opens LanceDB, or reads real captures.

```bash
python3 scripts/model/score_memory_review.py \
  src-tauri/tests/fixtures/gold/v0/review-draft.jsonl \
  --out /tmp/fndr-review-draft.md
```

The report must say that quality is unavailable because this fixture is still
draft. Guard cases require `--report-guard` and are report-only.
