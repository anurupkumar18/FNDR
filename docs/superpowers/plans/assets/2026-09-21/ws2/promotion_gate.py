#!/usr/bin/env python3
"""Decide whether a fine-tuned adapter may replace the active model configuration.

Inputs are two metric dictionaries with values in [0, 1] (higher is better), one for the base model and
one for the candidate, both scored on the same gold set and guard set by scripts/eval/score.py.

Rule: promote only if the target metric improves by at least min_gain AND no guard metric drops by more
than max_regress. Every decision returns its reasons so the evidence packet can quote them.
"""
import argparse
import json
import sys
from pathlib import Path


def decide(base, cand, target, min_gain, guards, max_regress):
    reasons = []
    ok = True
    for name in [target] + list(guards):
        if name not in base or name not in cand:
            return False, [f"missing metric: {name}"]
    gain = cand[target] - base[target]
    if gain >= min_gain:
        reasons.append(f"target {target} improved by {gain:+.3f} (needs {min_gain:+.3f})")
    else:
        ok = False
        reasons.append(f"target {target} changed by {gain:+.3f}, below the required {min_gain:+.3f}")
    for g in guards:
        delta = cand[g] - base[g]
        if delta < -max_regress:
            ok = False
            reasons.append(f"guard {g} regressed by {delta:+.3f} (limit -{max_regress:.3f})")
        else:
            reasons.append(f"guard {g} ok at {delta:+.3f}")
    return ok, reasons


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--base", required=True, help="JSON file of base metrics")
    ap.add_argument("--candidate", required=True, help="JSON file of candidate metrics")
    ap.add_argument("--target", default="grounding_rate")
    ap.add_argument("--guards", nargs="*", default=["format_validity", "recall_at_5"])
    ap.add_argument("--min-gain", type=float, default=0.05)
    ap.add_argument("--max-regress", type=float, default=0.01)
    args = ap.parse_args(argv)
    base = json.loads(Path(args.base).read_text())
    cand = json.loads(Path(args.candidate).read_text())
    ok, reasons = decide(base, cand, args.target, args.min_gain, args.guards, args.max_regress)
    print("PROMOTE" if ok else "REJECT")
    for r in reasons:
        print(" -", r)
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
