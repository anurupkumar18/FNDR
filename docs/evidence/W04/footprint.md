# W04 footprint: synthetic capture and storage maintenance

Measured 2026-10-15 on an Apple M1 Pro, arm64, 16 GiB RAM. At the final machine check, macOS reported 9.5 GiB swap used and load averages 4.62, 7.74, 9.04. Other build sessions were active. This is not the target 8 GiB M1, so the absolute CPU and memory numbers are a noisy local reference.

## Method

`src-tauri/examples/footprint_baseline.rs` runs a fixed 120 second wall-clock window in a release build. It resizes checked-in synthetic screen fixtures to 2880 x 1800 BGRA before the window. Each admitted poll uses the same BGRA to PNG conversion, `PerceptualHasher::check`, and Apple Vision OCR on a novel frame as the capture pipeline. `timer` models the previous 2 second active or 5 second idle pixel poll. `event` uses `TriggerGate`. `watched` calls the system sampler every second with GPU enabled; `background` uses the new 5 second cadence without GPU. The two policy runs use the same fixture timeline and binary.

The benchmark does not grab the real display or read the foreground app, window title, URL, or any owner file. It therefore excludes CoreGraphics display capture, WindowServer work, and the per-poll live context lookup. It includes real PNG conversion, dedupe, OCR, and telemetry syscalls against synthetic fixtures. `timer` plus `watched` is a reconstruction of the old policy in the new binary, not a run of the old app build. No full-day app run, 8 GiB M1 run, or battery power measurement was made. `make capture-baseline` needs a live `FNDR_METRICS_DUMP` from the running app, so it was not used under the synthetic-only rule.

Commands, from `src-tauri/` with the required SDKROOT, CXXFLAGS, CARGO_BUILD_JOBS=2 and shared CARGO_TARGET_DIR:

```sh
cargo build --release --example footprint_baseline
target/release/examples/footprint_baseline --window-secs 120 --profile idle --policy timer --sampler watched
target/release/examples/footprint_baseline --window-secs 120 --profile idle --policy event --sampler background
target/release/examples/footprint_baseline --window-secs 120 --profile active --policy timer --sampler watched
target/release/examples/footprint_baseline --window-secs 120 --profile active --policy event --sampler background
cargo test --test footprint_replay -- --nocapture
```

The timer runs exit nonzero because they exceed the capture budget. The event idle run exits zero. All four rows below are actual release runs, not extrapolated test counts. CPU is process CPU time from `getrusage(RUSAGE_SELF)` divided by wall time, expressed as a percent of one core. The original `MACH_TASK_BASIC_INFO` user and system fields stayed at zero during busy work, so a test caught that measurement bug and the counter was changed before these rows were collected.

## Before and after

| Profile | Policy and sampler | Process CPU % | RSS MiB | Physical footprint MiB | Wakeups/s | Pixel captures/h | Novel frames and OCR calls |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Idle | Timer, watched | 1.79 | 52.5 | 147.6 | 9.90 | 719 | 1 |
| Idle | Event, background | 0.58 | 54.9 | 127.3 | 9.97 | 180 | 1 |
| Active | Timer, watched | 4.29 | 55.9 | 153.6 | 10.12 | 1798 | 4 |
| Active | Event, background | 1.08 | 54.3 | 158.6 | 10.50 | 240 | 4 |

In this probe, event capture reduced pixel work by 75% idle and 87% active. Process CPU fell by 1.21 points idle and 3.21 points active. A repeat idle run with the billed-time column recorded timer at 1.99% CPU, 146.5 MiB RSS, 115.7 MiB physical footprint, 10.08 wakeups/s, 719 captures/h; event at 0.58% CPU, 172.7 MiB RSS, 147.5 MiB physical footprint, 10.23 wakeups/s, 180 captures/h. Both reported 1 billed ms per minute and zero disk bytes written in the 120 second window. The memory readings moved substantially between repeats on this swap-heavy host, so this evidence does not establish a memory reduction. Wakeups stayed near 10/s, so it does not establish a wakeup reduction either. Billed system time was too coarse to distinguish the modes, and is not a joule or battery-life measurement.

