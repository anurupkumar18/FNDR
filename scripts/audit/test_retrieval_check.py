import contextlib
import copy
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import retrieval_check as rc  # noqa: E402


def report(*, search_recall=1.0, ask_recall=1.0, ranks=None, case_set="knowledge-worker"):
    """A minimal schema-v1 report shaped like retrieval_qa's --json output."""
    ranks = ranks or {
        "pandas import error fix": ("keyword", 1, 1),
        "which plotting library am I allowed to use": ("paraphrase", 3, 1),
    }
    return {
        "schema_version": 1,
        "case_set": case_set,
        "case_count": len(ranks),
        "case_count_by_kind": {"keyword": 1, "paraphrase": 1},
        "paths": {
            "search": {
                "recall_at_5": search_recall,
                "mrr_at_10": 0.6,
                "latency_ms": {"p50": 500, "p95": 700},
                "recall_at_5_by_kind": {"keyword": 1.0, "paraphrase": search_recall},
            },
            "ask": {
                "recall_at_5": ask_recall,
                "mrr_at_10": 1.0,
                "latency_ms": {"p50": 1200, "p95": 1400},
                "recall_at_5_by_kind": {"keyword": 1.0, "paraphrase": ask_recall},
            },
        },
        "top1_agreement": {"count": 1, "total": 2, "rate": 0.5},
        "queries": [
            {
                "query": query,
                "kind": kind,
                "search_rank_at_10": search_rank,
                "ask_rank_at_10": ask_rank,
            }
            for query, (kind, search_rank, ask_rank) in ranks.items()
        ],
    }


class CompareTests(unittest.TestCase):
    def test_identical_reports_pass(self):
        result = rc.compare(report(), report())
        self.assertEqual(result.failures, [])

    def test_recall_drop_within_tolerance_passes(self):
        result = rc.compare(report(search_recall=0.955), report(search_recall=0.909))
        self.assertEqual(result.failures, [])

    def test_recall_drop_beyond_tolerance_fails_and_names_the_path(self):
        result = rc.compare(report(search_recall=0.955), report(search_recall=0.864))
        self.assertEqual(len(result.failures), 1)
        self.assertIn("search", result.failures[0])
        self.assertIn("Recall@5", result.failures[0])

    def test_drop_of_exactly_the_tolerance_passes_despite_float_error(self):
        result = rc.compare(report(ask_recall=1.0), report(ask_recall=0.95))
        self.assertEqual(result.failures, [])

    def test_previously_found_query_that_becomes_a_miss_fails_and_names_it(self):
        current = report(
            ranks={
                "pandas import error fix": ("keyword", 1, 1),
                "which plotting library am I allowed to use": ("paraphrase", None, 1),
            }
        )
        result = rc.compare(report(), current)
        self.assertEqual(len(result.failures), 1)
        self.assertIn("which plotting library am I allowed to use", result.failures[0])
        self.assertIn("search", result.failures[0])

    def test_rank_that_moves_within_top_ten_is_reported_but_passes(self):
        current = report(
            ranks={
                "pandas import error fix": ("keyword", 4, 1),
                "which plotting library am I allowed to use": ("paraphrase", 1, 1),
            }
        )
        result = rc.compare(report(), current)
        self.assertEqual(result.failures, [])
        changes = {(c.query, c.path): (c.before, c.after) for c in result.rank_changes}
        self.assertEqual(changes[("pandas import error fix", "search")], (1, 4))
        self.assertEqual(
            changes[("which plotting library am I allowed to use", "search")], (3, 1)
        )

    def test_query_that_was_already_a_miss_may_stay_a_miss(self):
        reference = report(
            ranks={
                "pandas import error fix": ("keyword", 1, 1),
                "which plotting library am I allowed to use": ("paraphrase", None, 1),
            }
        )
        result = rc.compare(reference, copy.deepcopy(reference))
        self.assertEqual(result.failures, [])

    def test_missing_path_fails(self):
        current = report()
        del current["paths"]["ask"]
        result = rc.compare(report(), current)
        self.assertTrue(any("ask" in failure for failure in result.failures))

    def test_new_path_is_measured_without_a_reference_and_passes(self):
        current = report()
        current["paths"]["retrieve"] = copy.deepcopy(current["paths"]["search"])
        for query in current["queries"]:
            query["retrieve_rank_at_10"] = 1
        result = rc.compare(report(), current)
        self.assertEqual(result.failures, [])
        self.assertIn("retrieve", result.new_paths)

    def test_dropped_query_fails_so_the_case_set_cannot_shrink_silently(self):
        current = report(ranks={"pandas import error fix": ("keyword", 1, 1)})
        result = rc.compare(report(), current)
        self.assertTrue(
            any("which plotting library am I allowed to use" in f for f in result.failures)
        )

    def test_new_query_is_reported_and_passes(self):
        ranks = {
            "pandas import error fix": ("keyword", 1, 1),
            "which plotting library am I allowed to use": ("paraphrase", 3, 1),
            "what was I reading in Slack yesterday": ("time", None, 2),
        }
        result = rc.compare(report(), report(ranks=ranks))
        self.assertEqual(result.failures, [])
        self.assertEqual(result.new_queries, ["what was I reading in Slack yesterday"])

    def test_different_case_sets_cannot_be_compared(self):
        with self.assertRaises(rc.ReportError):
            rc.compare(report(), report(case_set="office-pm"))

    def test_unknown_schema_version_is_rejected(self):
        bad = report()
        bad["schema_version"] = 99
        with self.assertRaises(rc.ReportError):
            rc.validate_report(bad)

    def test_duplicate_query_text_is_rejected(self):
        bad = report()
        bad["queries"].append(copy.deepcopy(bad["queries"][0]))
        with self.assertRaises(rc.ReportError):
            rc.validate_report(bad)


