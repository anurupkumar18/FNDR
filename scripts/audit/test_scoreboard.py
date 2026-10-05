import contextlib
import datetime
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import scoreboard as sb  # noqa: E402

try:
    import vault_health as vh
except ImportError:  # numpy is not installed; the contract test is skipped.
    vh = None


EM_DASH = "\u2014"
EN_DASH = "\u2013"


def v1_report(case_set="knowledge-worker"):
    """Shaped like docs/evidence/W02/retrieval-baseline-seeded.json (schema v1)."""
    return {
        "schema_version": 1,
        "case_set": case_set,
        "case_count": 22,
        "paths": {
            "search": {
                "recall_at_5": 0.9545454545454546,
                "mrr_at_10": 0.9090909090909091,
                "latency_ms": {"p50": 595, "p95": 764},
                "recall_at_5_by_kind": {"keyword": 1.0, "paraphrase": 0.875},
            },
            "ask": {
                "recall_at_5": 1.0,
                "mrr_at_10": 1.0,
                "latency_ms": {"p50": 1234, "p95": 1418},
                "recall_at_5_by_kind": {"keyword": 1.0, "paraphrase": 1.0},
            },
        },
        "top1_agreement": {"count": 17, "total": 22, "rate": 0.7727272727272727},
        "queries": [],
    }


def v2_report(case_set="office-pm"):
    """Shaped like scripts/demo/retrieval-reference/office-pm.json (schema v2)."""
    return {
        "schema_version": 2,
        "case_set": case_set,
        "case_count": 37,
        "paths": {
            "search": {
                "recall_at_5": 0.7,
                "mrr_at_10": 0.5891666666666666,
                "latency_ms": {"p50": 724, "p95": 913},
                "recall_at_5_by_kind": {"paraphrase": 0.3333333333333333, "time": 0.875},
                "no_match": {"cases": 4, "returned_nothing": 2},
            },
            "ask": {
                "recall_at_5": 0.9,
                "mrr_at_10": 0.6571428571428571,
                "latency_ms": {"p50": 3048, "p95": 3197},
                "recall_at_5_by_kind": {"paraphrase": 0.7777777777777778, "time": 0.875},
            },
        },
        "top1_agreement": {"count": 12, "total": 37, "rate": 0.32432432432432434},
        "queries": [],
    }


# Same shape as docs/evidence/W02/vault-health-owner.md, as rendered by vault_health.py.
VAULT_HEALTH_MD = """# FNDR vault health

Aggregate counts only; no memory text, titles, URLs, or file paths.

## Table health

| table | role | present | rows | active days | date range | rows/active day | zero/missing vectors | median clean chars | exact reopen |
|---|---|:---:|---:|---:|---|---:|---:|---:|---:|
| `memories_v4_minilm_384` | current parent | yes | 29 | 5 | 2026-09-17 to 2026-09-23 | 5.8 | 0.0% | 129 | 10.3% |
| `memories_v5_bge_1024` | future parent | yes | 0 | 0 | n/a | 0.0 | n/a | n/a | n/a |
| `memory_chunks_v1_bge_1024` | chunk | yes | 0 | 0 | n/a | 0.0 | n/a | n/a | n/a |

## Structured fields filled: `memories_v4_minilm_384`

| field | filled |
|---|---:|
| project | 0.0% |
| topic | 100.0% |
| outcome | 0.0% |
| next_steps | 0.0% |
| decisions | 0.0% |
| errors | 0.0% |

## Summary source: `memories_v4_minilm_384`

| source | rows |
|---|---:|
| fallback | 17 |
"""


def section(text, heading):
    """The lines of one '## heading' section, up to the next '## ' heading."""
    start = text.index(f"## {heading}\n")
    end = text.find("\n## ", start + 1)
    return text[start:] if end == -1 else text[start:end]


