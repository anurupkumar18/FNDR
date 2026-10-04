#!/usr/bin/env python3
"""Compare a retrieval_qa JSON report against the accepted reference (VS-04).

The check fails when any reference path's Recall@5 drops more than the allowed
tolerance, when a query that one path found in its top ten becomes a miss, or
when a reference path or query disappears. Everything else (MRR@10, per-kind
recall, latency, rank moves within the top ten, new paths and queries) is
reported but does not block, so a reviewer sees the whole picture.

Usage:
  retrieval_check.py --reference REF.json --current CUR.json [--max-recall-drop 0.05]

Exit codes: 0 pass, 1 regression, 2 unreadable or incompatible reports.
"""

from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass, field
from pathlib import Path

SUPPORTED_SCHEMA_VERSIONS = frozenset((1, 2))
DEFAULT_MAX_RECALL_DROP = 0.05
# Recall values are ratios of small integers; allow float noise at the boundary.
EPSILON = 1e-9


class ReportError(ValueError):
    """A report is unreadable, malformed, or not comparable to the reference."""


@dataclass(frozen=True)
class RankChange:
    query: str
    kind: str
    path: str
    before: int | None
    after: int | None


@dataclass
class CheckResult:
    case_set: str
    max_recall_drop: float
    paths: list[str]
    reference_paths: dict
    current_paths: dict
    reference_top1: dict = field(default_factory=dict)
    current_top1: dict = field(default_factory=dict)
    failures: list[str] = field(default_factory=list)
    rank_changes: list[RankChange] = field(default_factory=list)
    new_paths: list[str] = field(default_factory=list)
    new_queries: list[str] = field(default_factory=list)


def rank_key(path: str) -> str:
    return f"{path}_rank_at_10"


def validate_report(report: dict) -> None:
    if not isinstance(report, dict):
        raise ReportError("report is not a JSON object")
    version = report.get("schema_version")
    if version not in SUPPORTED_SCHEMA_VERSIONS:
        raise ReportError(f"unsupported schema_version {version!r}")
    for key in ("case_set", "paths", "queries"):
        if key not in report:
            raise ReportError(f"report is missing {key!r}")
    if not isinstance(report["paths"], dict) or not report["paths"]:
        raise ReportError("report has no paths")
    for name, metrics in report["paths"].items():
        if not isinstance(metrics, dict) or "recall_at_5" not in metrics:
            raise ReportError(f"path {name!r} has no recall_at_5")
    seen = set()
    for query in report["queries"]:
        text = query.get("query")
        if not isinstance(text, str) or not text.strip():
            raise ReportError("every query needs non-empty text")
        if text in seen:
            raise ReportError(f"duplicate query {text!r}")
        seen.add(text)
        for name in report["paths"]:
            if rank_key(name) not in query:
                raise ReportError(f"query {text!r} has no {rank_key(name)}")


def compare(reference: dict, current: dict, max_recall_drop: float = DEFAULT_MAX_RECALL_DROP) -> CheckResult:
    validate_report(reference)
    validate_report(current)
    if reference["case_set"] != current["case_set"]:
        raise ReportError(
            f"case set {current['case_set']!r} cannot be compared with reference {reference['case_set']!r}"
        )

    ref_paths = reference["paths"]
    cur_paths = current["paths"]
    result = CheckResult(
        case_set=reference["case_set"],
        max_recall_drop=max_recall_drop,
        paths=[name for name in ref_paths if name in cur_paths],
        reference_paths=ref_paths,
        current_paths=cur_paths,
        new_paths=[name for name in cur_paths if name not in ref_paths],
        reference_top1=reference.get("top1_agreement") or {},
        current_top1=current.get("top1_agreement") or {},
    )

    for name in ref_paths:
        if name not in cur_paths:
            result.failures.append(f"path {name} is missing from the current report")
            continue
        before = ref_paths[name]["recall_at_5"]
        after = cur_paths[name]["recall_at_5"]
        if before - after > max_recall_drop + EPSILON:
            result.failures.append(
                f"{name} Recall@5 dropped {before:.3f} -> {after:.3f} "
                f"({after - before:+.3f}, allowed -{max_recall_drop:.3f})"
            )

    current_queries = {query["query"]: query for query in current["queries"]}
    reference_texts = set()
    for ref_query in reference["queries"]:
        text = ref_query["query"]
        reference_texts.add(text)
        cur_query = current_queries.get(text)
        if cur_query is None:
            result.failures.append(f"query {text!r} is missing from the current report")
            continue
        for name in result.paths:
            before = ref_query.get(rank_key(name))
            after = cur_query.get(rank_key(name))
            if before == after:
                continue
            result.rank_changes.append(RankChange(text, ref_query.get("kind", ""), name, before, after))
            if before is not None and after is None:
                result.failures.append(
                    f"{name} lost {text!r} ({ref_query.get('kind', '')}): rank {before} -> miss"
                )

    result.new_queries = [query["query"] for query in current["queries"] if query["query"] not in reference_texts]
    return result


def fmt_rank(rank: int | None) -> str:
    return "miss" if rank is None else str(rank)


def fmt_delta(before, after) -> str:
    if before is None or after is None:
        return "n/a"
    return f"{after - before:+.3f}"


def fmt_score(score) -> str:
    return "none" if score is None else f"{score:.3f}"


