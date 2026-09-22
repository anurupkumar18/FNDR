import csv
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import plan_to_csv as p

HEADER = ("| ID | Title | WS | Owner | Executor | Week | Hours | Prio | Deps | Why | Earlier | PR shape | Verify | Evidence |\n"
          "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n")


def row(tid, title="Per-stage timings", ws="CAP", owner="Anurup", executor="Kunj+agent", week="W1", hours=6,
        prio="P0", deps="none"):
    return (f"| {tid} | {title} | {ws} | {owner} | {executor} | {week} | {hours} | {prio} | {deps} | Nothing is timed | "
            f"Only flush time exists | telemetry and loop | cargo test passes | Test output |\n")


ROW_A = row("CAP-01")
ROW_B = row("CAP-02", "Baseline report", deps="CAP-01", hours=8)
ROW_C = row("OPS-09", "Evidence packet", "OPS", "Felipe", "Felipe", "W4", 5, deps="all Beta P0")
ROW_D = row("MEM-03", "Post capture timings", "MEM", "Anurup", "Kunj+agent", "W2", 6, deps="CAP-01")


class ParseTests(unittest.TestCase):
    def test_rows_parse_and_other_tables_are_ignored(self):
        text = "| Other | Table |\n|---|---|\n| foo | bar |\n\n" + HEADER + ROW_A + ROW_B
        tickets = p.parse_manifest(text)
        self.assertEqual([t["id"] for t in tickets], ["CAP-01", "CAP-02"])
        self.assertEqual(tickets[0]["hours"], 6)
        self.assertEqual(tickets[1]["earlier"], "Only flush time exists")

    def test_wrong_column_count_duplicates_and_unknown_deps_are_errors(self):
        with self.assertRaises(ValueError):
            p.parse_manifest("| CAP-01 | only | three |\n")
        with self.assertRaises(ValueError):
            p.parse_manifest(ROW_A + ROW_A)
        with self.assertRaises(ValueError):
            p.parse_manifest(ROW_B)
        p.parse_manifest(ROW_A + ROW_B + ROW_C)  # "all Beta P0" is allowed


class HelperTests(unittest.TestCase):
    def setUp(self):
        self.tickets = p.parse_manifest(ROW_A + ROW_B + ROW_D + ROW_C)

    def test_executor_and_agent_flags(self):
        t = self.tickets[0]
        self.assertEqual(p.executor_name(t), "Kunj")
        self.assertTrue(p.uses_agent(t))
        self.assertFalse(p.uses_agent(self.tickets[3]))

    def test_branch_name_and_size_class(self):
        self.assertEqual(p.branch_name(self.tickets[0]), "feat/cap-01-per-stage-timings")
        self.assertEqual(p.branch_name(self.tickets[3]), "chore/ops-09-evidence-packet")
        self.assertTrue(p.size_class(2).startswith("S"))
        self.assertTrue(p.size_class(8).startswith("M"))
        self.assertTrue(p.size_class(9).startswith("L"))

    def test_position_and_hours_before_follow_schedule_order(self):
        # Beta order: CAP-01 (W1), CAP-02 (W1), MEM-03 (W2), OPS-09 (W4). P0 hours before MEM-03: 6 + 8.
        k, n, before = p.position_in_phase(self.tickets, self.tickets[2])
        self.assertEqual((k, n, before), (3, 4, 14))

    def test_unblocks_lists_dependents(self):
        self.assertEqual(p.unblocks(self.tickets, self.tickets[0]), ["CAP-02", "MEM-03"])
        self.assertEqual(p.unblocks(self.tickets, self.tickets[1]), [])

    def test_load_by_executor_counts_nominal_hours_by_priority(self):
        load = p.load_by_executor(self.tickets)
        self.assertEqual(load["Kunj"], {"P0": 20, "P1": 0})  # 6 + 8 + 6
        self.assertEqual(load["Felipe"], {"P0": 5, "P1": 0})


class DescriptionTests(unittest.TestCase):
    def setUp(self):
        self.tickets = p.parse_manifest(ROW_A + ROW_B + ROW_D + ROW_C)
        self.t = self.tickets[2]

    def test_required_sections_and_fields_are_present(self):
        d = p.build_description(self.t, {}, self.tickets)
        for text in ["**Owner (accountable):** Anurup", "**Executor:** Kunj (with an AI coding agent)", "**Time expected:** 6 hours",
                     "**Phase:** Beta, ticket 3 of 4", "## Why", "## Where this fits", "- Earlier: Only flush time exists",
                     "- Must be closed before you start: CAP-01", "## Pull request shape", "- Branch: `feat/mem-03-post-capture-timings`",
                     "## How to verify", "## Evidence to attach", "## Definition of done", "## Agent brief and switching between tools",
                     "docs/team/agent-switch.md", "make phase-progress"]:
            self.assertIn(text, d)
        self.assertIn("2026-09-21-ws5-post-capture-memory-pipeline.md", d)

    def test_quick_actions_use_executor_labels_and_agent_lane(self):
        d = p.build_description(self.t, {"kunj": "kunj.r"}, self.tickets)
        self.assertIn('/label ~"ws::memory" ~"type::feature" ~"prio::p0" ~"phase::beta" ~"status::ready" ~"evidence::needed" ~"agent-ok" ~"agent::either"', d)
        self.assertIn("/milestone %W02-Measure", d)
        self.assertIn("/estimate 6h", d)
        self.assertIn("/assign @kunj.r", d)

    def test_human_tickets_get_needs_human_and_no_agent_lane(self):
        d = p.build_description(self.tickets[3], {}, self.tickets)
        self.assertIn('~"needs-human"', d)
        self.assertNotIn("agent::either", d)
        self.assertNotIn("/assign", d)
        self.assertIn("(human work, no agent expected)", d)

    def test_final_phase_weeks_get_the_final_label(self):
        t = dict(self.t, week="W7", id="MEM-03")
        d = p.build_description(t, {}, [t])
        self.assertIn('~"phase::final"', d)
        self.assertIn("/milestone %W07-FineTune", d)


