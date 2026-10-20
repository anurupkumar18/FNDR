# Laya vs tev1 on FNDR decisions

Status: neither is adopted. Both stay out of the product path. The Python Laya bridge, its sidecar and `LayaDecider` were deleted (the `Decider` trait and `cascade` stay in `src-tauri/src/decision/mod.rs`). The older spike is in `docs/evidence/W03/laya-vs-fake-decider.md`.

## Setup

- Ollama 0.40.4 (the installed app is 0.32.14, too old for either model), run from a scratch binary on port 11435 with its own model directory. The installed app was not touched.
- Models: `laya` (421M, 846 MB), `tev1:0.8b` (1.5 GB), `tev1:4b` (4.4 GB). All called through `/v1/systemone`, one choice question per call, warm.
- Machine: 16 GB Apple Silicon with about 21 GB of swap already in use. This is not the 8 GB M1 reference profile in `docs/team/hardware.md`.
- Harness and raw per-case results: `docs/evidence/W04/systemone-bakeoff/` (`run.py` phrasing A, `runb.py` phrasing B).

## Test sets

| Suite | n | Source | Majority baseline |
|---|---|---|---|
| screens: does this screen hold a to-do | 30 | `src-tauri/tests/fixtures/task_screens.json` (synthetic, the task suggestion gate) | 63% |
| owner_tasks: would the owner keep this task | 22 | `~/Library/Application Support/fndr/tasks.json`, label = not dismissed | 73% |
| gate: does this tool call match the request | 25 | written for this test from the real tool names in `agent/tools.rs`, 12 allow and 13 block including page-text injection | 48% |

The live LanceDB tables could not be read from Python (the file format is newer than the installed `lance`; the read panicked), so no capture text from the live vault was used. `llm_traces.jsonl` stores no prompts.

## Results (accuracy, phrasing A / phrasing B)

| | screens | owner_tasks | gate | warm p50 | memory loaded (`ollama ps`) |
|---|---|---|---|---|---|
| laya | 0.63 / 0.63 | 0.64 / 0.68 | 0.88 / 0.72 | 35 to 50 ms | 1.1 GB |
| tev1:0.8b | 0.90 / 0.80 | 0.77 / 0.68 | 0.88 / 0.68 | 65 to 95 ms | 4.2 GB |
| tev1:4b | 0.77 / 0.83 | 0.82 / 0.77 | 1.00 / 0.96 | 475 to 683 ms | 10 GB |

- Laya on screens answers `no_task` for 28 of 30 screens, so it matches the majority baseline and finds none of the 11 real tasks. On the gate it allowed 3 injected or mismatched calls under phrasing A and 20 of 25 under phrasing B. Its confidence is flat (0.13 mean on correct gate answers, 0.06 on wrong ones), so a threshold cannot rescue it.
- tev1:4b is the only one above baseline on every suite and the only one whose confidence separates right from wrong (gate: every answer at 0.5 or above was correct, 14 of 25 covered). It still over-suggests tasks (7 false tasks of 19 non-task screens under phrasing A).
- tev1:0.8b is the best on screens under phrasing A and falls to 0.68 on two suites under phrasing B. Prompt phrasing moves it by up to 20 points.
- Every number moves 10 to 20 points between two phrasings, and n is 22 to 30. The gaps between tev1 sizes are inside that noise; the gap between Laya and a majority-class answer on screens is not.

## Why neither ships

1. tev1:4b needs 10 GB resident. The reference machine has 8 GB and FNDR already runs capture, OCR and embeddings. tev1:0.8b needs 4.2 GB for a 63.5% public-benchmark model. Laya fits (1.1 GB) and does not beat a constant answer here.
2. `/v1/systemone` exists only in Ollama 0.35 and later (0.40 for Laya). FNDR runs its own GGUF engine; adopting these means a new runtime dependency, and a newer one than the user's installed 0.32.
3. The tev1 weights show no license on the Ollama page. Check before bundling.
4. The incumbent was not measured: `task_suggestion_eval` was not run, so no claim is made that tev1:4b beats what ships today.

## Open question

Does tev1:4b's 0.5 confidence cut hold on real captured screens rather than 30 synthetic ones, and is a 4B model worth 10 GB when the same question can go to the model FNDR already loads?

## Rerun

```bash
OLLAMA_HOST=127.0.0.1:11435 ollama serve &   # Ollama 0.40 or later, separate port and OLLAMA_MODELS dir
for m in laya tev1:0.8b tev1:4b; do OLLAMA_HOST=127.0.0.1:11435 ollama pull $m; done
cd docs/evidence/W04/systemone-bakeoff
for m in laya tev1:0.8b tev1:4b; do python3 run.py $m; python3 runb.py $m; done
```

`owner_tasks` reads `~/Library/Application Support/fndr/tasks.json`, so it only runs on the owner's machine; the per-case result files omit that suite's task text. Pulling all three models takes about 6.8 GB of disk.
