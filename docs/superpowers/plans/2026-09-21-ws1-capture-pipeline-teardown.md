# WS1 Capture Pipeline Teardown Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Break the capture pipeline into 14 measurable stages, measure each on the M1 8 GB reference machine, and improve the most expensive ones with evidence, using best-in-market practice as the reference.

**Architecture:** Measure first, cut second. Extend the telemetry that already exists (`telemetry/runtime_metrics.rs`, `CapturePipelineStats`, `telemetry/system_metrics.rs`) with per-stage percentiles and a baseline report, add a replayable fixture corpus so optimizations are compared on identical input, then replace the costliest stages in order of measured cost. No new metrics layer.

**Tech Stack:** Rust (`src-tauri/src/capture/`, `ocr/`, `telemetry/`), Apple Vision, Accessibility API, ScreenCaptureKit, Python 3 (report script), Make.

**Spec:** `2026-09-21-beta-final-master-plan.md` sections 2, 4, 6, 7. Tickets: CAP-01, CAP-02, CAP-03, CAP-05, CAP-06, CAP-07, CAP-08 plus Final epic E-F1.

## Global Constraints

- Reference machine is Apple M1, 8 GB RAM. All budgets are for it.
- No raw screenshots of real use, database blobs, or tokens in git. Fixtures are synthetic or public content only.
- No commits to `main`. Branch per ticket, merge request per ticket.
- Reuse before adding: extend `runtime_metrics` and `CapturePipelineStats`; do not create a parallel scoreboard.
- Verification: `make test`, plus the ticket's own verify command. Say what you ran.
- No em dashes in code comments, docs, or commit messages.

---

## 1. Method: measure, then cut

1. **Instrument** every stage boundary (CAP-01).
2. **Baseline** with a 2-hour real run plus fixture replay (CAP-02, CAP-03).
3. **Rank** stages by cost: `ticks per hour x p50 ms` gives CPU-milliseconds per hour, and read RSS, wakeups, and bytes written next to it.
4. **Optimize the top three**, one merge request each, each with a before and after table.
5. **Re-measure and file a stage card** (template in section 4) so the improvement is evidence, not a claim.

## 2. Budgets (Apple M1, 8 GB, release build, no VLM loaded)

