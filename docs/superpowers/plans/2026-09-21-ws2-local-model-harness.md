# WS2 Local Model Harness and Learning Path Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Part 2 (the learning path) is run interactively by the owner with Claude, not by a subagent.

**Goal:** Build a support system around FNDR's local model so its stored output is grounded, structured, and useful to both agents and humans, and so the model can be measured, customized, fine-tuned, preference-tuned, promoted, and rolled back with evidence. Teach the owner every layer through staged questions and answers.

**Architecture:** Every LLM call becomes a named task with a versioned prompt, an output grammar, a validator, and a trace. A gold set and an eval runner turn every change into numbers with confidence intervals. Feedback events feed an improvement ladder that climbs from cheap (prompt and grammar) to expensive (LoRA, DPO). A promotion gate decides, from evidence, whether a candidate replaces the active model configuration, and a rollback is one file.

**Tech Stack:** Rust (`src-tauri/src/inference/`, `telemetry/`, `memory_review/`), llama.cpp through `llama-cpp-2` with Metal, Python 3 scripts for scoring and training data, MLX-LM (local LoRA), Vitest for the Intelligence panel.

**Spec:** `2026-09-21-beta-final-master-plan.md` sections 2, 4, 7. Tickets: MOD-02, MOD-04, MOD-05, MOD-06, MOD-08, MOD-15 (Beta) and MOD-03, MOD-07, MOD-09, MOD-11, MOD-12, MOD-13, MOD-14 plus LRN-01 to LRN-07 (Final and learning).

## Global Constraints

- Strictly local models. No cloud LLM at runtime, and (decision D-3, proposed) no cloud model generates training labels.
- Reference machine is Apple M1, 8 GB RAM. Model RAM budget when loaded: 2.5 GB resident, VLM not concurrently loaded.
- Personal captures, feedback exports, training pairs, adapters, and traces with content never enter git.
- Traces store hashes and lengths by default. Prompt and output text only with the developer switch `FNDR_TRACE_CONTENT=1`.
- No commits to `main`. Branch per ticket. `make test` before claiming done.
- No em dashes in code comments, docs, or commit messages.

---

# Part 1: The support system

## 1. What we are building

```
capture text
     |
     v
 Task catalog: name, prompt vN, schema, grammar, validator, budget, fallback
     |
     v
 llama.cpp runtime (one generation at a time today) ----> llm_traces (task, version, tokens, ms, validator)
     |
     v
 validators and grounding checks ----> stored memory (or a recorded, explained skip)
     |                                         |
     |                                         v
     |                                  humans and agents use it
     |                                         |
     v                                         v
 gold set + eval runner <----------- feedback events (edit, thumbs, agent cited)
     |                                         |
     v                                         v
 improvement ladder (prompt, grammar, few-shot, GEPA, LoRA SFT, DPO)
     |
     v
 promotion gate (target gain, guard regressions) ----> adapter registry (active pointer, rollback)
```

What exists today in v1: inline prompt constants (`MEMORY_SYNTHESIS_PROMPT` at `inference/qwen_vl_memory.rs:57`, `VISION_SYSTEM_PROMPT` at `inference/image_semantics.rs:1339`, `SCREEN_GUIDE_SYSTEM_PROMPT` at `inference/mod.rs:89`, the meeting breakdown prompt at `meeting/mod.rs:1295`, agent prompts in `agent/prompts.rs`), one shared `LlamaContext` behind a mutex ("only one generation runs at a time across the whole engine", `inference/mod.rs:1741`), an output schema `StructuredMemoryExtraction` (`inference/mod.rs:586`) with a canonical activity list (`inference/mod.rs:646`), and grounding checks (`field_supported_by_evidence`, `validate_structured_memory_extraction` in `capture/mod.rs`). What is missing: labeled data, scores, traces, feedback, versions, and any way to change the model without editing Rust.

## 2. Reality check: what can we train on an 8 GB M1

| Rung | What changes | Compute | Where it runs | Cost | Expected effect | Risk | Ticket |
|---|---|---|---|---|---|---|---|
| 1 Measure | Nothing yet | none | anywhere | hours | Know the truth | none | MOD-04, 05, 06 |
| 2 Prompt and schema | Prompt text, examples, output schema | inference only | local | hours | Often the largest cheap gain | Overfitting to gold set | MOD-03 |
| 3 Grammar-constrained decoding | Sampler masks tokens that break the schema | inference only | local | hours | Format validity near 100 percent by construction; does not fix truthfulness | Grammar too strict | MOD-07 |
| 4 Model swap and quantization | Which weights | inference only | local | a day | Quality per GB | RAM pressure | MOD-08 |
| 5 Automated prompt evolution (GEPA) | Prompt text found by search against the gold set | many inference calls | local | days | Research reports it can beat RL with about 35 times fewer rollouts | Weak reflector model | MOD-11 |
| 6 LoRA SFT | Small adapter matrices trained on labeled examples | training | local MLX or a bigger machine | days | Behavior and format tuned to FNDR | Forgetting, small data | MOD-12 |
| 7 DPO on preference pairs | Adapter trained to prefer chosen over rejected outputs | training | same as 6 | days | Aligns to what the user corrects | Feedback loops, reward hacking | MOD-13 |
| 8 Promote and roll back | Active adapter pointer | none | local | hours | Safe deployment | none | MOD-14 |

Honest note on "RLHF": classic RLHF trains a reward model and then optimizes with PPO while holding a policy, a frozen reference, a reward model, and a value model in memory. That does not fit an 8 GB machine. DPO reaches the same goal with a policy and a frozen reference and no reward model, which is why the plan uses DPO for the preference stage. Stage 6 of the learning path explains the derivation so you can say this precisely in interviews.

Honest note on compute: community guides report LoRA on a 4B model fits a 16 GB Mac, and 12B wants 32 GB. On 8 GB, plan for a 1B to 2B base model, 4-bit weights, batch size 1, and short sequences, and prove the pipeline end to end with a 20-step toy run (Task 10) before spending time on data. If a teammate has a 16 GB or larger Mac (OPS-06 finds out), train there.

## 3. What the research says (dated September 2026)

| Finding | Source | What we do with it |
|---|---|---|
| GEPA, reflective prompt evolution, accepted as an ICLR 2026 oral, reports beating GRPO by about 10 percent on average with up to 35 times fewer rollouts on Qwen3 8B | https://arxiv.org/abs/2507.19457 | Rung 5. Note the paper's reflector is a stronger model; ours is local, so expect less |
| MLX-LM supports LoRA, DoRA, QLoRA and full fine-tuning on Apple Silicon; guides use 4-bit bases and small batches on 16 GB | https://www.kdnuggets.com/fine-tuning-language-models-on-apple-silicon-with-mlx | Rung 6 on the Mac |
| SFT first to teach format and task, then DPO to prefer better responses is the usual sequence | https://medium.com/@dummahajan/train-your-own-llm-on-macbook-a-15-minute-guide-with-mlx-6c6ed9ad036a | Rung 6 then 7 |
| User edits can serve as implicit preference data for LoRA plus DPO personalization | https://github.com/VanillaCreamer/Awesome-Personalized-LLMs | Task 8 feedback events |
| Gemma 4 (April 2026) ships E2B and E4B on-device sizes with native function calling and audio input; about 1.5 GB and 5 GB at 4-bit | https://huggingface.co/blog/gemma4 and https://unsloth.ai/docs/models/gemma-4 | MOD-08 candidates. E4B will not coexist with the VLM on 8 GB |
| Apple Foundation Models framework: about 3B on-device model, guided generation, tool calling, free and offline, macOS 26 | https://developer.apple.com/videos/play/wwdc2025/286/ | Optional MOD-08 spike; a zero-RAM baseline if reachable from Rust through a Swift bridge |
| Reports that MLX beats llama.cpp on Apple Silicon and that Ollama 0.19 and later use MLX on it | https://apxml.com/posts/best-local-llms-apple-silicon-mac | Single-source claim; test it on our own prompts in MOD-08 before believing it |
| Agent memory frameworks are benchmarked on LongMemEval and LoCoMo; temporal graph designs score higher on temporal questions | https://mem0.ai/blog/state-of-ai-agent-memory-2026 | Vendor blog. Use the benchmark names in the related-work slide, not the numbers |