def v2_report(**kwargs):
    """A schema-v2 report: v1 plus open kind maps, top scores, and no_match."""
    value = report(**kwargs)
    value["schema_version"] = 2
    value["case_count_by_kind"]["negative"] = 1
    for path in value["paths"].values():
        path["no_match"] = {
            "cases": 1,
            "returned_nothing": 0,
            "no_strong_match": 1,
            "positive_without_strong_match": 0,
            "top_score_median": 0.31,
            "positive_top_score_median": 0.62,
        }
    for query in value["queries"]:
        query["search_top_score"] = 0.6
        query["ask_top_score"] = 0.7
    value["queries"].append(
        {
            "query": "rental car reservation",
            "kind": "negative",
            "search_rank_at_10": None,
            "ask_rank_at_10": None,
            "search_top_score": 0.31,
            "ask_top_score": 0.29,
        }
    )
    return value


class SchemaV2Tests(unittest.TestCase):
    def test_v2_report_is_valid(self):
        rc.validate_report(v2_report())

    def test_v1_reference_compares_with_v2_current(self):
        result = rc.compare(report(), v2_report())
        self.assertEqual(result.failures, [])
        self.assertEqual(result.new_queries, ["rental car reservation"])

    def test_negative_query_never_counts_as_lost(self):
        result = rc.compare(v2_report(), v2_report())
        self.assertEqual(result.failures, [])

    def test_render_shows_no_match_rows(self):
        text = rc.render(rc.compare(v2_report(), v2_report()))
        self.assertIn("| search | 1 | 0 | 1 | 0 | 0.310 | 0.620 |", text)

    def test_render_marks_no_match_counts_older_reports_lack(self):
        current = v2_report()
        for path in current["paths"].values():
            del path["no_match"]["no_strong_match"]
            del path["no_match"]["positive_without_strong_match"]
        text = rc.render(rc.compare(v2_report(), current))
        self.assertIn("| search | 1 | 0 | n/a | n/a | 0.310 | 0.620 |", text)


class RenderAndMainTests(unittest.TestCase):
    def write(self, directory, name, value):
        path = Path(directory) / name
        path.write_text(json.dumps(value))
        return path

    def run_main(self, reference, current):
        with tempfile.TemporaryDirectory() as directory:
            ref = self.write(directory, "ref.json", reference)
            cur = self.write(directory, "cur.json", current)
            out = io.StringIO()
            with contextlib.redirect_stdout(out):
                code = rc.main(["--reference", str(ref), "--current", str(cur)])
            return code, out.getvalue()

    def test_main_exits_zero_and_says_pass(self):
        code, text = self.run_main(report(), report())
        self.assertEqual(code, 0)
        self.assertIn("PASS", text)
        self.assertIn("| search |", text)
        self.assertIn("| ask |", text)

    def test_main_exits_one_and_names_the_regressed_query(self):
        current = report(
            search_recall=0.5,
            ranks={
                "pandas import error fix": ("keyword", None, 1),
                "which plotting library am I allowed to use": ("paraphrase", 3, 1),
            },
        )
        code, text = self.run_main(report(), current)
        self.assertEqual(code, 1)
        self.assertIn("FAIL", text)
        self.assertIn("pandas import error fix", text)

    def test_main_exits_two_on_unreadable_input(self):
        with tempfile.TemporaryDirectory() as directory:
            missing = Path(directory) / "missing.json"
            err = io.StringIO()
            with contextlib.redirect_stderr(err), contextlib.redirect_stdout(io.StringIO()):
                code = rc.main(["--reference", str(missing), "--current", str(missing)])
        self.assertEqual(code, 2)

    def test_render_reports_top1_agreement_without_gating_on_it(self):
        current = report()
        current["top1_agreement"] = {"count": 0, "total": 2, "rate": 0.0}
        code, text = self.run_main(report(), current)
        self.assertEqual(code, 0)
        self.assertIn("Top-1 agreement: 1/2 -> 0/2", text)

    def test_render_never_prints_absolute_paths(self):
        _, text = self.run_main(report(), report())
        self.assertNotIn("/tmp", text)
        self.assertNotIn(str(Path.home()), text)

    def test_committed_references_are_valid_reports(self):
        root = Path(__file__).resolve().parents[2]
        for case_set in ["knowledge-worker", "office-pm"]:
            reference = json.loads(
                (root / f"scripts/demo/retrieval-reference/{case_set}.json").read_text()
            )
            rc.validate_report(reference)
            self.assertEqual(reference["case_set"], case_set)
            # VS-09 added the shared retrieve path to Search and Ask.
            self.assertEqual(set(reference["paths"]), {"search", "ask", "retrieve"})


if __name__ == "__main__":
    unittest.main()
