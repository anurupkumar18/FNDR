import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import decision_report as dr


def overconfident_rows():
    # Always confident in option 0, right 6 of 10 times.
    rows = [[4.0, 0.0]] * 10
    labels = [0, 0, 0, 0, 0, 0, 1, 1, 1, 1]
    return rows, labels


class ReportTests(unittest.TestCase):
    def test_report_shows_calibration_improving_and_a_threshold_table(self):
        rows, labels = overconfident_rows()
        text = dr.report("merge", rows, labels, rows, labels, same_file=False)
        self.assertIn("# Decision report: merge", text)
        self.assertIn("accuracy 60.0 percent", text)
        self.assertIn("| 0.90 | not reachable | 0 percent | - |", text)
        self.assertNotIn("Warning", text)
        self.assertIn("A valid answer is not a correct answer", text)

    def test_same_file_fitting_is_flagged(self):
        rows, labels = overconfident_rows()
        self.assertIn("Warning: the temperature was fitted on the same examples", dr.report("x", rows, labels, rows, labels, True))

    def test_a_reachable_target_reports_coverage(self):
        rows = [[3.0, 0.0]] * 8 + [[0.2, 0.0]] * 2
        labels = [0] * 8 + [1, 1]
        text = dr.report("x", rows, labels, rows, labels, False)
        self.assertIn("| 0.99 |", text)
        self.assertRegex(text, r"\| 0\.9[059] \| 0\.\d{3} \| 80 percent \| 1\.000 \|")

    def test_cli_with_separate_fit_file_and_empty_input(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            rows, labels = overconfident_rows()
            body = "\n".join(json.dumps({"logits": r, "label": y}) for r, y in zip(rows, labels)) + "\n"
            (d / "eval.jsonl").write_text(body)
            (d / "fit.jsonl").write_text(body)
            self.assertEqual(dr.main([str(d / "eval.jsonl"), "--fit-jsonl", str(d / "fit.jsonl"), "--out", str(d / "o" / "r.md")]), 0)
            self.assertNotIn("Warning", (d / "o" / "r.md").read_text())
            (d / "empty.jsonl").write_text("")
            with self.assertRaises(ValueError):
                dr.load(d / "empty.jsonl")


if __name__ == "__main__":
    unittest.main()
