"""Immutable release decisions, with no Docker or registry dependency."""

from copy import deepcopy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import plan_component_release as release


DIGEST = "sha256:" + "a" * 64


def identity(**changes):
    value = dict(component="query", version="0.1.0", source_revision="1" * 40,
                 input_algorithm="fixture-v1", input_digest="sha256:" + "b" * 64,
                 platforms=["linux/amd64"])
    return value | changes


def record(**changes):
    return dict(identity=identity(), digest=DIGEST, identity_verified=True,
                sbom_subject=DIGEST, provenance_subject=DIGEST) | changes


def found(**changes):
    return dict(status="found", record=record(**changes))


def missing(**changes):
    return dict(status="missing", component="query", version="0.1.0") | changes


class PlannerTests(unittest.TestCase):
    def plan(self, observation=None, request=None, history=None):
        return release.plan_release(request or identity(), lambda: observation or found(), history)

    def test_unchanged_inputs_across_source_revisions_reuse_original(self):
        result = self.plan(request=identity(source_revision="2" * 40))
        self.assertEqual(result["action"], "reuse")
        self.assertEqual(result["digest"], DIGEST)
        self.assertEqual(result["source_revision"], "1" * 40)

    def test_changed_inputs_require_new_version(self):
        with self.assertRaisesRegex(release.Rejected, "identity"):
            self.plan(request=identity(input_digest="sha256:" + "c" * 64))

    def test_only_missing_tag_permits_publish_and_is_rechecked(self):
        calls = []
        def inspect():
            calls.append(1)
            return missing(version="0.2.0")
        result = release.plan_release(identity(version="0.2.0"), inspect)
        self.assertEqual(result["action"], "publish")
        self.assertIsNone(result["digest"])
        self.assertEqual(len(calls), 2)

    def test_missing_historical_image_rejected(self):
        with self.assertRaisesRegex(release.Rejected, "historical"):
            self.plan(missing(), history=record())

    def test_registry_failures_never_mean_missing(self):
        for status in ["unauthorized", "forbidden", "timeout", "error", "unknown", None]:
            with self.subTest(status=status), self.assertRaises(release.Rejected):
                self.plan({"status": status})

    def test_partial_evidence_completion_never_republishes(self):
        for field in ["sbom_subject", "provenance_subject"]:
            result = self.plan(found(**{field: None}))
            self.assertEqual(result["action"], "complete-evidence")
            self.assertEqual(result["digest"], DIGEST)
            self.assertEqual(result["missing_evidence"], [field.removesuffix("_subject")])

    def test_bad_evidence_subject_rejected(self):
        with self.assertRaisesRegex(release.Rejected, "subject"):
            self.plan(found(sbom_subject="sha256:" + "c" * 64))

    def test_unverified_or_wrong_identity_rejected(self):
        observations = [found(identity_verified=False), found(identity_verified=1),
                        found(identity=identity(component="auth")),
                        found(identity=identity(version="0.2.0")),
                        found(identity=identity(input_algorithm="different-v1")),
                        found(identity=identity(platforms=["linux/arm64"]))]
        for observation in observations:
            with self.subTest(observation=observation), self.assertRaises(release.Rejected):
                self.plan(observation)

    def test_history_digest_and_identity_cannot_be_replaced(self):
        for historical in [record(digest="sha256:" + "d" * 64,
                                  sbom_subject=None, provenance_subject=None),
                           record(identity=identity(input_digest="sha256:" + "d" * 64)),
                           record(identity=identity(source_revision="2" * 40))]:
            with self.subTest(history=historical), self.assertRaises(release.Rejected):
                self.plan(history=historical)

    def test_concurrent_matching_publication_reused_or_completed(self):
        for observation, expected in [(found(), "reuse"),
                                      (found(sbom_subject=None), "complete-evidence")]:
            observations = iter([missing(), observation])
            result = release.plan_release(identity(), lambda: next(observations))
            self.assertEqual(result["action"], expected)
            self.assertEqual(result["digest"], DIGEST)

    def test_concurrent_conflict_or_error_rejects(self):
        for observation in [found(identity=identity(input_digest="sha256:" + "c" * 64)),
                            {"status": "error"}]:
            observations = iter([missing(), observation])
            with self.assertRaises(release.Rejected):
                release.plan_release(identity(), lambda: next(observations))

    def test_inspector_exception_is_sanitized(self):
        def inspect():
            raise RuntimeError("sensitive upstream diagnostic")
        with self.assertRaisesRegex(release.Rejected, "registry inspection failed") as error:
            release.plan_release(identity(), inspect)
        self.assertNotIn("sensitive", str(error.exception))

    def test_invalid_requests_fail_closed(self):
        invalid = [identity(component="contracts"), identity(component="unknown"),
                   identity(version="latest"), identity(version="0.1.0+build"),
                   identity(version="01.1.0"), identity(source_revision="main"),
                   identity(input_digest="not-a-digest"), identity(platforms=[]),
                   identity(platforms=["linux/amd64", "linux/amd64"]),
                   identity(platforms=["nonsense"]), identity(input_algorithm=""),
                   identity(unexpected=True), None, []]
        for request in invalid:
            with self.subTest(request=request), self.assertRaises(release.Rejected):
                release.plan_release(request, lambda: found())

    def test_malformed_observations_and_history_fail_closed(self):
        for observation in [None, [], {"status": "found"},
                            {"status": "missing"}, missing(component="auth"),
                            missing(version="0.2.0"), missing(record=record()),
                            found(digest="bad"), found(unexpected=True)]:
            with self.subTest(observation=observation), self.assertRaises(release.Rejected):
                release.plan_release(identity(), lambda: observation)
        with self.assertRaises(release.Rejected):
            self.plan(history=record(identity=identity(version="0.0.9")))

    def test_all_runtime_component_names_supported(self):
        from ci_changed_components import ALL_COMPONENTS
        for component in ALL_COMPONENTS:
            with self.subTest(component=component):
                request = identity(component=component)
                result = self.plan(found(identity=request), request=request)
                self.assertEqual(result["image"], f"ghcr.io/ktjn/observable-{component}:0.1.0")

    def test_inputs_are_not_mutated(self):
        request, observation = identity(), found()
        before = deepcopy((request, observation))
        self.plan(observation, request)
        self.assertEqual((request, observation), before)


