#!/usr/bin/env python3
"""Business-spec readiness report for the 2.0 line.

Reads spec/system.yaml (with includes), docs/plans/business-spec-gap.yaml and the immutable
baseline (spec/business/baseline/v2.6) and prints, per plan phase, how many OBS requirements
and phase-gating refinement requirements are verified, partial or missing.

A requirement counts as verified only when all of these hold:
  - its status is `verified`;
  - the metamodel's own evidence evaluation verifies it for its current version
    (every criterion covered, no skipped records, no unsuperseded failing record);
  - every evidence record names the criteria it covers (no blanket record);
  - every evidence record names a verifier;
  - every evidence method is allowed for the requirement's obligation holder
    (platform: test, analysis, model-check, proof; organization or process: inspection, analysis).

The obligation holder comes from the baseline target (`<area>.<holder>`); the gap file must agree.

Usage:
  uv run --project models python scripts/spec-readiness.py [--priority P0] [--phase N] [--json]
  --strict        exit 1 on any evidence policy violation or a `verified` status without valid evidence
  --fail-below N  exit 1 when fewer than N P0 requirements are verified
"""
from __future__ import annotations

import argparse
import glob
import json
import sys
from collections import defaultdict
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
SPEC = ROOT / "spec/system.yaml"
GAP = ROOT / "docs/plans/business-spec-gap.yaml"
REGISTER = ROOT / "spec/business/refinements.yaml"
ALLOWED_METHODS = {
    "platform": {"test", "analysis", "inspection"},
    "organization": {"inspection"},
    "process": {"test", "inspection"},
}


def load_metamodel():
    sys.path.insert(0, str(ROOT / "system-spec/reference-implementation"))
    import metamodel  # type: ignore
    return metamodel


def baseline_holders() -> dict[str, str]:
    reg = yaml.safe_load(REGISTER.read_text(encoding="utf-8"))
    out = {}
    for f in sorted(glob.glob(str(ROOT / reg["baseline"] / "spec/areas/*.yaml"))):
        for r in yaml.safe_load(Path(f).read_text(encoding="utf-8")).get("requirements", []):
            out[r["id"]] = r["target"].rpartition(".")[2]
    return out


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--priority", default=None, help="only this priority (P0, P1, P2)")
    ap.add_argument("--phase", type=int, default=None, help="only this plan phase")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--strict", action="store_true")
    ap.add_argument("--fail-below", type=int, default=None)
    a = ap.parse_args(argv)

    mm = load_metamodel()
    spec = mm.load_spec(SPEC)
    gap_doc = yaml.safe_load(GAP.read_text(encoding="utf-8"))
    gap = gap_doc["requirements"]
    extra = gap_doc.get("refinements", {}) or {}
    holders = baseline_holders()

    all_reqs = {r.id: r for r in spec.requirements}
    reqs = {r.id: r for r in spec.requirements if r.id.startswith("OBS-") and r.status != "deprecated"}
    errors: list[str] = []

    missing_in_gap = sorted(set(reqs) - set(gap))
    extra_in_gap = sorted(set(gap) - set(reqs))
    if missing_in_gap or extra_in_gap:
        print(f"gap file out of sync: not classified {missing_in_gap}, unknown {extra_in_gap}", file=sys.stderr)
        return 2
    for rid, g in gap.items():
        if holders.get(rid) != g["holder"]:
            errors.append(f"{rid}: gap holder '{g['holder']}' differs from baseline holder '{holders.get(rid)}'")
        if reqs[rid].priority != g["priority"] or reqs[rid].target != g["target"]:
            errors.append(f"{rid}: gap priority/target differ from spec/system.yaml")
    for rid, g in extra.items():
        r = all_reqs.get(rid)
        if r is None or r.status == "deprecated":
            errors.append(f"{rid}: gap refinement names no active requirement")
            continue
        if r.priority != g["priority"]:
            errors.append(f"{rid}: gap refinement priority differs from spec/system.yaml")
        reqs[rid] = r
        gap[rid] = g
    if errors:
        for e in errors:
            print(e, file=sys.stderr)
        return 2

    ok = mm.verified_requirements(spec)
    records = defaultdict(list)
    for e in spec.evidence:
        records[(e.requirement_id, e.requirement_version)].append(e)

    violations: list[str] = []
    rows: dict[int, dict[str, int]] = defaultdict(lambda: defaultdict(int))
    p0 = defaultdict(lambda: [0, 0])
    for rid, r in reqs.items():
        g = gap[rid]
        holder = g["holder"]
        recs = records.get((rid, r.version), [])
        policy_ok = True
        for e in recs:
            if not e.criteria:
                violations.append(f"{rid}: evidence '{e.id}' covers no named criteria (blanket record)")
                policy_ok = False
            if not (e.verifier or "").strip():
                violations.append(f"{rid}: evidence '{e.id}' names no verifier")
                policy_ok = False
            if e.method not in ALLOWED_METHODS[holder]:
                violations.append(f"{rid}: evidence '{e.id}' method '{e.method}' not allowed for holder '{holder}'")
                policy_ok = False
        verified = r.status == "verified" and (rid, r.version) in ok and policy_ok
        if r.status == "verified" and not verified:
            violations.append(f"{rid}: status is verified without valid criterion-level evidence")
        if r.priority == "P0":
            p0[holder][1] += 1
            p0[holder][0] += int(verified)
        if a.priority and r.priority != a.priority:
            continue
        if a.phase is not None and g["phase"] != a.phase:
            continue
        state = "verified" if verified else g["status"]
        rows[g["phase"]][state] += 1
        rows[g["phase"]]["total"] += 1

    p0_verified = sum(v for v, _ in p0.values())
    out = {str(k): dict(v) for k, v in sorted(rows.items())}
    if a.json:
        print(json.dumps({"phases": out, "p0_verified": p0_verified,
                          "p0_by_holder": {h: {"verified": v, "total": t} for h, (v, t) in p0.items()},
                          "violations": violations}, indent=2))
    else:
        print(f"{'phase':>5} {'total':>6} {'verified':>9} {'partial':>8} {'missing':>8}")
        tot = defaultdict(int)
        for k, v in sorted(rows.items()):
            print(f"{k:>5} {v['total']:>6} {v['verified']:>9} {v['partial']:>8} {v['missing']:>8}")
            for c in ("total", "verified", "partial", "missing"):
                tot[c] += v[c]
        print(f"{'all':>5} {tot['total']:>6} {tot['verified']:>9} {tot['partial']:>8} {tot['missing']:>8}")
        print("P0 verified: " + ", ".join(f"{h} {v}/{t}" for h, (v, t) in sorted(p0.items())) + f" (total {p0_verified})")
        for v in violations:
            print(f"policy: {v}", file=sys.stderr)
    if a.strict and violations:
        return 1
    if a.fail_below is not None and p0_verified < a.fail_below:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
