"""Unit tests for the pure parts of embedding_bakeoff.py (VS-17, VS-19).

Runs without torch or model downloads:
  python3 -m unittest scripts/audit/test_embedding_bakeoff.py
"""

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import embedding_bakeoff as eb  # noqa: E402


def entry(**overrides):
    base = {
        "id": "m1",
        "app_name": "Google Chrome",
        "window_title": "Smart Reminders PRD - Google Docs",
        "url": "https://docs.google.com/document/d/abc/edit",
        "summary": "Drafted the Smart Reminders PRD with three email reminders around the due date.",
        "ocr_text": "Smart Reminders PRD\nStatus: Draft v2\nGoal\nCut median days-to-pay",
        "session": "launch-prd-draft",
        "project": "Smart Reminders launch",
        "topic": "Drafting the Smart Reminders PRD",
        "outcome": "PRD draft v2 is ready.",
        "next_steps": ["Get eng comments", "Decide default on or opt-in"],
        "decisions": ["Keep SMS reminders out of v1"],
        "errors": [],
    }
    base.update(overrides)
    return base


class PrimaryTextTests(unittest.TestCase):
    def test_follows_compose_insight_embedding_text_field_order(self):
        text = eb.app_primary_text(
            entry(decisions=["Keep SMS reminders out of v1", "Ship email reminders first"])
        )
        labels = [
            "project:",
            "topic:",
            "context:",
            "what_happened:",
            "why_mattered:",
            "what_changed:",
            "decisions:",
            "todos:",
            "urls:",
        ]
        positions = [text.index(label) for label in labels]
        self.assertEqual(positions, sorted(positions), text)

    def test_maps_seed_fields_like_seed_demo_and_derive_insight(self):
        text = eb.app_primary_text(entry())
        self.assertIn("project: Smart Reminders launch", text)
        self.assertIn("context: Drafted the Smart Reminders PRD", text)
        self.assertIn("what_happened: Drafted the Smart Reminders PRD", text)
        # why_mattered prefers the first decision.
        self.assertIn("why_mattered: Keep SMS reminders out of v1", text)
        # what_changed holds outcomes, never next steps or the decision already shown.
        self.assertNotIn("what_changed:", text)
        # A session id is not a thread and is not embedded.
        self.assertNotIn("context_thread:", text)
        self.assertIn("todos: Get eng comments; Decide default on or opt-in", text)

        two = eb.app_primary_text(
            entry(decisions=["Keep SMS reminders out of v1", "Ship email reminders first"])
        )
        self.assertIn("what_changed: Ship email reminders first", two)
        self.assertIn("urls: https://docs.google.com/document/d/abc/edit", text)

    def test_never_includes_ocr_or_window_title(self):
        text = eb.app_primary_text(entry(ocr_text="RAW_OCR_ONLY_TOKEN"))
        self.assertNotIn("RAW_OCR_ONLY_TOKEN", text)
        self.assertNotIn("Google Docs", text)

    def test_why_mattered_falls_back_to_error_then_first_summary_sentence(self):
        with_error = eb.app_primary_text(
            entry(decisions=[], errors=["ModuleNotFoundError: No module named 'pandas'"])
        )
        self.assertIn(
            "why_mattered: Encountered error: ModuleNotFoundError: No module named 'pandas'",
            with_error,
        )
        # The first summary sentence only repeats what_happened here, so it is not used.
        from_summary = eb.app_primary_text(
            entry(decisions=[], errors=[], summary="Read the rubric for the history essay. Then left.")
        )
        self.assertNotIn("why_mattered:", from_summary)
        short = eb.app_primary_text(entry(decisions=[], errors=[], summary="Q3 metrics. Long tail."))
        self.assertNotIn("why_mattered:", short)

    def test_whitespace_is_normalized_to_single_spaces(self):
        text = eb.app_primary_text(entry(summary="Line one.\n\nLine   two."))
        self.assertNotIn("\n", text)
        self.assertNotIn("  ", text)

    def test_empty_optional_fields_are_skipped(self):
        text = eb.app_primary_text(
            entry(project="", topic="", next_steps=[], decisions=[], errors=[], url=None)
        )
        for label in ("project:", "topic:", "what_changed:", "decisions:", "todos:", "urls:"):
            self.assertNotIn(label, text)

    def test_long_what_happened_is_clipped_like_clip_chars(self):
        long_summary = "word " * 100
        text = eb.app_primary_text(entry(summary=long_summary.strip()))
        what = text.split("what_happened: ", 1)[1].split(" why_mattered:", 1)[0]
        self.assertTrue(what.endswith("…"), what)
        self.assertLessEqual(len(what), 280)


