"""Regression tests for the pinned-install override (stdlib only).

scripts/render-pinned-compose.sh renders distribution/compose.pinned.yaml from
distribution/manifest.yaml, and `--check` fails when the committed override is
out of date. These tests pin the render/check behavior so a component version
bump that forgets the override is caught by the local gate.
"""

from pathlib import Path
import os
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
KEYS = ["contracts", "auth", "control", "ingest", "process", "storeClickhouse",
        "query", "alerting", "web"]


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


class PinnedInstallTests(unittest.TestCase):
    def _fixture(self, directory, *, version="0.1.0"):
        repo = Path(directory)
        (repo / "scripts").mkdir()
        shutil.copy(ROOT / "scripts/render-pinned-compose.sh", repo / "scripts")
        manifest = repo / "distribution/manifest.yaml"
        manifest.parent.mkdir(parents=True)
        lines = ["productVersion: 0.1.0", "components:"]
        lines += [f"  {key}: {version}" for key in KEYS]
        manifest.write_text("\n".join(lines) + "\n")
        return repo

    def _run(self, repo: Path, *args) -> subprocess.CompletedProcess:
        return subprocess.run(
            [bash_path(), "scripts/render-pinned-compose.sh", *args],
            cwd=repo,
            capture_output=True,
            text=True,
        )

    def test_render_then_check_passes(self):
        with tempfile.TemporaryDirectory(prefix="observable-pinned-") as directory:
            repo = self._fixture(directory)
            rendered = self._run(repo)
            self.assertEqual(rendered.returncode, 0, rendered.stdout + rendered.stderr)
            override = (repo / "distribution/compose.pinned.yaml").read_text()
            self.assertIn("auth-service:", override)
            self.assertIn("image: observable-auth:0.1.0", override)
            self.assertIn("frontend:", override)
            self.assertIn("image: observable-web:0.1.0", override)
            self.assertNotIn("contracts:", override)

            checked = self._run(repo, "--check")
            self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)

    def test_check_fails_when_override_is_stale(self):
        with tempfile.TemporaryDirectory(prefix="observable-pinned-") as directory:
            repo = self._fixture(directory)
            self._run(repo)
            override = repo / "distribution/compose.pinned.yaml"
            override.write_text(override.read_text().replace("0.1.0", "0.0.9"))
            result = self._run(repo, "--check")
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("out of date", result.stderr)

    def test_check_fails_when_override_is_missing(self):
        with tempfile.TemporaryDirectory(prefix="observable-pinned-") as directory:
            repo = self._fixture(directory)
            result = self._run(repo, "--check")
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)

    def test_check_fails_when_manifest_version_changes(self):
        with tempfile.TemporaryDirectory(prefix="observable-pinned-") as directory:
            repo = self._fixture(directory)
            self._run(repo)
            manifest = repo / "distribution/manifest.yaml"
            manifest.write_text(manifest.read_text().replace("  query: 0.1.0", "  query: 0.2.0"))
            result = self._run(repo, "--check")
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("observable-query:0.2.0", result.stderr)

    def test_repository_override_matches_the_manifest(self):
        result = subprocess.run(
            [bash_path(), "scripts/render-pinned-compose.sh", "--check"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
