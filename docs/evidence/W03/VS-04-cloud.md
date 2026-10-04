# VS-04 retrieval merge gate (cloud)

`make qa-retrieval-check` reseeds the QA profile, reruns `retrieval_qa` into a scratch report under `src-tauri/target/`, and compares it with the accepted reference for the case set (`scripts/demo/retrieval-reference/<case-set>.json`). It exits non-zero when any reference path's Recall@5 drops more than 0.05, when a query that a path found in its top ten becomes a miss, or when a reference path or query disappears. MRR@10, per-kind Recall@5, latency, top-1 agreement, and rank moves inside the top ten are printed but do not block.

- Comparator: `scripts/audit/retrieval_check.py` (pure Python, standard library only), tests in `scripts/audit/test_retrieval_check.py`.
- Reference: the VS-01 JSON (`docs/evidence/W02/retrieval-baseline-seeded.json`) copied unchanged to `scripts/demo/retrieval-reference/knowledge-worker.json`. A deliberate change to the accepted numbers updates that file in the same merge request (rule added to `docs/team/TEAM.md`).
- `QA_SKIP_SEED=1` skips the reseed for quick iteration. Reseeding is the default because capture-text, chunking, and embedding changes act at seed time.
- `make qa-retrieval` is unchanged and still writes the W02 evidence paths.

## Where this ran

Linux x86_64 cloud container, not the M1. The crate does not build on Linux as-is; it built here with a local, uncommitted shim (cfg-gating five macOS-only call sites, proposed as N10 in the cloud proposals file on `claude/train-e-docs`). MiniLM was downloaded from the pinned URL and both sha256 pins matched. Local should rerun the pass case on the M1 before merging.

Platform finding: on Linux every per-query rank@10 matched the M1 reference exactly for both paths. Top-1 agreement differed (18/22 here, 17/22 on the M1), so at least one non-relevant top result differs by platform; VS-21 (deterministic ties) should look at this. Latency is slower here (no Metal, debug build) and is not gated.

## Tests

`python3 scripts/audit/test_retrieval_check.py -v`: 20 tests, OK. Written first; the first run failed with `ModuleNotFoundError: No module named 'retrieval_check'`, and the top-1 agreement test failed before its line was rendered.

## Pass (current code)

```
# Retrieval check: knowledge-worker: PASS

Gate: Recall@5 may drop at most 0.05 on any path, and no query found in a path's top ten may become a miss.

| Path | Recall@5 ref | Recall@5 now | Delta | MRR@10 ref | MRR@10 now | Delta | p95 ms ref | p95 ms now |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| search | 0.955 | 0.955 | +0.000 | 0.909 | 0.909 | +0.000 | 764 | 1005 |
| ask | 1.000 | 1.000 | +0.000 | 1.000 | 1.000 | +0.000 | 1418 | 2495 |

Top-1 agreement: 17/22 -> 18/22 (reported, not gated).

| Path | Kind | Recall@5 ref | Recall@5 now | Delta |
|---|---|---:|---:|---:|
| search | keyword | 1.000 | 1.000 | +0.000 |
| search | paraphrase | 0.875 | 0.875 | +0.000 |
| ask | keyword | 1.000 | 1.000 | +0.000 |
| ask | paraphrase | 1.000 | 1.000 | +0.000 |

No per-query rank changed.
```

## Fail: reranker broken on purpose

`HARD_COVERAGE_THRESHOLD` in `src-tauri/src/search/reranker.rs` raised from 0.15 to 0.34, run, then reverted (`git checkout`). `make` exited with `Error 1` from the check and named the four paraphrase queries that regressed:

```
# Retrieval check: knowledge-worker: FAIL

Gate: Recall@5 may drop at most 0.05 on any path, and no query found in a path's top ten may become a miss.

| Path | Recall@5 ref | Recall@5 now | Delta | MRR@10 ref | MRR@10 now | Delta | p95 ms ref | p95 ms now |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| search | 0.955 | 0.773 | -0.182 | 0.909 | 0.750 | -0.159 | 764 | 997 |
| ask | 1.000 | 1.000 | +0.000 | 1.000 | 1.000 | +0.000 | 1418 | 2488 |

Top-1 agreement: 17/22 -> 15/22 (reported, not gated).

| Path | Kind | Recall@5 ref | Recall@5 now | Delta |
|---|---|---:|---:|---:|
| search | keyword | 1.000 | 1.000 | +0.000 |
| search | paraphrase | 0.875 | 0.375 | -0.500 |
| ask | keyword | 1.000 | 1.000 | +0.000 |
| ask | paraphrase | 1.000 | 1.000 | +0.000 |

## Regressions

- search Recall@5 dropped 0.955 -> 0.773 (-0.182, allowed -0.050)
- search lost 'how did I get python to find the data library last time' (paraphrase): rank 2 -> miss
- search lost 'what customers complained about during onboarding' (paraphrase): rank 1 -> miss
- search lost 'how much of my history grade depends on sources' (paraphrase): rank 1 -> miss
- search lost 'food bank inventory app presentation' (paraphrase): rank 1 -> miss

## Rank changes

| Query | Kind | Path | Ref rank@10 | Now rank@10 |
|---|---|---|---:|---:|
| how did I get python to find the data library last time | paraphrase | search | 2 | miss |
| what customers complained about during onboarding | paraphrase | search | 1 | miss |
| how much of my history grade depends on sources | paraphrase | search | 1 | miss |
| food bank inventory app presentation | paraphrase | search | 1 | miss |
```

A cruder break (threshold 0.9) dropped Search Recall@5 to 0.227 and the check listed 16 lost queries; Ask stayed at 1.000 because it does not use this reranker.
