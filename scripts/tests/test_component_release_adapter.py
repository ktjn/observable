"""Tests for component release history loading, registry adapter, and publication boundary."""

import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import component_release_adapter as adapter
import plan_component_release as release


DIGEST = "sha256:" + "a" * 64
DIGEST_B = "sha256:" + "b" * 64
REVISION = "c" * 40


def sample_identity(**changes):
    value = {
        "component": "query",
        "version": "0.1.0",
        "source_revision": REVISION,
        "input_algorithm": "component-input-v2",
        "input_digest": "sha256:" + "1" * 64,
        "platforms": ["linux/amd64"],
    }
    return value | changes


def sample_record(**changes):
    return {
        "identity": sample_identity(),
        "digest": DIGEST,
        "identity_verified": True,
        "sbom_subject": DIGEST,
        "provenance_subject": DIGEST,
    } | changes


class ReleaseHistoryTests(unittest.TestCase):
    def test_load_history_from_missing_or_none_returns_empty(self):
        self.assertEqual(adapter.load_history(None), {})
        with tempfile.TemporaryDirectory() as tmp:
            missing_path = Path(tmp) / "nonexistent.json"
            self.assertEqual(adapter.load_history(missing_path), {})

    def test_load_history_valid_records(self):
        record = sample_record()
        payload = {
            "algorithm": adapter.HISTORY_ALGORITHM,
            "records": [record],
        }
        with tempfile.NamedTemporaryFile("w", delete=False, suffix=".json", encoding="utf-8") as f:
            json.dump(payload, f)
            f.flush()
            path = Path(f.name)
        try:
            history = adapter.load_history(path)
            self.assertEqual(len(history), 1)
            self.assertIn(("query", "0.1.0"), history)
            self.assertEqual(history[("query", "0.1.0")]["digest"], DIGEST)
            self.assertTrue(history[("query", "0.1.0")]["identity_verified"])
        finally:
            path.unlink()

    def test_load_history_rejects_invalid_algorithm_or_structure(self):
        for payload in [
            {"algorithm": "wrong-v1", "records": []},
            {"records": []},
            {"algorithm": adapter.HISTORY_ALGORITHM},
            {"algorithm": adapter.HISTORY_ALGORITHM, "records": "not-a-list"},
            {"algorithm": adapter.HISTORY_ALGORITHM, "records": [{"bad": "record"}]},
            {"algorithm": adapter.HISTORY_ALGORITHM, "records": [sample_record(identity_verified=False)]},
        ]:
            with self.subTest(payload=payload):
                with tempfile.NamedTemporaryFile("w", delete=False, suffix=".json", encoding="utf-8") as f:
                    json.dump(payload, f)
                    f.flush()
                    path = Path(f.name)
                try:
                    with self.assertRaises(adapter.AdapterRejected):
                        adapter.load_history(path)
                finally:
                    path.unlink()

    def test_load_history_rejects_duplicate_component_and_version(self):
        record1 = sample_record()
        record2 = sample_record(digest=DIGEST_B, sbom_subject=DIGEST_B, provenance_subject=DIGEST_B)
        payload = {
            "algorithm": adapter.HISTORY_ALGORITHM,
            "records": [record1, record2],
        }
        with tempfile.NamedTemporaryFile("w", delete=False, suffix=".json", encoding="utf-8") as f:
            json.dump(payload, f)
            f.flush()
            path = Path(f.name)
        try:
            with self.assertRaisesRegex(adapter.AdapterRejected, "duplicate"):
                adapter.load_history(path)
        finally:
            path.unlink()


class RegistryAdapterTests(unittest.TestCase):
    def test_registry_adapter_missing_returns_planner_missing(self):
        def fake_inspect(component, version):
            return {"status": "missing", "component": component, "version": version}

        obs_fn = adapter.make_observation_adapter("query", "0.1.0", inspect_fn=fake_inspect, history={})
        obs = obs_fn()
        self.assertEqual(obs, {"status": "missing", "component": "query", "version": "0.1.0"})

    def test_registry_adapter_found_with_history(self):
        history = {("query", "0.1.0"): sample_record()}
        def fake_inspect(component, version):
            return {
                "status": "found",
                "component": component,
                "version": version,
                "image": f"ghcr.io/ktjn/observable-{component}:{version}",
                "digest": DIGEST,
                "media_type": "application/vnd.oci.image.index.v1+json",
                "identity_verified": False,
            }

        obs_fn = adapter.make_observation_adapter("query", "0.1.0", inspect_fn=fake_inspect, history=history)
        obs = obs_fn()
        self.assertEqual(obs["status"], "found")
        self.assertEqual(obs["record"]["digest"], DIGEST)
        self.assertTrue(obs["record"]["identity_verified"])

    def test_registry_adapter_found_digest_mismatch_with_history_rejects(self):
        history = {("query", "0.1.0"): sample_record(digest=DIGEST)}
        def fake_inspect(component, version):
            return {
                "status": "found",
                "component": component,
                "version": version,
                "image": f"ghcr.io/ktjn/observable-{component}:{version}",
                "digest": DIGEST_B,
                "media_type": "application/vnd.oci.image.index.v1+json",
                "identity_verified": False,
            }

        obs_fn = adapter.make_observation_adapter("query", "0.1.0", inspect_fn=fake_inspect, history=history)
        with self.assertRaisesRegex(adapter.AdapterRejected, "digest"):
            obs_fn()

    def test_registry_adapter_found_without_history_or_attestations_rejects(self):
        def fake_inspect(component, version):
            return {
                "status": "found",
                "component": component,
                "version": version,
                "image": f"ghcr.io/ktjn/observable-{component}:{version}",
                "digest": DIGEST,
                "media_type": "application/vnd.oci.image.index.v1+json",
                "identity_verified": False,
            }

        obs_fn = adapter.make_observation_adapter("query", "0.1.0", inspect_fn=fake_inspect, history={})
        with self.assertRaisesRegex(adapter.AdapterRejected, "unverified"):
            obs_fn()


