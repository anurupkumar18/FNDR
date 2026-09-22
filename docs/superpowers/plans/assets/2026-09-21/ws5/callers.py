#!/usr/bin/env python3
"""Classify Rust functions as live, debug-only, self-only, or dead by counting call sites.

usage: callers.py <src root> <symbol> [<symbol> ...] [--debug-marker debug]

A call is a match of `name(` on a line that is not the definition (`fn name`). Scanning of each file stops at
its first `#[cfg(test)]`, so test-only calls do not make a function look live.

  live        called from at least one file other than its own definition file and other than debug files
  debug-only  every call is from a file whose path contains the debug marker
  self-only   every call is inside the file that defines it (still check by hand: capture/mod.rs holds the
              capture loop itself, so a same-file caller there can be live)
  dead        no calls at all outside tests
"""
import argparse
import re
import sys
from collections import Counter
from pathlib import Path


def scan(root, symbol, debug_marker):
    call_re = re.compile(r"\b%s\s*\(" % re.escape(symbol))
    def_re = re.compile(r"\bfn\s+%s\b" % re.escape(symbol))
    def_files = set()
    calls = Counter()
    for path in Path(root).rglob("*.rs"):
        try:
            lines = path.read_text().splitlines()
        except (OSError, UnicodeDecodeError):
            continue
        for line in lines:
            if "#[cfg(test)]" in line:
                break
            if def_re.search(line):
                def_files.add(str(path))
            else:
                found = len(call_re.findall(line))
                if found:
                    calls[str(path)] += found
    return def_files, calls


def classify(def_files, calls, debug_marker):
    if not calls:
        return "dead"
    outside_self = {f for f in calls if f not in def_files}
    if not outside_self:
        return "self-only"
    non_debug = {f for f in outside_self if debug_marker not in f}
    if not non_debug:
        return "debug-only"
    return "live"


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("root")
    ap.add_argument("symbols", nargs="+")
    ap.add_argument("--debug-marker", default="debug")
    args = ap.parse_args(argv)
    print("| Symbol | Class | Calls | Top callers |")
    print("|---|---|---|---|")
    for symbol in args.symbols:
        def_files, calls = scan(args.root, symbol, args.debug_marker)
        top = ", ".join(f"{Path(f).relative_to(args.root)}:{n}" for f, n in calls.most_common(3))
        print(f"| {symbol} | {classify(def_files, calls, args.debug_marker)} | {sum(calls.values())} | {top or '-'} |")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