class ChunkTests(unittest.TestCase):
    def test_chunk_source_is_title_then_ocr(self):
        self.assertEqual(
            eb.chunk_source_text(entry(window_title="Title", ocr_text="line a\nline b")),
            "Title\nline a\nline b",
        )

    def test_short_text_is_one_normalized_chunk(self):
        self.assertEqual(eb.chunk_text("a  b\nc", max_tokens=300), ["a b c"])

    def test_empty_text_has_no_chunks(self):
        self.assertEqual(eb.chunk_text("   \n "), [])

    def test_long_text_windows_respect_budget_and_overlap(self):
        words = [f"w{i:02d}" for i in range(40)]  # 3 chars per word plus a space
        text = " ".join(words)
        chunks = eb.chunk_text(text, max_tokens=10, overlap_tokens=3, chars_per_token=4)
        self.assertGreater(len(chunks), 1)
        for chunk in chunks:
            self.assertLessEqual(eb.estimate_tokens(chunk, 4), 10, chunk)
        # Every word is covered, in order, and consecutive windows share a tail.
        covered = []
        for chunk in chunks:
            covered.extend(word for word in chunk.split() if word not in covered)
        self.assertEqual(covered, words)
        for left, right in zip(chunks, chunks[1:]):
            self.assertIn(left.split()[-1], right.split())
        # 3 chars per word: 10 words fill a 10-token window and 3 words overlap.
        self.assertEqual(chunks[0].split(), words[0:10])
        self.assertEqual(chunks[1].split(), words[7:17])

    def test_chunking_always_makes_progress_with_a_huge_word(self):
        chunks = eb.chunk_text("x" * 100 + " tail", max_tokens=5, overlap_tokens=4)
        self.assertEqual(chunks, ["x" * 100, "tail"])

    def test_chunking_is_deterministic(self):
        text = " ".join(f"token{i}" for i in range(500))
        self.assertEqual(eb.chunk_text(text), eb.chunk_text(text))


class RollupTests(unittest.TestCase):
    def test_memory_score_is_its_best_chunk(self):
        scores = eb.rollup_chunk_scores([("a", 0.5), ("a", 0.7), ("b", 0.6)])
        self.assertAlmostEqual(scores["b"], 0.6)
        # a: best 0.7; the 0.5 chunk is more than 0.05 below the best, so no bonus.
        self.assertAlmostEqual(scores["a"], 0.7)

    def test_several_close_chunks_add_a_small_capped_bonus(self):
        hits = [("a", 0.70), ("a", 0.69), ("a", 0.68), ("a", 0.67), ("a", 0.66), ("a", 0.10)]
        scores = eb.rollup_chunk_scores(hits)
        bonus = eb.MULTI_CHUNK_BONUS * eb.MULTI_CHUNK_BONUS_CAP
        self.assertAlmostEqual(scores["a"], 0.70 + bonus)

    def test_one_extra_close_chunk_adds_one_bonus_step(self):
        scores = eb.rollup_chunk_scores([("a", 0.70), ("a", 0.66)])
        self.assertAlmostEqual(scores["a"], 0.70 + eb.MULTI_CHUNK_BONUS)


class RankingTests(unittest.TestCase):
    def test_rank_sorts_by_score_then_id(self):
        ranked = eb.rank_ids({"b": 0.5, "a": 0.5, "c": 0.9, "d": 0.1}, k=3)
        self.assertEqual([memory_id for memory_id, _ in ranked], ["c", "a", "b"])

    def test_first_relevant_rank_is_one_based_within_k(self):
        ranked = ["x", "y", "b", "z"]
        self.assertEqual(eb.first_relevant_rank(ranked, {"b"}, 10), 3)
        self.assertIsNone(eb.first_relevant_rank(ranked, {"b"}, 2))
        self.assertIsNone(eb.first_relevant_rank(ranked, set(), 10))


