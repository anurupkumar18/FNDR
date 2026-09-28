#!/usr/bin/env python3
"""Aggregate-only health report for an FNDR LanceDB store.

The report deliberately exposes counts, dates, lengths, and percentages only.
It never renders memory text, titles, URLs, or file paths.
"""

from __future__ import annotations

import argparse
import collections
import datetime
import os
from pathlib import Path

import numpy as np


DEFAULT_DB = os.path.expanduser("~/Library/Application Support/com.fndr.app/lancedb")
V4_PARENT_TABLE = "memories_v4_minilm_384"
V5_PARENT_TABLE = "memories_v5_bge_1024"
CHUNK_TABLE = "memory_chunks_v1_bge_1024"

# Backward-compatible names from the original Task 5 interface.
MEMORY_TABLE = V4_PARENT_TABLE
CHUNK_TABLES = (V5_PARENT_TABLE, CHUNK_TABLE)

STRUCTURED_FIELDS = ("project", "topic", "outcome", "next_steps", "decisions", "errors")
EXACT_REOPEN_KINDS = frozenset(("browser_url", "file_path", "app_deep_link"))
PARENT_AGGREGATE_COLUMNS = (
    "timestamp",
    "embedding",
    "clean_text",
    *STRUCTURED_FIELDS,
    "summary_source",
    "reopen_kind",
)
CHUNK_AGGREGATE_COLUMNS = ("created_at", "embedding")

SUMMARY_SOURCE_CATEGORIES = frozenset(
    (
        "browser_semantic",
        "demo_seed",
        "diagnostic",
        "fallback",
        "llm",
        "tracker",
        "url_only",
        "vision_fallback",
        "vision_mtmd",
        "visual_capture",
        "visual_semantics_failed",
        "vlm",
    )
)
_TABLE_SPECS = (
    (V4_PARENT_TABLE, "current parent", PARENT_AGGREGATE_COLUMNS, "timestamp", True),
    (V5_PARENT_TABLE, "future parent", PARENT_AGGREGATE_COLUMNS, "timestamp", True),
    (CHUNK_TABLE, "chunk", CHUNK_AGGREGATE_COLUMNS, "created_at", False),
)


def _connect(db_path: str):
    import lancedb

    return lancedb.connect(db_path)


def _pct(part: int, whole: int) -> float:
    return round(100.0 * part / whole, 1) if whole else 0.0


def _filled(value) -> bool:
    if value is None:
        return False
    if isinstance(value, str):
        normalized = value.strip().casefold()
        return bool(normalized) and normalized != "unknown"
    if isinstance(value, (list, tuple, set, dict)):
        return bool(value)
    return True


def _safe_summary_source(value) -> str:
    if not isinstance(value, str) or not value.strip():
        return "<missing>"
    label = value.strip().casefold()
    return label if label in SUMMARY_SOURCE_CATEGORIES else "<other>"


def _date_bucket(timestamp_ms) -> str | None:
    if timestamp_ms is None:
        return None
    try:
        # Activity is a user-facing daily measure, so bucket in the machine's
        # local timezone just like the capture UI does.
        instant = datetime.datetime.fromtimestamp(float(timestamp_ms) / 1000.0)
    except (OSError, OverflowError, TypeError, ValueError):
        return None
    return instant.date().isoformat()


def _read_aggregate_columns(table, requested: tuple[str, ...]) -> tuple[int, dict[str, list]]:
    """Project only columns needed by the report; never materialize whole rows."""

    available = set(table.schema.names)
    selected = [column for column in requested if column in available]
    if selected:
        arrow = table.search().select(selected).to_arrow()
        rows = arrow.num_rows
        values = {
            column: arrow.column(column).to_pylist() if column in selected else [None] * rows
            for column in requested
        }
        return rows, values

    rows = table.count_rows()
    return rows, {column: [None] * rows for column in requested}


def _empty_table_health(name: str, role: str) -> dict:
    return {
        "table": name,
        "role": role,
        "present": False,
        "rows": 0,
        "first_day": None,
        "last_day": None,
        "active_days": 0,
        "rows_per_active_day": 0.0,
        "zero_or_missing_vectors_pct": None,
        "clean_text_chars_p50": None,
        "structured_pct": None,
        "summary_source": None,
        "exact_reopen_pct": None,
    }


def _table_health(
    table,
    name: str,
    role: str,
    columns: tuple[str, ...],
    time_column: str,
    parent: bool,
) -> dict:
    rows, values = _read_aggregate_columns(table, columns)
    days = collections.Counter(
        day for day in (_date_bucket(value) for value in values[time_column]) if day is not None
    )
    zero_or_missing = 0
    for vector in values["embedding"]:
        if vector is None:
            zero_or_missing += 1
            continue
        try:
            array = np.asarray(vector, dtype=np.float32)
        except (TypeError, ValueError):
            zero_or_missing += 1
            continue
        if array.size == 0 or not np.any(array):
            zero_or_missing += 1

    health = {
        "table": name,
        "role": role,
        "present": True,
        "rows": rows,
        "first_day": min(days) if days else None,
        "last_day": max(days) if days else None,
        "active_days": len(days),
        "rows_per_active_day": round(rows / len(days), 1) if days else 0.0,
        "zero_or_missing_vectors_pct": _pct(zero_or_missing, rows) if rows else None,
        "clean_text_chars_p50": None,
        "structured_pct": None,
        "summary_source": None,
        "exact_reopen_pct": None,
    }
    if not parent:
        return health

    text_lengths = [len(value) if isinstance(value, str) else 0 for value in values["clean_text"]]
    source_counts = collections.Counter(
        _safe_summary_source(value) for value in values["summary_source"]
    )
    exact_reopens = sum(
        1
        for value in values["reopen_kind"]
        if isinstance(value, str) and value.strip() in EXACT_REOPEN_KINDS
    )
    health.update(
        {
            "clean_text_chars_p50": int(np.median(text_lengths)) if text_lengths else None,
            "structured_pct": (
                {
                    field: _pct(sum(_filled(value) for value in values[field]), rows)
                    for field in STRUCTURED_FIELDS
                }
                if rows
                else None
            ),
            "summary_source": dict(sorted(source_counts.items())) if rows else None,
            "exact_reopen_pct": _pct(exact_reopens, rows) if rows else None,
        }
    )
    return health


