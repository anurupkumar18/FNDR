import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import phase_progress as pp
import plan_to_csv as p
from test_plan_to_csv import HEADER, row


def manifest():
    text = HEADER + "".join([
        row("CAP-01", executor="Kunj+agent", week="W1", hours=6),
        row("CAP-02", "Baseline", executor="Kunj+agent", week="W1", hours=8, deps="CAP-01"),
        row("MEM-03", "Post capture", "MEM", "Anurup", "Kunj+agent", "W2", 6, deps="CAP-01"),
        row("MOD-09", "Nice to have", "MOD", "Anurup", "Minh+agent", "W3", 4, "P1", "CAP-02"),
        row("OPS-09", "Packet", "OPS", "Felipe", "Felipe", "W4", 5, deps="all Beta P0"),
        row("MOD-11", "Final work", "MOD", "Anurup", "Anurup+agent", "W7", 10),
    ])
    return p.parse_manifest(text)


class ClosedTests(unittest.TestCase):
    def test_ids_come_from_closed_issue_titles_only(self):
        issues = [
            {"title": "[CAP-01] Per-stage timings", "state": "closed"},
            {"title": "[CAP-02] Baseline", "state": "opened"},
            {"title": "Random issue", "state": "closed"},
            {"title": "[MEM-03] Post capture", "state": "closed"},
        ]
        self.assertEqual(pp.closed_ids_from_issues(issues), {"CAP-01", "MEM-03"})


class SummaryTests(unittest.TestCase):
    def setUp(self):
        self.tickets = manifest()

    def test_summary_by_phase_priority_and_executor(self):
        by_phase, by_ws, by_exec = pp.summarize(self.tickets, {"CAP-01"})
        self.assertEqual(by_phase["Beta P0"], {"tickets": 4, "closed_tickets": 1, "hours": 25, "closed_hours": 6})
        self.assertEqual(by_phase["Beta P1"]["tickets"], 1)
        self.assertEqual(by_phase["Final P0"]["hours"], 10)
        self.assertEqual(by_exec["Kunj"]["hours"], 20)
        self.assertEqual(by_ws["CAP"]["closed_tickets"], 1)

    def test_readiness_respects_dependencies_and_the_all_rule(self):
        t = {x["id"]: x for x in self.tickets}
        self.assertTrue(pp.is_ready(t["CAP-01"], self.tickets, set()))
        self.assertFalse(pp.is_ready(t["CAP-02"], self.tickets, set()))
        self.assertTrue(pp.is_ready(t["CAP-02"], self.tickets, {"CAP-01"}))
        # OPS-09 waits for every other Beta P0: CAP-01, CAP-02, MEM-03
        self.assertFalse(pp.is_ready(t["OPS-09"], self.tickets, {"CAP-01", "CAP-02"}))
        self.assertTrue(pp.is_ready(t["OPS-09"], self.tickets, {"CAP-01", "CAP-02", "MEM-03"}))

    def test_next_up_is_p0_first_then_by_week_and_excludes_closed_and_blocked(self):
        nxt = pp.next_up(self.tickets, {"CAP-01"})
        self.assertEqual([x["id"] for x in nxt["Kunj"]], ["CAP-02", "MEM-03"])
        self.assertEqual(nxt["Minh"], [])  # MOD-09 needs CAP-02
        nxt2 = pp.next_up(self.tickets, {"CAP-01", "CAP-02"})
        self.assertEqual([x["id"] for x in nxt2["Minh"]], ["MOD-09"])


class RenderTests(unittest.TestCase):
    def test_render_has_all_sections_and_percentages(self):
        text = pp.render(manifest(), {"CAP-01"})
        for s in ["# Phase progress", "## By phase and priority", "## By workstream", "## By executor", "## Next up"]:
            self.assertIn(s, text)
        self.assertIn("| Beta P0 | 1 of 4 | 6 of 25 | 24% |", text)

    def test_cli_with_manual_closed_and_issues_json_and_unknown_ids(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "plan.md").write_text(HEADER + row("CAP-01") + row("CAP-02", "Baseline", deps="CAP-01"))
            (d / "issues.json").write_text(json.dumps([{"title": "[CAP-01] x", "state": "closed"}]))
            out = d / "out" / "progress.md"
            rc = pp.main(["--manifest", str(d / "plan.md"), "--issues-json", str(d / "issues.json"), "--closed", "ZZZ-99", "--out", str(out)])
            self.assertEqual(rc, 0)
            self.assertIn("| Beta P0 | 1 of 2 |", out.read_text())

    def test_api_mode_without_a_token_fails_cleanly(self):
        import os
        old = os.environ.pop("GITLAB_TOKEN", None)
        try:
            with tempfile.TemporaryDirectory() as d:
                (Path(d) / "plan.md").write_text(HEADER + row("CAP-01"))
                self.assertEqual(pp.main(["--manifest", str(Path(d) / "plan.md"), "--api"]), 1)
        finally:
            if old is not None:
                os.environ["GITLAB_TOKEN"] = old


class RealManifestTests(unittest.TestCase):
    def test_real_manifest_renders_and_nothing_is_ready_that_has_open_dependencies(self):
        from test_plan_to_csv import _find_plan_dir
        plan_dir = _find_plan_dir()
        if plan_dir is None:
            self.skipTest("master plan not found from this location")
        tickets = p.parse_manifest((plan_dir / "2026-09-21-beta-final-master-plan.md").read_text())
        text = pp.render(tickets, set())
        self.assertIn("Beta P0", text)
        ready_at_start = {t["id"] for t in tickets if pp.is_ready(t, tickets, set())}
        self.assertTrue({"OPS-01", "CAP-01", "CAP-03", "MEM-01"} <= ready_at_start)
        self.assertNotIn("CAP-02", ready_at_start)


if __name__ == "__main__":
    unittest.main()
