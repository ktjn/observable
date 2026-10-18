"""Guard the Helm lint gate against rewriting committed Chart.lock files.

`helm dependency update` re-resolves dependencies from Chart.yaml and rewrites
the tracked Chart.lock (including a fresh `generated` timestamp) on every run,
which dirties the working tree and can silently move a chart to a version other
than the one recorded in the lock. The gate must build from the committed lock
instead, so the lock stays the reproducible source of truth.
"""

from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[2]


class HelmLintDeterminismTests(unittest.TestCase):
    def test_gate_builds_dependencies_from_the_committed_lock(self):
        script = (ROOT / "scripts/helm-lint.sh").read_text()
        self.assertNotIn(
            "helm dependency update",
            script,
            "helm-lint must not run `helm dependency update` (it rewrites Chart.lock)",
        )
        self.assertIn("helm dependency build", script)

    def test_committed_locks_match_their_chart_versions(self):
        for chart_yaml in sorted((ROOT / "charts").glob("*/Chart.yaml")):
            lock = chart_yaml.with_name("Chart.lock")
            if not lock.exists():
                continue
            chart = chart_yaml.read_text()
            versions = re.findall(r"^\s*version:\s*\"?([0-9][0-9.]*)\"?\s*$", lock.read_text(), re.MULTILINE)
            self.assertTrue(versions, f"{lock} has no dependency versions")
            for version in versions:
                with self.subTest(chart=chart_yaml.parent.name, version=version):
                    self.assertIn(
                        version,
                        chart,
                        f"{lock} pins {version} but {chart_yaml.name} does not declare it",
                    )


if __name__ == "__main__":
    unittest.main()
