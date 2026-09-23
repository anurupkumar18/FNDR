import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import gitlab_sync as gs  # noqa: E402

ROSTER = {"anurupkumar": "Anurup", "minhpro001": "Minh"}

SAMPLE = """# Lane

Intro text that is not a ticket.

## VS-01 Record the baseline
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p0
- milestone: W02-Measure
- estimate: 3h
- depends: none

**Why.** Numbers first.

| a | b |
|---|---|
| 1 | 2 |

## VS-02 Add a persona
- assignee: anurupkumar
- labels: area::vault-search, type::qa, prio::p1
- milestone: W03-Build
- estimate: 4.5h
- depends: VS-01

**Why.** Coverage.
"""


class ParseTest(unittest.TestCase):
    def test_parses_metadata_body_and_dependencies(self):
        tickets = gs.parse_tickets(SAMPLE, "docs/team/tickets/sample.md")
        self.assertEqual([t.id for t in tickets], ["VS-01", "VS-02"])
        first, second = tickets
        self.assertEqual(first.title, "Record the baseline")
        self.assertEqual(first.labels, ["area::vault-search", "type::qa", "prio::p0"])
        self.assertEqual(first.depends, [])
        self.assertIn("| 1 | 2 |", first.body)
        self.assertNotIn("VS-02", first.body)
        self.assertEqual(second.depends, ["VS-01"])
        self.assertEqual(second.hours, 4.5)
        self.assertEqual(second.prio, "prio::p1")

    def test_validation_catches_bad_tickets(self):
        bad = SAMPLE.replace("assignee: anurupkumar\n- labels: area::vault-search, type::qa, prio::p1",
                             "assignee: someone\n- labels: area::nope, type::qa")
        bad = bad.replace("W03-Build", "W99-Nope").replace("depends: VS-01", "depends: XX-99")
        bad += "\n## VS-01 Duplicate\n- assignee: anurupkumar\n- labels: prio::p0\n- milestone: W02-Measure\n- estimate: 1h\n- depends: none\n\nBody \u2014 dash.\n"
        errors = "\n".join(gs.validate(gs.parse_tickets(bad, "x.md"), ROSTER))
        for expected in ("duplicate id", "someone", "W99-Nope", "needs a prio", "area::nope", "XX-99", "em or en dash"):
            self.assertIn(expected, errors)


class RenderTest(unittest.TestCase):
    def setUp(self):
        self.tickets = gs.parse_tickets(SAMPLE, "docs/team/tickets/sample.md")

    def test_title_and_create_labels_include_owner_and_ready(self):
        t = self.tickets[0]
        self.assertEqual(gs.issue_title(t), "[VS-01] Record the baseline")
        self.assertEqual(
            gs.create_labels(t, ROSTER),
            ["area::vault-search", "type::qa", "prio::p0", "owner::anurup", "status::ready", "phase::beta",
             "evidence::needed"],
        )

    def test_description_links_dependencies_once_issues_exist(self):
        t = self.tickets[1]
        self.assertIn("Depends on: VS-01.", gs.render_description(t, {}))
        self.assertIn("Depends on: #12 (VS-01).", gs.render_description(t, {"VS-01": 12}))
        self.assertIn("Source: `docs/team/tickets/sample.md`", gs.render_description(t, {}))

    def test_duration_uses_gitlab_time_format(self):
        self.assertEqual(gs.gitlab_duration(3), "3h")
        self.assertEqual(gs.gitlab_duration(4.5), "4h30m")

    def test_status_change_swaps_only_status_labels(self):
        add, remove = gs.status_change(["prio::p0", "status::ready", "owner::anurup"], "doing")
        self.assertEqual(add, ["status::doing"])
        self.assertEqual(remove, ["status::ready"])
        self.assertEqual(gs.status_change(["status::doing"], "doing"), ([], []))

    def test_board_url_falls_back_to_a_filter_when_scope_is_unavailable(self):
        self.assertTrue(gs.board_url(7, True, "minhpro001").endswith("/-/boards/7"))
        self.assertTrue(gs.board_url(7, False, "minhpro001").endswith("/-/boards/7?assignee_username=minhpro001"))
        self.assertTrue(gs.board_url(7, False, None).endswith("/-/boards/7"))


class RealTicketsTest(unittest.TestCase):
    def test_every_ticket_file_is_valid_and_everyone_has_work(self):
        roster = gs.load_roster()
        tickets = gs.load_all_tickets()
        self.assertEqual(gs.validate(tickets, roster), [])
        self.assertGreaterEqual(len(tickets), 100)
        for username in roster:
            mine = [t for t in tickets if t.assignee == username]
            self.assertGreaterEqual(len(mine), 15, username)
            self.assertTrue(any(t.id.startswith("PD-") for t in mine), f"{username} has no product ticket")


if __name__ == "__main__":
    unittest.main()
