# Proactive signals on the owner's vault: 0 stuck episodes, 1 thread with new work

The owner's active memory table held 20 memories on 2026-10-09, captured Oct 6 to Oct 8 across 7 apps. No ten-minute stretch in it has four captures of one thing, so the stuck detector had nothing to find, and this run says nothing about its precision on real work.

## How it was run

Read-only, on a copy: `cp -R ~/Library/Application\ Support/com.fndr.app/lancedb <scratch>/vault-copy/`, then

```
FNDR_SIGNALS_EVAL_DIR=<scratch>/vault-copy cargo test --lib proactive_signals::eval -- --include-ignored --nocapture
```

through `scripts/dev/test-clean.sh` (HEAD plus the signal files), because another lane's uncommitted work did not build in the checkout. The test (`src-tauri/src/proactive_signals/eval.rs`) refuses the real profile and prints counts and memory ids; screen text only with `FNDR_SIGNALS_EVAL_SHOW_TEXT=1`, which was used for the spot check below and is not copied here. The blocklist was `Config::default()`, not the owner's.

- **Stuck:** every capture is replayed as "now" with the 30 minutes before it; an episode counts once per issue per day.
- **What changed:** each thread is marked seen at its first capture and compared with everything stored after it.

## Results

| Measure | Value |
| --- | --- |
| Memories / admitted to surfaces | 20 / 19 |
| Minutes between consecutive captures | 4, 86, 3, 0, 18, 0, 0, 0, 1, 3, 0, 0, 0, 865, 232, 132, 4, 28, 1347 |
| Captures with an error line on screen | 0 |
| Stuck episodes (error / page) | 0 (0 / 0) |
| Episodes with an older match | 0 |
| Threads / with new memories after the marker / worth a nudge | 15 / 1 / 0 |

## Spot check

There are not five detections to check: the vault gives one digest and no episodes.

1. **Miss, fixed.** The one thread with new memories (ChatGPT, 4 new captures) first counted 29 new tasks and would have sent a nudge. All 29 were suggestions FNDR drew from those captures (`source_app` "Memory: ..."), 1 of them completed, out of 442 tasks in the store. A suggestion is not work the person took on, so the digest now leaves suggestions out, the same rule `tasks::suggest::open_commitments` uses (commit 45344f3, with a test). After the fix the digest is 4 new memories, 0 pages, 0 files, 0 tasks, 0 commits, and no nudge, which matches what those captures hold.
2. **Hit (correct negative).** The longest burst, 6 captures within 4 minutes, is under ten minutes and has no error line; no episode is right.
3. **Hit (correct negative).** The other 14 threads have no capture after their first, and their digests are empty.

## What this does not show

- Precision and recall of the stuck detector on real work. The synthetic fixtures in `stuck.rs` (error kept across moving line numbers, short error, error gone from screen, error glimpsed between other work, still page versus page being read, blocklisted app) are the only evidence.
- Whether an older match from hybrid search is a useful "what you did then". `past_match_for` keeps the best strong match from more than an hour before the episode; it was not exercised here.
- Whether a "Been on this a while?" toast helps or annoys. The once-per-issue-per-day limit and the off switch (`[proactive_signals] stuck = false`) are the guards.

Open question: does the owner's vault normally hold 20 memories, or was it cleared before this run? A rerun on a vault with a few days of steady capture is what would answer the precision question.