## 4. Tasks (Beta)

### Task 1: LLM call inventory and traces (MOD-02)

**Files:**
- Create: `src-tauri/src/telemetry/llm_trace.rs`
- Modify: `src-tauri/src/telemetry/mod.rs` (declare module)
- Modify: `src-tauri/src/inference/mod.rs:1749-1960` (`complete`, `complete_with_control`, `complete_blocking`)
- Create: `docs/product/llm-task-catalog.md`

**Interfaces:**
- Produces: `LlmTrace`, `TraceInput`, `build_trace(input: &TraceInput<'_>, include_content: bool) -> LlmTrace`, `append_trace(path: &Path, trace: &LlmTrace) -> std::io::Result<()>`, `with_task(task: &'static str, version: &'static str, fut: F) -> F::Output`, and the file `<app data dir>/llm_traces.jsonl`. MOD-15 counts lines in it; eval and GEPA read `task` and `prompt_version` from it.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/telemetry/llm_trace.rs` containing only:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn input<'a>() -> TraceInput<'a> {
        TraceInput {
            ts_ms: 1,
            task: "memory_extraction",
            prompt_version: "v3",
            model_id: "qwen3-vl-2b",
            prompt_tokens: 1200,
            output_tokens: 180,
            latency_ms: 950,
            prompt: "secret OCR text about a bank",
            output: "{\"topic\":\"x\"}",
            validator: "ok",
        }
    }

    #[test]
    fn default_trace_never_contains_text() {
        let t = build_trace(&input(), false);
        let json = serde_json::to_string(&t).unwrap();
        assert!(!json.contains("secret OCR"));
        assert!(t.prompt.is_none() && t.output.is_none());
        assert_eq!(t.output_len, 13);
        assert_eq!(t.prompt_sha256.len(), 64);
    }

    #[test]
    fn content_is_included_only_on_request() {
        let t = build_trace(&input(), true);
        assert_eq!(t.prompt.as_deref(), Some("secret OCR text about a bank"));
    }

    #[test]
    fn append_writes_one_parseable_line_per_trace() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("llm_traces.jsonl");
        append_trace(&path, &build_trace(&input(), false)).unwrap();
        append_trace(&path, &build_trace(&input(), false)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        let parsed: LlmTrace = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed.task, "memory_extraction");
    }

    #[test]
    fn oversized_file_rotates_once() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("llm_traces.jsonl");
        std::fs::write(&path, vec![b'x'; MAX_TRACE_BYTES as usize]).unwrap();
        append_trace(&path, &build_trace(&input(), false)).unwrap();
        assert!(dir.path().join("llm_traces.jsonl.1").exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap().lines().count(), 1);
    }

    #[tokio::test]
    async fn task_label_is_visible_inside_scope_and_defaults_outside() {
        assert_eq!(current_task(), ("unlabeled", "v0"));
        let inside = with_task("card_synthesis", "v2", async { current_task() }).await;
        assert_eq!(inside, ("card_synthesis", "v2"));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Declare `pub mod llm_trace;` in `src-tauri/src/telemetry/mod.rs`, then run: `cd src-tauri && cargo test llm_trace`
Expected: compile FAIL with "cannot find function `build_trace`", "cannot find function `with_task`", and similar.

- [ ] **Step 3: Write the implementation above the tests in the same file**

```rust
//! Local LLM call traces. One JSON line per model call, appended to a bounded file.
//!
//! By default a trace stores a hash and lengths, never prompt or output text. Full text is written only
//! when `include_content` is true (a developer switch), because prompts contain captured screen text.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::OpenOptions;
use std::future::Future;
use std::io::Write;
use std::path::{Path, PathBuf};

const MAX_TRACE_BYTES: u64 = 5 * 1024 * 1024;

tokio::task_local! {
    static LLM_TASK: (&'static str, &'static str);
}

/// Run `fut` with a task label and prompt version that traces recorded inside it will carry.
pub async fn with_task<F: Future>(task: &'static str, version: &'static str, fut: F) -> F::Output {
    LLM_TASK.scope((task, version), fut).await
}

pub fn current_task() -> (&'static str, &'static str) {
    LLM_TASK.try_with(|t| *t).unwrap_or(("unlabeled", "v0"))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LlmTrace {
    pub ts_ms: i64,
    pub task: String,
    pub prompt_version: String,
    pub model_id: String,
    pub prompt_tokens: u32,
    pub output_tokens: u32,
    pub latency_ms: u64,
    pub prompt_sha256: String,
    pub output_len: usize,
    pub validator: String,
    pub prompt: Option<String>,
    pub output: Option<String>,
}

pub struct TraceInput<'a> {
    pub ts_ms: i64,
    pub task: &'a str,
    pub prompt_version: &'a str,
    pub model_id: &'a str,
    pub prompt_tokens: u32,
    pub output_tokens: u32,
    pub latency_ms: u64,
    pub prompt: &'a str,
    pub output: &'a str,
    pub validator: &'a str,
}

pub fn build_trace(input: &TraceInput<'_>, include_content: bool) -> LlmTrace {
    LlmTrace {
        ts_ms: input.ts_ms,
        task: input.task.to_string(),
        prompt_version: input.prompt_version.to_string(),
        model_id: input.model_id.to_string(),
        prompt_tokens: input.prompt_tokens,
        output_tokens: input.output_tokens,
        latency_ms: input.latency_ms,
        prompt_sha256: format!("{:x}", Sha256::digest(input.prompt.as_bytes())),
        output_len: input.output.chars().count(),
        validator: input.validator.to_string(),
        prompt: include_content.then(|| input.prompt.to_string()),
        output: include_content.then(|| input.output.to_string()),
    }
}

/// Append one trace. When the file exceeds the cap it is renamed to `<name>.jsonl.1` (one generation kept).
pub fn append_trace(path: &Path, trace: &LlmTrace) -> std::io::Result<()> {
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() >= MAX_TRACE_BYTES {
            let mut rotated: PathBuf = path.to_path_buf();
            rotated.set_extension("jsonl.1");
            std::fs::rename(path, rotated)?;
        }
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let line = serde_json::to_string(trace).map_err(std::io::Error::other)?;
    writeln!(file, "{line}")
}
```

(The trace logic and its first four tests were compiled and run in an isolated crate before this plan was written, using a std-only hash stand-in. The repo version uses `sha2`, already a dependency, so the digest assertion here is 64 hex characters.)

- [ ] **Step 4: Run to verify it passes**

Run: `cd src-tauri && cargo test llm_trace`
Expected: PASS (5 tests). If `#[tokio::test]` is not available in this crate's test profile, `tokio` with `features = ["full"]` is already in `Cargo.toml`, so it is.

