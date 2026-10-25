"""Tests for verified build-input identity enforcement."""

import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import collect_component_inputs as inputs
import verify_component_inputs as verifier
import verify_component_base_images as base_images


def mock_verified_base_images(declared):
    return {
        "component": declared["component"],
        "source_revision": declared["source_revision"],
        "input_algorithm": declared["input_algorithm"],
        "input_digest": declared["input_digest"],
        "platforms": declared["platforms"],
        "images": declared["images"],
        "registry_images_verified": True,
        "identity_verified": False,
        "publication_authorized": False,
    }


class VerifiedInputTests(unittest.TestCase):
    @mock.patch("verify_component_base_images.verify")
    def test_verify_rust_component_inputs_succeeds(self, mock_verify):
        declared = inputs.resolve(ROOT, "query", "HEAD", ("linux/amd64",))
        mock_verify.return_value = mock_verified_base_images(declared)

        result = verifier.verify(ROOT, "query", "HEAD", ("linux/amd64",))
        self.assertEqual(result["component"], "query")
        self.assertEqual(result["version"], "0.1.0")
        self.assertTrue(result["identity_verified"])
        self.assertFalse(result["publication_authorized"])
        self.assertTrue(result["registry_images_verified"])
        self.assertTrue(result["materials_verified"])
        self.assertIn("cargo", result["materials"])
        self.assertIn("apt", result["materials"])
        self.assertEqual(result["materials"]["apt"]["algorithm"], "apt-material-lock-v1")

    @mock.patch("verify_component_base_images.verify")
    def test_verify_web_component_inputs_succeeds(self, mock_verify):
        declared = inputs.resolve(ROOT, "web", "HEAD", ("linux/amd64",))
        mock_verify.return_value = mock_verified_base_images(declared)

        result = verifier.verify(ROOT, "web", "HEAD", ("linux/amd64",))
        self.assertEqual(result["component"], "web")
        self.assertEqual(result["version"], "0.1.0")
        self.assertTrue(result["identity_verified"])
        self.assertFalse(result["publication_authorized"])
        self.assertTrue(result["registry_images_verified"])
        self.assertTrue(result["materials_verified"])
        self.assertIn("npm", result["materials"])

    @mock.patch("verify_component_base_images.verify")
    def test_verify_fails_when_base_image_registry_fails(self, mock_verify):
        mock_verify.side_effect = base_images.VerificationFailed("base image failed")

        with self.assertRaises(verifier.VerificationFailed):
            verifier.verify(ROOT, "query", "HEAD", ("linux/amd64",))

    @mock.patch("verify_component_base_images.verify")
    def test_verify_declared_input_consistency_check(self, mock_verify):
        declared = inputs.resolve(ROOT, "query", "HEAD", ("linux/amd64",))
        mock_verify.return_value = mock_verified_base_images(declared)

        result = verifier.verify(
            ROOT, "query", "HEAD", ("linux/amd64",),
            declared_input=declared,
        )
        self.assertEqual(result["input_digest"], declared["input_digest"])
        self.assertTrue(result["identity_verified"])

        # Tampered declared input must fail
        tampered = dict(declared)
        tampered["input_digest"] = "sha256:" + "0" * 64
        with self.assertRaises(verifier.VerificationFailed):
            verifier.verify(
                ROOT, "query", "HEAD", ("linux/amd64",),
                declared_input=tampered,
            )

    @mock.patch("verify_component_base_images.verify")
    def test_cli_invocation_success_and_failure(self, mock_verify):
        declared = inputs.resolve(ROOT, "auth", "HEAD", ("linux/amd64",))
        mock_verify.return_value = mock_verified_base_images(declared)

        with tempfile.TemporaryDirectory() as tmp:
            declared_file = Path(tmp) / "declared.json"
            declared_file.write_text(json.dumps(declared), encoding="utf-8")

            out = io.StringIO()
            with mock.patch("sys.stdout", out):
                code = verifier.main(["auth", "--declared-input", str(declared_file)])
            self.assertEqual(code, 0)
            parsed = json.loads(out.getvalue())
            self.assertTrue(parsed["identity_verified"])
            self.assertEqual(parsed["component"], "auth")
