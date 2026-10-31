"""Regression tests for the ClickHouse schema-compatibility gate (stdlib only).

scripts/check-clickhouse-schema-compat.sh enforces the Phase 3 / Phase 6 contract
that observable-query's and observable-alerting's tested storage-schema ranges
contain the store's declared current schema version
(migrations/clickhouse/SCHEMA_VERSION, which must match the highest migration).
These tests pin the gate's behavior with fixtures so a future edit cannot
silently make it accept an incompatible schema.
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


class ClickHouseSchemaCompatTests(unittest.TestCase):
    def _fixture(
        self,
        directory,
        *,
        version=4,
        migrations=4,
        minimum=4,
        maximum=4,
        alerting_min=None,
        alerting_max=None,
        support=True,
        alerting_support=True,
    ):
        repo = Path(directory)
        (repo / "scripts").mkdir()
        shutil.copy(
            ROOT / "scripts/check-clickhouse-schema-compat.sh", repo / "scripts"
        )
        migrations_dir = repo / "migrations/clickhouse"
        migrations_dir.mkdir(parents=True)
        for number in range(1, migrations + 1):
            (migrations_dir / f"{number:03d}_create_table_{number}.sql").write_text(
                "-- migration\n"
            )
        (migrations_dir / "SCHEMA_VERSION").write_text(f"{version}\n")
        if support:
            query_dir = repo / "services/query-api"
            query_dir.mkdir(parents=True)
            (query_dir / "clickhouse-schema-support.yaml").write_text(
                f"min: {minimum}\nmax: {maximum}\n"
            )
        if alerting_support:
            alert_dir = repo / "services/alert-evaluator"
            alert_dir.mkdir(parents=True)
            amin = minimum if alerting_min is None else alerting_min
            amax = maximum if alerting_max is None else alerting_max
            (alert_dir / "clickhouse-schema-support.yaml").write_text(
                f"min: {amin}\nmax: {amax}\n"
            )
        return repo

    def _run(self, repo: Path) -> subprocess.CompletedProcess:
        return subprocess.run(
            [bash_path(), "scripts/check-clickhouse-schema-compat.sh"],
            cwd=repo,
            capture_output=True,
            text=True,
        )

    def test_current_schema_inside_range_passes(self):
        with tempfile.TemporaryDirectory(prefix="observable-ch-compat-") as directory:
            result = self._run(self._fixture(directory))
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("within observable-query's tested range", result.stdout)
            self.assertIn("within observable-alerting's tested range", result.stdout)

    def test_store_ahead_of_query_range_fails(self):
        with tempfile.TemporaryDirectory(prefix="observable-ch-compat-") as directory:
            repo = self._fixture(directory, version=5, migrations=5, maximum=4)
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("outside observable-query's tested range", result.stderr)

    def test_store_ahead_of_alerting_range_fails(self):
        with tempfile.TemporaryDirectory(prefix="observable-ch-compat-") as directory:
            repo = self._fixture(
                directory, version=5, migrations=5, maximum=5, alerting_max=4
            )
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("outside observable-alerting's tested range", result.stderr)

    def test_store_behind_query_minimum_fails(self):
        with tempfile.TemporaryDirectory(prefix="observable-ch-compat-") as directory:
            repo = self._fixture(directory, version=3, migrations=3, minimum=4)
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("outside observable-query's tested range", result.stderr)

    def test_schema_version_must_match_highest_migration(self):
        with tempfile.TemporaryDirectory(prefix="observable-ch-compat-") as directory:
            repo = self._fixture(directory, version=3, migrations=4)
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("does not match the highest ClickHouse migration", result.stderr)

    def test_non_integer_schema_version_fails(self):
        with tempfile.TemporaryDirectory(prefix="observable-ch-compat-") as directory:
            repo = self._fixture(directory)
            (repo / "migrations/clickhouse/SCHEMA_VERSION").write_text("four\n")
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("not a non-negative integer", result.stderr)

    def test_missing_query_support_file_fails(self):
        with tempfile.TemporaryDirectory(prefix="observable-ch-compat-") as directory:
            repo = self._fixture(directory, support=False)
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("query schema-support file not found", result.stderr)

    def test_missing_alerting_support_file_fails(self):
        with tempfile.TemporaryDirectory(prefix="observable-ch-compat-") as directory:
            repo = self._fixture(directory, alerting_support=False)
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("alerting schema-support file not found", result.stderr)

    def test_empty_range_fails(self):
        with tempfile.TemporaryDirectory(prefix="observable-ch-compat-") as directory:
            repo = self._fixture(directory, minimum=5, maximum=4)
            result = self._run(repo)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn("range is empty", result.stderr)

    def test_repository_contract_holds(self):
        result = subprocess.run(
            [bash_path(), "scripts/check-clickhouse-schema-compat.sh"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