| Budget | Beta | Final | Source of the number |
|---|---|---|---|
| Idle CPU average over 10 minutes | 3 percent or lower | 2 percent or lower | Screenpipe documents 5 to 10 percent CPU as its own steady state, so 3 percent is a defensible edge (https://docs.screenpipe.com/architecture) |
| Resident memory, VLM unloaded (v1 has no unload path: models are `Box::leak`ed once loaded and the idle-unload stub in `model_worker.rs` has no callers, so measure "before first LLM call" instead until this is changed) | 700 MB or lower | 600 MB or lower | Screenpipe reports about 600 MB (same page) |
| Energy label at idle (`ProcessEnergySnapshot.label`) | `low` | `low` | `system_metrics.rs` |
| Text-only storage growth | measure in W1 | 1 GB per month or lower at 8 hours per day (hypothesis) | FNDR persists no screenshots (ADR 004) while Screenpipe reports 5 to 10 GB per month with JPEG frames |
| Capture to searchable, p95, VLM deferred | 15 seconds or lower | 10 seconds or lower | measure in W1 |
| Frames with one terminal outcome | 100 percent | 100 percent | `CapturePipelineStats` |

## 3. Stage catalog

"Now" describes what the file map shows. Anything marked verify is confirmed during CAP-01, not assumed.

| Stage | v1 location | Now | Problem or unknown | Metric | Best-in-market reference | Candidates | Ticket |
|---|---|---|---|---|---|---|---|
| S0 Trigger and cadence | `run_capture_loop` (`capture/mod.rs:1872`), `capture/sampling.rs`, `config.rs:1061` forced interval 60 s | Timed loop plus forced interval (verify exact scheduling) | Polling wakes the CPU when nothing changed | wakeups per minute, ticks per hour | Screenpipe is event-driven: app switch, click, scroll, typing pause, clipboard, idle fallback about every 5 s | NSWorkspace activation notifications, input-idle counter, active/idle/deep-idle policy (port v2 `SamplingPolicy`, T-308) | E-F1 |
| S1 Foreground context | `capture/macos.rs:22, 250, 298, 310` | `osascript` child process per read of title and URL | Process spawn per tick; per-browser Automation prompt | `capture.context_ms`, spawns per hour | Accessibility API and NSWorkspace, as Screenpipe uses the OS accessibility tree | `AXUIElement` focused-window title and document URL; keep osascript only as fallback where AX fails | CAP-05 |
| S2 Privacy gates before pixels | `privacy/`, `should_skip` (`capture/mod.rs:78`), `capture/permissions.rs` | Blocklist, self-app, surface policy | Are all gates strictly before pixel capture? Private-browsing and secure-input cases unproven | leak test: blocked fixtures store zero rows | v2 pinned a pre-pixel privacy test | Secure-input check, password-manager bundle IDs, tests | CAP-07 |
| S3 Pixel acquisition | `capture/macos.rs:106, 497` | `CGDisplay::screenshot` full display | Deprecated API family; full resolution; multi-display | `capture.pixels_ms`, bytes per frame | ScreenCaptureKit `SCScreenshotManager` is Apple's replacement | Window-scoped capture, downscale, hand the image to Vision without a PNG round trip (port v2 T-302) | CAP-06 |
| S4 Frame dedupe | `capture/dedupe.rs`, `VisualNoveltyTracker` (`capture/mod.rs:78-190`), `img_hash 3.2` | Perceptual hash plus novelty ring | Dedupe ratio and false-drop rate unknown | dedupe ratio, false drops on fixtures | Screenpipe skips identical frames | dHash 9x8 and A-B-A loop detection (port v2 T-303) | CAP-08 |
| S5 Text extraction | `ocr/vision.rs` (1,026 lines) | Apple Vision OCR on the frame | OCR cost per frame; AX text is exact and free but unused | `capture.ocr_ms`, chars per frame, CER | AX tree first with OCR fallback; macOS 26 `RecognizeDocumentsRequest` returns paragraphs, tables, lists | AX-first, OCR changed regions only, recognition level by app class, evaluate `RecognizeDocumentsRequest` | E-F1 |
| S6 Text cleanup | `capture/text_cleanup.rs` (923 lines) | App-aware noise removal | May share defects v2 fixed: byte vs character counts, substring app detection ("Search" matched "arc") | CER and noise precision on fixtures | v2 `fndr-textsignal` | Port the fixes with tests | E-F1 |
| S7 Admission | `capture/admission.rs`, `should_text_heavy_override` (`capture/mod.rs:5563`) | Surface policy and low-signal gate | The 2026-05-17 bug silently dropped every frame | drop-reason histogram, unexplained drops must be 0 | v2 admission as a pure function | Port; alarm when drop rate exceeds 95 percent for 10 minutes | E-F1 |
| S8 Semantic enrichment | `capture_pixel_vlm_route` (`capture/mod.rs:193`), `compose_visual_capture_record` (273), `inference/vlm_router.rs`, `qwen_vl_memory.rs` | Qwen3-VL-2B plus text LLM structure the frame | 3.5 GB RAM on an 8 GB machine; quality unmeasured | ms per frame, peak RAM, deferral rate | See WS2 | WS2 harness and bake-off | MOD-* |
| S9 Validation and grounding | `validate_structured_memory_extraction` (`capture/mod.rs:1197`), `field_supported_by_evidence` (1146) | Strips unsupported fields | Rules tested on few cases | grounding rate on gold set | See WS2 | Gold set | MOD-04, MOD-05 |
| S10 Embedding | `embedding/onnx.rs`, `embedding/chunking.rs`, `inference/model_config.rs` | MiniLM 384-d live, BGE-large 1024-d chunk table | Two contracts; BGE-large is heavy for 8 GB; never ablated | embed ms per chunk, RAM, Recall@5 | Qwen3-Embedding-0.6B was v2's pick | Ablation | E-F3 |
| S11 Merge and continuity | `merge_or_append_memory_record` (`capture/mod.rs:3925`) and helpers to line 4256 | Merges related captures | Merge precision unknown | merge precision on fixtures | v2 continuity port (T-307) | Fixture-driven tests | E-F1 |
| S12 Storage | `storage/lance_store.rs`, `capture.flush_ms` | Batched LanceDB writes | v2's audit found no index on any table; version growth unknown | rows per day, MB per day, query ms | v2 T-203 and T-204 | Scalar, FTS, vector indexes; compaction | E-F1 |
| S13 Downstream queues | `memory_review/`, `tasks`, `graph` | Async review, tasks, graph linking | Contend with capture via `model_pipeline_lock` | queue depth, lock wait ms | v2 model-worker priority queue (T-403) | Replace lock with priority queue | E-F2 |

## 4. Stage card (fill one per stage after measuring)

Save as `docs/evidence/W<nn>/stage-<Sx>.md`. It is also your interview story.

```markdown
# Stage S<n> <name>

- Contract: input, output, what it must never do
- Before: p50, p95, CPU-ms per hour, RAM, bytes (link to the baseline report)
- Change: one paragraph plus the merge request link
- After: same metrics, same machine, same build type
- What we tried and rejected, and why
- Remaining risk
```

---

## Task 1: Per-stage timings with percentiles (CAP-01)

**Files:**
- Modify: `src-tauri/src/telemetry/runtime_metrics.rs` (`Agg`, `AggregateSnapshot`, tests module)
- Modify: `src-tauri/src/capture/mod.rs` (stage boundaries inside `run_capture_loop`)
- Modify: any TypeScript type mirroring `AggregateSnapshot` (found in Step 7)

**Interfaces:**
- Consumes: existing `record_ms(op: &'static str, ms: u64)` and `RuntimeMetrics`.
- Produces: `AggregateSnapshot.p50_ms: u64`, `AggregateSnapshot.p95_ms: u64`, and `pub fn since_ms(op: &'static str, started: std::time::Instant)`. CAP-02 and MOD-15 read these.

- [ ] **Step 1: Write the failing tests**

Add inside `mod tests` in `src-tauri/src/telemetry/runtime_metrics.rs`:

```rust
    #[test]
    fn percentiles_use_nearest_rank_over_the_recent_window() {
        let mut agg = Agg::default();
        for ms in 1..=100u64 {
            agg.record(ms);
        }
        let snap = agg.to_snapshot();
        assert_eq!(snap.p50_ms, 50);
        assert_eq!(snap.p95_ms, 95);
        assert_eq!(snap.n, 100);
        assert_eq!(snap.max_ms, 100);
    }

    #[test]
    fn sample_window_is_bounded() {
        let mut agg = Agg::default();
        for ms in 0..(MAX_SAMPLES as u64 + 100) {
            agg.record(ms);
        }
        assert_eq!(agg.samples.len(), MAX_SAMPLES);
        assert_eq!(agg.percentile(1.0), MAX_SAMPLES as u64 + 99);
    }

    #[test]
    fn empty_aggregate_reports_zero_percentiles() {
        assert_eq!(Agg::default().percentile(0.95), 0);
    }

    #[test]
    fn since_ms_records_one_sample_for_the_op() {
        since_ms("test.since_ms_unique", std::time::Instant::now());
        let (aggregates, _, _) = global().snapshot_inner();
        assert_eq!(aggregates["test.since_ms_unique"].n, 1);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test runtime_metrics::tests -- --nocapture`
Expected: compile FAIL with "no field `p50_ms`", "cannot find value `MAX_SAMPLES`", and "cannot find function `since_ms`".

- [ ] **Step 3: Implement the minimal change**

In `src-tauri/src/telemetry/runtime_metrics.rs` add the constant next to `MAX_RECENT`, extend `Agg`, and extend `to_snapshot`:

```rust
const MAX_SAMPLES: usize = 512;

#[derive(Default, Clone)]
struct Agg {
    n: u64,
    sum_ms: u64,
    max_ms: u64,
    ewma_ms: f64,
    samples: VecDeque<u64>,
}

impl Agg {
    fn record(&mut self, ms: u64) {
        self.n += 1;
        self.sum_ms = self.sum_ms.saturating_add(ms);
        self.max_ms = self.max_ms.max(ms);
        self.ewma_ms = if self.n == 1 {
            ms as f64
        } else {
            EWMA_ALPHA * ms as f64 + (1.0 - EWMA_ALPHA) * self.ewma_ms
        };
        if self.samples.len() == MAX_SAMPLES {
            self.samples.pop_front();
        }
        self.samples.push_back(ms);
    }

    /// Nearest-rank percentile over the most recent `MAX_SAMPLES` samples.
    fn percentile(&self, p: f64) -> u64 {
        if self.samples.is_empty() {
            return 0;
        }
        let mut sorted: Vec<u64> = self.samples.iter().copied().collect();
        sorted.sort_unstable();
        let rank = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
        sorted[rank - 1]
    }

    fn to_snapshot(&self) -> AggregateSnapshot {
        let avg_ms = if self.n > 0 {
            self.sum_ms as f64 / self.n as f64
        } else {
            0.0
        };
        AggregateSnapshot {
            n: self.n,
            sum_ms: self.sum_ms,
            max_ms: self.max_ms,
            avg_ms,
            ewma_ms: self.ewma_ms,
            p50_ms: self.percentile(0.50),
            p95_ms: self.percentile(0.95),
        }
    }
}
```

Add two fields to the DTO (same file):

```rust
#[derive(Debug, Clone, Serialize)]
pub struct AggregateSnapshot {
    pub n: u64,
    pub sum_ms: u64,
    pub max_ms: u64,
    pub avg_ms: f64,
    pub ewma_ms: f64,
    pub p50_ms: u64,
    pub p95_ms: u64,
}
```

Add the helper next to the other free functions (`record_ms`, `bump`):

```rust
/// Record the wall time elapsed since `started` under `op` (for example `capture.ocr_ms`).
pub fn since_ms(op: &'static str, started: std::time::Instant) {
    record_ms(op, started.elapsed().as_millis() as u64);
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test runtime_metrics::tests`
Expected: PASS (all four new tests plus the existing ones).

- [ ] **Step 5: Locate the stage boundaries**

Run:

```bash
grep -n "macos_frontmost_app_name\|CGDisplay\|extract_ocr_text(\|embed_text_inputs_with_memo(\|merge_or_append_memory_record(\|capture_pixel_vlm_route(\|compose_visual_capture_record(" src-tauri/src/capture/mod.rs
```

Read each hit in context. The loop body is lines 1872 to 3846.

- [ ] **Step 6: Wrap each stage boundary using this exact pattern**

Do not change the wrapped call. Add two lines around it:

```rust
let stage_started = std::time::Instant::now();
let ocr_result = /* the existing call, unchanged */;
runtime_metrics::since_ms("capture.ocr_ms", stage_started);
```

Use these op names so CAP-02's report picks them up (its filters are the `capture.` and `mem.` prefixes):

| Stage | Op name |
|---|---|
| Foreground app, title, URL read | `capture.context_ms` |
| Screenshot acquisition | `capture.pixels_ms` |
| Perceptual and novelty dedupe | `capture.dedupe_ms` |
| OCR | `capture.ocr_ms` |
| Cleanup and admission | `capture.cleanup_ms` |
| VLM or LLM enrichment | `capture.semantic_ms` |
| Embedding | `capture.embed_ms` |
| Merge or append | `capture.merge_ms` |

`capture.flush_ms` already exists at `capture/mod.rs:2002`; leave it.

- [ ] **Step 7: Update any TypeScript mirror of the snapshot**

Run: `grep -rn "ewma_ms" src src-tauri/src --include="*.ts" --include="*.tsx" --include="*.rs"`
For each TypeScript hit add `p50_ms: number; p95_ms: number;`. Rust hits other than `runtime_metrics.rs` need the two fields too.

- [ ] **Step 8: Run the full check**

Run: `make test`
Expected: PASS. Then run the app (`npm run tauri dev`), open Pipeline Inspector, and confirm `capture.ocr_ms` shows a p95.

- [ ] **Step 9: Commit**

```bash
git checkout -b feat/cap-01-stage-timings
git add src-tauri/src/telemetry/runtime_metrics.rs src-tauri/src/capture/mod.rs src
git commit -m "feat(telemetry): per-stage capture timings with p50 and p95"
```

## Task 2: Metrics dump and baseline report (CAP-02)

**Files:**
- Modify: `src-tauri/src/lib.rs` (`CapturePipelineStats` methods, near line 229)
- Modify: `src-tauri/src/ipc/commands/stats.rs` (extract `current_runtime_snapshot`)
- Create: `src-tauri/src/telemetry/metrics_dump.rs`
- Modify: `src-tauri/src/telemetry/mod.rs` (declare the module)
- Create: `scripts/bench/summarize_metrics.py`
- Create: `scripts/bench/test_summarize_metrics.py`
- Modify: `Makefile`
- Create (by running): `docs/evidence/W01/capture-baseline.md`

**Interfaces:**
- Consumes: `AggregateSnapshot.p50_ms/p95_ms` from Task 1, `CapturePipelineStats`, `build_snapshot`.
- Produces: NDJSON lines `{"snapshot": RuntimeMetricsSnapshot, "skips": {reason: n}, "totals": {"evaluated": n, "stored": n, "skipped": n}}` and `make capture-baseline`.

- [ ] **Step 1: Write the failing Rust test for skip counts**

Add to the tests in `src-tauri/src/lib.rs` (or the nearest existing test module for `CapturePipelineStats`):

```rust
    #[test]
    fn skip_counts_and_totals_reflect_recorded_events() {
        let stats = CapturePipelineStats::default();
        stats.record_evaluated();
        stats.record_evaluated();
        stats.record_evaluated();
        stats.record_skip(SkipReason::Blocklist, "Example");
        stats.record_skip(SkipReason::PerceptualDup, "Example");
        stats.record_store(StoreOutcome::OcrPath);
        let counts = stats.skip_counts();
        assert_eq!(counts["blocklist"], 1);
        assert_eq!(counts["perceptual_dup"], 1);
        assert_eq!(counts["noise"], 0);
        assert_eq!(counts.len(), 15);
        assert_eq!(stats.evaluated_total(), 3);
        assert_eq!(stats.total_skipped(), 2);
        assert_eq!(stats.total_stored(), 1);
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd src-tauri && cargo test skip_counts_and_totals`
Expected: FAIL with "no method named `skip_counts`" and "no method named `evaluated_total`".

- [ ] **Step 3: Implement the two methods inside `impl CapturePipelineStats`**

```rust
    pub fn evaluated_total(&self) -> u64 {
        self.evaluated.load(Ordering::Relaxed)
    }

    /// Per-reason skip counts keyed by `SkipReason::as_str()`. Cheap (atomic reads only).
    pub fn skip_counts(&self) -> std::collections::BTreeMap<&'static str, u64> {
        let pairs: [(SkipReason, &AtomicU64); 15] = [
            (SkipReason::SelfApp, &self.skipped_self_app),
            (SkipReason::Blocklist, &self.skipped_blocklist),
            (SkipReason::SurfacePolicy, &self.skipped_surface_policy),
            (SkipReason::PerceptualDup, &self.skipped_perceptual_dup),
            (SkipReason::SemanticDup, &self.skipped_semantic_dup),
            (SkipReason::OcrFailed, &self.skipped_ocr_failed),
            (SkipReason::LowSignalText, &self.skipped_low_signal_text),
            (SkipReason::Noise, &self.skipped_noise),
            (SkipReason::Grounding, &self.skipped_grounding),
            (SkipReason::StackedExtraction, &self.skipped_stacked_extraction),
            (SkipReason::VisualSmall, &self.skipped_visual_small),
            (SkipReason::VisualNovelty, &self.skipped_visual_novelty),
            (SkipReason::VisualComposeFailed, &self.skipped_visual_compose_failed),
            (SkipReason::ScreenCaptureFailed, &self.skipped_screen_capture_failed),
            (SkipReason::EmbedderUnavailable, &self.skipped_embedder_unavailable),
        ];
        pairs
            .iter()
            .map(|(reason, counter)| (reason.as_str(), counter.load(Ordering::Relaxed)))
            .collect()
    }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd src-tauri && cargo test skip_counts_and_totals`
Expected: PASS.

- [ ] **Step 5: Extract the snapshot builder so the dump can reuse it**

In `src-tauri/src/ipc/commands/stats.rs`, move the body of `get_runtime_metrics` into a public function and make the command call it:

```rust
pub fn current_runtime_snapshot(
    state: &Arc<AppState>,
) -> crate::telemetry::runtime_metrics::RuntimeMetricsSnapshot {
    let emb = embedding_runtime_status();
    let embedding = crate::telemetry::runtime_metrics::EmbeddingMetricsSnapshot {
        backend: emb.backend,
        degraded: emb.degraded,
        detail: emb.detail,
        model_name: emb.model_name,
        dimension: emb.dimension,
        clip_session_loaded: crate::embedding::clip_session_loaded(),
        last_clip_infer_ms: crate::embedding::last_clip_infer_ms(),
    };
    let inference = crate::telemetry::runtime_metrics::InferenceMetricsSnapshot {
        ai_model_available: state.ai_model_available(),
        ai_model_loaded: state.ai_model_loaded(),
        loaded_model_id: state.loaded_model_id(),
    };
    crate::telemetry::runtime_metrics::build_snapshot(state, embedding, inference)
}

#[tauri::command]
pub async fn get_runtime_metrics(
    state: State<'_, Arc<AppState>>,
) -> Result<crate::telemetry::runtime_metrics::RuntimeMetricsSnapshot, String> {
    Ok(current_runtime_snapshot(state.inner()))
}
```

Run: `cd src-tauri && cargo check`
Expected: compiles. If a type mismatch appears on `state`, match the exact type the existing body used.

- [ ] **Step 6: Create the dump task**

Create `src-tauri/src/telemetry/metrics_dump.rs`:

```rust
//! Optional NDJSON dump of runtime metrics for baseline and soak reports.
//!
//! Enabled only when `FNDR_METRICS_DUMP` names a file. One line per minute. Contains counters and
//! timings only, never captured content.

use crate::AppState;
use serde_json::json;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

pub fn spawn_if_enabled(state: Arc<AppState>) {
    let Some(path) = std::env::var_os("FNDR_METRICS_DUMP") else {
        return;
    };
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let snapshot = crate::ipc::commands::stats::current_runtime_snapshot(&state);
            let stats = &state.capture_stats;
            let line = json!({
                "snapshot": snapshot,
                "skips": stats.skip_counts(),
                "totals": {
                    "evaluated": stats.evaluated_total(),
                    "stored": stats.total_stored(),
                    "skipped": stats.total_skipped(),
                },
            });
            if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                let _ = writeln!(file, "{line}");
            }
        }
    });
}
```

Declare it (`pub mod metrics_dump;`) in `src-tauri/src/telemetry/mod.rs`. Find where the 1 Hz sampler starts and add the call beside it:

Run: `grep -rn "spawn_sampler" src-tauri/src`
Add: `crate::telemetry::metrics_dump::spawn_if_enabled(state.clone());` next to that call.

Run: `cd src-tauri && cargo check`
Expected: compiles. (If `capture_stats` is not directly a field of `AppState`, use the accessor the capture loop uses at `capture/mod.rs:2054`, `state.capture_stats`.)

- [ ] **Step 7: Create the report script and its tests**

Create `scripts/bench/summarize_metrics.py`:

```python
#!/usr/bin/env python3
"""Summarize FNDR runtime-metrics NDJSON into a markdown baseline report.

Each input line is one JSON object written by the app when FNDR_METRICS_DUMP is set:
  {"snapshot": <RuntimeMetricsSnapshot>, "skips": {reason: count}, "totals": {"evaluated": n, "stored": n, "skipped": n}}
The report contains aggregates only. It never contains captured content.
"""
import argparse
import json
import math
import sys
from collections import Counter
from pathlib import Path


def percentile(values, p):
    """Nearest-rank percentile. p is in (0, 1]."""
    if not values:
        return 0.0
    ordered = sorted(values)
    rank = max(1, math.ceil(p * len(ordered)))
    return ordered[rank - 1]


def load(path):
    rows = []
    for line in Path(path).read_text().splitlines():
        line = line.strip()
        if line:
            rows.append(json.loads(line))
    return rows


def summarize(rows, machine, build):
    if not rows:
        raise ValueError("no samples in input")
    first = rows[0]["snapshot"]
    last_row = rows[-1]
    last = last_row["snapshot"]
    minutes = (last["generated_at_ms"] - first["generated_at_ms"]) / 60000.0
    cpu = [r["snapshot"]["system"]["process_cpu"]["cpu_percent"] for r in rows]
    rss = [r["snapshot"]["system"]["process_memory"]["rss_bytes"] for r in rows]
    footprint = [r["snapshot"]["system"]["process_memory"]["phys_footprint_bytes"] for r in rows]
    energy = Counter(r["snapshot"]["system"]["process_energy"]["label"] for r in rows)
    stages = {
        op: agg for op, agg in last["aggregates"].items() if op.startswith(("capture.", "mem."))
    }
    totals = last_row["totals"]
    skips = last_row["skips"]
    unexplained = totals["evaluated"] - totals["stored"] - totals["skipped"]

    mb = 1024 * 1024
    lines = [
        "# Capture pipeline baseline",
        "",
        f"- Machine: {machine}",
        f"- Build: {build}",
        f"- Duration: {minutes:.1f} minutes, {len(rows)} samples",
        f"- CPU: avg {sum(cpu) / len(cpu):.2f} percent, p95 {percentile(cpu, 0.95):.2f} percent",
        f"- RSS: peak {max(rss) / mb:.0f} MB, end {rss[-1] / mb:.0f} MB, growth {(rss[-1] - rss[0]) / mb:+.0f} MB",
        f"- Physical footprint: peak {max(footprint) / mb:.0f} MB",
        "- Energy label: " + ", ".join(f"{k} x{v}" for k, v in energy.most_common()),
        f"- Frames evaluated {totals['evaluated']}, stored {totals['stored']}, skipped {totals['skipped']}",
        f"- Unexplained drops: {unexplained}",
        "",
        "## Per-stage latency (ms)",
        "",
        "| Stage | n | avg | p50 | p95 | max |",
        "|---|---|---|---|---|---|",
    ]
    for op in sorted(stages):
        a = stages[op]
        lines.append(
            f"| {op} | {a['n']} | {a['avg_ms']:.1f} | {a['p50_ms']} | {a['p95_ms']} | {a['max_ms']} |"
        )
    lines += ["", "## Skip reasons", "", "| Reason | Count | Share of evaluated |", "|---|---|---|"]
    evaluated = max(totals["evaluated"], 1)
    for reason, count in sorted(skips.items(), key=lambda kv: -kv[1]):
        if count:
            lines.append(f"| {reason} | {count} | {100.0 * count / evaluated:.1f} percent |")
    outcomes = {k: v for k, v in last.get("counters", {}).items() if k.startswith("mem.outcome.")}
    if outcomes:
        lines += ["", "## Memory outcomes", "", "| Outcome | Count |", "|---|---|"]
        for key, count in sorted(outcomes.items()):
            lines.append(f"| {key[len('mem.outcome.'):]} | {count} |")
    return "\n".join(lines) + "\n"


def main(argv):
    parser = argparse.ArgumentParser()
    parser.add_argument("input")
    parser.add_argument("--out", required=True)
    parser.add_argument("--machine", default="unknown")
    parser.add_argument("--build", default="release")
    args = parser.parse_args(argv)
    report = summarize(load(args.input), args.machine, args.build)
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(report)
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
```

Create `scripts/bench/test_summarize_metrics.py`:

```python
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import summarize_metrics as sm


def row(ts_ms, cpu, rss_mb, evaluated, stored, skipped, skips):
    return {
        "snapshot": {
            "generated_at_ms": ts_ms,
            "aggregates": {
                "capture.ocr_ms": {"n": 10, "avg_ms": 41.5, "p50_ms": 40, "p95_ms": 88, "max_ms": 120},
                "capture.flush_ms": {"n": 3, "avg_ms": 5.0, "p50_ms": 5, "p95_ms": 6, "max_ms": 6},
                "mem.embed_ms": {"n": 4, "avg_ms": 90.0, "p50_ms": 88, "p95_ms": 140, "max_ms": 150},
                "embedding.other_ms": {"n": 1, "avg_ms": 1.0, "p50_ms": 1, "p95_ms": 1, "max_ms": 1},
            },
            "counters": {"mem.outcome.new": 5, "mem.outcome.merged_persisted": 3, "other.counter": 9},
            "system": {
                "process_cpu": {"cpu_percent": cpu},
                "process_memory": {"rss_bytes": rss_mb * 1024 * 1024, "phys_footprint_bytes": rss_mb * 1024 * 1024},
                "process_energy": {"label": "low"},
            },
        },
        "skips": skips,
        "totals": {"evaluated": evaluated, "stored": stored, "skipped": skipped},
    }


class SummarizeTests(unittest.TestCase):
    def test_percentile_is_nearest_rank(self):
        self.assertEqual(sm.percentile(list(range(1, 101)), 0.95), 95)
        self.assertEqual(sm.percentile(list(range(1, 101)), 0.5), 50)
        self.assertEqual(sm.percentile([], 0.95), 0.0)

    def test_report_has_stage_table_cpu_and_no_unexplained_drops(self):
        rows = [
            row(0, 2.0, 400, 0, 0, 0, {"blocklist": 0}),
            row(600000, 4.0, 430, 20, 8, 12, {"blocklist": 2, "perceptual_dup": 10}),
        ]
        text = sm.summarize(rows, "Apple M1, 8 GB", "release")
        self.assertIn("| capture.ocr_ms | 10 | 41.5 | 40 | 88 | 120 |", text)
        self.assertIn("| mem.embed_ms | 4 | 90.0 | 88 | 140 | 150 |", text)
        self.assertIn("| new | 5 |", text)
        self.assertIn("| merged_persisted | 3 |", text)
        self.assertNotIn("other.counter", text)
        self.assertNotIn("embedding.other_ms", text)
        self.assertIn("CPU: avg 3.00 percent", text)
        self.assertIn("Duration: 10.0 minutes", text)
        self.assertIn("Unexplained drops: 0", text)
        self.assertIn("| perceptual_dup | 10 | 50.0 percent |", text)
        self.assertNotIn("| blocklist | 0 |", text)

    def test_unexplained_drops_are_surfaced(self):
        rows = [row(0, 1.0, 400, 10, 2, 3, {}), row(60000, 1.0, 400, 10, 2, 3, {})]
        self.assertIn("Unexplained drops: 5", sm.summarize(rows, "m", "release"))

    def test_cli_roundtrip(self):
        with tempfile.TemporaryDirectory() as d:
            src = Path(d) / "m.ndjson"
            rows = [row(0, 1.0, 400, 1, 1, 0, {}), row(60000, 1.0, 410, 2, 2, 0, {})]
            src.write_text("\n".join(json.dumps(r) for r in rows) + "\n")
            out = Path(d) / "out" / "report.md"
            self.assertEqual(sm.main([str(src), "--out", str(out), "--machine", "M1"]), 0)
            self.assertTrue(out.read_text().startswith("# Capture pipeline baseline"))

    def test_empty_input_is_an_error(self):
        with self.assertRaises(ValueError):
            sm.summarize([], "m", "release")


if __name__ == "__main__":
    unittest.main()
```

- [ ] **Step 8: Run the script tests**

Run: `python3 scripts/bench/test_summarize_metrics.py -v`
Expected: 5 tests, all `ok`. (These exact files were run before this plan was written and passed.)

- [ ] **Step 9: Add the Make target**

Append to `Makefile` and add `capture-baseline` to the `.PHONY` line:

```make
capture-baseline:
	@test -n "$(METRICS)" || (echo "usage: make capture-baseline METRICS=/path/m.ndjson OUT=docs/evidence/W01/capture-baseline.md" && exit 1)
	python3 scripts/bench/summarize_metrics.py "$(METRICS)" --out "$(OUT)" --machine "$$(sysctl -n machdep.cpu.brand_string), $$(( $$(sysctl -n hw.memsize) / 1073741824 )) GB" --build release
```

- [ ] **Step 10: Take the real baseline (release build, not dev)**

Dev builds are much slower and not comparable. Build a release app, then run it for two hours doing normal work:

```bash
npm run tauri build
open --env FNDR_METRICS_DUMP="$HOME/fndr-metrics.ndjson" -a src-tauri/target/release/bundle/macos/FNDR.app
```

After two hours quit FNDR, then:

```bash
make capture-baseline METRICS="$HOME/fndr-metrics.ndjson" OUT=docs/evidence/W01/capture-baseline.md
```

Expected: the file exists with a stage table, a skip-reason table, and `Unexplained drops:` on its own line. If unexplained drops are not zero, that is the first finding: write it up as a bug, do not tune around it.

- [ ] **Step 11: Privacy check before committing the report**

Run: `grep -in "http\|@\|password" docs/evidence/W01/capture-baseline.md`
Expected: no output. The report contains counts and timings only.

- [ ] **Step 12: Commit**

```bash
git checkout -b feat/cap-02-baseline-report
git add src-tauri/src scripts/bench Makefile docs/evidence/W01/capture-baseline.md
git commit -m "feat(telemetry): metrics NDJSON dump and make capture-baseline report"
```

## Task 3: Synthetic fixture corpus with a character-error-rate test (CAP-03)

**Files:**
- Create: `src-tauri/tests/fixtures/screens/manifest.json`
- Create: `src-tauri/tests/fixtures/screens/*.png` (30 files)
- Create: `src-tauri/tests/fixtures/screens/README.md`
- Create: `src-tauri/tests/support/cer.rs`
- Create: `src-tauri/tests/capture_fixtures.rs`

**Interfaces:**
- Produces: `char_error_rate(reference: &str, hypothesis: &str) -> f64` and `manifest.json` entries `{id, app_class, bundle_id, window_title, expected_outcome, expected_text, cer_budget}`. CAP-08 and MOD-04 reuse the corpus.

- [ ] **Step 1: Write the failing unit tests for the metric**

Create `src-tauri/tests/support/cer.rs`:

```rust
/// Character error rate: Levenshtein distance over characters divided by reference length.
pub fn char_error_rate(reference: &str, hypothesis: &str) -> f64 {
    let r: Vec<char> = reference.chars().collect();
    let h: Vec<char> = hypothesis.chars().collect();
    if r.is_empty() {
        return if h.is_empty() { 0.0 } else { 1.0 };
    }
    let mut prev: Vec<usize> = (0..=h.len()).collect();
    for (i, rc) in r.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, hc) in h.iter().enumerate() {
            let cost = if rc == hc { 0 } else { 1 };
            cur.push((prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[h.len()] as f64 / r.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_is_zero_and_one_substitution_in_ten_is_a_tenth() {
        assert_eq!(char_error_rate("abcdefghij", "abcdefghij"), 0.0);
        assert!((char_error_rate("abcdefghij", "abcdeXghij") - 0.1).abs() < 1e-9);
    }

    #[test]
    fn empty_reference_and_unicode_are_handled() {
        assert_eq!(char_error_rate("", ""), 0.0);
        assert_eq!(char_error_rate("", "x"), 1.0);
        assert_eq!(char_error_rate("naïve café", "naïve café"), 0.0);
    }
}
```

(These exact tests were compiled and run standalone before this plan was written and passed.)

- [ ] **Step 2: Create the fixture set (30 images)**

Every image is synthetic or public content, never a real capture of your own work. Save each as PNG at 1440x900 and add its entry to `manifest.json`.

| Class | Count | How to produce |
|---|---|---|
| `editor` | 5 | Open a file from a public open-source repo in your editor and screenshot it |
| `terminal` | 5 | Run `cargo test` or `npm test` in a public repo and screenshot the output |
| `browser_article` | 5 | Screenshot public documentation pages |
| `chat_mock` | 5 | Screenshot an HTML mock chat you write yourself with invented messages |
| `pdf_paper` | 5 | Screenshot pages of a public arXiv paper |
| `privacy_negative` | 5 | Mock pages titled "Example Bank - Sign in", a mock password-manager window, a mock private-browsing window, a mock secure-input login, and the FNDR window itself |

`manifest.json` entry shape:

```json
{
  "id": "terminal-03",
  "file": "terminal-03.png",
  "app_class": "terminal",
  "bundle_id": "com.apple.Terminal",
  "window_title": "cargo test",
  "expected_outcome": "store",
  "expected_text": "running 12 tests\ntest result: ok. 12 passed; 0 failed",
  "cer_budget": 0.15
}
```

`expected_outcome` is `store` or `skip:<SkipReason as_str>` (for example `skip:blocklist`). `cer_budget` starts at 0.15 and is tightened after the first run.

- [ ] **Step 3: Write the failing integration test**

Create `src-tauri/tests/capture_fixtures.rs`:

```rust
#[path = "support/cer.rs"]
mod cer;

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Fixture {
    id: String,
    file: String,
    app_class: String,
    expected_outcome: String,
    expected_text: String,
    cer_budget: f64,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/screens")
}

fn load_manifest() -> Vec<Fixture> {
    let raw = std::fs::read_to_string(fixtures_dir().join("manifest.json")).expect("manifest.json");
    serde_json::from_str(&raw).expect("manifest parses")
}

#[test]
fn corpus_has_thirty_screens_across_six_classes() {
    let fixtures = load_manifest();
    assert_eq!(fixtures.len(), 30);
    for class in ["editor", "terminal", "browser_article", "chat_mock", "pdf_paper", "privacy_negative"] {
        assert_eq!(fixtures.iter().filter(|f| f.app_class == class).count(), 5, "class {class}");
    }
    for f in &fixtures {
        assert!(fixtures_dir().join(&f.file).exists(), "missing image for {}", f.id);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn ocr_plus_cleanup_stays_within_each_fixtures_cer_budget() {
    // Find the public OCR entry point the capture loop uses:
    //   grep -n "pub fn\|pub async fn" src/ocr/vision.rs
    // Then call it here on each PNG, run the same cleanup the loop runs, and compare.
    let mut failures = Vec::new();
    for f in load_manifest().iter().filter(|f| f.expected_outcome == "store") {
        let recognized = recognize_and_clean(&fixtures_dir().join(&f.file));
        let cer = cer::char_error_rate(&f.expected_text, &recognized);
        println!("{:<20} class={:<16} cer={:.3} budget={:.3}", f.id, f.app_class, cer, f.cer_budget);
        if cer > f.cer_budget {
            failures.push(format!("{} cer {:.3} > {:.3}", f.id, cer, f.cer_budget));
        }
    }
    assert!(failures.is_empty(), "over budget: {failures:?}");
}

#[cfg(target_os = "macos")]
fn recognize_and_clean(path: &std::path::Path) -> String {
    let _ = path;
    unimplemented!("wire to the OCR entry point named in the comment above")
}
```

The `recognize_and_clean` body is intentionally the one piece you write against the real OCR signature you find with the `grep` in the comment; the test is red until then, which is the point of Step 4.

- [ ] **Step 4: Run to verify it fails for the right reason**

Run: `cd src-tauri && cargo test --test capture_fixtures`
Expected: `corpus_has_thirty_screens_across_six_classes` FAILS until all 30 files and entries exist; the OCR test panics with `unimplemented`.

- [ ] **Step 5: Implement `recognize_and_clean` against the real API, then tighten budgets**

Read `src-tauri/src/ocr/vision.rs`, call its public recognize function, run the loop's cleanup (`text_cleanup.rs`), return the string. Run once, copy each printed CER, and set that fixture's `cer_budget` to its observed CER plus 0.05.

Run: `cd src-tauri && cargo test --test capture_fixtures -- --nocapture`
Expected: PASS and a printed CER table. Save that table into `docs/evidence/W01/ocr-cer.md`.

- [ ] **Step 6: Commit**

```bash
git checkout -b feat/cap-03-fixture-corpus
git add src-tauri/tests
git commit -m "test(capture): 30-screen synthetic fixture corpus with CER budgets"
```

## Task 4: Native window title and URL, no per-tick osascript (CAP-05)

**Files:**
- Create: `src-tauri/src/capture/ax_context.rs`
- Modify: `src-tauri/src/capture/macos.rs:250, 298, 382` (call the new source first, osascript second)
- Modify: `src-tauri/src/capture/permissions.rs` (Accessibility permission status)
- Modify: `src-tauri/src/capture/mod.rs` (declare the module)

**Interfaces:**
- Consumes: baseline `capture.context_ms` from Task 2.
- Produces: `pub trait ContextSource { fn window_title(&self, bundle_id: &str) -> Option<String>; fn browser_url(&self, bundle_id: &str) -> Option<String>; }`, `AxContextSource` (real), and `FallbackContextSource<A, B>` that tries `A` then `B`.

**Reuse first:** `src-tauri/src/accessibility/mod.rs` (806 lines) already declares the AXUIElement types and FFI and reads the focused input field for autofill. Extend that module (or call into it from `ax_context.rs`); do not declare the FFI a second time.

**Trade-off to state in the merge request:** the Accessibility API needs the Accessibility permission where osascript needs a per-browser Automation prompt. Autofill already depends on Accessibility, so confirm in `capture/permissions.rs` and onboarding whether users already grant it; if they do, this ticket adds no new prompt. The same permission later enables AX-first text extraction (S5).

- [ ] **Step 1: Write the failing test for the fallback logic (pure, no macOS APIs)**

Add to `src-tauri/src/capture/ax_context.rs`:

```rust
pub trait ContextSource {
    fn window_title(&self, bundle_id: &str) -> Option<String>;
    fn browser_url(&self, bundle_id: &str) -> Option<String>;
}

/// Tries `primary`, then `fallback`. Empty strings count as missing.
pub struct FallbackContextSource<A: ContextSource, B: ContextSource> {
    pub primary: A,
    pub fallback: B,
}

impl<A: ContextSource, B: ContextSource> ContextSource for FallbackContextSource<A, B> {
    fn window_title(&self, bundle_id: &str) -> Option<String> {
        self.primary
            .window_title(bundle_id)
            .filter(|t| !t.is_empty())
            .or_else(|| self.fallback.window_title(bundle_id).filter(|t| !t.is_empty()))
    }
    fn browser_url(&self, bundle_id: &str) -> Option<String> {
        self.primary
            .browser_url(bundle_id)
            .filter(|u| !u.is_empty())
            .or_else(|| self.fallback.browser_url(bundle_id).filter(|u| !u.is_empty()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Fake {
        title: Option<&'static str>,
        url: Option<&'static str>,
        calls: Cell<u32>,
    }
    impl ContextSource for Fake {
        fn window_title(&self, _: &str) -> Option<String> {
            self.calls.set(self.calls.get() + 1);
            self.title.map(str::to_string)
        }
        fn browser_url(&self, _: &str) -> Option<String> {
            self.calls.set(self.calls.get() + 1);
            self.url.map(str::to_string)
        }
    }
    fn fake(title: Option<&'static str>, url: Option<&'static str>) -> Fake {
        Fake { title, url, calls: Cell::new(0) }
    }

    #[test]
    fn primary_wins_and_fallback_is_never_called() {
        let src = FallbackContextSource { primary: fake(Some("A"), Some("https://a")), fallback: fake(Some("B"), Some("https://b")) };
        assert_eq!(src.window_title("x").as_deref(), Some("A"));
        assert_eq!(src.browser_url("x").as_deref(), Some("https://a"));
        assert_eq!(src.fallback.calls.get(), 0);
    }

    #[test]
    fn empty_or_missing_primary_falls_back() {
        let src = FallbackContextSource { primary: fake(Some(""), None), fallback: fake(Some("B"), Some("https://b")) };
        assert_eq!(src.window_title("x").as_deref(), Some("B"));
        assert_eq!(src.browser_url("x").as_deref(), Some("https://b"));
    }

    #[test]
    fn both_missing_returns_none() {
        let src = FallbackContextSource { primary: fake(None, None), fallback: fake(Some(""), None) };
        assert_eq!(src.window_title("x"), None);
        assert_eq!(src.browser_url("x"), None);
    }
}
```

- [ ] **Step 2: Run to verify it fails, then passes**

Run: `cd src-tauri && cargo test ax_context`
Expected: first run FAILS because the module is not declared; after adding `pub mod ax_context;` to `capture/mod.rs` it PASSES (3 tests).

- [ ] **Step 3: Time-boxed spike (4 hours) for the real AX implementation**

Goal: read the focused window's title and, for Safari, Chrome, Arc, and Firefox, its document URL through `AXUIElement`. Read `accessibility/mod.rs` first and reuse its `ApplicationServices` bindings (add `AXUIElementCopyAttributeValue` reads only if they are missing), then read `kAXFocusedWindowAttribute`, `kAXTitleAttribute`, and `kAXDocumentAttribute`. Where a browser does not expose the URL through AX, keep osascript for that browser only. Write the outcome per browser into `docs/evidence/W02/ax-context-findings.md` as a table: browser, title via AX (yes or no), URL via AX (yes or no), fallback needed.

- [ ] **Step 4: Implement `AxContextSource` behind `#[cfg(target_os = "macos")]` and wire the fallback**

In `capture/macos.rs` construct `FallbackContextSource { primary: AxContextSource, fallback: OsascriptContextSource }`, where `OsascriptContextSource` wraps the existing `run_bounded_osascript` calls unchanged. Time both paths with `since_ms("capture.context_ms", ...)`.

- [ ] **Step 5: Measure before and after**

Take a 30-minute release-build run with the old path (already in the baseline) and one with the new path. Put both rows in `docs/evidence/W02/cap-05-before-after.md`:

```markdown
| Metric | Before (osascript) | After (AX first) |
|---|---|---|
| capture.context_ms p50 / p95 | | |
| Idle wakeups per minute | | |
| CPU average | | |
```

Fill the cells from the two reports. Expected: p95 drops; if it does not, say so and keep the finding.

- [ ] **Step 6: Verify and commit**

Run: `cd src-tauri && cargo test macos ax_context` and `make test`.

```bash
git checkout -b feat/cap-05-native-context
git add src-tauri/src/capture docs/evidence/W02
git commit -m "feat(capture): read window title and URL via Accessibility, osascript as fallback"
```

## Task 5: Privacy Proof data over IPC (CAP-07)

**Files:**
- Create: `src-tauri/src/privacy_proof.rs`
- Modify: `src-tauri/src/http_util.rs` (count outbound requests)
- Modify: `src-tauri/src/lib.rs` (declare module, register command, `pub use` glob per the companion gotcha)
- Test: inline `#[cfg(test)]` in `privacy_proof.rs`

**Interfaces:**
- Consumes: `CapturePipelineStats::skip_counts()`, `evaluated_total()`, `total_stored()` from Task 2.
- Produces: `#[derive(Serialize)] pub struct PrivacyProof { evaluated: u64, stored: u64, skipped_by_reason: BTreeMap<String, u64>, egress_requests: u64, egress_hosts: Vec<String> }`, `pub fn record_egress(host: &str)`, and the Tauri command `get_privacy_proof`. FEA-05 renders this.

Two facts before writing code: Tauri command re-exports must use a glob (`pub use privacy_proof::*;`) or the macro-generated symbols are not visible; and `http_util.rs` already builds the bounded `reqwest` clients, so counting there covers downloads and local probes.

- [ ] **Step 1: Create `src-tauri/src/privacy_proof.rs` containing only these tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CapturePipelineStats, SkipReason, StoreOutcome};

    #[test]
    fn proof_reports_skips_by_reason_and_never_carries_content() {
        let stats = CapturePipelineStats::default();
        stats.record_evaluated();
        stats.record_evaluated();
        stats.record_skip(SkipReason::Blocklist, "Example Bank");
        stats.record_store(StoreOutcome::OcrPath);
        let proof = build_privacy_proof(&stats);
        assert_eq!(proof.evaluated, 2);
        assert_eq!(proof.stored, 1);
        assert_eq!(proof.skipped_by_reason["blocklist"], 1);
        let json = serde_json::to_string(&proof).unwrap();
        assert!(!json.contains("Example Bank"), "app names must not leak into the proof");
    }

    #[test]
    fn egress_counts_requests_and_keeps_only_hosts() {
        let before = build_privacy_proof(&CapturePipelineStats::default()).egress_requests;
        record_egress("huggingface.co");
        record_egress("huggingface.co");
        let proof = build_privacy_proof(&CapturePipelineStats::default());
        assert!(proof.egress_requests >= before + 2);
        assert!(proof.egress_hosts.contains(&"huggingface.co".to_string()));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Declare the module (`pub mod privacy_proof;` in `src-tauri/src/lib.rs`), then run: `cd src-tauri && cargo test privacy_proof`
Expected: compile FAIL with "cannot find function `build_privacy_proof`" and "cannot find function `record_egress`".

- [ ] **Step 2b: Write the implementation above the test module in the same file**

```rust
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

use parking_lot::Mutex;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PrivacyProof {
    pub evaluated: u64,
    pub stored: u64,
    pub skipped_by_reason: BTreeMap<String, u64>,
    pub egress_requests: u64,
    pub egress_hosts: Vec<String>,
}

static EGRESS_REQUESTS: AtomicU64 = AtomicU64::new(0);
static EGRESS_HOSTS: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();

fn hosts() -> &'static Mutex<BTreeSet<String>> {
    EGRESS_HOSTS.get_or_init(|| Mutex::new(BTreeSet::new()))
}

/// Count one outbound HTTP request. Records the host only, never the path or query.
pub fn record_egress(host: &str) {
    EGRESS_REQUESTS.fetch_add(1, Ordering::Relaxed);
    hosts().lock().insert(host.to_string());
}

pub fn build_privacy_proof(stats: &crate::CapturePipelineStats) -> PrivacyProof {
    PrivacyProof {
        evaluated: stats.evaluated_total(),
        stored: stats.total_stored(),
        skipped_by_reason: stats
            .skip_counts()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        egress_requests: EGRESS_REQUESTS.load(Ordering::Relaxed),
        egress_hosts: hosts().lock().iter().cloned().collect(),
    }
}
```

Run: `cd src-tauri && cargo test privacy_proof`
Expected: PASS (2 tests).

- [ ] **Step 3: Add the command and register it**

```rust
#[tauri::command]
pub async fn get_privacy_proof(
    state: tauri::State<'_, std::sync::Arc<crate::AppState>>,
) -> Result<PrivacyProof, String> {
    Ok(build_privacy_proof(&state.capture_stats))
}
```

Add `pub use privacy_proof::*;` in `lib.rs` and add `get_privacy_proof` to the `generate_handler!` list (find it with `grep -n "generate_handler" src-tauri/src/lib.rs`).

- [ ] **Step 4: Count real egress**

Find every outbound call site:

Run: `grep -rn "reqwest::\|Client::builder\|\.send()" src-tauri/src --include="*.rs"`

In `http_util.rs`, call `crate::privacy_proof::record_egress(host)` inside the helper each caller uses. Any call site that bypasses the helper is itself a finding: route it through the helper.

- [ ] **Step 5: Verify and commit**

Run: `cd src-tauri && cargo test privacy_proof && cd .. && make test`
Expected: PASS.

```bash
git checkout -b feat/cap-07-privacy-proof-data
git add src-tauri/src
git commit -m "feat(privacy): get_privacy_proof reports skips by reason and egress counts"
```

## Task 6: Dedupe upgrade, dHash plus A-B-A (CAP-08)

**Files:**
- Modify: `src-tauri/src/capture/dedupe.rs`
- Modify: `src-tauri/tests/fixtures/screens/` (add `sequences.json`)
- Test: `src-tauri/tests/capture_fixtures.rs` (add a dedupe test) and inline unit tests

**Interfaces:**
- Consumes: the fixture corpus from Task 3.
- Produces: `luma_9x8_from_rgba(rgba: &[u8], width: usize, height: usize) -> [u8; 72]`, `dhash_9x8(luma: &[u8; 72]) -> u64`, `hamming(a: u64, b: u64) -> u32`, `is_aba(recent: &VecDeque<u64>, hash: u64, threshold: u32) -> bool`.

Read the v2 implementation first: `~/FNDR-2.0/crates/fndr-capture/src/dedup.rs` (T-303). Port only these functions, add a `// Ported from FNDR v2 crates/fndr-capture/src/dedup.rs` note.

- [ ] **Step 1: Write the failing tests**

Add to `capture/dedupe.rs`:

```rust
use std::collections::VecDeque;

#[cfg(test)]
mod dhash_tests {
    use super::*;

    #[test]
    fn dhash_identical_zero_and_reversed_gradient_is_max_distance() {
        let mut inc = [0u8; 72];
        let mut dec = [0u8; 72];
        for row in 0..8 {
            for col in 0..9 {
                inc[row * 9 + col] = (col * 10) as u8;
                dec[row * 9 + col] = (250 - col * 10) as u8;
            }
        }
        assert_eq!(hamming(dhash_9x8(&inc), dhash_9x8(&inc)), 0);
        assert_eq!(hamming(dhash_9x8(&inc), dhash_9x8(&dec)), 64);
    }

    #[test]
    fn solid_color_frames_hash_identically() {
        let rgba = vec![200u8; 64 * 48 * 4];
        assert_eq!(dhash_9x8(&luma_9x8_from_rgba(&rgba, 64, 48)), 0);
    }

    #[test]
    fn aba_detects_flicker_but_not_progress() {
        let (a, b) = (0u64, u64::MAX);
        let mut recent = VecDeque::new();
        recent.push_back(a);
        recent.push_back(b);
        assert!(is_aba(&recent, a, 4));
        assert!(!is_aba(&recent, b, 4));
        let mut single = VecDeque::new();
        single.push_back(a);
        assert!(!is_aba(&single, a, 4));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd src-tauri && cargo test dhash_tests`
Expected: FAIL with "cannot find function `dhash_9x8`".

- [ ] **Step 3: Implement**

```rust
/// Sample a 9x8 luma grid from RGBA pixels (nearest neighbour, no full-resolution decode).
pub fn luma_9x8_from_rgba(rgba: &[u8], width: usize, height: usize) -> [u8; 72] {
    let mut out = [0u8; 72];
    for row in 0..8 {
        for col in 0..9 {
            let x = (col * (width - 1)) / 8;
            let y = (row * (height - 1)) / 7;
            let i = (y * width + x) * 4;
            let (r, g, b) = (rgba[i] as f32, rgba[i + 1] as f32, rgba[i + 2] as f32);
            out[row * 9 + col] = (0.299 * r + 0.587 * g + 0.114 * b) as u8;
        }
    }
    out
}

/// Difference hash: one bit per horizontally adjacent pair of the 9x8 grid.
pub fn dhash_9x8(luma: &[u8; 72]) -> u64 {
    let mut hash = 0u64;
    for row in 0..8 {
        for col in 0..8 {
            if luma[row * 9 + col] > luma[row * 9 + col + 1] {
                hash |= 1u64 << (row * 8 + col);
            }
        }
    }
    hash
}

pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// True when `hash` matches the frame two steps back but not the previous frame (A-B-A flicker).
pub fn is_aba(recent: &VecDeque<u64>, hash: u64, threshold: u32) -> bool {
    if recent.len() < 2 {
        return false;
    }
    let previous = recent[recent.len() - 1];
    let two_back = recent[recent.len() - 2];
    hamming(hash, two_back) <= threshold && hamming(hash, previous) > threshold
}
```

- [ ] **Step 3b: Run the unit tests**

Run: `cd src-tauri && cargo test dhash_tests`
Expected: PASS (3 tests; these exact functions were compiled and run standalone before this plan was written).

- [ ] **Step 4: Create dedupe sequences and the acceptance test**

Create six sequences of ten frames from the fixture corpus, each with the expected keep flags, in `sequences.json`:

| Sequence | Construction | Expected |
|---|---|---|
| `idle` | one screen repeated 10 times | keep 1 |
| `cursor_blink` | one screen, cursor pixel toggled | keep 1 |
| `clock_tick` | one screen, clock digits change | keep 1 |
| `scroll` | article scrolled by 40 px per frame | keep all 10 |
| `tab_flicker` | A B A B A B pattern | keep 2 |
| `typing` | terminal line grows by one word per frame | keep all 10 |

Add a test asserting the new dedupe keeps exactly the expected frames for every sequence, with zero false drops (no `scroll` or `typing` frame dropped), and prints the dedupe ratio per sequence. Save the printed table to `docs/evidence/W03/cap-08-before-after.md` together with the same table produced by the old `img_hash` path.

- [ ] **Step 5: Verify and commit**

Run: `cd src-tauri && cargo test capture_fixtures dhash_tests && cd .. && make test`

```bash
git checkout -b feat/cap-08-dedupe-upgrade
git add src-tauri docs/evidence/W03
git commit -m "feat(capture): dHash and A-B-A dedupe ported from v2, measured on fixtures"
```

## Task 7: ScreenCaptureKit one-shot capture (CAP-06, P1)

**Files:**
- Create: `src-tauri/src/capture/sck.rs`
- Modify: `src-tauri/src/capture/macos.rs:106` (backend switch)
- Modify: `src-tauri/Cargo.toml` (dependency)

**Interfaces:**
- Consumes: `capture.pixels_ms` baseline (Task 2).
- Produces: env switch `FNDR_CAPTURE_BACKEND` with values `cg` (default until proven) and `sck`.

Read `~/FNDR-2.0/crates/fndr-capture/src/source.rs` first. v2 pinned `screencapturekit = "=9.0.1"` and needed a `.cargo/config.toml` rpath for the crate's Swift shim, and hit a macOS 26.1 bundle and TCC quirk that the T-310 soak still has open. Expect the same friction. This is why the ticket is P1.

- [ ] **Step 1: Time-boxed spike (4 hours)**

Can this repo build with that crate, capture one frame through `SCScreenshotManager`, and hand back the same image type `CGDisplay::screenshot` returns today? Write the result in `docs/evidence/W04/sck-spike.md` as go or no-go with the reason.

- [ ] **Step 2 (go only): implement behind the env switch, default `cg`**

Both backends produce the same image type. `capture.pixels_ms` is recorded for whichever is active.

- [ ] **Step 3: Compare**

Run two 30-minute release-build sessions, one per backend, and record `capture.pixels_ms` p50 and p95, CPU, and RSS in `docs/evidence/W04/cap-06-before-after.md`. Run the permission-recovery checklist by hand: revoke Screen Recording in System Settings, confirm capture reports `ScreenCaptureFailed` and does not crash, re-grant, confirm capture resumes without relaunch.

- [ ] **Step 4: Flip the default only if both hold**

`sck` p95 is not worse than `cg`, and the recovery checklist passes. Otherwise keep `cg` and record why. Either outcome is a valid, evidenced result.

- [ ] **Step 5: Commit**

```bash
git checkout -b feat/cap-06-screencapturekit
git add src-tauri docs/evidence/W04
git commit -m "feat(capture): ScreenCaptureKit backend behind FNDR_CAPTURE_BACKEND"
```

## Final epic E-F1 (W5 to W12): decompose at the Beta retro

Order by the measured cost ranking from the baseline, not by this list. Candidates with acceptance:

| Item | Acceptance |
|---|---|
| Event-driven triggers: NSWorkspace activation, typing pause, idle fallback | Idle wakeups per minute drop by at least half vs the baseline report |
| AX-first text with OCR fallback | Skipped OCR percentage reported; CER on fixtures not worse |
| Vision `RecognizeDocumentsRequest` evaluation (macOS 26) | Table, list, paragraph fidelity vs current OCR on fixtures, with a go or no-go |
| Port v2 adaptive `SamplingPolicy` | Simulated-timeline test passes; idle CPU at or below 2 percent |
| Port v2 text-signal fixes (character counts, `[LOW_CONF]`, bundle-first app identity) | Fixture regressions for "Search contains arc" style bugs pass |
| Admission as a pure function plus drop-rate alarm | Alarm fires in a test that drops 95 percent for 10 simulated minutes |
| Storage indexes and compaction | Query plan uses indexes on a 100,000-row fixture; version count bounded over a simulated day |
| Split `capture/mod.rs` by stage, one stage per merge request, only where touched | No behavior change; each stage module has its contract test |

## Self-review

**Spec coverage (ask 1):** stage breakdown in section 3 (14 stages), resource usage in section 2 and Tasks 1 and 2, best-in-market references in section 3 and cited pages, improvement tasks 4 to 7 and E-F1.

**Placeholder scan:** the only intentional gap is the body of `recognize_and_clean` in Task 3, which must be written against the real OCR signature discovered by the stated `grep`; the test is designed to fail until it is. The AX FFI body in Task 4 Step 3 is a time-boxed spike whose output is a findings table, not code I can responsibly pre-write without compiling against the SDK.

**Type consistency:** `p50_ms` and `p95_ms` (Task 1) are read by the Python report (Task 2). `skip_counts`, `evaluated_total`, `total_stored`, `total_skipped` (Task 2) are consumed by `build_privacy_proof` (Task 5). `luma_9x8_from_rgba`, `dhash_9x8`, `hamming`, `is_aba` are defined once (Task 6) and match the standalone-tested source.

## Sources

- Screenpipe architecture: https://docs.screenpipe.com/architecture
- Apple, Read documents using the Vision framework (WWDC25): https://developer.apple.com/videos/play/wwdc2025/272/
- Apple, `RecognizeDocumentsRequest`: https://developer.apple.com/documentation/vision/recognizedocumentsrequest
- Apple, Capturing screen content in macOS: https://developer.apple.com/documentation/ScreenCaptureKit/capturing-screen-content-in-macos
