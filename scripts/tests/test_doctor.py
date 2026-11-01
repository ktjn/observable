"""Regression tests for the platform doctor diagnostic script (stdlib only)."""

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


class DoctorScriptTests(unittest.TestCase):
    def test_help_prints_usage(self):
        res = subprocess.run(
            [bash_path(), "scripts/doctor.sh", "--help"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertIn("Usage: scripts/doctor.sh", res.stdout)
        self.assertIn("--preflight", res.stdout)
        self.assertIn("--verbose", res.stdout)
        self.assertIn("--timeout", res.stdout)

    def test_timeout_requires_value(self):
        res = subprocess.run(
            [bash_path(), "scripts/doctor.sh", "--timeout"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertEqual(res.returncode, 1)
        self.assertIn("FAIL: --timeout requires a value", res.stderr)

    def test_unknown_flag_fails(self):
        res = subprocess.run(
            [bash_path(), "scripts/doctor.sh", "--invalid-flag-xyz"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        self.assertEqual(res.returncode, 1)
        self.assertIn("FAIL: unknown argument", res.stderr)

    def test_preflight_checks_ports_and_tools(self):
        res = subprocess.run(
            [bash_path(), "scripts/doctor.sh", "--preflight"],
            cwd=ROOT,
            capture_output=True,
            text=True,
        )
        # Should exit 0 on clean or warnings
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertIn("Checking Tooling Prerequisites", res.stdout)
        self.assertIn("Checking Host Resources", res.stdout)
        self.assertIn("Preflight Port Availability Check", res.stdout)
        self.assertIn("Preflight Summary", res.stdout)


if __name__ == "__main__":
    unittest.main()
