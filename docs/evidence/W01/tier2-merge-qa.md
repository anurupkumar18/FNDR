# Tier 2 merge QA: SEC-01, CAP-01, CAP-02, MEM-02

Real-app verification performed before merging each Tier 2 MR (`!16`, `!13`,
`!14`, `!18`) directly into `main`, on the reference machine (M1, 8 GB). Tier 1
(`!12`, `!15`, `!17`) had zero runtime-behavior risk and was merged earlier
without this step; Tier 2 changes actual runtime behavior (auth, capture
timing, capture composition), so each ticket was built and run for real
before merging, in dependency order (SEC-01, CAP-01, CAP-02, MEM-02).

## SEC-01 (MCP auth required by default)

- `cargo test --lib` on `fix/sec-01-require-mcp-auth-default`: 642/642 passed,
  including the two new adversarial tests
  (`mcp_rejects_unauthenticated_tool_call_in_default_local_mode`,
  `mcp_rejects_web_origin_in_local_mode`) that were confirmed failing against
  the pre-fix code before the fix landed.
- Ran the real app (`npm run tauri dev`). Capture pipeline started and stored
  a real captured frame end to end (OCR to embedding to LanceDB) with no
  errors.
- Could not do a live curl-based auth check against a running MCP server:
  `start_mcp_server` (the Tauri command that calls `mcp::start`) is only
  referenced from a test file (`ControlPanel.test.tsx`) — no production panel
  or button calls it. The MCP server backend is correct and fully covered by
  its own adversarial tests (which spin up a real axum server and issue real
  HTTP requests), but there is currently no way to start it from the running
  app's UI. This is a pre-existing gap unrelated to SEC-01's actual change
  (auth defaults), not a regression it introduced. Worth its own ticket if
  MCP is meant to be user-facing before Beta.

## CAP-01 + CAP-02 (per-stage timings, NDJSON baseline dump)

- `cargo test --lib` on the merged branch: 646 passed (642 + 4 new), then 647
  after CAP-02 (+1). All 0 failed.
- Live verification instead of a UI glance: `FNDR_METRICS_DUMP=/tmp/...`
  against the real running app. First NDJSON line (written immediately, per
  `tokio::time::interval`'s first-tick-is-immediate behavior) confirmed the
  dump schema. Second line (written 60s later, after a full capture cycle)
  confirmed real percentiles:

  ```
  capture.pixels_ms  {n: 7,  p50_ms: 1533, p95_ms: 1799}
  capture.ocr_ms     {n: 3,  p50_ms: 207,  p95_ms: 406}
  capture.dedupe_ms  {n: 7,  p50_ms: 2311, p95_ms: 2537}
  capture.context_ms {n: 18, p50_ms: 0,    p95_ms: 915}
  capture.cleanup_ms {n: 3,  p50_ms: 2,    p95_ms: 4}
  totals: {evaluated: 9, skipped: 7, stored: 1}
  ```

- Finding: the plan's verification step for CAP-01 says "open Pipeline
  Inspector, confirm p95 shows" — this UI does not currently exist reachably.
  `EngineMetricsCard.tsx` (which has the p50/p95 columns from CAP-01) lives in
  `EngineMetricsPanel.tsx`, which is not wired into `AppPanels.tsx`, not in
  the `PanelKey` union, and not in `DEMO_COMMAND_IDS` — there is no way to
  open it from the running app today. `PipelineInspectorPanel.tsx` (the
  literal "Pipeline Inspector" the plan refers to) has the same problem: not
  reachable from anywhere. Both appear to be pre-existing scaffolding that
  was never wired to navigation, predating CAP-01. The NDJSON dump above is
  offered as the substitute live-behavior proof.
- Related: a local, never-pushed commit from 2026-09-17
  (`demo: harden alpha-only local flow`, preserved at
  `preserve/demo-alpha-hardening-sep17`, pushed to origin for safekeeping)
  independently added `"engine-metrics"` to its own version of
  `DEMO_COMMAND_IDS`, which would have fixed exactly this reachability gap.
  It now conflicts with the `DEMO_COMMAND_IDS`/`isDemoCommand` mechanism that
  landed on `main` separately (different demo command list, plus an
  `"agent"` panel key that may not exist on `main`). Needs a deliberate merge
  decision, not a rushed one — left unmerged on purpose.

## MEM-02 (dead post-capture code removal)

- Local test-merge against `origin/main` (after CAP-01/CAP-02 landed):
  clean, no conflicts.
- `cargo test --lib` on the merged result: 645/645 (net decrease from 647 is
  expected — MEM-02 deletes the dead composer plus its private-only tests).
- Live run on fully-merged `main`: real capture completed and stored with no
  errors (`lancedb:add_batch inserted_count=1`), confirming the canonical
  `compose_memory_embedding_document` path still works after the dead-code
  removal.

## Full-stack check on fully-merged `main`

- `cargo build` (src-tauri): clean.
- `cargo test --lib`: 645 passed, 0 failed, 4 ignored.
- `npm run typecheck`: clean.
- `npm test`: 185 passed (34 files).
- Live app run: capture loop starts, captures, OCRs, embeds, and stores a
  real frame with no errors or panics.

## Open items surfaced here (not fixed, on purpose — out of scope for a
hardening/instrumentation ticket)

1. No production UI entry point starts the MCP server (`start_mcp_server` is
   test-only).
2. `EngineMetricsPanel` / `PipelineInspectorPanel` are unreachable from the
   app shell.
3. `preserve/demo-alpha-hardening-sep17` needs a deliberate reconciliation
   with `main`'s current `DEMO_COMMAND_IDS`/`PanelKey` — it would fix #2 for
   the engine-metrics case specifically.
