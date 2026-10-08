# VS-34 the retrieval gate runs in GitHub Actions (cloud)

## What runs

`.github/workflows/retrieval-gate.yml`, job "Retrieval gate (synthetic corpora)", on pull requests into main and pushes to main that touch `src-tauri/**`, `scripts/demo/**`, `scripts/audit/retrieval_check.py`, the `Makefile`, or the workflow itself.

1. Same runner and build setup as `test.yml`: `macos-26` (the Swift speech helper needs the macOS 26 SDK), stable Rust, the cargo cache, `protobuf`, and an empty `dist`.
2. **The embedder cache.** The pinned MiniLM (`Xenova/all-MiniLM-L6-v2` at revision 751bff3, the same URL as `EMBEDDING_MODEL_DOWNLOAD_URL` in `src-tauri/src/inference/model_config.rs`) is restored from the Actions cache, or downloaded once. Both files are then checked against the sha256 pins in `model_config.rs` before anything runs. The models folder is linked where the seed script and the embedder look for it.
3. `make qa-retrieval-check` for knowledge-worker, then for office-PM (the second runs even when the first fails). Each run seeds a fresh synthetic profile from `scripts/demo/` under `TZ=America/Denver` and compares the report with `scripts/demo/retrieval-reference/`.
4. Both `check.md` reports are added to the job summary.

The job fails when any path's Recall@5 drops more than 0.05, or when a query a path found in its top ten becomes a miss (VS-04's rule in `scripts/audit/retrieval_check.py`). No real data is involved: the runner has no FNDR profile, and the only download is the pinned model.

## Runs

| Run | Commit | Job | Result | Time |
|---|---|---|---|---|
| Pass: PR #31 (train F) | ba06763 | https://github.com/anurupkumar18/FNDR/actions/runs/37340425264/job/111865808214 | success; both personas PASS, "No per-query rank changed" | 15 min 27 s, cold caches |
| Fail: negative control, PR #32 (closed) | 27fac82, the planner drops the vector route | https://github.com/anurupkumar18/FNDR/actions/runs/37340909547/job/111867434041 | failure; office-PM lost five paraphrase queries from the top ten | 17 min 21 s |

- **The negative control fails on the rule it should.** Office-PM lost five queries from the top ten, for example "which applicant came out on top for the design role" (rank 3 to a miss). The cloud measured the same change before pushing: knowledge-worker lost "which plotting library am I allowed to use" (rank 4 to a miss), and office-PM paraphrase Recall@5 fell 0.778 to 0.444.
- **macOS and Linux agree query by query.** On the negative control, every office-PM rank in the CI log equals the cloud's Linux run on the same change. For example, "who is filling in during my end-of-year vacation" went 1 to 4 and "LL-1482 spam placement 1.8%" went 2 to 1 on both. On #31 the macOS run reproduced the Linux-recorded references exactly. So the references can stay platform-independent.
- **The first choice of control passed the gate, which is itself a finding.** Taking the keyword route out did not lower Recall@5. It moved knowledge-worker to 1.000 and office-PM from 0.900 to 0.950, and lowered MRR@10 (0.966 to 0.947, 0.661 to 0.648). No top-ten hit was lost, so the gate passed. Dropping the vector route was used instead. See `retrieval-ablation-cloud.md`.
- **The MiniLM cache works.** The first run downloaded and saved it (key `minilm-751bff3...`), and the hash check passed.
- The Rust test job also failed on #32, as its body said it would (`tests/query_plan_rules.rs` expects the vector route); only the gate job is the evidence here.

## Why the job could not have worked before today

- Until #24, any Rust build on the hosted runners failed on the Swift helper (`macos-14`).
- Until b1a1776, the knowledge-worker gate failed on six days of the week: "churn drivers due Thursday" was read as a filter for memories captured on Thursday, and its memory is seeded three days before the run. A CI gate would have been red most days for a reason unrelated to the pull request.
