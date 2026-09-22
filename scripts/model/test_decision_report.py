import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import decision_report as dr


def overconfident_rows():
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
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            rows, labels = overconfident_rows()
            body = "\n".join(json.dumps({"logits": row, "label": label}) for row, label in zip(rows, labels)) + "\n"
            (directory / "eval.jsonl").write_text(body)
            (directory / "fit.jsonl").write_text(body)
            self.assertEqual(dr.main([str(directory / "eval.jsonl"), "--fit-jsonl", str(directory / "fit.jsonl"), "--out", str(directory / "out" / "report.md")]), 0)
            self.assertNotIn("Warning", (directory / "out" / "report.md").read_text())
            (directory / "empty.jsonl").write_text("")
            with self.assertRaises(ValueError):
                dr.load(directory / "empty.jsonl")


if __name__ == "__main__":
    unittest.main()
