#!/usr/bin/env python3
"""Regression tests for scripts/spec-readiness.py evidence policy.

Run: uv run --project models python scripts/tests/test_spec_readiness.py
"""
from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def _load_readiness():
    s = importlib.util.spec_from_file_location("spec_readiness", ROOT / "scripts/spec-readiness.py")
    mod = importlib.util.module_from_spec(s)
    s.loader.exec_module(mod)
    return mod


class ReadinessPolicyTests(unittest.TestCase):
    RID = "OBS-03-01"

    def setUp(self):
        self.rd = _load_readiness()
        self.mm = self.rd.load_metamodel()
        self.spec = self.mm.load_spec(self.rd.SPEC)
        self.req = next(r for r in self.spec.requirements if r.id == self.RID)
        self.req.status = "verified"
        mm, spec = self.mm, self.spec

        class Shim:
            def __getattr__(self, name):
                return getattr(mm, name)

            def load_spec(self, _path):
                return spec

        self.rd.load_metamodel = lambda: Shim()
        # Requirements verified elsewhere in the spec are the baseline; each case measures its
        # own record against it.
        _, base = self._report()
        self.base = base["p0_verified"]
        self.base_platform = base["p0_by_holder"]["platform"]["verified"]

    def ev(self, eid, **kw):
        base = dict(id=eid, requirement_id=self.RID, requirement_version=self.req.version, method="test",
                    passed=True, provenance=f"tests/x.rs::{eid}", criteria=list(self.req.criteria), verifier="cargo-test")
        base.update(kw)
        return self.mm.Evidence(**base)

    def run_report(self, *records):
        self.spec.evidence = list(self.spec.evidence) + list(records)
        return self._report()

    def _report(self):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf), contextlib.redirect_stderr(io.StringIO()):
            rc = self.rd.main(["--phase", "2", "--json", "--strict"])
        return rc, json.loads(buf.getvalue())

    def test_complete_criterion_evidence_verifies(self):
        rc, out = self.run_report(self.ev("ev-ok"))
        self.assertEqual(rc, 0)
        self.assertEqual(out["p0_by_holder"]["platform"]["verified"], self.base_platform + 1)

    def test_skipped_record_does_not_verify(self):
        rc, out = self.run_report(self.ev("ev-skip", skipped=["negative path skipped"]))
        self.assertEqual(rc, 1)
        self.assertEqual(out["p0_verified"], self.base)

    def test_unsuperseded_failing_record_blocks_verification(self):
        rc, out = self.run_report(self.ev("ev-ok"), self.ev("ev-fail", passed=False, criteria=[self.req.criteria[1]]))
        self.assertEqual(rc, 1)
        self.assertEqual(out["p0_verified"], self.base)

    def test_blanket_record_is_a_policy_violation(self):
        rc, out = self.run_report(self.ev("ev-blanket", criteria=[]))
        self.assertEqual(rc, 1)
        self.assertEqual(out["p0_verified"], self.base)
        self.assertTrue(any("blanket" in v for v in out["violations"]))

    def test_inspection_cannot_verify_platform_requirement(self):
        rc, out = self.run_report(self.ev("ev-inspect", method="inspection"))
        self.assertEqual(rc, 1)
        self.assertEqual(out["p0_verified"], self.base)

    def test_missing_verifier_is_a_policy_violation(self):
        rc, out = self.run_report(self.ev("ev-nobody", verifier=None))
        self.assertEqual(rc, 1)
        self.assertEqual(out["p0_verified"], self.base)

    def test_partial_criteria_do_not_verify(self):
        rc, out = self.run_report(self.ev("ev-half", criteria=[self.req.criteria[0]]))
        self.assertEqual(rc, 1)
        self.assertEqual(out["p0_verified"], self.base)


if __name__ == "__main__":
    unittest.main()
