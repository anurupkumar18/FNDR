# LRN-01 evidence: what runs when FNDR calls the LLM (Stage 1)

Owner: Anurup. Executor: Anurup with Claude. Date started: 2026-09-21. Status: in progress.

All numbers below were measured on the reference machine. No captured user content was used: every prompt is synthetic.

## Setup

| Item | Value |
|---|---|
| Machine | Apple M1, 8 GB (`sysctl hw.memsize` = 8589934592) |
| Engine | llama.cpp 0.4.1 (build 10964, commit b29c606e2), Homebrew, Metal + BLAS backends |
| Model A (measured in CP1 to CP4 and CP6) | `Llama-3.2-1B-Instruct-Q4_K_M.gguf`, 807,694,464 bytes, 1.24 B parameters. **Legacy file on disk: no code path loads it** (see the correction below) |
| Model B (the app's live model) | `Qwen3VL-2B-Instruct-Q4_K_M.gguf`, 1,107,409,952 bytes. Serves as both the text engine and the optional vision model |
| Machine state | FNDR closed. Swap already 3.3 GB of 4 GB used before starting (`vm.swapusage`). Free memory 54 percent. |
| Metal working-set cap | `recommendedMaxWorkingSetSize` = 5726.63 MB (reported by ggml at startup) |

## Correction (found while building MOD-02, 2026-09-21): Stage 1 measured a model the app does not run

`inference_preferred_model_id` (`models.rs:181`) ignores its arguments and always returns `qwen3-vl-2b`, the model catalog has one entry, and both `InferenceEngine::new` call sites use that id. The first real trace line confirmed it (`"model_id":"qwen3-vl-2b"`). Llama-3.2-1B appears only in the old-models cleanup list (`model_config.rs`). So the app's **text engine is Qwen3-VL-2B Q4_K_M** (28 layers, 8 KV heads, head dim 128, 1,056 MiB of weights).

What still holds: the mechanisms and formulas (tokens, quantization bits, the bandwidth ceiling, the KV formula) and the method of predicting speed from bytes. The numbers were re-measured on the live model on 2026-09-21 with a quiet machine:

| Quantity (Qwen3-VL-2B Q4_K_M, 1.03 GiB, 1.72 B parameters) | Predicted from the formulas | Measured |
|---|---|---|
| Decode ceiling (68.25 GB/s over 1.107 GB) | 61.6 tokens/sec | n/a |
| Decode `tg128` | about 46 (75 percent of ceiling) | **44.14 +/- 0.64** (71.6 percent of ceiling) |
| Prefill `pp512` | not predicted | 581.52 +/- 1.03 |
| Peak resident set size / footprint | not predicted | 1,244,839,936 bytes (1,187 MiB) / 161,124,544 bytes (154 MiB) |
| KV cache at 2,048 context | 224 MiB | 224.00 MiB (28 layers, 8 KV heads, head dim 128) |
| Decode at depth 0 / 1,024 / 2,048 / 4,096 | slowdown near 17 percent at 2,048 | 45.16 / 41.38 / 38.25 / 33.20, so **15.3 percent** slower at 2,048 and 26.5 percent at 4,096 |

Consequence: the extraction estimate from CP3 (prefill 1.7 s, decode 5.5 s) becomes about 2.6 s prefill (1,500 tokens at 581 per second) plus about 7.9 s decode (350 tokens at 44 per second) at best, roughly 10.5 s per call. Token counts under the Qwen tokenizer: the extraction system prompt is 357 tokens (same as Llama by coincidence); the three CP1 strings are 14, 28 and 33 tokens.

## F3 and F10: measured on the real extraction path (2026-09-21)

The real `extract_structured_memory` was run on eight synthetic captures (fixture `src-tauri/tests/fixtures/extraction_cases.json`: terminal test failure, Rust code, web article, chat thread, spreadsheet, email inbox, planning doc, sparse page) on the live model with greedy decoding. Latencies are not quoted because the dev app was running at the same time.

| Configuration (`n_ctx` / output cap) | Parsed | What happened |
|---|---|---|
| 2,048 / 400 (before) | 2 of 8 | Six answers were cut off at 400 tokens and discarded with no repair attempt; the spreadsheet prompt was truncated to 1,648 tokens |
| 2,048 / 900 (diagnostic) | 7 of 8 | Natural answers were 316, 351, 416, 426, 465, 507 and 573 tokens; the spreadsheet prompt was truncated to 1,148 tokens and its answer ran to the 900 cap |
| 4,096 / 640 (after) | 8 of 8 | Longest answer 573 tokens; spreadsheet prompt 1,694 tokens fits; no case hit the cap |

Cost of the fix: the KV cache for the text engine grows from 224 MiB to 448 MiB. Re-run with `cargo test --lib extraction_fits_default_token_budget -- --ignored --nocapture` (the process aborts at exit afterwards, finding F9; read the `test ... ok` line). Unit tests `extraction_prompt_budget_covers_dense_ocr` and `extraction_output_cap_exceeds_measured_answer_lengths` pin the arithmetic.

Real captures (the running dev app's `llm_traces.jsonl`, 15 extraction calls plus one repair and one review, mostly a chat-window capture with little text; the app rebuilds on source changes, so the file mixes caps of 400, 900 and 640 from the diagnostic edits): answers ran 200 to 408 tokens. Under the original 400 cap, 1 of 6 calls hit the cap (35.8 s, nothing stored from the LLM) and one call needed the repair pass; one 408 token answer would also have been cut. Text-poor captures understate the problem, so 2 of 15 is a lower bound. In-app decode ran about 25 to 33 tokens per second (200 token answers in 6.7 to 9.1 s), below the 44 measured by the CLI benchmark, consistent with the memory pressure the app itself reported (`host_memory_high`).

Not yet known: the cap-hit rate on text-rich real captures, and how much extra capture stall the longer answers cost. Before the fix each failed call already spent 400 tokens of decode for nothing.

## Checkpoint results (prediction, then measurement)

### CP1: tokens are not words (`llama-tokenize`, synthetic strings)

| String | Chars | Predicted tokens | Measured | Chars per token |
|---|---|---|---|---|
| A, prose (12 words) | 81 | 12 | 14 | 5.79 |
| B, code line | 92 | 30 | 26 | 3.54 |
| C, URL with hex id (78 chars, first quoted as 77) | 78 | 35 | 31 | 2.52 |

Ranking of cost per character was predicted correctly. Numbers split into 3-digit chunks (`1872` becomes `187|2`); hex ids fragment worst.

Extraction system prompt (`inference/mod.rs:1337`): 1,434 characters, 357 tokens (4.0 chars per token). An empty 24-field JSON answer costs 101 tokens compact or 171 tokens indented.

### CP2: what is inside the GGUF (`gguf_breakdown.py`)

| Storage type | MiB | Share |
|---|---|---|
| q4_K (about 4.5 bits per weight) | 445.5 | 58.4 percent |
| q6_K (about 6.56 bits per weight) | 317.1 | 41.6 percent |
| f32 (norms) | 0.3 | 0.0 percent |

- 147 tensors, 762.8 MiB of tensor data. Average is about 5.2 bits per weight (predicted 6.5, which used 1.0 B parameters instead of 1.24 B).
- Largest tensor: `token_embd.weight`, 205.5 MiB, q6_K, 27 percent of the file.
- There is no separate `output.weight`: the vocabulary table is tied, so it is read at both ends of every token.
- In the "M" mix, 8 of 16 `ffn_down` tensors and 8 of 16 `attn_v` tensors are q6_K; the rest are q4_K.

### CP3: speed against the memory-bandwidth ceiling (`llama-bench -p 512 -n 128 -r 3`)

| Test | Result |
|---|---|
| `pp512` (prefill) | 874.17 +/- 2.62 tokens/sec |
| `tg128` (decode) | 64.09 +/- 1.09 tokens/sec |
| Ceiling for decode | 68.25 GB/s divided by 0.80 GB = 85.3 tokens/sec, so measured is 75.1 percent of the ceiling |
| Peak resident set size | 898,416,640 bytes (857 MiB) |
| Peak memory footprint | 129,240,640 bytes (123 MiB) |
| Swaps | 0 |

Predictions: decode bucket 5 to 20 (measured 64.1, wrong); fraction of ceiling 25 to 50 percent (measured 75 percent, wrong). Extraction call estimate with measured speeds (1,500 prompt tokens, 350 output tokens): prefill 1.7 s, decode 5.5 s, decode about 3.2x longer (correct).

### CP4: KV cache and context depth

Formula: `2 x layers x kv_heads x head_dim x context x 2 bytes`. Llama-3.2-1B: 16 layers, 8 KV heads, head dim 64. At 2,048 context the log reports 64.00 MiB (matches the formula); at 4,096 it is 128 MiB.

| Depth | Predicted from bytes (763 MiB weights + KV) | Measured `tg64` | Slowdown |
|---|---|---|---|
| 0 | n/a | 64.76 +/- 0.69 | n/a |
| 1,024 | n/a | 58.17 +/- 8.05 (noisy, spread overlaps the next row) | n/a |
| 2,048 | 59.7 | 59.27 +/- 1.61 | 8.5 percent |
| 4,096 | 55.4 | 55.53 +/- 1.13 | 14.3 percent |

Prediction recorded before the run: more than 30 percent slower at 2,048 (wrong).

### CP5: KV cache of the vision model at FNDR's context

Qwen3-VL-2B: 28 layers, 8 KV heads, head dim 128. At 512 context the log reports 56.00 MiB. FNDR requests `context_size * 8` = 32,768 cells (`image_semantics.rs:1204`, `QWEN_CONTEXT_SIZE` = 4,096).

| Quantity | Predicted | Measured (llama.cpp, text only, no image, no projector) |
|---|---|---|
| KV cache at 32,768 cells | 3,584 MiB | 3,584.00 MiB |
| Weights + projector + KV | 5,073 MiB (under the cap) | not measured as a total |
| Peak resident set size | n/a | 4,451,745,792 bytes (4,245 MiB) |
| Peak memory footprint | n/a | 3,964,841,664 bytes (3,781 MiB) |

The KV cache is about 95 percent of the footprint. Not yet measured inside the FNDR app.

### CP6: sampler settings on FNDR's extraction prompt (`sampling_lab.py`, synthetic failing-test capture, 3 runs per setting)

| Setting | Distinct outputs (of 3) | Parseable JSON (of 3) | Avg chars |
|---|---|---|---|
| S1 greedy + repeat penalty 1.1 (FNDR text path) | 1 | 3 | 887 |
| S2 greedy, no penalty | 1 | 3 | 576 |
| S3 temp 0.7, top-p 0.9, top-k 40 | 3 | 3 | 573 |
| S4 temp 1.5, no truncation | 3 | 1 | 731 |

Predictions recorded before the run: S1 distinct 1 (correct), S3 distinct 3 (correct), S4 distinct 3 (correct), S1 most valid (tied with S2 and S3).

Greedy output for the same input, S1 versus S2 (one prompt, so an observation, not a conclusion):

| Field | S1 (penalty 1.1) | S2 (no penalty) |
|---|---|---|
| Output tokens | 234 | 170 |
| `outcome` | `completed` (the test failed) | `failed` |
| `errors` | empty | the assertion failure |
| `memory_context` | an escaped copy of the rest of the schema | the failing test name |
| `activity_type` | `testing_agent_output` (not in the enum) | `testing` (not in the enum) |

Sampling noise: only 3 runs per setting were used. In two single-run validation checks by Claude, S3 (temperature 0.7) produced unparseable JSON both times, while the owner's 3-run pass had 3 of 3 valid, so S3's true valid rate is uncertain. The greedy rows (S1, S2) are deterministic and are not affected.

Caveats: the lab's parseability check does not validate the schema, and both outputs used an activity type outside the enum (FNDR's `normalize_activity_type` would map `testing` to `testing_workflow` and `testing_agent_output` to `unknown`). The CLI feeds the last 64 prompt tokens to the penalty window, while FNDR's Rust loop (`inference/mod.rs:1904`) only feeds generated tokens, so the lab approximates but does not replicate FNDR's window.

## Findings ledger

| # | Finding | Status | Where |
|---|---|---|---|
| F1 | CORRECTED: the 90 s idle-unload stub lives in `inference/model_worker.rs`, whose `QwenVlmWorker` has no caller anywhere in the crate (only `pub mod model_worker;` references it), so it is dead code and not the cause of any resident memory. The real state: the text engine (Llama 1B, 763 MiB weights, 64 MiB KV) loads once at startup and is `Box::leak`ed, and the optional vision runtime is a `OnceLock` singleton that is never released. No unload path exists for either | Code-read; resident cost of the text engine not yet measured in the app | `inference/model_worker.rs:85`, `inference/mod.rs:28`, `inference/mod.rs:875`, `inference/image_semantics.rs:1147` |
| F2 | `QWEN_TEMPERATURE` (0.1) and `QWEN_TOP_P` (0.8) are declared but never read; the live text sampler is repetition penalty 1.1 then greedy, the image path uses penalty 1.05 then greedy, only `vlm.rs` samples with temperature 0.7 and top-p 0.9 | Confirmed in code | `inference/model_config.rs:73`, `inference/mod.rs:1892`, `inference/image_semantics.rs:1300`, `inference/vlm.rs:354` |
| F3 | **MEASURED and fixed on branch `fix/extraction-token-budget`.** Prompt and output share one window (`n_ctx` 2,048), and an overflow is cut from the front of the prompt, where the rules live. A synthetic 1,718 character spreadsheet capture needs a 1,694 token prompt but the old budget was 2,048 minus the 400 cap, 1,648, so its prompt was truncated. Raising only the output cap would have made this worse (a 900 cap left a 1,148 token budget) | Measured on the real code path with the live model; fix is `TEXT_ENGINE_DEFAULT_N_CTX` 4,096 | `inference/mod.rs:900`, `model_config.rs` |
| F4 | CORRECTED: the 32,768-cell context is in `MtmdVlmRuntime` (`image_semantics.rs:1204`), used by the optional glasses and photo import path. It needs an mmproj file the app never downloads (README calls it optional setup; `capture/mod.rs:5751`), so it is not on the baseline screenshot capture path. `MULTIMODAL_MODEL_RAM_GB = 3.5` (`model_config.rs:10`) also leaves out the 3.5 GiB KV cache | Measured with llama.cpp; impact limited to photo import | `inference/image_semantics.rs:1204`, `inference/model_config.rs:10` |
| F5 | DOWNGRADED: the repeat-penalty concern came from Llama and does not replicate on the live model. On Qwen3-VL-2B with FNDR's extraction prompt, greedy with penalty 1.1 and greedy without produced equivalent answers (456 versus 449 tokens, same activity type, topic, and error), so the penalty costs about 7 tokens and shows no quality harm on this sample (n=1, CLI window differs from FNDR's) | Not a finding on the live model; keep only as a gold-set A/B candidate | `inference/mod.rs:1893` |
| F6 | CORRECTED: my lab's parse check accepts any JSON, but FNDR is not exposed the same way. `normalize_activity_type` (`inference/mod.rs:668`) maps aliases (`testing` becomes `testing_workflow`) and turns any unrecognized value into `unknown`, and a one-pass repair exists (`inference/mod.rs:1396`). Residual question: how often does the model produce a value that ends up as `unknown`, losing information? | Code-read; the rate is unmeasured | `inference/mod.rs:668`, `inference/mod.rs:1388` |
| F7 | This machine has a half-installed vision model: the 2B weights plus a **4B** projector (`mmproj-Qwen3VL-4B-Instruct-Q8_0.gguf`). `resolve_qwen3_vl_2b_mmproj` (`models.rs:111`) accepts only 2B-named files, so it returns nothing and the vision runtime reports the projector missing. Likely left over from the earlier 4B tier (`CLEANUP_OLD_MODEL_DIRS`) | Confirmed by `ls` and code-read | `models.rs:104`, `models.rs:111` |
| F8 | Stage 1 numbers (CP1, CP3, CP4, CP6) were taken on Llama-3.2-1B, a legacy file the app never loads; the live text model is Qwen3-VL-2B. `inference_preferred_model_id` ignores its arguments, so the model is hard-wired by design and pinned by the test `inference_preferred_model_id_always_returns_qwen3_vl_2b`; the error was my assumption, not the code | Confirmed by code, tests and the first trace line | `models.rs:181`, `models.rs:47` |
| F9 | Any process that has loaded a model aborts at exit: the model is `Box::leak`ed, and ggml's Metal teardown asserts `[rsets->data count] == 0` (SIGABRT after the tests pass, reproduced twice). The app's exit handler (`main.rs:865`) does no model teardown, so a crash report at quit is plausible but not verified in the app | Reproduced in the test binary; app behavior unverified | `inference/mod.rs:875`, `main.rs:865` |
| F10 | **MEASURED and fixed on branch `fix/extraction-token-budget`.** The 400 token extraction cap sat below the live model's natural answer (316 to 573 tokens). A cut-off answer has no closing brace, `extract_json_object` returns `None`, and `extract_structured_memory` returns `None` through the `?` without trying the repair pass. Six of eight synthetic captures were discarded this way, each costing a full 400 token generation while capture waits. Fix: `EXTRACTION_MAX_OUTPUT_TOKENS` 640 with a 4,096 token window | Measured on the real code path; the fix is verified on eight synthetic captures, not yet on real captures | `inference/mod.rs:1348`, `inference/mod.rs:428`, `model_config.rs` |

