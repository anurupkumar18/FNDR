import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

import lancedb
import pyarrow as pa

sys.path.insert(0, str(Path(__file__).parent))
import vault_health as vh  # noqa: E402


DAY_MS = 86_400_000
START_MS = 1_758_600_000_000
SECRET = "PRIVATE raw title url path and memory text"


def fixed_vectors(values):
    return pa.array(values, type=pa.list_(pa.float32(), 2))


def parent_rows(*, second_vector, second_reopen):
    return pa.table(
        {
            "timestamp": [START_MS, START_MS + DAY_MS],
            "embedding": fixed_vectors([[0.1, 0.2], second_vector]),
            "clean_text": [SECRET[:20], "y" * 300],
            "project": ["Essay", ""],
            "topic": ["unknown", "Drafting"],
            "outcome": ["", "Done"],
            "next_steps": [["Finish"], []],
            "decisions": pa.array([[], ["Chose"]], type=pa.list_(pa.string())),
            "errors": pa.array([[], ["Failed"]], type=pa.list_(pa.string())),
            "summary_source": ["llm", "fallback"],
            "raw_evidence": [json.dumps({"source_kind": "ax", "private": SECRET}),
                             json.dumps({"source_kind": "ocr", "private": SECRET})],
            "reopen_kind": ["browser_url", second_reopen],
            # These sensitive columns exist to prove the audit never selects or renders them.
            "title": [SECRET, SECRET],
            "url": [f"https://example.invalid/{SECRET}", None],
            "reopen_file_path": [None, f"/Users/person/{SECRET}"],
        }
    )


class RecordingQuery:
    def __init__(self, source, selections):
        self.source = source
        self.selections = selections
        self.columns = None

    def select(self, columns):
        self.columns = list(columns)
        self.selections.append(self.columns)
        return self

    def to_arrow(self):
        return self.source.select(self.columns)


class RecordingTable:
    def __init__(self, source, selections):
        self.source = source
        self.schema = source.schema
        self.selections = selections

    def count_rows(self):
        return self.source.num_rows

    def search(self):
        return RecordingQuery(self.source, self.selections)


class RecordingDatabase:
    def __init__(self, tables):
        self.tables = tables

    def list_tables(self):
        return SimpleNamespace(tables=list(self.tables))

    def open_table(self, name):
        return self.tables[name]


