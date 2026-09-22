#!/usr/bin/env python3
"""Coverage versus precision report for one bounded decision.

Input JSONL, one labeled example per line: {"logits": [float, ...], "label": int}
`label` is the index of the correct option. Output is a markdown report: accuracy, calibration error before and
after temperature scaling, and for each target precision the threshold and the share of decisions that can be
automated at that precision.

Fit the temperature on one split and report on another. Passing the same file for both is allowed for a first
look but the report says so, because fitting and scoring on the same data flatters the result.
"""
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
    return [r["logits"] for r in rows], [r["label"] for r in rows]


def report(name, fit_rows, fit_labels, eval_rows, eval_labels, same_file):
    temperature = dc.fit_temperature(fit_rows, fit_labels)
    c0, ok0 = dc.confidences_and_correctness(eval_rows, eval_labels, 1.0)
    c1, ok1 = dc.confidences_and_correctness(eval_rows, eval_labels, temperature)
    accuracy = sum(ok1) / len(ok1)
    lines = [
        f"# Decision report: {name}",
        "",
        f"- Examples scored: {len(eval_labels)}; accuracy {100 * accuracy:.1f} percent",
        f"- Fitted temperature: {temperature}",
        f"- Expected calibration error: {dc.expected_calibration_error(c0, ok0):.3f} before, {dc.expected_calibration_error(c1, ok1):.3f} after",
    ]
    if same_file:
        lines.append("- Warning: the temperature was fitted on the same examples it was scored on; use a held-out split for claims.")
    lines += ["", "| Target precision | Threshold | Automated share | Precision reached |", "|---|---|---|---|"]
    for target in TARGETS:
        found = dc.threshold_for_precision(c1, ok1, target)
        if found is None:
            lines.append(f"| {target:.2f} | not reachable | 0 percent | - |")
        else:
            threshold, coverage, precision = found
            lines.append(f"| {target:.2f} | {threshold:.3f} | {100 * coverage:.0f} percent | {precision:.3f} |")
    lines.append("")
    lines.append("A valid answer is not a correct answer: this report measures correctness against labels.")
    return "\n".join(lines) + "\n"


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("eval_jsonl")
    ap.add_argument("--fit-jsonl", help="separate file to fit the temperature on (recommended)")
    ap.add_argument("--name", default="decision")
    ap.add_argument("--out", required=True)
    args = ap.parse_args(argv)
    eval_rows, eval_labels = load(args.eval_jsonl)
    if args.fit_jsonl:
        fit_rows, fit_labels = load(args.fit_jsonl)
    else:
        fit_rows, fit_labels = eval_rows, eval_labels
    text = report(args.name, fit_rows, fit_labels, eval_rows, eval_labels, same_file=not args.fit_jsonl)
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(text)
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
