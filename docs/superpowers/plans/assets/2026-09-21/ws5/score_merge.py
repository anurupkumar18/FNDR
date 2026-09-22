#!/usr/bin/env python3
"""Score memory merge and dedup decisions with pairwise clustering metrics.

Input JSONL, one line per captured frame in a replayed session:
  {"frame_id": str, "story_id": str, "memory_id": str}
`story_id` is the gold label (frames that belong to one story). `memory_id` is the memory the pipeline stored
that frame into (merged frames share a memory id).

Metrics (pairs of frames):
  precision      of the pairs the pipeline put in one memory, the share that are really one story
  recall         of the pairs that are really one story, the share the pipeline put in one memory
  false_merges   frames from different stories that ended up in one memory (information loss, the costly error)
  fragmentation  stored memories per story (1.0 is ideal; above 1.0 means duplicates)
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
        for a, b in combinations(sorted(members), 2):
            pairs.add((a, b))
    return pairs


def score(frames):
    if not frames:
        raise ValueError("no frames")
    by_story = defaultdict(list)
    by_memory = defaultdict(list)
    memories_per_story = defaultdict(set)
    for f in frames:
        by_story[f["story_id"]].append(f["frame_id"])
        by_memory[f["memory_id"]].append(f["frame_id"])
        memories_per_story[f["story_id"]].add(f["memory_id"])
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
        "fragmentation": sum(len(m) for m in memories_per_story.values()) / len(by_story),
    }


def f1(precision, recall):
    return 0.0 if precision + recall == 0 else 2 * precision * recall / (precision + recall)


def render(name, result):
    return "\n".join([
        f"# Merge and dedup score: {name}",
        "",
        f"- Frames {result['frames']}, stories {result['stories']}, stored memories {result['memories']}",
        f"- Pairwise precision {result['precision']:.3f}, recall {result['recall']:.3f}, F1 {f1(result['precision'], result['recall']):.3f}",
        f"- False merges (pairs from different stories in one memory): {result['false_merges']}",
        f"- Fragmentation (memories per story, 1.0 is ideal): {result['fragmentation']:.2f}",
        "",
    ])


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("frames_jsonl")
    ap.add_argument("--name", default="run")
    ap.add_argument("--out", required=True)
    args = ap.parse_args(argv)
    frames = [json.loads(l) for l in Path(args.frames_jsonl).read_text().splitlines() if l.strip()]
    result = score(frames)
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    Path(args.out).write_text(render(args.name, result))
    print(f"wrote {args.out}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
