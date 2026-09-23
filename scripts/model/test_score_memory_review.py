import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import score_memory_review as smr


def case(
    case_id="review-1",
    *,
    label_status="human_reviewed",
    outcome="reviewed_local",
    context="Reviewed cargo test output: 12 passed.",
    activity_type="testing_workflow",
    must_mention=None,
    must_not=None,
    decision="improves",
):
    return {
        "id": case_id,
        "split": "dev",
        "labeler": "human-a",
        "reviewed_by": "human-b",
        "label_status": label_status,
        "evidence": "cargo test reported 12 passed",
        "before": {"display_summary": "Terminal output"},
        "reviewed": {
            "activity_type": activity_type,
            "memory_context": context,
            "display_summary": context,
        },
        "outcome": outcome,
        "expected": {
            "activity_type": "testing_workflow",
            "must_mention": must_mention or ["12 passed"],
            "must_not": must_not or ["failed"],
            "human_decision": decision,
        },
    }


class ScoreMemoryReviewTests(unittest.TestCase):
    def test_human_reviewed_cases_report_quality_metrics(self):
        result = smr.score_cases([case()])

        self.assertTrue(result["quality_score_available"])
        self.assertEqual(result["case_count"], 1)
        self.assertEqual(result["outcome_counts"], {"reviewed_local": 1})
        self.assertEqual(result["quality"]["required_term_recall"], 1.0)
        self.assertEqual(result["quality"]["forbidden_term_violations"], 0)
        self.assertEqual(result["quality"]["activity_accuracy"], 1.0)
        self.assertEqual(result["quality"]["unsafe_review_rate"], 0.0)
        self.assertEqual(result["quality"]["improvement_rate"], 1.0)

    def test_draft_labels_only_report_structural_counts(self):
        result = smr.score_cases([case(label_status="draft")])

        self.assertFalse(result["quality_score_available"])
        self.assertNotIn("quality", result)
        self.assertEqual(result["case_count"], 1)
        self.assertEqual(result["outcome_counts"], {"reviewed_local": 1})

    def test_forbidden_or_missing_evidence_makes_a_review_unsafe(self):
        result = smr.score_cases(
            [
                case(
                    "hallucination",
                    context="Reviewed cargo test failed after 12 passed.",
                ),
                case("omission", context="Reviewed terminal output.", must_not=["failure"]),
            ]
        )

        self.assertEqual(result["quality"]["forbidden_term_violations"], 1)
        self.assertEqual(result["quality"]["required_term_recall"], 0.5)
        self.assertEqual(result["quality"]["unsafe_review_rate"], 1.0)
        self.assertEqual(result["quality"]["improvement_rate"], 0.0)

    def test_validator_rejection_is_counted_as_unsafe(self):
        result = smr.score_cases([case(outcome="review_failed")])

        self.assertEqual(result["outcome_counts"], {"review_failed": 1})
        self.assertEqual(result["quality"]["unsafe_review_rate"], 1.0)

    def test_parser_names_the_case_with_invalid_label_status(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "review.jsonl"
            row = case(label_status="unverified")
            path.write_text(json.dumps(row) + "\n")

            with self.assertRaisesRegex(ValueError, "review-1.*label_status"):
                smr.load_cases(path)


if __name__ == "__main__":
    unittest.main()
