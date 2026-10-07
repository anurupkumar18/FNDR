import csv
import io
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import recall_chart as chart  # noqa: E402


class RecallChartTests(unittest.TestCase):
    def test_rows_cover_every_persona_path_and_stage(self):
        data = chart.rows(chart.default_sources())
        keys = {(row["persona"], row["path"], row["stage"]) for row in data}
        self.assertEqual(len(data), len(chart.PERSONAS) * len(chart.PATHS) * 2)
        self.assertIn(("office-pm", "search", "before"), keys)
        self.assertIn(("knowledge-worker", "ask", "after"), keys)

    def test_committed_csv_matches_the_committed_reports(self):
        # The chart data in the repo must be what the command produces now,
        # so a stale CSV fails here instead of reaching a slide.
        expected = io.StringIO()
        writer = csv.DictWriter(expected, fieldnames=chart.CSV_FIELDS, lineterminator="\n")
        writer.writeheader()
        writer.writerows(chart.rows(chart.default_sources()))
        committed = (chart.ROOT / "docs/evidence/W03/beta-demo-recall.csv").read_bytes().decode()
        self.assertEqual(committed, expected.getvalue())

    @unittest.skipUnless(importlib.util.find_spec("matplotlib"), "matplotlib not installed")
    def test_png_is_written_and_identical_on_a_second_run(self):
        data = chart.rows(chart.default_sources())
        with tempfile.TemporaryDirectory() as directory:
            first = Path(directory) / "first.png"
            second = Path(directory) / "second.png"
            chart.write_png(data, first)
            chart.write_png(data, second)
            self.assertEqual(first.read_bytes()[:8], b"\x89PNG\r\n\x1a\n")
            self.assertEqual(first.read_bytes(), second.read_bytes())


if __name__ == "__main__":
    unittest.main()
