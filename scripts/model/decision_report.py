#!/usr/bin/env python3
"""Coverage-versus-precision report for one bounded decision."""

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import decision_calibration as dc

TARGETS = (0.90, 0.95, 0.99)


def load(path):
    rows = [json.loads(line) for line in Path(path).read_text().splitlines() if line.strip()]
    if not rows:
        raise ValueError("no examples")
    return [row["logits"] for row in rows], [row["label"] for row in rows]


def report(name, fit_rows, fit_labels, eval_rows, eval_labels, same_file):
    temperature = dc.fit_temperature(fit_rows, fit_labels)
    confs_before, correct_before = dc.confidences_and_correctness(eval_rows, eval_labels, 1.0)
    confs_after, correct_after = dc.confidences_and_correctness(eval_rows, eval_labels, temperature)
    accuracy = sum(correct_after) / len(correct_after)
    lines = [
        f"# Decision report: {name}",
        "",
        f"- Examples scored: {len(eval_labels)}; accuracy {100 * accuracy:.1f} percent",
        f"- Fitted temperature: {temperature}",
        "- Expected calibration error: "
        f"{dc.expected_calibration_error(confs_before, correct_before):.3f} before, "
        f"{dc.expected_calibration_error(confs_after, correct_after):.3f} after",
    ]
    if same_file:
        lines.append("- Warning: the temperature was fitted on the same examples it was scored on; use a held-out split for claims.")
    lines += ["", "| Target precision | Threshold | Automated share | Precision reached |", "|---|---|---|---|"]
    for target in TARGETS:
        found = dc.threshold_for_precision(confs_after, correct_after, target)
        if found is None:
            lines.append(f"| {target:.2f} | not reachable | 0 percent | - |")
        else:
            threshold, coverage, precision = found
            lines.append(f"| {target:.2f} | {threshold:.3f} | {100 * coverage:.0f} percent | {precision:.3f} |")
    lines.extend(["", "A valid answer is not a correct answer: this report measures correctness against labels."])
    return "\n".join(lines) + "\n"


def main(argv):
    parser = argparse.ArgumentParser()
    parser.add_argument("eval_jsonl")
    parser.add_argument("--fit-jsonl", help="separate file to fit temperature on")
    parser.add_argument("--name", default="decision")
    parser.add_argument("--out", required=True)
    args = parser.parse_args(argv)
    eval_rows, eval_labels = load(args.eval_jsonl)
    if args.fit_jsonl:
        fit_rows, fit_labels = load(args.fit_jsonl)
    else:
        fit_rows, fit_labels = eval_rows, eval_labels
    text = report(args.name, fit_rows, fit_labels, eval_rows, eval_labels, not args.fit_jsonl)
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(text)
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
