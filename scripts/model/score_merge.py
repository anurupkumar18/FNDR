#!/usr/bin/env python3
"""Score memory merge and dedup decisions with pairwise clustering metrics.

Input JSONL has one line per replayed frame:
  {"frame_id": str, "story_id": str, "memory_id": str}
`story_id` is the gold label. Frames sharing a `memory_id` were stored in the
same memory by FNDR's capture merge path.
"""

import argparse
import json
import sys
from collections import defaultdict
from itertools import combinations
from pathlib import Path


def _pairs(groups):
    pairs = set()
    for members in groups.values():
        for first, second in combinations(sorted(members), 2):
            pairs.add((first, second))
    return pairs


def score(frames):
    if not frames:
        raise ValueError("no frames")
    by_story = defaultdict(list)
    by_memory = defaultdict(list)
    memories_per_story = defaultdict(set)
    for frame in frames:
        by_story[frame["story_id"]].append(frame["frame_id"])
        by_memory[frame["memory_id"]].append(frame["frame_id"])
        memories_per_story[frame["story_id"]].add(frame["memory_id"])
    gold = _pairs(by_story)
    predicted = _pairs(by_memory)
    correct = gold & predicted
    return {
        "frames": len(frames),
        "stories": len(by_story),
        "memories": len(by_memory),
        "precision": len(correct) / len(predicted) if predicted else 1.0,
        "recall": len(correct) / len(gold) if gold else 1.0,
        "false_merges": len(predicted - gold),
        "fragmentation": sum(len(memories) for memories in memories_per_story.values())
        / len(by_story),
    }


def f1(precision, recall):
    return 0.0 if precision + recall == 0 else 2 * precision * recall / (precision + recall)


def render(name, result):
    return "\n".join(
        [
            f"# Merge and dedup score: {name}",
            "",
            f"- Frames {result['frames']}, stories {result['stories']}, stored memories {result['memories']}",
            f"- Pairwise precision {result['precision']:.3f}, recall {result['recall']:.3f}, F1 {f1(result['precision'], result['recall']):.3f}",
            f"- False merges (pairs from different stories in one memory): {result['false_merges']}",
            f"- Fragmentation (memories per story, 1.0 is ideal): {result['fragmentation']:.2f}",
            "",
        ]
    )


def main(argv):
    parser = argparse.ArgumentParser()
    parser.add_argument("frames_jsonl")
    parser.add_argument("--name", default="run")
    parser.add_argument("--out", required=True)
    args = parser.parse_args(argv)
    frames = [
        json.loads(line)
        for line in Path(args.frames_jsonl).read_text().splitlines()
        if line.strip()
    ]
    result = score(frames)
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(render(args.name, result))
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
