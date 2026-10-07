# VS-31: memory review parse failures

Counts only; computed from `llm_traces.jsonl` on the owner profile (no memory text read or copied).

| Task | Calls recorded | Calls that stopped at the token cap | Cap |
|---|---:|---:|---:|
| memory_review | 78 | 19 (24%) | 320 |
| memory_extraction | 212 | 13 (6%) | 400, 640, 900 |

Cause for the `review_memory_record returned no parseable JSON` log line: a quarter of review calls hit the 320 token cap, so the JSON is cut off, `extract_json_object` finds no balanced object, and the repair retry (which only runs when an object is found but invalid) never starts.

Change: `MEMORY_REVIEW_MAX_TOKENS` is 512 for the review and its repair call (`src-tauri/src/inference/mod.rs`).

Not yet verified live: a fresh journey run must show zero parse failures in the metrics dump and trace token counts below the cap. That check is queued behind the owner session. The structured-memory JSON parse error from the handoff is a different task (`memory_extraction`, 6% at cap) and is left for the same live check.
