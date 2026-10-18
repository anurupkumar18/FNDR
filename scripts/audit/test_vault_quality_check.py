import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import vault_quality_check as vq  # noqa: E402


def scorecard(**overrides):
    """A scorecard shaped like vault_qa's JSON, with healthy numbers."""
    report = {
        "summary_quality": {"narrated": "5 (4%)", "placeholder": "10 (8%)", "cut_inside_token": "0 (0%)"},
        "vector_health": {"zero_primary_vector": "0 (0%)", "embedding_text_carries_session_id": "0 (0%)"},
        "unrelated_queries": [{"query": "q", "marked_strong": False}],
        "labels": {"activity_type": {"coding": 3, "unknown": 2}},
        "voice": {
            "shown_after_cleanup": {
                "past-tense verb": 6,
                "states what the screen held (the, a, in)": 2,
                "other": 1,
                "narrator (the user, you)": 0,
                "placeholder or empty": 1,
            }
        },
        "known_item_search": {"by_query": {"window_title": {"a_memory_with_the_same_title_in_top5": "9/10"}}},
    }
    report.update(overrides)
    return report


THRESHOLDS = {
    "max_percent": {"summary_quality.narrated": 10},
    "max_count": {"unrelated_queries_marked_strong": 0, "activity_labels_outside_the_list": 0},
    "min_fraction": {
        "known_item_search.by_query.window_title.a_memory_with_the_same_title_in_top5": 0.8,
        "voice_in_voice": 0.55,
    },
    "max_fraction": {"voice_out_of_voice": 0.08},
}


def failures(report, thresholds=THRESHOLDS):
    return [line for passed, line in vq.check(report, thresholds) if passed is False]


class VaultQualityCheckTest(unittest.TestCase):
    def test_a_healthy_scorecard_passes_every_check(self):
        results = vq.check(scorecard(), THRESHOLDS)
        self.assertEqual(len(results), 6)
        self.assertTrue(all(passed for passed, _ in results))

    def test_too_many_narrated_summaries_fail(self):
        report = scorecard(summary_quality={"narrated": "30 (19%)"})
        self.assertEqual(len(failures(report)), 1)
        self.assertIn("summary_quality.narrated: 19%", failures(report)[0])

    def test_one_unrelated_query_marked_strong_fails(self):
        report = scorecard(unrelated_queries=[{"query": "q", "marked_strong": True}])
        self.assertIn("unrelated_queries_marked_strong: 1", failures(report)[0])

    def test_a_label_outside_the_prompt_list_fails(self):
        report = scorecard(labels={"activity_type": {"coding": 3, "screen_review": 2}})
        self.assertIn("activity_labels_outside_the_list: 2", failures(report)[0])

    def test_title_search_under_the_floor_fails(self):
        report = scorecard(
            known_item_search={"by_query": {"window_title": {"a_memory_with_the_same_title_in_top5": "7/11"}}}
        )
        self.assertIn("7/11", failures(report)[0])

    def test_narrator_openings_count_as_out_of_voice(self):
        report = scorecard()
        report["voice"]["shown_after_cleanup"]["narrator (the user, you)"] = 3
        self.assertTrue(any("voice_out_of_voice" in line for line in failures(report)))

    def test_what_the_scorecard_did_not_measure_is_reported_and_does_not_fail(self):
        report = scorecard(known_item_search={"by_query": {"window_title": {"a_memory_with_the_same_title_in_top5": "0/0"}}})
        results = vq.check(report, THRESHOLDS)
        self.assertEqual([line for passed, line in results if passed is None][0][:12], "not measured")
        self.assertEqual(failures(report), [])


if __name__ == "__main__":
    unittest.main()
