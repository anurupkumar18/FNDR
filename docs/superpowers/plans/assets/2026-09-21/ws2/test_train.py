import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import build_pairs as bp
import promotion_gate as pg


class PairTests(unittest.TestCase):
    def test_edit_becomes_chosen_and_original_rejected(self):
        ev = [{"prompt": "P", "output": "bad", "signal": "edit", "edited_output": "good"}]
        self.assertEqual(bp.build_pairs(ev), [{"prompt": "P", "chosen": "good", "rejected": "bad"}])

    def test_up_and_down_on_same_prompt_pair_up(self):
        ev = [
            {"prompt": "P", "output": "meh", "signal": "thumbs_down"},
            {"prompt": "P", "output": "nice", "signal": "thumbs_up"},
            {"prompt": "Q", "output": "only up", "signal": "thumbs_up"},
        ]
        self.assertEqual(bp.build_pairs(ev), [{"prompt": "P", "chosen": "nice", "rejected": "meh"}])

    def test_identical_empty_long_and_duplicate_pairs_are_dropped(self):
        ev = [
            {"prompt": "P", "output": "same", "signal": "edit", "edited_output": "same"},
            {"prompt": "P", "output": "x", "signal": "edit", "edited_output": ""},
            {"prompt": "P", "output": "x" * 10, "signal": "edit", "edited_output": "y" * 7000},
            {"prompt": "R", "output": "a", "signal": "edit", "edited_output": "b"},
            {"prompt": "R", "output": "c", "signal": "edit", "edited_output": "b"},
        ]
        self.assertEqual(bp.build_pairs(ev, max_chars=6000), [{"prompt": "R", "chosen": "b", "rejected": "a"}])

    def test_cli_writes_jsonl(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "fb.jsonl").write_text(json.dumps({"prompt": "P", "output": "bad", "signal": "edit", "edited_output": "good"}) + "\n")
            self.assertEqual(bp.main([str(d / "fb.jsonl"), "--out", str(d / "o" / "pairs.jsonl")]), 0)
            lines = (d / "o" / "pairs.jsonl").read_text().splitlines()
            self.assertEqual(json.loads(lines[0])["chosen"], "good")


class GateTests(unittest.TestCase):
    base = {"grounding_rate": 0.80, "format_validity": 0.98, "recall_at_5": 0.70}

    def test_promotes_on_gain_without_regression(self):
        cand = {"grounding_rate": 0.87, "format_validity": 0.985, "recall_at_5": 0.70}
        ok, reasons = pg.decide(self.base, cand, "grounding_rate", 0.05, ["format_validity", "recall_at_5"], 0.01)
        self.assertTrue(ok, reasons)

    def test_rejects_small_gain(self):
        cand = {"grounding_rate": 0.82, "format_validity": 0.98, "recall_at_5": 0.70}
        ok, _ = pg.decide(self.base, cand, "grounding_rate", 0.05, ["format_validity"], 0.01)
        self.assertFalse(ok)

    def test_rejects_guard_regression_even_with_big_gain(self):
        cand = {"grounding_rate": 0.95, "format_validity": 0.90, "recall_at_5": 0.70}
        ok, reasons = pg.decide(self.base, cand, "grounding_rate", 0.05, ["format_validity"], 0.01)
        self.assertFalse(ok)
        self.assertTrue(any("guard format_validity regressed" in r for r in reasons))

    def test_missing_metric_is_a_rejection_not_a_crash(self):
        ok, reasons = pg.decide(self.base, {"grounding_rate": 0.9}, "grounding_rate", 0.05, ["format_validity"], 0.01)
        self.assertFalse(ok)
        self.assertIn("missing metric", reasons[0])

    def test_cli_exit_code_follows_decision(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "b.json").write_text(json.dumps(self.base))
            (d / "c.json").write_text(json.dumps({"grounding_rate": 0.9, "format_validity": 0.98, "recall_at_5": 0.7}))
            self.assertEqual(pg.main(["--base", str(d / "b.json"), "--candidate", str(d / "c.json")]), 0)
            (d / "c2.json").write_text(json.dumps({"grounding_rate": 0.81, "format_validity": 0.98, "recall_at_5": 0.7}))
            self.assertEqual(pg.main(["--base", str(d / "b.json"), "--candidate", str(d / "c2.json")]), 1)


if __name__ == "__main__":
    unittest.main()