class MetricTests(unittest.TestCase):
    """Mirrors the PathScore tests in src-tauri/examples/retrieval_qa.rs."""

    def test_overall_and_per_kind_metrics(self):
        metrics = eb.score_cases(
            [
                ("keyword", 1, 0.9),
                ("keyword", None, 0.4),
                ("paraphrase", 4, 0.7),
                ("paraphrase", 7, 0.6),
            ]
        )
        self.assertAlmostEqual(metrics["recall_at_5"], 0.5)
        self.assertAlmostEqual(metrics["mrr_at_10"], (1.0 + 0.25 + 1.0 / 7.0) / 4.0)
        self.assertAlmostEqual(metrics["recall_at_5_by_kind"]["keyword"], 0.5)
        self.assertAlmostEqual(metrics["recall_at_5_by_kind"]["paraphrase"], 0.5)
        self.assertEqual(metrics["core_cases"], 4)
        self.assertEqual(metrics["core_hits_at_5"], 2)
        self.assertIsNone(metrics["no_match"])

    def test_time_and_app_get_their_own_recall_but_not_the_headline(self):
        metrics = eb.score_cases([("keyword", 1, 0.9), ("time", None, 0.5), ("app", 2, 0.8)])
        self.assertAlmostEqual(metrics["recall_at_5"], 1.0)
        self.assertAlmostEqual(metrics["mrr_at_10"], 1.0)
        self.assertAlmostEqual(metrics["recall_at_5_by_kind"]["time"], 0.0)
        self.assertAlmostEqual(metrics["recall_at_5_by_kind"]["app"], 1.0)
        self.assertNotIn("paraphrase", metrics["recall_at_5_by_kind"])

    def test_rank_beyond_ten_counts_as_a_miss_for_mrr(self):
        metrics = eb.score_cases([("keyword", 11, 0.9)])
        self.assertEqual(metrics["mrr_at_10"], 0.0)
        self.assertEqual(metrics["recall_at_5"], 0.0)

    def test_negative_cases_are_no_match_rows_never_misses(self):
        metrics = eb.score_cases(
            [
                ("keyword", 1, 0.9),
                ("paraphrase", 2, 0.7),
                ("negative", None, 0.3),
                ("negative", None, 0.5),
                ("negative", None, None),
            ]
        )
        self.assertAlmostEqual(metrics["recall_at_5"], 1.0)
        self.assertNotIn("negative", metrics["recall_at_5_by_kind"])
        no_match = metrics["no_match"]
        self.assertEqual(no_match["cases"], 3)
        self.assertEqual(no_match["returned_nothing"], 1)
        self.assertAlmostEqual(no_match["top_score_median"], 0.4)
        self.assertAlmostEqual(no_match["positive_top_score_median"], 0.8)
        self.assertAlmostEqual(no_match["separation"], 0.4)
        # Both positive top scores beat both returned negative top scores.
        self.assertAlmostEqual(no_match["separation_auc"], 1.0)

    def test_separation_auc_is_scale_free_pairwise_ordering(self):
        self.assertAlmostEqual(eb.separation_auc([0.9, 0.8], [0.1, 0.85]), 0.75)
        self.assertAlmostEqual(eb.separation_auc([0.5], [0.5]), 0.5)
        self.assertAlmostEqual(
            eb.separation_auc([9.0, 8.0], [1.0, 8.5]), eb.separation_auc([0.9, 0.8], [0.1, 0.85])
        )
        self.assertIsNone(eb.separation_auc([], [0.1]))
        self.assertIsNone(eb.separation_auc([0.1], []))

    def test_paired_comparison_counts_hits_and_reciprocal_ranks_on_core_cases_only(self):
        def q(kind, rank):
            return {"kind": kind, "rank_at_10": rank}

        mine = [q("keyword", 1), q("keyword", 2), q("paraphrase", 7), q("paraphrase", 3), q("time", 1)]
        base = [q("keyword", 1), q("keyword", 1), q("paraphrase", None), q("paraphrase", 9), q("time", None)]
        comparison = eb.compare_to_baseline(mine, base)
        # Hit@5: paraphrase rank 3 vs 9 is a gain; nothing is lost; time is ignored.
        self.assertEqual((comparison["gains"], comparison["losses"]), (1, 0))
        # Reciprocal rank: better on 7 vs miss and 3 vs 9, worse on 2 vs 1.
        self.assertEqual((comparison["rr_better"], comparison["rr_worse"]), (2, 1))
        self.assertAlmostEqual(comparison["mcnemar_p"], 1.0)
        self.assertAlmostEqual(comparison["rr_sign_p"], 1.0)

    def test_case_level_recall_counts_a_case_once_even_with_many_relevant_ids(self):
        relevant = {"a", "b", "c"}
        rank = eb.first_relevant_rank(["x", "b", "a", "c"], relevant, 10)
        metrics = eb.score_cases([("keyword", rank, 0.9)])
        self.assertEqual(rank, 2)
        self.assertEqual(metrics["recall_at_5"], 1.0)
        self.assertEqual(metrics["core_hits_at_5"], 1)

    def test_median_handles_even_odd_and_empty(self):
        self.assertIsNone(eb.median([]))
        self.assertEqual(eb.median([3.0, 1.0, 2.0]), 2.0)
        self.assertEqual(eb.median([4.0, 1.0, 2.0, 3.0]), 2.5)

    def test_percentile_matches_rust_nearest_rank_rounding(self):
        self.assertEqual(eb.percentile([], 95.0), 0)
        self.assertEqual(eb.percentile([10, 20, 30, 40, 50], 50.0), 30)
        self.assertEqual(eb.percentile([10, 20, 30, 40, 50], 95.0), 50)
        # Index 2.5 rounds half away from zero in Rust (3), not to even (2).
        self.assertEqual(eb.percentile([1, 2, 3, 4, 5, 6], 50.0), 4)

    def test_mcnemar_exact_two_sided(self):
        self.assertEqual(eb.mcnemar_exact(0, 0), 1.0)
        self.assertAlmostEqual(eb.mcnemar_exact(3, 0), 0.25)
        self.assertAlmostEqual(eb.mcnemar_exact(0, 5), 0.0625)
        self.assertAlmostEqual(eb.mcnemar_exact(2, 2), 1.0)
        self.assertAlmostEqual(eb.mcnemar_exact(6, 1), 0.125)


