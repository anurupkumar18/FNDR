#!/usr/bin/env python3
"""Print the Friday scoreboard (PD-05) as one Markdown page.

Inputs are files other tools already wrote: retrieval_qa JSON reports (schema
v1 or v2, one per case set), the vault health Markdown from vault_health.py,
and optional voice and user-session evidence files that are linked by name,
not parsed. A missing or unreadable input prints "not measured" instead of
failing, so the page can be posted every week whatever exists. The page shows
file names only, never directories.

Usage:
  scoreboard.py [--retrieval FILE]... [--vault-health FILE] [--voice FILE]
                [--sessions FILE] [--date YYYY-MM-DD] [--out FILE]
"""

from __future__ import annotations

import argparse
import datetime
import json
import re
import sys
from pathlib import Path

SUPPORTED_SCHEMA_VERSIONS = frozenset((1, 2))
# Recall values are ratios of small integers; allow float noise at a target.
EPSILON = 1e-9

# Month plan section 3, Beta targets.
SEARCH_RECALL_TARGET = 0.90
PARAPHRASE_RECALL_TARGET = 0.80
MEDIAN_TEXT_TARGET = 800
STRUCTURED_TARGET_PCT = 60.0
EXACT_REOPEN_TARGET_PCT = 90.0
TARGETED_STRUCTURED_FIELDS = (("project", "Memories with a project"), ("next_steps", "Memories with next steps"))

NOT_MEASURED = "not measured"
LONG_DASHES = {0x2013: "-", 0x2014: "-"}
TEXT_SOURCE_CATEGORIES = frozenset(("ax", "ocr", "browser_semantic", "mixed", "unknown"))


# Retrieval reports


def load_retrieval(path: Path) -> tuple[dict | None, str | None]:
    """Return (report, None) or (None, why it was skipped)."""
    if not path.is_file():
        return None, "not found"
    try:
        report = json.loads(path.read_text())
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        return None, "could not read"
    if not isinstance(report, dict) or not isinstance(report.get("paths"), dict):
        return None, "could not read"
    version = report.get("schema_version")
    if version not in SUPPORTED_SCHEMA_VERSIONS:
        return None, f"unsupported schema_version {version}"
    return report, None


def number(value) -> float | None:
    return value if isinstance(value, (int, float)) and not isinstance(value, bool) else None


def fmt_ratio(value) -> str:
    value = number(value)
    return "n/a" if value is None else f"{value:.3f}"


def fmt_plain(value) -> str:
    value = number(value)
    if value is None:
        return "n/a"
    return str(int(value)) if float(value).is_integer() else str(value)


def top1(report: dict) -> tuple[int, int] | None:
    agreement = report.get("top1_agreement")
    if not isinstance(agreement, dict):
        return None
    count, total = number(agreement.get("count")), number(agreement.get("total"))
    if count is None or not total:
        return None
    return int(count), int(total)


def fmt_top1(report: dict) -> str:
    pair = top1(report)
    if pair is None:
        return "n/a"
    count, total = pair
    return f"{count}/{total} ({count / total:.3f})"


def paraphrase_recall(metrics: dict):
    by_kind = metrics.get("recall_at_5_by_kind")
    return number(by_kind.get("paraphrase")) if isinstance(by_kind, dict) else None


def status(value, target) -> str:
    if value is None:
        return NOT_MEASURED
    return "met" if value + EPSILON >= target else "not met"


def cell(text) -> str:
    return str(text).replace("|", "\\|").replace("\n", " ")


