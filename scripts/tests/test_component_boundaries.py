"""Regression tests for the component-boundary lint (stdlib only).

The lint in scripts/check-component-boundaries.sh silently passed on every tree
when it was first added: file paths from `find .` were matched against owner
patterns written without the leading "./", the exclude predicates AND-ed
together so `find` matched nothing, and the violation counter aborted the
script under `set -e`. These tests pin the detector's actual behavior so a
future regression cannot make it a no-op again.
"""

from pathlib import Path
import os
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


def bash_path() -> str:
    bash = shutil.which("bash")
    if os.name == "nt":
        git = shutil.which("git")
        if git:
            git_bash = Path(git).parent.parent / "bin/bash.exe"
            if git_bash.exists():
                bash = str(git_bash)
    if not bash:
        raise AssertionError("Bash is required for shell gate regression tests")
    return bash


class ComponentBoundaryLintTests(unittest.TestCase):
    def _fixture(self, directory: str) -> Path:
        """Copy the lint (and an allowlist) into an isolated mini-repo."""
        repo = Path(directory)
        (repo / "scripts").mkdir()
        (repo / "services/query-api/src").mkdir(parents=True)
        shutil.copy(
            ROOT / "scripts/check-component-boundaries.sh", repo / "scripts"
        )
        return repo

    def _run(self, repo: Path) -> subprocess.CompletedProcess:
        return subprocess.run(
            [bash_path(), "scripts/check-component-boundaries.sh"],
            cwd=repo,
            capture_output=True,
            text=True,
        )

    def test_detects_cross_component_source_import(self):
        with tempfile.TemporaryDirectory(prefix="observable-boundary-") as directory:
            repo = self._fixture(directory)
            (repo / "services/query-api/src/lib.rs").write_text(
                "use services::admin_service::Thing;\n"
            )
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("services/query-api/src/lib.rs", result.stdout)

    def test_same_component_reference_is_allowed(self):
        with tempfile.TemporaryDirectory(prefix="observable-boundary-") as directory:
            repo = self._fixture(directory)
            (repo / "services/query-api/src/lib.rs").write_text(
                "use query_api::planner::QueryPlanner;\n"
            )
            result = self._run(repo)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_documented_coupling_is_accepted(self):
        with tempfile.TemporaryDirectory(prefix="observable-boundary-") as directory:
            repo = self._fixture(directory)
            (repo / "services/query-api/src/lib.rs").write_text(
                "use services::admin_service::Thing;\n"
            )
            (repo / "scripts/component-boundary-allowlist.txt").write_text(
                "services/query-api/src/lib.rs control transitional fixture coupling\n"
            )
            result = self._run(repo)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("transitional coupling", result.stdout)

    def test_allowlist_entry_requires_a_reason(self):
        with tempfile.TemporaryDirectory(prefix="observable-boundary-") as directory:
            repo = self._fixture(directory)
            (repo / "services/query-api/src/lib.rs").write_text(
                "use services::admin_service::Thing;\n"
            )
            (repo / "scripts/component-boundary-allowlist.txt").write_text(
                "services/query-api/src/lib.rs control\n"
            )
            result = self._run(repo)
            self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
            self.assertIn("reason required", result.stderr)

    def test_repository_tree_is_clean(self):
        result = subprocess.run(
            [bash_path(), "scripts/check-component-boundaries.sh"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
