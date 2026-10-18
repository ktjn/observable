"""Regression tests for the per-component changelog gate (stdlib only).

scripts/check-component-changelogs.sh enforces the docs/component-decomposition.md
Release Model requirement that every component in the distribution manifest has a
per-component CHANGELOG.md with an entry for its current version. These tests pin
the gate with fixtures so a future edit cannot silently accept a missing changelog.
"""

from pathlib import Path
import os
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]

# manifest key -> changelog path, mirroring scripts/check-component-changelogs.sh.
COMPONENTS = {
    "contracts": "contracts/CHANGELOG.md",
    "auth": "services/auth-service/CHANGELOG.md",
    "control": "services/admin-service/CHANGELOG.md",
    "ingest": "services/ingest-gateway/CHANGELOG.md",
    "process": "services/stream-processor/CHANGELOG.md",
    "storeClickhouse": "services/storage-writer/CHANGELOG.md",
    "query": "services/query-api/CHANGELOG.md",
    "alerting": "services/alert-evaluator/CHANGELOG.md",
    "web": "apps/frontend/CHANGELOG.md",
}


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


class ComponentChangelogTests(unittest.TestCase):
    def _fixture(self, directory, *, version="0.1.0"):
        repo = Path(directory)
        (repo / "scripts").mkdir()
        shutil.copy(
            ROOT / "scripts/check-component-changelogs.sh", repo / "scripts"
        )
        manifest = repo / "distribution/manifest.yaml"
        manifest.parent.mkdir(parents=True)
        lines = ["productVersion: 0.1.0", "components:"]
        for key in COMPONENTS:
            lines.append(f"  {key}: 0.1.0")
        manifest.write_text("\n".join(lines) + "\n")
        for rel in COMPONENTS.values():
            path = repo / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(f"# Changelog\n\n## [{version}] - 2026-07-18\n")
        return repo

    def _run(self, repo: Path) -> subprocess.CompletedProcess:
        return subprocess.run(
            [bash_path(), "scripts/check-component-changelogs.sh"],
            cwd=repo,
            capture_output=True,
            text=True,
        )

    def test_every_component_has_a_changelog_entry(self):
        with tempfile.TemporaryDirectory(prefix="observable-changelog-") as directory:
            result = self._run(self._fixture(directory))
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("component changelogs present", result.stdout)

    def test_missing_changelog_fails(self):
        with tempfile.TemporaryDirectory(prefix="observable-changelog-") as directory:
            repo = self._fixture(directory)
            (repo / COMPONENTS["query"]).unlink()
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("missing changelog for component 'query'", result.stderr)

    def test_missing_version_entry_fails(self):
        with tempfile.TemporaryDirectory(prefix="observable-changelog-") as directory:
            repo = self._fixture(directory)
            (repo / COMPONENTS["auth"]).write_text(
                "# Changelog\n\n## [0.0.9] - 2026-07-18\n"
            )
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("no changelog entry for version 0.1.0", result.stderr)

    def test_component_missing_from_manifest_fails(self):
        with tempfile.TemporaryDirectory(prefix="observable-changelog-") as directory:
            repo = self._fixture(directory)
            (repo / "distribution/manifest.yaml").write_text(
                "productVersion: 0.1.0\ncomponents:\n  auth: 0.1.0\n"
            )
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("component 'contracts' missing from", result.stderr)

    def test_repository_changelogs_are_valid(self):
        result = subprocess.run(
            [bash_path(), "scripts/check-component-changelogs.sh"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
