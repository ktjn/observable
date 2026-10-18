"""Guard the component list shared across the distribution tooling (stdlib only).

The deployable component set is duplicated in several scripts. If a new component
is added to only some of them it is silently exempted from the corresponding gate
(e.g. missing from the changelog check or the pinned-install override), so these
tests assert every list agrees. This is the cross-script companion to
`test_component_builds.py` (which checks the Cargo package mapping).
"""

from pathlib import Path
import re
import sys
import unittest


ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

import ci_changed_components as changed  # noqa: E402


def kebab(camel: str) -> str:
    return re.sub(r"(?<!^)(?=[A-Z])", "-", camel).lower()


class ComponentRegistryConsistencyTests(unittest.TestCase):
    def _expected_list(self, rel: str) -> list:
        text = (ROOT / rel).read_text()
        match = re.search(r"^EXPECTED=\(([^)]*)\)", text, re.MULTILINE)
        self.assertIsNotNone(match, f"{rel} has no EXPECTED=(...) list")
        return match[1].split()

    def test_manifest_tooling_lists_agree(self):
        manifest = self._expected_list("scripts/check-distribution-manifest.sh")
        self.assertEqual(manifest, self._expected_list("scripts/check-component-changelogs.sh"))
        self.assertEqual(manifest, self._expected_list("scripts/render-pinned-compose.sh"))

    def test_distribution_manifest_keys_match_the_tooling_list(self):
        manifest = self._expected_list("scripts/check-distribution-manifest.sh")
        keys = re.findall(
            r"^  ([A-Za-z]+):", (ROOT / "distribution/manifest.yaml").read_text(), re.MULTILINE
        )
        self.assertEqual(sorted(keys), sorted(manifest))

    def test_image_builder_list_matches_the_manifest_tooling(self):
        text = (ROOT / "scripts/build-component-images.sh").read_text()
        match = re.search(r"^ALL_COMPONENTS=\(([^)]*)\)", text, re.MULTILINE)
        self.assertIsNotNone(match, "build-component-images.sh has no ALL_COMPONENTS list")
        builder = match[1].split()
        # contracts has no runtime image, and the builder uses kebab-case names.
        expected = sorted(kebab(key) for key in self._expected_list("scripts/check-distribution-manifest.sh")
                          if key != "contracts")
        self.assertEqual(sorted(builder), expected)

    def _function_body(self, rel: str, name: str) -> str:
        text = (ROOT / rel).read_text()
        match = re.search(rf"(?ms)^{re.escape(name)}\(\) \{{\n(.*?)^\}}", text)
        self.assertIsNotNone(match, f"{rel}: function {name}() not found")
        return match[1]

    def _case_arms(self, rel: str, name: str) -> list:
        return re.findall(r"^\s+([A-Za-z][A-Za-z-]*)\)", self._function_body(rel, name), re.MULTILINE)

    def test_case_maps_cover_every_component(self):
        camel = self._expected_list("scripts/check-distribution-manifest.sh")
        kebab_components = [kebab(key) for key in camel if key != "contracts"]
        maps = [
            ("scripts/check-distribution-manifest.sh", "component_source", camel),
            ("scripts/check-distribution-manifest.sh", "component_helm_key", camel),
            ("scripts/check-component-changelogs.sh", "component_changelog", camel),
            ("scripts/render-pinned-compose.sh", "component_compose", camel),
            ("scripts/build-component-images.sh", "component_package", kebab_components),
        ]
        for rel, name, expected in maps:
            arms = self._case_arms(rel, name)
            with self.subTest(file=rel, function=name, direction="missing"):
                missing = [key for key in expected if key not in arms]
                self.assertEqual(
                    missing, [], f"{rel}: {name}() has no case arm for {missing} (silently skipped)"
                )
            with self.subTest(file=rel, function=name, direction="stale"):
                stale = [arm for arm in arms if arm not in expected]
                self.assertEqual(
                    stale, [], f"{rel}: {name}() has arms for non-components {stale}"
                )

    def test_ci_changed_components_matches_the_builder(self):
        text = (ROOT / "scripts/build-component-images.sh").read_text()
        match = re.search(r"^ALL_COMPONENTS=\(([^)]*)\)", text, re.MULTILINE)
        self.assertIsNotNone(match)
        self.assertEqual(sorted(changed.ALL_COMPONENTS), sorted(match[1].split()))


if __name__ == "__main__":
    unittest.main()