## Reproduce

Scripts are in `docs/superpowers/plans/assets/2026-09-21/ws2/lab-stage1/` (`tokcount.sh`, `gguf_breakdown.py`, `sampling_lab.py`, and `sysmsg.txt`, a verbatim copy of the extraction system prompt). The benchmark commands are quoted in the checkpoint sections above. Install: `brew install llama.cpp`.

## Open

- Exit check answers (explain in your own words, no slides): pending from the owner.
- Owner decision 2026-09-21: stop after Stage 1 and start Stage 2 (prompts, structured output, validators) next session.
- Owner chose build-first for Stage 2. Recon then showed F1 and F4 mostly do not touch the baseline path (see the corrected rows), so the next step is an inventory of which LLM calls run in the baseline app (MOD-02) before any in-app measurement.
- MOD-02 Task 1 is implemented on branch `feat/mod-02-llm-traces` (uncommitted): trace module, token counts, per-call traces, task labels on all 15 call sites, and `docs/product/llm-task-catalog.md`. Remaining for the ticket: a real app run to see labeled lines in `llm_traces.jsonl` (owner).
- F3 and F10 are fixed on `fix/extraction-token-budget` (see the measured table). Follow-ups: confirm the cap-hit rate on real captures from traces, and consider shrinking the 24-field schema, which would cut both output tokens and decode time.
- Not done yet: triage which findings become tickets (nothing is ticketed yet).