class CliTests(unittest.TestCase):
    def run_cli(self, text):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "input.json"
            path.write_text(text, encoding="utf-8")
            return subprocess.run([sys.executable, str(ROOT / "scripts/plan_component_release.py"),
                                   str(path)], capture_output=True, text=True)

    def test_cli_reuse_is_deterministic(self):
        text = json.dumps(dict(request=identity(), observations=[found()]))
        first, second = self.run_cli(text), self.run_cli(text)
        self.assertEqual(first.returncode, 0, first.stderr)
        self.assertEqual(first.stdout, second.stdout)
        self.assertEqual(json.loads(first.stdout)["action"], "reuse")

    def test_cli_publish_requires_two_observations(self):
        for observations, expected in [([missing()], 1), ([missing()] * 2, 0),
                                       ([found(), None], 1)]:
            result = self.run_cli(json.dumps(dict(request=identity(), observations=observations)))
            self.assertEqual(result.returncode, expected, result.stderr)

    def test_invalid_json_schema_or_duplicate_keys_produce_no_plan(self):
        for text in ['{', '{}', '{"request": {}, "request": {}}', '[]',
                     json.dumps(dict(request=identity(), observations=[found()], secret="sentinel"))]:
            result = self.run_cli(text)
            self.assertEqual(result.returncode, 1)
            self.assertEqual(result.stdout, "")
            self.assertNotIn("sentinel", result.stderr)
            self.assertNotIn("Traceback", result.stderr)


if __name__ == "__main__":
    unittest.main()