def fmt_agreement(agreement: dict) -> str:
    if not agreement:
        return "n/a"
    return f"{agreement.get('count', 0)}/{agreement.get('total', 0)}"


def cell(text: str) -> str:
    return str(text).replace("|", "\\|").replace("\n", " ")


def render(result: CheckResult) -> str:
    verdict = "PASS" if not result.failures else "FAIL"
    lines = [
        f"# Retrieval check: {result.case_set}: {verdict}",
        "",
        f"Gate: Recall@5 may drop at most {result.max_recall_drop:.2f} on any path, and no query found in a path's top ten may become a miss.",
        "",
        "| Path | Recall@5 ref | Recall@5 now | Delta | MRR@10 ref | MRR@10 now | Delta | p95 ms ref | p95 ms now |",
        "|---|---:|---:|---:|---:|---:|---:|---:|---:|",
    ]
    for name in result.paths:
        ref = result.reference_paths[name]
        cur = result.current_paths[name]
        lines.append(
            f"| {name} | {ref['recall_at_5']:.3f} | {cur['recall_at_5']:.3f} | {fmt_delta(ref['recall_at_5'], cur['recall_at_5'])} "
            f"| {ref.get('mrr_at_10', 0):.3f} | {cur.get('mrr_at_10', 0):.3f} | {fmt_delta(ref.get('mrr_at_10'), cur.get('mrr_at_10'))} "
            f"| {ref.get('latency_ms', {}).get('p95', 'n/a')} | {cur.get('latency_ms', {}).get('p95', 'n/a')} |"
        )
    for name in result.new_paths:
        cur = result.current_paths[name]
        lines.append(
            f"| {name} (new) | n/a | {cur['recall_at_5']:.3f} | n/a | n/a | {cur.get('mrr_at_10', 0):.3f} | n/a "
            f"| n/a | {cur.get('latency_ms', {}).get('p95', 'n/a')} |"
        )

    if result.reference_top1 or result.current_top1:
        lines += [
            "",
            f"Top-1 agreement: {fmt_agreement(result.reference_top1)} -> {fmt_agreement(result.current_top1)} (reported, not gated).",
        ]

    kinds = sorted(
        {
            kind
            for name in result.paths
            for side in (result.reference_paths[name], result.current_paths[name])
            for kind in side.get("recall_at_5_by_kind", {})
        }
    )
    if kinds:
        lines += ["", "| Path | Kind | Recall@5 ref | Recall@5 now | Delta |", "|---|---|---:|---:|---:|"]
        for name in result.paths:
            ref_kinds = result.reference_paths[name].get("recall_at_5_by_kind", {})
            cur_kinds = result.current_paths[name].get("recall_at_5_by_kind", {})
            for kind in kinds:
                before = ref_kinds.get(kind)
                after = cur_kinds.get(kind)
                lines.append(
                    f"| {name} | {kind} | {'n/a' if before is None else f'{before:.3f}'} "
                    f"| {'n/a' if after is None else f'{after:.3f}'} | {fmt_delta(before, after)} |"
                )

    no_match_rows = [
        (name, result.current_paths[name]["no_match"])
        for name in [*result.paths, *result.new_paths]
        if result.current_paths[name].get("no_match")
    ]
    if no_match_rows:
        lines += [
            "",
            "No-match queries (reported, not gated; VS-12 sets the threshold):",
            "",
            "| Path | Negative cases | Returned nothing | Median top score, negative | Median top score, positive |",
            "|---|---:|---:|---:|---:|",
        ]
        for name, no_match in no_match_rows:
            lines.append(
                f"| {name} | {no_match.get('cases', 0)} | {no_match.get('returned_nothing', 0)} "
                f"| {fmt_score(no_match.get('top_score_median'))} | {fmt_score(no_match.get('positive_top_score_median'))} |"
            )

    if result.failures:
        lines += ["", "## Regressions", ""]
        lines += [f"- {cell(failure)}" for failure in result.failures]

    if result.rank_changes:
        lines += ["", "## Rank changes", "", "| Query | Kind | Path | Ref rank@10 | Now rank@10 |", "|---|---|---|---:|---:|"]
        for change in result.rank_changes:
            lines.append(
                f"| {cell(change.query)} | {change.kind} | {change.path} | {fmt_rank(change.before)} | {fmt_rank(change.after)} |"
            )
    else:
        lines += ["", "No per-query rank changed."]

    if result.new_queries:
        lines += ["", "## New queries (no reference yet)", ""]
        lines += [f"- {cell(query)}" for query in result.new_queries]

    return "\n".join(lines) + "\n"


def load_report(path: Path) -> dict:
    try:
        return json.loads(Path(path).read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise ReportError(f"cannot read {Path(path).name}: {error.__class__.__name__}") from error


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--current", required=True, type=Path)
    parser.add_argument("--max-recall-drop", type=float, default=DEFAULT_MAX_RECALL_DROP)
    parser.add_argument("--out", type=Path, help="also write the markdown report here")
    args = parser.parse_args(argv)
    try:
        result = compare(load_report(args.reference), load_report(args.current), args.max_recall_drop)
    except ReportError as error:
        print(f"retrieval check could not run: {error}", file=sys.stderr)
        return 2
    text = render(result)
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(text)
    print(text, end="")
    return 0 if not result.failures else 1


if __name__ == "__main__":
    sys.exit(main())