class VaultHealthTest(unittest.TestCase):
    def test_non_ascii_source_labels_do_not_alias_known_categories(self):
        for evidence, expected in [
            ({"source_kind": "browser_ſemantic"}, "unknown"),
            ({"source_kind": "ax", "text_source_kinds": ["browser_ſemantic"]}, "unknown"),
            ({"text_source_kinds": ["browser_ſemantic", "browser_semantic"]}, "mixed"),
        ]:
            with self.subTest(evidence=evidence):
                self.assertEqual(vh._safe_text_source(json.dumps(evidence)), expected)

    def test_text_source_lineage_is_allowlisted_and_array_takes_precedence(self):
        for evidence, expected in [
            ({"source_kind": "OCR"}, "ocr"),
            ({"source_kind": "mixed"}, "mixed"),
            ({"source_kind": "ax", "text_source_kinds": ["ocr", "ocr"]}, "ocr"),
            ({"source_kind": "ax", "text_source_kinds": ["ax", "ocr"]}, "mixed"),
            ({"source_kind": "ax", "text_source_kinds": ["ax", SECRET]}, "mixed"),
            ({"source_kind": "ax", "text_source_kinds": [None, "ax"]}, "mixed"),
            ({"source_kind": "ax", "text_source_kinds": []}, "ax"),
            ({"source_kind": "ax", "text_source_kinds": "ocr"}, "ax"),
            ({"source_kind": "visual_capture"}, "unknown"),
            ({"text_source_kinds": [SECRET, None]}, "unknown"),
            ({"source_kind": SECRET}, "unknown"),
            ([], "unknown"),
        ]:
            self.assertEqual(vh._safe_text_source(json.dumps(evidence)), expected)
        for raw in [None, "", "{malformed", "null"]:
            self.assertEqual(vh._safe_text_source(raw), "unknown")

    def test_text_source_table_reports_only_aggregate_lengths_and_shares(self):
        source = pa.table({
            "clean_text": ["x" * n for n in [100, 300, 199, 200, 50, 500, 150, 250]],
            "raw_evidence": [json.dumps({"source_kind": kind, "private": SECRET})
                             for kind in ["ax", "ax", "ocr", "browser_semantic", "mixed", SECRET]]
                            + [None, "malformed"],
        })
        selections = []
        row = vh._table_health(RecordingTable(source, selections), vh.V4_PARENT_TABLE,
                               "current parent", vh.PARENT_AGGREGATE_COLUMNS, "timestamp", True)
        self.assertEqual(row["text_source"]["ax"], {
            "rows": 2, "share_pct": 25.0, "clean_text_chars_p50": 200,
            "under_200_chars_pct": 50.0,
        })
        self.assertEqual(row["text_source"]["unknown"]["rows"], 3)
        self.assertEqual(row["text_source"]["unknown"]["clean_text_chars_p50"], 250)
        self.assertEqual(row["text_source"]["browser_semantic"]["under_200_chars_pct"], 0.0)
        self.assertNotIn(SECRET, json.dumps(row))
        _, projected = vh._read_aggregate_columns(RecordingTable(source, []), vh.PARENT_AGGREGATE_COLUMNS)
        self.assertNotIn(SECRET, json.dumps(projected))

    def test_legacy_rows_without_evidence_are_unknown_not_ocr(self):
        source = parent_rows(second_vector=[0.0, 0.0], second_reopen="app_bundle").drop(["raw_evidence"])
        row = vh._table_health(RecordingTable(source, []), vh.V4_PARENT_TABLE,
                               "current parent", vh.PARENT_AGGREGATE_COLUMNS, "timestamp", True)
        self.assertEqual(row["text_source"], {"unknown": {
            "rows": 2, "share_pct": 100.0, "clean_text_chars_p50": 160,
            "under_200_chars_pct": 50.0,
        }})

    def test_text_source_median_preserves_fractional_character_counts(self):
        source = pa.table({"clean_text": ["x" * 199, "x" * 200]})
        row = vh._table_health(RecordingTable(source, []), vh.V4_PARENT_TABLE,
                               "current parent", vh.PARENT_AGGREGATE_COLUMNS, "timestamp", True)
        self.assertEqual(row["text_source"]["unknown"]["clean_text_chars_p50"], 199.5)
        self.assertEqual(row["text_source"]["unknown"]["under_200_chars_pct"], 50.0)

    def seed_all_tables(self, directory):
        db = lancedb.connect(directory)
        db.create_table(
            vh.V4_PARENT_TABLE,
            parent_rows(second_vector=[0.0, 0.0], second_reopen="app_bundle"),
        )
        db.create_table(
            vh.V5_PARENT_TABLE,
            parent_rows(second_vector=[0.3, 0.4], second_reopen="file_path").slice(0, 1),
        )
        db.create_table(
            vh.CHUNK_TABLE,
            pa.table(
                {
                    "created_at": [START_MS, START_MS + DAY_MS],
                    "embedding": fixed_vectors([[0.2, 0.1], [0.0, 0.0]]),
                    "text": [SECRET, SECRET],
                    "window_title": [SECRET, SECRET],
                }
            ),
        )

    def test_reports_v4_v5_and_chunk_health_separately_without_raw_values(self):
        with tempfile.TemporaryDirectory() as directory:
            self.seed_all_tables(directory)
            report = vh.summarize(directory)

        v4 = report["table_health"][vh.V4_PARENT_TABLE]
        v5 = report["table_health"][vh.V5_PARENT_TABLE]
        chunks = report["table_health"][vh.CHUNK_TABLE]

        self.assertEqual(
            (v4["role"], v4["rows"], v4["active_days"]),
            ("current parent", 2, 2),
        )
        self.assertEqual(v4["zero_or_missing_vectors_pct"], 50.0)
        self.assertEqual(v4["clean_text_chars_p50"], 160)
        self.assertEqual(v4["structured_pct"]["project"], 50.0)
        self.assertEqual(v4["structured_pct"]["topic"], 50.0)
        self.assertEqual(v4["structured_pct"]["next_steps"], 50.0)
        self.assertEqual(v4["summary_source"], {"fallback": 1, "llm": 1})
        self.assertEqual(v4["exact_reopen_pct"], 50.0)

        self.assertEqual((v5["role"], v5["rows"]), ("future parent", 1))
        self.assertEqual(v5["zero_or_missing_vectors_pct"], 0.0)
        self.assertEqual(v5["exact_reopen_pct"], 100.0)

        self.assertEqual((chunks["role"], chunks["rows"], chunks["active_days"]), ("chunk", 2, 2))
        self.assertEqual(chunks["zero_or_missing_vectors_pct"], 50.0)
        self.assertIsNone(chunks["clean_text_chars_p50"])
        self.assertIsNone(chunks["exact_reopen_pct"])

        # Keep the original Task 5 v4 keys usable by downstream evidence scripts.
        self.assertEqual(report["memories"], 2)
        self.assertEqual(report["chunk_rows"], {vh.V5_PARENT_TABLE: 1, vh.CHUNK_TABLE: 2})
        rendered = vh.render(report)
        for table in (vh.V4_PARENT_TABLE, vh.V5_PARENT_TABLE, vh.CHUNK_TABLE):
            self.assertIn(table, rendered)
        self.assertNotIn(SECRET, rendered)

    def test_reads_only_the_columns_needed_for_aggregate_metrics(self):
        selections = []
        source = parent_rows(second_vector=[0.0, 0.0], second_reopen="app_bundle")
        database = RecordingDatabase(
            {vh.V4_PARENT_TABLE: RecordingTable(source, selections)}
        )

        with tempfile.TemporaryDirectory() as directory:
            with mock.patch.object(vh, "_connect", return_value=database):
                vh.summarize(directory)

        self.assertEqual(selections, [list(vh.PARENT_AGGREGATE_COLUMNS)])
        selected = set(selections[0])
        self.assertTrue(selected.isdisjoint({"title", "url", "reopen_file_path", "text"}))

    def test_summary_provenance_allows_known_categories_only(self):
        self.assertEqual(vh._safe_summary_source("Visual_Capture"), "visual_capture")
        self.assertEqual(vh._safe_summary_source("PrivateCustomerName"), "<other>")
        self.assertEqual(vh._safe_summary_source(""), "<missing>")

    def test_missing_database_is_refused_without_creating_it_or_output(self):
        with tempfile.TemporaryDirectory() as directory:
            missing = Path(directory) / "does-not-exist"
            output = Path(directory) / "report.md"
            stderr = io.StringIO()
            with contextlib.redirect_stderr(stderr), self.assertRaises(SystemExit) as raised:
                vh.main(["--db", str(missing), "--out", str(output)])

            self.assertEqual(raised.exception.code, 2)
            self.assertIn("database directory does not exist", stderr.getvalue())
            self.assertFalse(missing.exists())
            self.assertFalse(output.exists())

    def test_cli_writes_only_the_aggregate_report(self):
        with tempfile.TemporaryDirectory() as directory:
            db_path = Path(directory) / "vault"
            db_path.mkdir()
            self.seed_all_tables(str(db_path))
            output = Path(directory) / "vault-health.md"
            stdout = io.StringIO()

            with contextlib.redirect_stdout(stdout):
                self.assertEqual(
                    vh.main(["--db", str(db_path), "--out", str(output)]),
                    0,
                )

            self.assertTrue(output.is_file())
            self.assertNotIn(SECRET, output.read_text())
            self.assertNotIn(SECRET, stdout.getvalue())

    def test_missing_v4_table_is_reported_not_raised(self):
        with tempfile.TemporaryDirectory() as directory:
            lancedb.connect(directory).create_table("tasks", pa.table({"id": ["task-1"]}))
            report = vh.summarize(directory)

        self.assertNotIn("memories", report)
        self.assertFalse(report["table_health"][vh.V4_PARENT_TABLE]["present"])
        rendered = vh.render(report)
        self.assertIn(f"No `{vh.V4_PARENT_TABLE}` table found.", rendered)
        self.assertIn(f"| `{vh.V4_PARENT_TABLE}` | current parent | no |", rendered)
        self.assertIn("n/a", rendered)


if __name__ == "__main__":
    unittest.main()
