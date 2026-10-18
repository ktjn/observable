"""Regression tests for the ClickHouse migration-immutability gate (stdlib only).

scripts/check-clickhouse-migration-immutability.sh records a sha256 per applied
migration and fails when one is edited or removed, or when a migration is present
but not recorded. Editing a shipped migration silently changes the schema a fresh
deployment gets, so these tests pin the gate.
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


class MigrationImmutabilityTests(unittest.TestCase):
    def _fixture(self, directory, *, migrations=("001_create_spans.sql", "002_create_logs.sql")):
        repo = Path(directory)
        (repo / "scripts").mkdir()
        shutil.copy(
            ROOT / "scripts/check-clickhouse-migration-immutability.sh", repo / "scripts"
        )
        migrations_dir = repo / "migrations/clickhouse"
        migrations_dir.mkdir(parents=True)
        for name in migrations:
            (migrations_dir / name).write_text(f"CREATE TABLE IF NOT EXISTS t_{name}\n")
        return repo

    def _run(self, repo: Path, *args) -> subprocess.CompletedProcess:
        return subprocess.run(
            [bash_path(), "scripts/check-clickhouse-migration-immutability.sh", *args],
            cwd=repo,
            capture_output=True,
            text=True,
        )

    def test_update_then_verify_passes(self):
        with tempfile.TemporaryDirectory(prefix="observable-mig-imm-") as directory:
            repo = self._fixture(directory)
            updated = self._run(repo, "--update")
            self.assertEqual(updated.returncode, 0, updated.stdout + updated.stderr)
            self.assertIn("recorded 2 ClickHouse migration checksum(s)", updated.stdout)
            verified = self._run(repo)
            self.assertEqual(verified.returncode, 0, verified.stdout + verified.stderr)

    def test_edited_migration_fails(self):
        with tempfile.TemporaryDirectory(prefix="observable-mig-imm-") as directory:
            repo = self._fixture(directory)
            self._run(repo, "--update")
            path = repo / "migrations/clickhouse/002_create_logs.sql"
            path.write_text(path.read_text() + "-- tampered\n")
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("was edited", result.stderr)

    def test_removed_migration_fails(self):
        with tempfile.TemporaryDirectory(prefix="observable-mig-imm-") as directory:
            repo = self._fixture(directory)
            self._run(repo, "--update")
            (repo / "migrations/clickhouse/002_create_logs.sql").unlink()
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("is missing", result.stderr)

    def test_unrecorded_migration_fails(self):
        with tempfile.TemporaryDirectory(prefix="observable-mig-imm-") as directory:
            repo = self._fixture(directory)
            self._run(repo, "--update")
            (repo / "migrations/clickhouse/003_create_metrics.sql").write_text(
                "CREATE TABLE IF NOT EXISTS t_003\n"
            )
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("is not recorded", result.stderr)

    def test_missing_manifest_fails(self):
        with tempfile.TemporaryDirectory(prefix="observable-mig-imm-") as directory:
            repo = self._fixture(directory)
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("MIGRATION_CHECKSUMS is missing", result.stderr)

    def test_repository_migrations_are_recorded(self):
        result = subprocess.run(
            [bash_path(), "scripts/check-clickhouse-migration-immutability.sh"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
