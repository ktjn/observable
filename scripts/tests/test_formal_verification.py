"""Consistency tests for the formal-verification tooling (stdlib only).

scripts/run-formal-verification.sh pins the TLA+ tool by sha256 and runs the
committed models; formal/tla/README.md documents the same pin and every
invariant/property the configuration checks. These tests keep the pin and the
documentation from drifting apart. They do not run TLC (that is the
"Formal verification (TLC)" step of scripts/local-ci.sh).
"""

from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[2]
RUNNER = ROOT / "scripts/run-formal-verification.sh"
MODEL_DIR = ROOT / "formal/tla"
README = MODEL_DIR / "README.md"


class FormalVerificationConsistencyTests(unittest.TestCase):
    def test_runner_and_readme_agree_on_the_pinned_digest(self):
        runner = RUNNER.read_text()
        readme = README.read_text()
        sha = re.search(r'TLA_SHA256="([0-9a-f]{64})"', runner)
        self.assertIsNotNone(sha, "runner must pin TLA_SHA256")
        self.assertIn(sha[1], readme, "README must document the pinned sha256")

    def test_runner_and_readme_agree_on_the_tool_version(self):
        runner = RUNNER.read_text()
        readme = README.read_text()
        version = re.search(r'TLA_VERSION="(v[0-9.]+)"', runner)
        self.assertIsNotNone(version, "runner must pin TLA_VERSION")
        self.assertIn(version[1], readme, "README must document the pinned version")

    def test_runner_enforces_a_wall_clock_budget(self):
        runner = RUNNER.read_text()
        self.assertIn("TLC_TIMEOUT", runner)
        self.assertIn("timeout", runner)
        self.assertIn("${name}.log", runner)

    def test_models_and_configs_exist(self):
        self.assertTrue((MODEL_DIR / "TelemetryDelivery.tla").is_file())
        self.assertTrue((MODEL_DIR / "TelemetryDelivery.cfg").is_file())
        self.assertTrue((MODEL_DIR / "ComponentCompatibility.tla").is_file())
        self.assertTrue((MODEL_DIR / "ComponentCompatibility.cfg").is_file())

    def test_every_configured_invariant_is_documented(self):
        readme = README.read_text()
        for cfg in sorted(MODEL_DIR.glob("*.cfg")):
            names = []
            section = None
            for line in cfg.read_text().splitlines():
                stripped = line.strip()
                if stripped in ("INVARIANTS", "PROPERTIES"):
                    section = stripped
                    continue
                if section and stripped:
                    names.append(stripped)
                elif not stripped:
                    section = None
            self.assertTrue(names, f"{cfg.name} declares no invariants/properties")
            self.assertGreaterEqual(
                len(names), 2, f"{cfg.name} declares fewer invariants than expected: {names}"
            )
            for name in names:
                with self.subTest(model=cfg.name, name=name):
                    self.assertIn(name, readme, f"{name} is not documented in formal/tla/README.md")


if __name__ == "__main__":
    unittest.main()
