"""Guard per-component versioning in the distribution manifest (stdlib only)."""

from pathlib import Path
import os
import re
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
COMPONENT_SOURCES = [
    "services/auth-service/Cargo.toml",
    "services/admin-service/Cargo.toml",
    "services/ingest-gateway/Cargo.toml",
    "services/stream-processor/Cargo.toml",
    "services/storage-writer/Cargo.toml",
    "services/query-api/Cargo.toml",
    "services/alert-evaluator/Cargo.toml",
    "apps/frontend/package.json",
]


def find_bash():
    bash = shutil.which("bash")
    if os.name == "nt":
        git = Path(shutil.which("git"))
        git_bash = git.parent.parent / "bin/bash.exe"
        if git_bash.exists():
            bash = str(git_bash)
    return bash


class ComponentVersionTests(unittest.TestCase):
    def setUp(self):
        self.bash = find_bash()
        self.assertIsNotNone(self.bash, "Bash is required for shell gate regression tests")
        self._tmp = tempfile.TemporaryDirectory(prefix="observable-component-versions-")
        self.repo = Path(self._tmp.name)
        for rel in ["VERSION", "distribution/manifest.yaml", *COMPONENT_SOURCES]:
            target = self.repo / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(ROOT / rel, target)

    def tearDown(self):
        self._tmp.cleanup()

    def check(self):
        return subprocess.run(
            [self.bash, str(ROOT / "scripts/check-distribution-manifest.sh"),
             str(self.repo / "distribution/manifest.yaml"), str(self.repo)],
            capture_output=True, text=True,
        )

    def set_manifest_component(self, key, version):
        path = self.repo / "distribution/manifest.yaml"
        text, count = re.subn(rf"(?m)^(  {key}:)\s*\S+$", rf"\g<1> {version}", path.read_text())
        self.assertEqual(count, 1)
        path.write_text(text)

    def set_cargo_version(self, rel, version):
        path = self.repo / rel
        text, count = re.subn(r'(?m)^(version\s*=\s*)"[^"]+"', rf'\g<1>"{version}"', path.read_text(), count=1)
        self.assertEqual(count, 1)
        path.write_text(text)

    def test_baseline_passes(self):
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_package_bump_without_manifest_fails(self):
        self.set_cargo_version("services/query-api/Cargo.toml", "9.9.9")
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("services/query-api/Cargo.toml", result.stderr)

    def test_manifest_bump_without_package_fails(self):
        self.set_manifest_component("web", "9.9.9")
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("apps/frontend/package.json", result.stderr)

    def test_component_may_diverge_from_product_version(self):
        self.set_cargo_version("services/query-api/Cargo.toml", "9.9.9")
        self.set_manifest_component("query", "9.9.9")
        result = self.check()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_product_version_must_match_root_version(self):
        (self.repo / "VERSION").write_text("9.9.9\n")
        result = self.check()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("does not match VERSION", result.stderr)

    def test_component_version_lookup(self):
        self.set_manifest_component("storeClickhouse", "9.9.9")
        for name in ("store-clickhouse", "storeClickhouse"):
            with self.subTest(component=name):
                result = subprocess.run(
                    [self.bash, str(ROOT / "scripts/component-version.sh"), name,
                     str(self.repo / "distribution/manifest.yaml")],
                    capture_output=True, text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(result.stdout.strip(), "9.9.9")


if __name__ == "__main__":
    unittest.main()
