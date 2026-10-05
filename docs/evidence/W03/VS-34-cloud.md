# VS-34 the retrieval gate runs in GitHub Actions (cloud)

## What runs

`.github/workflows/retrieval-gate.yml`, job "Retrieval gate (synthetic corpora)", on pull requests into main and pushes to main that touch `src-tauri/**`, `scripts/demo/**`, `scripts/audit/retrieval_check.py`, the `Makefile`, or the workflow itself.

1. Same runner and build setup as `test.yml`: `macos-26` (the Swift speech helper needs the macOS 26 SDK), stable Rust, the cargo cache, `protobuf`, and an empty `dist`.
2. **The embedder cache.** The pinned MiniLM (`Xenova/all-MiniLM-L6-v2` at revision 751bff3, the same URL as `EMBEDDING_MODEL_DOWNLOAD_URL` in `src-tauri/src/inference/model_config.rs`) is restored from the Actions cache, or downloaded once. Both files are then checked against the sha256 pins in `model_config.rs` before anything runs. The models folder is linked where the seed script and the embedder look for it.
3. `make qa-retrieval-check` for knowledge-worker, then for office-PM (the second runs even when the first fails). Each run seeds a fresh synthetic profile from `scripts/demo/` under `TZ=America/Denver` and compares the report with `scripts/demo/retrieval-reference/`.
4. Both `check.md` reports are added to the job summary.

The job fails when any path's Recall@5 drops more than 0.05, or when a query a path found in its top ten becomes a miss (VS-04's rule in `scripts/audit/retrieval_check.py`). No real data is involved: the runner has no FNDR profile, and the only download is the pinned model.

## Runs

Two runs are needed for the done-when: one on this branch (PR #31), which should pass, and one on a throwaway negative-control branch that takes the keyword route out of planning, which should fail. Both are recorded in the next commit once they have run.

## Why the job could not have worked before today

- Until #24, any Rust build on the hosted runners failed on the Swift helper (`macos-14`).
- Until b1a1776, the knowledge-worker gate failed on six days of the week: "churn drivers due Thursday" was read as a filter for memories captured on Thursday, and its memory is seeded three days before the run. A CI gate would have been red most days for a reason unrelated to the pull request.