- [x] **Step 5: Carry token counts out of `complete_blocking` with an out-parameter (owner decision 2026-09-21)**

`complete_blocking` has one caller (`complete_with_control`) and six early returns, so returning a tuple would touch every return. Instead add a private `#[derive(Debug, Default, Clone, Copy)] struct TokenUsage { prompt_tokens: u32, output_tokens: u32 }` and a fifth parameter `usage: &mut TokenUsage`. Set `usage.prompt_tokens = prompt_len as u32` right after the prompt is tokenized and `usage.output_tokens += 1` after each generated piece is appended. The early returns are untouched, so behavior is unchanged.

- [x] **Step 6: Record the trace inside the blocking closure (owner decision 2026-09-21)**

Add `trace_path: Option<PathBuf>` to `InferenceEngine`, set in `new` to `<app data dir>/llm_traces.jsonl` (`None` when there is no app data dir, so tests and evals write nothing). In `complete_with_control`, read `current_task()` before `spawn_blocking` because task-locals do not cross into it, take `let started = Instant::now()`, and inside the closure call `self_static.record_trace(task, prompt_version, &prompt_owned, &output, usage, started)`. `record_trace` builds the trace (content only when `FNDR_TRACE_CONTENT=1`, validator `not_validated`) and appends it, logging a debug line on failure. Doing the write inside the blocking closure keeps file IO off the async thread.

Verified end to end by the ignored test `complete_writes_labeled_trace_lines_with_token_counts` (`cargo test --lib complete_writes_labeled -- --ignored --nocapture`), which loads the real model once, makes one explicitly labeled call and two real method calls, and checks the trace lines for task, version, token counts and latency. Read the `test ... ok` line: the process aborts at exit afterwards because the leaked model trips ggml's Metal teardown (finding F9).

- [ ] **Step 7: Build the catalog**

Run:

```bash
grep -rn "\.complete(\|complete_with_control(" src-tauri/src --include="*.rs" | grep -v "fn complete"
```

Create `docs/product/llm-task-catalog.md` with one row per call site, starting from these known prompt sources:

| Task id | Prompt source | File and line |
|---|---|---|
| `memory_extraction_ocr` | (find via the grep) | |
| `memory_synthesis_vlm` | `MEMORY_SYNTHESIS_PROMPT` | `inference/qwen_vl_memory.rs:57` |
| `visual_extraction` | `VISION_SYSTEM_PROMPT` | `inference/image_semantics.rs:1339` |
| `screen_guide` | `SCREEN_GUIDE_SYSTEM_PROMPT` | `inference/mod.rs:89` |
| `meeting_breakdown` | `build_legacy_breakdown_prompt` | `meeting/mod.rs:1295` |
| `memory_review` and `daily_review` | prompt built in `memory_review/pipeline.rs` | verify |
| `card_synthesis` | `search/memory_cards.rs` | verify |
| `query_expansion` | `inference/mod.rs` near the expansion parser (line about 706) | verify |
| `agent_*` | `agent/prompts.rs` | verify |
| `daily_summary`, `wrapped` | `summariser/` and the Wrapped feature | verify |

For each row fill: max tokens, timeout, expected output shape, validator, fallback when the model fails, and how often it runs per day (from traces after a day of use). Wrap each real call site in `with_task("<task id>", "v1", async { ... }).await`.

- [ ] **Step 8: Verify and commit**

Run: `make test` and `cd src-tauri && cargo test llm_trace`
Then run the app for ten minutes, and `wc -l "<app data dir>/llm_traces.jsonl"` shows lines with real task labels.

```bash
git checkout -b feat/mod-02-llm-traces
git add src-tauri/src docs/product/llm-task-catalog.md
git commit -m "feat(telemetry): trace every LLM call with task label and token counts"
```

### Task 2: Gold set v0 (MOD-04)

**Files:**
- Create: `src-tauri/tests/fixtures/gold/v0/extraction.jsonl` (50 lines)
- Create: `src-tauri/tests/fixtures/gold/v0/questions.jsonl` (30 lines)
- Create: `src-tauri/tests/fixtures/gold/v0/guard.jsonl` (20 lines)
- Create: `src-tauri/tests/fixtures/gold/v0/README.md` (labeling guide and agreement result)

**Interfaces:**
- Consumes: the 30-screen corpus from CAP-03 (its `expected_text` becomes `evidence` for 30 of the 50 cases; write 20 more from public content).
- Produces: JSONL that `scripts/model/score.py` reads.

Line shapes:

```json
{"id": "g-001", "evidence": "running 12 tests\ntest result: ok. 12 passed; 0 failed", "app": "Terminal", "window_title": "cargo test", "expected": {"activity_type": "testing_workflow", "topic": "cargo test run", "must_mention": ["12 passed"], "must_not": ["deploy"]}, "labeler": "MK", "split": "dev"}
{"id": "q-001", "question": "when did the tests last pass in the repo", "relevant": ["g-001"]}
```

- [ ] **Step 1: Write the labeling guide in `README.md`**

Rules every labeler follows: `activity_type` must be one of the 19 values in `CANONICAL_ACTIVITY_TYPES` (`inference/mod.rs:646`); `topic` is at most 12 words and states what the user was doing; `memory_context` may only contain facts visible in `evidence`; put facts that must appear in `must_mention` and traps a hallucinating model falls into in `must_not`; leave a field empty rather than guess.

- [ ] **Step 2: Write 50 extraction cases, 30 retrieval questions, and 20 guard cases**

Splits: 30 `dev` cases (used for prompt and grammar iteration), 20 `test` cases (looked at only when reporting), and the separate 20-case guard file (never used for tuning; it detects regressions). Content is synthetic or public only. A second teammate re-labels 10 cases blind.

- [ ] **Step 3: Record agreement**

Compute percent agreement on `activity_type` for the 10 double-labeled cases and put it in the README with the disagreements resolved. If agreement is below 80 percent the label definitions are the problem, not the labelers: tighten the guide and re-label those 10.

- [ ] **Step 4: Verify the files parse and the counts are right**

Run:

```bash
python3 - <<'PY'
import json
d = "src-tauri/tests/fixtures/gold/v0/"
counts = {f: sum(1 for l in open(d + f) if l.strip()) for f in ["extraction.jsonl", "questions.jsonl", "guard.jsonl"]}
print(counts)
assert counts == {"extraction.jsonl": 50, "questions.jsonl": 30, "guard.jsonl": 20}
for f in counts:
    for l in open(d + f):
        if l.strip():
            json.loads(l)
print("ok")
PY
```

Expected: the counts dictionary then `ok`.

- [ ] **Step 5: Commit**

```bash
git checkout -b feat/mod-04-gold-set
git add src-tauri/tests/fixtures/gold
git commit -m "test(eval): gold set v0 with 50 extraction cases, 30 questions, 20 guard cases"
```

### Task 3: Eval runner and baseline (MOD-05, MOD-06)

**Files:**
- Create: `scripts/model/score.py`, `scripts/model/test_score.py`, `scripts/model/build_pairs.py`, `scripts/model/promotion_gate.py`, `scripts/model/test_train.py` (copied from the tested assets)
- Create: `src-tauri/examples/eval_predict.rs`
- Modify: `Makefile` (targets `eval`, `py-test`), `.gitignore` (`.eval-tmp/`)
- Create (by running): `docs/evidence/W03/eval-baseline.md` and `docs/evidence/W03/eval-baseline.json`

