"""Regression tests for the backup-restore drill script (stdlib only)."""

from pathlib import Path
import os
import shutil
import subprocess
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


class BackupRestoreDrillTests(unittest.TestCase):
    def test_help_prints_usage(self):
        res = subprocess.run(
            [bash_path(), "scripts/backup-restore-drill.sh", "--help"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertIn("Usage: scripts/backup-restore-drill.sh", res.stdout)
        self.assertIn("--with-clickhouse", res.stdout)
        self.assertIn("alert_rules", res.stdout)

    def test_unknown_argument_fails(self):
        res = subprocess.run(
            [bash_path(), "scripts/backup-restore-drill.sh", "--unknown-flag"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertEqual(res.returncode, 1)
        self.assertIn("FAIL: unknown argument", res.stderr)


if __name__ == "__main__":
    unittest.main()