def retrieval_section(paths: list[Path]) -> list[str]:
    lines = ["## Retrieval", ""]
    reports = []
    for path in paths:
        report, problem = load_retrieval(path)
        if problem:
            lines.append(f"- {path.name}: {problem}, {NOT_MEASURED}.")
            continue
        reports.append(report)
        lines.append(
            f"- {cell(report.get('case_set', 'unnamed'))} from {path.name} "
            f"(schema v{report['schema_version']}, {fmt_plain(report.get('case_count'))} cases)"
        )
    if not reports:
        if not paths:
            lines.append(f"No retrieval report given: {NOT_MEASURED}.")
        return lines

    lines += [
        "",
        "| Persona | Path | Recall@5 | Paraphrase Recall@5 | MRR@10 | p95 ms | Top-1 agreement |",
        "|---|---|---:|---:|---:|---:|---:|",
    ]
    for report in reports:
        persona = cell(report.get("case_set", "unnamed"))
        for name, metrics in report["paths"].items():
            metrics = metrics if isinstance(metrics, dict) else {}
            latency = metrics.get("latency_ms") if isinstance(metrics.get("latency_ms"), dict) else {}
            lines.append(
                f"| {persona} | {cell(name)} | {fmt_ratio(metrics.get('recall_at_5'))} "
                f"| {fmt_ratio(paraphrase_recall(metrics))} | {fmt_ratio(metrics.get('mrr_at_10'))} "
                f"| {fmt_plain(latency.get('p95'))} | {fmt_top1(report)} |"
            )

    lines += [
        "",
        "Beta targets (month plan section 3):",
        "",
        "| Persona | Measure | Beta target | Measured | Status |",
        "|---|---|---|---:|---|",
    ]
    for report in reports:
        persona = cell(report.get("case_set", "unnamed"))
        search = report["paths"].get("search")
        search = search if isinstance(search, dict) else {}
        recall = number(search.get("recall_at_5"))
        paraphrase = paraphrase_recall(search)
        pair = top1(report)
        lines += [
            f"| {persona} | Search Recall@5 | 0.90 or higher | {fmt_ratio(recall)} | {status(recall, SEARCH_RECALL_TARGET)} |",
            f"| {persona} | Paraphrase Recall@5 (Search) | 0.80 or higher | {fmt_ratio(paraphrase)} "
            f"| {status(paraphrase, PARAPHRASE_RECALL_TARGET)} |",
            f"| {persona} | Search and Ask same top result | always "
            f"| {'n/a' if pair is None else f'{pair[0]}/{pair[1]}'} "
            f"| {NOT_MEASURED if pair is None else ('met' if pair[0] == pair[1] else 'not met')} |",
        ]
    return lines


# Vault health


def markdown_tables(text: str) -> list[tuple[str, list[str], list[list[str]]]]:
    """Every pipe table in the text as (nearest '## ' heading, header, rows)."""
    tables = []
    heading = ""
    lines = text.splitlines()
    index = 0
    while index < len(lines):
        line = lines[index].strip()
        if line.startswith("## "):
            heading = line[3:].strip()
        is_table = (
            line.startswith("|")
            and index + 1 < len(lines)
            and re.fullmatch(r"\|[\s:|-]+\|", lines[index + 1].strip()) is not None
        )
        if not is_table:
            index += 1
            continue
        header = split_row(line)
        rows = []
        index += 2
        while index < len(lines) and lines[index].strip().startswith("|"):
            rows.append(split_row(lines[index].strip()))
            index += 1
        tables.append((heading, header, rows))
    return tables


def split_row(line: str) -> list[str]:
    return [part.strip().strip("`").strip() for part in line.strip().strip("|").split("|")]


def parse_number(text: str | None) -> float | None:
    if text is None:
        return None
    match = re.fullmatch(r"(-?\d+(?:\.\d+)?)\s*%?", text.strip())
    return float(match.group(1)) if match else None


def parse_int(text: str | None) -> int | None:
    value = parse_number(text)
    return None if value is None else int(value)


