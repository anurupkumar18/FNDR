import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import rank_decisions as rd


class RankTests(unittest.TestCase):
    def test_weighted_total_and_order(self):
        decisions = {
            "Enrichment gate": dict(volume=5, cost=5, labels=3, bounded=5, stakes=3),
            "Merge vs append vs new": dict(volume=5, cost=3, labels=4, bounded=5, stakes=4),
            "Tool-call safety": dict(volume=1, cost=2, labels=2, bounded=4, stakes=5),
        }
        ranked = rd.rank(decisions)
        self.assertEqual(
            [(total, name) for total, name, _ in ranked],
            [(34, "Enrichment gate"), (33, "Merge vs append vs new"), (19, "Tool-call safety")],
        )

    def test_ties_are_ordered_by_name_and_scores_are_validated(self):
        ranked = rd.rank(
            {
                "B": dict(volume=1, cost=1, labels=1, bounded=1, stakes=1),
                "A": dict(volume=1, cost=1, labels=1, bounded=1, stakes=1),
            }
        )
        self.assertEqual([name for _, name, _ in ranked], ["A", "B"])
        with self.assertRaises(ValueError):
            rd.rank({"X": dict(volume=6, cost=1, labels=1, bounded=1, stakes=1)})
        with self.assertRaises(ValueError):
            rd.rank({"X": dict(volume=1, cost=1, labels=1, bounded=1)})


if __name__ == "__main__":
    unittest.main()
