import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import score_merge as sm


def fr(fid, story, mem):
    return {"frame_id": fid, "story_id": story, "memory_id": mem}


class ScoreTests(unittest.TestCase):
    def test_perfect_merge(self):
        r = sm.score([fr("a", "S1", "m1"), fr("b", "S1", "m1"), fr("c", "S2", "m2")])
        self.assertEqual((r["precision"], r["recall"], r["false_merges"], r["fragmentation"]), (1.0, 1.0, 0, 1.0))

    def test_duplicates_lower_recall_and_raise_fragmentation(self):
        r = sm.score([fr("a", "S1", "m1"), fr("b", "S1", "m2"), fr("c", "S1", "m2")])
        self.assertEqual(r["recall"], 1 / 3)
        self.assertEqual(r["precision"], 1.0)
        self.assertEqual(r["fragmentation"], 2.0)

    def test_false_merge_is_counted_and_lowers_precision(self):
        r = sm.score([fr("a", "S1", "m1"), fr("b", "S2", "m1")])
        self.assertEqual(r["false_merges"], 1)
        self.assertEqual(r["precision"], 0.0)
        self.assertEqual(r["recall"], 1.0)

    def test_all_singletons_have_vacuous_precision_and_recall(self):
        r = sm.score([fr("a", "S1", "m1"), fr("b", "S2", "m2")])
        self.assertEqual((r["precision"], r["recall"]), (1.0, 1.0))

    def test_f1_and_render(self):
        self.assertEqual(sm.f1(1.0, 1.0), 1.0)
        self.assertEqual(sm.f1(0.0, 0.0), 0.0)
        text = sm.render("t", sm.score([fr("a", "S1", "m1"), fr("b", "S1", "m1")]))
        self.assertIn("Pairwise precision 1.000, recall 1.000, F1 1.000", text)

    def test_empty_input_is_an_error_and_cli_roundtrips(self):
        with self.assertRaises(ValueError):
            sm.score([])
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "f.jsonl").write_text("\n".join(json.dumps(x) for x in [fr("a", "S1", "m1"), fr("b", "S1", "m1")]) + "\n")
            self.assertEqual(sm.main([str(d / "f.jsonl"), "--name", "x", "--out", str(d / "o" / "r.md")]), 0)
            self.assertIn("Merge and dedup score: x", (d / "o" / "r.md").read_text())


if __name__ == "__main__":
    unittest.main()
