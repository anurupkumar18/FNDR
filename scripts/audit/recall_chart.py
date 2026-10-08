#!/usr/bin/env python3
"""Rebuild the Beta demo recall chart from committed evidence (VS-66).

Reads, for each persona, the "before" report (the VS-03 baseline) and the
"after" report (the accepted retrieval reference), and writes:

  beta-demo-recall.csv  one row per persona, path, and stage
  beta-demo-recall.png  Recall@5 and paraphrase Recall@5, before and after

Only committed files are read, so anyone can regenerate the chart:

  python3 scripts/audit/recall_chart.py            # or: make recall-chart

The PNG needs matplotlib (pip install matplotlib); --no-png writes the CSV only.
"""

from __future__ import annotations

import argparse
import csv
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PERSONAS = ("knowledge-worker", "office-pm")
PATHS = ("search", "ask")
KINDS = ("keyword", "paraphrase", "time", "app")
CSV_FIELDS = [
    "persona",
    "path",
    "stage",
    "recall_at_5",
    "mrr_at_10",
    "p95_ms",
    *(f"recall_at_5_{kind}" for kind in KINDS),
]


def default_sources(root: Path = ROOT) -> dict[str, tuple[Path, Path]]:
    return {
        persona: (
            root / f"docs/evidence/W03/retrieval-baseline-{persona}.json",
            root / f"scripts/demo/retrieval-reference/{persona}.json",
        )
        for persona in PERSONAS
    }


def rows(sources: dict[str, tuple[Path, Path]]) -> list[dict]:
    out = []
    for persona, (before_path, after_path) in sources.items():
        reports = {
            "before": json.loads(before_path.read_text()),
            "after": json.loads(after_path.read_text()),
        }
        for path in PATHS:
            for stage, report in reports.items():
                metrics = report["paths"][path]
                by_kind = metrics.get("recall_at_5_by_kind", {})
                row = {
                    "persona": persona,
                    "path": path,
                    "stage": stage,
                    "recall_at_5": round(metrics["recall_at_5"], 3),
                    "mrr_at_10": round(metrics["mrr_at_10"], 3),
                    "p95_ms": metrics.get("latency_ms", {}).get("p95"),
                }
                for kind in KINDS:
                    row[f"recall_at_5_{kind}"] = round(by_kind.get(kind, 0.0), 3)
                out.append(row)
    return out


def write_csv(data: list[dict], path: Path) -> None:
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=CSV_FIELDS, lineterminator="\n")
        writer.writeheader()
        writer.writerows(data)


def write_png(data: list[dict], path: Path) -> None:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    groups = [(row["persona"], row["path"]) for row in data if row["stage"] == "before"]
    value = {(row["persona"], row["path"], row["stage"]): row for row in data}
    labels = [f"{persona}\n{path.capitalize()}" for persona, path in groups]
    positions = range(len(groups))
    width = 0.38

    figure, axes = plt.subplots(1, 2, figsize=(11, 4.2), sharey=True)
    for axis, metric, title in (
        (axes[0], "recall_at_5", "Recall@5 (keyword and paraphrase queries)"),
        (axes[1], "recall_at_5_paraphrase", "Recall@5, paraphrase queries only"),
    ):
        for offset, stage, color in ((-width / 2, "before", "#9aa5b1"), (width / 2, "after", "#2f6fdb")):
            heights = [value[(persona, path, stage)][metric] for persona, path in groups]
            bars = axis.bar([p + offset for p in positions], heights, width, label=stage, color=color)
            axis.bar_label(bars, labels=[f"{h:.2f}" for h in heights], fontsize=8, padding=2)
        axis.set_title(title, fontsize=10)
        axis.set_xticks(list(positions))
        axis.set_xticklabels(labels, fontsize=8)
        axis.set_ylim(0, 1.12)
        axis.spines[["top", "right"]].set_visible(False)
    axes[0].set_ylabel("share of queries with the right memory in the top five")
    axes[0].legend(frameon=False, fontsize=8, loc="lower left")
    figure.suptitle(
        "FNDR retrieval on synthetic personas: before (VS-03 baseline) and after (current reference)",
        fontsize=10,
    )
    figure.tight_layout()
    # No timestamp or version in the file, so an unchanged chart is byte-identical.
    figure.savefig(path, dpi=150, metadata={"Software": None})
    plt.close(figure)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--out-dir", type=Path, default=ROOT / "docs/evidence/W03")
    parser.add_argument("--no-png", action="store_true", help="write the CSV only")
    args = parser.parse_args(argv)

    data = rows(default_sources())
    args.out_dir.mkdir(parents=True, exist_ok=True)
    write_csv(data, args.out_dir / "beta-demo-recall.csv")
    print(f"wrote {args.out_dir / 'beta-demo-recall.csv'}")
    if not args.no_png:
        try:
            write_png(data, args.out_dir / "beta-demo-recall.png")
        except ImportError:
            print("matplotlib is not installed: pip install matplotlib, or pass --no-png", file=sys.stderr)
            return 2
        print(f"wrote {args.out_dir / 'beta-demo-recall.png'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