class ScoreboardTestCase(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def write(self, name, content):
        path = self.tmp / name
        path.write_text(content if isinstance(content, str) else json.dumps(content))
        return str(path)

    def run_main(self, *argv):
        stdout = io.StringIO()
        with contextlib.redirect_stdout(stdout):
            code = sb.main(list(argv))
        return code, stdout.getvalue()


class RetrievalTests(ScoreboardTestCase):
    def test_schema_v1_rows_show_each_path_metric(self):
        code, out = self.run_main("--retrieval", self.write("baseline.json", v1_report()), "--date", "2026-10-09")
        self.assertEqual(code, 0)
        retrieval = section(out, "Retrieval")
        self.assertIn("| knowledge-worker | search | 0.955 | 0.875 | 0.909 | 764 | 17/22 (0.773) |", retrieval)
        self.assertIn("| knowledge-worker | ask | 1.000 | 1.000 | 1.000 | 1418 | 17/22 (0.773) |", retrieval)

    def test_schema_v2_rows_show_each_path_metric(self):
        code, out = self.run_main("--retrieval", self.write("office.json", v2_report()))
        self.assertEqual(code, 0)
        retrieval = section(out, "Retrieval")
        self.assertIn("| office-pm | search | 0.700 | 0.333 | 0.589 | 913 | 12/37 (0.324) |", retrieval)
        self.assertIn("| office-pm | ask | 0.900 | 0.778 | 0.657 | 3197 | 12/37 (0.324) |", retrieval)

    def test_persona_comes_from_case_set_not_file_name(self):
        code, out = self.run_main(
            "--retrieval", self.write("a.json", v1_report()),
            "--retrieval", self.write("b.json", v2_report(case_set="persona-three")),
        )
        self.assertEqual(code, 0)
        self.assertIn("| knowledge-worker | search |", out)
        self.assertIn("| persona-three | search |", out)
        self.assertNotIn("| a |", out)

    def test_beta_targets_are_marked_met_or_not_met(self):
        _, out = self.run_main(
            "--retrieval", self.write("kw.json", v1_report()),
            "--retrieval", self.write("pm.json", v2_report()),
        )
        retrieval = section(out, "Retrieval")
        self.assertIn("| knowledge-worker | Search Recall@5 | 0.90 or higher | 0.955 | met |", retrieval)
        self.assertIn("| knowledge-worker | Paraphrase Recall@5 (Search) | 0.80 or higher | 0.875 | met |", retrieval)
        self.assertIn("| knowledge-worker | Search and Ask same top result | always | 17/22 | not met |", retrieval)
        self.assertIn("| office-pm | Search Recall@5 | 0.90 or higher | 0.700 | not met |", retrieval)
        self.assertIn("| office-pm | Paraphrase Recall@5 (Search) | 0.80 or higher | 0.333 | not met |", retrieval)

    def test_target_boundary_counts_as_met_and_full_agreement_is_always(self):
        report = v1_report()
        report["paths"]["search"]["recall_at_5"] = 0.9
        report["paths"]["search"]["recall_at_5_by_kind"]["paraphrase"] = 0.8
        report["top1_agreement"] = {"count": 22, "total": 22, "rate": 1.0}
        _, out = self.run_main("--retrieval", self.write("edge.json", report))
        self.assertIn("| Search Recall@5 | 0.90 or higher | 0.900 | met |", out)
        self.assertIn("| Paraphrase Recall@5 (Search) | 0.80 or higher | 0.800 | met |", out)
        self.assertIn("| Search and Ask same top result | always | 22/22 | met |", out)

    def test_missing_fields_are_not_measured(self):
        report = v1_report()
        del report["top1_agreement"]
        del report["paths"]["search"]["recall_at_5_by_kind"]
        del report["paths"]["search"]["latency_ms"]
        _, out = self.run_main("--retrieval", self.write("sparse.json", report))
        self.assertIn("| knowledge-worker | search | 0.955 | n/a | 0.909 | n/a | n/a |", out)
        self.assertIn("| Paraphrase Recall@5 (Search) | 0.80 or higher | n/a | not measured |", out)
        self.assertIn("| Search and Ask same top result | always | n/a | not measured |", out)

    def test_no_reports_means_not_measured(self):
        code, out = self.run_main()
        self.assertEqual(code, 0)
        self.assertIn("not measured", section(out, "Retrieval"))
        self.assertNotIn("| search |", out)

    def test_missing_unreadable_and_unsupported_files_are_skipped_not_fatal(self):
        future = v1_report()
        future["schema_version"] = 9
        code, out = self.run_main(
            "--retrieval", str(self.tmp / "absent.json"),
            "--retrieval", self.write("broken.json", "{not json"),
            "--retrieval", self.write("future.json", future),
            "--retrieval", self.write("good.json", v2_report()),
        )
        self.assertEqual(code, 0)
        retrieval = section(out, "Retrieval")
        self.assertIn("absent.json: not found, not measured", retrieval)
        self.assertIn("broken.json: could not read, not measured", retrieval)
        self.assertIn("future.json: unsupported schema_version 9, not measured", retrieval)
        self.assertIn("| office-pm | search | 0.700 |", retrieval)

    def test_sources_name_files_only(self):
        path = self.write("retrieval-baseline-seeded.json", v1_report())
        _, out = self.run_main("--retrieval", path)
        self.assertIn("retrieval-baseline-seeded.json (schema v1, 22 cases)", out)
        self.assertNotIn(str(self.tmp), out)


class VaultHealthTests(ScoreboardTestCase):
    def test_parses_aggregate_numbers(self):
        parsed = sb.parse_vault_health(VAULT_HEALTH_MD)
        self.assertEqual(parsed["memories"], 29)
        self.assertEqual(parsed["active_days"], 5)
        self.assertEqual(parsed["memories_per_active_day"], 5.8)
        self.assertEqual(parsed["median_clean_chars"], 129)
        self.assertEqual(parsed["exact_reopen_pct"], 10.3)
        self.assertEqual(parsed["chunk_rows"], 0)
        self.assertEqual(parsed["structured_pct"]["project"], 0.0)
        self.assertEqual(parsed["structured_pct"]["topic"], 100.0)
        self.assertEqual(list(parsed["structured_pct"]), ["project", "topic", "outcome", "next_steps", "decisions", "errors"])

    def test_vault_table_rows_and_targets(self):
        _, out = self.run_main("--vault-health", self.write("vault-health-owner.md", VAULT_HEALTH_MD))
        vault = section(out, "Vault health")
        self.assertIn("vault-health-owner.md", vault)
        self.assertIn("| Memories per active day | 5.8 (29 memories, 5 active days) | none | n/a |", vault)
        self.assertIn("| Median clean text per memory | 129 characters | 800 or more | not met |", vault)
        self.assertIn("| Memories with a project | 0.0% | 60% or more | not met |", vault)
        self.assertIn("| Memories with next steps | 0.0% | 60% or more | not met |", vault)
        self.assertIn("| Other structured fields | topic 100.0%, outcome 0.0%, decisions 0.0%, errors 0.0% | none | n/a |", vault)
        self.assertIn("| Chunk rows | 0 | every memory | not met |", vault)
        self.assertIn("| Memories that reopen exactly | 10.3% | 90% of a live day | not met |", vault)

    def test_targets_met_when_numbers_reach_them(self):
        text = (
            VAULT_HEALTH_MD.replace("| 129 | 10.3% |", "| 812 | 90.0% |")
            .replace("| project | 0.0% |", "| project | 61.0% |")
            .replace("| next_steps | 0.0% |", "| next_steps | 60.0% |")
            .replace("| chunk | yes | 0 | 0 |", "| chunk | yes | 140 | 3 |")
        )
        _, out = self.run_main("--vault-health", self.write("vh.md", text))
        self.assertIn("| Median clean text per memory | 812 characters | 800 or more | met |", out)
        self.assertIn("| Memories with a project | 61.0% | 60% or more | met |", out)
        self.assertIn("| Memories with next steps | 60.0% | 60% or more | met |", out)
        self.assertIn("| Memories that reopen exactly | 90.0% | 90% of a live day | met |", out)
        # Chunk rows count chunks, not memories, so a non-zero count cannot prove coverage.
        self.assertIn("| Chunk rows | 140 | every memory | unknown |", out)

    def test_missing_lines_become_not_measured(self):
        text = VAULT_HEALTH_MD.split("## Structured fields filled")[0]
        text = text.replace("| `memory_chunks_v1_bge_1024` | chunk | yes | 0 | 0 | n/a | 0.0 | n/a | n/a | n/a |\n", "")
        text = text.replace("| 129 | 10.3% |", "| n/a | n/a |")
        parsed = sb.parse_vault_health(text)
        self.assertIsNone(parsed["median_clean_chars"])
        self.assertIsNone(parsed["chunk_rows"])
        self.assertEqual(parsed["structured_pct"], {})
        _, out = self.run_main("--vault-health", self.write("partial.md", text))
        vault = section(out, "Vault health")
        self.assertIn("| Memories per active day | 5.8 (29 memories, 5 active days) | none | n/a |", vault)
        self.assertIn("| Median clean text per memory | not measured | 800 or more | not measured |", vault)
        self.assertIn("| Memories with a project | not measured | 60% or more | not measured |", vault)
        self.assertIn("| Chunk rows | not measured | every memory | not measured |", vault)
        self.assertIn("| Memories that reopen exactly | not measured | 90% of a live day | not measured |", vault)

    def test_unrelated_markdown_parses_to_nothing(self):
        parsed = sb.parse_vault_health("# Notes\n\nNothing here.\n")
        self.assertIsNone(parsed["memories_per_active_day"])
        self.assertIsNone(parsed["exact_reopen_pct"])

    def test_missing_or_absent_vault_health_is_not_measured(self):
        _, out = self.run_main()
        self.assertIn("not measured", section(out, "Vault health"))
        _, out = self.run_main("--vault-health", str(self.tmp / "gone.md"))
        self.assertIn("gone.md: not found, not measured", section(out, "Vault health"))

    @unittest.skipIf(vh is None, "vault_health.py needs numpy")
    def test_parses_what_vault_health_renders(self):
        report = {"table_health": {}}
        for name, role, *_ in vh._TABLE_SPECS:
            report["table_health"][name] = vh._empty_table_health(name, role)
        report["table_health"][vh.V4_PARENT_TABLE].update(
            {
                "present": True,
                "rows": 40,
                "active_days": 4,
                "first_day": "2026-10-01",
                "last_day": "2026-10-04",
                "rows_per_active_day": 10.0,
                "zero_or_missing_vectors_pct": 0.0,
                "clean_text_chars_p50": 640,
                "exact_reopen_pct": 55.0,
                "structured_pct": {field: 25.0 for field in vh.STRUCTURED_FIELDS},
                "summary_source": {"llm": 40},
            }
        )
        report["table_health"][vh.CHUNK_TABLE].update({"present": True, "rows": 90})
        parsed = sb.parse_vault_health(vh.render(report))
        self.assertEqual(parsed["memories_per_active_day"], 10.0)
        self.assertEqual(parsed["median_clean_chars"], 640)
        self.assertEqual(parsed["exact_reopen_pct"], 55.0)
        self.assertEqual(parsed["chunk_rows"], 90)
        self.assertEqual(parsed["structured_pct"]["next_steps"], 25.0)


class PageTests(ScoreboardTestCase):
    def test_title_carries_the_date(self):
        _, out = self.run_main("--date", "2026-10-09")
        self.assertTrue(out.startswith("# FNDR Friday scoreboard: 2026-10-09\n"))

    def test_date_defaults_to_today(self):
        _, out = self.run_main()
        self.assertTrue(out.startswith(f"# FNDR Friday scoreboard: {datetime.date.today().isoformat()}\n"))

    def test_bad_date_is_refused(self):
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            sb.main(["--date", "next friday"])

    def test_not_measured_yet_lists_what_has_no_input(self):
        _, out = self.run_main()
        pending = section(out, "Not measured yet")
        for measure in ("Voice latency", "Voice command success", "Reopen outcome share", "User sessions"):
            self.assertIn(measure, pending)

    def test_voice_and_sessions_files_are_linked_by_name(self):
        voice = self.write("voice-baseline.md", "# whatever is inside is not parsed\n")
        sessions = self.write("pd-18-sessions.md", "# notes\n")
        _, out = self.run_main("--voice", voice, "--sessions", sessions)
        linked = section(out, "Linked evidence")
        self.assertIn("Voice latency: see `voice-baseline.md`", linked)
        self.assertIn("User sessions: see `pd-18-sessions.md`", linked)
        pending = section(out, "Not measured yet")
        self.assertNotIn("Voice latency", pending)
        self.assertNotIn("User sessions", pending)
        self.assertIn("Voice command success", pending)
        self.assertNotIn("whatever is inside", out)
        self.assertNotIn(str(self.tmp), out)

    def test_missing_linked_file_stays_not_measured(self):
        _, out = self.run_main("--voice", str(self.tmp / "nope.md"))
        self.assertIn("Voice latency", section(out, "Not measured yet"))
        self.assertIn("nope.md not found", out)

    def test_output_never_contains_absolute_paths_or_long_dashes(self):
        report = v2_report(case_set=f"office{EM_DASH}pm{EN_DASH}x")
        _, out = self.run_main(
            "--retrieval", self.write("r.json", report),
            "--vault-health", self.write("v.md", VAULT_HEALTH_MD),
            "--voice", self.write("voice.md", "x"),
        )
        self.assertNotIn(str(self.tmp), out)
        self.assertNotIn(EM_DASH, out)
        self.assertNotIn(EN_DASH, out)

    def test_out_writes_the_same_page(self):
        target = self.tmp / "nested" / "scoreboard.md"
        _, out = self.run_main("--retrieval", self.write("r.json", v1_report()), "--out", str(target))
        self.assertEqual(target.read_text(), out)


if __name__ == "__main__":
    unittest.main()
