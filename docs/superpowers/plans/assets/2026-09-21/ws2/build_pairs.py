#!/usr/bin/env python3
"""Build DPO preference pairs from an exported FNDR feedback file.

Input JSONL, one event per line:
  {"trace_id": str, "task": str, "prompt": str, "output": str, "signal": str, "edited_output": str | null}
Signals: "edit", "thumbs_up", "thumbs_down".

Pair rules:
  1. edit: chosen = edited_output, rejected = output (the user's own correction is the strongest signal).
  2. thumbs_down and thumbs_up on the same prompt: chosen = the up output, rejected = the down output.
Pairs are dropped when chosen equals rejected, when either side is empty, or when either side is longer
than max_chars. Duplicate (prompt, chosen) pairs are kept once.

The output file is training data derived from personal captures. It stays on this machine and is never committed.
"""
import argparse
import hashlib
import json
import sys
from collections import defaultdict
from pathlib import Path


def _key(prompt, chosen):
    return hashlib.sha256((prompt + "\x00" + chosen).encode()).hexdigest()


def build_pairs(events, max_chars=6000):
    pairs = []
    seen = set()

    def add(prompt, chosen, rejected):
        if not prompt or not chosen or not rejected or chosen == rejected:
            return
        if len(chosen) > max_chars or len(rejected) > max_chars:
            return
        k = _key(prompt, chosen)
        if k in seen:
            return
        seen.add(k)
        pairs.append({"prompt": prompt, "chosen": chosen, "rejected": rejected})

    by_prompt = defaultdict(lambda: {"up": [], "down": []})
    for e in events:
        signal = e.get("signal")
        if signal == "edit" and e.get("edited_output"):
            add(e["prompt"], e["edited_output"], e["output"])
        elif signal == "thumbs_up":
            by_prompt[e["prompt"]]["up"].append(e["output"])
        elif signal == "thumbs_down":
            by_prompt[e["prompt"]]["down"].append(e["output"])

    for prompt, group in by_prompt.items():
        for up in group["up"]:
            for down in group["down"]:
                add(prompt, up, down)
    return pairs


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("feedback")
    ap.add_argument("--out", required=True)
    ap.add_argument("--max-chars", type=int, default=6000)
    args = ap.parse_args(argv)
    events = [json.loads(l) for l in Path(args.feedback).read_text().splitlines() if l.strip()]
    pairs = build_pairs(events, args.max_chars)
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text("".join(json.dumps(p) + "\n" for p in pairs))
    print(f"{len(pairs)} pairs from {len(events)} events")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