def _table_names(database) -> list[str]:
    response = database.list_tables()
    names = response.tables if hasattr(response, "tables") else response
    return sorted(str(name) for name in names)


def summarize(db_path: str) -> dict:
    """Return aggregate health metrics without exposing stored content."""

    expanded = Path(db_path).expanduser()
    if not expanded.is_dir():
        raise FileNotFoundError("database directory does not exist")

    database = _connect(str(expanded))
    names = _table_names(database)
    name_set = set(names)
    health = {}
    tables = {}

    for name, role, columns, time_column, parent in _TABLE_SPECS:
        if name not in name_set:
            health[name] = _empty_table_health(name, role)
            continue
        table_health = _table_health(
            database.open_table(name),
            name,
            role,
            columns,
            time_column,
            parent,
        )
        health[name] = table_health
        tables[name] = table_health["rows"]

    # Preserve the original inventory behavior for non-memory tables while
    # reading only their row counts.
    for name in names:
        if name not in tables:
            tables[name] = database.open_table(name).count_rows()

    report = {"tables": dict(sorted(tables.items())), "table_health": health}
    v4 = health[V4_PARENT_TABLE]
    if v4["present"]:
        report.update(
            {
                "memories": v4["rows"],
                "first_day": v4["first_day"],
                "last_day": v4["last_day"],
                "active_days": v4["active_days"],
                "memories_per_active_day": v4["rows_per_active_day"],
                "zero_or_missing_vectors_pct": v4["zero_or_missing_vectors_pct"],
                "clean_text_chars_p50": v4["clean_text_chars_p50"],
                "structured_pct": v4["structured_pct"],
                "summary_source": v4["summary_source"],
                "reopen_specific_pct": v4["exact_reopen_pct"],
                "exact_reopen_pct": v4["exact_reopen_pct"],
                "chunk_rows": {
                    V5_PARENT_TABLE: health[V5_PARENT_TABLE]["rows"],
                    CHUNK_TABLE: health[CHUNK_TABLE]["rows"],
                },
            }
        )
    return report


def _display(value, *, suffix: str = "") -> str:
    return "n/a" if value is None else f"{value}{suffix}"


def render(report: dict) -> str:
    """Render aggregate Markdown that is safe to paste into QA evidence."""

    lines = [
        "# FNDR vault health",
        "",
        "Aggregate counts only; no memory text, titles, URLs, or file paths.",
        "",
        "## Table health",
        "",
        "| table | role | present | rows | active days | date range | rows/active day | "
        "zero/missing vectors | median clean chars | exact reopen |",
        "|---|---|:---:|---:|---:|---|---:|---:|---:|---:|",
    ]
    for name, _, _, _, _ in _TABLE_SPECS:
        row = report["table_health"][name]
        date_range = (
            f"{row['first_day']} to {row['last_day']}"
            if row["first_day"] is not None
            else "n/a"
        )
        lines.append(
            f"| `{name}` | {row['role']} | {'yes' if row['present'] else 'no'} | "
            f"{row['rows']} | {row['active_days']} | {date_range} | "
            f"{row['rows_per_active_day']} | "
            f"{_display(row['zero_or_missing_vectors_pct'], suffix='%')} | "
            f"{_display(row['clean_text_chars_p50'])} | "
            f"{_display(row['exact_reopen_pct'], suffix='%')} |"
        )

    if not report["table_health"][V4_PARENT_TABLE]["present"]:
        lines += ["", f"No `{V4_PARENT_TABLE}` table found."]

    for name in (V4_PARENT_TABLE, V5_PARENT_TABLE):
        row = report["table_health"][name]
        if not row["present"] or not row["rows"]:
            continue
        lines += [
            "",
            f"## Structured fields filled: `{name}`",
            "",
            "| field | filled |",
            "|---|---:|",
        ]
        lines += [f"| {field} | {percent}% |" for field, percent in row["structured_pct"].items()]
        lines += [
            "",
            f"## Summary source: `{name}`",
            "",
            "| source | rows |",
            "|---|---:|",
        ]
        lines += [
            f"| {source} | {count} |"
            for source, count in sorted(
                row["summary_source"].items(),
                key=lambda item: (-item[1], item[0]),
            )
        ]
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Aggregate-only FNDR vault health report")
    parser.add_argument("--db", default=DEFAULT_DB, help="FNDR LanceDB directory")
    parser.add_argument("--out", help="optional Markdown output file")
    args = parser.parse_args(argv)

    try:
        report = summarize(args.db)
    except FileNotFoundError as error:
        parser.error(str(error))
    text = render(report)
    if args.out:
        output = Path(args.out).expanduser()
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(text, encoding="utf-8")
    print(text, end="")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