**Interfaces:**
- Consumes: gold files from Task 2; the extraction function named in the catalog from Task 1.
- Produces: `score.py` with `recall_at_k`, `mrr_at_k`, `wilson_interval`, `is_supported`, `parse_prediction`, `score_extraction`, `score_retrieval`, `flat_metrics`, `render_report`; CLI `--gold --predictions --questions --rankings --required-keys --text-fields --model-label --out --json-out`. `promotion_gate.decide(base, cand, target, min_gain, guards, max_regress) -> (bool, list[str])`. `build_pairs.build_pairs(events, max_chars) -> list[dict]`.

These five Python files were written test-first and run before this plan was written: `test_score.py` runs 8 tests and `test_train.py` runs 9, all passing. Copy them rather than retyping.

- [ ] **Step 1: Copy the tested scripts**

```bash
mkdir -p scripts/model
cp docs/superpowers/plans/assets/2026-09-21/ws2/score.py \
   docs/superpowers/plans/assets/2026-09-21/ws2/test_score.py \
   docs/superpowers/plans/assets/2026-09-21/ws2/build_pairs.py \
   docs/superpowers/plans/assets/2026-09-21/ws2/promotion_gate.py \
   docs/superpowers/plans/assets/2026-09-21/ws2/test_train.py scripts/model/
```

- [ ] **Step 2: Run the script tests from their new home**

Run: `python3 scripts/model/test_score.py && python3 scripts/model/test_train.py`
Expected: `Ran 8 tests ... OK` then `Ran 9 tests ... OK`.

- [ ] **Step 3: Add Make targets**

