# VS-08 reciprocal rank fusion: measured, not adopted (cloud, negative result)

## What was tried

Ask's fusion (`context_runtime/fusion.rs::fuse`) adds each route's score times the route weight (`FusionWeights::for_intent`). VS-08 asked for one pure `fuse_rrf` function and a choice of k by MRR@10. I implemented it test-first, ordered Ask's fused hits by it (keeping each hit's weighted score as its absolute evidence strength, because the verifier's 0.3 confidence threshold and the cards read that score), and ran the retrieval gate for k = 30, 60, 90 on both personas, all on the same fresh seed. The function, for anyone rerunning this:

```rust
pub struct RankedList { pub weight: f32, pub hits: Vec<(String, f32)> }

/// Each list sorted by score (ties by id), each id counted once at its best
/// rank, adding weight / (k + rank) with 1-based ranks. Highest first, ties by id.
pub fn fuse_rrf(lists: &[RankedList], k: f32) -> Vec<(String, f32)> {
    let mut fused: HashMap<&str, f32> = HashMap::new();
    for list in lists {
        let mut ranked = list.hits.iter().collect::<Vec<_>>();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut seen = HashSet::new();
        for (id, _) in ranked {
            if seen.insert(id.as_str()) {
                *fused.entry(id.as_str()).or_default() += list.weight / (k + seen.len() as f32);
            }
        }
    }
    let mut fused = fused.into_iter().map(|(id, s)| (id.to_string(), s)).collect::<Vec<_>>();
    fused.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    fused
}
```

Its unit tests (ties, an empty list, a doc found by only one list, score scales ignored, list weights honored, each doc counted once per list, and `fuse` ordering by ranks while keeping the strength as `score`) all passed, and the ordering test failed on the old score-only sort, as expected.

## Results (Ask path; Search does not use this fusion)

| Persona | Fusion | Recall@5 (kw+para) | MRR@10 (kw+para) | Paraphrase Recall@5 | Time Recall@5 | MRR@10 (all positive kinds) |
|---|---|---:|---:|---:|---:|---:|
| knowledge-worker | weighted score fusion (today) | 1.000 | 0.966 | 1.000 | 1.000 | 0.950 |
| knowledge-worker | RRF k=30 | 0.955 | 0.938 | 0.875 | 1.000 | 0.913 |
| knowledge-worker | RRF k=60 | 0.955 | 0.938 | 0.875 | 1.000 | 0.913 |
| knowledge-worker | RRF k=90 | 0.955 | 0.938 | 0.875 | 1.000 | 0.913 |
| knowledge-worker | score-weighted RRF k=60 | 0.955 | 0.960 | 0.875 | 1.000 | 0.932 |
| office-pm | weighted score fusion (today) | 0.900 | 0.661 | 0.778 | 1.000 | 0.750 |
| office-pm | RRF k=30 | 0.850 | 0.685 | 0.667 | 1.000 | 0.764 |
| office-pm | RRF k=60 | 0.850 | 0.685 | 0.667 | 1.000 | 0.759 |
| office-pm | RRF k=90 | 0.850 | 0.685 | 0.667 | 1.000 | 0.759 |
| office-pm | score-weighted RRF k=60 | 0.850 | 0.705 | 0.667 | 1.000 | 0.791 |

1. **k does not matter here**: 30, 60, and 90 give identical Recall@5 and MRR@10 on both personas (all-kinds MRR differs by 0.005 on office-PM).
2. **RRF loses Recall@5 on paraphrase queries** on both personas (one query each falls out of the top five: "which plotting library am I allowed to use" 4 to 8, "how much money are we losing to customers leaving" 5 to 10) and gains MRR on office-PM. The mechanism, read from the route traces of the second query: the vector route ranks the right answers (the churn rows) first; the keyword route returns 9 weak one-word matches (on "customers") for this paraphrase; under RRF every keyword-found document earns a full rank vote, so documents found weakly by both routes outrank the vector route's top three. Weighted score fusion damps those weak votes because their BM25 score is low.
3. A score-weighted variant (`weight * score / (k + rank)`) has the best MRR@10 on office-PM (0.705 against 0.661) but keeps the same Recall@5 loss.

## Decision

The rule set before the last run: adopt a rank fusion only if Recall@5 is no lower than today's fusion on both personas and MRR@10 does not drop. Neither variant passes the Recall@5 half, so Ask keeps weighted score fusion and no fusion code changes. Recall@5 is the Beta metric and the merge gate's metric; the differences are one or two queries per persona, within what a larger query set could reverse.

Revisit when VS-18 adds chunk vector and chunk BM25 routes: more routes on different scales is where rank fusion earns its keep. The function above and the gate make that a one-hour rerun.

## Verification

- Gate runs: knowledge-worker and office-PM, `make qa-retrieval-check QA_SKIP_SEED=1` after one reseed, for k = 60, 30, 90 and the score-weighted variant; every run passed the gate (exit 0, no query lost), which is why the decision rests on the stricter adopt rule above.
- `cargo test --lib context_runtime`: 57 passed with the RRF change; the change is reverted, so the committed code is unchanged.