def parse_vault_health(text: str) -> dict:
    """Aggregate numbers from vault_health.py Markdown; anything absent is None."""
    parsed = {
        "memories": None,
        "active_days": None,
        "memories_per_active_day": None,
        "median_clean_chars": None,
        "exact_reopen_pct": None,
        "chunk_rows": None,
        "structured_pct": {},
        "text_source": {},
    }
    parent_table = None
    for heading, header, rows in markdown_tables(text):
        if heading == "Table health":
            for row in rows:
                cells = dict(zip(header, row))
                role = cells.get("role")
                if role == "current parent" and parent_table is None:
                    parent_table = cells.get("table")
                    parsed["memories"] = parse_int(cells.get("rows"))
                    parsed["active_days"] = parse_int(cells.get("active days"))
                    parsed["memories_per_active_day"] = parse_number(cells.get("rows/active day"))
                    parsed["median_clean_chars"] = parse_int(cells.get("median clean chars"))
                    parsed["exact_reopen_pct"] = parse_number(cells.get("exact reopen"))
                elif role == "chunk" and parsed["chunk_rows"] is None:
                    parsed["chunk_rows"] = parse_int(cells.get("rows"))
        elif heading.startswith("Structured fields filled") and not parsed["structured_pct"]:
            if parent_table and parent_table not in heading:
                continue
            for row in rows:
                cells = dict(zip(header, row))
                value = parse_number(cells.get("filled"))
                if cells.get("field") and value is not None:
                    parsed["structured_pct"][cells["field"]] = value
        elif parent_table and heading == f"Text source: `{parent_table}`":
            for row in rows:
                cells = dict(zip(header, row))
                source = cells.get("source")
                if source not in TEXT_SOURCE_CATEGORIES:
                    continue
                metrics = {
                    "rows": parse_int(cells.get("rows")),
                    "share_pct": parse_number(cells.get("share")),
                    "clean_text_chars_p50": parse_number(cells.get("median clean chars")),
                    "under_200_chars_pct": parse_number(cells.get("under 200 chars")),
                }
                if all(value is not None for value in metrics.values()):
                    parsed["text_source"][source] = metrics
    return parsed


def fmt_pct(value) -> str:
    return NOT_MEASURED if value is None else f"{value:.1f}%"


def vault_section(path: Path | None) -> list[str]:
    lines = ["## Vault health", ""]
    if path is None:
        return lines + [f"No vault health file given: {NOT_MEASURED}."]
    if not path.is_file():
        return lines + [f"- {path.name}: not found, {NOT_MEASURED}."]
    try:
        parsed = parse_vault_health(path.read_text())
    except (OSError, UnicodeDecodeError):
        return lines + [f"- {path.name}: could not read, {NOT_MEASURED}."]

    per_day = parsed["memories_per_active_day"]
    if per_day is None:
        per_day_text = NOT_MEASURED
    else:
        per_day_text = fmt_plain(per_day)
        if parsed["memories"] is not None and parsed["active_days"] is not None:
            per_day_text += f" ({parsed['memories']} memories, {parsed['active_days']} active days)"
    median = parsed["median_clean_chars"]
    chunks = parsed["chunk_rows"]
    if chunks is None:
        chunk_status = NOT_MEASURED
    elif chunks == 0 and parsed["memories"]:
        chunk_status = "not met"
    else:
        chunk_status = "unknown"
    structured = parsed["structured_pct"]

    lines += [
        f"From {path.name}, current parent table.",
        "",
        "| Measure | Value | Beta target | Status |",
        "|---|---|---|---|",
        f"| Memories per active day | {per_day_text} | none | n/a |",
        f"| Median clean text per memory | {NOT_MEASURED if median is None else f'{median} characters'} "
        f"| 800 or more | {status(median, MEDIAN_TEXT_TARGET)} |",
    ]
    for field, label in TARGETED_STRUCTURED_FIELDS:
        value = structured.get(field)
        lines.append(f"| {label} | {fmt_pct(value)} | 60% or more | {status(value, STRUCTURED_TARGET_PCT)} |")
    targeted = {field for field, _ in TARGETED_STRUCTURED_FIELDS}
    others = [f"{cell(field)} {value:.1f}%" for field, value in structured.items() if field not in targeted]
    if others:
        lines.append(f"| Other structured fields | {', '.join(others)} | none | n/a |")
    reopen = parsed["exact_reopen_pct"]
    lines += [
        f"| Chunk rows | {NOT_MEASURED if chunks is None else chunks} | every memory | {chunk_status} |",
        f"| Memories that reopen exactly | {fmt_pct(reopen)} | 90% of a live day | {status(reopen, EXACT_REOPEN_TARGET_PCT)} |",
        "",
        "The Beta target counts memories with both a project and next steps; each field's share is an upper bound for it. "
        "Chunk rows count chunks, not memories, so a non-zero count does not prove every memory is covered. "
        "Exact reopen is the stored share across the whole vault, not a live day.",
    ]
    sources = parsed["text_source"]
    if sources:
        lines += [
            "", "Text source breakdown (known observation lineage; older merges may be incomplete):", "",
            "| source | rows | share | median clean chars | under 200 chars |",
            "|---|---:|---:|---:|---:|",
        ]
        lines += [
            f"| {source} | {metrics['rows']} | {fmt_pct(metrics['share_pct'])} | "
            f"{fmt_plain(metrics['clean_text_chars_p50'])} | {fmt_pct(metrics['under_200_chars_pct'])} |"
            for source, metrics in sorted(sources.items())
        ]
    else:
        lines += ["", f"Text source breakdown: {NOT_MEASURED}."]
    return lines


