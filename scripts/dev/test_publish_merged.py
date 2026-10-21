"""Scenario tests for publish-merged.sh on throwaway repositories.

Two bare repositories stand in for GitLab (origin, with both push URLs) and
GitHub (gh). The check is a no-op Makefile target, so each case runs in a few
seconds:
  python3 -m unittest scripts/dev/test_publish_merged.py
"""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).parent / "publish-merged.sh"


def git(*args, cwd):
    return subprocess.run(["git", *args], cwd=cwd, check=True, capture_output=True, text=True).stdout.strip()


class PublishMergedTest(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.env = {**os.environ, "GIT_AUTHOR_NAME": "t", "GIT_AUTHOR_EMAIL": "t@t",
                    "GIT_COMMITTER_NAME": "t", "GIT_COMMITTER_EMAIL": "t@t", "GIT_CONFIG_GLOBAL": os.devnull,
                    "TMPDIR": str(self.root) + "/"}
        os.environ.update({k: v for k, v in self.env.items() if k.startswith("GIT_")})
        for name in ("gitlab.git", "github.git"):
            git("init", "-q", "--bare", "-b", "main", name, cwd=self.root)
        seed = self.root / "seed"
        git("init", "-q", "-b", "main", "seed", cwd=self.root)
        (seed / "scripts/dev").mkdir(parents=True)
        shutil.copy(SCRIPT, seed / "scripts/dev/publish-merged.sh")
        (seed / "Makefile").write_text("scripts-test:\n\t@true\n")
        (seed / "a.txt").write_text("base\n")
        git("add", "-A", cwd=seed)
        git("commit", "-qm", "base", cwd=seed)
        for name in ("gitlab.git", "github.git"):
            git("push", "-q", str(self.root / name), "main", cwd=seed)

    def tearDown(self):
        shutil.rmtree(self.root, ignore_errors=True)

    def head(self, name):
        return git("--git-dir", str(self.root / name), "rev-parse", "main", cwd=self.root)

    def subjects(self, name):
        return git("--git-dir", str(self.root / name), "log", "--format=%s", "main", cwd=self.root).splitlines()

    def push_alone(self, name, subject):
        clone = self.root / f"other-{subject}"
        git("clone", "-q", str(self.root / name), str(clone), cwd=self.root)
        (clone / f"{subject}.txt").write_text(subject)
        git("add", "-A", cwd=clone)
        git("commit", "-qm", subject, cwd=clone)
        git("push", "-q", "origin", "main", cwd=clone)

    def local_commit(self, subject):
        work = self.root / "work"
        git("clone", "-q", str(self.root / "gitlab.git"), str(work), cwd=self.root)
        git("remote", "set-url", "--add", "--push", "origin", str(self.root / "gitlab.git"), cwd=work)
        git("remote", "set-url", "--add", "--push", "origin", str(self.root / "github.git"), cwd=work)
        git("remote", "add", "gh", str(self.root / "github.git"), cwd=work)
        (work / f"{subject}.txt").write_text(subject)
        git("add", "-A", cwd=work)
        git("commit", "-qm", subject, cwd=work)
        return work

    def publish(self, work):
        return subprocess.run(["scripts/dev/publish-merged.sh", "--scripts", "test"], cwd=work,
                              capture_output=True, text=True, env=self.env)

    def test_equal_remotes_publish_to_both(self):
        result = self.publish(self.local_commit("docs"))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.head("gitlab.git"), self.head("github.git"))
        self.assertIn("docs", self.subjects("github.git"))

    def test_a_push_to_github_alone_reaches_both(self):
        self.push_alone("github.git", "kunj")
        result = self.publish(self.local_commit("docs"))
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.head("gitlab.git"), self.head("github.git"))
        self.assertIn("kunj", self.subjects("gitlab.git"))
        self.assertIn("docs", self.subjects("github.git"))

    def test_diverged_remotes_stop_before_any_push(self):
        self.push_alone("github.git", "github-only")
        self.push_alone("gitlab.git", "gitlab-only")
        before = (self.head("gitlab.git"), self.head("github.git"))
        result = self.publish(self.local_commit("docs"))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("diverged", result.stderr)
        self.assertEqual((self.head("gitlab.git"), self.head("github.git")), before)


if __name__ == "__main__":
    unittest.main()
