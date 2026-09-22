#!/usr/bin/env python3
"""Checkpoint 2: where do the bytes in a GGUF file go? Groups tensor sizes by storage type."""
import os, re, subprocess, sys, collections
M = os.path.expanduser(os.environ.get("FNDR_LAB_MODEL", "~/Library/Application Support/com.fndr.app/models/Qwen3VL-2B-Instruct-Q4_K_M.gguf"))
out = subprocess.run(["llama-gguf", M, "r"], capture_output=True, text=True)
rows = []
for ln in (out.stdout + out.stderr).splitlines():
    m = re.search(r"read_1: tensor\[\d+\]: name = (\S+), size = (\d+), offset = \d+, type = (\S+), n_elts = (\d+)", ln)
    if m:
        rows.append((m.group(1), int(m.group(2)), m.group(3)))
total = sum(r[1] for r in rows)
by_type = collections.Counter()
for _, size, t in rows:
    by_type[t] += size
print(f"tensors: {len(rows)}   total tensor bytes: {total:,}  ({total/2**20:.1f} MiB)\n")
print(f"{'type':8} {'MiB':>8} {'share':>7}")
for t, b in by_type.most_common():
    print(f"{t:8} {b/2**20:8.1f} {100*b/total:6.1f}%")
big = max(rows, key=lambda r: r[1])
print(f"\nlargest tensor: {big[0]}  {big[1]/2**20:.1f} MiB  ({big[2]})")
