import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import score as s

REQUIRED = s.DEFAULT_REQUIRED
FIELDS = s.DEFAULT_TEXT_FIELDS


class MetricTests(unittest.TestCase):
    def test_recall_and_mrr(self):
        self.assertEqual(s.recall_at_k(["a", "b", "c"], ["c", "z"], 3), 0.5)
        self.assertEqual(s.recall_at_k(["a", "b", "c"], ["c"], 2), 0.0)
        self.assertEqual(s.mrr_at_k(["a", "b", "c"], ["b"], 10), 0.5)
        self.assertEqual(s.mrr_at_k(["a"], ["z"], 10), 0.0)
        self.assertEqual(s.recall_at_k([], [], 5), 0.0)

    def test_wilson_interval_is_wide_for_small_n(self):
        lo, hi = s.wilson_interval(45, 50)
        self.assertTrue(0.78 < lo < 0.80, lo)
        self.assertTrue(0.95 < hi < 0.97, hi)
        self.assertEqual(s.wilson_interval(0, 0), (0.0, 0.0))
        lo2, hi2 = s.wilson_interval(450, 500)
        self.assertLess(hi2 - lo2, hi - lo)

    def test_supported_substring_and_overlap(self):
        ev = "Running cargo test in the FNDR repo, 12 tests passed"
        self.assertTrue(s.is_supported("cargo test", ev))
        self.assertTrue(s.is_supported("FNDR repo tests passed", ev))
        self.assertFalse(s.is_supported("deploying kubernetes cluster", ev))
        self.assertTrue(s.is_supported("", ev))


class ExtractionTests(unittest.TestCase):
    def setUp(self):
        self.cases = {
            "c1": {"evidence": "cargo test passed in fndr repo", "expected": {"activity_type": "coding"}},
            "c2": {"evidence": "reading the wikipedia article on llamas", "expected": {"activity_type": "reading"}},
            "c3": {"evidence": "slack thread about lunch", "expected": {"activity_type": "communication"}},
        }
        good = {"activity_type": "coding", "topic": "cargo test", "memory_context": "tests passed in fndr repo", "user_intent": "", "entities": ["fndr"]}
        halluc = {"activity_type": "reading", "topic": "kubernetes deploy", "memory_context": "rolled out cluster", "user_intent": "", "entities": []}
        self.preds = {
            "c1": json.dumps(good),
            "c2": json.dumps(halluc),
            "c3": "not json at all",
        }

    def test_scores(self):
        r = s.score_extraction(self.cases, self.preds, REQUIRED, FIELDS)
        self.assertEqual(r["cases"], 3)
        self.assertEqual(r["format_validity"][:2], (2, 3))
        hits, total, _ = r["grounding_rate"]
        self.assertEqual(total, 5)
        self.assertEqual(hits, 3)
        self.assertEqual(r["activity_accuracy"][:2], (2, 3))

    def test_missing_prediction_counts_as_invalid(self):
        r = s.score_extraction(self.cases, {}, REQUIRED, FIELDS)
        self.assertEqual(r["format_validity"][:2], (0, 3))

    def test_missing_required_key_is_invalid(self):
        self.assertIsNone(s.parse_prediction('{"topic": "x"}', REQUIRED))
        self.assertIsNone(s.parse_prediction("[1,2]", REQUIRED))
        self.assertIsNone(s.parse_prediction(None, REQUIRED))


class EndToEndTests(unittest.TestCase):
    def test_cli_writes_report_with_intervals(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "gold.jsonl").write_text(json.dumps({"id": "c1", "evidence": "cargo test passed", "expected": {"activity_type": "coding"}}) + "\n")
            (d / "pred.jsonl").write_text(json.dumps({"id": "c1", "output": json.dumps({"activity_type": "coding", "topic": "cargo test", "memory_context": "passed", "user_intent": "", "entities": []})}) + "\n")
            (d / "q.jsonl").write_text(json.dumps({"id": "q1", "relevant": ["m1"]}) + "\n")
            (d / "r.jsonl").write_text(json.dumps({"id": "q1", "ranked": ["m0", "m1"]}) + "\n")
            out = d / "out" / "report.md"
            json_out = d / "out" / "metrics.json"
            rc = s.main(["--gold", str(d / "gold.jsonl"), "--predictions", str(d / "pred.jsonl"),
                         "--questions", str(d / "q.jsonl"), "--rankings", str(d / "r.jsonl"),
                         "--model-label", "test-model", "--out", str(out), "--json-out", str(json_out)])
            self.assertEqual(rc, 0)
            metrics = json.loads(json_out.read_text())
            self.assertEqual(metrics["model_label"], "test-model")
            self.assertEqual(metrics["format_validity"], 1.0)
            self.assertEqual(metrics["mrr_at_10"], 0.5)
            text = out.read_text()
            self.assertIn("Model: test-model", text)
            self.assertIn("Format validity: 100.0 percent (1/1", text)
            self.assertIn("MRR@10: 0.500", text)
            self.assertIn("95 percent CI", text)


class GateIntegrationTests(unittest.TestCase):
    def test_score_output_feeds_the_promotion_gate(self):
        sys.path.insert(0, str(Path(__file__).parent))
        import promotion_gate as pg  # noqa: E402  (ships next to this file in scripts/train/ or the assets dir)
        base = {"format_validity": 0.90, "grounding_rate": 0.80, "recall_at_5": 0.70}
        cand = {"format_validity": 0.99, "grounding_rate": 0.88, "recall_at_5": 0.70}
        ok, _ = pg.decide(base, cand, "grounding_rate", 0.05, ["format_validity", "recall_at_5"], 0.01)
        self.assertTrue(ok)


if __name__ == "__main__":
    unittest.main()
