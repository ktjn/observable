"""Guard the local-validation / GitHub artifact-build boundary (stdlib only)."""

from pathlib import Path
import os
import re
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class CiPolicyTests(unittest.TestCase):
    def test_model_compatibility_checks_uncommitted_edits(self):
        bash = shutil.which("bash")
        if os.name == "nt":
            git = Path(shutil.which("git"))
            git_bash = git.parent.parent / "bin/bash.exe"
            if git_bash.exists():
                bash = str(git_bash)
        self.assertIsNotNone(bash, "Bash is required for shell gate regression tests")
        with tempfile.TemporaryDirectory(prefix="observable-ci-policy-") as directory:
            repo = Path(directory)
            (repo / "scripts").mkdir()
            (repo / "models").mkdir()
            (repo / "bin").mkdir()
            shutil.copy(ROOT / "scripts/check-breaking-changes.sh", repo / "scripts")
            # Stub only the downstream tool: verify the shell gate dispatches it
            # for a working-tree edit even when BASE and HEAD are identical.
            (repo / "bin/uv").write_text("#!/bin/sh\nexit 7\n")
            (repo / "bin/uv").chmod(0o755)
            model = repo / "models/example.mdl"
            model.write_text("baseline\n")
            for args in (
                ["init", "-q"],
                ["add", "."],
                ["-c", "user.name=CI Policy Test", "-c", "user.email=ci-policy@example.invalid",
                 "-c", "commit.gpgsign=false", "commit", "-qm", "fixture"],
            ):
                subprocess.run(["git", *args], cwd=repo, check=True, capture_output=True)
            model.write_text("breaking working-tree edit\n")
            result = subprocess.run(
                [bash, "-c", 'export PATH="$PWD/bin:$PATH"; bash scripts/check-breaking-changes.sh HEAD HEAD'],
                cwd=repo, capture_output=True, text=True,
            )
            self.assertEqual(result.returncode, 7, result.stdout + result.stderr)

    def test_github_does_not_run_validation_suites(self):
        forbidden = re.compile(
            r"cargo (?:fmt|clippy|test|nextest)|npm run (?:test|lint|typecheck)"
            r"|npx playwright|scripts/(?:local-ci|check-[\w-]+|helm-lint|verify-version)\.sh"
            r"|docker compose up (?:smoke-test|perf-smoke)"
        )
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            with self.subTest(workflow=path.name):
                executable = "\n".join(
                    line for line in path.read_text().splitlines()
                    if not line.lstrip().startswith("#")
                )
                self.assertIsNone(forbidden.search(executable))

    def test_release_build_does_not_inherit_validation(self):
        dockerfile = (ROOT / "Dockerfile").read_text()
        stages = {}
        current = None
        for line in dockerfile.splitlines():
            match = re.match(r"FROM (\S+) AS (\S+)", line, re.IGNORECASE)
            if match:
                current = match[2]
                stages[current] = [match[1]]
            elif current:
                stages[current].extend(re.findall(r"--from=(\S+)", line))

        def dependencies(stage, seen):
            if stage in seen:
                return
            seen.add(stage)
            for source in stages.get(stage, []):
                dependencies(source, seen)

        for target in ("runtime", "component-runtime", "component-runtime-ci"):
            seen = set()
            dependencies(target, seen)
            with self.subTest(target=target):
                self.assertIn(target, stages)
                self.assertNotIn("rust-ci", seen)

    def test_local_gate_retains_moved_coverage(self):
        local = (ROOT / "scripts/local-ci.sh").read_text()
        for command in (
            "scripts/verify-version.sh",
            "scripts/check-component-boundaries.sh",
        "scripts/check-component-images.sh",
        "scripts/check-clickhouse-schema-compat.sh",
        "scripts/check-component-changelogs.sh",
        "scripts/render-pinned-compose.sh",
        "scripts/run-formal-verification.sh",
            "--target json-schema",
            "scripts/build-playground.sh",
            "playwright.playground.config.ts",
            "docker compose up perf-smoke --abort-on-container-exit",
            "cargo test --workspace --tests",
            "npm run test:a11y --workspace=apps/frontend",
        ):
            with self.subTest(command=command):
                self.assertTrue(command in local, f"Missing local coverage: {command}")


if __name__ == "__main__":
    unittest.main()
