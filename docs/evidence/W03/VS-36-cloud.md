# VS-36 Resume suggests cited next steps from the whole thread (cloud)

## What was there

- `tasks::extract_from_memory::extract_task_candidates` had no production caller. It read `MemorySynthesisOutput` (the model's raw output). Resume reads stored `MemoryRecord`s, which carry the same `next_steps`, `decisions`, and `errors` fields after capture.
- Resume's `next_steps` lists only the newest memory's own steps. When the newest memory names none, the thread offers none, even if an earlier memory in the same thread does. No step says which memory it came from.

## What changed

- The extractor now reads a `MemoryRecord`. It keeps its rules:
  - every next step;
  - a decision only when it states work to do ("todo", "need to", "will ");
  - every error, as "Fix: ...".
- It drops two fields nothing could use: `due_date` (always empty) and an `evidence` string built from the model summary. Each candidate keeps `title`, `source_memory_id`, and `confidence` (the memory's `confidence_score` times 0.85, 0.70, or 0.65 by kind).
- `ResumeThread.suggested_next_steps` (new; additive in the JSON that MCP `memory.resume_work` returns) runs the extractor over the thread's memories, newest first. It keeps the first copy of each step, compared by `tasks::normalize_task_text` (case and punctuation ignored), up to three.
- `next_steps` is unchanged, so nothing that reads it changes.
- The thread's memories are now ordered by time, then id, so memories captured in the same millisecond come out in the same order every call.

## Test (written first)

`tests/resume_work.rs::resume_offers_up_to_three_cited_next_steps_from_the_whole_thread` builds one synthetic session of six memories:

- one next step 100 and 90 minutes ago;
- an error 60 minutes ago;
- two decisions 30 minutes ago, one stating work and one a choice already made;
- the 90-minute step repeated in different case 20 minutes ago;
- a newest memory with no step.

```
steps == [
  ("write the integration test.", "parser-4"),
  ("We will ship the parser behind a flag", "parser-3"),
  ("Fix: connection refused on port 5432", "parser-2"),
]
```

- Every cited id is in the thread's evidence.
- The existing `next_steps` is empty for this thread: that is the gap the ticket describes.
- Before the change the test did not compile (`no field suggested_next_steps on type &ResumeThread`).

Results:

- `cargo test --lib`: 910 passed, 0 failed, 9 ignored. The extractor's three unit tests now build `MemoryRecord`s, and the MCP `resume_work` tool test still passes.
- `--test resume_work`: 3 passed.

## Bar (anti-bloat gate)

The extractor meets the ticket's bar in a test: up to three steps, each with its source memory. So it is kept and connected, not deleted. It calls no model and adds no storage.

## What can go wrong

- An older step may already be done. Each suggestion names its memory, so an agent or a person can check it; nothing marks steps as done yet.
- The decision rule is a word list ("will " also matches "we will not"). It is the extractor's existing rule, not tuned here.
- `todos` and `blockers` on `MemoryRecord` are not read. Adding them is a one-line change each, but the extractor never read them, and no test set says they help.
- The desktop app does not call `resume_work` today; the field reaches MCP clients only.

## Retrieval gate, before and after, same seed

Both personas were seeded once under America/Denver time on 2026-10-05, then the gate ran on fda001c (VS-33) and on this change. All four runs pass. Resume is not on any retrieval path, so this is a check that nothing else moved.

| Persona | Path | Recall@5 before / after | MRR@10 before / after | p50 ms before / after | p95 ms before / after |
|---|---|---|---|---|---|
| knowledge-worker | search | 1.000 / 1.000 | 0.966 / 0.966 | 225 / 226 | 351 / 414 |
| knowledge-worker | ask | 1.000 / 1.000 | 0.966 / 0.966 | 848 / 885 | 1018 / 976 |
| knowledge-worker | retrieve | 1.000 / 1.000 | 0.966 / 0.966 | 196 / 195 | 333 / 359 |
| office-pm | search | 0.900 / 0.900 | 0.661 / 0.661 | 213 / 219 | 356 / 365 |
| office-pm | ask | 0.900 / 0.900 | 0.661 / 0.661 | 1292 / 1337 | 1512 / 1479 |
| office-pm | retrieve | 0.900 / 0.900 | 0.661 / 0.661 | 189 / 197 | 329 / 361 |

Every per-query rank is identical on all three paths. Top scores differ by at most 0.0002 (recency between runs). Latency moves both ways within run-to-run noise.
