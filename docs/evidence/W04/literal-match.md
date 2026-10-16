# Exact literals in a query settle near ties, 2026-10-13

**Change.** `context_runtime::fusion` adds up to 0.08 (`LITERAL_MATCH_BONUS`) to a memory in proportion to the query's literals it holds as whole tokens. A literal is a token with a digit and punctuation inside it ("1.8%", "$1.24M", "LL-1482") or with an underscore ("labels_idempotency_key_key"). The keyword index splits these into pieces many memories share: "1.8%" became 1 and 8, so the thread that said "18%" ranked above the one that said "1.8%".

**How it was measured.** Each persona was seeded today in its own profile; `retrieval_qa` ran with the change, then with `fusion.rs` reverted, on the same profiles; `scripts/audit/retrieval_check.py` compared them. `FNDR_QA_SHOW_MISSES=1` (added to `retrieval_qa` in this change) lists what Search puts above the first relevant memory, which is how the cases were found.

## knowledge-worker (before, then after)

| Path | Recall@5 ref | Recall@5 now | Delta | MRR@10 ref | MRR@10 now | Delta | p95 ms ref | p95 ms now |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| search | 1.000 | 1.000 | +0.000 | 0.909 | 0.909 | +0.000 | 184 | 185 |
| ask | 1.000 | 1.000 | +0.000 | 0.909 | 0.909 | +0.000 | 620 | 600 |
| retrieve | 1.000 | 1.000 | +0.000 | 0.909 | 0.909 | +0.000 | 186 | 190 |

No rank changed.

## office-pm

| Path | Recall@5 ref | Recall@5 now | Delta | MRR@10 ref | MRR@10 now | Delta | p95 ms ref | p95 ms now |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| search | 0.950 | 0.950 | +0.000 | 0.654 | 0.679 | +0.025 | 182 | 221 |
| ask | 0.950 | 0.950 | +0.000 | 0.654 | 0.679 | +0.025 | 798 | 1220 |
| retrieve | 0.950 | 0.950 | +0.000 | 0.654 | 0.679 | +0.025 | 180 | 192 |

"LL-1482 spam placement 1.8%" moved from rank 2 to 1. No other rank changed.

## software-engineer

| Path | Recall@5 ref | Recall@5 now | Delta | MRR@10 ref | MRR@10 now | Delta | p95 ms ref | p95 ms now |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| search | 1.000 | 1.000 | +0.000 | 0.883 | 0.914 | +0.032 | 183 | 196 |
| ask | 1.000 | 1.000 | +0.000 | 0.883 | 0.914 | +0.032 | 637 | 656 |
| retrieve | 1.000 | 1.000 | +0.000 | 0.883 | 0.914 | +0.032 | 175 | 178 |

"UniqueViolation labels_idempotency_key_key" moved from rank 3 to 1. No other rank changed.

## No-match queries

Identical before and after on all three personas: the same negatives under the 0.25 bar, the same highest negative score (0.499, 0.537, 0.299), and the same one real query under the bar ("the survey themes I coded yesterday", 0.123 on knowledge-worker, a time query that predates this change).

## Limits

Synthetic corpora of 20 to 40 memories. The p95 columns come from one run each and moved both ways (office-pm Ask 798 to 1220 ms, knowledge-worker Ask 620 to 600 ms); the added work is a substring check per candidate, so treat them as noise until measured on purpose. Most remaining misses are paraphrases ("customers leaving" for churn), which this does not address.