class CliTests(unittest.TestCase):
    def test_csv_has_title_description_and_filters_by_week_but_keeps_context(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "plan.md").write_text(HEADER + ROW_A + ROW_B + ROW_C)
            out = d / "out.csv"
            self.assertEqual(p.main([str(d / "plan.md"), "--out", str(out), "--weeks", "W1"]), 0)
            with out.open() as fh:
                rows = list(csv.reader(fh))
            self.assertEqual(rows[0], ["title", "description"])
            self.assertEqual([r[0] for r in rows[1:]], ["[CAP-01] Per-stage timings", "[CAP-02] Baseline report"])
            self.assertIn("ticket 1 of 3", rows[1][1])  # position uses the whole phase, not just the selected week

    def test_roster_file_is_used_for_the_executor(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            (d / "plan.md").write_text(HEADER + ROW_A)
            (d / "roster.json").write_text(json.dumps({"Kunj": "kunj.r"}))
            out = d / "out.csv"
            p.main([str(d / "plan.md"), "--out", str(out), "--roster", str(d / "roster.json")])
            with out.open() as fh:
                rows = list(csv.reader(fh))
            self.assertIn("/assign @kunj.r", rows[1][1])


def _find_plan_dir():
    """Walk up from this file to the directory holding the master plan (works in scripts/team and in the assets dir)."""
    for parent in Path(__file__).resolve().parents:
        for candidate in (parent / "docs/superpowers/plans", parent):
            if (candidate / "2026-09-21-beta-final-master-plan.md").exists():
                return candidate
    return None


class RealManifestTests(unittest.TestCase):
    def setUp(self):
        self.plan_dir = _find_plan_dir()
        if self.plan_dir is None:
            self.skipTest("master plan not found from this location")
        self.tickets = p.parse_manifest((self.plan_dir / "2026-09-21-beta-final-master-plan.md").read_text())
        self.by_id = {t["id"]: t for t in self.tickets}

    def test_every_ticket_has_steps_in_the_plan_file_its_link_points_to(self):
        missing = []
        for t in self.tickets:
            plan = self.plan_dir / p.PLAN_FILES[p.prefix_of(t)]
            if not plan.exists():
                missing.append(f"{plan.name} does not exist")
            elif t["id"] not in plan.read_text():
                missing.append(f"{t['id']} not found in {plan.name}")
        self.assertEqual(missing, [])

    def test_nobody_is_planned_over_four_weeks_of_fifteen_hours_of_p0(self):
        over = {name: v["P0"] for name, v in p.load_by_executor(self.tickets).items() if v["P0"] > 60}
        self.assertEqual(over, {})

    def test_people_weeks_and_prefixes_are_known(self):
        for t in self.tickets:
            self.assertIn(t["week"], p.MILESTONES)
            self.assertIn(p.prefix_of(t), p.WS_LABELS)
            self.assertIn(t["owner"], p.PEOPLE)
            self.assertIn(p.executor_name(t), p.PEOPLE)
            self.assertIn(t["prio"], {"P0", "P1"})

    def test_dependencies_are_never_scheduled_after_the_ticket_that_needs_them(self):
        late = []
        for t in self.tickets:
            for dep in p.deps_of(t):
                if dep in self.by_id and p.week_number(self.by_id[dep]["week"]) > p.week_number(t["week"]):
                    late.append(f"{t['id']} ({t['week']}) needs {dep} ({self.by_id[dep]['week']})")
        self.assertEqual(late, [])

    def test_dependency_graph_has_no_cycles(self):
        state = {}

        def visit(tid, stack):
            if state.get(tid) == "done":
                return
            self.assertNotIn(tid, stack, f"cycle through {tid}")
            for dep in p.deps_of(self.by_id[tid]):
                if dep in self.by_id:
                    visit(dep, stack + [tid])
            state[tid] = "done"

        for tid in self.by_id:
            visit(tid, [])

    def test_a_p0_ticket_never_depends_on_a_p1_ticket(self):
        bad = [f"{t['id']} needs {d}" for t in self.tickets if t["prio"] == "P0"
               for d in p.deps_of(t) if d in self.by_id and self.by_id[d]["prio"] == "P1"]
        self.assertEqual(bad, [])


if __name__ == "__main__":
    unittest.main()
