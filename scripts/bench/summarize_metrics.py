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


def summarize(rows, machine, build, before_context_p95=None):
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
    if before_context_p95 is not None and (after := stages.get("capture.context_ms")):
        after_p95 = after["p95_ms"]
        lines += [
            "",
            "## CAP-05 context lookup comparison",
            "",
            "| Measurement | capture.context_ms p95 (ms) |",
            "|---|---:|",
            f"| AppleScript baseline | {before_context_p95} |",
            f"| Accessibility result | {after_p95} |",
            f"| Change | {after_p95 - before_context_p95:+} |",
        ]
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
    parser.add_argument("--before-context-p95", type=int)
    args = parser.parse_args(argv)
    report = summarize(load(args.input), args.machine, args.build, args.before_context_p95)
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(report)
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
