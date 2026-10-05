"""Offline boundary checks for the pinned MiniLM bootstrap."""

import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class MiniLmBootstrapTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        bootstrap = self.root / "scripts/bootstrap"
        bootstrap.mkdir(parents=True)
        self.script = bootstrap / "download-minilm.sh"
        shutil.copyfile(Path(__file__).with_name("download-minilm.sh"), self.script)
        self.target = self.root / "installed models"
        self.target.mkdir()
        self.assets = self.root / "assets"
        self.assets.mkdir()
        self.files = {"all-MiniLM-L6-v2.onnx": b"pinned weights", "tokenizer.json": b"pinned tokenizer"}
        constants = []
        for prefix, (name, content) in zip(["EMBEDDING_MODEL", "EMBEDDING_TOKENIZER"], self.files.items()):
            asset = self.assets / name
            asset.write_bytes(content)
            for suffix, value in [("FILENAME", name), ("DOWNLOAD_URL", asset.as_uri()),
                                  ("SHA256", hashlib.sha256(content).hexdigest())]:
                constants.append(f'pub const {prefix}_{suffix}: &str =\n    "{value}";')
        config = self.root / "src-tauri/src/inference/model_config.rs"
        config.parent.mkdir(parents=True)
        config.write_text("\n".join(constants))
    def run_bootstrap(self):
        return subprocess.run(
            ["bash", str(self.script), str(self.target)], capture_output=True, text=True,
        )

    def test_existing_unapproved_tokenizer_is_preserved(self):
        tokenizer = self.target / "tokenizer.json"
        tokenizer.write_bytes(b"existing tokenizer configuration")
        result = self.run_bootstrap()
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(tokenizer.read_bytes(), b"existing tokenizer configuration")
        self.assertFalse((self.target / "all-MiniLM-L6-v2.onnx").exists())

    def test_missing_assets_use_authoritative_urls_and_digests(self):
        result = self.run_bootstrap()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        for name, content in self.files.items():
            self.assertEqual((self.target / name).read_bytes(), content)

    def test_corrupt_download_promotes_neither_asset(self):
        (self.assets / "tokenizer.json").write_bytes(b"unexpected upstream bytes")
        result = self.run_bootstrap()
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(list(self.target.iterdir()), [])

    def test_valid_install_is_reused_without_download_or_rewrite(self):
        for name, content in self.files.items():
            path = self.target / name
            path.write_bytes(content)
            os.utime(path, ns=(1_000_000_000, 1_000_000_000))
        shutil.rmtree(self.assets)
        result = self.run_bootstrap()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        for name in self.files:
            self.assertEqual((self.target / name).stat().st_mtime_ns, 1_000_000_000)


if __name__ == "__main__":
    unittest.main()
