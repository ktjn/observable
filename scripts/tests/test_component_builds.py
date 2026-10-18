"""Guard service-scoped component builds (stdlib only)."""

from pathlib import Path
import re
import sys
import unittest


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import ci_changed_components as changed  # noqa: E402


ALL_RUST = list(changed.RUST_COMPONENTS)


class AffectedComponentTests(unittest.TestCase):
    def assertAffected(self, paths, expected):
        self.assertEqual(changed.affected_components(paths), expected)

    def test_service_change_is_scoped(self):
        self.assertAffected(["services/query-api/src/main.rs"], ["query"])
        self.assertAffected(["services/auth-service/Cargo.toml"], ["auth"])

    def test_lib_change_reaches_only_dependents(self):
        self.assertAffected(["libs/query-core/src/lib.rs"], ["query"])
        self.assertAffected(["libs/observable-process/src/lib.rs"], ["process"])

    def test_shared_lib_change_reaches_every_dependent(self):
        self.assertAffected(["libs/domain/src/lib.rs"], ALL_RUST)

    def test_dev_only_crates_do_not_rebuild_artifacts(self):
        self.assertAffected(["libs/test-support/src/lib.rs"], [])
        self.assertAffected(["libs/playground-wasm/src/lib.rs"], [])

    def test_global_inputs_rebuild_every_rust_component(self):
        for path in ("Cargo.lock", "Cargo.toml", "Dockerfile", "proto/otlp/x.proto"):
            with self.subTest(path=path):
                self.assertAffected([path], ALL_RUST)

    def test_unknown_crate_layout_fails_safe(self):
        self.assertAffected(["libs/not-a-member/src/lib.rs"], ALL_RUST)

    def test_frontend_and_docs(self):
        self.assertAffected(["apps/frontend/src/main.tsx"], ["web"])
        self.assertAffected(["docs/architecture.md"], [])

    def test_package_mapping_matches_image_builder(self):
        script = (ROOT / "scripts/build-component-images.sh").read_text()
        mapping = dict(re.findall(r'^\s+([a-z-]+)\) echo "([a-z-]+)" ;;$', script, re.MULTILINE))
        self.assertEqual(mapping, changed.RUST_COMPONENTS)


class ComposeComponentImageTests(unittest.TestCase):
    def test_compose_runs_one_image_per_component(self):
        compose = (ROOT / "docker-compose.yml").read_text()
        self.assertNotIn("observable-services", compose)
        for component, package in changed.RUST_COMPONENTS.items():
            with self.subTest(component=component):
                block = re.search(rf"(?ms)^  {re.escape(package)}:\n(.*?)(?=^  \S|\Z)", compose)
                self.assertIsNotNone(block, f"compose service {package} missing")
                body = block[1]
                self.assertIn(f"image: observable-{component}:local", body)
                self.assertIn("target: component-runtime", body)
                self.assertIn(f"args: {{ SERVICE: {package} }}", body)


class HelmComponentImageTests(unittest.TestCase):
    def test_helm_and_kind_tooling_use_component_images(self):
        paths = [
            *sorted((ROOT / "charts/observable/templates").glob("*.yaml")),
            *sorted((ROOT / "charts/observable-common/templates").glob("*.tpl")),
            ROOT / "charts/observable/values.yaml",
            ROOT / "charts/observable/values.production-example.yaml",
            ROOT / "scripts/kind-test.sh",
            ROOT / "scripts/dev-reload.sh",
            ROOT / "scripts/canary-promote.sh",
        ]
        forbidden = re.compile(r"observable-services|global\.image\.(?:repository|tag)|frontendImage")
        for path in paths:
            with self.subTest(path=str(path.relative_to(ROOT))):
                self.assertIsNone(forbidden.search(path.read_text()))


class WorkflowScopeTests(unittest.TestCase):
    def test_artifact_build_is_service_scoped(self):
        workflow = (ROOT / ".github/workflows/build.yml").read_text()
        self.assertNotIn("cargo build --release --workspace", workflow)
        self.assertIn("needs.changes.outputs.rust_components", workflow)


class DockerfileComponentStageTests(unittest.TestCase):
    """Guard the per-component images against shipping a cache-mount stub.

    `component-builder` builds into a BuildKit cache at /app/target, which only
    exists for the duration of that RUN. The built binary must be copied to a
    real layer path inside the same RUN (bash by analogy with rust-builder);
    otherwise component-runtime copies the cargo-chef `cook` dummy binary baked
    into the cacher layer and every component image exits on start.
    """

    def _stage(self, name):
        dockerfile = (ROOT / "Dockerfile").read_text()
        block = re.search(rf"(?ms)^FROM \S+ AS {re.escape(name)}\n(.*?)(?=^FROM |\Z)", dockerfile)
        self.assertIsNotNone(block, f"{name} stage missing")
        return block[1]

    def test_component_builder_copies_binary_within_build_run(self):
        body = self._stage("component-builder")
        self.assertIn('cargo build --release -p "${SERVICE}"', body)
        self.assertIn('cp "/app/target/release/${SERVICE}" "/app/bin/${SERVICE}"', body)

    def test_component_runtime_takes_binary_from_bin_dir(self):
        body = self._stage("component-runtime")
        self.assertIn(
            "COPY --from=component-builder /app/bin/${SERVICE} /usr/local/bin/${SERVICE}",
            body,
        )
        self.assertNotIn("/app/target/release/${SERVICE}", body)


class ComponentImageCheckTests(unittest.TestCase):
    def test_check_script_covers_every_rust_component(self):
        script = (ROOT / "scripts/check-component-images.sh").read_text()
        for component, package in changed.RUST_COMPONENTS.items():
            with self.subTest(component=component):
                self.assertIn(f"{component}:{package}", script)


if __name__ == "__main__":
    unittest.main()
