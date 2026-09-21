# MOD-02 part 1: which LLM calls exist in v1 and which are live

Date: 2026-09-21. Method: every method in `src-tauri/src/inference/mod.rs` that calls `self.complete` or `self.complete_with_control`, classified with `callers.py` (name-based scan of `src-tauri/src`, test code excluded), then the capture-path call sites read by hand. Vision and glasses import are parked (owner decision 2026-09-21), so those rows are listed but not analyzed.

Cost column: worst-case decode seconds if the model emits the full `max_tokens` at 44 tokens per second, the best-case decode speed of the live model (Qwen3-VL-2B) measured on 2026-09-21 (output tokens only, prefill not included, real in-app speed not yet measured).

## Inventory (14 methods)

| Method | Method line in `inference/mod.rs` | max_tokens | Worst-case decode | Class | Called from | Trigger |
|---|---|---|---|---|---|---|
| `extract_structured_memory` | 1348 | 400 (x2 with repair) | 9.1 s (18.1 s with repair) | live | `capture/mod.rs:2721` (capture loop), `capture/mod.rs:335`, parked: `glasses_import.rs` | Capture hot path |
| `summarize_memory_node` | 964 | 90 | 2.0 s | live | `capture/mod.rs:4316` (merge, gated by `allow_llm_summary`) | Capture, when merging records |
| `extract_todos` | 1312 | 200 | 4.5 s | live | `capture/mod.rs:5385` (`maybe_create_tasks_from_memory`), `meeting/mod.rs` x2 | Capture and meetings |
| `review_memory_record` | 1452 | 320 (x2 with repair) | 7.2 s (14.5 s with repair) | live | `memory_review/inference_provider.rs` | Background, pressure gated per its doc comment |
| `answer` | 1033 | 150 | 3.4 s | live | `mcp/mod.rs`, `context_runtime/composer.rs`, `evals/memory_quality.rs` | User question, MCP |
| `answer_screen_guide` | 1054 | caller-supplied | n/a | live | `ipc/commands/screen_guide.rs` | User action |
| `expand_search_query` | 1098 | 80 | 1.8 s | live | `search/hybrid.rs` | Search time |
| `refine_query_plan` | 1202 | 80 | 1.8 s | live | `context_runtime/query_plan.rs` | Search time |
| `synthesize_memory_card` | 1154 | 180 | 4.1 s | live | `search/memory_cards.rs` | Search results |
| `extract_meeting_breakdown` | 1551 | 360 | 8.2 s | live | `meeting/mod.rs` | Meetings |
| `generate_daily_briefing` | 1643 | 160 | 3.6 s | live | `main.rs`, `ipc/commands/hermes_agent.rs` | Startup and agent |
| `synthesize_vision_description` | 1249 | 120 | 2.7 s | live, parked | `inference/image_semantics.rs` | Photo import only |
| `summarize_memory_detail` | 1124 | 150 | 3.4 s | dead | none | Never |
| `generate_daily_summary` | 1703 | 350 | 7.9 s | dead | none (the IPC command `generate_daily_summary_for_date` in `ipc/commands/stats.rs:718` is a separate function; not analyzed) | Never |

Two methods are dead by this scan; confirm by hand before deleting because the scan is name-based (names such as `answer` are generic and can match unrelated calls).

## What the capture-path read shows

- `capture/mod.rs:2721` calls `extract_structured_memory` inside `run_capture_loop` while holding `state.model_pipeline_lock`. The comment above it says capture pauses and heavy model work is serialized. If that is accurate, each memory that reaches this call blocks capture for prefill plus decode (roughly 2.6 s of prefill plus up to 9.1 s of decode at the live model's benchmark speed).
- `capture/mod.rs:2691` loads the engine lazily (`ensure_inference_engine`) if it is not loaded yet.
- After extraction the code already runs `normalize_structured_memory_json`, `normalize_activity_type`, a one-pass repair, `validate_structured_memory_extraction` (grounding confidence and issues) and a browser-semantics seed fallback. So format and grounding validation exist; what is missing is a measurement of how often each one fires.

## Unknowns that MOD-02 tracing would answer

1. How many `extract_structured_memory` calls per hour reach the model, and what fraction are skipped by earlier gates?
2. Real prefill and decode seconds per call inside the app (not the CLI best case).
3. How often the repair pass runs, and how often `activity_type` ends as `unknown`.
4. How often the prompt is truncated (LRN-01 finding F3).
5. Who passes `allow_llm_summary = true` to `merge_memory_records_with_policy`.

## Update: the 400-token cap (finding F10)

On the live model a short synthetic capture produced a 449 to 456 token answer, over the 400 cap. A truncated answer has no closing brace, `extract_json_object` returns `None`, and `extract_structured_memory` returns `None` through the `?` at the call site without trying the repair pass. Traces now record `max_tokens`, so cap hits are `output_tokens >= max_tokens` per task.
