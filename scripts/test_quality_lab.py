import importlib.util
import json
import tempfile
import unittest
from unittest.mock import patch
from pathlib import Path


SCRIPT = Path(__file__).with_name("quality_lab.py")
SPEC = importlib.util.spec_from_file_location("quality_lab", SCRIPT)
quality_lab = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(quality_lab)


class QualityLabProfileTests(unittest.TestCase):
    def test_source_fingerprint_covers_ui_styles_tests_runner_and_dependency_locks(self):
        paths = {path.relative_to(quality_lab.ROOT).as_posix() for path in quality_lab.fingerprint_paths()}
        self.assertIn("scripts/quality_lab.py", paths)
        self.assertIn("scripts/test_quality_lab.py", paths)
        self.assertTrue(any(path.startswith("src/") and path.endswith(".css") for path in paths))
        self.assertTrue(any(path.startswith("src-tauri/tests/") and path.endswith(".rs") for path in paths))
        self.assertIn("package-lock.json", paths)
        self.assertIn("src-tauri/Cargo.lock", paths)

    def test_profile_fingerprint_omits_shared_model_symlink_targets(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            profile = root / "profile"
            profile.mkdir()
            (profile / "memory.db").write_bytes(b"synthetic")
            shared = root / "shared-models"
            shared.mkdir()
            (shared / "large.weights").write_bytes(b"do not hash model files")
            (profile / "models").symlink_to(shared)
            snapshot = quality_lab.profile_snapshot(profile)
            self.assertEqual(snapshot["file_count"], 1)
            self.assertEqual(snapshot["bytes"], len(b"synthetic"))
            self.assertEqual(snapshot["shared_asset_links"], ["models"])

    def test_profile_isolated_under_fndr_quality_lab_namespace(self):
        with tempfile.TemporaryDirectory() as temp:
            home = Path(temp)
            profile = quality_lab.profile_for(home, "knowledge-worker")
            self.assertEqual(
                profile,
                home / "Library/Application Support/com.fndr.app.quality-lab/knowledge-worker",
            )

    def test_rejects_owner_vault_and_its_parent_or_children(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            owner = root / "Library/Application Support/com.fndr.app"
            for candidate in (owner, owner / "nested", owner.parent):
                with self.subTest(candidate=candidate):
                    with self.assertRaises(ValueError):
                        quality_lab.assert_isolated(candidate, owner)

    def test_accepts_sibling_quality_profile(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            owner = root / "Library/Application Support/com.fndr.app"
            lab = root / "Library/Application Support/com.fndr.app.quality-lab/knowledge-worker"
            quality_lab.assert_isolated(lab, owner)

    def test_rejects_symlink_that_resolves_into_owner_vault(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            owner = root / "Library/Application Support/com.fndr.app"
            owner.mkdir(parents=True)
            alias = root / "lab-alias"
            alias.symlink_to(owner)
            with self.assertRaises(ValueError):
                quality_lab.assert_isolated(alias, owner)

    def test_rejects_unknown_synthetic_suite(self):
        with self.assertRaises(ValueError):
            quality_lab.profile_for(Path("/tmp/example"), "owner-vault")

    def test_refuses_scoring_or_start_while_native_app_or_dev_server_is_active(self):
        active_app = type("Result", (), {"returncode": 0})()
        with patch.object(quality_lab.subprocess, "run", return_value=active_app):
            with self.assertRaisesRegex(ValueError, "already running"):
                quality_lab.assert_no_running_fndr()

        inactive = type("Result", (), {"returncode": 1})()
        with patch.object(quality_lab.subprocess, "run", side_effect=[inactive, type("Result", (), {"returncode": 0})()]):
            with self.assertRaisesRegex(ValueError, "dev server"):
                quality_lab.assert_no_running_fndr()

    def test_fixture_log_metrics_capture_ocr_privacy_dedupe_and_production_hasher(self):
        fixture_rows = [
            {"id": "editor-01", "app_class": "editor", "expected_outcome": "store"},
            {"id": "privacy-secure-input", "app_class": "privacy_negative", "expected_outcome": "skip:sensitive_context"},
        ]
        log = """\
| Sequence | Expected keep | img_hash keeps | dHash+ABA keeps |
|---|---|---|---|
| typing | 10 | 1 | 10 |
editor-01            class=editor           cer=0.393 budget=0.443
test production_perceptual_hasher_accepts_novel_fixture_and_skips_repeats ... ok
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
fixture | expected admission | actual admission
privacy-secure-input | skip:sensitive_context | skip:sensitive_context
editor-01 | store | store
"""

        metrics = quality_lab.parse_capture_fixture_metrics(log, fixture_rows)

        self.assertEqual(metrics["ocr_cleanup"]["case_count"], 1)
        self.assertEqual(metrics["ocr_cleanup"]["within_budget_count"], 1)
        self.assertEqual(metrics["ocr_cleanup"]["worst_case"]["fixture_id"], "editor-01")
        self.assertEqual(metrics["dedupe_sequences"][0]["sequence"], "typing")
        self.assertEqual(metrics["dedupe_sequences"][0]["dhash_aba_kept"], 10)
        self.assertEqual(metrics["pre_frame_admission"]["case_count"], 2)
        self.assertEqual(metrics["pre_frame_admission"]["privacy_skip_count"], 1)
        self.assertTrue(metrics["pre_frame_admission"]["all_expected"])
        self.assertTrue(metrics["production_fixture_dedupe"]["passed"])
        self.assertEqual(metrics["test_summaries"][0]["passed"], 6)

    def test_fixture_metric_parser_rejects_incomplete_success_log(self):
        with self.assertRaisesRegex(ValueError, "missing OCR measurements"):
            quality_lab.parse_capture_fixture_metrics("test result: ok. 1 passed", [])


class QualityLabComparisonTests(unittest.TestCase):
    def write_run(self, root: Path, name: str, *, suite="knowledge-worker", case_set="knowledge-worker", dataset="same", recall=1.0, rank=1):
        report = root / name
        report.mkdir()
        (report / "run.json").write_text(json.dumps({
            "lane": "production_retrieval",
            "suite": suite,
            "exit_code": 0,
            "git_head": name,
            "source_fingerprint_sha256": name,
            "input_sha256": {"queries.json": dataset},
            "profile_states": {
                "before": {"sha256": name, "file_count": 20, "bytes": 500},
                "after": {
                    "sha256": f"{name}-after",
                    "file_count": 20 if name == "before" else 21,
                    "bytes": 500 if name == "before" else 620,
                },
            },
        }))
        (report / "current.json").write_text(json.dumps({
            "schema_version": 2,
            "case_set": case_set,
            "case_count": 1,
            "paths": {
                path: {
                    "recall_at_5": recall,
                    "mrr_at_10": float(1 / rank) if rank is not None else 0.0,
                    "latency_ms": {"p95": 100},
                    "no_match": {"no_strong_match": 1, "positive_without_strong_match": 0},
                }
                for path in ("search", "ask", "retrieve")
            },
            "queries": [{
                "query": "find the example",
                "kind": "paraphrase",
                "search_rank_at_10": rank,
                "ask_rank_at_10": rank,
                "retrieve_rank_at_10": rank,
            }],
        }))
        return report

    def test_compares_paired_inputs_and_lists_metric_and_case_deltas(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            before = self.write_run(root, "before", recall=1.0, rank=1)
            after = self.write_run(root, "after", recall=0.0, rank=None)
            report = quality_lab.compare_reports(before, after)
            self.assertIn("Recall@5", report)
            self.assertIn("paraphrase", report)
            self.assertIn("rank 1 -> miss", report)
            self.assertIn("Source fingerprint: differs", report)
            self.assertIn("Profile state: differs", report)

    def test_refuses_different_query_inputs(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            before = self.write_run(root, "before", dataset="first")
            after = self.write_run(root, "after", dataset="second")
            with self.assertRaisesRegex(ValueError, "inputs differ"):
                quality_lab.compare_reports(before, after)

    def test_refuses_different_suites(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            before = self.write_run(root, "before")
            after = self.write_run(root, "after", suite="office-pm", case_set="office-pm")
            with self.assertRaisesRegex(ValueError, "same suite"):
                quality_lab.compare_reports(before, after)


if __name__ == "__main__":
    unittest.main()