# Linked evidence and the page


def linked_and_pending(voice: Path | None, sessions: Path | None) -> list[str]:
    linked = []
    pending = []
    for label, target, path in (
        ("Voice latency", "first partial text 0.5 s, final text 1 s after release", voice),
        ("Voice command success", "45 of 50 on the utterance script", None),
        ("Reopen outcome share", "RE-13 runtime counters", None),
        ("User sessions", "PD-18 interviews and outside users", sessions),
    ):
        if path is not None and path.is_file():
            linked.append(f"- {label}: see `{path.name}`.")
        else:
            missing = f" ({path.name} not found)" if path is not None else ""
            pending.append(f"- {label} ({target}): {NOT_MEASURED} yet{missing}.")
    lines = []
    if linked:
        lines += ["## Linked evidence", "", *linked, ""]
    lines += ["## Not measured yet", "", *pending]
    return lines


def render(
    date: str,
    retrieval: list[Path],
    vault_health: Path | None,
    voice: Path | None,
    sessions: Path | None,
) -> str:
    lines = [
        f"# FNDR Friday scoreboard: {date}",
        "",
        "Numbers come from the files named below; nothing is typed by hand. Regenerate with `make scoreboard`.",
        "",
        *retrieval_section(retrieval),
        "",
        *vault_section(vault_health),
        "",
        *linked_and_pending(voice, sessions),
    ]
    return ("\n".join(lines) + "\n").translate(LONG_DASHES)


def iso_date(text: str) -> str:
    try:
        return datetime.date.fromisoformat(text).isoformat()
    except ValueError as error:
        raise argparse.ArgumentTypeError("use YYYY-MM-DD") from error


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--retrieval", action="append", type=Path, default=[], help="retrieval_qa JSON report (repeatable)")
    parser.add_argument("--vault-health", type=Path, help="vault_health.py Markdown output")
    parser.add_argument("--voice", type=Path, help="voice latency evidence to link")
    parser.add_argument("--sessions", type=Path, help="user session notes to link")
    parser.add_argument("--date", type=iso_date, default=datetime.date.today().isoformat())
    parser.add_argument("--out", type=Path, help="also write the page here")
    args = parser.parse_args(argv)

    text = render(args.date, args.retrieval, args.vault_health, args.voice, args.sessions)
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(text)
    print(text, end="")
    return 0


if __name__ == "__main__":
    sys.exit(main())