class PublicationBoundaryTests(unittest.TestCase):
    def test_plan_clean_miss_produces_publish_intent(self):
        calls = []
        def fake_inspect(component, version):
            calls.append((component, version))
            return {"status": "missing", "component": component, "version": version}

        request = sample_identity()
        plan = adapter.plan_component_release_with_adapter(request, history={}, inspect_fn=fake_inspect)
        self.assertEqual(plan["action"], "publish")
        self.assertIsNone(plan["digest"])
        self.assertEqual(len(calls), 2)  # Reinspection occurred
        self.assertFalse(plan["publication_authorized"])

    def test_plan_race_detected_reinspected_found_matching_inputs_reuses(self):
        history = {("query", "0.1.0"): sample_record()}
        calls = []
        def fake_inspect(component, version):
            calls.append(len(calls))
            if len(calls) == 1:
                return {"status": "missing", "component": component, "version": version}
            return {
                "status": "found",
                "component": component,
                "version": version,
                "image": f"ghcr.io/ktjn/observable-{component}:{version}",
                "digest": DIGEST,
                "media_type": "application/vnd.oci.image.index.v1+json",
                "identity_verified": False,
            }

        # No historical record before publish
        request = sample_identity()
        # When second inspect finds it and history is updated, adapter verifies it
        plan = adapter.plan_component_release_with_adapter(
            request, history=history, inspect_fn=fake_inspect, allow_initial_miss_with_history=True
        )
        self.assertEqual(plan["action"], "reuse")
        self.assertEqual(plan["digest"], DIGEST)

    def test_enforce_publication_boundary_publish_requires_write_and_lock(self):
        publish_plan = sample_identity() | {
            "action": "publish",
            "image": "ghcr.io/ktjn/observable-query:0.1.0",
            "digest": None,
            "missing_evidence": ["sbom", "provenance"],
            "publication_authorized": False,
        }

        # Missing lock token fails
        with self.assertRaisesRegex(adapter.AdapterRejected, "lock"):
            adapter.enforce_publication_boundary(publish_plan, registry_write_enabled=True, lock_token=None)

        # Write disabled fails
        with self.assertRaisesRegex(adapter.AdapterRejected, "write"):
            adapter.enforce_publication_boundary(publish_plan, registry_write_enabled=False, lock_token="token-123")

        # Both present succeeds
        authorized = adapter.enforce_publication_boundary(publish_plan, registry_write_enabled=True, lock_token="token-123")
        self.assertTrue(authorized["publication_authorized"])

    def test_enforce_publication_boundary_reuse_permits_no_publish(self):
        reuse_plan = sample_identity() | {
            "action": "reuse",
            "image": "ghcr.io/ktjn/observable-query:0.1.0",
            "digest": DIGEST,
            "missing_evidence": [],
            "publication_authorized": False,
        }
        res = adapter.enforce_publication_boundary(reuse_plan, registry_write_enabled=False)
        self.assertEqual(res["action"], "reuse")
        self.assertFalse(res["publication_authorized"])

    def test_cli_end_to_end_invocation(self):
        import io
        from contextlib import redirect_stdout, redirect_stderr

        with tempfile.TemporaryDirectory() as tmp:
            req_file = Path(tmp) / "request.json"
            hist_file = Path(tmp) / "history.json"

            req_file.write_text(json.dumps(sample_identity()), encoding="utf-8")
            hist_payload = {
                "algorithm": adapter.HISTORY_ALGORITHM,
                "records": [sample_record()],
            }
            hist_file.write_text(json.dumps(hist_payload), encoding="utf-8")

            out = io.StringIO()
            with redirect_stdout(out):
                # When history is present and inspection is not mocked, we can pass mock inspect
                with mock.patch("inspect_component_registry.inspect") as mock_inspect:
                    mock_inspect.return_value = {
                        "status": "found",
                        "component": "query",
                        "version": "0.1.0",
                        "image": "ghcr.io/ktjn/observable-query:0.1.0",
                        "digest": DIGEST,
                        "media_type": "application/vnd.oci.image.index.v1+json",
                        "identity_verified": False,
                    }
                    code = adapter.main([str(req_file), "--history", str(hist_file)])

            self.assertEqual(code, 0)
            res = json.loads(out.getvalue())
            self.assertEqual(res["action"], "reuse")
            self.assertEqual(res["digest"], DIGEST)
