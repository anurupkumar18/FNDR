#!/usr/bin/env python3
"""Pass or fail a vault scorecard against written thresholds.

Reads the JSON that `cargo run --example vault_qa` prints and the thresholds in
`vault-quality-thresholds.json`. Prints one line per check and exits 1 when any
fails. It reads counts only; the scorecard holds no memory text.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

# The activity labels the extraction prompt offers (inference/prompts.rs ACTIVITY_TYPES).
ACTIVITY_LABELS = {
    "coding", "debugging", "reviewing_agent_output", "researching", "planning", "writing", "studying",
    "watching_or_listening", "configuring_tool", "testing_workflow", "reading_results",
    "organizing_information", "communication", "job_or_career_work", "travel_or_logistics",
    "entertainment_or_personal_interest", "unknown", "",
}
IN_VOICE = ("past-tense verb", "states what the screen held (the, a, in)")
OUT_OF_VOICE = ("narrator (the user, you)", "dangling verb (has, is)", "-ing word")


def lookup(report: dict, dotted: str):
    value = report
    for key in dotted.split("."):
        if not isinstance(value, dict) or key not in value:
            return None
        value = value[key]
    return value


def percent(text) -> float | None:
    """'14 (9%)' -> 9.0"""
    match = re.search(r"\((\d+(?:\.\d+)?)%\)", str(text))
    return float(match.group(1)) if match else None


def fraction(text) -> float | None:
    """'10/11' -> 0.909; '0/0' -> None (not measured)."""
    match = re.fullmatch(r"\s*(\d+)\s*/\s*(\d+)\s*", str(text))
    if not match or int(match.group(2)) == 0:
        return None
    return int(match.group(1)) / int(match.group(2))


def derived(report: dict) -> dict:
    """Numbers the thresholds name that the scorecard holds in another shape."""
    values: dict[str, float] = {}
    unrelated = report.get("unrelated_queries")
    if isinstance(unrelated, list):
        values["unrelated_queries_marked_strong"] = sum(1 for q in unrelated if q.get("marked_strong"))
    labels = lookup(report, "labels.activity_type")
    if isinstance(labels, dict):
        values["activity_labels_outside_the_list"] = sum(
            count for label, count in labels.items() if label not in ACTIVITY_LABELS
        )
    shown = lookup(report, "voice.shown_after_cleanup")
    if isinstance(shown, dict) and sum(shown.values()) > 0:
        total = sum(shown.values())
        values["voice_in_voice"] = sum(shown.get(kind, 0) for kind in IN_VOICE) / total
        values["voice_out_of_voice"] = sum(shown.get(kind, 0) for kind in OUT_OF_VOICE) / total
    return values


def check(report: dict, thresholds: dict) -> list[tuple[bool | None, str]]:
    """One (passed, line) per threshold; passed is None when the scorecard did not measure it."""
    extra = derived(report)
    results: list[tuple[bool | None, str]] = []

    def judge(name: str, value: float | None, limit: float, at_most: bool, shown: str) -> None:
        if value is None:
            results.append((None, f"not measured  {name}"))
            return
        passed = value <= limit if at_most else value >= limit
        bound = "at most" if at_most else "at least"
        results.append((passed, f"{'ok  ' if passed else 'FAIL'}  {name}: {shown} ({bound} {limit:g})"))

    for name, limit in thresholds.get("max_percent", {}).items():
        value = percent(lookup(report, name))
        judge(name, value, limit, True, f"{value:g}%" if value is not None else "")
    for name, limit in thresholds.get("max_count", {}).items():
        value = extra.get(name)
        judge(name, value, limit, True, f"{value:g}" if value is not None else "")
    for key, at_most in (("min_fraction", False), ("max_fraction", True)):
        for name, limit in thresholds.get(key, {}).items():
            raw = lookup(report, name)
            value = extra.get(name) if name in extra else fraction(raw)
            shown = str(raw) if raw is not None and name not in extra else (f"{value:.2f}" if value is not None else "")
            judge(name, value, limit, at_most, shown)
    return results


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scorecard", required=True, type=Path)
    parser.add_argument("--thresholds", type=Path, default=Path(__file__).with_name("vault-quality-thresholds.json"))
    args = parser.parse_args(argv)
    report = json.loads(args.scorecard.read_text())
    results = check(report, json.loads(args.thresholds.read_text()))
    for _, line in results:
        print(line)
    failed = sum(1 for passed, _ in results if passed is False)
    unmeasured = sum(1 for passed, _ in results if passed is None)
    print(f"\nVault quality: {'FAIL' if failed else 'PASS'} ({failed} failed, {unmeasured} not measured, {len(results)} checks)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
