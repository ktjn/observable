#!/usr/bin/env python3
"""Verify the immutable business-spec baseline and that derived fragments only deviate as declared.

Checks:
  1. Every baseline file matches MANIFEST.sha256; no file is missing or unlisted.
  2. Every baseline requirement and risk exists in spec/business with identical fields, except
     fields covered by a field policy in spec/business/refinements.yaml.
  3. Baseline views, experiences and state sets are preserved: changed or removed baseline
     content needs a declared refinement path; additions are allowed.
  4. Every declared refinement path still names a real deviation (no stale entries).

Usage: python3 scripts/check-business-baseline.py [--report]
"""
from __future__ import annotations

import argparse
import glob
import hashlib
import sys
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
REGISTER = ROOT / "spec/business/refinements.yaml"


def _yaml(path: Path):
    return yaml.safe_load(path.read_text(encoding="utf-8")) or {}


def _items(pattern: Path, key: str) -> dict[str, dict]:
    out: dict[str, dict] = {}
    for f in sorted(glob.glob(str(pattern))):
        for item in _yaml(Path(f)).get(key, []) or []:
            out[item["id"]] = item
    return out


def check_manifest(base: Path) -> list[str]:
    errors = []
    manifest = base / "MANIFEST.sha256"
    if not manifest.is_file():
        return [f"{manifest.relative_to(ROOT)}: missing"]
    listed = {}
    for line in manifest.read_text(encoding="utf-8").splitlines():
        digest, _, rel = line.partition("  ")
        listed[rel] = digest
    actual = {p.relative_to(base).as_posix() for p in base.rglob("*") if p.is_file()} - {"MANIFEST.sha256", "SOURCE.md"}
    for rel in sorted(actual - set(listed)):
        errors.append(f"baseline file not in manifest: {rel}")
    for rel, digest in sorted(listed.items()):
        p = base / rel
        if not p.is_file():
            errors.append(f"baseline file missing: {rel}")
        elif hashlib.sha256(p.read_bytes()).hexdigest() != digest:
            errors.append(f"baseline file modified: {rel}")
    return errors


def _covered(path: str, declared: set[str]) -> str | None:
    parts = path.split("/")
    for i in range(len(parts), 0, -1):
        prefix = "/".join(parts[:i])
        if prefix in declared:
            return prefix
    return None


def _diff(base, derived, path: str, out: list[str]) -> None:
    """Record paths where baseline content is changed or removed; additions are ignored."""
    if isinstance(base, dict):
        if not isinstance(derived, dict):
            out.append(path)
            return
        for k, v in base.items():
            if k not in derived:
                out.append(f"{path}/{k}")
            else:
                _diff(v, derived[k], f"{path}/{k}", out)
    elif isinstance(base, list):
        if not isinstance(derived, list):
            out.append(path)
            return
        keyed = all(isinstance(x, dict) and "id" in x for x in base)
        if keyed:
            index = {x["id"]: x for x in derived if isinstance(x, dict) and "id" in x}
            for x in base:
                if x["id"] not in index:
                    out.append(f"{path}/{x['id']}")
                else:
                    _diff(x, index[x["id"]], f"{path}/{x['id']}", out)
        else:
            missing = [x for x in base if x not in derived]
            if missing:
                out.append(path)
    elif base != derived:
        out.append(path)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--report", action="store_true", help="list declared refinements in use")
    a = ap.parse_args(argv)

    reg = _yaml(REGISTER)
    base = ROOT / reg["baseline"]
    errors = check_manifest(base)
    policies = reg.get("field_policies", {})
    declared = {r["path"]: r for r in reg.get("refinements", [])}
    for path, r in declared.items():
        for f in ("reason", "decision"):
            if not str(r.get(f, "")).strip():
                errors.append(f"refinement '{path}': missing {f}")

    deviations: list[str] = []

    base_reqs = _items(base / "spec/areas/*.yaml", "requirements")
    reqs = _items(ROOT / "spec/business/areas/*.yaml", "requirements")
    req_ok = set(policies.get("requirements", {}))
    for rid, b in base_reqs.items():
        d = reqs.get(rid)
        if d is None:
            deviations.append(f"requirements/{rid}")
            continue
        for k in set(b) | set(d):
            if k not in req_ok and b.get(k) != d.get(k):
                deviations.append(f"requirements/{rid}/{k}")

    risk_policy = policies.get("risks", {})
    prefix = risk_policy.get("id_prefix", "")
    base_risks = {r["id"]: r for r in _yaml(base / "spec/risks.yaml").get("risks", [])}
    risks = {r["id"]: r for r in _yaml(ROOT / "spec/business/risks.yaml").get("risks", [])}
    risk_ok = set(risk_policy.get("fields", {}))
    for rid, b in base_risks.items():
        d = risks.get(prefix + rid)
        if d is None:
            deviations.append(f"risks/{rid}")
            continue
        for k in set(b) | set(d):
            if k not in risk_ok and k != "id" and b.get(k) != d.get(k):
                deviations.append(f"risks/{rid}/{k}")

    bx = _yaml(base / "spec/experience/ux-workflows.yaml")
    move = policies.get("reference_paths", {})
    if move:
        src, dst = move["from"], move["to"]
        for v in bx.get("views", []) + bx.get("experiences", []):
            for ref in v.get("references", []) or []:
                if ref["path"].startswith(src):
                    rel = ref["path"][len(src):]
                    ref["path"] = dst + rel
                    moved, orig = ROOT / ref["path"], base / src / rel
                    if not moved.is_file() or moved.read_bytes() != orig.read_bytes():
                        errors.append(f"relocated reference differs from baseline: {ref['path']}")
    dx: dict = {"views": [], "experiences": []}
    for f in sorted(glob.glob(str(ROOT / "spec/business/experience/*.yaml"))):
        doc = _yaml(Path(f))
        dx["views"] += doc.get("views", []) or []
        dx["experiences"] += doc.get("experiences", []) or []
    _diff(bx.get("views", []), dx["views"], "views", deviations)
    _diff(bx.get("experiences", []), dx["experiences"], "experiences", deviations)
    base_sets = _yaml(base / "spec.yaml").get("state_sets", {})
    _diff(base_sets, _yaml(ROOT / "spec/system.yaml").get("state_sets", {}), "state_sets", deviations)

    used: set[str] = set()
    for dev in deviations:
        hit = _covered(dev, set(declared))
        if hit:
            used.add(hit)
        else:
            errors.append(f"undeclared deviation from baseline: {dev}")
    for path in sorted(set(declared) - used):
        errors.append(f"stale refinement (no deviation at or below path): {path}")

    declared_fields = set(policies.get("requirements", {}))
    permitted = {"target", "version", "status"}
    if declared_fields != permitted:
        errors.append(f"requirement refinements must be exactly {sorted(permitted)}; got {sorted(declared_fields)}")
    if a.report:
        for path in sorted(used):
            print(f"refined: {path} — {declared[path]['reason']}")
    for e in errors:
        print(e, file=sys.stderr)
    print(f"Business baseline check: {len(base_reqs)} requirements, {len(base_risks)} risks, "
          f"{len(used)} declared refinements, {len(errors)} errors")
    return int(bool(errors))


if __name__ == "__main__":
    sys.exit(main())
