#!/usr/bin/env python3
"""Business-spec readiness report for the 2.0 line.

Reads spec/system.yaml (with includes) and docs/plans/business-spec-gap.yaml and
prints, per plan phase, how many OBS requirements are verified, partial or
missing. A requirement counts as verified only when its status is `verified`
and it has passing evidence for its version.

Usage:
  uv run --project models python scripts/spec-readiness.py [--priority P0] [--phase N] [--json]
  --fail-below N   exit 1 when fewer than N P0 requirements are verified
"""
from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parent.parent
SPEC = ROOT / "spec/system.yaml"
GAP = ROOT / "docs/plans/business-spec-gap.yaml"


def load_spec():
    from system_spec_ref.metamodel import load_spec as _load  # type: ignore
    return _load(SPEC)


def _load_spec_fallback():
    sys.path.insert(0, str(ROOT / "system-spec/reference-implementation"))
    from metamodel import load_spec as _load  # type: ignore
    return _load(SPEC)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--priority", default=None, help="only this priority (P0, P1, P2)")
    ap.add_argument("--phase", type=int, default=None, help="only this plan phase")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--fail-below", type=int, default=None)
    a = ap.parse_args(argv)

    try:
        spec = load_spec()
    except ImportError:
        spec = _load_spec_fallback()
    gap = yaml.safe_load(GAP.read_text(encoding="utf-8"))["requirements"]
    reqs = {r.id: r for r in spec.requirements if r.id.startswith("OBS-") and r.status != "deprecated"}
    passing = {(e.requirement_id, e.requirement_version) for e in spec.evidence if e.passed}

    missing_in_gap = sorted(set(reqs) - set(gap))
    extra_in_gap = sorted(set(gap) - set(reqs))
    if missing_in_gap or extra_in_gap:
        print(f"gap file out of sync: not classified {missing_in_gap}, unknown {extra_in_gap}", file=sys.stderr)
        return 2

    rows: dict[int, dict[str, int]] = defaultdict(lambda: defaultdict(int))
    p0_verified = 0
    for rid, r in reqs.items():
        g = gap[rid]
        if a.priority and r.priority != a.priority:
            continue
        if a.phase is not None and g["phase"] != a.phase:
            continue
        verified = r.status == "verified" and (rid, r.version) in passing
        state = "verified" if verified else g["status"]
        rows[g["phase"]][state] += 1
        rows[g["phase"]]["total"] += 1
        if verified and r.priority == "P0":
            p0_verified += 1

    out = {str(k): dict(v) for k, v in sorted(rows.items())}
    if a.json:
        print(json.dumps({"phases": out, "p0_verified": p0_verified}, indent=2))
    else:
        print(f"{'phase':>5} {'total':>6} {'verified':>9} {'partial':>8} {'missing':>8}")
        tot = defaultdict(int)
        for k, v in sorted(rows.items()):
            print(f"{k:>5} {v['total']:>6} {v['verified']:>9} {v['partial']:>8} {v['missing']:>8}")
            for c in ("total", "verified", "partial", "missing"):
                tot[c] += v[c]
        print(f"{'all':>5} {tot['total']:>6} {tot['verified']:>9} {tot['partial']:>8} {tot['missing']:>8}")
        print(f"P0 verified: {p0_verified}")
    if a.fail_below is not None and p0_verified < a.fail_below:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
