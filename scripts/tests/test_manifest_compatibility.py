"""Unit and regression tests for scripts/check-manifest-compatibility.py (stdlib only)."""

from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from check_manifest_compatibility import (
    parse_manifest,
    parse_semver,
    satisfies_constraint,
    verify_compatibility,
)
MANIFEST_FILE = ROOT / "distribution/manifest.yaml"
SPEC_FILE = ROOT / "spec/system.yaml"


class ManifestCompatibilityTests(unittest.TestCase):
    def test_parse_semver(self):
        self.assertEqual(parse_semver("0.1.0"), (0, 1, 0))
        self.assertEqual(parse_semver("1.2.3-rc1"), (1, 2, 3))
        self.assertEqual(parse_semver("2.0.4+build123"), (2, 0, 4))
        with self.assertRaises(ValueError):
            parse_semver("invalid-semver")

    def test_satisfies_constraint_caret(self):
        # 0.1.x
        self.assertTrue(satisfies_constraint("0.1.0", "^0.1"))
        self.assertTrue(satisfies_constraint("0.1.4", "^0.1"))
        self.assertFalse(satisfies_constraint("0.2.0", "^0.1"))
        self.assertFalse(satisfies_constraint("1.0.0", "^0.1"))

        # 1.x.x
        self.assertTrue(satisfies_constraint("1.0.0", "^1.0"))
        self.assertTrue(satisfies_constraint("1.5.2", "^1.0"))
        self.assertFalse(satisfies_constraint("2.0.0", "^1.0"))
        self.assertFalse(satisfies_constraint("0.9.9", "^1.0"))

    def test_satisfies_constraint_tilde_and_gte(self):
        self.assertTrue(satisfies_constraint("0.1.2", "~0.1.0"))
        self.assertFalse(satisfies_constraint("0.2.0", "~0.1.0"))
        self.assertTrue(satisfies_constraint("1.0.0", ">=0.1.0"))
        self.assertFalse(satisfies_constraint("0.0.9", ">=0.1.0"))

    def test_real_manifest_and_spec_are_compatible(self):
        manifest_text = MANIFEST_FILE.read_text(encoding="utf-8")
        spec_text = SPEC_FILE.read_text(encoding="utf-8")
        is_valid, logs, errors = verify_compatibility(manifest_text, spec_text)
        self.assertTrue(is_valid, f"Real manifest should be compatible: {errors}")
        self.assertEqual(len(errors), 0)
        self.assertGreater(len(logs), 0)

    def test_compatible_version_bump_passes(self):
        manifest_text = """
productVersion: 0.1.1
components:
  contracts: 0.1.0
  auth: 0.1.1
  control: 0.1.0
  ingest: 0.1.0
  process: 0.1.0
  storeClickhouse: 0.1.0
  query: 0.1.0
  alerting: 0.1.0
  web: 0.1.0
"""
        spec_text = SPEC_FILE.read_text(encoding="utf-8")
        is_valid, logs, errors = verify_compatibility(manifest_text, spec_text)
        self.assertTrue(is_valid, f"Compatible bump should pass: {errors}")

    def test_incompatible_major_version_bump_fails(self):
        # auth is bumped to 0.2.0, while web, ingest, query, control, alerting require ^0.1
        manifest_text = """
productVersion: 0.2.0
components:
  contracts: 0.1.0
  auth: 0.2.0
  control: 0.1.0
  ingest: 0.1.0
  process: 0.1.0
  storeClickhouse: 0.1.0
  query: 0.1.0
  alerting: 0.1.0
  web: 0.1.0
"""
        spec_text = SPEC_FILE.read_text(encoding="utf-8")
        is_valid, logs, errors = verify_compatibility(manifest_text, spec_text)
        self.assertFalse(is_valid)
        self.assertGreater(len(errors), 0)
        err_msg = "\n".join(errors)
        self.assertIn("validate-credential@^0.1", err_msg)
        self.assertIn("observable-auth is pinned to version 0.2.0", err_msg)

    def test_missing_provider_fails(self):
        manifest_text = """
productVersion: 0.1.0
components:
  contracts: 0.1.0
  control: 0.1.0
  ingest: 0.1.0
  process: 0.1.0
  storeClickhouse: 0.1.0
  query: 0.1.0
  alerting: 0.1.0
  web: 0.1.0
"""
        spec_text = SPEC_FILE.read_text(encoding="utf-8")
        is_valid, logs, errors = verify_compatibility(manifest_text, spec_text)
        self.assertFalse(is_valid)
        self.assertTrue(
            any("provider observable-auth is missing" in err for err in errors)
        )

    def test_cli_execution_success(self):
        script = ROOT / "scripts/check_manifest_compatibility.py"
        res = subprocess.run(
            [sys.executable, str(script)],
            cwd=str(ROOT),
            capture_output=True,
            text=True,
        )
        self.assertEqual(res.returncode, 0, res.stderr)
        self.assertIn("verified across all components", res.stdout)

    def test_cli_execution_quiet_flag(self):
        script = ROOT / "scripts/check_manifest_compatibility.py"
        res = subprocess.run(
            [sys.executable, str(script), "--quiet"],
            cwd=str(ROOT),
            capture_output=True,
            text=True,
        )
        self.assertEqual(res.returncode, 0)
        self.assertEqual(res.stdout.strip(), "")


if __name__ == "__main__":
    unittest.main()
