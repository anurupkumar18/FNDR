# LLM task catalog

Every job that calls the local text model, with the task id it stamps on its trace line in `<app data dir>/llm_traces.jsonl` (see `src-tauri/src/telemetry/llm_trace.rs`). A trace line with `output_tokens >= max_tokens` means the answer was cut off (see finding F10). Task ids and the prompt version (`LLM_PROMPT_VERSION`, currently `v1`) are set in `src-tauri/src/inference/mod.rs`; bump the version when a prompt changes so eval and trace rows stay comparable.

The live text model is Qwen3-VL-2B Q4_K_M (`models.rs:181` hard-wires it). Facts below come from reading the code on 2026-09-21. The column "Runs per day" is left for numbers taken from real traces after a day of use; nothing in it is an estimate.

Run this after a day of use to fill it:

```bash
python3 - <<'EOF'
import json, collections, pathlib
p = pathlib.Path.home() / "Library/Application Support/com.fndr.app/llm_traces.jsonl"
rows = [json.loads(l) for l in p.read_text().splitlines() if l.strip()]
by = collections.defaultdict(list)
for r in rows: by[r["task"]].append(r)
for task, rs in sorted(by.items()):
    lat = sorted(r["latency_ms"] for r in rs)
    print(f"{task:26} calls={len(rs):5} p50_ms={lat[len(lat)//2]:6} p95_ms={lat[int(len(lat)*0.95)-1 if len(lat)>1 else 0]:6} "
          f"prompt_tok_avg={sum(r['prompt_tokens'] for r in rs)//len(rs):5} out_tok_avg={sum(r['output_tokens'] for r in rs)//len(rs):4} "
          f"cap_hits={sum(r['output_tokens'] >= r['max_tokens'] for r in rs)}")
EOF
```

## Tasks

| Task id | Engine method | Line | max_tokens | Input cap | Expected output | Validation and fallback | Class | Triggered by | Runs per day |
|---|---|---|---|---|---|---|---|---|---|
| `memory_extraction` | `extract_structured_memory` | `inference/mod.rs:1348` | 400 | OCR text up to 4,000 chars | JSON with 24 fields (schema in the system prompt) | `normalize_structured_memory_json`, `normalize_activity_type`, then `validate_structured_memory_extraction` (grounding) in the capture loop; browser-semantics seed fills gaps | live | Capture loop (`capture/mod.rs:2721`, holds `model_pipeline_lock`), visual-only fallback (`capture/mod.rs:335`) | from traces |
| `memory_extraction_repair` | same method, second call | `inference/mod.rs:1348` | 400 | the invalid JSON candidate | corrected JSON | one repair pass; `None` if it still fails to parse | live | Only when the first parse fails | from traces |
| `memory_snippet` | `summarize_memory_node` | `inference/mod.rs:964` | 90 | OCR up to `MAX_OCR_SUMMARY_CHARS` | 1 to 2 sentences, 16 to 34 words | empty string on failure | live | Merging memory records (`capture/mod.rs:4316`, only when `allow_llm_summary`) | from traces |
| `todo_extraction` | `extract_todos` | `inference/mod.rs:1312` | 200 | memory text up to 2,000 chars | lines of todos or exactly `NONE` | empty string on failure | live | `capture/mod.rs:5385` (`maybe_create_tasks_from_memory`), meetings | from traces |
| `memory_review` | `review_memory_record` | `inference/mod.rs:1452` | 320 | clean text up to 4,000 chars, 12 same-day candidates | JSON (`MemoryReviewPromptOutput`) | `None` if unparseable after one repair; caller records `review_failed` | live | Background review worker (`memory_review/inference_provider.rs`), pressure gated per its doc comment | from traces |
| `memory_review_repair` | same method, second call | `inference/mod.rs:1452` | 320 | the invalid JSON candidate | corrected JSON | as above | live | Only when the first parse fails | from traces |
| `query_expansion` | `expand_search_query` | `inference/mod.rs:1098` | 80 | query | JSON array of 5 to 8 lowercase terms | `parse_expansion_terms`; falls back to the lowercased query | live | Search (`search/hybrid.rs`) | from traces |
| `query_plan` | `refine_query_plan` | `inference/mod.rs:1202` | 80 | query | tiny JSON object, optional fields | run under a timeout; `None` on timeout or bad output | live | Context runtime (`context_runtime/query_plan.rs`) | from traces |
| `card_synthesis` | `synthesize_memory_card` | `inference/mod.rs:1154` | 180 | up to 6 snippets | JSON with title, summary, action, context | `None` if unparseable | live | Search results (`search/memory_cards.rs`) | from traces |
| `answer` | `answer` | `inference/mod.rs:1033` | 150 | context up to 1,000 chars | free text | empty string on failure | live | MCP, context composer, evals | from traces |
| `screen_guide` | `answer_screen_guide` | `inference/mod.rs:1054` | 96 | `SCREEN_GUIDE_SYSTEM_PROMPT` plus screen text | short guidance text | cancellable with a deadline; empty on timeout | live | Screen Guide IPC (`ipc/commands/screen_guide.rs`) | from traces |
| `meeting_breakdown` | `extract_meeting_breakdown` | `inference/mod.rs:1551` | 360 | transcript up to 7,000 chars | JSON (`MeetingTaskBreakdownDraft`) | `None` if unparseable; text is whitespace-normalized | live | Meetings (`meeting/mod.rs`) | from traces |
| `daily_briefing` | `generate_daily_briefing` | `inference/mod.rs:1643` | 160 | cards up to 900 chars | free text | empty string on failure | live | Startup and the Hermes agent command | from traces |
| `vision_description` | `synthesize_vision_description` | `inference/mod.rs:1249` | 120 | scene block from the vision path | free text | parked (optional photo import) | live, parked | `inference/image_semantics.rs` | not planned |
| `memory_detail` | `summarize_memory_detail` | `inference/mod.rs:1124` | 150 | | | | dead by scan | none | never |
| `daily_summary` | `generate_daily_summary` | `inference/mod.rs:1703` | 350 | | | | dead by scan | none (the IPC command `generate_daily_summary_for_date` is a different function) | never |

Two tasks are dead by a name-based caller scan (`summarize_memory_detail`, `generate_daily_summary`). They still carry a task id so a trace would show any call; delete them only after checking by hand.

## What is not traced yet

- Calls that do not go through `InferenceEngine::complete` (the vision runtime in `image_semantics.rs` and `vlm.rs` have their own paths). Parked with the vision work.
- The `validator` field is `not_validated` for every line. MOD-03 (prompt registry and validators) is where each task's validator result gets recorded.
