import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import summarize_metrics as sm


def row(ts_ms, cpu, rss_mb, evaluated, stored, skipped, skips):
    return {
        "snapshot": {
            "generated_at_ms": ts_ms,
            "aggregates": {
                "capture.ocr_ms": {"n": 10, "avg_ms": 41.5, "p50_ms": 40, "p95_ms": 88, "max_ms": 120},
                "capture.flush_ms": {"n": 3, "avg_ms": 5.0, "p50_ms": 5, "p95_ms": 6, "max_ms": 6},
                "mem.embed_ms": {"n": 4, "avg_ms": 90.0, "p50_ms": 88, "p95_ms": 140, "max_ms": 150},
                "embedding.other_ms": {"n": 1, "avg_ms": 1.0, "p50_ms": 1, "p95_ms": 1, "max_ms": 1},
            },
            "counters": {"mem.outcome.new": 5, "mem.outcome.merged_persisted": 3, "other.counter": 9},
            "system": {
                "process_cpu": {"cpu_percent": cpu},
                "process_memory": {"rss_bytes": rss_mb * 1024 * 1024, "phys_footprint_bytes": rss_mb * 1024 * 1024},
                "process_energy": {"label": "low"},
            },
        },
        "skips": skips,
        "totals": {"evaluated": evaluated, "stored": stored, "skipped": skipped},
    }


class SummarizeTests(unittest.TestCase):
    def test_percentile_is_nearest_rank(self):
        self.assertEqual(sm.percentile(list(range(1, 101)), 0.95), 95)
        self.assertEqual(sm.percentile(list(range(1, 101)), 0.5), 50)
        self.assertEqual(sm.percentile([], 0.95), 0.0)

    def test_report_has_stage_table_cpu_and_no_unexplained_drops(self):
        rows = [
            row(0, 2.0, 400, 0, 0, 0, {"blocklist": 0}),
            row(600000, 4.0, 430, 20, 8, 12, {"blocklist": 2, "perceptual_dup": 10}),
        ]
        text = sm.summarize(rows, "Apple M1, 8 GB", "release")
        self.assertIn("| capture.ocr_ms | 10 | 41.5 | 40 | 88 | 120 |", text)
        self.assertIn("| mem.embed_ms | 4 | 90.0 | 88 | 140 | 150 |", text)
        self.assertIn("| new | 5 |", text)
        self.assertIn("| merged_persisted | 3 |", text)
        self.assertNotIn("other.counter", text)
        self.assertNotIn("embedding.other_ms", text)
        self.assertIn("CPU: avg 3.00 percent", text)
        self.assertIn("Duration: 10.0 minutes", text)
        self.assertIn("Unexplained drops: 0", text)
        self.assertIn("| perceptual_dup | 10 | 50.0 percent |", text)
        self.assertNotIn("| blocklist | 0 |", text)

    def test_unexplained_drops_are_surfaced(self):
        rows = [row(0, 1.0, 400, 10, 2, 3, {}), row(60000, 1.0, 400, 10, 2, 3, {})]
        self.assertIn("Unexplained drops: 5", sm.summarize(rows, "m", "release"))

    def test_cli_roundtrip(self):
        with tempfile.TemporaryDirectory() as d:
            src = Path(d) / "m.ndjson"
            rows = [row(0, 1.0, 400, 1, 1, 0, {}), row(60000, 1.0, 410, 2, 2, 0, {})]
            src.write_text("\n".join(json.dumps(r) for r in rows) + "\n")
            out = Path(d) / "out" / "report.md"
            self.assertEqual(sm.main([str(src), "--out", str(out), "--machine", "M1"]), 0)
            self.assertTrue(out.read_text().startswith("# Capture pipeline baseline"))

    def test_empty_input_is_an_error(self):
        with self.assertRaises(ValueError):
            sm.summarize([], "m", "release")


if __name__ == "__main__":
    unittest.main()