The old sampler spawned `/usr/sbin/ioreg -l -d 1 -n IOAccelerator -r` once per second. A separate 20-call run measured 15.54 ms wall and 13.97 ms child CPU per call. That is about 1.4% of one core in child CPU at one call per second. Process CPU in the table excludes the child. Inference from the two measurements: old idle total including this child was about 3.4% of one core versus 0.58% in the event and background probe. This is a cost estimate, not a paired whole-app CPU reading. The new sampler calls `ioreg` only after the visible metrics IPC command signals display demand. The optional once-per-minute metrics dump uses the background snapshot and does not signal that demand.

## Capture quality and budgets

`cargo test --test footprint_replay -- --nocapture` replays one synthetic hour with all 25 positive fixture screens, scroll and typing bursts, app switches, passive changes, and an animated title. On 2 second active polls, timer performed 1800 pixel captures and event performed 201. Production perceptual dedupe considered 12 fixture states novel in each path; every timer-kept long-lived state was kept by event. The replay also includes a four-second passive change with unchanged app and title. Timer kept it and event missed it. This is the expected loss for a passive transient shorter than the 10 second active or 20 second idle fallback. A separate static idle hour at 5 second polls performed 720 timer captures and 180 event captures. Its sampler plan never requested GPU sampling when no display was watching.

The example budget is 2.5% of one core idle, 3% active, 12 wakeups/s, and at most 180 idle or 600 active pixel captures/h. CPU limits leave room above the measured event values of 0.58% and 1.08% while detecting a return to the 4.29% active timer path. The wakeup limit is above the observed 9.9 to 10.5/s range because these runs did not show a wakeup improvement. The idle pixel limit matches the deterministic 20 second fallback, and the active limit allows app and input triggers above the 10 second fallback while rejecting the old 1800/h timer path. The budget is a synthetic regression guard, not an 8 GiB device acceptance threshold.

The default is event capture with a timer fallback. `capture_pipeline.event_driven = false` restores the old fixed-rate loop. The 60 second `forced_capture_interval` remains unchanged and can still force a duplicate static frame through OCR. Focus switches between same-title windows in the same app have no separate window identity in `FrontmostAppContext`; the fallback still samples them. These need a live capture-quality check before the battery setting can be treated as proven on daily use.

## Storage

ADR-004 keeps stable capture screenshots in memory only. Capture records set `screenshot_path` to `None`, and the quality gate rejects one that has a screenshot path. `screenshot_retention_days` defaults to 30 but has no retention effect; its only production reader is the MCP settings echo. Existing artifact cleanup still checks `frames/` and legacy screenshot paths in capture cleanup, privacy cleanup, memory deletion, and storage maintenance. Legacy graph, memory-card, and retrieval paths pass screenshot fields through, but stable capture does not create them. The config comment now states this. No screenshot encryption or deletion claim is inferred from this dead setting.

Before this change, `delete_older_than` ran only at startup and `compact_and_prune` had no production caller. A long-running process could accumulate Lance versions and retain rows past the configured age until restart. After a 10 minute startup delay, a background task retries every 5 minutes until capture is paused or deep idle and no frame is in progress. It then applies `retention_days` and compacts and prunes both the memories and memory_chunks tables, repeating after 24 hours. Retention removes an old memory's chunks, graph node, and touching edges before its parent row so a failed cleanup can be retried. A synthetic store test removed all of those old records, kept a current row and five chunks, and reduced chunk-table versions to at most three. The existing 1200-row compaction test still passed. These tests exercise the store and lock boundaries; the full day scheduler was not run for 24 hours.

The existing six-hour storage reclaim task already uses `compact_memory_record_payload` to clear raw `text` for ordinary capture records, trim `clean_text`, and clear legacy screenshot paths before rewrite. Agent notes retain their capped text. It applies to the records it scans, not only to an age-selected old tier. A hot-full versus old-compact tier is therefore not claimed here. New capture stores text-derived memory fields by default and no raw screenshot file.

## CPU budget for import scans

While capture is running, an import scan should stay below 2% of one core sustained and yield before a capture frame starts. This leaves the measured event idle path near 2.6% combined, below the reconstructed old idle path including the `ioreg` child near 3.4%. On the target 8 GiB M1, this remains a provisional budget until a full-app run measures contention, OCR latency, and memory pressure.
