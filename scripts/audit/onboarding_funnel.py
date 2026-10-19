#!/usr/bin/env python3
"""Summarize local, content-free onboarding event files from test profiles."""

import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path


EVENTS = {
    "step_started", "step_finished", "step_skipped", "error", "completed",
    "search_submitted", "result_selected", "reopen_succeeded", "reopen_failed",
}
STEPS = {
    "welcome", "privacy_promise", "biometrics", "model_download",
    "permissions", "bring_past", "indexing_started", "complete",
}
FIELDS = {"event", "step", "at_ms", "monotonic_ms", "duration_ms"}


def read_run(path: Path) -> list[dict]:
    rows = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    if not rows or rows[0].get("event") != "step_started":
        raise ValueError(f"{path}: no onboarding start")
    for row in rows:
        if set(row) - FIELDS or row.get("event") not in EVENTS:
            raise ValueError(f"{path}: unexpected event field or kind")
        if "step" in row and row["step"] not in STEPS:
            raise ValueError(f"{path}: unexpected step")
        if not isinstance(row.get("at_ms"), int) or row["at_ms"] < 0:
            raise ValueError(f"{path}: invalid time")
        if "monotonic_ms" in row and (not isinstance(row["monotonic_ms"], int) or row["monotonic_ms"] < 0):
            raise ValueError(f"{path}: invalid monotonic time")
        if "duration_ms" in row and (not isinstance(row["duration_ms"], int) or row["duration_ms"] < 0):
            raise ValueError(f"{path}: invalid duration")
    return rows


def activation_proxy(rows: list[dict]) -> bool:
    def elapsed_time(row: dict) -> int:
        return row.get("monotonic_ms", row["at_ms"])

    completed_at = next((elapsed_time(row) for row in rows if row["event"] == "completed"), None)
    if completed_at is None:
        return False
    stage = 0
    for row in rows:
        if not completed_at <= elapsed_time(row) <= completed_at + 600_000:
            continue
        if row["event"] == "search_submitted":
            stage = 1
        elif row["event"] == "result_selected" and stage == 1:
            stage = 2
        elif row["event"] == "reopen_succeeded" and stage == 2:
            return True
    return False


def render(paths: list[Path]) -> str:
    skipped = Counter()
    steps = defaultdict(Counter)
    durations = defaultdict(list)
    completed = errors = proxies = 0
    for path in paths:
        rows = read_run(path)
        completed += any(row["event"] == "completed" for row in rows)
        errors += sum(row["event"] == "error" for row in rows)
        proxies += activation_proxy(rows)
        for row in rows:
            if "step" in row and row["event"] in {"step_started", "step_finished", "step_skipped", "error"}:
                steps[row["step"]][row["event"]] += 1
            if row["event"] == "step_skipped":
                skipped[row["step"]] += 1
            if row["event"] in {"step_finished", "step_skipped"} and "duration_ms" in row:
                durations[row["step"]].append(row["duration_ms"])
    lines = [
        "# Local onboarding funnel summary", "",
        f"Trials: {len(paths)}",
        f"Completed onboarding: {completed}",
        f"Incomplete onboarding: {len(paths) - completed}",
        f"Errors: {errors}",
        f"Activation proxy: {proxies}",
        "Observed activation: not measured", "",
        "## Skips", "",
    ]
    lines.extend(f"{step}: {count}" for step, count in sorted(skipped.items()))
    lines.extend(["", "## Step funnel", ""])
    lines.extend(
        f"{step}: started {counts['step_started']}, finished {counts['step_finished']}, "
        f"skipped {counts['step_skipped']}, errors {counts['error']}"
        for step, counts in sorted(steps.items())
    )
    lines.extend(["", "## Step durations", ""])
    lines.extend(
        f"{step}: {len(times)} {'sample' if len(times) == 1 else 'samples'}, mean {sum(times) // len(times)} ms"
        for step, times in sorted(durations.items())
    )
    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--events", type=Path, action="append", required=True,
                        help="Local onboarding-funnel.jsonl file; repeat for each test profile")
    args = parser.parse_args()
    print(render(args.events), end="")


if __name__ == "__main__":
    main()
