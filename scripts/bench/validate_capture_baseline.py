#!/usr/bin/env python3
"""Reject incomplete metrics runs before generating CAP-05 and MEM-03 evidence."""

import argparse
import json
import sys
from pathlib import Path


def load(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines() if line.strip()]


def validate(rows, min_samples=10, min_minutes=10):
    errors = []
    if len(rows) < min_samples:
        errors.append(f"need at least {min_samples} metrics samples, found {len(rows)}")
    if not rows:
        return errors + ["no metrics samples found"]

    first = rows[0]["snapshot"]["generated_at_ms"]
    last = rows[-1]["snapshot"]["generated_at_ms"]
    duration_minutes = (last - first) / 60000.0
    if duration_minutes < min_minutes:
        errors.append(f"need at least {min_minutes} minutes of metrics, found {duration_minutes:.1f}")

    snapshot = rows[-1]["snapshot"]
    aggregates = snapshot.get("aggregates", {})
    if "capture.context_ms" not in aggregates:
        errors.append("capture.context_ms was not observed")
    if not any(name.startswith("mem.") for name in aggregates):
        errors.append("no mem.* post-capture timing was observed")
    counters = snapshot.get("counters", {})
    if not any(name.startswith("mem.outcome.") and count > 0 for name, count in counters.items()):
        errors.append("no mem.outcome.* counter was observed")
    return errors


def main(argv):
    parser = argparse.ArgumentParser()
    parser.add_argument("input")
    parser.add_argument("--min-samples", type=int, default=10)
    parser.add_argument("--min-minutes", type=int, default=10)
    args = parser.parse_args(argv)
    errors = validate(load(args.input), args.min_samples, args.min_minutes)
    if errors:
        for error in errors:
            print(f"not ready: {error}")
        return 1
    print("baseline metrics are ready for report generation")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
