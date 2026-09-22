import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import validate_capture_baseline as validator


def row(timestamp, aggregates, counters):
    return {
        "snapshot": {"generated_at_ms": timestamp, "aggregates": aggregates, "counters": counters},
        "skips": {},
        "totals": {"evaluated": 1, "stored": 1, "skipped": 0},
    }


class ValidateCaptureBaselineTests(unittest.TestCase):
    def test_complete_dogfood_run_is_ready_for_report_generation(self):
        rows = [
            row(0, {}, {}),
            row(
                600000,
                {"capture.context_ms": {"n": 12}, "mem.embed_ms": {"n": 3}},
                {"mem.outcome.new": 3},
            ),
        ]
        self.assertEqual(validator.validate(rows, min_samples=2), [])

    def test_missing_duration_stage_or_outcome_is_actionable(self):
        rows = [row(0, {}, {}), row(60000, {"capture.context_ms": {"n": 1}}, {})]
        errors = validator.validate(rows, min_samples=3)
        self.assertIn("need at least 3 metrics samples, found 2", errors)
        self.assertIn("no mem.* post-capture timing was observed", errors)
        self.assertIn("no mem.outcome.* counter was observed", errors)


if __name__ == "__main__":
    unittest.main()