Append to `Makefile` (and add `eval py-test` to `.PHONY`; add `py-test` to the `test` target's command list):

```make
py-test:
	python3 scripts/model/test_score.py
	python3 scripts/model/test_train.py
	python3 scripts/bench/test_summarize_metrics.py

MODEL_LABEL ?= current
OUT ?= docs/evidence/eval-run.md

eval:
	cd src-tauri && cargo run --release --example eval_predict -- --gold tests/fixtures/gold/v0 --out ../.eval-tmp
	python3 scripts/model/score.py \
	  --gold src-tauri/tests/fixtures/gold/v0/extraction.jsonl \
	  --predictions .eval-tmp/predictions.jsonl \
	  --questions src-tauri/tests/fixtures/gold/v0/questions.jsonl \
	  --rankings .eval-tmp/rankings.jsonl \
	  --model-label "$(MODEL_LABEL)" --out $(OUT) --json-out $(OUT:.md=.json)
```

- [ ] **Step 4: Write the predictor example**

`src-tauri/examples/eval_predict.rs` reads `--gold <dir>` and `--out <dir>`, and for each line of `extraction.jsonl` runs the real extraction path (the function named in the Task 1 catalog for `memory_extraction_ocr`) on `evidence`, writing `{"id", "output"}` lines to `predictions.jsonl`. For each line of `questions.jsonl` it runs the hybrid search function the search command uses (`search_hybrid_memories`) over a temporary index built from the gold evidence texts, writing `{"id", "ranked"}` lines of memory ids to `rankings.jsonl`. It accepts `--model <gguf path>` and `--grammar on|off` (used by Tasks 4 and 7). It never touches the user's real store: the index lives in a `tempfile` directory.

Verify: `cd src-tauri && cargo run --release --example eval_predict -- --gold tests/fixtures/gold/v0 --out ../.eval-tmp`
Expected: `.eval-tmp/predictions.jsonl` has 50 lines and `.eval-tmp/rankings.jsonl` has 30.

- [ ] **Step 5: Take the baseline**

Run: `make eval MODEL_LABEL="current-1b-baseline" OUT=docs/evidence/W03/eval-baseline.md`
Expected: the markdown report and the JSON exist. The report prints each rate with a 95 percent confidence interval. Read the interval first: at n=50 a score of 90 percent has an interval of roughly plus or minus 8 points, so only differences larger than that mean anything.

- [ ] **Step 6: Commit**

```bash
git checkout -b feat/mod-05-eval-runner
git add scripts/model src-tauri/examples Makefile .gitignore docs/evidence/W03
git commit -m "feat(eval): make eval scores extraction and retrieval with confidence intervals"
```

### Task 4: Model bake-off on the M1 8 GB (MOD-08)

**Files:**
- Create: `docs/evidence/W04/model-bakeoff.md`
- Create: `docs/decisions/016-local-model-selection.md`

**Interfaces:**
- Consumes: `make eval` (Task 3), `system_metrics` peak RSS.
- Produces: the chosen model id for `FNDR_MODEL_PROFILE = "m1_8gb_default"` and the evidence for it.

Candidates (check current releases and licenses on the day; do not trust this table's dates):

| Candidate | Approx 4-bit RAM | License to check | Note |
|---|---|---|---|
| Current: Qwen3-VL-2B Q4_K_M (the only model the app loads; the Llama-3.2-1B file on disk is legacy) | about 1.1 GB weights | Qwen license terms, check on the day | Baseline |
| Qwen3 4B Instruct Q4 | about 2.5 GB | Apache 2.0 | v2's Q&A pick |
| Gemma 4 E2B Q4 | about 1.5 GB | Gemma terms | Native function calling |
| Gemma 4 E4B Q4 | about 5 GB | Gemma terms | Will not coexist with the VLM; test alone |
| Apple Foundation Models | system managed | Apple terms | Optional spike through a Swift bridge; check whether custom adapters are available to us, otherwise it is a baseline only |

- [ ] **Step 1: Run each candidate three times, with grammar off, on the dev split**

```bash
for m in current qwen3-4b gemma4-e2b gemma4-e4b; do
  for i in 1 2 3; do
    make eval MODEL_LABEL="$m-run$i" OUT=docs/evidence/W04/eval-$m-run$i.md
  done
done
```

Expected: 12 report pairs. If `make eval` needs the model path, pass it through `MODEL` and the predictor's `--model` flag. Restart the app between models so RSS readings are clean.

- [ ] **Step 2: Record cost next to quality**

For each model note tokens per second and peak resident memory from a one-minute run of `capture.semantic_ms` in Pipeline Inspector (Task 1 of WS1 makes that readable). Also run `llama-bench` (install with `brew install llama.cpp`) on each GGUF: `llama-bench -m <file> -p 512 -n 128`.

- [ ] **Step 3: Write the comparison table**

`docs/evidence/W04/model-bakeoff.md` must contain: model, quant, peak RAM, tokens per second, format validity, grounding rate, activity accuracy, each with its 95 percent interval, and the run-to-run spread from the three runs.

- [ ] **Step 4: Apply the decision rule and record it**

Pick the smallest model whose grounding rate is within 3 points of the best candidate and whose peak RAM is at or under 2.5 GB. If two candidates tie inside the intervals, prefer the smaller and say the data cannot separate them. Write the choice as ADR-016 with the table linked.

- [ ] **Step 5: Commit**

```bash
git checkout -b docs/mod-08-model-bakeoff
git add docs/evidence/W04 docs/decisions/016-local-model-selection.md
git commit -m "docs(model): bake-off on M1 8 GB and ADR-016 model selection"
```

### Task 5: Intelligence panel v0 (MOD-15)

**Files:**
- Create: `src/domains/intelligence/IntelligencePanel.tsx`
- Create: `src/domains/intelligence/IntelligencePanel.test.tsx`
- Create: `src/domains/intelligence/types.ts`
- Modify: the app shell file that lists panels (find with `grep -rn "PrivacyPanel\|MemoryCardsPanel" src/app`), and the Rust side: a command `get_latest_eval_metrics` that reads `<app data dir>/eval/latest.json` (the file `make eval` copies there).

**Interfaces:**
- Consumes: the flat JSON that `score.py --json-out` writes (Task 3) and the trace count from `llm_traces.jsonl` (Task 1).
- Produces: `EvalMetrics` type and `IntelligencePanel({ metrics, traceCount })`.

- [ ] **Step 1: Write the failing test**

`src/domains/intelligence/IntelligencePanel.test.tsx`:

```tsx
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { IntelligencePanel } from "./IntelligencePanel";
import type { EvalMetrics } from "./types";

const metrics: EvalMetrics = {
    model_label: "qwen3-4b-q4",
    cases: 50,
    format_validity: 0.98,
    grounding_rate: 0.91,
    activity_accuracy: 0.76,
    recall_at_5: 0.83,
    mrr_at_10: 0.71,
    questions: 30,
};

afterEach(cleanup);

describe("IntelligencePanel", () => {
    it("shows the model label and each score as a percentage", () => {
        render(<IntelligencePanel metrics={metrics} traceCount={128} />);
        expect(screen.getByText(/qwen3-4b-q4/)).toBeInTheDocument();
        expect(screen.getByText("98.0%")).toBeInTheDocument();
        expect(screen.getByText("91.0%")).toBeInTheDocument();
        expect(screen.getByText("83.0%")).toBeInTheDocument();
        expect(screen.getByText(/128 model calls traced/)).toBeInTheDocument();
    });

    it("explains how to produce numbers when no eval has run", () => {
        render(<IntelligencePanel metrics={null} traceCount={0} />);
        expect(screen.getByText(/run make eval/i)).toBeInTheDocument();
    });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `npm test -- intelligence`
Expected: FAIL, cannot find module `./IntelligencePanel`.

- [ ] **Step 3: Implement**

`src/domains/intelligence/types.ts`:

```ts
export interface EvalMetrics {
    model_label: string;
    cases: number;
    format_validity: number;
    grounding_rate: number;
    activity_accuracy: number;
    recall_at_5: number;
    mrr_at_10: number;
    questions: number;
}
```

`src/domains/intelligence/IntelligencePanel.tsx`:

```tsx
import type { EvalMetrics } from "./types";

interface Props {
    metrics: EvalMetrics | null;
    traceCount: number;
}

const pct = (value: number) => `${(value * 100).toFixed(1)}%`;

export function IntelligencePanel({ metrics, traceCount }: Props) {
    if (!metrics) {
        return (
            <section aria-label="Intelligence">
                <h2>Intelligence</h2>
                <p>No eval report yet. Run make eval to produce one.</p>
            </section>
        );
    }
    return (
        <section aria-label="Intelligence">
            <h2>Intelligence</h2>
            <p>Model: {metrics.model_label}</p>
            <dl>
                <dt>Format validity</dt>
                <dd>{pct(metrics.format_validity)}</dd>
                <dt>Grounding rate</dt>
                <dd>{pct(metrics.grounding_rate)}</dd>
                <dt>Activity accuracy</dt>
                <dd>{pct(metrics.activity_accuracy)}</dd>
                <dt>Recall at 5</dt>
                <dd>{pct(metrics.recall_at_5)}</dd>
                <dt>MRR at 10</dt>
                <dd>{pct(metrics.mrr_at_10)}</dd>
            </dl>
            <p>
                {traceCount} model calls traced ({metrics.cases} extraction cases, {metrics.questions} questions)
            </p>
        </section>
    );
}
```

Styling uses the existing design tokens (Felipe owns the visual pass; see `docs/product/DESIGN_DIRECTION.md`).

- [ ] **Step 4: Run to verify it passes, then wire the data**

Run: `npm test -- intelligence`
Expected: PASS (2 tests). Then add the `get_latest_eval_metrics` command, add the panel to the shell, and confirm a screenshot shows numbers from `docs/evidence/W03/eval-baseline.json`.

- [ ] **Step 5: Commit**

```bash
git checkout -b feat/mod-15-intelligence-panel
git add src src-tauri/src
git commit -m "feat(ui): Intelligence panel shows model eval scores and trace counts"
```

## 5. Tasks (Final phase, decomposed further at the Beta retro)

These are specified to the level needed to start; each gets its own detailed steps at the W5 retro using Beta results.

### Task 6: Prompt registry and validators (MOD-03, W5 to W6)

**Files:** create `src-tauri/prompts/<task>/v<n>.md`, `schema.json`, `examples.jsonl`; create `src-tauri/src/inference/prompt_registry.rs`.
**Interfaces:** `pub fn load_prompt(task: &str, version: &str) -> Option<&'static str>` (compiled in with `include_str!`), and the version string that `with_task` passes to traces.

- [ ] Move the top three prompts from the catalog (by daily call count) into files, one version per file, with a header comment stating the change from the previous version.
- [ ] Test: for every task in the catalog, `load_prompt(task, current_version)` returns text and the file's declared schema parses as JSON.
- [ ] Acceptance: `make eval` results for `v1` equal the pre-move baseline within the confidence interval (a pure refactor must not change scores).

### Task 7: Grammar-constrained JSON (MOD-07, W5 to W6)

**Files:** create `src-tauri/prompts/memory_extraction/grammar.gbnf`; modify the sampler construction in `inference/mod.rs`.
**Interfaces:** `--grammar on|off` in `eval_predict` (Task 3).

- [ ] Write the grammar from the real schema. Starting point covering the core keys of `StructuredMemoryExtraction`; extend it with every other key the prompt asks for:

```
root ::= "{" ws
  "\"activity_type\"" ws ":" ws activity ws "," ws
  "\"project\"" ws ":" ws string ws "," ws
  "\"topic\"" ws ":" ws string ws "," ws
  "\"memory_context\"" ws ":" ws string ws "," ws
  "\"user_intent\"" ws ":" ws string ws "," ws
  "\"outcome\"" ws ":" ws string ws "," ws
  "\"tags\"" ws ":" ws strlist ws "," ws
  "\"entities\"" ws ":" ws strlist ws "," ws
  "\"decisions\"" ws ":" ws strlist ws "," ws
  "\"errors\"" ws ":" ws strlist ws "," ws
  "\"next_steps\"" ws ":" ws strlist ws
"}"
activity ::= "\"coding\"" | "\"debugging\"" | "\"reviewing_agent_output\"" | "\"researching\"" | "\"planning\"" | "\"writing\"" | "\"studying\"" | "\"watching_or_listening\"" | "\"configuring_tool\"" | "\"testing_workflow\"" | "\"reading_results\"" | "\"organizing_information\"" | "\"communication\"" | "\"job_or_career_work\"" | "\"travel_or_logistics\"" | "\"entertainment_or_personal_interest\"" | "\"observing\"" | "\"screen_review\"" | "\"unknown\""
strlist ::= "[" ws (string (ws "," ws string)*)? ws "]"
string ::= "\"" ( [^"\\\x7F\x00-\x1F] | "\\" (["\\bfnrt] | "u" [0-9a-fA-F]{4}) )* "\""
ws ::= [ \t\n]*
```

The 19 activity values are copied from `CANONICAL_ACTIVITY_TYPES`; a test compares the grammar's list to that constant so they cannot drift.
- [ ] Validate the grammar text with llama.cpp before touching Rust: `llama-cli -m <gguf> --grammar-file src-tauri/prompts/memory_extraction/grammar.gbnf -p "<a prompt>" -n 200`. It has not been run yet because llama.cpp is not installed on this machine; this step is where it gets validated.
- [ ] Wire it in through the sampler chain in `llama-cpp-2` (check the crate's docs for the grammar sampler constructor; llama.cpp supports GBNF natively).
- [ ] Acceptance: `make eval` with `--grammar on` vs `off` on the same model: format validity moves toward 100 percent; grounding rate is reported alongside, because a grammar makes output valid, not true.

### Task 8: Feedback events (MOD-09, W5)

**Reuse first (anti-bloat):** `agent/audit.rs` already stores retrieval feedback: `RetrievalFeedbackRating` (Useful, Irrelevant, Wrong, Stale, MissingContext), `append_feedback`, `list_feedback`, and the MCP tool `agent.rate_result`. Extend that store with the two signals it lacks (an edit of a memory's text, and `agent_cited`) and with the trace fields below; do not create a second feedback store. Read `agent/audit.rs` (560 lines) before writing any code.

**Files:** modify `src-tauri/src/agent/audit.rs` (new signal variants and fields); modify memory card UI (`src/domains/memory-vault/MemoryCard.tsx`, `ExpandedMemoryCard.tsx`) for thumbs and edit; add export command.
**Interfaces:** each feedback record gains `{ task, trace_id, memory_id, signal: "edit" | "thumbs_up" | "thumbs_down" | "agent_cited" | "dismiss", prompt_sha256, output_len, edited_output: Option<String>, prompt_version, model_id }` alongside the existing rating, in the existing local file; an explicit `export_feedback_for_training` command that writes `feedback-export.jsonl` (with prompt and output text) only when invoked, to a path the user chooses.

- [ ] Tests: an edit event stores the edited text locally and is excluded from any default export; thumbs events carry no text.
- [ ] `agent_cited` is emitted when an MCP `memory.get_context_pack` call includes a memory (a weak positive; never used alone as a preference).
- [ ] Acceptance: 20 events created by using the app for a day appear in the export, and `build_pairs.py` turns the edit events into pairs.

### Task 9: Automated prompt evolution with GEPA (MOD-11, W6 to W7)

**Files:** create `scripts/model/gepa_run.py` (thin wrapper), results in `docs/evidence/W07/gepa.md`.
- [ ] Serve the local model with `llama-server` on loopback for the run only; wire an evaluate function that runs a candidate prompt over the dev split and returns the score plus a text explanation of the failures (the trace and validator outputs).
- [ ] Use the `gepa` package (`pip install gepa`) or DSPy's GEPA optimizer with the local model as both task model and reflector. Expect weaker results than the paper's stronger reflector and say so in the report.
- [ ] Report on the test split, not the dev split the optimizer saw. Acceptance: a table of prompt version, dev score, test score, and the guard set, promoted through Task 12's gate.

### Task 10: LoRA SFT experiment (MOD-12, W7)

**Files:** `scripts/model/make_sft_data.py` (gold plus accepted outputs to chat-format JSONL), `docs/evidence/W07/lora-sft.md`.
- [ ] **Step 0, the pipeline proof (2 hours):** install `pip install mlx-lm` (MLX 0.31 is already installed on this Mac; `mlx_lm` is not). Train 20 iterations on 20 examples: `python3 -m mlx_lm lora --help` first, because flags change between releases; typical invocation is `mlx_lm.lora --model <mlx 4-bit base> --train --data <dir> --iters 20 --batch-size 1 --num-layers 8`. Record wall time per iteration and peak memory. Extrapolate before committing to a real run.
- [ ] **Step 1, the conversion proof:** the runtime is llama.cpp, so the adapter must reach GGUF. Two routes: fuse the adapter into the base (`mlx_lm.fuse`), export to Hugging Face format, convert with llama.cpp's `convert_hf_to_gguf.py`, quantize, and load as a full model; or train with a PEFT-format adapter and convert it with llama.cpp's `convert_lora_to_gguf.py` for hot loading. Prove one route end to end on the toy adapter before any real data. Record which one worked in the evidence file.
- [ ] **Step 2:** train on the dev split plus accepted outputs (labels come from hand labeling and the local model's validator-passing outputs only, per D-3; do not train the model on its own unfiltered outputs). Evaluate on the test split and the guard set.
- [ ] Acceptance: an evidence file with training data size, hyperparameters, wall time, memory, and scores from `make eval` for base and adapter, with confidence intervals.

### Task 11: DPO from feedback (MOD-13, W8 to W9)

**Files:** `scripts/model/build_pairs.py` (tested, from Task 3), `docs/evidence/W09/dpo.md`.
- [ ] Build pairs: `python3 scripts/model/build_pairs.py <feedback-export.jsonl> --out /tmp/pairs.jsonl` (rules: an edit gives chosen equal to the edited text and rejected equal to the original; a thumbs-up and thumbs-down on the same prompt pair up; identical, empty, over-long, and duplicate pairs are dropped).
- [ ] Train a DPO adapter starting from the SFT adapter. Community tooling such as `mlx-lm-lora` implements DPO for MLX; verify it exists and its flags at the time. Use a low beta (start at 0.1) so the policy stays near the reference.
- [ ] Watch for reward hacking: if the model learns to please clicks (longer, more confident text) the guard set and the grounding rate will show it. Acceptance: pairs count, beta, scores for base, SFT, and DPO adapters.

### Task 12: Adapter registry, promotion gate, rollback (MOD-14, W10 to W11)

**Files:** create `src-tauri/src/inference/adapter_registry.rs`; use `scripts/model/promotion_gate.py`.
**Interfaces:** directory `<app data dir>/adapters/<name>/<version>/{adapter.gguf, manifest.json}`, pointer file `<app data dir>/adapters/active.json` `{ "name", "version" }`; manifest `{ base_model_sha256, eval_json, created_at, parent_version }`.

- [ ] Rust tests: an adapter loads only if its `base_model_sha256` equals the loaded model's digest; a missing or corrupt `active.json` falls back to the base model with a logged reason; rollback is deleting `active.json`.
- [ ] Gate: `python3 scripts/model/promotion_gate.py --base docs/evidence/W03/eval-baseline.json --candidate <candidate.json> --target grounding_rate --min-gain 0.05 --max-regress 0.01`. Exit code 0 means promote, 1 means reject, and it prints each reason. The rule: the target metric must rise by the minimum and no guard metric may fall by more than the limit.
- [ ] Acceptance: a demo where a candidate is promoted, then rolled back, with both events visible in the Intelligence panel.

Also in this epic (E-F2): replace the single engine-wide generation mutex with a priority queue (interactive query above capture-time synthesis above background review above backfill), modeled on `~/FNDR-2.0/crates/fndr-inference/src/model_worker.rs`, so review and daily jobs stop contending with capture.

---

# Part 2: The learning path (questions and answers with Claude)

## 6. How a session works

1. Say "start Stage N". I ask five to six questions, one at a time. Answer in your own words, even if unsure; a wrong answer shows me what to teach.
2. After each answer I confirm what is right, correct what is not, and go one level deeper ("what happens behind the scenes").
3. We run one lab on your own machine against FNDR's real models and data.
4. You write a one-page summary in your own words (`docs/learning/stage-N.md`) and teach it back to a teammate. Teaching back is the exit check.
5. The stage's claims for your resume come only from numbers your lab produced.

Cadence: LRN-01 (Stage 1) in W1, LRN-02 in W2, LRN-03 in W3, LRN-04 in W4, LRN-05 in W6, LRN-06 in W8, LRN-07 in W10. Each is about two hours.

Your level (prompting and RAG only) means we begin at Stage 1 and go slowly through it.

## 7. The stages

### Stage 1: What runs when FNDR calls the LLM (LRN-01, W1)

Why it matters: every performance and quality decision downstream rests on this model of the machine.

Questions I will ask:
1. FNDR sends a 1,200-token prompt to a 1B model. Walk me through what happens to the text before any math.
2. What is inside a Q4_K_M GGUF file, and why is a 1B-parameter model about 0.8 GB and not 4 GB?
3. Why does generation slow down and use more RAM as the context gets longer?
4. What is the difference between prefill and decode, and which dominates FNDR's short extraction jobs?
5. `model_config.rs` declares `QWEN_TEMPERATURE = 0.1` and `QWEN_TOP_P = 0.8`, but nothing reads them. The live text path in `inference/mod.rs:1892` uses a repetition penalty plus greedy. What does each of those four settings do to the next token, and which one is FNDR really running?
6. `QWEN_IDLE_UNLOAD_SECONDS = 90` promises an unload, but `model_worker.rs:85` only logs and nothing in the crate calls `QwenVlmWorker`. Separately, models are `Box::leak`ed, so no model is ever freed. Which LLM calls actually run in the baseline app, and which of them hold memory? (Answer with the MOD-02 inventory, not from memory.)

Answer key (read after answering):
- Text becomes tokens (byte-pair encoding), each token becomes a vector by table lookup, then N transformer blocks (attention plus MLP) run, then a final layer scores every vocabulary token (logits), a sampler picks one, it is appended, and the loop repeats. The KV cache keeps each past token's keys and values so they are not recomputed.
- Weights are stored quantized. Q4_K_M averages about 4.85 bits per weight, so 1.24 billion weights times 4.85 bits divided by 8 is about 0.75 GB. The same weights in 16-bit would be 2.5 GB and in 32-bit about 5 GB.
- KV cache size is `2 x layers x kv_heads x head_dim x context x bytes`. For a Llama-3.2-1B shape (16 layers, 8 KV heads, head dim 64, a worked example only) at 4,096 context in 16-bit that is `2 x 16 x 8 x 64 x 4096 x 2` bytes, which is 128 MiB. The app's live model, Qwen3-VL-2B (28 layers, 8 KV heads, head dim 128), costs 112 KiB per token, so 224 MiB at the text engine's default 2,048 context. It grows linearly with context.
- Prefill processes the whole prompt in parallel and is limited by compute. Decode produces one token at a time and is limited by memory bandwidth, because every token reads all the weights. Ceiling for decode tokens per second is about `memory bandwidth / model bytes`. The M1 has 68.25 GB/s, so a 0.8 GB model has a ceiling near 85 tokens per second and a 2.5 GB model near 27. Real numbers are lower; the gap is what the lab measures.
- Temperature rescales the probabilities before sampling; near 0 it almost always takes the top token (good for extraction). Top-p keeps only the smallest set of tokens whose probabilities add to the threshold and samples from those. Greedy is the limit of temperature 0: always the top token. A repetition penalty lowers the score of tokens seen in the last N (here 64) so a small model does not loop. Verified in code: the text LLM runs `penalties(64, 1.1)` then `greedy()`; the image path runs `penalties(64, 1.05)` then `greedy()`; only `vlm.rs` uses temperature 0.7 and top-p 0.9. The 0.1 and 0.8 constants are dead.
- Unloading would return unified memory so capture, the embedder, and the user's apps are not starved. In v1 no unload path exists: the idle branch in `model_worker.rs` only logs and that worker has no callers, and `Box::leak` (`inference/mod.rs:875`, `vlm.rs:115`, `image_semantics.rs:1187`) keeps loaded weights for the life of the process. What actually loads in the baseline is the text engine (about 0.8 GB with its KV cache); the vision runtime is optional photo import. Confirm by inventory (MOD-02) and by reading the process footprint before and after the first call.

Lab: `brew install llama.cpp`, then `llama-bench -m <the app's live model, Qwen3VL-2B-Instruct-Q4_K_M.gguf> -p 512 -n 128` at context 512, 2048, and 4096. Record prefill and decode tokens per second and peak RSS. Compute the decode ceiling from the formula above and compute the KV cache for your model from its printed layer and head counts. Write down where measurement and formula disagree and one hypothesis why.

Exit check: explain to a teammate, without slides, why a bigger model is slower per token and why a longer prompt costs more memory.

### Stage 2: Prompts, structured output, validators (LRN-02, W2)

Questions:
1. Why does a 1B model emit invalid JSON more often than a very large one?
2. What does a grammar do to the sampler, and what does it not fix?
3. What is the difference between format validity and grounding? Give a valid-but-false example.
4. On 2026-05-17 a validator rule silently dropped every stored memory. What harness feature would have caught it in an hour instead of days?
5. Why is "the prompt got longer" not the same as "the prompt got better"?

Answer key:
- Small models have less capacity to hold a syntax and a task at once, and errors compound token by token.
- A grammar sets the probability of any token that would break the grammar to zero at each step, so the output is valid by construction. It cannot make the content true.
- Valid and false: `{"activity_type":"coding","topic":"deploying kubernetes"}` when the screen showed a recipe. Grounding checks each value against the evidence.
- Per-reason skip counters with an alarm on drop rate, plus a gold set that fails when extraction output collapses (WS1 and this plan both add these).
- Only a measured score on held-out cases says better.

Lab: take `MEMORY_SYNTHESIS_PROMPT`, run the 30 dev cases with grammar off and on (Task 7), and read ten failures by hand. Sort them into syntax, hallucination, wrong enum, and empty.

Exit check: draw the path from screen text to a stored memory and mark each place a validator can reject it.

### Stage 3: Evals (LRN-03, W3)

Questions:
1. What does "the model got better" mean, and who decides?
2. You score 45 of 50 cases correct. How sure are you the true rate is 90 percent?
3. What is a guard set and why must it never be used for tuning? (Hint: `tests/anti_overfitting.rs` exists in this repo.)
4. What biases does an LLM judge have?
5. Why report the test split only when you are done?

Answer key:
- A metric on a fixed, labeled set that matches the job, agreed before looking at results.
- The 95 percent Wilson interval for 45 of 50 is roughly 79 to 96 percent, so "90" hides a wide range. More cases narrow it slowly (interval width shrinks with the square root of n).
- A guard set catches regressions (formatting, retrieval) you did not tune for. Tuning on it turns it into another dev set and hides overfitting.
- Position bias, verbosity bias, and preferring outputs that resemble its own style. Use it for ranking candidates, not as ground truth, and spot-check against human labels.
- Each look at the test split is a chance to overfit to it.

Lab: run `make eval` twice on the same model and read the run-to-run spread; then change one word in the prompt and see whether the change is bigger than the interval.

Exit check: explain to a teammate why we report intervals.

### Stage 4: Embeddings and retrieval (LRN-04, W4)

Questions:
1. What is an embedding, and why do we compare with cosine similarity?
2. BGE and Qwen3-Embedding use query instructions or prefixes (`embedding/prefixes.rs`). Why must queries and documents be embedded differently?
3. A 1024-dimension float32 vector is 4 KB. What does 100,000 chunks cost, and how does 384 dimensions change it?
4. What is Reciprocal Rank Fusion, and why fuse keyword and vector search?
5. What does a reranker add that a bi-encoder cannot?

Answer key:
- An embedding maps text to a point so that related texts are near each other; cosine measures the angle, ignoring length.
- Retrieval models are trained on question and passage pairs, so the query side and document side play different roles; the prefix tells the model which role it is.
- 100,000 chunks times 4 KB is about 410 MB at 1024 dimensions, about 154 MB at 384. Storage, RAM, and search time scale with dimension.
- RRF scores each document as the sum over rankers of `1 / (60 + rank)`, so a document ranked well by either keyword or vector search rises without needing comparable scores. Keyword finds exact strings the vector misses; vectors find paraphrases.
- A cross-encoder reads the query and document together, so it can judge relevance more precisely but is too slow to run on everything.

Lab: use `make eval` retrieval rows to compare MiniLM (384-d), BGE-large (1024-d), and one more candidate on Recall@5, MRR@10, embed milliseconds per chunk, and RAM.

Exit check: explain why FNDR has two embedding contracts today and what it would take to have one.

### Stage 5: Fine-tuning and LoRA (LRN-05, W6)

Questions:
1. What changes inside a model when you fine-tune it?
2. LoRA replaces a weight update with two small matrices. For a 2048 by 2048 weight and rank 16, how many trainable numbers is that compared with the full matrix?
3. What is the training loss for supervised fine-tuning, and why mask the prompt tokens?
4. What is catastrophic forgetting, and which of our sets detects it?
5. Should we fine-tune the model to remember facts from the user's screen? Why or why not?

Answer key:
- The weights change through gradient descent so the model gives higher probability to the training targets.
- LoRA freezes the weight W and learns an update BA where B is d by r and A is r by k. Trainable parameters are `r x (d + k)`: 16 x 4096 = 65,536 versus 4,194,304 for the full matrix, about 1.6 percent. It works because useful adaptations tend to be low-rank.
- Cross-entropy on the next token, computed only on the answer tokens so the model learns to produce answers, not to repeat prompts.
- The model loses abilities it had before; the guard set (format validity, retrieval behavior) shows it.
- No. Fine-tune behavior and format; use retrieval for facts, because facts change daily and cannot be reliably edited in or removed from weights, and a memory system that cannot delete is a privacy problem.

Lab: Task 10 Step 0 and Step 1 (the toy run and the conversion proof).

Exit check: explain the difference between teaching the model a behavior and teaching it a fact.

### Stage 6: Preference tuning, RLHF, DPO, GEPA (LRN-06, W8)

Questions:
1. Name the three stages of classic RLHF.
2. Why can't we run PPO-based RLHF on an 8 GB laptop?
3. DPO removes the reward model. What is the key idea?
4. What does the beta in DPO control?
5. Where will our preference pairs come from, and how could they mislead the model?
6. Why might prompt evolution (GEPA) beat reinforcement learning at our scale?

Answer key:
- Supervised fine-tuning, a reward model trained on human preference pairs, then policy optimization (PPO) against that reward with a penalty for drifting from the reference model.
- PPO needs the policy, a frozen reference, a reward model, and a value model in memory at once, and many sampled rollouts.
- The optimal policy under the RLHF objective implies an implicit reward `beta x log(pi / pi_ref)`, so the preference likelihood can be optimized directly with a classification-style loss: `-log sigmoid(beta x [(log pi(chosen) - log pi_ref(chosen)) - (log pi(rejected) - log pi_ref(rejected))])`.
- Beta sets how strongly the policy is held near the reference; small beta lets it move far, large beta keeps it close.
- Edits and thumbs, plus weak agent-cited signals. Misleading risks: users click on fluent but wrong outputs, edits reflect one mood or project, and the model learns to please rather than to be grounded (reward hacking); the guard set and grounding rate are the defense.
- GEPA changes text, not weights, and learns from natural-language failure reports instead of a single number, so it needs far fewer rollouts (the paper reports up to 35 times fewer than GRPO). It cannot teach skills the base model lacks.

Lab: build pairs with `build_pairs.py` from synthetic feedback, run Task 11, and pass the result through `promotion_gate.py`.

Exit check: explain why the gate exists in one minute.

### Stage 7: Deploy and operate (LRN-07, W10)

Questions: How do we know an adapter matches the base model it was trained for? What is the smallest possible rollback? What can drift in production that an eval will not show? Why must training data and adapters stay out of git? What is the license situation for each base model we ship or download?

Answer key summary: adapter manifests carry the base model digest; rollback is removing `active.json`; drift comes from new apps and screen layouts, so keep a rolling feedback and trace review; personal data cannot leave the machine and adapters can memorize it; licenses differ (Llama community license versus Apache 2.0 for Qwen, Gemma terms of use), so check redistribution before bundling.

Lab: run the promote-then-rollback demo from Task 12 and record both events.

Exit check: present the full loop, measure to promote to roll back, in five minutes.

## 8. Resume and presentation claims (use only with evidence)

Fill a claim only from a file that exists in `docs/evidence/`. Templates:

- "Measured extraction format validity and grounding on a labeled gold set with 95 percent Wilson intervals; chose the on-device model by data on an 8 GB M1 (ADR-016)."
- "Added grammar-constrained decoding; format validity moved from A to B on n cases (evidence file)."
- "Trained and evaluated a LoRA adapter locally with MLX; promoted through an eval gate with automatic rollback."
- "Built a feedback-to-DPO pipeline from user edits, with a guard set against reward hacking."

## Self-review

**Spec coverage (ask 2):** support system (Part 1 architecture and Tasks 1 to 12), customizable and improvable (prompt registry, grammar, adapters, gate), RLHF and post-training (rungs 6 and 7, Tasks 10 and 11, honest RLHF vs DPO note), learning through staged Q and A (Part 2, seven stages), what happens behind the scenes (answer keys and labs), resume and presentation framing (section 8).

**Placeholder scan:** the extraction function name in Task 3 Step 4 is resolved by the grep command that step names, because it is a fact about code that the inventory step discovers. Task 7's grammar has not been run through llama.cpp; the step that validates it is written into the task.

**Type consistency:** `flat_metrics` keys (`format_validity`, `grounding_rate`, `activity_accuracy`, `recall_at_5`, `mrr_at_10`) match `EvalMetrics` in Task 5 and the gate's `--target` and `--guards` defaults. `with_task` and `current_task` in Task 1 are the names Task 6 relies on. `build_pairs` and `decide` signatures match the tested assets.

## Sources

- GEPA: https://arxiv.org/abs/2507.19457
- Fine-tuning on Apple Silicon with MLX: https://www.kdnuggets.com/fine-tuning-language-models-on-apple-silicon-with-mlx
- SFT then DPO on a MacBook: https://medium.com/@dummahajan/train-your-own-llm-on-macbook-a-15-minute-guide-with-mlx-6c6ed9ad036a
- Personalized LLM research list: https://github.com/VanillaCreamer/Awesome-Personalized-LLMs
- Gemma 4 on device: https://huggingface.co/blog/gemma4 and https://unsloth.ai/docs/models/gemma-4
- Apple Foundation Models: https://developer.apple.com/videos/play/wwdc2025/286/
- Local LLMs on Apple Silicon (single source, verify): https://apxml.com/posts/best-local-llms-apple-silicon-mac
- Agent memory state of the field (vendor blog): https://mem0.ai/blog/state-of-ai-agent-memory-2026
