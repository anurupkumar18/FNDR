import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("onboarding_funnel.py")


class OnboardingFunnelReportTest(unittest.TestCase):
    def test_activation_window_uses_monotonic_time_when_wall_clock_changes(self):
        with tempfile.TemporaryDirectory() as scratch:
            path = Path(scratch) / "run.jsonl"
            rows = [
                {"event": "step_started", "step": "welcome", "at_ms": 5000, "monotonic_ms": 1000},
                {"event": "completed", "step": "complete", "at_ms": 5000, "monotonic_ms": 2000},
                {"event": "search_submitted", "at_ms": 4000, "monotonic_ms": 3000},
                {"event": "result_selected", "at_ms": 3000, "monotonic_ms": 4000},
                {"event": "reopen_succeeded", "at_ms": 2000, "monotonic_ms": 5000},
            ]
            path.write_text("".join(json.dumps(row) + "\n" for row in rows))
            result = subprocess.run(
                [sys.executable, str(SCRIPT), "--events", str(path)],
                text=True, capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("Activation proxy: 1", result.stdout)

    def test_three_runs_keep_the_full_denominator_and_do_not_claim_observed_success(self):
        with tempfile.TemporaryDirectory() as scratch:
            files = []
            runs = [
                [
                    {"event": "step_started", "step": "welcome", "at_ms": 1000},
                    {"event": "step_finished", "step": "welcome", "at_ms": 2000, "duration_ms": 1000},
                    {"event": "step_started", "step": "model_download", "at_ms": 2000},
                    {"event": "step_skipped", "step": "model_download", "at_ms": 4000, "duration_ms": 2000},
                    {"event": "completed", "step": "complete", "at_ms": 5000},
                    {"event": "search_submitted", "at_ms": 6000},
                    {"event": "result_selected", "at_ms": 7000},
                    {"event": "reopen_succeeded", "at_ms": 8000},
                ],
                [
                    {"event": "step_started", "step": "welcome", "at_ms": 1000},
                    {"event": "error", "step": "welcome", "at_ms": 2000},
                ],
                [
                    {"event": "step_started", "step": "welcome", "at_ms": 1000},
                    {"event": "completed", "step": "complete", "at_ms": 5000},
                    {"event": "search_submitted", "at_ms": 6000},
                    {"event": "reopen_failed", "at_ms": 7000},
                ],
            ]
            for index, rows in enumerate(runs, 1):
                path = Path(scratch) / f"run-{index}.jsonl"
                path.write_text("".join(json.dumps(row) + "\n" for row in rows))
                files.append(str(path))
            result = subprocess.run(
                [sys.executable, str(SCRIPT), *(item for path in files for item in ("--events", path))],
                text=True, capture_output=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("Trials: 3", result.stdout)
            self.assertIn("Completed onboarding: 2", result.stdout)
            self.assertIn("Incomplete onboarding: 1", result.stdout)
            self.assertIn("Activation proxy: 1", result.stdout)
            self.assertIn("Observed activation: not measured", result.stdout)
            self.assertIn("model_download: 1", result.stdout)
            self.assertIn("Errors: 1", result.stdout)
            self.assertIn("welcome: started 3, finished 1, skipped 0, errors 1", result.stdout)


if __name__ == "__main__":
    unittest.main()