class RerankTests(unittest.TestCase):
    def test_candidates_are_the_top_depth_with_their_best_chunk_text(self):
        case = {
            "ranked": [["a", 0.9], ["b", 0.8], ["c", 0.7]],
            "best_chunk": {"a": "text a", "b": "text b", "c": "text c"},
        }
        self.assertEqual(eb.rerank_candidates(case, 2), [["a", "text a"], ["b", "text b"]])

    def test_reranked_head_then_the_untouched_vector_tail(self):
        ranked = [["a", 0.9], ["b", 0.8], ["c", 0.7], ["d", 0.6]]
        reordered = eb.apply_rerank(ranked, {"a": 0.1, "b": 2.5, "c": 2.5})
        # b and c tie on the cross-encoder and fall back to id order; d keeps its vector place.
        self.assertEqual([memory_id for memory_id, _ in reordered], ["b", "c", "a", "d"])
        self.assertEqual(reordered[0][1], 2.5)
        self.assertEqual(reordered[-1], ["d", 0.6])

    def test_rule_rejects_an_mrr_gain_under_three_hundredths(self):
        verdict, reason = eb.rerank_verdict(0.75, 0.779, 40.0)
        self.assertEqual(verdict, "rejected")
        self.assertIn("+0.029", reason)

    def test_rule_needs_the_m1_when_quality_passes(self):
        verdict, reason = eb.rerank_verdict(0.75, 0.78, 40.0)
        self.assertEqual(verdict, "needs-M1")
        self.assertIn("under 150 ms", reason)
        verdict, reason = eb.rerank_verdict(0.70, 0.80, 400.0)
        self.assertEqual(verdict, "needs-M1")
        self.assertIn("over 150 ms", reason)

    def test_rule_never_keeps_on_cloud_latency(self):
        self.assertNotEqual(eb.rerank_verdict(0.1, 0.9, 1.0)[0], "kept")


class CorpusTests(unittest.TestCase):
    def test_low_signal_control_is_excluded_and_cases_validate(self):
        with tempfile.TemporaryDirectory() as tmp:
            demo = Path(tmp)
            (demo / "p-week.json").write_text(
                json.dumps([entry(id="keep"), entry(id="ctrl", low_signal=True)])
            )
            (demo / "p-queries.json").write_text(
                json.dumps(
                    [
                        {"query": "q1", "kind": "keyword", "relevant_ids": ["keep"]},
                        {"query": "q2", "kind": "negative", "relevant_ids": []},
                    ]
                )
            )
            memories, cases = eb.load_persona(demo, "p")
        self.assertEqual([memory["id"] for memory in memories], ["keep"])
        self.assertEqual(len(cases), 2)

    def test_a_case_pointing_at_a_hidden_memory_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            demo = Path(tmp)
            (demo / "p-week.json").write_text(
                json.dumps([entry(id="keep"), entry(id="ctrl", low_signal=True)])
            )
            (demo / "p-queries.json").write_text(
                json.dumps([{"query": "q1", "kind": "keyword", "relevant_ids": ["ctrl"]}])
            )
            with self.assertRaises(ValueError):
                eb.load_persona(demo, "p")

    def test_real_personas_load(self):
        for persona in eb.PERSONAS:
            memories, cases = eb.load_persona(eb.DEMO_DIR, persona)
            self.assertTrue(memories)
            self.assertFalse(any(memory.get("low_signal") for memory in memories))
            self.assertTrue(any(case["kind"] == "negative" for case in cases))


if __name__ == "__main__":
    unittest.main()
