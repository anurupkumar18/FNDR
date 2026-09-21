import io
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import callers as c


def make(root, files):
    for rel, body in files.items():
        p = Path(root) / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(body)


class ClassifyTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name
        make(self.root, {
            "memory/validate.rs": "pub fn decide_memory(x: u8) -> u8 { x }\npub fn helper() { decide_memory(1); }\n",
            "ipc/commands/debug.rs": "fn dbg() { crate::memory::validate::decide_memory(2); distill(3); }\n",
            "capture/mod.rs": "fn run() { compose(1); compose(2); }\n#[cfg(test)]\nmod tests { fn t() { only_in_tests(1); } }\n",
            "capture/other.rs": "pub fn compose(x: u8) {}\npub fn only_in_tests(x: u8) {}\npub fn unused(x: u8) {}\n",
            "memory/distill.rs": "pub fn distill(x: u8) {}\n",
        })

    def tearDown(self):
        self.tmp.cleanup()

    def cls(self, symbol):
        d, calls = c.scan(self.root, symbol, "debug")
        return c.classify(d, calls, "debug")

    def test_live_when_called_from_a_non_debug_file(self):
        self.assertEqual(self.cls("compose"), "live")

    def test_debug_only_when_every_outside_caller_is_a_debug_file(self):
        self.assertEqual(self.cls("distill"), "debug-only")

    def test_self_only_when_only_the_defining_file_calls_it(self):
        make(self.root, {"solo.rs": "fn a() { b(1); }\nfn b(x: u8) {}\n"})
        self.assertEqual(self.cls("b"), "self-only")

    def test_dead_when_never_called_and_test_calls_do_not_count(self):
        self.assertEqual(self.cls("unused"), "dead")
        self.assertEqual(self.cls("only_in_tests"), "dead")

    def test_decide_memory_is_both_self_called_and_debug_called_so_it_is_debug_only(self):
        self.assertEqual(self.cls("decide_memory"), "debug-only")

    def test_cli_prints_a_markdown_table(self):
        buf = io.StringIO()
        with redirect_stdout(buf):
            c.main([self.root, "compose", "unused"])
        out = buf.getvalue()
        self.assertIn("| compose | live | 2 |", out)
        self.assertIn("| unused | dead | 0 |", out)


if __name__ == "__main__":
    unittest.main()
