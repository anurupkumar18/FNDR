#!/usr/bin/env python3
"""EM-01: check that docs/product/embedding-coverage.md names every production
memory writer.

A production writer is a non-test source file under src-tauri/src that calls a
store method that writes memory rows or chunk rows. Test modules (everything
from `#[cfg(test)]` followed by `mod`) are ignored, as is the store itself.

Usage:
    python3 scripts/audit/check_embedding_coverage.py          # check, exit 1 on gap
    python3 scripts/audit/check_embedding_coverage.py --list   # print writers
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "src-tauri" / "src"
DOC = ROOT / "docs" / "product" / "embedding-coverage.md"

WRITE_CALL = re.compile(
    r"\.(add_batch|add_batch_and_get_count|add_batch_preserving_ids|"
    r"add_v5_batch_preserving_ids|replace_memory_preserving_chunks|"
    r"replace_memories_preserving_chunks|replace_all_memories_preserving_ids|"
    r"upsert_memory_chunks|insert_memory_chunk)\("
)
# The store defines these methods and calls them on itself; benches and evals
# are not part of the shipped app.
IGNORED_PREFIXES = ("storage/lance_store/", "evals/")


def production_text(path: Path) -> str:
    """Return the file's text with every `#[cfg(test)] mod ... { }` block blanked.

    Lines are kept (blanked, not removed) so reported line numbers stay true.
    Braces inside string or char literals can skew the match; the module end
    is found by counting braces, which is accurate enough for this repo and is
    cross-checked by the `--list` output.
    """
    lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    out = list(lines)
    index = 0
    while index < len(lines):
        if lines[index].strip() != "#[cfg(test)]":
            index += 1
            continue
        probe = index + 1
        while probe < len(lines) and lines[probe].strip().startswith("#["):
            probe += 1
        head = lines[probe].strip() if probe < len(lines) else ""
        if not head.startswith(("mod ", "pub mod ", "pub(crate) mod ")):
            index += 1
            continue
        depth = 0
        opened = False
        cursor = probe
        while cursor < len(lines):
            depth += lines[cursor].count("{") - lines[cursor].count("}")
            opened = opened or "{" in lines[cursor]
            if opened and depth <= 0:
                break
            cursor += 1
        for blank in range(index, min(cursor, len(lines) - 1) + 1):
            out[blank] = ""
        index = cursor + 1
    return "\n".join(out)


def production_writers() -> dict[str, list[int]]:
    writers: dict[str, list[int]] = {}
    for path in sorted(SRC.rglob("*.rs")):
        rel = path.relative_to(SRC).as_posix()
        if rel.startswith(IGNORED_PREFIXES) or "/tests/" in rel or rel.endswith("tests.rs"):
            continue
        text = production_text(path)
        hits = [
            number
            for number, line in enumerate(text.splitlines(), start=1)
            if WRITE_CALL.search(line) and not line.lstrip().startswith("//")
        ]
        if hits:
            writers[rel] = hits
    return writers


def main(argv: list[str]) -> int:
    writers = production_writers()
    if "--list" in argv:
        for rel, hits in writers.items():
            print(f"{rel}: {', '.join(map(str, hits))}")
        return 0
    doc = DOC.read_text(encoding="utf-8") if DOC.exists() else ""
    missing = [rel for rel in writers if not re.search(f"`{re.escape(rel)}[`:]", doc)]
    if missing:
        print(f"{DOC.relative_to(ROOT)} does not name these production writers:")
        for rel in missing:
            print(f"  {rel} (lines {', '.join(map(str, writers[rel]))})")
        return 1
    print(f"ok: all {len(writers)} production writer files are in the coverage doc")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
